//! JSON çıktısı: özeti başka programların (jq, CI adımları, betikler) okuyabileceği biçimde yazar.
//!
//! JSON yazmak için bir kütüphane kullanılmadı: yalnızca yazıyoruz ve yapı küçük. JSON'un
//! kendisi bir Rust `enum`'ıyla modellenir (serde_json'daki `Value` türü de böyle bir enum'dır)
//! ve özyinelemeli olarak yazılır. Doğruluğu testlerde serde_json ile ayrıştırılarak denetlenir;
//! serde_json yalnızca geliştirme bağımlılığıdır, programın içine girmez.

use std::io::{self, Write};

use crate::karsilastirma::{ANLAMLILIK, ARTIS_KATI, Degisim, EN_AZ_KAYIT, Karsilastirma, Pencere};
use crate::rapor::{KalipSatiri, Ozet};

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
    // Karşılaştırma yalnızca istendiyse yazılır; normal raporun biçimi değişmez
    if let Some(karsilastirma) = &ozet.karsilastirma {
        alanlar.push(("karsilastirma", karsilastirma_json(ozet, karsilastirma)));
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
        ("nokta", Json::Metin(karsilastirma.nokta.to_string())),
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
        ("baslangic", Json::Metin(pencere.baslangic.to_string())),
        ("bitis", Json::Metin(pencere.bitis.to_string())),
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

/// Tablodaki bir satırın JSON'u.
fn satir_json(satir: &KalipSatiri<'_>) -> Json {
    Json::Nesne(satir_alanlari(satir))
}

/// Bir kalıp satırının alanları. İstisnası olmayan satırlarda "istisna" null'dır; istisnanın
/// uygulama kodunda yeri bulunamadıysa "dosya" ve "satir" null'dır.
fn satir_alanlari(satir: &KalipSatiri<'_>) -> Vec<(&'static str, Json)> {
    let istisna = match satir.parmak_izi {
        Some(parmak_izi) => {
            let (dosya, satir_no) = match parmak_izi.konum() {
                // u32 her zaman usize'a sığar (Rust'ın desteklediği bütün sistemler 32 ya da
                // 64 bit); `as` burada bilgi kaybettirmez
                Some((dosya, satir_no)) => (metin(dosya), Json::Sayi(satir_no as usize)),
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

#[cfg(test)]
mod testler {
    use super::*;
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
