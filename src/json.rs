//! JSON çıktısı: özeti başka programların (jq, CI adımları, betikler) okuyabileceği biçimde yazar.
//!
//! Alan adları, sabit değerler (`method`, `severity`) ve yer tutucular (`<NUM>`) İngilizcedir
//! ve arayüzün dilinden bağımsızdır: betikler dil değişince bozulmaz.
//!
//! JSON yazmak için bir kütüphane kullanılmadı: yalnızca yazıyoruz ve yapı küçük. JSON'un
//! kendisi bir Rust `enum`'ıyla modellenir (serde_json'daki `Value` türü de böyle bir enum'dır)
//! ve özyinelemeli olarak yazılır. Doğruluğu testlerde serde_json ile ayrıştırılarak denetlenir;
//! serde_json yalnızca geliştirme bağımlılığıdır, programın içine girmez.

use std::io::{self, Write};

use crate::ani_artis::{AniArtis, AniArtislar, EN_AZ_DILIM, EN_AZ_MAD, EN_DUSUK_SEVIYE, ESIK};
use crate::datetime::DateTime;
use crate::karsilastirma::{ANLAMLILIK, ARTIS_KATI, Degisim, EN_AZ_KAYIT, Karsilastirma, Pencere};
use crate::lang;
use crate::rapor::{FiltreOzeti, KalipSatiri, Ozet, SaatDilimiOzeti};

/// Bir JSON değeri. Nesnelerde alanlar eklendikleri sırayla yazılır.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    /// `null`
    Yok,
    /// `true` ya da `false`
    Mantiksal(bool),
    Sayi(usize),
    Ondalik(f64),
    Metin(String),
    Dizi(Vec<Json>),
    Nesne(Vec<(&'static str, Json)>),
}

impl Json {
    /// Okunaklı (girintili) JSON olarak yazar. `derinlik`: değerin iç içe kaçıncı düzeyde
    /// olduğu; her düzey iki boşluk girintidir.
    pub fn yaz(&self, cikti: &mut impl Write, derinlik: usize) -> io::Result<()> {
        match self {
            Json::Yok => write!(cikti, "null"),
            Json::Mantiksal(deger) => write!(cikti, "{deger}"),
            Json::Sayi(sayi) => write!(cikti, "{sayi}"),
            // Rust ondalık sayıları hiçbir zaman "1e-7" gibi üslü yazmaz ("0.0000001");
            // her iki yazım da geçerli JSON'dur. NaN ve sonsuz ise JSON'da yoktur.
            Json::Ondalik(sayi) if sayi.is_finite() => write!(cikti, "{sayi}"),
            Json::Ondalik(_) => write!(cikti, "null"),
            Json::Metin(metin) => metin_yaz(cikti, metin),
            Json::Dizi(ogeler) if ogeler.is_empty() => write!(cikti, "[]"),
            Json::Dizi(ogeler) => {
                writeln!(cikti, "[")?;
                for (sira, oge) in ogeler.iter().enumerate() {
                    girinti_yaz(cikti, derinlik + 1)?;
                    oge.yaz(cikti, derinlik + 1)?;
                    // Son ögeden sonra virgül olmaz; JSON fazladan virgülü kabul etmez
                    if sira + 1 < ogeler.len() {
                        write!(cikti, ",")?;
                    }
                    writeln!(cikti)?;
                }
                girinti_yaz(cikti, derinlik)?;
                write!(cikti, "]")
            }
            Json::Nesne(alanlar) if alanlar.is_empty() => write!(cikti, "{{}}"),
            Json::Nesne(alanlar) => {
                writeln!(cikti, "{{")?;
                for (sira, (anahtar, deger)) in alanlar.iter().enumerate() {
                    girinti_yaz(cikti, derinlik + 1)?;
                    metin_yaz(cikti, anahtar)?;
                    write!(cikti, ": ")?;
                    deger.yaz(cikti, derinlik + 1)?;
                    if sira + 1 < alanlar.len() {
                        write!(cikti, ",")?;
                    }
                    writeln!(cikti)?;
                }
                girinti_yaz(cikti, derinlik)?;
                write!(cikti, "}}")
            }
        }
    }
}

fn girinti_yaz(cikti: &mut impl Write, derinlik: usize) -> io::Result<()> {
    write!(cikti, "{:1$}", "", derinlik * 2)
}

/// JSON metni: çift tırnak içinde, RFC 8259'a göre kaçışlı.
///
/// Kaçış zorunlu olanlar: `"`, `\` ve U+0000–U+001F arasındaki kontrol karakterleri. Log
/// mesajlarında sekme ve terminal renk kodları (ESC, U+001B) gerçekten görülür.
///
/// DEL (U+007F) ve C1 kontrol karakterleri (U+0080–U+009F) de kaçılır. Zorunlu değildir, RFC
/// her karakterin kaçılmasına izin verir; böylece JSON doğrudan terminale yazıldığında da
/// (`veylog --bicim json dosya.log`) içinde terminalin komut sayacağı tek karakter kalmaz.
/// Türkçe harfler ve diğer bütün karakterler UTF-8 olarak olduğu gibi yazılır.
fn metin_yaz(cikti: &mut impl Write, metin: &str) -> io::Result<()> {
    write!(cikti, "\"")?;
    for harf in metin.chars() {
        match harf {
            '"' => write!(cikti, "\\\"")?,
            '\\' => write!(cikti, "\\\\")?,
            '\n' => write!(cikti, "\\n")?,
            '\r' => write!(cikti, "\\r")?,
            '\t' => write!(cikti, "\\t")?,
            '\u{08}' => write!(cikti, "\\b")?,
            '\u{0C}' => write!(cikti, "\\f")?,
            // `is_control`: U+0000–U+001F, U+007F ve U+0080–U+009F
            kontrol if kontrol.is_control() => {
                write!(cikti, "\\u{:04x}", u32::from(kontrol))?;
            }
            diger => write!(cikti, "{diger}")?,
        }
    }
    write!(cikti, "\"")
}

/// Özeti JSON olarak yazar.
///
/// `--ilk` ve `--siralama` burada da geçerlidir: "rows" dizisi tablodaki satırlardır, "omitted"
/// sınır yüzünden dışarıda kalanların sayısıdır. Bütün satırlar için `--hepsi`.
pub fn yaz(cikti: &mut impl Write, ozet: &Ozet) -> io::Result<()> {
    ozet_json(ozet).yaz(cikti, 0)?;
    writeln!(cikti)
}

/// Özetin JSON ağacı. Kalıplar ve örnekler kısaltılmadan tam halleriyle yazılır. Bütün
/// zamanlar raporun saat diliminde, `2026-10-24 16:10:00` biçimindedir; dilim `time_zone`
/// alanındadır.
fn ozet_json(ozet: &Ozet) -> Json {
    let zaman_araligi = match ozet.zaman_araligi {
        Some((en_eski, en_yeni)) => {
            Json::Nesne(vec![("oldest", zaman(en_eski)), ("newest", zaman(en_yeni))])
        }
        None => Json::Yok,
    };
    let seviyeler = ozet
        .seviyeler
        .iter()
        .map(|&(seviye, adet)| (seviye.ad(), Json::Sayi(adet)))
        .collect();
    let satirlar = ozet
        .gosterilecek_satirlar()
        .iter()
        .map(satir_json)
        .collect();
    let analiz = &ozet.analiz;

    let mut alanlar = vec![
        ("version", metin(env!("CARGO_PKG_VERSION"))),
        ("file", metin(&ozet.dosya)),
        ("lines", Json::Sayi(ozet.satir_sayisi)),
        ("records", Json::Sayi(ozet.kayit_sayisi)),
        ("continuation_lines", Json::Sayi(ozet.devam_satiri_sayisi)),
        ("orphan_lines", Json::Sayi(ozet.yetim_satir_sayisi)),
    ];
    // Filtre yalnızca verildiyse yazılır; normal raporun biçimi değişmez
    if let Some(filtre) = &ozet.filtre {
        alanlar.push(("filter", filtre_json(filtre)));
    }
    alanlar.extend([
        ("time_range", zaman_araligi),
        (
            "time_zone",
            ozet.saat_dilimi
                .as_ref()
                .map_or(Json::Yok, saat_dilimi_json),
        ),
        ("levels", Json::Nesne(seviyeler)),
        (
            "patterns",
            Json::Nesne(vec![
                ("distinct_messages", Json::Sayi(analiz.ham_mesaj_sayisi)),
                (
                    "distinct_messages_estimated",
                    Json::Mantiksal(analiz.ham_mesaj_yaklasik),
                ),
                ("masked_messages", Json::Sayi(analiz.maskeli_mesaj_sayisi)),
                ("drain_patterns", Json::Sayi(analiz.kalip_sayisi)),
                ("groups", Json::Sayi(analiz.satirlar.len())),
                (
                    "threshold",
                    Json::Ondalik(ozet.ayarlar.drain.benzerlik_esigi),
                ),
                ("location", metin(ozet.ayarlar.konum_turu.ingilizce_ad())),
                ("sort", metin(ozet.ayarlar.siralama.ingilizce_ad())),
                ("omitted", Json::Sayi(ozet.gosterilmeyen())),
                ("rows", Json::Dizi(satirlar)),
            ]),
        ),
    ]);
    // Karşılaştırma ve ani artışlar yalnızca istendiyse yazılır; normal raporun biçimi değişmez
    if let Some(karsilastirma) = &ozet.karsilastirma {
        alanlar.push(("comparison", karsilastirma_json(ozet, karsilastirma)));
    }
    if let Some(ani_artislar) = &ozet.ani_artislar {
        alanlar.push(("spikes", ani_artis_json(ozet, ani_artislar)));
    }
    Json::Nesne(alanlar)
}

/// Filtrenin JSON'u: verilmeyen sınırlar null'dır. `records` filtreye uyan kayıtlardır;
/// dosyanın bütün kayıtları en üstteki `records`tır.
fn filtre_json(filtre: &FiltreOzeti) -> Json {
    let zaman_ya_da_yok = |zaman: Option<DateTime>| zaman.map_or(Json::Yok, self::zaman);
    Json::Nesne(vec![
        (
            "min_level",
            filtre.seviye.map_or(Json::Yok, |seviye| metin(seviye.ad())),
        ),
        ("since", zaman_ya_da_yok(filtre.baslangic)),
        ("until", zaman_ya_da_yok(filtre.bitis)),
        (
            "last_seconds",
            filtre
                .son
                .map_or(Json::Yok, |saniye| Json::Sayi(saniye as usize)),
        ),
        ("records", Json::Sayi(filtre.kayit_sayisi)),
    ])
}

/// Saat diliminin JSON'u: raporun dilimi, nereden geldiği (`--saat-dilimi` ya da logda yazılı
/// ilk dilim) ve kayıtların ona nasıl alındığı.
fn saat_dilimi_json(dilim: &SaatDilimiOzeti) -> Json {
    let cevrilen_dilimler = dilim
        .sayilar
        .cevrilen_dilimler
        .iter()
        .map(|kaynak| Json::Metin(kaynak.to_string()))
        .collect();
    Json::Nesne(vec![
        ("zone", Json::Metin(dilim.dilim.to_string())),
        (
            "source",
            metin(if dilim.verildi { "option" } else { "log" }),
        ),
        ("converted_records", Json::Sayi(dilim.sayilar.cevrilen)),
        ("converted_from", Json::Dizi(cevrilen_dilimler)),
        ("records_without_zone", Json::Sayi(dilim.sayilar.yazilmamis)),
    ])
}

/// Karşılaştırmanın JSON'u: nokta, iki yarı, "artan" eşikleri ve üç liste. Listeler en fazla
/// `--ilk` kadar satır içerir; "omitted" dışarıda kalanları sayar.
fn karsilastirma_json(ozet: &Ozet, karsilastirma: &Karsilastirma) -> Json {
    let liste = |degisimler: &[Degisim]| {
        let satirlar = ozet
            .sinirla(degisimler)
            .iter()
            .map(|degisim| degisim_json(ozet, degisim))
            .collect();
        Json::Nesne(vec![
            ("count", Json::Sayi(degisimler.len())),
            ("omitted", Json::Sayi(ozet.disarida_kalan(degisimler))),
            ("rows", Json::Dizi(satirlar)),
        ])
    };
    Json::Nesne(vec![
        ("point", zaman(karsilastirma.nokta)),
        ("before", pencere_json(&karsilastirma.once)),
        ("after", pencere_json(&karsilastirma.sonra)),
        ("min_factor", Json::Ondalik(ARTIS_KATI)),
        ("significance", Json::Ondalik(ANLAMLILIK)),
        ("min_records", Json::Sayi(EN_AZ_KAYIT)),
        ("new", liste(&karsilastirma.yeni)),
        ("gone", liste(&karsilastirma.kaybolan)),
        ("increased", liste(&karsilastirma.artan)),
    ])
}

fn pencere_json(pencere: &Pencere) -> Json {
    Json::Nesne(vec![
        ("start", zaman(pencere.baslangic)),
        ("end", zaman(pencere.bitis)),
        ("seconds", Json::Sayi(pencere.saniye)),
        ("records", Json::Sayi(pencere.kayit_sayisi)),
    ])
}

/// Karşılaştırmadaki bir satır: kalıp satırının alanları ve iki yarıdaki sayıları.
fn degisim_json(ozet: &Ozet, degisim: &Degisim) -> Json {
    let mut alanlar = satir_alanlari(&ozet.analiz.satirlar[degisim.satir]);
    alanlar.extend([
        ("before", Json::Sayi(degisim.once)),
        ("after", Json::Sayi(degisim.sonra)),
        // Yenilerde kat sonsuzdur; JSON'da sonsuz olmadığı için null yazılır
        ("factor", Json::Ondalik(degisim.kat)),
        ("p_value", Json::Ondalik(degisim.p_degeri)),
    ]);
    Json::Nesne(alanlar)
}

/// Ani artışların JSON'u: dilimler, eşikler ve en fazla `--ilk` kadar artış. Logda kayıt yoksa
/// "start" ve "end" null'dır; log `min_slices`'tan kısaysa artış aranmaz.
fn ani_artis_json(ozet: &Ozet, ani_artislar: &AniArtislar) -> Json {
    let (baslangic, bitis) = match ani_artislar.aralik {
        Some((baslangic, bitis)) => (zaman(baslangic), zaman(bitis)),
        None => (Json::Yok, Json::Yok),
    };
    let artislar = &ani_artislar.artislar;
    let satirlar = ozet
        .sinirla(artislar)
        .iter()
        .map(|artis| artis_json(ozet, artis))
        .collect();
    Json::Nesne(vec![
        (
            "slice_minutes",
            Json::Sayi(usize::from(ani_artislar.dilim_dakika)),
        ),
        ("min_level", metin(EN_DUSUK_SEVIYE.ad())),
        ("start", baslangic),
        ("end", bitis),
        ("slices", Json::Sayi(ani_artislar.dilim_sayisi)),
        ("min_slices", Json::Sayi(EN_AZ_DILIM)),
        ("threshold", Json::Ondalik(ESIK)),
        ("min_mad", Json::Ondalik(EN_AZ_MAD)),
        ("count", Json::Sayi(artislar.len())),
        ("omitted", Json::Sayi(ozet.disarida_kalan(artislar))),
        ("rows", Json::Dizi(satirlar)),
    ])
}

/// Bir ani artış: kalıp satırının alanları, kalıbın olağan seviyesi ve artışın kendisi.
/// Satırın alanları (seviye, ilk, son, örnek) bütün kalıbındır; artışınkiler "spike"ta.
fn artis_json(ozet: &Ozet, artis: &AniArtis) -> Json {
    let mut alanlar = satir_alanlari(&ozet.analiz.satirlar[artis.satir]);
    alanlar.extend([
        ("usual", Json::Ondalik(artis.olagan)),
        ("mad", Json::Ondalik(artis.mad)),
        (
            "spike",
            Json::Nesne(vec![
                ("start", zaman(artis.baslangic)),
                ("end", zaman(artis.bitis)),
                ("slices", Json::Sayi(artis.dilim_sayisi)),
                ("records", Json::Sayi(artis.kayit_sayisi)),
                ("level", metin(artis.seviye.ad())),
                ("first", zaman(artis.ilk)),
                ("last", zaman(artis.son)),
                ("sample", metin(&artis.ornek)),
                (
                    "peak",
                    Json::Nesne(vec![
                        ("start", zaman(artis.zirve_baslangic)),
                        ("end", zaman(artis.zirve_bitis)),
                        ("count", Json::Sayi(artis.zirve_adet)),
                        ("score", Json::Ondalik(artis.puan)),
                    ]),
                ),
            ]),
        ),
    ]);
    Json::Nesne(alanlar)
}

/// Tablodaki bir satırın JSON'u.
fn satir_json(satir: &KalipSatiri) -> Json {
    Json::Nesne(satir_alanlari(satir))
}

/// Bir kalıp satırının alanları. İstisnası olmayan satırlarda "exception" null'dır; istisnanın
/// uygulama kodunda yeri bulunamadıysa "file", "line" ve "method" null'dır. View hatalarında
/// konum bir şablondur; şablonun satırı bilinmediği için "line" null'dır. "method" konumun
/// içinde bulunduğu fonksiyondur; konum satırla belirlendiyse null'dır. Kalıptaki yer
/// tutucular İngilizce adlarıyladır (`<NUM>`).
fn satir_alanlari(satir: &KalipSatiri) -> Vec<(&'static str, Json)> {
    let istisna = match &satir.parmak_izi {
        Some(parmak_izi) => {
            let (dosya, satir_no) = match parmak_izi.konum() {
                // u32 her zaman usize'a sığar (Rust'ın desteklediği bütün sistemler 32 ya da
                // 64 bit); `as` burada bilgi kaybettirmez
                Some((dosya, satir_no)) => (
                    metin(&dosya),
                    satir_no.map_or(Json::Yok, |satir_no| Json::Sayi(satir_no as usize)),
                ),
                None => (Json::Yok, Json::Yok),
            };
            Json::Nesne(vec![
                ("class", Json::Metin(parmak_izi.sinif())),
                ("file", dosya),
                ("line", satir_no),
                ("method", parmak_izi.metot().map_or(Json::Yok, metin)),
            ])
        }
        None => Json::Yok,
    };

    vec![
        ("count", Json::Sayi(satir.adet)),
        ("level", metin(satir.seviye.ad())),
        ("first", zaman(satir.ilk)),
        ("last", zaman(satir.son)),
        ("pattern", metin(&lang::english_placeholders(&satir.kalip))),
        ("sample", metin(&satir.ornek)),
        ("variables", degiskenler_json(satir)),
        ("exception", istisna),
    ]
}

/// Kalıptaki `<*>` konumlarının değerleri: her konum için sırası (1'den), en sık değerler
/// adetleriyle ve listelenmeyen farklı değer sayısı. Kalıpta `<*>` yoksa boş dizi.
fn degiskenler_json(satir: &KalipSatiri) -> Json {
    Json::Dizi(
        satir
            .degiskenler
            .iter()
            .map(|degisken| {
                let degerler = degisken
                    .degerler
                    .iter()
                    .map(|(deger, adet)| {
                        Json::Nesne(vec![("value", metin(deger)), ("count", Json::Sayi(*adet))])
                    })
                    .collect();
                Json::Nesne(vec![
                    ("position", Json::Sayi(degisken.konum)),
                    ("values", Json::Dizi(degerler)),
                    ("others", Json::Sayi(degisken.baska)),
                ])
            })
            .collect(),
    )
}

fn metin(metin: &str) -> Json {
    Json::Metin(metin.to_owned())
}

/// Bir an, raporun saat diliminde: "2026-10-24 16:10:00"
fn zaman(zaman: DateTime) -> Json {
    Json::Metin(zaman.to_string())
}

#[cfg(test)]
mod testler {
    use super::*;
    use crate::ani_artis::{CARPAN, deneme_logu};
    use crate::datetime::TimePoint;
    use crate::parmak_izi::KonumTuru;
    use crate::rapor::RaporAyarlari;

    const ORNEK_LOG: &str = include_str!("../ornekler/laravel.log");

    fn yazi(deger: &Json) -> String {
        let mut cikti = Vec::new();
        deger.yaz(&mut cikti, 0).unwrap();
        String::from_utf8(cikti).unwrap()
    }

    /// Örnek logun JSON raporu, serde_json ile ayrıştırılmış olarak.
    fn ornek_rapor(ayarlar: &RaporAyarlari) -> serde_json::Value {
        let ozet = Ozet::metinden("ornekler/laravel.log", ORNEK_LOG, ayarlar).unwrap();
        let mut cikti = Vec::new();
        yaz(&mut cikti, &ozet).unwrap();
        // Geçerli bir JSON değilse burada hata verir
        serde_json::from_slice(&cikti).expect("geçerli JSON değil")
    }

    #[test]
    fn ozel_karakterler_kacilir() {
        assert_eq!(yazi(&metin("a\"b\\c\nd\te")), r#""a\"b\\c\nd\te""#);
        // RFC'deki kısa kaçışlar; \u000d de geçerli olurdu ama okunması zordur
        assert_eq!(yazi(&metin("\r\u{8}\u{c}")), r#""\r\b\f""#);
        // Kontrol karakterleri \u ile, Türkçe harfler olduğu gibi
        assert_eq!(
            yazi(&metin("\u{1b}[31mKırmızı\u{0}")),
            r#""\u001b[31mKırmızı\u0000""#
        );
        // Sınırlar: U+001F son zorunlu kaçış; boşluk ve ~ kaçılmaz; DEL ve C1 kaçılır;
        // U+00A0 (bölünmez boşluk) kontrol değildir
        assert_eq!(
            yazi(&metin("\u{1f} ~\u{7f}\u{80}\u{9b}\u{9f}\u{a0}")),
            "\"\\u001f ~\\u007f\\u0080\\u009b\\u009f\u{a0}\""
        );
    }

    #[test]
    fn zor_metinler_bozulmadan_geri_okunur() {
        // Yazıp serde_json ile geri okuyunca aynı metin çıkmalı
        for zor in [
            "",
            "düz metin",
            "\"tırnak\" ve \\ters bölü\\",
            "sekme\tsatır\nbaşı\rgeri",
            "\u{0}\u{1}\u{8}\u{c}\u{1b}\u{1f}\u{7f}\u{80}\u{9b}\u{9f}",
            "Ünlü harfler: ğüşıöçĞÜŞİÖÇ",
            "emoji 🚀 ve U+2028 \u{2028} ayıracı",
            r"App\Jobs\FaturaGonder",
        ] {
            let okunan: String = serde_json::from_str(&yazi(&metin(zor))).unwrap();
            assert_eq!(okunan, zor);
        }
    }

    #[test]
    fn sayilar_ve_bos_degerler() {
        assert_eq!(yazi(&Json::Ondalik(0.7)), "0.7");
        assert_eq!(yazi(&Json::Ondalik(1.0)), "1");
        assert_eq!(yazi(&Json::Ondalik(f64::NAN)), "null");
        assert_eq!(yazi(&Json::Sayi(42)), "42");
        assert_eq!(yazi(&Json::Dizi(vec![])), "[]");
        assert_eq!(yazi(&Json::Nesne(vec![])), "{}");
    }

    #[test]
    fn ic_ice_yapi_girintili_yazilir() {
        let deger = Json::Nesne(vec![
            ("a", Json::Sayi(1)),
            ("b", Json::Dizi(vec![Json::Yok, metin("x")])),
            ("c", Json::Nesne(vec![])),
        ]);
        assert_eq!(
            yazi(&deger),
            "{\n  \"a\": 1,\n  \"b\": [\n    null,\n    \"x\"\n  ],\n  \"c\": {}\n}"
        );
    }

    #[test]
    fn ornek_log_raporu_gecerli_ve_dogru() {
        let rapor = ornek_rapor(&RaporAyarlari::default());

        assert_eq!(rapor["file"], "ornekler/laravel.log");
        assert_eq!(rapor["lines"], 205);
        assert_eq!(rapor["time_range"]["oldest"], "2026-10-24 08:59:58");
        assert_eq!(rapor["levels"]["CRITICAL"], 2);
        assert_eq!(rapor["levels"]["ERROR"], 16);

        let kaliplar = &rapor["patterns"];
        assert_eq!(kaliplar["drain_patterns"], 23);
        assert_eq!(kaliplar["groups"], 24);
        assert_eq!(kaliplar["threshold"], 0.7);
        assert_eq!(kaliplar["location"], "method");
        assert_eq!(kaliplar["sort"], "severity");
        // Varsayılan --ilk 10: on satır yazılır, kalanlar sayılır
        assert_eq!(kaliplar["rows"].as_array().unwrap().len(), 10);
        assert_eq!(kaliplar["omitted"], 14);

        // Zamanlar ve metinler kısaltılmadan, sınıf adı tam haliyle
        let redis = &kaliplar["rows"][0];
        assert_eq!(redis["level"], "CRITICAL");
        assert_eq!(redis["first"], "2026-10-24 17:05:13");
        assert_eq!(
            redis["exception"]["file"],
            "app/Http/Middleware/HizSiniri.php"
        );
        assert_eq!(redis["exception"]["line"], 22);
        // Konumun içinde bulunduğu fonksiyon, tam adıyla
        assert_eq!(
            redis["exception"]["method"],
            r"App\Http\Middleware\HizSiniri->handle"
        );

        let kayit = &kaliplar["rows"][3];
        assert_eq!(
            kayit["exception"]["class"],
            r"Illuminate\Database\UniqueConstraintViolationException"
        );
        // Terminal 80 karakterde keser; JSON'daki kalıp tam haliyle 251 karakter
        let kalip = kayit["pattern"].as_str().unwrap();
        assert_eq!(kalip.chars().count(), 251);
        assert!(kalip.ends_with("values <*> <EMAIL>, <TIME>, <TIME>))"));
    }

    #[test]
    fn joker_konumlarinin_degerleri_yazilir() {
        let rapor = ornek_rapor(&RaporAyarlari {
            ilk: None,
            ..RaporAyarlari::default()
        });
        let satirlar = rapor["patterns"]["rows"].as_array().unwrap();
        let girisler = satirlar
            .iter()
            .find(|satir| satir["pattern"] == "Kullanıcı <NUM> <*> yaptı")
            .unwrap();
        assert_eq!(
            girisler["variables"],
            serde_json::json!([{
                "position": 3,
                "values": [{"value": "giriş", "count": 8}, {"value": "çıkış", "count": 1}],
                "others": 0
            }])
        );
        // Jokersiz kalıpta boş dizi
        let sepet = satirlar
            .iter()
            .find(|satir| satir["pattern"] == "Sepet önbellekten okundu")
            .unwrap();
        assert_eq!(sepet["variables"], serde_json::json!([]));
        // Birden çok joker ve listelenmeyen değerler
        let log: String = (0..7)
            .map(|sira| {
                format!(
                    "[2026-10-24 10:00:{sira:02}] local.INFO: Durum: ORDER_{sira} için OLAY_{} işlendi\n",
                    sira % 2
                )
            })
            .collect();
        let ozet = Ozet::metinden("a.log", &log, &RaporAyarlari::default()).unwrap();
        let mut cikti = Vec::new();
        yaz(&mut cikti, &ozet).unwrap();
        let rapor: serde_json::Value = serde_json::from_slice(&cikti).unwrap();
        let satir = &rapor["patterns"]["rows"][0];
        assert_eq!(satir["pattern"], "Durum: <*> için <*> işlendi");
        let degiskenler = satir["variables"].as_array().unwrap();
        assert_eq!(degiskenler.len(), 2);
        assert_eq!(degiskenler[0]["position"], 2);
        assert_eq!(degiskenler[0]["values"].as_array().unwrap().len(), 5);
        assert_eq!(degiskenler[0]["others"], 2);
        assert_eq!(degiskenler[1]["position"], 4);
        assert_eq!(
            degiskenler[1]["values"],
            serde_json::json!([{"value": "OLAY_0", "count": 4}, {"value": "OLAY_1", "count": 3}])
        );
        assert_eq!(degiskenler[1]["others"], 0);
    }

    #[test]
    fn satirla_belirlenen_konumda_metot_yok() {
        let ayarlar = RaporAyarlari {
            konum_turu: KonumTuru::Satir,
            ..RaporAyarlari::default()
        };
        let rapor = ornek_rapor(&ayarlar);
        assert_eq!(rapor["patterns"]["location"], "line");
        let redis = &rapor["patterns"]["rows"][0];
        assert_eq!(redis["exception"]["line"], 22);
        assert!(redis["exception"]["method"].is_null());
    }

    #[test]
    fn saat_dilimi_yazilir() {
        // Laravel'in biçiminde saat dilimi yazılmaz: alan null
        assert!(ornek_rapor(&RaporAyarlari::default())["time_zone"].is_null());
        // Yaz saatinden kış saatine geçen bir log: zamanlar ilk dilimde (+03:00)
        let metin = "[2026-10-25T02:30:00+03:00] local.ERROR: Kuyruk 1 dolu\n\
                     [2026-10-25T02:10:00+02:00] local.ERROR: Kuyruk 2 dolu";
        let ozet = Ozet::metinden("a.log", metin, &RaporAyarlari::default()).unwrap();
        let mut cikti = Vec::new();
        yaz(&mut cikti, &ozet).unwrap();
        let rapor: serde_json::Value = serde_json::from_slice(&cikti).unwrap();
        assert_eq!(
            rapor["time_zone"],
            serde_json::json!({
                "zone": "+03:00",
                "source": "log",
                "converted_records": 1,
                "converted_from": ["+02:00"],
                "records_without_zone": 0
            })
        );
        assert_eq!(
            rapor["time_range"],
            serde_json::json!({"oldest": "2026-10-25 02:30:00", "newest": "2026-10-25 03:10:00"})
        );
        let satir = &rapor["patterns"]["rows"][0];
        assert_eq!(
            (&satir["first"], &satir["last"]),
            (&"2026-10-25 02:30:00".into(), &"2026-10-25 03:10:00".into())
        );
    }

    #[test]
    fn filtre_yalnizca_verildiyse_yazilir() {
        use crate::filtre::Filtre;
        use crate::kayit::Seviye;
        assert!(
            ornek_rapor(&RaporAyarlari::default())
                .get("filter")
                .is_none()
        );
        let ayarlar = RaporAyarlari {
            filtre: Filtre {
                seviye: Some(Seviye::Warning),
                baslangic: TimePoint::parse("16:00"),
                ..Filtre::default()
            },
            ..RaporAyarlari::default()
        };
        let rapor = ornek_rapor(&ayarlar);
        assert_eq!(
            rapor["filter"],
            serde_json::json!({
                "min_level": "WARNING",
                "since": "2026-10-24 16:00:00",
                "until": null,
                "last_seconds": null,
                "records": 13
            })
        );
        // Dosyanın kayıt sayısı değişmez; seviyeler filtreye uyanlardır
        assert_eq!(rapor["records"], 59);
        assert!(rapor["levels"].get("INFO").is_none());
        // Alan yetim satırlardan sonra, zaman aralığından önce yazılır
        let ozet = Ozet::metinden("ornekler/laravel.log", ORNEK_LOG, &ayarlar).unwrap();
        let mut cikti = Vec::new();
        yaz(&mut cikti, &ozet).unwrap();
        let metin = String::from_utf8(cikti).unwrap();
        let yer = |alan: &str| metin.find(&format!("\n  \"{alan}\": ")).unwrap();
        assert!(yer("orphan_lines") < yer("filter"));
        assert!(yer("filter") < yer("time_range"));
    }

    #[test]
    fn karsilastirma_istenirse_yazilir() {
        // İstenmediyse alan hiç yok: normal raporun biçimi değişmez
        let normal = ornek_rapor(&RaporAyarlari::default());
        assert!(normal.get("comparison").is_none());

        let rapor = ornek_rapor(&RaporAyarlari {
            karsilastir: TimePoint::parse("16:11"),
            ..RaporAyarlari::default()
        });
        let karsilastirma = &rapor["comparison"];
        assert_eq!(karsilastirma["point"], "2026-10-24 16:11:00");
        // `json!`: serde_json'un JSON yazımına benzeyen makrosu; karşılaştırmak için bir değer üretir
        assert_eq!(
            karsilastirma["before"],
            serde_json::json!({
                "start": "2026-10-24 08:59:58",
                "end": "2026-10-24 16:11:00",
                "seconds": 25_862,
                "records": 39
            })
        );
        assert_eq!(
            karsilastirma["after"],
            serde_json::json!({
                "start": "2026-10-24 16:11:00",
                "end": "2026-10-24 18:31:07",
                "seconds": 8_408,
                "records": 20
            })
        );
        assert_eq!(karsilastirma["min_factor"], 2.0);
        assert_eq!(karsilastirma["significance"], 0.01);
        assert_eq!(karsilastirma["min_records"], 2);
        assert_eq!(karsilastirma["new"]["count"], 3);
        // Varsayılan --ilk 10: on bir kaybolanın onu yazılır, biri sayılır
        let kaybolan = &karsilastirma["gone"];
        assert_eq!(kaybolan["count"], 11);
        assert_eq!(kaybolan["rows"].as_array().unwrap().len(), 10);
        assert_eq!(kaybolan["omitted"], 1);
        assert_eq!(kaybolan["rows"][0]["factor"], 0.0);
        // Yenide kat sonsuz; JSON'da sonsuz olmadığı için null
        assert!(karsilastirma["new"]["rows"][0]["factor"].is_null());

        let artan = &karsilastirma["increased"]["rows"][0];
        assert_eq!(
            (&artan["count"], &artan["before"], &artan["after"]),
            (&6.into(), &1.into(), &5.into())
        );
        // scipy.stats.binom.sf(4, 6, 8408 / 34270)
        let p_degeri = artan["p_value"].as_f64().unwrap();
        assert!((p_degeri - 0.00424335232204597).abs() < 1e-15);
    }

    #[test]
    fn ani_artislar_istenirse_yazilir() {
        let normal = ornek_rapor(&RaporAyarlari::default());
        assert!(normal.get("spikes").is_none());

        let rapor = ornek_rapor(&RaporAyarlari {
            ani_artis: Some(10),
            ..RaporAyarlari::default()
        });
        let ani_artislar = &rapor["spikes"];
        // Satırlar dışındaki alanlar: dilimler ve eşikler
        let mut ust_bilgi = ani_artislar.clone();
        ust_bilgi.as_object_mut().unwrap().remove("rows");
        assert_eq!(
            ust_bilgi,
            serde_json::json!({
                "slice_minutes": 10,
                "min_level": "WARNING",
                "start": "2026-10-24 08:50:00",
                "end": "2026-10-24 18:40:00",
                "slices": 59,
                "min_slices": 10,
                "threshold": 3.5,
                // Rust 1.0'ı "1" diye yazar; serde_json onu tamsayı olarak okur
                "min_mad": 1,
                "count": 1,
                "omitted": 0
            })
        );

        // Satır: kalıp satırının alanları, olağan seviye ve artışın kendisi
        let satir = &ani_artislar["rows"][0];
        assert_eq!(satir["count"], 6);
        assert!(satir["pattern"].as_str().unwrap().starts_with("cURL error"));
        assert_eq!(satir["exception"]["file"], "app/Services/OdemeServisi.php");
        assert_eq!(satir["usual"].as_f64(), Some(0.0));
        assert_eq!(satir["mad"].as_f64(), Some(0.0));
        assert_eq!(
            satir["spike"],
            serde_json::json!({
                "start": "2026-10-24 16:10:00",
                "end": "2026-10-24 16:20:00",
                "slices": 1,
                "records": 6,
                "level": "ERROR",
                "first": "2026-10-24 16:10:02",
                "last": "2026-10-24 16:16:02",
                "sample": "cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes received (see https://curl.haxx.se/libcurl/c/libcurl-errors.html) for https://api.odeme.test/v1/tahsilat",
                "peak": {
                    "start": "2026-10-24 16:10:00",
                    "end": "2026-10-24 16:20:00",
                    "count": 6,
                    "score": CARPAN * 6.0
                }
            })
        );
    }

    #[test]
    fn ani_artislar_ilk_kadar_ve_alanlari_tam() {
        let metin = deneme_logu();
        let ayarlar = RaporAyarlari {
            ani_artis: Some(1),
            ilk: Some(2),
            ..RaporAyarlari::default()
        };
        let ozet = Ozet::metinden("a.log", &metin, &ayarlar).unwrap();
        let mut cikti = Vec::new();
        yaz(&mut cikti, &ozet).unwrap();
        let rapor: serde_json::Value = serde_json::from_slice(&cikti).unwrap();
        let ani_artislar = &rapor["spikes"];

        // Üç artışın ikisi yazılır, biri sayılır
        assert_eq!(ani_artislar["count"], 3);
        assert_eq!(ani_artislar["omitted"], 1);
        let satirlar = ani_artislar["rows"].as_array().unwrap();
        assert_eq!(satirlar.len(), 2);

        // Üç dilimlik artış: zirve ortadaki dilim
        assert_eq!(satirlar[0]["pattern"], "Kuyruk dolu");
        assert_eq!(
            satirlar[0]["spike"],
            serde_json::json!({
                "start": "2026-10-24 10:30:00",
                "end": "2026-10-24 10:33:00",
                "slices": 3,
                "records": 22,
                "level": "ERROR",
                "first": "2026-10-24 10:30:30",
                "last": "2026-10-24 10:32:36",
                "sample": "Kuyruk dolu",
                "peak": {
                    "start": "2026-10-24 10:31:00",
                    "end": "2026-10-24 10:32:00",
                    "count": 9,
                    "score": CARPAN * 9.0
                }
            })
        );
        // Olağan seviyesi 2 olan kalıp
        assert_eq!(satirlar[1]["pattern"], "Yavaş istek");
        assert_eq!(satirlar[1]["usual"].as_f64(), Some(2.0));
        assert_eq!(satirlar[1]["mad"].as_f64(), Some(0.0));
    }

    #[test]
    fn bos_logda_ani_artis_araligi_null() {
        let ayarlar = RaporAyarlari {
            ani_artis: Some(1),
            ..RaporAyarlari::default()
        };
        let ozet = Ozet::metinden("bos.log", "", &ayarlar).unwrap();
        let mut cikti = Vec::new();
        yaz(&mut cikti, &ozet).unwrap();
        let rapor: serde_json::Value = serde_json::from_slice(&cikti).unwrap();
        let ani_artislar = &rapor["spikes"];
        assert!(ani_artislar["start"].is_null());
        assert!(ani_artislar["end"].is_null());
        assert_eq!(ani_artislar["slices"], 0);
        assert_eq!(ani_artislar["rows"], serde_json::json!([]));
    }

    #[test]
    fn hepsi_bilgisiz_satirlar_ve_bos_alanlar() {
        let rapor = ornek_rapor(&RaporAyarlari {
            ilk: None,
            ..RaporAyarlari::default()
        });
        let satirlar = rapor["patterns"]["rows"].as_array().unwrap();
        assert_eq!(satirlar.len(), 24);
        assert_eq!(rapor["patterns"]["omitted"], 0);

        // Kuyruk hatası tamamen vendor içinde: istisna var, konumu yok
        let kuyruk = satirlar
            .iter()
            .find(|satir| satir["pattern"].as_str().unwrap().contains("FaturaGonder"))
            .unwrap();
        assert_eq!(
            kuyruk["exception"]["class"],
            r"Illuminate\Queue\MaxAttemptsExceededException"
        );
        assert!(kuyruk["exception"]["file"].is_null());
        assert!(kuyruk["exception"]["line"].is_null());

        // İstisnasız bir kayıt
        let giris = satirlar
            .iter()
            .find(|satir| satir["pattern"].as_str().unwrap().starts_with("Kullanıcı"))
            .unwrap();
        assert!(giris["exception"].is_null());
    }
}
