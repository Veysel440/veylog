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

use crate::datetime;
use crate::json_okuyucu::{self, Onek};
use crate::kayit::Seviye;
use crate::lang::{self, Lang};
use crate::time_format::TimeFormat;

/// `--zaman-bicimi` verilmemiş bir desende `%datetime%`'ın yerine konan ifade: Laravel'in ve
/// Monolog'un varsayılan zaman biçimleri.
const ISO_8601: &str = "[0-9]{4}-[0-9]{2}-[0-9]{2}[ T][0-9]{2}:[0-9]{2}:[0-9]{2}(?:[.][0-9]+)?(?:Z|[+-][0-9]{2}:?[0-9]{2})?";

/// Satırların biçimi.
#[derive(Debug, Clone, Default)]
pub struct SatirBicimi {
    /// `--desen`; None ise Laravel'in biçimi. Kutuda: çoğu zaman yoktur, ayarlar küçük kalsın.
    desen: Option<Box<Desen>>,
    /// `--zaman-bicimi`; None ise ISO 8601 benzeri.
    zaman: Option<TimeFormat>,
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
/// (bağlam yoksa `""`) satırın dışından gelebilir, onun yeri önemsizdir. Satırın dışından
/// gelen boş olmayan bir parça bir programlama hatasıdır: testlerde panikler, sürümde boş
/// bir yer verir (yanlış bir dilim ya da panik yerine).
fn aralik(satir: &str, parca: &str) -> Range<usize> {
    if parca.is_empty() {
        return 0..0;
    }
    let bas = (parca.as_ptr() as usize).checked_sub(satir.as_ptr() as usize);
    match bas {
        Some(bas) if bas + parca.len() <= satir.len() => bas..bas + parca.len(),
        _ => {
            debug_assert!(false, "parça satırın dışında");
            0..0
        }
    }
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
        if lang::current() == Lang::En {
            return match self {
                DesenHatasi::Gecersiz(aciklama) => {
                    write!(f, "the regular expression cannot be compiled: {aciklama}")
                }
                DesenHatasi::GrupYok(ad) => write!(
                    f,
                    "there is no (?P<{}>...) group; the time and message groups are required",
                    grup_ingilizce(ad)
                ),
                DesenHatasi::BilinmeyenGrup(ad) => write!(
                    f,
                    "unknown group \"{ad}\"; available: time, level, message, context, channel"
                ),
                DesenHatasi::IkiKez(alan) => {
                    write!(f, "two groups for {}", grup_ingilizce(alan))
                }
                DesenHatasi::ZamanYok => write!(f, "no %datetime%; the time is required"),
                DesenHatasi::ZamanMesajdanSonra => write!(
                    f,
                    "%datetime% comes after %message%; the message takes the rest of the line, the time must come before %message%"
                ),
            };
        }
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

/// Bir grubun İngilizce adı: `zaman` → `time`. Gruplar iki adla da yazılabilir.
fn grup_ingilizce(ad: &str) -> &str {
    match ad {
        "zaman" => "time",
        "seviye" => "level",
        "mesaj" => "message",
        "baglam" => "context",
        "kanal" => "channel",
        diger => diger,
    }
}

impl SatirBicimi {
    /// `desen`: `--desen` (Monolog'un biçim dizgesi ya da düzenli ifade); None ise Laravel'in
    /// biçimi. `zaman`: `--zaman-bicimi`.
    pub fn new(desen: Option<&str>, zaman: Option<TimeFormat>) -> Result<SatirBicimi, DesenHatasi> {
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
        match self.tani(satir) {
            Tanima::Baslik(baslik) => Some(baslik),
            Tanima::TaninmayanSeviye | Tanima::Degil => None,
        }
    }

    /// Satır bir kaydın başlığı mı; değilse, desene ve zamana uyup yalnızca seviyesi mi
    /// tanınmadı?
    pub fn tani<'a>(&self, satir: &'a str) -> Tanima<'a> {
        match &self.desen {
            None => self
                .laravel_basligi(satir)
                .map_or(Tanima::Degil, Tanima::Baslik),
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
    fn desen_basligi<'a>(&self, desen: &Desen, satir: &'a str) -> Tanima<'a> {
        let Some(yakalanan) = desen.ifade.captures(satir) else {
            return Tanima::Degil;
        };
        let Some(zaman) = yakalanan
            .get(desen.zaman)
            .and_then(|zaman| self.zaman_coz(zaman.as_str().trim()))
        else {
            return Tanima::Degil;
        };
        // Seviye grubu yoksa ya da (isteğe bağlıysa) bir şey yakalamadıysa INFO. Yakaladığı
        // seviye bilinmiyorsa satır bir başlık değildir: Laravel'in biçimindeki gibi. Ama
        // desene ve zamana uyduğu için büyük olasılıkla bir kayıttır: sayılır, uyarılır.
        let seviye = match desen.seviye.and_then(|grup| yakalanan.get(grup)) {
            Some(seviye) if !seviye.as_str().trim().is_empty() => {
                match seviye_oku(seviye.as_str()) {
                    Some(seviye) => seviye,
                    None => return Tanima::TaninmayanSeviye,
                }
            }
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
        Tanima::Baslik(Baslik {
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
            Some(bicim) => bicim.normalize(metin).map(Cow::Owned),
            None => datetime::is_record_time(metin).then_some(Cow::Borrowed(metin)),
        }
    }
}

/// Bir satırın tanınması (`SatirBicimi::tani`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tanima<'a> {
    /// Bir kaydın başlığı.
    Baslik(Baslik<'a>),
    /// Desene ve zamana uyuyor ama seviyesi tanınmadı: kayıt sayılmaz.
    TaninmayanSeviye,
    /// Başlık değil: bir kaydın devamı ya da yetim bir satır.
    Degil,
}

/// Başlıkta seviyeden sonra gelen kısmı mesaj ve bağlam olarak ikiye ayırır.
///
/// Laravel, mesajın arkasına bağlam bilgisini JSON olarak ekler:
/// `Kullanıcı 381 giriş yaptı {"ip":"203.0.113.24"}`
/// → `Kullanıcı 381 giriş yaptı` ve `{"ip":"203.0.113.24"}`
///
/// Bağlam ` {"` ile başlar ve geçerli bir JSON nesnesidir. Mesajın kendisi de ` {"` içerebilir
/// (`Ödeme {"tutar":50} onaylandı`); bu yüzden ` {"`'lerin her biri soldan denenir ve
/// sonrası bağlama en çok benzeyen ilki seçilir (`baglam_mi`). Ardından bir kelime gelen
/// geçerli bir JSON mesajın parçasıdır; hiçbir aday kalmazsa mesaj bölünmez. En fazla
/// `BAGLAM_ADAYI` aday denenir: her deneme adayın JSON'unu baştan okur, iç içe yüzlerce aday
/// taşıyan bir satır böylece yavaşlatamaz.
///
/// Monolog'un `%context% %extra%` biçiminde boş bağlam ve ek `[]` diye yazılır (Laravel'in
/// varsayılan biçimlendiricisi onları hiç yazmaz, `formatter_with` ile kurulan yazar):
/// `Kuyruk dolu [] []`. Mesajın sonundaki bu `[]`'lar atılır; yoksa aynı mesaj bağlamlı ve
/// bağlamsız halleriyle iki kalıba bölünürdü. Baştaki ve sondaki boşluklar atılır.
pub fn mesaji_ayir(metin: &str) -> (&str, &str) {
    // En çok benzeyen ilk aday: benzerlik ve konumu
    let mut secilen: Option<(Benzerlik, usize)> = None;
    for (konum, _) in metin.match_indices(" {\"").take(BAGLAM_ADAYI) {
        let benzerlik = baglam_mi(&metin[konum + 1..]);
        if benzerlik > secilen.map_or(Benzerlik::Degil, |(onceki, _)| onceki) {
            secilen = Some((benzerlik, konum));
        }
        if benzerlik == Benzerlik::Kesin {
            break;
        }
    }
    let (mesaj, baglam) = match secilen.map(|(_, konum)| konum) {
        Some(konum) => metin.split_at(konum),
        None => (metin, ""),
    };
    // `[]`'ların önündeki boşluk mesaj boşken de durur: sağdan atılır, sonra baştan kırpılır
    (bos_ekleri_at(mesaj.trim_end()).trim_start(), baglam.trim())
}

/// `mesaji_ayir`'ın denediği en fazla aday sayısı.
const BAGLAM_ADAYI: usize = 32;

/// `{` ile başlayan bir metnin Monolog'un bağlamına benzerliği; azdan çoğa sıralı.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Benzerlik {
    /// Ardından bir kelime gelen geçerli bir JSON: mesajın parçası
    /// (`Ödeme {"tutar":50} onaylandı`).
    Degil,
    /// Geçersiz bir JSON. Monolog geçerli JSON yazar, ama bozuk bir satırda ya da başka bir
    /// yazıcının satırında başka aday yoksa bağlam yine buradan başlar (0.1'deki gibi).
    Zayif,
    /// Tam bir JSON nesnesi ama ardından JSON olmayan, harf ya da rakamla başlamayan bir şey
    /// (`--desen`'deki bir biçimde bağlamdan sonra gelen alanlar: `{...} [production]`).
    Olasi,
    /// Tam bir JSON nesnesi ve ardından hiçbir şey ya da ek (`[]`, bir nesne); ya da satır
    /// sonuna kadar yarım kalan bir JSON (bağlamın yığın izi sonraki satırlarda sürer).
    Kesin,
}

fn baglam_mi(metin: &str) -> Benzerlik {
    match json_okuyucu::onek(metin) {
        Onek::Yarim => Benzerlik::Kesin,
        Onek::Gecersiz => Benzerlik::Zayif,
        Onek::Tam(son) => {
            let ek = metin[son..].trim();
            let ek_json = ek.starts_with(['{', '['])
                && match json_okuyucu::onek(ek) {
                    Onek::Tam(son) => ek[son..].trim().is_empty(),
                    Onek::Yarim => true,
                    Onek::Gecersiz => false,
                };
            if ek.is_empty() || ek_json {
                Benzerlik::Kesin
            } else if ek.starts_with(char::is_alphanumeric) {
                Benzerlik::Degil
            } else {
                Benzerlik::Olasi
            }
        }
    }
}

/// Mesajın sonundaki boş bağlam ve ek: en fazla iki ` []`. Mesaj boşsa ilk `[]`'ın önündeki
/// boşluk metnin başındadır (`" [] []"`); bu yüzden metnin başı kırpılmadan verilir.
fn bos_ekleri_at(mut mesaj: &str) -> &str {
    for _ in 0..2 {
        match mesaj.strip_suffix(" []") {
            Some(kalan) => mesaj = kalan.trim_end(),
            None => break,
        }
    }
    mesaj
}

/// Desenle okunan bir seviye. Büyük küçük harf fark etmez. Tanınanlar:
///
/// - Laravel'in adları: `DEBUG`, `INFO`, `NOTICE`, `WARNING`, `ERROR`, `CRITICAL`, `ALERT`,
///   `EMERGENCY`;
/// - başka araçların adları ve kısaltmaları: log4j ve NLog'un `TRACE`, `WARN`, `FATAL`'ı,
///   Serilog'un `VRB`, `DBG`, `INF`, `WRN`, `ERR`, `FTL`'si, syslog'un `CRIT`, `EMERG`'i,
///   .NET'in `Information`, `fail`, `dbug`, `trce`'si, Java'nın `SEVERE`, `CONFIG`, `FINE`'ı,
///   Go'nun `PANIC`'i;
/// - bu adların en az üç harflik başları: Monolog'un `setMaxLevelNameLength`'i ve logrus
///   `ERRO`, `WARN`, `DEBU` yazar;
/// - Monolog'un sayıları (`%level%`): `100`–`600`.
fn seviye_oku(metin: &str) -> Option<Seviye> {
    const DIGER: [(&str, Seviye); 31] = [
        ("WARN", Seviye::Warning),
        ("WRN", Seviye::Warning),
        ("ERR", Seviye::Error),
        ("FAIL", Seviye::Error),
        ("SEVERE", Seviye::Error),
        ("CRIT", Seviye::Critical),
        ("CRT", Seviye::Critical),
        ("FATAL", Seviye::Critical),
        ("FTL", Seviye::Critical),
        ("PANIC", Seviye::Critical),
        ("EMERG", Seviye::Emergency),
        ("INFORMATION", Seviye::Info),
        ("CONFIG", Seviye::Info),
        ("TRACE", Seviye::Debug),
        ("TRC", Seviye::Debug),
        ("TRCE", Seviye::Debug),
        ("VERBOSE", Seviye::Debug),
        ("VRB", Seviye::Debug),
        ("DBG", Seviye::Debug),
        ("DBUG", Seviye::Debug),
        ("FINE", Seviye::Debug),
        ("FINER", Seviye::Debug),
        ("FINEST", Seviye::Debug),
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
    let adlar = || {
        Seviye::HEPSI
            .into_iter()
            .map(|seviye| (seviye.ad(), seviye))
            .chain(DIGER)
    };
    if let Some((_, seviye)) = adlar().find(|(ad, _)| ad.eq_ignore_ascii_case(metin)) {
        return Some(seviye);
    }
    // Bir adın başı: en az üç harf. Başı o olan bütün adlar aynı seviyeyi göstermeli; bugünkü
    // adların ilk üç harfi bir seviyeyi tek başına belirler (CRI, CON; FAT, FIN; EME, ERR)
    if metin.len() < 3 || !metin.bytes().all(|bayt| bayt.is_ascii_alphabetic()) {
        return None;
    }
    let mut bulunan = None;
    for (ad, seviye) in adlar() {
        let bas_mi = ad.len() > metin.len() && ad[..metin.len()].eq_ignore_ascii_case(metin);
        if !bas_mi {
            continue;
        }
        match bulunan {
            None => bulunan = Some(seviye),
            Some(onceki) if onceki == seviye => {}
            Some(_) => return None,
        }
    }
    bulunan
}

/// `--desen`'i derler: Monolog'un biçim dizgesiyse (`%message%` içeriyorsa) önce düzenli
/// ifadeye çevrilir.
fn desen_derle(desen: &str, zaman: Option<&TimeFormat>) -> Result<Desen, DesenHatasi> {
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
fn monolog_bicimi(bicim: &str, zaman: Option<&TimeFormat>) -> Result<String, DesenHatasi> {
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
                        zaman.map_or(ISO_8601.to_string(), TimeFormat::regex),
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
        SatirBicimi::new(desen, Some(TimeFormat::compile(zaman).unwrap())).unwrap()
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
        // Başka araçların adları ve kısaltmaları; en az üç harflik başlar
        for (metin, seviye) in [
            ("INF", Seviye::Info),
            ("wrn", Seviye::Warning),
            ("DBG", Seviye::Debug),
            ("VRB", Seviye::Debug),
            ("FTL", Seviye::Critical),
            ("Information", Seviye::Info),
            ("fail", Seviye::Error),
            ("dbug", Seviye::Debug),
            ("trce", Seviye::Debug),
            ("SEVERE", Seviye::Error),
            ("CONFIG", Seviye::Info),
            ("FINEST", Seviye::Debug),
            ("PANIC", Seviye::Critical),
            ("550", Seviye::Alert),
            ("ERRO", Seviye::Error),
            ("WARNI", Seviye::Warning),
            ("DEBU", Seviye::Debug),
            ("NOTI", Seviye::Notice),
            ("CRI", Seviye::Critical),
            ("EME", Seviye::Emergency),
            ("ALE", Seviye::Alert),
            ("FATA", Seviye::Critical),
            ("TRAC", Seviye::Debug),
            ("PANI", Seviye::Critical),
            ("Infor", Seviye::Info),
            ("FIN", Seviye::Debug),
        ] {
            assert_eq!(seviye_oku(metin), Some(seviye), "{metin}");
        }
        for metin in [
            "", "hata", "401", "WARNINGS", "IN", "E", "ER", "ERRORS", "ERR0", "XYZ", "INFOS",
            "İNFO", "E-R", "1",
        ] {
            assert_eq!(seviye_oku(metin), None, "{metin}");
        }
    }

    #[test]
    fn seviyesi_taninmayan_satir_ayirt_edilir() {
        let desen = bicim(r"(?P<zaman>\S+ \S+) (?P<seviye>\w+) +(?P<mesaj>.*)");
        assert!(matches!(
            desen.tani("2026-10-27 14:03:11 warn x"),
            Tanima::Baslik(_)
        ));
        // Desene ve zamana uyuyor, seviye bilinmiyor
        assert_eq!(
            desen.tani("2026-10-27 14:03:11 sorun x"),
            Tanima::TaninmayanSeviye
        );
        // Desene ya da zamana uymuyor
        assert_eq!(desen.tani("#3 2026-10-27 14:03:11 warn x"), Tanima::Degil);
        assert_eq!(desen.tani("2026-10-27 25:03:11 sorun x"), Tanima::Degil);
        // Laravel'in biçiminde bilinmeyen seviye ayrıca sayılmaz: `[`'li başka satırlar çoktur
        assert_eq!(
            SatirBicimi::default().tani("[2026-10-27 14:03:11] production.HATA: x"),
            Tanima::Degil
        );
    }

    #[test]
    fn mesaj_ve_baglam_ayrilir() {
        for (metin, mesaj, baglam) in [
            (r#"Giriş {"ip":"x"}"#, "Giriş", r#"{"ip":"x"}"#),
            ("  Giriş yapıldı  ", "Giriş yapıldı", ""),
            // Mesaj boş: bağlamın önündeki boşluk ayırır
            (r#" {"a":1}"#, "", r#"{"a":1}"#),
            // Mesajın içindeki JSON'u bir kelime izliyorsa mesajın parçasıdır
            (
                r#"Ödeme {"tutar":50} onaylandı {"siparis":5}"#,
                r#"Ödeme {"tutar":50} onaylandı"#,
                r#"{"siparis":5}"#,
            ),
            (
                r#"Ödeme {"tutar":50} onaylandı"#,
                r#"Ödeme {"tutar":50} onaylandı"#,
                "",
            ),
            // Geçersiz JSON'dan sonra geçerli bir bağlam varsa o seçilir; yoksa geçersiz olan
            (
                r#"Şablon {"x" bozuk {"a":1}"#,
                r#"Şablon {"x" bozuk"#,
                r#"{"a":1}"#,
            ),
            (
                r#"Şablon {"x" bozuk {"a":1} sonra"#,
                "Şablon",
                r#"{"x" bozuk {"a":1} sonra"#,
            ),
            (r#"Bozuk {"a": 'b'}"#, "Bozuk", r#"{"a": 'b'}"#),
            // Satır sonunda yarım kalan bağlam: istisnanın yığın izi sonraki satırlarda
            (
                r#"Bağlantı koptu {"exception":"[object] (PDOException(code: 2002): x at /var/www/magaza/a.php:12)"#,
                "Bağlantı koptu",
                r#"{"exception":"[object] (PDOException(code: 2002): x at /var/www/magaza/a.php:12)"#,
            ),
            // Bağlam ve ek
            (
                r#"Kuyruk dolu {"a":1} {"uid":"ab"}"#,
                "Kuyruk dolu",
                r#"{"a":1} {"uid":"ab"}"#,
            ),
            (r#"Kuyruk dolu {"a":1} []"#, "Kuyruk dolu", r#"{"a":1} []"#),
            // Boş bağlam ve ek (`[]`) atılır: en fazla iki, önlerinde boşlukla
            ("Kuyruk dolu [] []", "Kuyruk dolu", ""),
            ("Kuyruk dolu []", "Kuyruk dolu", ""),
            (
                r#"Kuyruk dolu [] {"uid":"ab"}"#,
                "Kuyruk dolu",
                r#"{"uid":"ab"}"#,
            ),
            ("Liste [] [] []", "Liste []", ""),
            ("Liste:[]", "Liste:[]", ""),
            // Mesaj boş, bağlam ve ek boş; mesaj `[]`
            (" [] []", "", ""),
            ("  [] [] []", "[]", ""),
            ("[] []", "[]", ""),
            (r#"  [] {"uid":"ab"}"#, "", r#"{"uid":"ab"}"#),
            // Bağlamdan sonra JSON olmayan bir alan (desenle okunan biçimlerde): başka bir
            // aday yoksa bağlamdır
            (
                r#"Giriş {"ip":"x"} - istek"#,
                "Giriş",
                r#"{"ip":"x"} - istek"#,
            ),
            (r#"A {"b":1} - B {"c":2}"#, r#"A {"b":1} - B"#, r#"{"c":2}"#),
            // Ardından ek gelen bağlam, ardından JSON olmayan bir şey gelen öncekinden iyidir
            (
                r#"A {"b":1} - B {"c":2} []"#,
                r#"A {"b":1} - B"#,
                r#"{"c":2} []"#,
            ),
            // Satır sonunda yarım kalan bağlam da (yığın izi sonraki satırlarda) öncekinden iyidir
            (
                r#"A {"b":1} - B {"exception":"[object] (X(code: 0): y at /a.php:3)"#,
                r#"A {"b":1} - B"#,
                r#"{"exception":"[object] (X(code: 0): y at /a.php:3)"#,
            ),
        ] {
            assert_eq!(mesaji_ayir(metin), (mesaj, baglam), "{metin}");
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
