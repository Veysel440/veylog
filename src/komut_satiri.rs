//! Komut satırı: argümanları okur, seçenekleri rapor ayarlarına çevirir, yardım metnini üretir.
//!
//! ```text
//! veylog [SEÇENEKLER] <log-dosyası>
//! veylog --esik 0,8 --ilk 20 --siralama yeni storage/logs/laravel.log
//! veylog --bicim json --hepsi storage/logs/laravel.log
//! veylog --karsilastir 15:00 storage/logs/laravel.log
//! veylog --ani-artis 1 storage/logs/laravel.log
//! veylog --seviye error --son 2sa storage/logs/laravel.log
//! veylog --saat-dilimi +03:00 storage/logs/laravel.log
//! veylog --desen '%datetime% [%level_name%] %message% %context%' storage/logs/app.log
//! veylog --zaman-bicimi 'd.m.Y H:i:s' storage/logs/laravel.log
//! veylog --konum satir storage/logs/laravel.log
//! veylog --izle --son 1sa storage/logs/laravel.log
//! veylog --lang en --last 2h storage/logs/laravel.log
//! zcat laravel-2026-10-24.log.gz | veylog -
//! ```
//!
//! Ayrıştırma kuralları:
//! - Seçenekler dosya adından önce ya da sonra gelebilir. Aynı seçenek iki kez yazılırsa
//!   sonuncusu geçerlidir.
//! - Değer boşlukla (`--esik 0,8`), `=` ile (`--esik=0,8`) ya da kısa seçeneklerde bitişik
//!   (`-n20`) verilebilir.
//! - Tek başına `-` ve `--`'dan sonraki her şey dosya adıdır: `veylog -- -garip-ad.log`. `-`
//!   adlı dosya ise standart girdidir (okunması `main`'in işidir).
//! - Türkçe karakterli yazımlar da geçerlidir: `--eşik`, `--sıralama önem`.
//! - Her uzun seçeneğin ve her değer adının İngilizcesi de her dilde geçerlidir: `--threshold`,
//!   `--sort count` (`lang::OPTION_NAMES`). Yardım ve mesajlar seçili dildeki adları gösterir.

use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use crate::ani_artis::{EN_DUSUK_SEVIYE, EN_UZUN_DILIM};
use crate::datetime::{TimePoint, TimeZone};
use crate::filtre::{milisaniye_ayristir, sure_ayristir};
use crate::izle::{EN_KISA_ARALIK, EN_UZUN_ARALIK, VARSAYILAN_ARALIK, aralik_yaz};
use crate::kayit::Seviye;
use crate::lang::{self, Lang, OPTION_NAMES};
use crate::parmak_izi::KonumTuru;
use crate::rapor::{RaporAyarlari, Siralama, VARSAYILAN_ILK, ondalik};
use crate::satir_bicimi::SatirBicimi;
use crate::time_format::TimeFormat;

/// Dili seçen ortam değişkenleri, önce okunandan başlayarak: `VEYLOG_LANG=tr` ve Türkçe adıyla
/// `VEYLOG_DIL=tr`. `--lang` (`--dil`) verilirse o geçerlidir.
pub const DIL_DEGISKENLERI: [&str; 2] = ["VEYLOG_LANG", "VEYLOG_DIL"];

/// Kısa kullanım satırı; hata mesajlarının altında da gösterilir.
pub fn kullanim() -> &'static str {
    lang::pick(
        "Kullanım: veylog [SEÇENEKLER] <log-dosyası>",
        "Usage: veylog [OPTIONS] <log-file>",
    )
}

/// Çıktının biçimi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bicim {
    /// Terminal için hizalı tablo.
    Metin,
    /// Başka programlar için: jq, CI adımları, betikler.
    Json,
    /// GitHub issue ve PR'larına yapıştırmak için.
    Markdown,
}

impl Bicim {
    /// Bütün biçimler. Yardım metni ve hata mesajları bu listeden üretilir.
    pub const HEPSI: [Bicim; 3] = [Bicim::Metin, Bicim::Json, Bicim::Markdown];

    /// Komut satırındaki Türkçe adı.
    pub fn ad(self) -> &'static str {
        match self {
            Bicim::Metin => "metin",
            Bicim::Json => "json",
            Bicim::Markdown => "markdown",
        }
    }

    /// Komut satırındaki İngilizce adı.
    pub fn ingilizce_ad(self) -> &'static str {
        match self {
            Bicim::Metin => "text",
            diger => diger.ad(),
        }
    }

    /// Seçili dildeki adı: yardım metni ve hata mesajları için.
    pub fn gosterilen_ad(self) -> &'static str {
        lang::pick(self.ad(), self.ingilizce_ad())
    }

    /// Yardım metnindeki açıklaması.
    pub fn aciklama(self) -> &'static str {
        match self {
            Bicim::Metin => lang::pick("terminal için tablo", "table for the terminal"),
            Bicim::Json => lang::pick(
                "başka programlar için (jq, CI adımları)",
                "for other programs (jq, CI steps)",
            ),
            Bicim::Markdown => lang::pick(
                "GitHub issue ve PR'larına yapıştırmak için",
                "to paste into GitHub issues and PRs",
            ),
        }
    }

    /// Komut satırındaki adından (Türkçe ya da İngilizce) biçime çevirir: "json" → Bicim::Json
    pub fn ayristir(ad: &str) -> Option<Bicim> {
        Bicim::HEPSI
            .into_iter()
            .find(|bicim| bicim.ad() == ad || bicim.ingilizce_ad() == ad)
    }
}

/// Komut satırının istediği iş.
#[derive(Debug)]
pub enum Komut {
    /// Log dosyasını özetle. Seçenekler kutudadır: yardım ve sürüm değerleri boşken enum,
    /// içindeki en büyük değer kadar yer kaplamasın.
    Ozet(Box<Secenekler>),
    /// Yardım metnini göster.
    Yardim,
    /// Sürümü göster.
    Surum,
}

/// Özet için gereken her şey: hangi dosya, hangi ayarlarla, hangi biçimde.
#[derive(Debug)]
pub struct Secenekler {
    pub dosya: PathBuf,
    pub rapor: RaporAyarlari,
    pub bicim: Bicim,
    /// `--izle` verildiyse canlı izlemenin ayarları.
    pub izleme: Option<Izleme>,
}

/// Canlı izlemenin ayarları (`--izle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Izleme {
    /// Girdiye ne sıklıkla bakılacağı ve raporun en sık ne kadarda bir çizileceği.
    pub aralik: Duration,
}

impl Default for Izleme {
    fn default() -> Self {
        Izleme {
            aralik: VARSAYILAN_ARALIK,
        }
    }
}

/// Komut satırındaki bir yanlışlık. Program bunu yazar ve 2 koduyla çıkar. Mesajı seçili
/// dilde yazılır (`Display`).
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
    /// Dosya adı boş: `veylog "$LOG"` ve LOG tanımsız.
    BosDosyaAdi,
    /// Birlikte kullanılamayan iki seçenek (Türkçe adlarıyla): `--son` ile `--baslangic`.
    Celisen(&'static str, &'static str),
    /// Yalnızca başka bir seçenekle anlamlı bir seçenek tek başına verilmiş (Türkçe
    /// adlarıyla): `--izle`siz `--aralik`.
    Gerektirir(&'static str, &'static str),
    /// Seçenek canlı izlemeyle (`--izle`) kullanılamaz; nedeni Türkçe ve İngilizce.
    IzlemeyleKullanilamaz {
        secenek: String,
        neden: [&'static str; 2],
    },
}

impl fmt::Display for SecenekHatasi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ingilizce = lang::current() == Lang::En;
        match self {
            SecenekHatasi::BilinmeyenSecenek { secenek, oneri } => {
                // "--esik mi" ama "--siralama mı": soru eki ünlü uyumuna uyar. Kalıp cümle
                // ("bunu mu") her öneride doğru kalır.
                match (ingilizce, oneri) {
                    (false, None) => write!(f, "bilinmeyen seçenek: {secenek}"),
                    (false, Some(oneri)) => write!(
                        f,
                        "bilinmeyen seçenek: {secenek} (bunu mu demek istediniz: {oneri})"
                    ),
                    (true, None) => write!(f, "unknown option: {secenek}"),
                    (true, Some(oneri)) => {
                        write!(f, "unknown option: {secenek} (did you mean {oneri}?)")
                    }
                }
            }
            SecenekHatasi::DegerEksik(secenek) if ingilizce => {
                write!(f, "{secenek} needs a value")
            }
            SecenekHatasi::DegerEksik(secenek) => write!(f, "{secenek} bir değer bekliyor"),
            SecenekHatasi::DegerAlmaz(secenek) if ingilizce => {
                write!(f, "{secenek} does not take a value")
            }
            SecenekHatasi::DegerAlmaz(secenek) => write!(f, "{secenek} değer almaz"),
            SecenekHatasi::GecersizDeger {
                secenek,
                deger,
                beklenen,
            } => {
                if ingilizce {
                    write!(f, "invalid value \"{deger}\" for {secenek}: {beklenen}")
                } else {
                    write!(f, "{secenek} için geçersiz değer \"{deger}\": {beklenen}")
                }
            }
            SecenekHatasi::DosyaYok => {
                f.write_str(lang::pick("log dosyası verilmedi", "no log file given"))
            }
            SecenekHatasi::FazlaDosya(fazla) if ingilizce => write!(
                f,
                "unexpected argument: {fazla} (only one log file can be given)"
            ),
            SecenekHatasi::FazlaDosya(fazla) => write!(
                f,
                "fazladan argüman: {fazla} (yalnızca bir log dosyası verilebilir)"
            ),
            SecenekHatasi::BosDosyaAdi => f.write_str(lang::pick(
                "dosya adı boş olamaz",
                "the file name cannot be empty",
            )),
            SecenekHatasi::Celisen(birinci, ikinci) => {
                let (birinci, ikinci) = (lang::option_name(birinci), lang::option_name(ikinci));
                if ingilizce {
                    write!(f, "{birinci} and {ikinci} cannot be used together")
                } else {
                    write!(f, "{birinci} ile {ikinci} birlikte kullanılamaz")
                }
            }
            SecenekHatasi::Gerektirir(secenek, gereken) => {
                let (secenek, gereken) = (lang::option_name(secenek), lang::option_name(gereken));
                if ingilizce {
                    write!(f, "{secenek} can only be used with {gereken}")
                } else {
                    write!(f, "{secenek} yalnızca {gereken} ile kullanılabilir")
                }
            }
            SecenekHatasi::IzlemeyleKullanilamaz {
                secenek,
                neden: [tr, en],
            } => {
                if ingilizce {
                    write!(f, "{secenek} cannot be used with --watch: {en}")
                } else {
                    write!(f, "{secenek} ile --izle birlikte kullanılamaz: {tr}")
                }
            }
        }
    }
}

/// Rust'ta hata türleri `Debug` ve `Display`'e ek olarak `Error` özelliğini de taşır; böylece
/// başka hatalarla aynı yerlerde (ör. `Box<dyn Error>` içinde) kullanılabilir.
impl std::error::Error for SecenekHatasi {}

/// Argümanlarda seçilen dil: `--dil en` (`--lang en`, `--dil=en`); yoksa ya da tanınmıyorsa
/// None (`main` o zaman ortam değişkenlerine bakar).
///
/// Argümanlar ayrıştırılmadan önce bakılır: ayrıştırma hataları da seçilen dilde yazılsın.
/// Tanınmayan bir `--dil` değerini ayrıştırıcı ayrıca hata olarak bildirir. Seçenek birden
/// çok kez yazıldıysa sonuncusu geçerlidir; `--`'dan sonrası dosya adıdır.
pub fn dil_bul(argumanlar: &[OsString]) -> Option<Lang> {
    let mut bulunan = None;
    let mut argumanlar = argumanlar.iter();
    while let Some(arguman) = argumanlar.next() {
        let Some(metin) = arguman.to_str() else {
            continue;
        };
        if metin == "--" {
            break;
        }
        let (ad, bitisik) = ad_ve_bitisik_deger(metin);
        if matches!(sadelestir(ad).as_str(), "--dil" | "--lang") {
            let deger = match bitisik {
                Some(deger) => Some(deger),
                None => argumanlar.next().and_then(|deger| deger.to_str()),
            };
            if let Some(dil) = deger.and_then(Lang::parse) {
                bulunan = Some(dil);
            }
        }
    }
    bulunan
}

/// Program adından sonraki argümanları okur.
///
/// Argümanlar `OsString`'dir: işletim sisteminin verdiği ham hali. Linux'ta bir dosya adı
/// geçerli UTF-8 olmayabilir; `String`'e çevirmeye zorlamak programı çökertirdi.
pub fn ayristir<I>(argumanlar: I) -> Result<Komut, SecenekHatasi>
where
    I: IntoIterator<Item = OsString>,
{
    let mut rapor = RaporAyarlari::default();
    let mut bicim = Bicim::Metin;
    let mut dosya: Option<PathBuf> = None;
    // Desen zaman biçimine bağlıdır (Monolog'un dizgesinde `%datetime%`): ikisi de okununca
    // derlenir. Seçeneğin yazıldığı ad hata mesajı için saklanır.
    let mut desen: Option<(String, String)> = None;
    let mut zaman_bicimi: Option<TimeFormat> = None;
    let mut izle = false;
    let mut aralik: Option<Duration> = None;
    // `--bicim`in yazıldığı hali: `--izle` ile çelişirse hata mesajında
    let mut bicim_yazimi: Option<String> = None;
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
                // Boş bir ad hiçbir dosyayı göstermez; okunamadı hatası (ya da fazladan
                // argüman hatası) adsız kalırdı
                if arguman.is_empty() {
                    return Err(SecenekHatasi::BosDosyaAdi);
                }
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
        let sonuc = secenek_isle(
            &secenek,
            ad,
            bitisik,
            &mut argumanlar,
            &mut Durum {
                rapor: &mut rapor,
                bicim: &mut bicim,
                bicim_yazimi: &mut bicim_yazimi,
                desen: &mut desen,
                zaman_bicimi: &mut zaman_bicimi,
                izle: &mut izle,
                aralik: &mut aralik,
            },
        );
        match sonuc {
            Ok(Some(komut)) => return Ok(komut),
            Ok(None) => {}
            // Tek tireyle yazılmış bir uzun seçenek (`-hepsi`) kısa seçenek ve değeri sayılıp
            // yanlış bir hata verirdi ("-h değer almaz"): uzun seçeneği önerilir
            Err(hata) => {
                let uzun = (!secenek.starts_with("--") && secenek.chars().count() > 2)
                    .then(|| oneri_bul(&format!("-{secenek}")))
                    .flatten();
                return Err(match uzun {
                    Some(oneri) => SecenekHatasi::BilinmeyenSecenek {
                        secenek,
                        oneri: Some(oneri),
                    },
                    None => hata,
                });
            }
        }
    }

    // `--son` aralığın başını kendisi belirler: başka bir sınırla çelişir
    if rapor.filtre.son.is_some() {
        if rapor.filtre.baslangic.is_some() {
            return Err(SecenekHatasi::Celisen("--son", "--baslangic"));
        }
        if rapor.filtre.bitis.is_some() {
            return Err(SecenekHatasi::Celisen("--son", "--bitis"));
        }
    }
    rapor.satir_bicimi = match &desen {
        Some((ad, deger)) => SatirBicimi::new(Some(deger), zaman_bicimi)
            .map_err(|hata| gecersiz(ad, deger, hata.to_string()))?,
        None => SatirBicimi::new(None, zaman_bicimi).expect("desensiz biçim her zaman derlenir"),
    };
    let izleme = if izle {
        // İzlenen log sürekli uzar: rapor her çizimde baştan yazılır, bu yalnızca metin
        // biçiminde okunabilir; karşılaştırmanın "sonra" yarısı da hiç bitmez
        if bicim != Bicim::Metin {
            return Err(SecenekHatasi::IzlemeyleKullanilamaz {
                secenek: bicim_yazimi.unwrap_or_default(),
                neden: [
                    "izleme yalnızca metin biçimindedir",
                    "watching is only in the text format",
                ],
            });
        }
        if rapor.karsilastir.is_some() {
            return Err(SecenekHatasi::IzlemeyleKullanilamaz {
                secenek: lang::option_name("--karsilastir").to_owned(),
                neden: [
                    "izlenen log sürekli uzar; karşılaştırma için --izle olmadan çalıştırın",
                    "a watched log keeps growing; run without --watch to compare",
                ],
            });
        }
        Some(Izleme {
            aralik: aralik.unwrap_or(VARSAYILAN_ARALIK),
        })
    } else {
        if aralik.is_some() {
            return Err(SecenekHatasi::Gerektirir("--aralik", "--izle"));
        }
        None
    };
    let dosya = dosya.ok_or(SecenekHatasi::DosyaYok)?;
    Ok(Komut::Ozet(Box::new(Secenekler {
        dosya,
        rapor,
        bicim,
        izleme,
    })))
}

/// Ayrıştırılırken biriken seçenekler: `ayristir`ın yerel değişkenleri.
struct Durum<'a> {
    rapor: &'a mut RaporAyarlari,
    bicim: &'a mut Bicim,
    /// `--bicim`in yazıldığı hali: `--izle` ile çelişirse hata mesajında
    bicim_yazimi: &'a mut Option<String>,
    desen: &'a mut Option<(String, String)>,
    zaman_bicimi: &'a mut Option<TimeFormat>,
    izle: &'a mut bool,
    aralik: &'a mut Option<Duration>,
}

/// Bir seçeneği işler: `ad` seçeneğin adı, `bitisik` bitişik yazılmış değeri, `secenek`
/// yazıldığı hali. Yardım ya da sürüm istendiyse o komut.
fn secenek_isle(
    secenek: &str,
    ad: &str,
    bitisik: Option<&str>,
    argumanlar: &mut impl Iterator<Item = OsString>,
    durum: &mut Durum<'_>,
) -> Result<Option<Komut>, SecenekHatasi> {
    match sadelestir(ad).as_str() {
        "-e" | "--esik" | "--threshold" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            durum.rapor.drain.similarity_threshold = esik_ayristir(ad, &deger)?;
        }
        "--konum" | "--location" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            durum.rapor.konum_turu = konum_ayristir(ad, &deger)?;
        }
        "-n" | "--ilk" | "--top" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            durum.rapor.ilk = Some(ilk_ayristir(ad, &deger)?);
        }
        "--hepsi" | "--all" => {
            deger_almaz(ad, bitisik)?;
            durum.rapor.ilk = None;
        }
        "-s" | "--siralama" | "--sort" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            durum.rapor.siralama = siralama_ayristir(ad, &deger)?;
        }
        "-b" | "--bicim" | "--format" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            *durum.bicim = bicim_ayristir(ad, &deger)?;
            *durum.bicim_yazimi = Some(format!("{ad} {deger}"));
        }
        "-k" | "--karsilastir" | "--compare" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            durum.rapor.karsilastir = Some(karsilastir_ayristir(ad, &deger)?);
        }
        "-a" | "--ani-artis" | "--spikes" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            durum.rapor.ani_artis = Some(ani_artis_ayristir(ad, &deger)?);
        }
        "--seviye" | "--level" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            durum.rapor.filtre.seviye = Some(seviye_ayristir(ad, &deger)?);
        }
        "--baslangic" | "--since" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            durum.rapor.filtre.baslangic = Some(sinir_ayristir(ad, &deger)?);
        }
        "--bitis" | "--until" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            durum.rapor.filtre.bitis = Some(sinir_ayristir(ad, &deger)?);
        }
        "--son" | "--last" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            durum.rapor.filtre.son = Some(son_ayristir(ad, &deger)?);
        }
        "--saat-dilimi" | "--timezone" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            durum.rapor.saat_dilimi = Some(saat_dilimi_ayristir(ad, &deger)?);
        }
        "--desen" | "--pattern" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            *durum.desen = Some((ad.to_owned(), deger));
        }
        "--zaman-bicimi" | "--time-format" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            *durum.zaman_bicimi = Some(
                TimeFormat::compile(&deger)
                    .map_err(|hata| gecersiz(ad, &deger, hata.to_string()))?,
            );
        }
        "--izle" | "--watch" => {
            deger_almaz(ad, bitisik)?;
            *durum.izle = true;
        }
        "--aralik" | "--interval" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            *durum.aralik = Some(aralik_ayristir(ad, &deger)?);
        }
        // Dil, ayrıştırmadan önce seçilir (`dil_bul`); burada yalnızca denetlenir
        "--dil" | "--lang" => {
            let deger = deger_al(ad, bitisik, argumanlar)?;
            dil_ayristir(ad, &deger)?;
        }
        "-h" | "--yardim" | "--help" => {
            deger_almaz(ad, bitisik)?;
            return Ok(Some(Komut::Yardim));
        }
        "-V" | "--surum" | "--version" => {
            deger_almaz(ad, bitisik)?;
            return Ok(Some(Komut::Surum));
        }
        _ => {
            let oneri = oneri_bul(ad).or_else(|| kisa_oneri(ad));
            return Err(SecenekHatasi::BilinmeyenSecenek {
                secenek: secenek.to_owned(),
                oneri,
            });
        }
    }
    Ok(None)
}

/// Kısa seçeneklerin adları.
const KISA_SECENEKLER: [&str; 8] = ["-e", "-n", "-s", "-b", "-k", "-a", "-h", "-V"];

/// Bilinmeyen bir kısa seçeneğin büyük ya da küçük harfle yazılmış hali bir kısa seçenekse o:
/// `-v` → `-V`, `-N` → `-n`.
fn kisa_oneri(ad: &str) -> Option<&'static str> {
    let harf = ad.strip_prefix('-')?;
    if harf.chars().count() != 1 {
        return None;
    }
    KISA_SECENEKLER.into_iter().find(|kisa| {
        let kisa = &kisa[1..];
        kisa != harf && kisa.eq_ignore_ascii_case(harf)
    })
}

/// `--yardim` ile gösterilen metin, seçili dilde. Varsayılan değerler ayarların kendisinden
/// okunur; böylece yardım metni programla çelişemez.
pub fn yardim_metni() -> String {
    let varsayilan = RaporAyarlari::default();
    let siralamalar: String = Siralama::HEPSI
        .iter()
        .map(|siralama| {
            format!(
                "{:28}{:<10}{}\n",
                "",
                siralama.gosterilen_ad(),
                siralama.aciklama()
            )
        })
        .collect();
    let bicimler: String = Bicim::HEPSI
        .iter()
        .map(|bicim| {
            format!(
                "{:28}{:<10}{}\n",
                "",
                bicim.gosterilen_ad(),
                bicim.aciklama()
            )
        })
        .collect();
    let konum_turleri: String = KonumTuru::HEPSI
        .iter()
        .map(|tur| format!("{:28}{:<8}{}\n", "", tur.gosterilen_ad(), tur.aciklama()))
        .collect();
    let surum = env!("CARGO_PKG_VERSION");
    let kullanim = kullanim();
    let en_kisa_aralik = aralik_yaz(EN_KISA_ARALIK);
    let en_uzun_aralik = aralik_yaz(EN_UZUN_ARALIK);
    let aralik = aralik_yaz(Izleme::default().aralik);
    let esik = ondalik(varsayilan.drain.similarity_threshold);
    let konum = varsayilan.konum_turu.gosterilen_ad();
    let siralama = varsayilan.siralama.gosterilen_ad();
    let bicim = Bicim::Metin.gosterilen_ad();
    let seviye = EN_DUSUK_SEVIYE.ad();
    let seviyeler = seviye_adlari().join(", ");
    let [lang_degiskeni, dil_degiskeni] = DIL_DEGISKENLERI;
    if lang::current() == Lang::En {
        return format!(
            "\
veylog {surum}: summarizes Laravel log files.
Turkish: veylog --lang tr --help (or {lang_degiskeni}=tr)

{kullanim}

Options:
  -e, --threshold <num>   Drain similarity threshold, between 0 and 1. The higher it is, the
                          less patterns merge. [default: {esik}]
      --location <kind>   What identifies where an error happened. [default: {konum}]
{konum_turleri}  -n, --top <num>         Number of rows in the table. [default: {VARSAYILAN_ILK}]
      --all               Show the whole table.
  -s, --sort <key>        Order of the table. [default: {siralama}]
{siralamalar}  -b, --format <name>     Output format. [default: {bicim}]
{bicimler}  -k, --compare <time>    Split the log at this moment and compare the two halves: new,
                          gone and increased patterns. Time: 15:00, 15:00:30, 2026-10-24
                          or \"2026-10-24 15:00\". A time alone means the log's last day.
  -a, --spikes <minutes>  Split the log into slices of this length; find the moments when a
                          pattern's {seviye} and above records jump above their usual
                          level. Slices of 1 to {EN_UZUN_DILIM} minutes; with 1, every
                          minute is counted separately.
      --timezone <zone>   Show times in this time zone: +03:00, -05:30, Z (UTC). Records
                          with a time zone are converted to it, records without one are
                          counted in it. Default: the zone of the first record that has one.

Log format (default: Laravel's; Monolog's JSON lines are always read):
      --pattern <pattern> Format of a line that starts a record: a Monolog format string
                          (\"[%datetime%] %level_name%: %message% %context%\") or a regular
                          expression with named groups: (?P<time>...) and (?P<message>...)
                          are required, (?P<level>...) and (?P<context>...) optional. Other
                          lines continue the previous record.
      --time-format <format>
                          Format of the time, as a PHP date() string (Monolog's dateFormat):
                          \"d.m.Y H:i:s\", \"d/M/Y:H:i:s O\", \"U\". Default:
                          2026-10-24 14:03:11 or ISO 8601 (2026-10-24T14:03:11+03:00).

Filters (the report uses only matching records; line and record counts are the file's):
      --level <level>     This level and above:
                          {seviyeler}.
      --since <time>      Records from this moment (inclusive).
      --until <time>      Records before this moment (exclusive). Times are written as for
                          --compare; a time alone means the log's last day.
      --last <duration>   The end of the log: from this long before the newest record.
                          Duration: 30m, 2h, 1d, 1h30m (s, m, h, d).

Live watching (text format only; cannot be used with --compare):
      --watch             Read the log to its end, write the report, then keep reading new
                          records and redraw the report as they arrive. Time filters
                          (--last 1h) are applied at each redraw relative to the newest
                          record so far: a sliding window. A rotated log is read from the
                          start. On standard input the last report is written when the
                          input ends (tail -f laravel.log | veylog --watch -).
      --interval <dur>    Check the input this often; redraw at most this often. Duration
                          as for --last, ms also works (500ms); between {en_kisa_aralik} and
                          {en_uzun_aralik}.
                          [default: {aralik}]

      --lang <language>   Language of the interface: en, tr. The {lang_degiskeni} (or {dil_degiskeni})
                          environment variable also sets it. [default: en]
  -h, --help              Show this help.
  -V, --version           Show the version.

With - instead of a file the log is read from standard input. Options can come before or
after the file name. A value follows a space or \"=\" (--threshold 0.8, --threshold=0.8);
short options also take it attached (-n20); the decimal separator can be a dot or a comma.
Every option also has a Turkish name (--esik, --ilk, --bicim, --izle...). --top applies to
every format and section; for all rows in JSON use --all. --sort orders the pattern table
and the comparison; spikes are ordered by score. Times of --compare, --since and --until are
in the report's time zone. JSON output is the same in every language.

Exit codes: 0 success, 1 the file could not be read or the output could not be written,
2 usage error (including a comparison point outside the log and an empty filter range).

Examples:
  veylog storage/logs/laravel.log
  veylog --threshold 0.8 --top 20 storage/logs/laravel.log
  veylog --sort newest --all storage/logs/laravel.log
  veylog --format json --all storage/logs/laravel.log | jq '.patterns.rows[0]'
  veylog --format markdown storage/logs/laravel.log > report.md
  veylog --compare 15:00 storage/logs/laravel.log
  veylog --spikes 1 storage/logs/laravel.log
  veylog --level error --last 2h storage/logs/laravel.log
  veylog --timezone +03:00 storage/logs/laravel.log
  veylog --pattern '%datetime% [%level_name%] %message% %context%' storage/logs/app.log
  veylog --time-format 'd.m.Y H:i:s' storage/logs/laravel.log
  veylog --location line storage/logs/laravel.log
  veylog --watch --last 1h storage/logs/laravel.log
  zcat laravel-2026-10-24.log.gz | veylog -
"
        );
    }
    format!(
        "\
veylog {surum}: Laravel log dosyalarını özetler.
English: veylog --lang en --help (or {lang_degiskeni}=en)

{kullanim}

Seçenekler:
  -e, --esik <sayı>       Drain benzerlik eşiği, 0 ile 1 arası. Büyüdükçe kalıplar daha az
                          birleşir. [varsayılan: {esik}]
      --konum <tür>       Bir hatanın yeri neyle belirlensin. [varsayılan: {konum}]
{konum_turleri}  -n, --ilk <sayı>        Tabloda gösterilecek satır sayısı. [varsayılan: {VARSAYILAN_ILK}]
      --hepsi             Tablonun tamamını göster.
  -s, --siralama <ölçüt>  Tablonun sırası. [varsayılan: {siralama}]
{siralamalar}  -b, --bicim <ad>        Çıktının biçimi. [varsayılan: {bicim}]
{bicimler}  -k, --karsilastir <zaman>
                          Logu bu anda ikiye böl ve iki yarıyı karşılaştır: yeni çıkan,
                          kaybolan ve artan kalıplar. Zaman: 15:00, 15:00:30, 2026-10-24
                          ya da \"2026-10-24 15:00\". Yalnızca saat verilirse logun son günü.
  -a, --ani-artis <dakika>
                          Logu bu uzunlukta dilimlere böl; her kalıbın
                          {seviye} ve üstündeki kayıtlarının olağan seviyesinden sıçradığı
                          anları bul. Dilim 1 ile {EN_UZUN_DILIM} dakika arası; 1 verilirse her
                          dakika ayrı sayılır.
      --saat-dilimi <dilim>
                          Zamanları bu saat diliminde göster: +03:00, -05:30, Z (UTC).
                          Saat dilimi yazılı kayıtlar bu dilime çevrilir, yazılmamışlar bu
                          dilimde sayılır. Verilmezse logda saat dilimi yazılı ilk kaydınki.

Log biçimi (verilmezse Laravel'inki; Monolog'un JSON satırları her zaman okunur):
      --desen <desen>     Bir kaydı başlatan satırın biçimi: Monolog'un biçim dizgesi
                          (\"[%datetime%] %level_name%: %message% %context%\") ya da
                          adlandırılmış gruplu bir düzenli ifade: (?P<zaman>...) ve
                          (?P<mesaj>...) gerekli, (?P<seviye>...) ve (?P<baglam>...) isteğe
                          bağlı. Uymayan satırlar bir önceki kaydın devamıdır.
      --zaman-bicimi <biçim>
                          Zamanın biçimi, PHP'nin date() dizgesiyle (Monolog'un
                          dateFormat'ı): \"d.m.Y H:i:s\", \"d/M/Y:H:i:s O\", \"U\". Verilmezse
                          2026-10-24 14:03:11 ya da ISO 8601 (2026-10-24T14:03:11+03:00).

Filtreler (raporu yalnızca uyan kayıtlarla çıkarır; satır ve kayıt sayıları dosyanındır):
      --seviye <seviye>   Bu seviye ve üstündekiler:
                          {seviyeler}.
      --baslangic <zaman> Bu andan (dahil) sonraki kayıtlar.
      --bitis <zaman>     Bu andan (hariç) önceki kayıtlar. Zamanlar --karsilastir gibi
                          yazılır; yalnızca saat verilirse logun son günü.
      --son <süre>        Logun son kısmı: en yeni kayıttan bu kadar öncesinden bu yana.
                          Süre: 30dk, 2sa, 1g, 1sa30dk (sn, dk, sa, g).

Canlı izleme (yalnızca metin biçiminde; --karsilastir ile kullanılamaz):
      --izle              Logu sonuna kadar oku, raporu yaz, sonra eklenen kayıtları
                          okumaya devam et; yeni kayıt geldikçe raporu yeniden çiz. Zaman
                          filtreleri (--son 1sa) her çizimde o ana kadarki en yeni kayda
                          göre uygulanır: kayan pencere. Döndürülen log baştan okunur.
                          Standart girdide girdi bitince son rapor yazılır ve çıkılır
                          (tail -f laravel.log | veylog --izle -).
      --aralik <süre>     Girdiye bu sıklıkla bak; raporu en sık bu kadarda bir çiz. Süre
                          --son gibi, ms de olur (500ms); {en_kisa_aralik} ile {en_uzun_aralik}
                          arası.
                          [varsayılan: {aralik}]

      --dil <dil>         Arayüzün dili: en, tr. {lang_degiskeni} (ya da {dil_degiskeni}) ortam
                          değişkeniyle de seçilir. [varsayılan: en]
  -h, --yardim            Bu yardımı göster.
  -V, --surum             Sürümü göster.

Dosya yerine - verilirse log standart girdiden okunur. Seçenekler dosya adından önce ya da
sonra yazılabilir. Değer boşlukla ya da \"=\" ile verilir (--esik 0,8 ya da --esik=0,8), kısa
seçeneklerde bitişik de yazılabilir (-n20); ondalık ayracı virgül de olabilir, nokta da.
Türkçe karakterli yazımlar da geçerlidir (--eşik, --sıralama önem, --biçim, --karşılaştır,
--ani-artış, --zaman-biçimi, --konum satır); her seçeneğin İngilizce adı da (--threshold,
--top, --format, --watch...). --ilk her biçimde ve her bölümde geçerlidir; JSON'da bütün
satırlar için --hepsi. --siralama kalıp tablosunun ve karşılaştırmanın sırasını belirler;
ani artışlar puanı en yüksek olan başta olacak şekilde dizilir. --karsilastir, --baslangic
ve --bitis zamanları raporun saat dilimindedir. JSON çıktısı her dilde aynıdır.

Çıkış kodları: 0 başarılı, 1 dosya okunamadı ya da çıktı yazılamadı, 2 kullanım hatası
(karşılaştırma noktasının logun dışında kalması ve boş bir filtre aralığı da).

Örnekler:
  veylog --dil tr storage/logs/laravel.log
  veylog --esik 0,8 --ilk 20 storage/logs/laravel.log
  veylog --siralama yeni --hepsi storage/logs/laravel.log
  veylog --bicim json --hepsi storage/logs/laravel.log | jq '.patterns.rows[0]'
  veylog --bicim markdown storage/logs/laravel.log > rapor.md
  veylog --karsilastir 15:00 storage/logs/laravel.log
  veylog --ani-artis 1 storage/logs/laravel.log
  veylog --seviye error --son 2sa storage/logs/laravel.log
  veylog --saat-dilimi +03:00 storage/logs/laravel.log
  veylog --desen '%datetime% [%level_name%] %message% %context%' storage/logs/app.log
  veylog --zaman-bicimi 'd.m.Y H:i:s' storage/logs/laravel.log
  veylog --konum satir storage/logs/laravel.log
  veylog --izle --son 1sa storage/logs/laravel.log
  zcat laravel-2026-10-24.log.gz | veylog -
"
    )
}

/// Seçeneği adına ve bitişik yazılmış değerine ayırır:
/// `--esik=0,8` → (`--esik`, `0,8`), `-n20` ve `-n=20` → (`-n`, `20`), `--hepsi` →
/// (`--hepsi`, yok)
fn ad_ve_bitisik_deger(secenek: &str) -> (&str, Option<&str>) {
    if secenek.starts_with("--") {
        match secenek.split_once('=') {
            Some((ad, deger)) => (ad, Some(deger)),
            None => (secenek, None),
        }
    } else {
        // Kısa seçenek: "-" ve bir harf. Üçüncü karakterden sonrası değerdir; uzun
        // seçeneklerdeki gibi "=" ile de yazılabilir (`-n=20`). Harf sınırı `char_indices` ile
        // bulunur; Türkçe harfler UTF-8'de birden fazla bayttır.
        match secenek.char_indices().nth(2) {
            Some((konum, _)) => {
                let deger = &secenek[konum..];
                (
                    &secenek[..konum],
                    Some(deger.strip_prefix('=').unwrap_or(deger)),
                )
            }
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

/// "şunlardan biri olmalı: a, b, c" ya da "must be one of: a, b, c"
fn biri_olmali(adlar: &[&str]) -> String {
    let adlar = adlar.join(", ");
    match lang::current() {
        Lang::Tr => format!("şunlardan biri olmalı: {adlar}"),
        Lang::En => format!("must be one of: {adlar}"),
    }
}

/// `--esik`: 0 ile 1 arasında bir sayı. Türkçedeki gibi virgülle de yazılabilir.
fn esik_ayristir(ad: &str, deger: &str) -> Result<f64, SecenekHatasi> {
    match deger.replace(',', ".").parse::<f64>() {
        // "NaN" ve "inf" de f64 olarak ayrıştırılır. NaN hiçbir sayıyla karşılaştırılamadığı
        // için aralık denetimi ikisini de reddeder. "-0" aralıktadır (−0 = 0) ama raporda
        // "-0" diye yazılırdı: `abs` işaretini atar.
        Ok(esik) if (0.0..=1.0).contains(&esik) => Ok(esik.abs()),
        _ => Err(gecersiz(
            ad,
            deger,
            lang::pick(
                "0 ile 1 arasında bir sayı olmalı (ör. 0,7)",
                "must be a number between 0 and 1 (e.g. 0.7)",
            ),
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
            lang::pick(
                "1 ya da daha büyük bir tam sayı olmalı (tablonun tamamı için --hepsi)",
                "must be a whole number of at least 1 (for the whole table: --all)",
            ),
        )),
    }
}

/// `--siralama`: sıralamalardan birinin adı.
fn siralama_ayristir(ad: &str, deger: &str) -> Result<Siralama, SecenekHatasi> {
    Siralama::ayristir(&deger_sadelestir(deger)).ok_or_else(|| {
        let adlar: Vec<&str> = Siralama::HEPSI
            .iter()
            .map(|siralama| siralama.gosterilen_ad())
            .collect();
        gecersiz(ad, deger, biri_olmali(&adlar))
    })
}

/// `--konum`: konum türlerinden birinin adı.
fn konum_ayristir(ad: &str, deger: &str) -> Result<KonumTuru, SecenekHatasi> {
    KonumTuru::ayristir(&deger_sadelestir(deger)).ok_or_else(|| {
        let adlar: Vec<&str> = KonumTuru::HEPSI
            .iter()
            .map(|tur| tur.gosterilen_ad())
            .collect();
        gecersiz(ad, deger, biri_olmali(&adlar))
    })
}

/// `--bicim`: biçimlerden birinin adı.
fn bicim_ayristir(ad: &str, deger: &str) -> Result<Bicim, SecenekHatasi> {
    Bicim::ayristir(&deger_sadelestir(deger)).ok_or_else(|| {
        let adlar: Vec<&str> = Bicim::HEPSI
            .iter()
            .map(|bicim| bicim.gosterilen_ad())
            .collect();
        gecersiz(ad, deger, biri_olmali(&adlar))
    })
}

/// `--dil`: dillerden birinin adı.
fn dil_ayristir(ad: &str, deger: &str) -> Result<Lang, SecenekHatasi> {
    Lang::parse(deger).ok_or_else(|| {
        let adlar: Vec<&str> = Lang::ALL.iter().map(|dil| dil.code()).collect();
        gecersiz(ad, deger, biri_olmali(&adlar))
    })
}

/// `--karsilastir`: bir zaman. Logun aralığına uyup uymadığı dosya okununca denetlenir.
fn karsilastir_ayristir(ad: &str, deger: &str) -> Result<TimePoint, SecenekHatasi> {
    TimePoint::parse(deger).ok_or_else(|| {
        // Biçimi doğru ama takvimde olmayan bir tarih de (2026-02-29) buraya düşer
        gecersiz(
            ad,
            deger,
            lang::pick(
                "geçerli bir tarih ya da saat olmalı (ör. 15:00, 15:00:30, 2026-10-24, \"2026-10-24 15:00\")",
                "must be a valid date or time (e.g. 15:00, 15:00:30, 2026-10-24, \"2026-10-24 15:00\")",
            ),
        )
    })
}

/// Seviyelerin komut satırındaki adları, önem sırasıyla: debug, info, ...
fn seviye_adlari() -> Vec<String> {
    Seviye::HEPSI
        .iter()
        .map(|seviye| seviye.ad().to_lowercase())
        .collect()
}

/// `--seviye`: bir seviyenin adı, büyük ya da küçük harfle.
fn seviye_ayristir(ad: &str, deger: &str) -> Result<Seviye, SecenekHatasi> {
    Seviye::HEPSI
        .into_iter()
        .find(|seviye| seviye.ad().eq_ignore_ascii_case(deger))
        .ok_or_else(|| {
            let adlar = seviye_adlari();
            let adlar: Vec<&str> = adlar.iter().map(String::as_str).collect();
            gecersiz(ad, deger, biri_olmali(&adlar))
        })
}

/// `--baslangic` ve `--bitis`: bir zaman, `--karsilastir` gibi.
fn sinir_ayristir(ad: &str, deger: &str) -> Result<TimePoint, SecenekHatasi> {
    TimePoint::parse(deger).ok_or_else(|| {
        gecersiz(
            ad,
            deger,
            lang::pick(
                "geçerli bir tarih ya da saat olmalı (ör. 15:00, 2026-10-24, \"2026-10-24 15:00\")",
                "must be a valid date or time (e.g. 15:00, 2026-10-24, \"2026-10-24 15:00\")",
            ),
        )
    })
}

/// `--saat-dilimi`: bir saat dilimi farkı.
fn saat_dilimi_ayristir(ad: &str, deger: &str) -> Result<TimeZone, SecenekHatasi> {
    TimeZone::parse(deger).ok_or_else(|| {
        gecersiz(
            ad,
            deger,
            lang::pick(
                "bir saat dilimi olmalı (ör. +03:00, -05:30, +0530, Z; en fazla ±23:59)",
                "must be a time zone offset (e.g. +03:00, -05:30, +0530, Z; at most ±23:59)",
            ),
        )
    })
}

/// `--son`: bir süre, saniye olarak.
fn son_ayristir(ad: &str, deger: &str) -> Result<u64, SecenekHatasi> {
    sure_ayristir(&sadelestir(deger)).ok_or_else(|| {
        gecersiz(
            ad,
            deger,
            lang::pick(
                "bir süre olmalı (ör. 30dk, 2sa, 1g, 1sa30dk; birimler: sn, dk, sa, g)",
                "must be a duration (e.g. 30m, 2h, 1d, 1h30m; units: s, m, h, d)",
            ),
        )
    })
}

/// `--aralik`: bir süre, milisaniyeye kadar; `EN_KISA_ARALIK` ile `EN_UZUN_ARALIK` arası.
fn aralik_ayristir(ad: &str, deger: &str) -> Result<Duration, SecenekHatasi> {
    match milisaniye_ayristir(&sadelestir(deger)).map(Duration::from_millis) {
        Some(aralik) if (EN_KISA_ARALIK..=EN_UZUN_ARALIK).contains(&aralik) => Ok(aralik),
        _ => {
            let (en_kisa, en_uzun) = (aralik_yaz(EN_KISA_ARALIK), aralik_yaz(EN_UZUN_ARALIK));
            Err(gecersiz(
                ad,
                deger,
                match lang::current() {
                    Lang::Tr => format!(
                        "{en_kisa} ile {en_uzun} arasında bir süre olmalı (ör. 500ms, 2sn, 1dk; birimler: ms, sn, dk, sa, g)"
                    ),
                    Lang::En => format!(
                        "must be a duration between {en_kisa} and {en_uzun} (e.g. 500ms, 2s, 1m; units: ms, s, m, h, d)"
                    ),
                },
            ))
        }
    }
}

/// `--ani-artis`: dilimlerin uzunluğu, 1 dakika ile 1 gün (1440 dakika) arası bir tam sayı.
fn ani_artis_ayristir(ad: &str, deger: &str) -> Result<u16, SecenekHatasi> {
    match deger.parse::<u16>() {
        Ok(dakika) if (1..=EN_UZUN_DILIM).contains(&dakika) => Ok(dakika),
        _ => Err(gecersiz(
            ad,
            deger,
            match lang::current() {
                Lang::Tr => format!(
                    "1 ile {EN_UZUN_DILIM} arasında bir dakika sayısı olmalı (ör. 1, 5, 60)"
                ),
                Lang::En => format!(
                    "must be a number of minutes between 1 and {EN_UZUN_DILIM} (e.g. 1, 5, 60)"
                ),
            },
        )),
    }
}

fn gecersiz(secenek: &str, deger: &str, beklenen: impl Into<String>) -> SecenekHatasi {
    SecenekHatasi::GecersizDeger {
        secenek: secenek.to_owned(),
        deger: deger.to_owned(),
        beklenen: beklenen.into(),
    }
}

/// Bir değerin adı küçük harfle ve Türkçe harfleri ASCII karşılıklarıyla: `JSON` → `json`,
/// `ÖNEM` → `onem`, `SATIR` → `satir`. Değerler (`--seviye`, `--dil` gibi) büyük harfle de
/// yazılabilir. `İ` ayrıca çevrilir: küçük hali `i` ve bir birleşik noktadır.
fn deger_sadelestir(metin: &str) -> String {
    sadelestir(&metin.replace('İ', "i").to_lowercase())
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

/// Bilinen uzun seçenekler, Türkçe ve İngilizce adlarıyla; seçili dildekiler önce. Yazım
/// hatası yapıldığında öneri bu listeden seçilir: eşit yakınlıkta seçili dildeki ad önerilir.
fn uzun_secenekler() -> impl Iterator<Item = &'static str> {
    let ingilizce_once = lang::current() == Lang::En;
    let ad = |&(tr, en): &(&'static str, &'static str), ingilizce: bool| {
        if ingilizce { en } else { tr }
    };
    OPTION_NAMES
        .iter()
        .map(move |cift| ad(cift, ingilizce_once))
        .chain(
            OPTION_NAMES
                .iter()
                .map(move |cift| ad(cift, !ingilizce_once)),
        )
}

/// Bilinmeyen bir uzun seçeneğe en çok benzeyen geçerli seçenek. Yazım hatası sayılacak kadar
/// yakın değilse None: adı 5 harften kısaysa en fazla 1, uzunsa en fazla 2 harf fark.
fn oneri_bul(secenek: &str) -> Option<&'static str> {
    let ad = sadelestir(secenek.strip_prefix("--")?);
    let sinir = if ad.chars().count() >= 5 { 2 } else { 1 };
    uzun_secenekler()
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
            Ok(Komut::Ozet(secenekler)) => *secenekler,
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
        assert_eq!(secenekler.rapor.drain.similarity_threshold, 0.7);
        assert_eq!(secenekler.rapor.ilk, Some(VARSAYILAN_ILK));
        assert_eq!(secenekler.rapor.siralama, Siralama::Onem);
        assert_eq!(secenekler.rapor.konum_turu, KonumTuru::Metot);
        assert_eq!(secenekler.bicim, Bicim::Metin);
        assert_eq!(secenekler.rapor.karsilastir, None);
        assert_eq!(secenekler.rapor.ani_artis, None);
        assert!(secenekler.rapor.filtre.bos_mu());
    }

    #[test]
    fn filtreler_okunur() {
        let filtre = ozet(&["--seviye", "warning", "a.log"]).rapor.filtre;
        assert_eq!(filtre.seviye, Some(Seviye::Warning));
        assert!(!filtre.sonu_gerekir());
        for yazim in ["ERROR", "Error", "error"] {
            assert_eq!(
                ozet(&["--seviye", yazim, "a.log"]).rapor.filtre.seviye,
                Some(Seviye::Error)
            );
        }
        assert_eq!(
            hata(&["--seviye", "hata", "a.log"]).to_string(),
            "--seviye için geçersiz değer \"hata\": şunlardan biri olmalı: debug, info, notice, warning, error, critical, alert, emergency"
        );

        let filtre = ozet(&["--başlangıç", "15:00", "--bitiş=2026-10-25", "a.log"])
            .rapor
            .filtre;
        assert_eq!(filtre.baslangic, TimePoint::parse("15:00"));
        assert_eq!(filtre.bitis, TimePoint::parse("2026-10-25"));
        assert!(filtre.sonu_gerekir());
        assert!(matches!(
            hata(&["--bitis", "2026-02-30", "a.log"]),
            SecenekHatasi::GecersizDeger { .. }
        ));

        assert_eq!(
            ozet(&["--son", "1sa30dk", "a.log"]).rapor.filtre.son,
            Some(5_400)
        );
        assert_eq!(ozet(&["--son=2g", "a.log"]).rapor.filtre.son, Some(172_800));
        assert_eq!(
            ozet(&["--son", "1gün", "a.log"]).rapor.filtre.son,
            Some(86_400)
        );
        assert_eq!(
            hata(&["--son", "2 saat", "a.log"]).to_string(),
            "--son için geçersiz değer \"2 saat\": bir süre olmalı (ör. 30dk, 2sa, 1g, 1sa30dk; birimler: sn, dk, sa, g)"
        );
    }

    #[test]
    fn saat_dilimi_okunur() {
        assert_eq!(ozet(&["a.log"]).rapor.saat_dilimi, None);
        for (argumanlar, beklenen) in [
            (&["--saat-dilimi", "+03:00", "a.log"][..], "+03:00"),
            // Eksiyle başlayan değer seçenek sanılmaz
            (&["--saat-dilimi", "-05:30", "a.log"], "-05:30"),
            (&["--saat-dilimi=Z", "a.log"], "+00:00"),
            (&["a.log", "--saat-dilimi", "+0530"], "+05:30"),
        ] {
            let dilim = ozet(argumanlar).rapor.saat_dilimi.unwrap();
            assert_eq!(dilim.to_string(), beklenen, "{argumanlar:?}");
        }
        assert_eq!(
            hata(&["--saat-dilimi", "3", "a.log"]).to_string(),
            "--saat-dilimi için geçersiz değer \"3\": bir saat dilimi olmalı (ör. +03:00, -05:30, +0530, Z; en fazla ±23:59)"
        );
        assert_eq!(
            hata(&["--saat-dilimi"]),
            SecenekHatasi::DegerEksik("--saat-dilimi".to_string())
        );
        assert_eq!(
            hata(&["--saat-dilim", "+03:00", "a.log"]),
            SecenekHatasi::BilinmeyenSecenek {
                secenek: "--saat-dilim".to_string(),
                oneri: Some("--saat-dilimi")
            }
        );
    }

    #[test]
    fn son_baska_bir_sinirla_kullanilamaz() {
        assert_eq!(
            hata(&["--son", "1sa", "--baslangic", "15:00", "a.log"]).to_string(),
            "--son ile --baslangic birlikte kullanılamaz"
        );
        assert_eq!(
            hata(&["--bitis", "15:00", "--son", "1sa", "a.log"]),
            SecenekHatasi::Celisen("--son", "--bitis")
        );
        // Seviyeyle birlikte olur
        let filtre = ozet(&["--son", "1sa", "--seviye", "error", "a.log"])
            .rapor
            .filtre;
        assert_eq!(
            (filtre.son, filtre.seviye),
            (Some(3_600), Some(Seviye::Error))
        );
    }

    #[test]
    fn ani_artis_dilimi_okunur() {
        for (argumanlar, beklenen) in [
            (&["--ani-artis", "1", "a.log"][..], 1),
            (&["-a", "10", "a.log"], 10),
            (&["-a5", "a.log"], 5),
            (&["a.log", "--ani-artış=60"], 60),
            (&["-a", "1440", "a.log"], 1_440),
        ] {
            assert_eq!(
                ozet(argumanlar).rapor.ani_artis,
                Some(beklenen),
                "{argumanlar:?}"
            );
        }
        for deger in ["0", "1441", "-1", "1,5", "dakika", "", "70000"] {
            assert_eq!(
                hata(&["--ani-artis", deger, "a.log"]).to_string(),
                format!(
                    "--ani-artis için geçersiz değer \"{deger}\": 1 ile 1440 arasında bir dakika sayısı olmalı (ör. 1, 5, 60)"
                )
            );
        }
        // Değeri unutulursa dosya adı değer sanılır ve sayı olmadığı için reddedilir
        assert!(matches!(
            hata(&["-a", "a.log"]),
            SecenekHatasi::GecersizDeger { .. }
        ));
    }

    #[test]
    fn karsilastirma_noktasi_okunur() {
        let saat_15 = TimePoint::parse("15:00");
        for argumanlar in [
            &["--karsilastir", "15:00", "a.log"][..],
            &["-k", "15:00", "a.log"],
            &["-k15:00", "a.log"],
            &["--karşılaştır=15:00", "a.log"],
        ] {
            assert_eq!(
                ozet(argumanlar).rapor.karsilastir,
                saat_15,
                "{argumanlar:?}"
            );
        }
        assert_eq!(
            ozet(&["--karsilastir", "2026-10-24 15:00", "a.log"])
                .rapor
                .karsilastir,
            TimePoint::parse("2026-10-24T15:00")
        );
        assert_eq!(
            hata(&["--karsilastir", "25:00", "a.log"]).to_string(),
            "--karsilastir için geçersiz değer \"25:00\": geçerli bir tarih ya da saat olmalı (ör. 15:00, 15:00:30, 2026-10-24, \"2026-10-24 15:00\")"
        );
        // 2026 artık yıl değil
        assert!(matches!(
            hata(&["-k", "2026-02-29", "a.log"]),
            SecenekHatasi::GecersizDeger { .. }
        ));
    }

    #[test]
    fn bicim_secilebilir() {
        for (argumanlar, beklenen) in [
            (&["--bicim", "json", "a.log"][..], Bicim::Json),
            (&["-b", "markdown", "a.log"], Bicim::Markdown),
            (&["--biçim=json", "a.log"], Bicim::Json),
            (&["-bmetin", "a.log"], Bicim::Metin),
        ] {
            assert_eq!(ozet(argumanlar).bicim, beklenen, "{argumanlar:?}");
        }
        assert_eq!(
            hata(&["--bicim", "xml", "a.log"]).to_string(),
            "--bicim için geçersiz değer \"xml\": şunlardan biri olmalı: metin, json, markdown"
        );
    }

    #[test]
    fn konum_turu_secilebilir() {
        for (argumanlar, beklenen) in [
            (&["--konum", "satir", "a.log"][..], KonumTuru::Satir),
            (&["--konum=metot", "a.log"], KonumTuru::Metot),
            (&["a.log", "--konum", "satır"], KonumTuru::Satir),
        ] {
            assert_eq!(
                ozet(argumanlar).rapor.konum_turu,
                beklenen,
                "{argumanlar:?}"
            );
        }
        assert_eq!(
            hata(&["--konum", "dosya", "a.log"]).to_string(),
            "--konum için geçersiz değer \"dosya\": şunlardan biri olmalı: metot, satir"
        );
        assert_eq!(
            hata(&["a.log", "--konum"]),
            SecenekHatasi::DegerEksik("--konum".to_string())
        );
    }

    #[test]
    fn izleme_secenekleri() {
        assert_eq!(ozet(&["a.log"]).izleme, None);
        assert_eq!(
            ozet(&["--izle", "a.log"]).izleme,
            Some(Izleme {
                aralik: Duration::from_secs(2)
            })
        );
        for (argumanlar, beklenen) in [
            (&["--izle", "--aralik", "500ms", "a.log"][..], 500),
            (&["--izle", "--aralık=1sn500ms", "a.log"], 1_500),
            (&["a.log", "--aralik", "1dk", "--izle"], 60_000),
            // İkinci yazılan geçerli
            (
                &["--izle", "--aralik", "1sn", "--aralik", "3sn", "a.log"],
                3_000,
            ),
        ] {
            assert_eq!(
                ozet(argumanlar).izleme.map(|izleme| izleme.aralik),
                Some(Duration::from_millis(beklenen)),
                "{argumanlar:?}"
            );
        }
        // Öteki seçeneklerle birlikte
        let secenekler = ozet(&[
            "--izle", "--son", "1sa", "-a", "1", "--seviye", "error", "a.log",
        ]);
        assert!(secenekler.izleme.is_some());
        assert_eq!(secenekler.rapor.filtre.son, Some(3_600));
        assert_eq!(secenekler.rapor.ani_artis, Some(1));
        assert_eq!(
            hata(&["--aralik", "1sn", "a.log"]),
            SecenekHatasi::Gerektirir("--aralik", "--izle")
        );
        // Sayının taşması da aralığın dışıdır
        for deger in ["50ms", "0sn", "iki", "1.5sn", "2sa", "999999999999g"] {
            assert_eq!(
                hata(&["--izle", "--aralik", deger, "a.log"]).to_string(),
                format!(
                    "--aralik için geçersiz değer \"{deger}\": 100 ms ile 1 sa arasında bir süre olmalı (ör. 500ms, 2sn, 1dk; birimler: ms, sn, dk, sa, g)"
                ),
                "{deger}"
            );
        }
        assert_eq!(
            ozet(&["--izle", "--aralik", "1sa", "a.log"]).izleme,
            Some(Izleme {
                aralik: Duration::from_secs(3_600)
            })
        );
        assert_eq!(
            hata(&["--izle", "--bicim", "json", "a.log"]).to_string(),
            "--bicim json ile --izle birlikte kullanılamaz: izleme yalnızca metin biçimindedir"
        );
        assert_eq!(
            hata(&["--izle", "-b", "markdown", "a.log"]).to_string(),
            "-b markdown ile --izle birlikte kullanılamaz: izleme yalnızca metin biçimindedir"
        );
        // Metin biçimi açıkça verilebilir
        assert!(
            ozet(&["--izle", "--bicim", "metin", "a.log"])
                .izleme
                .is_some()
        );
        assert_eq!(
            hata(&["--izle", "--karsilastir", "15:00", "a.log"]).to_string(),
            "--karsilastir ile --izle birlikte kullanılamaz: izlenen log sürekli uzar; karşılaştırma için --izle olmadan çalıştırın"
        );
        assert_eq!(
            hata(&["--izle=evet", "a.log"]),
            SecenekHatasi::DegerAlmaz("--izle".to_string())
        );
        assert_eq!(oneri_bul("--izl"), Some("--izle"));
        assert_eq!(oneri_bul("--aralk"), Some("--aralik"));
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
            &["-e=0.8", "-n=20", "-s=adet", "a.log"],
            // Değerler büyük harfle de
            &["-e", "0,8", "-n", "20", "-s", "ADET", "a.log"],
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
                secenekler.rapor.drain.similarity_threshold, 0.8,
                "{argumanlar:?}"
            );
            assert_eq!(secenekler.rapor.ilk, Some(20), "{argumanlar:?}");
            assert_eq!(secenekler.rapor.siralama, Siralama::Adet, "{argumanlar:?}");
        }
        assert_eq!(
            ozet(&["--siralama", "önem", "a.log"]).rapor.siralama,
            Siralama::Onem
        );
        for (secenekler, beklenen) in [
            (
                &["--sıralama", "ÖNEM"][..],
                (Siralama::Onem, Bicim::Metin, KonumTuru::Metot),
            ),
            (
                &["-s", "Adet", "--bicim", "JSON"],
                (Siralama::Adet, Bicim::Json, KonumTuru::Metot),
            ),
            (
                &["--konum", "SATIR", "-b", "Markdown"],
                (Siralama::Onem, Bicim::Markdown, KonumTuru::Satir),
            ),
            (
                &["--bicim", "METİN", "--konum", "SATIR"],
                (Siralama::Onem, Bicim::Metin, KonumTuru::Satir),
            ),
            (
                &["--format", "TEXT", "--sort", "NEWEST"],
                (Siralama::Yeni, Bicim::Metin, KonumTuru::Metot),
            ),
        ] {
            let secenekler = ozet(&[secenekler, &["a.log"]].concat());
            assert_eq!(
                (
                    secenekler.rapor.siralama,
                    secenekler.bicim,
                    secenekler.rapor.konum_turu
                ),
                beklenen,
                "{secenekler:?}"
            );
        }
    }

    #[test]
    fn ayni_secenek_tekrarlanirsa_sonuncusu_gecerlidir() {
        assert_eq!(ozet(&["--ilk", "5", "--hepsi", "a.log"]).rapor.ilk, None);
        assert_eq!(ozet(&["--hepsi", "--ilk", "5", "a.log"]).rapor.ilk, Some(5));
        assert_eq!(
            ozet(&["-e", "0,5", "-e", "0,9", "a.log"])
                .rapor
                .drain
                .similarity_threshold,
            0.9
        );
    }

    #[test]
    fn cizgiyle_baslayan_dosya_adlari_okunabilir() {
        assert_eq!(ozet(&["--", "--esik"]).dosya, PathBuf::from("--esik"));
        // Tek başına "-" de dosya adı olarak geçer; `main` onu standart girdi sayar
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
    fn eksi_sifir_esik_sifirdir() {
        for deger in ["-0", "-0,0", "0"] {
            let esik = ozet(&["--esik", deger, "a.log"])
                .rapor
                .drain
                .similarity_threshold;
            assert_eq!(esik, 0.0, "{deger}");
            // -0,0 == 0,0 doğrudur; işaret ayrıca denetlenir
            assert!(esik.is_sign_positive(), "{deger}");
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
        assert_eq!(hata(&[""]), SecenekHatasi::BosDosyaAdi);
        assert_eq!(hata(&["", "a.log"]), SecenekHatasi::BosDosyaAdi);
        // Boş ad ikinci sıradaysa da: "fazladan argüman: " diye adsız bir hata olmaz
        assert_eq!(hata(&["a.log", ""]), SecenekHatasi::BosDosyaAdi);
        assert_eq!(hata(&[""]).to_string(), "dosya adı boş olamaz");
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
            ("--karsilastr", Some("--karsilastir")),
            ("--aniartis", Some("--ani-artis")),
            // İngilizce adların yazım hataları da
            ("--treshold", Some("--threshold")),
            ("--watc", Some("--watch")),
            ("--timezon", Some("--timezone")),
            // Tek tireyle yazılan uzun seçenekler kısa seçenek ve değeri sayılmaz
            ("-hepsi", Some("--hepsi")),
            ("-izle", Some("--izle")),
            ("-son", Some("--son")),
            ("-bicim", Some("--bicim")),
            ("-desen", Some("--desen")),
            // Kısa seçeneğin öteki harfi
            ("-v", Some("-V")),
            ("-N", Some("-n")),
            ("-H", Some("-h")),
            // Hiçbir seçeneğe yakın olmayanlar
            ("--quiet", None),
            ("--xyz", None),
            ("-x", None),
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
        // Öneri listesi ile ayrıştırıcı birbirinden kopmasın: Türkçe ve İngilizce adların her
        // biri tanınır
        for secenek in uzun_secenekler() {
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
        assert!(metin.contains(kullanim()));
        assert!(metin.contains("[varsayılan: 0,7]"));
        assert!(metin.contains("[varsayılan: 10]"));
        assert!(metin.contains("[varsayılan: metin]"));
        assert!(metin.contains("-k, --karsilastir <zaman>"));
        assert!(metin.contains("--saat-dilimi <dilim>"));
        // Seviye ve en uzun dilim sabitlerden okunur
        assert!(metin.contains("-a, --ani-artis <dakika>"));
        assert!(metin.contains("WARNING ve üstündeki kayıtlarının"));
        assert!(metin.contains("Dilim 1 ile 1440 dakika arası"));
        // Kısa seçeneğin bitişik değeri ve standart girdi de anlatılır
        assert!(metin.contains("(-n20)"));
        assert!(metin.contains("Dosya yerine - verilirse log standart girdiden okunur."));
        for siralama in Siralama::HEPSI {
            assert!(metin.contains(siralama.ad()), "{}", siralama.ad());
        }
        for bicim in Bicim::HEPSI {
            assert!(metin.contains(bicim.aciklama()), "{}", bicim.ad());
        }
        assert!(metin.contains("--konum <tür>"));
        assert!(metin.contains("[varsayılan: metot]"));
        for tur in KonumTuru::HEPSI {
            assert!(metin.contains(tur.aciklama()), "{}", tur.ad());
        }
    }

    #[test]
    fn yardim_metninin_satirlari_kisa() {
        // Terminalde satırlar bölünmesin: en uzunu 92 karakter; iki dilde de
        for dil in Lang::ALL {
            let _dil = lang::scoped(dil);
            for satir in yardim_metni().lines() {
                assert!(satir.chars().count() <= 92, "{dil:?}: {satir}");
            }
        }
    }

    #[test]
    fn ingilizce_adlar_turkce_adlarla_ayni_sonucu_verir() {
        for (turkce, ingilizce) in [
            (
                &["--esik", "0,8", "--ilk", "5", "--siralama", "adet", "a.log"][..],
                &[
                    "--threshold",
                    "0.8",
                    "--top",
                    "5",
                    "--sort",
                    "count",
                    "a.log",
                ][..],
            ),
            (
                &[
                    "--bicim", "markdown", "--konum", "satir", "--hepsi", "a.log",
                ],
                &[
                    "--format",
                    "markdown",
                    "--location",
                    "line",
                    "--all",
                    "a.log",
                ],
            ),
            (
                &[
                    "--bicim",
                    "metin",
                    "--siralama",
                    "onem",
                    "--konum",
                    "metot",
                    "a.log",
                ],
                &[
                    "--format",
                    "text",
                    "--sort",
                    "severity",
                    "--location",
                    "method",
                    "a.log",
                ],
            ),
            (
                &[
                    "--siralama",
                    "yeni",
                    "--karsilastir",
                    "15:00",
                    "--ani-artis",
                    "5",
                    "a.log",
                ],
                &[
                    "--sort",
                    "newest",
                    "--compare",
                    "15:00",
                    "--spikes",
                    "5",
                    "a.log",
                ],
            ),
            (
                &[
                    "--seviye",
                    "error",
                    "--baslangic",
                    "10:00",
                    "--bitis",
                    "11:00",
                    "a.log",
                ],
                &[
                    "--level", "error", "--since", "10:00", "--until", "11:00", "a.log",
                ],
            ),
            (
                &[
                    "--son",
                    "1sa30dk",
                    "--saat-dilimi",
                    "+03:00",
                    "--izle",
                    "--aralik",
                    "500ms",
                    "a.log",
                ],
                &[
                    "--last",
                    "1h30m",
                    "--timezone",
                    "+03:00",
                    "--watch",
                    "--interval",
                    "500ms",
                    "a.log",
                ],
            ),
            (
                &[
                    "--desen",
                    "[%datetime%] %message%",
                    "--zaman-bicimi",
                    "d.m.Y H:i:s",
                    "--dil",
                    "tr",
                    "a.log",
                ],
                &[
                    "--pattern",
                    "[%datetime%] %message%",
                    "--time-format",
                    "d.m.Y H:i:s",
                    "--lang",
                    "en",
                    "a.log",
                ],
            ),
        ] {
            assert_eq!(
                format!("{:?}", ozet(turkce)),
                format!("{:?}", ozet(ingilizce)),
                "{ingilizce:?}"
            );
        }
        // Değer adları seçeneğin dilinden bağımsız: Türkçe seçeneğe İngilizce değer de olur
        assert_eq!(
            ozet(&["--siralama", "count", "a.log"]).rapor.siralama,
            Siralama::Adet
        );
        assert_eq!(
            ozet(&["--sort", "adet", "a.log"]).rapor.siralama,
            Siralama::Adet
        );
        assert_eq!(ozet(&["-b", "text", "a.log"]).bicim, Bicim::Metin);
        assert_eq!(
            ozet(&["--konum", "line", "a.log"]).rapor.konum_turu,
            KonumTuru::Satir
        );
        // Dil yalnızca denetlenir
        assert!(matches!(ayir(&["--lang=EN", "a.log"]), Ok(Komut::Ozet(_))));
        assert_eq!(
            hata(&["--dil", "de", "a.log"]).to_string(),
            "--dil için geçersiz değer \"de\": şunlardan biri olmalı: tr, en"
        );
        assert_eq!(
            hata(&["--lang"]),
            SecenekHatasi::DegerEksik("--lang".to_owned())
        );
        // Yardım ve sürüm de
        assert!(matches!(ayir(&["--help"]), Ok(Komut::Yardim)));
        assert!(matches!(ayir(&["--version"]), Ok(Komut::Surum)));
    }

    #[test]
    fn dil_argumanlardan_ayristirmadan_once_bulunur() {
        let bul = |argumanlar: &[&str]| {
            let argumanlar: Vec<OsString> = argumanlar.iter().map(OsString::from).collect();
            dil_bul(&argumanlar)
        };
        assert_eq!(bul(&["a.log"]), None);
        assert_eq!(bul(&["--dil", "en", "a.log"]), Some(Lang::En));
        assert_eq!(bul(&["a.log", "--lang=en"]), Some(Lang::En));
        assert_eq!(bul(&["--dıl", "EN"]), Some(Lang::En));
        // Sonuncusu geçerli; tanınmayan değer yok sayılır (ayrıştırıcı hata verir)
        assert_eq!(bul(&["--dil", "en", "--dil", "tr"]), Some(Lang::Tr));
        assert_eq!(bul(&["--dil", "en", "--dil", "de"]), Some(Lang::En));
        assert_eq!(bul(&["--dil", "de"]), None);
        // Değersiz seçenek ve `--`'dan sonrası
        assert_eq!(bul(&["--dil"]), None);
        assert_eq!(bul(&["--", "--dil", "en"]), None);
        // Ayrıştırma hatası seçilen dilde yazılır: hata dilden önce gelse de
        let _dil = lang::scoped(bul(&["--esk", "1", "--lang", "en", "a.log"]).unwrap());
        assert_eq!(
            hata(&["--esk", "1", "--lang", "en", "a.log"]).to_string(),
            "unknown option: --esk (did you mean --esik?)"
        );
    }

    #[test]
    fn hata_mesajlari_ingilizce() {
        let _dil = lang::scoped(Lang::En);
        for (argumanlar, beklenen) in [
            (
                &["--treshold", "a.log"][..],
                "unknown option: --treshold (did you mean --threshold?)",
            ),
            (&["--quiet", "a.log"], "unknown option: --quiet"),
            (&["a.log", "--top"], "--top needs a value"),
            (&["--all=yes", "a.log"], "--all does not take a value"),
            (
                &["--threshold", "1.5", "a.log"],
                "invalid value \"1.5\" for --threshold: must be a number between 0 and 1 (e.g. 0.7)",
            ),
            (
                &["--top", "0", "a.log"],
                "invalid value \"0\" for --top: must be a whole number of at least 1 (for the whole table: --all)",
            ),
            (
                &["--sort", "x", "a.log"],
                "invalid value \"x\" for --sort: must be one of: severity, count, newest",
            ),
            (
                &["--format", "xml", "a.log"],
                "invalid value \"xml\" for --format: must be one of: text, json, markdown",
            ),
            (
                &["--location", "x", "a.log"],
                "invalid value \"x\" for --location: must be one of: method, line",
            ),
            (
                &["--level", "x", "a.log"],
                "invalid value \"x\" for --level: must be one of: debug, info, notice, warning, error, critical, alert, emergency",
            ),
            (
                &["--compare", "25:00", "a.log"],
                "invalid value \"25:00\" for --compare: must be a valid date or time (e.g. 15:00, 15:00:30, 2026-10-24, \"2026-10-24 15:00\")",
            ),
            (
                &["--since", "x", "a.log"],
                "invalid value \"x\" for --since: must be a valid date or time (e.g. 15:00, 2026-10-24, \"2026-10-24 15:00\")",
            ),
            (
                &["--timezone", "3", "a.log"],
                "invalid value \"3\" for --timezone: must be a time zone offset (e.g. +03:00, -05:30, +0530, Z; at most ±23:59)",
            ),
            (
                &["--last", "2 hours", "a.log"],
                "invalid value \"2 hours\" for --last: must be a duration (e.g. 30m, 2h, 1d, 1h30m; units: s, m, h, d)",
            ),
            (
                &["--watch", "--interval", "50ms", "a.log"],
                "invalid value \"50ms\" for --interval: must be a duration between 100 ms and 1 h (e.g. 500ms, 2s, 1m; units: ms, s, m, h, d)",
            ),
            (
                &["--spikes", "0", "a.log"],
                "invalid value \"0\" for --spikes: must be a number of minutes between 1 and 1440 (e.g. 1, 5, 60)",
            ),
            (
                &["--lang", "de", "a.log"],
                "invalid value \"de\" for --lang: must be one of: tr, en",
            ),
            (&[], "no log file given"),
            (
                &["a.log", "b.log"],
                "unexpected argument: b.log (only one log file can be given)",
            ),
            (&[""], "the file name cannot be empty"),
            // Türkçe adla yazılan seçenekler de İngilizce adlarıyla anılır
            (
                &["--son", "1sa", "--baslangic", "15:00", "a.log"],
                "--last and --since cannot be used together",
            ),
            (
                &["--aralik", "1sn", "a.log"],
                "--interval can only be used with --watch",
            ),
            (
                &["--watch", "--format", "json", "a.log"],
                "--format json cannot be used with --watch: watching is only in the text format",
            ),
            (
                &["--izle", "-k", "15:00", "a.log"],
                "--compare cannot be used with --watch: a watched log keeps growing; run without --watch to compare",
            ),
            (
                &["--pattern", "(?P<zaman>x)", "a.log"],
                "invalid value \"(?P<zaman>x)\" for --pattern: there is no (?P<message>...) group; the time and message groups are required",
            ),
            (
                &["--time-format", "H:i", "a.log"],
                "invalid value \"H:i\" for --time-format: the time format has no year, month and day; year (Y), month (m), day (d), hour (H) and minute (i) are required, or a Unix time (U)",
            ),
            (
                &["--time-format", "Y-m-d h:i", "a.log"],
                "invalid value \"Y-m-d h:i\" for --time-format: a 12-hour clock (h, g) cannot be read without an AM/PM marker (A or a)",
            ),
        ] {
            assert_eq!(hata(argumanlar).to_string(), beklenen, "{argumanlar:?}");
        }
        assert_eq!(kullanim(), "Usage: veylog [OPTIONS] <log-file>");
        // Eşit yakınlıkta seçili dildeki ad önerilir: "--sor" hem --son hem --sort'a bir harf
        assert_eq!(oneri_bul("--sor"), Some("--sort"));
        let _turkce = lang::scoped(Lang::Tr);
        assert_eq!(oneri_bul("--sor"), Some("--son"));
    }

    #[test]
    fn ingilizce_yardim_metni() {
        let _dil = lang::scoped(Lang::En);
        let metin = yardim_metni();
        assert!(metin.starts_with("veylog "), "{metin}");
        assert!(metin.contains("Usage: veylog [OPTIONS] <log-file>"));
        // Her seçeneğin İngilizce adı yazılır; Türkçe harf yoktur
        for &(_, ingilizce) in &OPTION_NAMES {
            assert!(metin.contains(&format!("{ingilizce} ")), "{ingilizce}");
        }
        assert!(
            !metin.contains(|harf| "çğıöşüÇĞİÖŞÜ".contains(harf)),
            "{metin}"
        );
        // Varsayılanlar ve değer adları İngilizce; ondalık noktayla
        for parca in [
            "[default: 0.7]",
            "[default: method]",
            "[default: severity]",
            "[default: text]",
            "[default: 2 s]",
            "between 100 ms and\n                          1 h.",
            "WARNING and above",
            "1 to 1440 minutes",
            "VEYLOG_LANG (or VEYLOG_DIL)",
            "[default: en]",
            "\nTurkish: veylog --lang tr --help (or VEYLOG_LANG=tr)\n",
        ] {
            assert!(metin.contains(parca), "{parca}");
        }
        for siralama in Siralama::HEPSI {
            assert!(metin.contains(siralama.ingilizce_ad()), "{siralama:?}");
            assert!(metin.contains(siralama.aciklama()), "{siralama:?}");
        }
        for tur in KonumTuru::HEPSI {
            assert!(metin.contains(tur.aciklama()), "{tur:?}");
        }
        // Türkçe yardımda bütün Türkçe adlar ve İngilizce yardımın yolu
        drop(_dil);
        let metin = yardim_metni();
        for &(turkce, _) in &OPTION_NAMES {
            assert!(metin.contains(&format!("{turkce} ")), "{turkce}");
        }
        assert!(metin.contains("English: veylog --lang en --help (or VEYLOG_LANG=en)\n"));
        assert!(metin.contains("[varsayılan: en]"));
    }

    #[cfg(unix)]
    #[test]
    fn utf8_olmayan_dosya_adi_kabul_edilir() {
        use std::os::unix::ffi::OsStringExt;
        let ad = OsString::from_vec(b"bozuk-\xff.log".to_vec());
        let secenekler = match ayristir([ad.clone()]) {
            Ok(Komut::Ozet(secenekler)) => *secenekler,
            diger => panic!("özet bekleniyordu, gelen: {diger:?}"),
        };
        assert_eq!(secenekler.dosya, PathBuf::from(ad));
    }
}
