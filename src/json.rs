//! JSON çıktısı: özeti başka programların (jq, CI adımları, betikler) okuyabileceği biçimde yazar.
//!
//! JSON yazmak için bir kütüphane kullanılmadı: yalnızca yazıyoruz ve yapı küçük. JSON'un
//! kendisi bir Rust `enum`'ıyla modellenir (serde_json'daki `Value` türü de böyle bir enum'dır)
//! ve özyinelemeli olarak yazılır. Doğruluğu testlerde serde_json ile ayrıştırılarak denetlenir;
//! serde_json yalnızca geliştirme bağımlılığıdır, programın içine girmez.

use std::io::{self, Write};

use crate::ani_artis::{AniArtis, AniArtislar, EN_AZ_DILIM, EN_AZ_MAD, EN_DUSUK_SEVIYE, ESIK};
use crate::karsilastirma::{ANLAMLILIK, ARTIS_KATI, Degisim, EN_AZ_KAYIT, Karsilastirma, Pencere};
use crate::rapor::{KalipSatiri, Ozet};
use crate::zaman::Zaman;

/// Bir JSON değeri. Nesnelerde alanlar eklendikleri sırayla yazılır.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    /// `null`
    Yok,
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
/// `--ilk` ve `--siralama` burada da geçerlidir: "satirlar" dizisi tablodaki satırlardır,
/// "gosterilmeyen" sınır yüzünden dışarıda kalanların sayısıdır. Bütün satırlar için `--hepsi`.
pub fn yaz(cikti: &mut impl Write, ozet: &Ozet<'_>) -> io::Result<()> {
    ozet_json(ozet).yaz(cikti, 0)?;
    writeln!(cikti)
}

/// Özetin JSON ağacı. Zamanlar, kalıplar ve örnekler kısaltılmadan tam halleriyle yazılır.
fn ozet_json(ozet: &Ozet<'_>) -> Json {
    let zaman_araligi = match ozet.zaman_araligi {
        Some((en_eski, en_yeni)) => Json::Nesne(vec![
            ("en_eski", metin(en_eski)),
            ("en_yeni", metin(en_yeni)),
        ]),
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
        ("surum", metin(env!("CARGO_PKG_VERSION"))),
        ("dosya", metin(&ozet.dosya)),
        ("satir_sayisi", Json::Sayi(ozet.satir_sayisi)),
        ("kayit_sayisi", Json::Sayi(ozet.kayit_sayisi)),
        ("devam_satiri_sayisi", Json::Sayi(ozet.devam_satiri_sayisi)),
        ("yetim_satir_sayisi", Json::Sayi(ozet.yetim_satir_sayisi)),
        ("zaman_araligi", zaman_araligi),
        ("seviyeler", Json::Nesne(seviyeler)),
        (
            "kaliplar",
            Json::Nesne(vec![
                ("ham_mesaj_sayisi", Json::Sayi(analiz.ham_mesaj_sayisi)),
                (
                    "maskeli_mesaj_sayisi",
                    Json::Sayi(analiz.maskeli_mesaj_sayisi),
                ),
                ("kalip_sayisi", Json::Sayi(analiz.kalip_sayisi)),
                ("grup_sayisi", Json::Sayi(analiz.satirlar.len())),
                ("esik", Json::Ondalik(ozet.ayarlar.drain.benzerlik_esigi)),
                ("siralama", metin(ozet.ayarlar.siralama.ad())),
                ("gosterilmeyen", Json::Sayi(ozet.gosterilmeyen())),
                ("satirlar", Json::Dizi(satirlar)),
            ]),
        ),
    ];
    // Karşılaştırma ve ani artışlar yalnızca istendiyse yazılır; normal raporun biçimi değişmez
    if let Some(karsilastirma) = &ozet.karsilastirma {
        alanlar.push(("karsilastirma", karsilastirma_json(ozet, karsilastirma)));
    }
    if let Some(ani_artislar) = &ozet.ani_artislar {
        alanlar.push(("ani_artislar", ani_artis_json(ozet, ani_artislar)));
    }
    Json::Nesne(alanlar)
}

/// Karşılaştırmanın JSON'u: nokta, iki yarı, "artan" eşikleri ve üç liste. Listeler en fazla
/// `--ilk` kadar satır içerir; "gosterilmeyen" dışarıda kalanları sayar.
fn karsilastirma_json(ozet: &Ozet<'_>, karsilastirma: &Karsilastirma) -> Json {
    let liste = |degisimler: &[Degisim]| {
        let satirlar = ozet
            .sinirla(degisimler)
            .iter()
            .map(|degisim| degisim_json(ozet, degisim))
            .collect();
        Json::Nesne(vec![
            ("kalip_sayisi", Json::Sayi(degisimler.len())),
            ("gosterilmeyen", Json::Sayi(ozet.disarida_kalan(degisimler))),
            ("satirlar", Json::Dizi(satirlar)),
        ])
    };
    Json::Nesne(vec![
        ("nokta", zaman(karsilastirma.nokta)),
        ("once", pencere_json(&karsilastirma.once)),
        ("sonra", pencere_json(&karsilastirma.sonra)),
        ("artis_kati", Json::Ondalik(ARTIS_KATI)),
        ("anlamlilik", Json::Ondalik(ANLAMLILIK)),
        ("en_az_kayit", Json::Sayi(EN_AZ_KAYIT)),
        ("yeni", liste(&karsilastirma.yeni)),
        ("kaybolan", liste(&karsilastirma.kaybolan)),
        ("artan", liste(&karsilastirma.artan)),
    ])
}

fn pencere_json(pencere: &Pencere) -> Json {
    Json::Nesne(vec![
        ("baslangic", zaman(pencere.baslangic)),
        ("bitis", zaman(pencere.bitis)),
        ("saniye", Json::Sayi(pencere.saniye)),
        ("kayit_sayisi", Json::Sayi(pencere.kayit_sayisi)),
    ])
}

/// Karşılaştırmadaki bir satır: kalıp satırının alanları ve iki yarıdaki sayıları.
fn degisim_json(ozet: &Ozet<'_>, degisim: &Degisim) -> Json {
    let mut alanlar = satir_alanlari(&ozet.analiz.satirlar[degisim.satir]);
    alanlar.extend([
        ("once", Json::Sayi(degisim.once)),
        ("sonra", Json::Sayi(degisim.sonra)),
        // Yenilerde kat sonsuzdur; JSON'da sonsuz olmadığı için null yazılır
        ("kat", Json::Ondalik(degisim.kat)),
        ("p_degeri", Json::Ondalik(degisim.p_degeri)),
    ]);
    Json::Nesne(alanlar)
}

/// Ani artışların JSON'u: dilimler, eşikler ve en fazla `--ilk` kadar artış. Logda kayıt yoksa
/// "baslangic" ve "bitis" null'dır; log `en_az_dilim`'den kısaysa artış aranmaz.
fn ani_artis_json(ozet: &Ozet<'_>, ani_artislar: &AniArtislar) -> Json {
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
            "dilim_dakika",
            Json::Sayi(usize::from(ani_artislar.dilim_dakika)),
        ),
        ("en_dusuk_seviye", metin(EN_DUSUK_SEVIYE.ad())),
        ("baslangic", baslangic),
        ("bitis", bitis),
        ("dilim_sayisi", Json::Sayi(ani_artislar.dilim_sayisi)),
        ("en_az_dilim", Json::Sayi(EN_AZ_DILIM)),
        ("esik", Json::Ondalik(ESIK)),
        ("en_az_mad", Json::Ondalik(EN_AZ_MAD)),
        ("artis_sayisi", Json::Sayi(artislar.len())),
        ("gosterilmeyen", Json::Sayi(ozet.disarida_kalan(artislar))),
        ("satirlar", Json::Dizi(satirlar)),
    ])
}

/// Bir ani artış: kalıp satırının alanları, kalıbın olağan seviyesi ve artışın kendisi.
/// Satırın alanları (seviye, ilk, son, örnek) bütün kalıbındır; artışınkiler "artis"te.
fn artis_json(ozet: &Ozet<'_>, artis: &AniArtis) -> Json {
    let mut alanlar = satir_alanlari(&ozet.analiz.satirlar[artis.satir]);
    alanlar.extend([
        ("olagan", Json::Ondalik(artis.olagan)),
        ("mad", Json::Ondalik(artis.mad)),
        (
            "artis",
            Json::Nesne(vec![
                ("baslangic", zaman(artis.baslangic)),
                ("bitis", zaman(artis.bitis)),
                ("dilim_sayisi", Json::Sayi(artis.dilim_sayisi)),
                ("kayit_sayisi", Json::Sayi(artis.kayit_sayisi)),
                ("seviye", metin(artis.seviye.ad())),
                ("ilk", zaman(artis.ilk)),
                ("son", zaman(artis.son)),
                ("ornek", metin(&artis.ornek)),
                (
                    "zirve",
                    Json::Nesne(vec![
                        ("baslangic", zaman(artis.zirve_baslangic)),
                        ("bitis", zaman(artis.zirve_bitis)),
                        ("adet", Json::Sayi(artis.zirve_adet)),
                        ("puan", Json::Ondalik(artis.puan)),
                    ]),
                ),
            ]),
        ),
    ]);
    Json::Nesne(alanlar)
}

/// Tablodaki bir satırın JSON'u.
fn satir_json(satir: &KalipSatiri<'_>) -> Json {
    Json::Nesne(satir_alanlari(satir))
}

/// Bir kalıp satırının alanları. İstisnası olmayan satırlarda "istisna" null'dır; istisnanın
/// uygulama kodunda yeri bulunamadıysa "dosya" ve "satir" null'dır. View hatalarında konum bir
/// şablondur; şablonun satırı bilinmediği için "satir" null'dır.
fn satir_alanlari(satir: &KalipSatiri<'_>) -> Vec<(&'static str, Json)> {
    let istisna = match satir.parmak_izi {
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
                ("sinif", Json::Metin(parmak_izi.sinif())),
                ("dosya", dosya),
                ("satir", satir_no),
            ])
        }
        None => Json::Yok,
    };

    vec![
        ("adet", Json::Sayi(satir.adet)),
        ("seviye", metin(satir.seviye.ad())),
        ("ilk", metin(satir.ilk)),
        ("son", metin(satir.son)),
        ("kalip", metin(&satir.kalip)),
        ("ornek", metin(satir.ornek)),
        ("istisna", istisna),
    ]
}

fn metin(metin: &str) -> Json {
    Json::Metin(metin.to_owned())
}

/// Bir an, logdaki biçimden bağımsız olarak: "2026-10-24 16:10:00"
fn zaman(zaman: Zaman) -> Json {
    Json::Metin(zaman.to_string())
}

#[cfg(test)]
mod testler {
    use super::*;
    use crate::ani_artis::{CARPAN, deneme_logu};
    use crate::kayit::kayitlari_ayir;
    use crate::rapor::RaporAyarlari;
    use crate::zaman::ZamanNoktasi;
    use std::path::Path;

    const ORNEK_LOG: &str = include_str!("../ornekler/laravel.log");

    fn yazi(deger: &Json) -> String {
        let mut cikti = Vec::new();
        deger.yaz(&mut cikti, 0).unwrap();
        String::from_utf8(cikti).unwrap()
    }

    /// Örnek logun JSON raporu, serde_json ile ayrıştırılmış olarak.
    fn ornek_rapor(ayarlar: &RaporAyarlari) -> serde_json::Value {
        let log = kayitlari_ayir(ORNEK_LOG);
        let ozet =
            Ozet::hesapla(Path::new("ornekler/laravel.log"), ORNEK_LOG, &log, ayarlar).unwrap();
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

        assert_eq!(rapor["dosya"], "ornekler/laravel.log");
        assert_eq!(rapor["satir_sayisi"], 203);
        assert_eq!(rapor["zaman_araligi"]["en_eski"], "2026-10-24 08:59:58");
        assert_eq!(rapor["seviyeler"]["CRITICAL"], 2);
        assert_eq!(rapor["seviyeler"]["ERROR"], 16);

        let kaliplar = &rapor["kaliplar"];
        assert_eq!(kaliplar["kalip_sayisi"], 23);
        assert_eq!(kaliplar["grup_sayisi"], 24);
        assert_eq!(kaliplar["esik"], 0.7);
        assert_eq!(kaliplar["siralama"], "onem");
        // Varsayılan --ilk 10: on satır yazılır, kalanlar sayılır
        assert_eq!(kaliplar["satirlar"].as_array().unwrap().len(), 10);
        assert_eq!(kaliplar["gosterilmeyen"], 14);

        // Zamanlar ve metinler kısaltılmadan, sınıf adı tam haliyle
        let redis = &kaliplar["satirlar"][0];
        assert_eq!(redis["seviye"], "CRITICAL");
        assert_eq!(redis["ilk"], "2026-10-24 17:05:13");
        assert_eq!(
            redis["istisna"]["dosya"],
            "app/Http/Middleware/HizSiniri.php"
        );
        assert_eq!(redis["istisna"]["satir"], 22);

        let kayit = &kaliplar["satirlar"][3];
        assert_eq!(
            kayit["istisna"]["sinif"],
            r"Illuminate\Database\UniqueConstraintViolationException"
        );
        // Terminal 80 karakterde keser; JSON'daki kalıp tam haliyle 257 karakter
        let kalip = kayit["kalip"].as_str().unwrap();
        assert_eq!(kalip.chars().count(), 257);
        assert!(kalip.ends_with("values <*> <EPOSTA>, <ZAMAN>, <ZAMAN>))"));
    }

    #[test]
    fn karsilastirma_istenirse_yazilir() {
        // İstenmediyse alan hiç yok: normal raporun biçimi değişmez
        let normal = ornek_rapor(&RaporAyarlari::default());
        assert!(normal.get("karsilastirma").is_none());

        let rapor = ornek_rapor(&RaporAyarlari {
            karsilastir: ZamanNoktasi::ayristir("16:11"),
            ..RaporAyarlari::default()
        });
        let karsilastirma = &rapor["karsilastirma"];
        assert_eq!(karsilastirma["nokta"], "2026-10-24 16:11:00");
        // `json!`: serde_json'un JSON yazımına benzeyen makrosu; karşılaştırmak için bir değer üretir
        assert_eq!(
            karsilastirma["once"],
            serde_json::json!({
                "baslangic": "2026-10-24 08:59:58",
                "bitis": "2026-10-24 16:11:00",
                "saniye": 25_862,
                "kayit_sayisi": 39
            })
        );
        assert_eq!(
            karsilastirma["sonra"],
            serde_json::json!({
                "baslangic": "2026-10-24 16:11:00",
                "bitis": "2026-10-24 18:31:07",
                "saniye": 8_408,
                "kayit_sayisi": 20
            })
        );
        assert_eq!(karsilastirma["artis_kati"], 2.0);
        assert_eq!(karsilastirma["anlamlilik"], 0.01);
        assert_eq!(karsilastirma["en_az_kayit"], 2);
        assert_eq!(karsilastirma["yeni"]["kalip_sayisi"], 3);
        // Varsayılan --ilk 10: on bir kaybolanın onu yazılır, biri sayılır
        let kaybolan = &karsilastirma["kaybolan"];
        assert_eq!(kaybolan["kalip_sayisi"], 11);
        assert_eq!(kaybolan["satirlar"].as_array().unwrap().len(), 10);
        assert_eq!(kaybolan["gosterilmeyen"], 1);
        assert_eq!(kaybolan["satirlar"][0]["kat"], 0.0);
        // Yenide kat sonsuz; JSON'da sonsuz olmadığı için null
        assert!(karsilastirma["yeni"]["satirlar"][0]["kat"].is_null());

        let artan = &karsilastirma["artan"]["satirlar"][0];
        assert_eq!(
            (&artan["adet"], &artan["once"], &artan["sonra"]),
            (&6.into(), &1.into(), &5.into())
        );
        // scipy.stats.binom.sf(4, 6, 8408 / 34270)
        let p_degeri = artan["p_degeri"].as_f64().unwrap();
        assert!((p_degeri - 0.00424335232204597).abs() < 1e-15);
    }

    #[test]
    fn ani_artislar_istenirse_yazilir() {
        let normal = ornek_rapor(&RaporAyarlari::default());
        assert!(normal.get("ani_artislar").is_none());

        let rapor = ornek_rapor(&RaporAyarlari {
            ani_artis: Some(10),
            ..RaporAyarlari::default()
        });
        let ani_artislar = &rapor["ani_artislar"];
        // Satırlar dışındaki alanlar: dilimler ve eşikler
        let mut ust_bilgi = ani_artislar.clone();
        ust_bilgi.as_object_mut().unwrap().remove("satirlar");
        assert_eq!(
            ust_bilgi,
            serde_json::json!({
                "dilim_dakika": 10,
                "en_dusuk_seviye": "WARNING",
                "baslangic": "2026-10-24 08:50:00",
                "bitis": "2026-10-24 18:40:00",
                "dilim_sayisi": 59,
                "en_az_dilim": 10,
                "esik": 3.5,
                // Rust 1.0'ı "1" diye yazar; serde_json onu tamsayı olarak okur
                "en_az_mad": 1,
                "artis_sayisi": 1,
                "gosterilmeyen": 0
            })
        );

        // Satır: kalıp satırının alanları, olağan seviye ve artışın kendisi
        let satir = &ani_artislar["satirlar"][0];
        assert_eq!(satir["adet"], 6);
        assert!(satir["kalip"].as_str().unwrap().starts_with("cURL error"));
        assert_eq!(satir["istisna"]["dosya"], "app/Services/OdemeServisi.php");
        assert_eq!(satir["olagan"].as_f64(), Some(0.0));
        assert_eq!(satir["mad"].as_f64(), Some(0.0));
        assert_eq!(
            satir["artis"],
            serde_json::json!({
                "baslangic": "2026-10-24 16:10:00",
                "bitis": "2026-10-24 16:20:00",
                "dilim_sayisi": 1,
                "kayit_sayisi": 6,
                "seviye": "ERROR",
                "ilk": "2026-10-24 16:10:02",
                "son": "2026-10-24 16:16:02",
                "ornek": "cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes received (see https://curl.haxx.se/libcurl/c/libcurl-errors.html) for https://api.odeme.test/v1/tahsilat",
                "zirve": {
                    "baslangic": "2026-10-24 16:10:00",
                    "bitis": "2026-10-24 16:20:00",
                    "adet": 6,
                    "puan": CARPAN * 6.0
                }
            })
        );
    }

    #[test]
    fn ani_artislar_ilk_kadar_ve_alanlari_tam() {
        let metin = deneme_logu();
        let log = kayitlari_ayir(&metin);
        let ayarlar = RaporAyarlari {
            ani_artis: Some(1),
            ilk: Some(2),
            ..RaporAyarlari::default()
        };
        let ozet = Ozet::hesapla(Path::new("a.log"), &metin, &log, &ayarlar).unwrap();
        let mut cikti = Vec::new();
        yaz(&mut cikti, &ozet).unwrap();
        let rapor: serde_json::Value = serde_json::from_slice(&cikti).unwrap();
        let ani_artislar = &rapor["ani_artislar"];

        // Üç artışın ikisi yazılır, biri sayılır
        assert_eq!(ani_artislar["artis_sayisi"], 3);
        assert_eq!(ani_artislar["gosterilmeyen"], 1);
        let satirlar = ani_artislar["satirlar"].as_array().unwrap();
        assert_eq!(satirlar.len(), 2);

        // Üç dilimlik artış: zirve ortadaki dilim
        assert_eq!(satirlar[0]["kalip"], "Kuyruk dolu");
        assert_eq!(
            satirlar[0]["artis"],
            serde_json::json!({
                "baslangic": "2026-10-24 10:30:00",
                "bitis": "2026-10-24 10:33:00",
                "dilim_sayisi": 3,
                "kayit_sayisi": 22,
                "seviye": "ERROR",
                "ilk": "2026-10-24 10:30:30",
                "son": "2026-10-24 10:32:36",
                "ornek": "Kuyruk dolu",
                "zirve": {
                    "baslangic": "2026-10-24 10:31:00",
                    "bitis": "2026-10-24 10:32:00",
                    "adet": 9,
                    "puan": CARPAN * 9.0
                }
            })
        );
        // Olağan seviyesi 2 olan kalıp
        assert_eq!(satirlar[1]["kalip"], "Yavaş istek");
        assert_eq!(satirlar[1]["olagan"].as_f64(), Some(2.0));
        assert_eq!(satirlar[1]["mad"].as_f64(), Some(0.0));
    }

    #[test]
    fn bos_logda_ani_artis_araligi_null() {
        let log = kayitlari_ayir("");
        let ayarlar = RaporAyarlari {
            ani_artis: Some(1),
            ..RaporAyarlari::default()
        };
        let ozet = Ozet::hesapla(Path::new("bos.log"), "", &log, &ayarlar).unwrap();
        let mut cikti = Vec::new();
        yaz(&mut cikti, &ozet).unwrap();
        let rapor: serde_json::Value = serde_json::from_slice(&cikti).unwrap();
        let ani_artislar = &rapor["ani_artislar"];
        assert!(ani_artislar["baslangic"].is_null());
        assert!(ani_artislar["bitis"].is_null());
        assert_eq!(ani_artislar["dilim_sayisi"], 0);
        assert_eq!(ani_artislar["satirlar"], serde_json::json!([]));
    }

    #[test]
    fn hepsi_bilgisiz_satirlar_ve_bos_alanlar() {
        let rapor = ornek_rapor(&RaporAyarlari {
            ilk: None,
            ..RaporAyarlari::default()
        });
        let satirlar = rapor["kaliplar"]["satirlar"].as_array().unwrap();
        assert_eq!(satirlar.len(), 24);
        assert_eq!(rapor["kaliplar"]["gosterilmeyen"], 0);

        // Kuyruk hatası tamamen vendor içinde: istisna var, konumu yok
        let kuyruk = satirlar
            .iter()
            .find(|satir| satir["kalip"].as_str().unwrap().contains("FaturaGonder"))
            .unwrap();
        assert_eq!(
            kuyruk["istisna"]["sinif"],
            r"Illuminate\Queue\MaxAttemptsExceededException"
        );
        assert!(kuyruk["istisna"]["dosya"].is_null());
        assert!(kuyruk["istisna"]["satir"].is_null());

        // İstisnasız bir kayıt
        let giris = satirlar
            .iter()
            .find(|satir| satir["kalip"].as_str().unwrap().starts_with("Kullanıcı"))
            .unwrap();
        assert!(giris["istisna"].is_null());
    }
}
