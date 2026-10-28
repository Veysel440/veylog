//! Kayıt başlığının biçimi: bir kaydı hangi satırın başlattığı; zamanının, seviyesinin ve
//! mesajının satırın neresinde olduğu.
//!
//! Varsayılan Laravel'in biçimidir: `[2026-10-27 14:03:11] production.ERROR: Mesaj {bağlam}`.
//! Uygulama Monolog'a başka bir satır biçimi verdiyse (`config/logging.php`'de `formatter_with`
//! ile `format`) biçim `--desen` ile tanıtılır. İki yazılışı vardır:
//!
//! - **Monolog'un biçim dizgesi**, `config/logging.php`'deki gibi:
//!   `[%datetime%] %level_name%: %message% %context% %extra%`. Dizge bir düzenli ifadeye
//!   çevrilir: harfler olduğu gibi aranır, `%datetime%` zamanı, `%level_name%` ya da `%level%`
//!   seviyeyi, `%channel%` kanalı yakalar, mesajdan önceki öteki yer tutucular (`%extra.ip%`)
//!   herhangi bir metne uyar. `%message%`'dan sonrası mesajın parçası sayılır: bağlam
//!   Laravel'in biçimindeki gibi mesajın sonundan ayrılır (` {"` ile başlar), `%extra%` ise
//!   mesajda kalır.
//! - **Adlandırılmış grupları olan bir düzenli ifade**: `zaman` ve `mesaj` grupları gerekir,
//!   `seviye`, `baglam` ve `kanal` isteğe bağlıdır (İngilizce adları da olur: `time`,
//!   `message`, `level`, `context`, `channel`). Seviye grubu yoksa her kayıt INFO sayılır.
//!
//! İfade satırın başına bağlanır: satırın başından uymalıdır. Uymayan satırlar bir önceki
//! kaydın devamıdır, Laravel'in biçimindeki gibi.
//!
//! Zaman ISO 8601 benzeri bir biçimde olmalıdır (`2026-10-27 14:03:11`,
//! `2026-10-27T14:03:11.123456+03:00`); başka bir biçimse `--zaman-bicimi` ile verilir
//! (`zaman_bicimi` modülü). Laravel'in satır biçiminde de geçerlidir: yalnızca tarih biçimi
//! değiştirilmiş loglar için `--desen` gerekmez.

use std::borrow::Cow;
use std::fmt;
use std::ops::Range;

use regex::Regex;

use crate::kayit::Seviye;
use crate::zaman;
use crate::zaman_bicimi::ZamanBicimi;

/// `--zaman-bicimi` verilmemiş bir desende `%datetime%`'ın yerine konan ifade: Laravel'in ve
/// Monolog'un varsayılan zaman biçimleri.
const ISO_8601: &str = "[0-9]{4}-[0-9]{2}-[0-9]{2}[ T][0-9]{2}:[0-9]{2}:[0-9]{2}(?:[.][0-9]+)?(?:Z|[+-][0-9]{2}:?[0-9]{2})?";

/// Satırların biçimi.
#[derive(Debug, Clone, Default)]
pub struct SatirBicimi {
    /// `--desen`; None ise Laravel'in biçimi. Kutuda: çoğu zaman yoktur, ayarlar küçük kalsın.
    desen: Option<Box<Desen>>,
    /// `--zaman-bicimi`; None ise ISO 8601 benzeri.
    zaman: Option<ZamanBicimi>,
}

/// Derlenmiş bir desen ve gruplarının numaraları.
#[derive(Debug, Clone)]
struct Desen {
    ifade: Regex,
    zaman: usize,
    seviye: Option<usize>,
    mesaj: usize,
    baglam: Option<usize>,
}

/// Bir kayıt başlığından okunanlar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baslik<'a> {
    /// İç biçimdeki zaman: `2026-10-27 14:03:11`, varsa kesir ve saat dilimiyle. Logda böyle
    /// yazılmışsa satırdan ödünç alınır; `--zaman-bicimi` ile çevrildiyse kayda aittir.
    pub zaman: Cow<'a, str>,
    pub seviye: Seviye,
    /// Mesaj ve bağlam bir arada, baştaki ve sondaki boşluklar atılmış.
    pub govde: &'a str,
    pub mesaj: &'a str,
    /// Mesajdan sonraki bağlam: `{"ip":"203.0.113.24"}`. Yoksa boş.
    pub baglam: &'a str,
}

impl Baslik<'_> {
    /// Başlığın parçalarının satırdaki yerleri. Toplayıcı başlık satırını saklar, kayıt
    /// tamamlanınca başlığı bunlardan kurar: satırı ikinci kez ayrıştırmaz (bir desende
    /// düzenli ifadeyi ikinci kez çalıştırmaz).
    pub fn yeri(&self, satir: &str) -> BaslikYeri {
        BaslikYeri {
            zaman: match &self.zaman {
                Cow::Borrowed(zaman) => Ok(aralik(satir, zaman)),
                Cow::Owned(zaman) => Err(zaman.clone()),
            },
            seviye: self.seviye,
            govde: aralik(satir, self.govde),
            mesaj: aralik(satir, self.mesaj),
            baglam: aralik(satir, self.baglam),
        }
    }
}

/// Bir başlığın parçalarının başlık satırındaki yerleri (`Baslik::yeri`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaslikYeri {
    /// Zaman satırdaysa yeri; `--zaman-bicimi` ile çevrildiyse çevrilmiş hali.
    zaman: Result<Range<usize>, String>,
    seviye: Seviye,
    govde: Range<usize>,
    mesaj: Range<usize>,
    baglam: Range<usize>,
}

impl BaslikYeri {
    /// Başlık, yerleri bulunduğu satırla aynı olan bir satırdan.
    pub fn baslik<'a>(&'a self, satir: &'a str) -> Baslik<'a> {
        Baslik {
            zaman: match &self.zaman {
                Ok(yer) => Cow::Borrowed(&satir[yer.clone()]),
                Err(zaman) => Cow::Borrowed(zaman),
            },
            seviye: self.seviye,
            govde: &satir[self.govde.clone()],
            mesaj: &satir[self.mesaj.clone()],
            baglam: &satir[self.baglam.clone()],
        }
    }
}

/// `parca`nın `satir` içindeki yeri. Başlığın parçaları satırın dilimleridir; boş bir parça
/// (bağlam yoksa `""`) satırın dışından gelebilir, onun yeri önemsizdir.
fn aralik(satir: &str, parca: &str) -> Range<usize> {
    if parca.is_empty() {
        return 0..0;
    }
    let bas = parca.as_ptr() as usize - satir.as_ptr() as usize;
    debug_assert!(bas + parca.len() <= satir.len(), "parça satırın dışında");
    bas..bas + parca.len()
}

/// Desen derlenemedi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesenHatasi {
    /// Düzenli ifade geçersiz; düzenli ifade kütüphanesinin açıklamasıyla.
    Gecersiz(String),
    /// Gerekli bir grup yok: `zaman` ya da `mesaj`.
    GrupYok(&'static str),
    /// Bilinmeyen bir grup adı.
    BilinmeyenGrup(String),
    /// Aynı alan iki grupta: `zaman` ve `time`.
    IkiKez(&'static str),
    /// Monolog'un biçim dizgesinde `%datetime%` yok.
    ZamanYok,
    /// `%datetime%` `%message%`'dan sonra: mesaj satırın geri kalanını alır, zaman
    /// okunamaz.
    ZamanMesajdanSonra,
}

impl fmt::Display for DesenHatasi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DesenHatasi::Gecersiz(aciklama) => {
                write!(f, "düzenli ifade derlenemedi: {aciklama}")
            }
            DesenHatasi::GrupYok(ad) => write!(
                f,
                "(?P<{ad}>...) grubu yok; zaman ve mesaj grupları gerekli"
            ),
            DesenHatasi::BilinmeyenGrup(ad) => write!(
                f,
                "\"{ad}\" grubu bilinmiyor; kullanılabilenler: zaman, seviye, mesaj, baglam, kanal"
            ),
            DesenHatasi::IkiKez(alan) => write!(f, "{alan} için iki grup var"),
            DesenHatasi::ZamanYok => write!(f, "%datetime% yok; zaman gerekli"),
            DesenHatasi::ZamanMesajdanSonra => write!(
                f,
                "%datetime% %message%'dan sonra; mesaj satırın geri kalanını alır, zaman %message%'dan önce olmalı"
            ),
        }
    }
}

impl SatirBicimi {
    /// `desen`: `--desen` (Monolog'un biçim dizgesi ya da düzenli ifade); None ise Laravel'in
    /// biçimi. `zaman`: `--zaman-bicimi`.
    pub fn new(
        desen: Option<&str>,
        zaman: Option<ZamanBicimi>,
    ) -> Result<SatirBicimi, DesenHatasi> {
        let desen = match desen {
            Some(desen) => Some(Box::new(desen_derle(desen, zaman.as_ref())?)),
            None => None,
        };
        Ok(SatirBicimi { desen, zaman })
    }

    /// `--desen` verildi mi?
    pub fn desenli(&self) -> bool {
        self.desen.is_some()
    }

    /// `--zaman-bicimi` verildi mi?
    pub fn zaman_bicimli(&self) -> bool {
        self.zaman.is_some()
    }

    /// Satır bir kaydın başlığıysa başlıktan okunanlar.
    pub fn baslik<'a>(&self, satir: &'a str) -> Option<Baslik<'a>> {
        match &self.desen {
            None => self.laravel_basligi(satir),
            Some(desen) => self.desen_basligi(desen, satir),
        }
    }

    /// Laravel'in biçimi: `[zaman] kanal.SEVİYE: mesaj bağlam`
    fn laravel_basligi<'a>(&self, satir: &'a str) -> Option<Baslik<'a>> {
        let geri = satir.strip_prefix('[')?; // "2026-10-24 14:03:11] production.ERROR: ..."
        let (zaman, kalan) = geri.split_once("] ")?; // "2026-10-24 14:03:11" ve "production.ERROR: ..."
        let zaman = self.zaman_coz(zaman)?;
        let (kanal_seviye, govde) = kalan.split_once(':')?; // "production.ERROR" ve " Mesaj {...}"
        let (_, seviye_metni) = kanal_seviye.rsplit_once('.')?; // "ERROR"
        let seviye = Seviye::ayristir(seviye_metni)?;
        // Bağlam ` {"` ile ayrılır: boş mesajda bağlamın önündeki boşluk kırpılmadan aranır
        let (mesaj, baglam) = mesaji_ayir(govde);
        Some(Baslik {
            zaman,
            seviye,
            govde: govde.trim(),
            mesaj,
            baglam,
        })
    }

    /// `--desen`: grupların yakaladıkları.
    fn desen_basligi<'a>(&self, desen: &Desen, satir: &'a str) -> Option<Baslik<'a>> {
        let yakalanan = desen.ifade.captures(satir)?;
        let zaman = self.zaman_coz(yakalanan.get(desen.zaman)?.as_str().trim())?;
        // Seviye grubu yoksa ya da (isteğe bağlıysa) bir şey yakalamadıysa INFO. Yakaladığı
        // seviye bilinmiyorsa satır bir başlık değildir: Laravel'in biçimindeki gibi.
        let seviye = match desen.seviye.and_then(|grup| yakalanan.get(grup)) {
            Some(seviye) if !seviye.as_str().trim().is_empty() => seviye_oku(seviye.as_str())?,
            _ => Seviye::Info,
        };
        let govde = yakalanan
            .get(desen.mesaj)
            .map_or("", |mesaj| mesaj.as_str());
        let (mesaj, baglam) = match desen.baglam {
            Some(grup) => (
                govde.trim(),
                yakalanan
                    .get(grup)
                    .map_or("", |baglam| baglam.as_str().trim()),
            ),
            None => mesaji_ayir(govde),
        };
        let govde = govde.trim();
        Some(Baslik {
            zaman,
            seviye,
            govde,
            mesaj,
            baglam,
        })
    }

    /// Başlıktaki zamanı iç biçime çevirir; geçersizse None. `--zaman-bicimi` yoksa zaman
    /// zaten iç biçimdedir: satırdan ödünç alınır.
    fn zaman_coz<'a>(&self, metin: &'a str) -> Option<Cow<'a, str>> {
        match &self.zaman {
            Some(bicim) => bicim.coz(metin).map(Cow::Owned),
            None => zaman::kayit_zamani_mi(metin).then_some(Cow::Borrowed(metin)),
        }
    }
}

/// Başlıkta seviyeden sonra gelen kısmı mesaj ve bağlam olarak ikiye ayırır.
///
/// Laravel, mesajın arkasına bağlam bilgisini JSON olarak ekler:
/// `Kullanıcı 381 giriş yaptı {"ip":"203.0.113.24"}`
/// → `Kullanıcı 381 giriş yaptı` ve `{"ip":"203.0.113.24"}`
///
/// Bağlam ` {"` ile başlar; ondan öncesi mesajdır. Baştaki ve sondaki boşluklar atılır.
pub fn mesaji_ayir(metin: &str) -> (&str, &str) {
    let (mesaj, baglam) = match metin.find(" {\"") {
        Some(konum) => metin.split_at(konum),
        None => (metin, ""),
    };
    (mesaj.trim(), baglam.trim())
}

/// Desenle okunan bir seviye: Laravel'in adları (büyük küçük harf fark etmez), başka
/// araçların kısaltmaları ve Monolog'un sayıları (`%level%`).
fn seviye_oku(metin: &str) -> Option<Seviye> {
    const DIGER: [(&str, Seviye); 14] = [
        ("WARN", Seviye::Warning),
        ("ERR", Seviye::Error),
        ("CRIT", Seviye::Critical),
        ("EMERG", Seviye::Emergency),
        ("FATAL", Seviye::Critical),
        ("TRACE", Seviye::Debug),
        ("100", Seviye::Debug),
        ("200", Seviye::Info),
        ("250", Seviye::Notice),
        ("300", Seviye::Warning),
        ("400", Seviye::Error),
        ("500", Seviye::Critical),
        ("550", Seviye::Alert),
        ("600", Seviye::Emergency),
    ];
    let metin = metin.trim();
    Seviye::HEPSI
        .into_iter()
        .map(|seviye| (seviye.ad(), seviye))
        .chain(DIGER)
        .find(|(ad, _)| ad.eq_ignore_ascii_case(metin))
        .map(|(_, seviye)| seviye)
}

/// `--desen`'i derler: Monolog'un biçim dizgesiyse (`%message%` içeriyorsa) önce düzenli
/// ifadeye çevrilir.
fn desen_derle(desen: &str, zaman: Option<&ZamanBicimi>) -> Result<Desen, DesenHatasi> {
    let ifade = if desen.contains("%message%") {
        monolog_bicimi(desen, zaman)?
    } else {
        // Önce kullanıcının yazdığı haliyle derlenir: hata, içine konduğu `^(?:...)`'ya göre
        // değil, onun ifadesine göre anlatılsın
        Regex::new(desen).map_err(derleme_hatasi)?;
        desen.to_string()
    };
    // Satırın başına bağlanır; `(?:...)` kullanıcının `|` ile ayırdığı seçeneklerin hepsini
    // kapsar
    let ifade = Regex::new(&format!("^(?:{ifade})")).map_err(derleme_hatasi)?;
    let (mut zaman_grubu, mut seviye, mut mesaj, mut baglam) = (None, None, None, None);
    for (sira, ad) in ifade.capture_names().enumerate() {
        let Some(ad) = ad else {
            continue;
        };
        let (alan, yer) = match ad {
            "zaman" | "time" => ("zaman", &mut zaman_grubu),
            "seviye" | "level" => ("seviye", &mut seviye),
            "mesaj" | "message" => ("mesaj", &mut mesaj),
            "baglam" | "context" => ("baglam", &mut baglam),
            // Kanal okunur ama kullanılmaz
            "kanal" | "channel" => continue,
            _ => return Err(DesenHatasi::BilinmeyenGrup(ad.to_string())),
        };
        if yer.replace(sira).is_some() {
            return Err(DesenHatasi::IkiKez(alan));
        }
    }
    Ok(Desen {
        zaman: zaman_grubu.ok_or(DesenHatasi::GrupYok("zaman"))?,
        seviye,
        mesaj: mesaj.ok_or(DesenHatasi::GrupYok("mesaj"))?,
        baglam,
        ifade,
    })
}

/// Düzenli ifade kütüphanesinin hatası tek satıra indirilir. Sözdizimi hatası birkaç satırdır
/// (ifade, hatanın yerini gösteren `^` ve `error: unclosed group`); asıl açıklama sonuncusudur.
fn derleme_hatasi(hata: regex::Error) -> DesenHatasi {
    let metin = hata.to_string();
    let aciklama = match metin.rsplit_once("error: ") {
        Some((_, aciklama)) => aciklama.trim().to_string(),
        None => metin.split_whitespace().collect::<Vec<_>>().join(" "),
    };
    DesenHatasi::Gecersiz(aciklama)
}

/// Monolog'un biçim dizgesini düzenli ifadeye çevirir. `%message%`'dan sonrası atılır: mesaj
/// satırın geri kalanını alır.
fn monolog_bicimi(bicim: &str, zaman: Option<&ZamanBicimi>) -> Result<String, DesenHatasi> {
    let (once, sonra) = bicim
        .split_once("%message%")
        .expect("Monolog biçimi %message% içerir");
    if !once.contains("%datetime%") {
        return Err(if sonra.contains("%datetime%") {
            DesenHatasi::ZamanMesajdanSonra
        } else {
            DesenHatasi::ZamanYok
        });
    }
    let mut ifade = String::new();
    // Aynı yer tutucu iki kez geçerse grup bir kez açılır; düzenli ifadede iki grup aynı adı
    // taşıyamaz
    let mut gruplar: Vec<&str> = Vec::new();
    let mut kalan = once;
    while let Some(bas) = kalan.find('%') {
        ifade.push_str(&regex::escape(&kalan[..bas]));
        let geri = &kalan[bas + 1..];
        match geri.find('%') {
            Some(son) if yer_tutucu_mu(&geri[..son]) => {
                let ad = &geri[..son];
                let (grup, govde) = match ad {
                    "datetime" => (
                        "zaman",
                        zaman.map_or(ISO_8601.to_string(), ZamanBicimi::duzenli_ifade),
                    ),
                    "level_name" => ("seviye", "[A-Za-z]+".to_string()),
                    "level" => ("seviye", "[0-9]+".to_string()),
                    "channel" => ("kanal", ".*?".to_string()),
                    // Bağlam, ek alanlar ve bilinmeyen yer tutucular: herhangi bir metin
                    _ => ("", ".*?".to_string()),
                };
                if grup.is_empty() || gruplar.contains(&grup) {
                    ifade.push_str(&format!("(?:{govde})"));
                } else {
                    gruplar.push(grup);
                    ifade.push_str(&format!("(?P<{grup}>{govde})"));
                }
                kalan = &geri[son + 1..];
            }
            // Yer tutucu olmayan `%` olduğu gibi aranır
            _ => {
                ifade.push('%');
                kalan = geri;
            }
        }
    }
    ifade.push_str(&regex::escape(kalan));
    ifade.push_str("(?P<mesaj>.*)");
    Ok(ifade)
}

/// Monolog'un yer tutucu adı mı: `datetime`, `extra.ip`, `context.user_id`
fn yer_tutucu_mu(ad: &str) -> bool {
    !ad.is_empty()
        && ad
            .bytes()
            .all(|bayt| bayt.is_ascii_alphanumeric() || bayt == b'_' || bayt == b'.')
}

#[cfg(test)]
mod testler {
    use super::*;

    fn bicim(desen: &str) -> SatirBicimi {
        SatirBicimi::new(Some(desen), None).unwrap()
    }

    fn zamanli(desen: Option<&str>, zaman: &str) -> SatirBicimi {
        SatirBicimi::new(desen, Some(ZamanBicimi::derle(zaman).unwrap())).unwrap()
    }

    /// Başlığın alanları kısaca: (zaman, seviye, mesaj, bağlam)
    fn alanlar(baslik: Option<Baslik<'_>>) -> Option<(String, Seviye, &str, &str)> {
        baslik.map(|baslik| {
            (
                baslik.zaman.into_owned(),
                baslik.seviye,
                baslik.mesaj,
                baslik.baglam,
            )
        })
    }

    #[test]
    fn laravelin_bicimi() {
        let laravel = SatirBicimi::default();
        assert_eq!(
            laravel.baslik(
                r#"[2026-10-27 14:03:11] production.ERROR: Ödeme alınamadı {"siparis":5} "#
            ),
            Some(Baslik {
                zaman: Cow::Borrowed("2026-10-27 14:03:11"),
                seviye: Seviye::Error,
                govde: r#"Ödeme alınamadı {"siparis":5}"#,
                mesaj: "Ödeme alınamadı",
                baglam: r#"{"siparis":5}"#,
            })
        );
        for satir in [
            "[2026-10-27 14:03:11] production.HATA: x",
            "[27.10.2026 14:03:11] production.ERROR: x",
            "2026-10-27 14:03:11 production.ERROR: x",
            "[2026-10-27 14:03:11] ERROR: x",
        ] {
            assert_eq!(laravel.baslik(satir), None, "{satir}");
        }
        // Yalnızca zaman biçimi değiştirilmiş Laravel logu
        let turkce = zamanli(None, "d.m.Y H:i:s");
        assert_eq!(
            alanlar(turkce.baslik("[27.10.2026 14:03:11] production.ERROR: x")),
            Some(("2026-10-27 14:03:11".to_string(), Seviye::Error, "x", ""))
        );
        // Zaman biçimi verilince yalnızca o biçim okunur
        assert_eq!(
            turkce.baslik("[2026-10-27 14:03:11] production.ERROR: x"),
            None
        );
    }

    #[test]
    fn monologun_bicim_dizgesi() {
        // Seviye kanalsız, köşeli parantezde
        let desen = bicim("%datetime% [%level_name%] %message% %context% %extra%\n");
        assert_eq!(
            alanlar(desen.baslik(
                r#"2026-10-27T14:03:11+03:00 [WARNING] Disk dolmak üzere {"yuzde":91} []"#
            )),
            Some((
                "2026-10-27T14:03:11+03:00".to_string(),
                Seviye::Warning,
                "Disk dolmak üzere",
                r#"{"yuzde":91} []"#
            ))
        );
        // Mesajdan önce ek alanlar; kanal noktalı olabilir
        let desen = bicim("[%datetime%] [%extra.istek%] %channel%.%level_name%: %message%");
        assert_eq!(
            alanlar(desen.baslik("[2026-10-27 14:03:11] [ab12] odeme.servis.ERROR: Zaman aşımı")),
            Some((
                "2026-10-27 14:03:11".to_string(),
                Seviye::Error,
                "Zaman aşımı",
                ""
            ))
        );
        // Boş ek alan: Monolog yer tutucuyu siler, boşluklar kalır
        assert!(
            desen
                .baslik("[2026-10-27 14:03:11] [] odeme.ERROR: x")
                .is_some()
        );
        assert_eq!(desen.baslik("[2026-10-27 14:03:11] [ab12] x"), None);
        // Monolog'un sayısal seviyesi; yazılı `%` olduğu gibi aranır
        let desen = bicim("%datetime% %level% %% %message%");
        assert_eq!(
            alanlar(desen.baslik("2026-10-27 14:03:11 550 %% Sunucu çöktü")),
            Some((
                "2026-10-27 14:03:11".to_string(),
                Seviye::Alert,
                "Sunucu çöktü",
                ""
            ))
        );
        assert_eq!(desen.baslik("2026-10-27 14:03:11 999 %% x"), None);
        // Zaman biçimiyle; düzenli ifadenin özel karakterleri (`.`, `(`) harf olarak aranır
        let desen = zamanli(Some("(%datetime%) %level_name%. %message%"), "d.m.Y H:i");
        assert_eq!(
            alanlar(desen.baslik("(27.10.2026 14:03) INFO. Başladı")),
            Some((
                "2026-10-27 14:03:00".to_string(),
                Seviye::Info,
                "Başladı",
                ""
            ))
        );
        assert_eq!(desen.baslik("(27x10x2026 14:03) INFO. Başladı"), None);
        // Aynı yer tutucu iki kez
        let desen = bicim("%datetime% %datetime% %level_name% %level_name% %message%");
        assert!(
            desen
                .baslik("2026-10-27 14:03:11 2026-10-27 14:03:11 INFO INFO x")
                .is_some()
        );
    }

    #[test]
    fn monolog_dizgesinin_duzenli_ifadesi() {
        // Belgelerdeki örnek: harfler kaçışlanır, sondaki satır sonu mesaja katılır
        let bicim = zamanli(
            Some("%datetime% [%level_name%] %channel%: %message% %context% %extra%\n"),
            "d.m.Y H:i:s",
        );
        assert_eq!(
            bicim.desen.as_ref().unwrap().ifade.as_str(),
            r"^(?:(?P<zaman>[0-9]{2}\.[0-9]{2}\.[0-9]{4} [0-9]{2}:[0-9]{2}:[0-9]{2}) \[(?P<seviye>[A-Za-z]+)\] (?P<kanal>.*?): (?P<mesaj>.*))"
        );
    }

    #[test]
    fn duzenli_ifade() {
        let desen = bicim(r"(?P<zaman>\S+ \S+) (?P<seviye>\w+) +(?P<mesaj>.*)");
        assert_eq!(
            alanlar(desen.baslik("2026-10-27 14:03:11 warn   Kuyruk dolu")),
            Some((
                "2026-10-27 14:03:11".to_string(),
                Seviye::Warning,
                "Kuyruk dolu",
                ""
            ))
        );
        // Satırın başına bağlıdır
        assert_eq!(
            desen.baslik("#3 2026-10-27 14:03:11 warn Kuyruk dolu"),
            None
        );
        // Bilinmeyen seviye: başlık değil
        assert_eq!(desen.baslik("2026-10-27 14:03:11 sorun x"), None);
        // Geçersiz zaman: başlık değil
        assert_eq!(desen.baslik("2026-10-27 25:03:11 warn x"), None);
        // Seviye grubu yoksa INFO; İngilizce grup adları; ayrı bağlam grubu
        let desen = bicim(r"(?P<time>[^|]+)\|(?P<message>[^|]*)\|(?P<context>.*)");
        assert_eq!(
            alanlar(desen.baslik(r#"2026-10-27 14:03:11 | Giriş {"a":1} | {"ip":"x"}"#)),
            Some((
                "2026-10-27 14:03:11".to_string(),
                Seviye::Info,
                r#"Giriş {"a":1}"#,
                r#"{"ip":"x"}"#
            ))
        );
        // Seviye grubu boş yakaladıysa da INFO
        let desen = bicim(r"(?P<zaman>\S+ \S+) (?P<seviye>[A-Z]*) ?(?P<mesaj>.*)");
        assert_eq!(
            desen
                .baslik("2026-10-27 14:03:11 x")
                .map(|baslik| baslik.seviye),
            Some(Seviye::Info)
        );
        // `|` ile ayrılan seçeneklerin hepsi satırın başına bağlanır
        let desen = bicim(r"x|(?P<zaman>\S+ \S+) (?P<mesaj>.*)");
        assert_eq!(desen.baslik("a 2026-10-27 14:03:11 m"), None);
        assert!(desen.baslik("2026-10-27 14:03:11 m").is_some());
    }

    #[test]
    fn seviye_yazilislari() {
        for (metin, seviye) in [
            ("error", Seviye::Error),
            ("Warning", Seviye::Warning),
            ("WARN", Seviye::Warning),
            ("err", Seviye::Error),
            ("crit", Seviye::Critical),
            ("emerg", Seviye::Emergency),
            ("FATAL", Seviye::Critical),
            ("trace", Seviye::Debug),
            ("100", Seviye::Debug),
            ("250", Seviye::Notice),
            ("400", Seviye::Error),
            (" 600 ", Seviye::Emergency),
        ] {
            assert_eq!(seviye_oku(metin), Some(seviye), "{metin}");
        }
        for metin in ["", "hata", "401", "WARNINGS"] {
            assert_eq!(seviye_oku(metin), None, "{metin}");
        }
    }

    #[test]
    fn desen_hatalari() {
        let hata = |desen: &str| SatirBicimi::new(Some(desen), None).unwrap_err();
        assert_eq!(hata(r"(?P<mesaj>.*)"), DesenHatasi::GrupYok("zaman"));
        assert_eq!(hata(r"(?P<zaman>\S+)"), DesenHatasi::GrupYok("mesaj"));
        assert_eq!(
            hata(r"(?P<zaman>\S+) (?P<mesage>.*)"),
            DesenHatasi::BilinmeyenGrup("mesage".to_string())
        );
        assert_eq!(
            hata(r"(?P<zaman>\S+) (?P<time>\S+) (?P<mesaj>.*)"),
            DesenHatasi::IkiKez("zaman")
        );
        assert!(matches!(hata(r"(?P<zaman>\S+"), DesenHatasi::Gecersiz(_)));
        assert_eq!(hata("[%level_name%] %message%"), DesenHatasi::ZamanYok);
        assert_eq!(
            hata("%level_name%: %message% (%datetime%)"),
            DesenHatasi::ZamanMesajdanSonra
        );
        // Kütüphanenin birkaç satırlık açıklaması tek satıra iner
        assert_eq!(
            hata(r"(?P<zaman>\S+"),
            DesenHatasi::Gecersiz("unclosed group".to_string())
        );
        assert_eq!(
            hata(r"(?P<zaman>\S+").to_string(),
            "düzenli ifade derlenemedi: unclosed group"
        );
        assert_eq!(
            DesenHatasi::GrupYok("zaman").to_string(),
            "(?P<zaman>...) grubu yok; zaman ve mesaj grupları gerekli"
        );
    }
}
