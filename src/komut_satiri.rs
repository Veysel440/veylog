//! Komut satırı: argümanları okur, seçenekleri rapor ayarlarına çevirir, yardım metnini üretir.
//!
//! ```text
//! veylog [SEÇENEKLER] <log-dosyası>
//! veylog --esik 0,8 --ilk 20 --siralama yeni storage/logs/laravel.log
//! ```
//!
//! Ayrıştırma kuralları:
//! - Seçenekler dosya adından önce ya da sonra gelebilir. Aynı seçenek iki kez yazılırsa
//!   sonuncusu geçerlidir.
//! - Değer boşlukla (`--esik 0,8`), `=` ile (`--esik=0,8`) ya da kısa seçeneklerde bitişik
//!   (`-n20`) verilebilir.
//! - Tek başına `-` ve `--`'dan sonraki her şey dosya adıdır: `veylog -- -garip-ad.log`
//! - Türkçe karakterli yazımlar da geçerlidir: `--eşik`, `--sıralama önem`.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use crate::rapor::{RaporAyarlari, Siralama, VARSAYILAN_ILK, ondalik};

/// Kısa kullanım satırı; hata mesajlarının altında da gösterilir.
pub const KULLANIM: &str = "Kullanım: veylog [SEÇENEKLER] <log-dosyası>";

/// Bilinen uzun seçenekler. Yazım hatası yapıldığında öneri bu listeden seçilir.
const UZUN_SECENEKLER: [&str; 8] = [
    "--esik",
    "--ilk",
    "--hepsi",
    "--siralama",
    "--yardim",
    "--surum",
    "--help",
    "--version",
];

/// Komut satırının istediği iş.
#[derive(Debug)]
pub enum Komut {
    /// Log dosyasını özetle.
    Ozet(Secenekler),
    /// Yardım metnini göster.
    Yardim,
    /// Sürümü göster.
    Surum,
}

/// Özet için gereken her şey: hangi dosya, hangi ayarlarla.
#[derive(Debug)]
pub struct Secenekler {
    pub dosya: PathBuf,
    pub rapor: RaporAyarlari,
}

/// Komut satırındaki bir yanlışlık. Program bunu yazar ve 2 koduyla çıkar.
#[derive(Debug, PartialEq)]
pub enum SecenekHatasi {
    /// Tanınmayan seçenek (`--esk`); benzer bir seçenek varsa önerisiyle.
    BilinmeyenSecenek {
        secenek: String,
        oneri: Option<&'static str>,
    },
    /// Değer bekleyen seçeneğin değeri yok: en sonda kalmış `--esik`.
    DegerEksik(String),
    /// Değer almayan seçeneğe değer verilmiş: `--hepsi=evet`.
    DegerAlmaz(String),
    /// Değer geçersiz: `--esik 1,5`.
    GecersizDeger {
        secenek: String,
        deger: String,
        beklenen: String,
    },
    /// Log dosyası verilmemiş.
    DosyaYok,
    /// İkinci bir dosya adı verilmiş; yalnızca bir log dosyası okunur.
    FazlaDosya(String),
}

impl fmt::Display for SecenekHatasi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecenekHatasi::BilinmeyenSecenek { secenek, oneri } => {
                write!(f, "bilinmeyen seçenek: {secenek}")?;
                // "--esik mi" ama "--siralama mı": soru eki ünlü uyumuna uyar. Kalıp cümle
                // ("bunu mu") her öneride doğru kalır.
                if let Some(oneri) = oneri {
                    write!(f, " (bunu mu demek istediniz: {oneri})")?;
                }
                Ok(())
            }
            SecenekHatasi::DegerEksik(secenek) => write!(f, "{secenek} bir değer bekliyor"),
            SecenekHatasi::DegerAlmaz(secenek) => write!(f, "{secenek} değer almaz"),
            SecenekHatasi::GecersizDeger {
                secenek,
                deger,
                beklenen,
            } => write!(f, "{secenek} için geçersiz değer \"{deger}\": {beklenen}"),
            SecenekHatasi::DosyaYok => write!(f, "log dosyası verilmedi"),
            SecenekHatasi::FazlaDosya(fazla) => write!(
                f,
                "fazladan argüman: {fazla} (yalnızca bir log dosyası verilebilir)"
            ),
        }
    }
}

/// Rust'ta hata türleri `Debug` ve `Display`'e ek olarak `Error` özelliğini de taşır; böylece
/// başka hatalarla aynı yerlerde (ör. `Box<dyn Error>` içinde) kullanılabilir.
impl std::error::Error for SecenekHatasi {}

/// Program adından sonraki argümanları okur.
///
/// Argümanlar `OsString`'dir: işletim sisteminin verdiği ham hali. Linux'ta bir dosya adı
/// geçerli UTF-8 olmayabilir; `String`'e çevirmeye zorlamak programı çökertirdi.
pub fn ayristir<I>(argumanlar: I) -> Result<Komut, SecenekHatasi>
where
    I: IntoIterator<Item = OsString>,
{
    let mut rapor = RaporAyarlari::default();
    let mut dosya: Option<PathBuf> = None;
    let mut secenekler_bitti = false;
    let mut argumanlar = argumanlar.into_iter();

    while let Some(arguman) = argumanlar.next() {
        // Seçenekler "-" ile başlar. Tek başına "-" ve "--"dan sonraki her şey dosya adıdır.
        // Geçerli UTF-8 olmayan bir argüman seçenek olamaz, ama dosya adı olabilir.
        let secenek = match arguman.to_str() {
            Some("--") if !secenekler_bitti => {
                secenekler_bitti = true;
                continue;
            }
            Some(metin) if !secenekler_bitti && metin.starts_with('-') && metin != "-" => {
                metin.to_owned()
            }
            _ => {
                if dosya.is_some() {
                    return Err(SecenekHatasi::FazlaDosya(
                        arguman.to_string_lossy().into_owned(),
                    ));
                }
                dosya = Some(PathBuf::from(arguman));
                continue;
            }
        };

        let (ad, bitisik) = ad_ve_bitisik_deger(&secenek);
        match sadelestir(ad).as_str() {
            "-e" | "--esik" => {
                let deger = deger_al(ad, bitisik, &mut argumanlar)?;
                rapor.drain.benzerlik_esigi = esik_ayristir(ad, &deger)?;
            }
            "-n" | "--ilk" => {
                let deger = deger_al(ad, bitisik, &mut argumanlar)?;
                rapor.ilk = Some(ilk_ayristir(ad, &deger)?);
            }
            "--hepsi" => {
                deger_almaz(ad, bitisik)?;
                rapor.ilk = None;
            }
            "-s" | "--siralama" => {
                let deger = deger_al(ad, bitisik, &mut argumanlar)?;
                rapor.siralama = siralama_ayristir(ad, &deger)?;
            }
            "-h" | "--yardim" | "--help" => {
                deger_almaz(ad, bitisik)?;
                return Ok(Komut::Yardim);
            }
            "-V" | "--surum" | "--version" => {
                deger_almaz(ad, bitisik)?;
                return Ok(Komut::Surum);
            }
            _ => {
                let oneri = oneri_bul(ad);
                return Err(SecenekHatasi::BilinmeyenSecenek { secenek, oneri });
            }
        }
    }

    let dosya = dosya.ok_or(SecenekHatasi::DosyaYok)?;
    Ok(Komut::Ozet(Secenekler { dosya, rapor }))
}

/// `--yardim` ile gösterilen metin. Varsayılan değerler ayarların kendisinden okunur; böylece
/// yardım metni programla çelişemez.
pub fn yardim_metni() -> String {
    let varsayilan = RaporAyarlari::default();
    let siralamalar: String = Siralama::HEPSI
        .iter()
        .map(|siralama| format!("{:28}{:<6}{}\n", "", siralama.ad(), siralama.aciklama()))
        .collect();

    format!(
        "\
veylog {surum}: Laravel log dosyalarını özetler.

{KULLANIM}

Seçenekler:
  -e, --esik <sayı>       Drain benzerlik eşiği, 0 ile 1 arası. Büyüdükçe kalıplar daha az
                          birleşir. [varsayılan: {esik}]
  -n, --ilk <sayı>        Tabloda gösterilecek satır sayısı. [varsayılan: {VARSAYILAN_ILK}]
      --hepsi             Tablonun tamamını göster.
  -s, --siralama <ölçüt>  Tablonun sırası. [varsayılan: {siralama}]
{siralamalar}  -h, --yardim            Bu yardımı göster.
  -V, --surum             Sürümü göster.

Seçenekler dosya adından önce ya da sonra yazılabilir. Değer boşlukla ya da \"=\" ile
verilir (--esik 0,8 ya da --esik=0,8); ondalık ayracı virgül de olabilir, nokta da.
Türkçe karakterli yazımlar da geçerlidir (--eşik, --sıralama önem).

Çıkış kodları: 0 başarılı, 1 dosya okunamadı ya da çıktı yazılamadı, 2 kullanım hatası.

Örnekler:
  veylog storage/logs/laravel.log
  veylog --esik 0,8 --ilk 20 storage/logs/laravel.log
  veylog --siralama yeni --hepsi storage/logs/laravel.log
",
        surum = env!("CARGO_PKG_VERSION"),
        esik = ondalik(varsayilan.drain.benzerlik_esigi),
        siralama = varsayilan.siralama.ad(),
    )
}

/// Seçeneği adına ve bitişik yazılmış değerine ayırır:
/// `--esik=0,8` → (`--esik`, `0,8`), `-n20` → (`-n`, `20`), `--hepsi` → (`--hepsi`, yok)
fn ad_ve_bitisik_deger(secenek: &str) -> (&str, Option<&str>) {
    if secenek.starts_with("--") {
        match secenek.split_once('=') {
            Some((ad, deger)) => (ad, Some(deger)),
            None => (secenek, None),
        }
    } else {
        // Kısa seçenek: "-" ve bir harf. Üçüncü karakterden sonrası değerdir. Harf sınırı
        // `char_indices` ile bulunur; Türkçe harfler UTF-8'de birden fazla bayttır.
        match secenek.char_indices().nth(2) {
            Some((konum, _)) => (&secenek[..konum], Some(&secenek[konum..])),
            None => (secenek, None),
        }
    }
}

/// Seçeneğin değerini alır: bitişik yazıldıysa ondan, yoksa bir sonraki argümandan.
fn deger_al(
    ad: &str,
    bitisik: Option<&str>,
    argumanlar: &mut impl Iterator<Item = OsString>,
) -> Result<String, SecenekHatasi> {
    if let Some(deger) = bitisik {
        return Ok(deger.to_owned());
    }
    match argumanlar.next() {
        // Geçerli UTF-8 olmayan baytlar � olur; böyle bir değer zaten ayrıştırılamaz
        Some(deger) => Ok(deger.to_string_lossy().into_owned()),
        None => Err(SecenekHatasi::DegerEksik(ad.to_owned())),
    }
}

/// Değer almayan bir seçeneğe (`--hepsi=evet`, `-hV`) değer verilmişse hata.
fn deger_almaz(ad: &str, bitisik: Option<&str>) -> Result<(), SecenekHatasi> {
    match bitisik {
        Some(_) => Err(SecenekHatasi::DegerAlmaz(ad.to_owned())),
        None => Ok(()),
    }
}

/// `--esik`: 0 ile 1 arasında bir sayı. Türkçedeki gibi virgülle de yazılabilir.
fn esik_ayristir(ad: &str, deger: &str) -> Result<f64, SecenekHatasi> {
    match deger.replace(',', ".").parse::<f64>() {
        // "NaN" ve "inf" de f64 olarak ayrıştırılır. NaN hiçbir sayıyla karşılaştırılamadığı
        // için aralık denetimi ikisini de reddeder.
        Ok(esik) if (0.0..=1.0).contains(&esik) => Ok(esik),
        _ => Err(gecersiz(
            ad,
            deger,
            "0 ile 1 arasında bir sayı olmalı (ör. 0,7)",
        )),
    }
}

/// `--ilk`: en az 1 olan bir tam sayı.
fn ilk_ayristir(ad: &str, deger: &str) -> Result<usize, SecenekHatasi> {
    match deger.parse::<usize>() {
        Ok(sayi) if sayi >= 1 => Ok(sayi),
        _ => Err(gecersiz(
            ad,
            deger,
            "1 ya da daha büyük bir tam sayı olmalı (tablonun tamamı için --hepsi)",
        )),
    }
}

/// `--siralama`: sıralamalardan birinin adı.
fn siralama_ayristir(ad: &str, deger: &str) -> Result<Siralama, SecenekHatasi> {
    Siralama::ayristir(&sadelestir(deger)).ok_or_else(|| {
        let adlar: Vec<&str> = Siralama::HEPSI
            .iter()
            .map(|siralama| siralama.ad())
            .collect();
        gecersiz(
            ad,
            deger,
            format!("şunlardan biri olmalı: {}", adlar.join(", ")),
        )
    })
}

fn gecersiz(secenek: &str, deger: &str, beklenen: impl Into<String>) -> SecenekHatasi {
    SecenekHatasi::GecersizDeger {
        secenek: secenek.to_owned(),
        deger: deger.to_owned(),
        beklenen: beklenen.into(),
    }
}

/// Türkçe harfleri ASCII karşılıklarına çevirir: `--sıralama` → `--siralama`, `önem` → `onem`.
/// Böylece seçenekler hem Türkçe klavyeyle hem ASCII ile yazılabilir.
fn sadelestir(metin: &str) -> String {
    metin
        .chars()
        .map(|harf| match harf {
            'ç' => 'c',
            'ğ' => 'g',
            'ı' => 'i',
            'ö' => 'o',
            'ş' => 's',
            'ü' => 'u',
            diger => diger,
        })
        .collect()
}

/// Bilinmeyen bir uzun seçeneğe en çok benzeyen geçerli seçenek. Yazım hatası sayılacak kadar
/// yakın değilse None: adı 5 harften kısaysa en fazla 1, uzunsa en fazla 2 harf fark.
fn oneri_bul(secenek: &str) -> Option<&'static str> {
    let ad = sadelestir(secenek.strip_prefix("--")?);
    let sinir = if ad.chars().count() >= 5 { 2 } else { 1 };
    UZUN_SECENEKLER
        .into_iter()
        .map(|aday| (duzenleme_uzakligi(&ad, &aday[2..]), aday))
        .filter(|&(uzaklik, _)| uzaklik <= sinir)
        .min_by_key(|&(uzaklik, _)| uzaklik)
        .map(|(_, aday)| aday)
}

/// İki metin arasındaki düzenleme uzaklığı (Levenshtein): birini ötekine çevirmek için gereken
/// en az harf ekleme, silme ya da değiştirme sayısı. `esk` → `esik`: 1, `hepis` → `hepsi`: 2
///
/// Dinamik programlama: `onceki[j]`, `a`nın şimdiye kadar işlenen kısmını `b`nin ilk `j`
/// harfine çevirmenin en az maliyetidir. Tablonun yalnızca iki satırı tutulur.
fn duzenleme_uzakligi(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut onceki: Vec<usize> = (0..=b.len()).collect();
    for (i, harf_a) in a.chars().enumerate() {
        let mut simdiki = vec![i + 1; b.len() + 1];
        for (j, &harf_b) in b.iter().enumerate() {
            let degistir = onceki[j] + usize::from(harf_a != harf_b);
            let sil = onceki[j + 1] + 1;
            let ekle = simdiki[j] + 1;
            simdiki[j + 1] = degistir.min(sil).min(ekle);
        }
        onceki = simdiki;
    }
    onceki[b.len()]
}

#[cfg(test)]
mod testler {
    use super::*;

    fn ayir(argumanlar: &[&str]) -> Result<Komut, SecenekHatasi> {
        ayristir(argumanlar.iter().map(OsString::from))
    }

    /// Özet bekleyen testler için: ayrıştırır ve seçenekleri döndürür.
    fn ozet(argumanlar: &[&str]) -> Secenekler {
        match ayir(argumanlar) {
            Ok(Komut::Ozet(secenekler)) => secenekler,
            diger => panic!("özet bekleniyordu, gelen: {diger:?}"),
        }
    }

    fn hata(argumanlar: &[&str]) -> SecenekHatasi {
        ayir(argumanlar).unwrap_err()
    }

    #[test]
    fn varsayilanlar_kullanilir() {
        let secenekler = ozet(&["laravel.log"]);
        assert_eq!(secenekler.dosya, PathBuf::from("laravel.log"));
        assert_eq!(secenekler.rapor.drain.benzerlik_esigi, 0.7);
        assert_eq!(secenekler.rapor.ilk, Some(VARSAYILAN_ILK));
        assert_eq!(secenekler.rapor.siralama, Siralama::Onem);
    }

    #[test]
    fn butun_yazimlar_ayni_sonucu_verir() {
        for argumanlar in [
            &[
                "--esik",
                "0.8",
                "--ilk",
                "20",
                "--siralama",
                "adet",
                "a.log",
            ][..],
            &["--esik=0,8", "--ilk=20", "--siralama=adet", "a.log"],
            &["-e", "0,8", "-n", "20", "-s", "adet", "a.log"],
            &["-e0.8", "-n20", "-sadet", "a.log"],
            &[
                "a.log",
                "--eşik",
                "0,8",
                "--ilk",
                "20",
                "--sıralama",
                "adet",
            ],
        ] {
            let secenekler = ozet(argumanlar);
            assert_eq!(secenekler.dosya, PathBuf::from("a.log"), "{argumanlar:?}");
            assert_eq!(
                secenekler.rapor.drain.benzerlik_esigi, 0.8,
                "{argumanlar:?}"
            );
            assert_eq!(secenekler.rapor.ilk, Some(20), "{argumanlar:?}");
            assert_eq!(secenekler.rapor.siralama, Siralama::Adet, "{argumanlar:?}");
        }
        assert_eq!(
            ozet(&["--siralama", "önem", "a.log"]).rapor.siralama,
            Siralama::Onem
        );
    }

    #[test]
    fn ayni_secenek_tekrarlanirsa_sonuncusu_gecerlidir() {
        assert_eq!(ozet(&["--ilk", "5", "--hepsi", "a.log"]).rapor.ilk, None);
        assert_eq!(ozet(&["--hepsi", "--ilk", "5", "a.log"]).rapor.ilk, Some(5));
        assert_eq!(
            ozet(&["-e", "0,5", "-e", "0,9", "a.log"])
                .rapor
                .drain
                .benzerlik_esigi,
            0.9
        );
    }

    #[test]
    fn cizgiyle_baslayan_dosya_adlari_okunabilir() {
        assert_eq!(ozet(&["--", "--esik"]).dosya, PathBuf::from("--esik"));
        assert_eq!(ozet(&["-"]).dosya, PathBuf::from("-"));
        // "--"dan sonra seçenek de olsa dosya adıdır; ikinci dosya adı hatadır
        assert_eq!(
            hata(&["--", "a.log", "-n"]),
            SecenekHatasi::FazlaDosya("-n".to_owned())
        );
    }

    #[test]
    fn yardim_ve_surum_dosya_gerektirmez() {
        for argumanlar in [
            &["--yardim"][..],
            &["-h"],
            &["--help"],
            &["--yardım"],
            &["a.log", "--help"],
        ] {
            assert!(
                matches!(ayir(argumanlar), Ok(Komut::Yardim)),
                "{argumanlar:?}"
            );
        }
        for argumanlar in [&["--surum"][..], &["-V"], &["--version"], &["--sürüm"]] {
            assert!(
                matches!(ayir(argumanlar), Ok(Komut::Surum)),
                "{argumanlar:?}"
            );
        }
    }

    #[test]
    fn gecersiz_degerler_reddedilir() {
        for argumanlar in [
            &["--esik", "1,5"][..],
            &["--esik", "-0.1"],
            &["--esik", "yarım"],
            &["--esik", "NaN"],
            &["--esik", "inf"],
            &["--esik="],
            &["--ilk", "0"],
            &["--ilk", "-3"],
            &["--ilk", "2.5"],
            &["--siralama", "rastgele"],
        ] {
            let mut tam: Vec<&str> = argumanlar.to_vec();
            tam.push("a.log");
            assert!(
                matches!(hata(&tam), SecenekHatasi::GecersizDeger { .. }),
                "{tam:?}"
            );
        }
    }

    #[test]
    fn eksik_fazla_ve_bilinmeyen_argumanlar_reddedilir() {
        assert_eq!(hata(&[]), SecenekHatasi::DosyaYok);
        assert_eq!(
            hata(&["a.log", "b.log"]),
            SecenekHatasi::FazlaDosya("b.log".to_owned())
        );
        assert_eq!(
            hata(&["a.log", "--esik"]),
            SecenekHatasi::DegerEksik("--esik".to_owned())
        );
        assert_eq!(
            hata(&["--hepsi=evet", "a.log"]),
            SecenekHatasi::DegerAlmaz("--hepsi".to_owned())
        );
        assert_eq!(
            hata(&["-x", "a.log"]),
            SecenekHatasi::BilinmeyenSecenek {
                secenek: "-x".to_owned(),
                oneri: None
            }
        );
    }

    #[test]
    fn yazim_hatasina_oneri_verilir() {
        for (yazilan, oneri) in [
            ("--esk", Some("--esik")),
            ("--siralma", Some("--siralama")),
            ("--hepis", Some("--hepsi")),
            ("--Esik", Some("--esik")),
            ("--verison", Some("--version")),
            ("--top", None),
            ("--all", None),
        ] {
            assert_eq!(
                hata(&[yazilan, "a.log"]),
                SecenekHatasi::BilinmeyenSecenek {
                    secenek: yazilan.to_owned(),
                    oneri
                },
                "{yazilan}"
            );
        }
    }

    #[test]
    fn listedeki_her_uzun_secenek_taninir() {
        // Öneri listesi ile ayrıştırıcı birbirinden kopmasın
        for secenek in UZUN_SECENEKLER {
            let sonuc = ayir(&[secenek, "1", "a.log"]);
            assert!(
                !matches!(sonuc, Err(SecenekHatasi::BilinmeyenSecenek { .. })),
                "{secenek}"
            );
        }
    }

    #[test]
    fn hata_mesajlari_anlasilir() {
        assert_eq!(
            hata(&["--esik", "1,5", "a.log"]).to_string(),
            "--esik için geçersiz değer \"1,5\": 0 ile 1 arasında bir sayı olmalı (ör. 0,7)"
        );
        assert_eq!(
            hata(&["--siralama", "x", "a.log"]).to_string(),
            "--siralama için geçersiz değer \"x\": şunlardan biri olmalı: onem, adet, yeni"
        );
        assert_eq!(
            hata(&["--esk", "a.log"]).to_string(),
            "bilinmeyen seçenek: --esk (bunu mu demek istediniz: --esik)"
        );
    }

    #[test]
    fn duzenleme_uzakligi_dogru_hesaplanir() {
        assert_eq!(duzenleme_uzakligi("esik", "esik"), 0);
        assert_eq!(duzenleme_uzakligi("esk", "esik"), 1);
        assert_eq!(duzenleme_uzakligi("hepis", "hepsi"), 2);
        assert_eq!(duzenleme_uzakligi("", "ilk"), 3);
        assert_eq!(duzenleme_uzakligi("kitten", "sitting"), 3);
    }

    #[test]
    fn yardim_metni_varsayilanlari_ve_siralamalari_icerir() {
        let metin = yardim_metni();
        assert!(metin.contains(KULLANIM));
        assert!(metin.contains("[varsayılan: 0,7]"));
        assert!(metin.contains("[varsayılan: 10]"));
        for siralama in Siralama::HEPSI {
            assert!(metin.contains(siralama.ad()), "{}", siralama.ad());
        }
    }

    #[cfg(unix)]
    #[test]
    fn utf8_olmayan_dosya_adi_kabul_edilir() {
        use std::os::unix::ffi::OsStringExt;
        let ad = OsString::from_vec(b"bozuk-\xff.log".to_vec());
        let secenekler = match ayristir([ad.clone()]) {
            Ok(Komut::Ozet(secenekler)) => secenekler,
            diger => panic!("özet bekleniyordu, gelen: {diger:?}"),
        };
        assert_eq!(secenekler.dosya, PathBuf::from(ad));
    }
}
