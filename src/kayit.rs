//! Laravel log satırlarını kayıtlara ayırır.
//!
//! Bir Laravel kaydı şöyle bir başlık satırıyla başlar:
//!
//! ```text
//! [2026-10-24 14:03:11] production.ERROR: Mesaj
//! [2026-10-24T14:03:11.123456+03:00] production.ERROR: Mesaj   (ISO 8601)
//! ```
//!
//! Başlıktan sonra gelen satırlar (stack trace, çok satırlı mesajlar) bir sonraki
//! başlığa kadar o kayda aittir.
//!
//! Kanalın biçimlendiricisi Monolog'un `JsonFormatter`'ı ise kayıt tek satırlık bir JSON
//! nesnesidir; iki biçim aynı dosyada karışık olabilir:
//!
//! ```text
//! {"message":"Mesaj","context":{},"level":400,"level_name":"ERROR","channel":"production","datetime":"2026-10-24T14:03:11.123456+00:00","extra":{}}
//! ```
//!
//! Başlık satırının biçimi `satir_bicimi` modülündedir: varsayılan Laravel'inkidir, başkası
//! `--desen` ile verilir.
//!
//! Satırlar akış halinde, teker teker gelir (`KayitToplayici`). Bir kaydın bittiği ancak bir
//! sonraki kayıt başladığında ya da girdi bittiğinde anlaşılır; bellekte yalnızca o an
//! toplanmakta olan kayıt durur. Tamamlanan kayıt bir `KayitAlici`ya verilir.
//!
//! Kaydın mesajı PHP'nin istisna metniyse (`Log::error($e)`; `istisna_metni` modülü) kaydın
//! mesajı istisnanın mesajıdır: `report($e)` ile loglanan aynı hatanın mesajı gibi.

use std::borrow::Cow;
use std::iter;
use std::mem;

use crate::istisna_metni::{self, IstisnaMetni};
use crate::json_okuyucu::{self, Deger};
use crate::satir_bicimi::{Baslik, BaslikYeri, SatirBicimi, Tanima};
use crate::zaman;

/// Laravel log seviyeleri. Sıralama önem sırasıdır: Debug en düşük, Emergency en yüksek.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Seviye {
    Debug,
    Info,
    Notice,
    Warning,
    Error,
    Critical,
    Alert,
    Emergency,
}

impl Seviye {
    /// Bütün seviyeler, önem sırasıyla.
    pub const HEPSI: [Seviye; 8] = [
        Seviye::Debug,
        Seviye::Info,
        Seviye::Notice,
        Seviye::Warning,
        Seviye::Error,
        Seviye::Critical,
        Seviye::Alert,
        Seviye::Emergency,
    ];

    /// Log dosyasındaki yazılışından seviyeye çevirir: "ERROR" → Seviye::Error
    pub(crate) fn ayristir(metin: &str) -> Option<Seviye> {
        Seviye::HEPSI
            .into_iter()
            .find(|seviye| seviye.ad() == metin)
    }

    /// Ekrana yazarken kullanılacak ad.
    pub fn ad(self) -> &'static str {
        match self {
            Seviye::Debug => "DEBUG",
            Seviye::Info => "INFO",
            Seviye::Notice => "NOTICE",
            Seviye::Warning => "WARNING",
            Seviye::Error => "ERROR",
            Seviye::Critical => "CRITICAL",
            Seviye::Alert => "ALERT",
            Seviye::Emergency => "EMERGENCY",
        }
    }
}

/// Tek bir log kaydı: başlık satırından (ya da JSON satırından) okunanlar ve ona ait devam
/// satırları.
///
/// `'a`: kayıt, satırların metnini kopyalamaz, onlardan ödünç alır. İstisnalar kayda aittir
/// (`Cow::Owned`): JSON satırlarındaki kaçışlı mesajların çözülmüş halleri
/// (`Trait \"X\" not found`), `--zaman-bicimi` ile çevrilen zamanlar ve açıklamalı istisna
/// metinlerinden kurulan mesajlar.
///
/// Alanlar gizli, dışarıdan yalnızca okunabilir (aşağıdaki metotlarla). Böylece bir kayıt
/// yalnızca bu modülde, denetlenerek oluşturulabilir.
#[derive(Debug, PartialEq)]
pub struct Kayit<'a> {
    /// İç biçimde: "2026-10-24 14:03:11", varsa kesir ve saat dilimiyle
    zaman: Cow<'a, str>,
    seviye: Seviye,
    /// Bağlam JSON'u olmadan mesajın kendisi: "Kullanıcı 381 giriş yaptı". Mesaj PHP'nin istisna
    /// metniyse istisnanın mesajı.
    mesaj: Cow<'a, str>,
    /// Başlık satırında mesajdan sonra gelen kısım: `{"ip":"203.0.113.24"}`. Yoksa ya da kayıt
    /// bir JSON satırıysa boş.
    baglam: &'a str,
    /// Metin biçiminde başlıkta seviyeden sonra gelen her şey (mesaj ve bağlam); JSON
    /// satırında çözülmüş mesajın tamamı. PHP'nin istisna metni burada başlar.
    govde: Cow<'a, str>,
    /// Başlıktan sonra gelen satırlar: stack trace, çok satırlı mesajlar.
    devam_satirlari: Vec<&'a str>,
    /// Kayıt bir JSON satırı mı?
    json: bool,
    /// Kayıt bir JSON satırıysa bağlamdaki istisna: `{"class":"...","file":"/yol/X.php:26",...}`
    json_istisna: Option<Deger<'a>>,
    /// Mesaj PHP'nin istisna metni mi (`Log::error($e)`)?
    istisna_metni: bool,
}

impl<'a> Kayit<'a> {
    /// Başlık satırından okunanlarla, devam satırları olmadan.
    fn basliktan(baslik: Baslik<'a>) -> Kayit<'a> {
        Kayit {
            zaman: baslik.zaman,
            seviye: baslik.seviye,
            mesaj: Cow::Borrowed(baslik.mesaj),
            baglam: baslik.baglam,
            govde: Cow::Borrowed(baslik.govde),
            devam_satirlari: Vec::new(),
            json: false,
            json_istisna: None,
            istisna_metni: false,
        }
    }

    /// Kaydın zamanı, iç biçimde. İlk 19 karakteri her zaman takvimde var olan bir
    /// tarih-saattir (`2026-10-24 14:03:11` ya da `2026-10-24T14:03:11`);
    /// `zaman::kayit_zamani_mi` bunu denetler.
    pub fn zaman(&self) -> &str {
        &self.zaman
    }

    pub fn seviye(&self) -> Seviye {
        self.seviye
    }

    /// Bağlam JSON'u olmadan mesajın kendisi; en fazla `MESAJ_SINIRI` bayt.
    ///
    /// Dönen metin kayda bağlıdır (`&self` kadar yaşar), satıra değil: JSON satırındaki kaçışlı
    /// bir mesajın çözülmüş hali yalnızca kayıtta vardır.
    pub fn mesaj(&self) -> &str {
        bayt_siniri(&self.mesaj, MESAJ_SINIRI)
    }

    /// Başlık satırında mesajdan sonra gelen bağlam: `{"exception":"[object] (...`
    /// Bağlam yoksa boş metin.
    pub fn baglam(&self) -> &'a str {
        self.baglam
    }

    /// Başlıktan sonra gelen satırlar.
    ///
    /// Dönen dilim kayda aittir (`&self` kadar yaşar); içindeki metinler ise satırlara
    /// (`'a` kadar yaşar).
    pub fn devam_satirlari(&self) -> &[&'a str] {
        &self.devam_satirlari
    }

    /// Kayıt bir JSON satırıysa ve bağlamında bir istisna varsa o istisna: Monolog onu bir
    /// nesne olarak yazar (`class`, `message`, `file`, belki `trace` ve `previous`).
    pub fn json_istisna(&self) -> Option<&Deger<'a>> {
        self.json_istisna.as_ref()
    }

    /// Mesaj PHP'nin istisna metniyse (`Log::error($e)`) en dıştaki istisna.
    pub fn istisna_metni(&self) -> Option<IstisnaMetni<'_>> {
        if !self.istisna_metni {
            return None;
        }
        if self.json {
            istisna_metni::ayristir(self.govde.lines())
        } else {
            istisna_metni::ayristir(
                iter::once(&*self.govde).chain(self.devam_satirlari.iter().copied()),
            )
        }
    }

    /// Mesaj PHP'nin istisna metniyse kaydın mesajını istisnanın mesajı yapar. Metin
    /// biçiminde istisna metni devam satırlarına uzanır: kayıt tamamlanınca çağrılır.
    fn istisna_metnine_bak(&mut self) {
        let mesaj = match &self.govde {
            Cow::Borrowed(govde) if !self.json => {
                // Satırlardan ödünç alınan parçalar kayda değil satırlara bağlıdır (`'a`)
                let govde: &'a str = govde;
                let Some(metin) = istisna_metni::ayristir(
                    iter::once(govde).chain(self.devam_satirlari.iter().copied()),
                ) else {
                    return;
                };
                if metin.aciklama.is_empty() {
                    Cow::Borrowed(metin.mesaj.trim())
                } else {
                    Cow::Owned(metin.kayit_mesaji())
                }
            }
            // JSON satırı: istisna metni mesajın tamamındadır
            govde => match istisna_metni::ayristir(govde.lines()) {
                Some(metin) => Cow::Owned(metin.kayit_mesaji()),
                None => return,
            },
        };
        self.mesaj = mesaj;
        self.istisna_metni = true;
    }
}

/// Tamamlanan bir kaydın logdaki yeri.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KayitBilgisi {
    /// Kaydın sıra numarası: logun kaçıncı kaydı olduğu, 0'dan başlayarak.
    pub sira: usize,
    /// Kaydın başlık satırının girdideki bayt konumu. Kayıt sonradan buradan yeniden okunabilir.
    pub konum: u64,
}

/// Tamamlanan kayıtları alan taraf: özetin deposu ya da testlerde bir liste.
///
/// Kayıt ödünç verilir ve yalnızca bu çağrı boyunca yaşar: toplayıcı satırlarını bir sonraki
/// kayıt için yeniden kullanır. Saklanacak bir şey varsa alıcı onu kopyalamalıdır.
pub trait KayitAlici {
    fn kayit(&mut self, kayit: &Kayit<'_>, bilgi: KayitBilgisi);
}

/// Satırların nereye gittiği. Her satır tam olarak bir yere gider: bir kaydın başlığı, bir
/// kaydın devamı ya da hiçbir kayda ait olmayan bir yetim satır.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SatirSayilari {
    pub satir: usize,
    pub kayit: usize,
    pub devam: usize,
    /// İlk kayıttan önce gelen, hiçbir kayda ait olmayan satırlar. Dosya bir kaydın ortasından
    /// başlıyorsa olur (ör. log dosyası döndürüldüğünde).
    pub yetim: usize,
    /// `--desen`'e ve zamana uyduğu halde seviyesi tanınmadığı için kayıt sayılmayan satırlar.
    /// Ayrı bir yer değildir: bu satırlar da yukarıdakiler gibi bir kaydın devamı ya da yetim
    /// sayılır. Kullanıcı uyarılsın diye ayrıca sayılır.
    pub taninmayan_seviye: usize,
}

/// Bir kaydın saklanan devam satırlarının toplam boyu en fazla bu kadar bayt (256 MB) olabilir.
/// Sığmayan ilk satırdan sonrakiler sayılır ama saklanmaz: kaydın başı (istisnanın sınıfı ve
/// yeri, uygulama kodunun çerçeveleri) olduğu gibi kalır, sonundan bir kısım eksilir. Gerçek
/// kayıtlar bunun çok altındadır; sınır, sonu gelmeyen bir kaydın (bir döküm, bozuk bir dosya)
/// belleği doldurmasını önler.
const DEVAM_SINIRI: usize = 256 << 20;
/// Bir kaydın saklanan devam satırı sayısı da sınırlıdır (bir milyon): boş satırlar bayt
/// tutmaz, ama her satırın yeri ve kayıt tamamlanırken dilimi bellek tutar. PHP'nin en uzun
/// yığın izleri birkaç yüz satırdır.
const DEVAM_SATIR_SINIRI: usize = 1 << 20;

/// Bir başlık satırının ve bir JSON kaydının mesajının en fazla bu kadar baytı (1 MB) okunur ve
/// saklanır: toplayıcı başlık satırını kayıt tamamlanana kadar kopyalar, 256 MB'lık bir satır
/// belleği ikinci kez tutmasın. Kaydın bütün parçaları satırın başındadır (zaman, seviye,
/// mesaj ve bağlamın başı); istisnanın yığın izi devam satırlarındadır.
pub const BASLIK_SINIRI: usize = 1 << 20;

/// Bir mesajın en fazla bu kadar baytı (64 KB) maskelenir, kümelenir ve örnek olarak saklanır.
/// Satırlar 256 MB'a kadar okunur, ama yüz megabaytlık bir mesajın maskelenmiş kopyaları ve
/// Drain'in belirteçleri gigabaytlar tutardı. Gerçek mesajlar bunun çok altındadır; daha
/// uzunları, ilk 64 KB'ları aynıysa aynı mesaj sayılır.
pub const MESAJ_SINIRI: usize = 64 << 10;

/// Metnin en fazla `sinir` baytlık başı; bir karakter ortadan bölünmez.
pub fn bayt_siniri(metin: &str, sinir: usize) -> &str {
    if metin.len() <= sinir {
        return metin;
    }
    let mut son = sinir;
    while !metin.is_char_boundary(son) {
        son -= 1;
    }
    &metin[..son]
}

/// Satırları teker teker alıp kayıtlara toplar.
///
/// Başlık satırı ya da bir JSON kaydı yeni bir kayıt başlatır; sonraki satırlar bir sonraki
/// kayda kadar o kayda eklenir. Toplanmakta olan kayıt yeni bir kayıt başlayınca ya da `bitir`
/// çağrılınca tamamlanır ve alıcıya verilir.
///
/// Bazı Windows araçları dosyanın başına bir UTF-8 BOM'u (U+FEFF) yazar. Atılmazsa satır `[`
/// ile başlamaz ve kayıt bir önceki kaydın devam satırı sayılır. BOM her satırın başında
/// atılır: birden çok dosya birleştirildiğinde (`cat a.log b.log | veylog -`) her dosyanın
/// BOM'u kendi ilk satırındadır.
#[derive(Debug, Default)]
pub struct KayitToplayici {
    /// Satırların biçimi: hangi satır bir kaydı başlatır.
    bicim: SatirBicimi,
    bekleyen: Bekleyen,
    /// Toplanmakta olan metin kaydının başlık satırı.
    baslik: String,
    /// Toplanmakta olan JSON kaydının zamanı ve çözülmüş mesajının tamamı.
    json_zaman: String,
    json_govde: String,
    /// Toplanmakta olan kaydın devam satırları art arda ve her birinin bittiği yer. Tamponlar
    /// kayıttan kayda yeniden kullanılır; her satır için ayrı bir bellek ayrılmaz. Konumlar
    /// `u32`: devam satırları `DEVAM_SINIRI`'nı geçmez.
    devam: String,
    devam_sonlari: Vec<u32>,
    /// Kaydın bir devam satırı sınırlara sığmadı: sonrakiler de saklanmaz.
    devam_kesildi: bool,
    /// Devam satırlarının bayt ve satır sınırları; None ise (`Default`) `DEVAM_SINIRI` ve
    /// `DEVAM_SATIR_SINIRI`. Testlerde daha küçük sınırlar verilir.
    sinirlar: Option<(usize, usize)>,
    sayilar: SatirSayilari,
}

/// Toplanmakta olan kayıt.
#[derive(Debug, Default)]
enum Bekleyen {
    #[default]
    Yok,
    /// Metin biçiminde bir kayıt; başlık satırı `baslik`ta, parçalarının yerleri burada. Kayıt
    /// tamamlanınca başlık bunlardan kurulur; satır yeniden ayrıştırılmaz.
    Metin(KayitBilgisi, BaslikYeri),
    /// İstisnası olmayan bir JSON kaydı; alanları `json_*`'da. Devam satırları beklenir:
    /// istisna onlarda olabilir.
    Json(KayitBilgisi, Seviye),
    /// İstisnalı bir JSON kaydı. Gereken her şey JSON satırındaydı; satır gelir gelmez alıcıya
    /// verildi. Devam satırları yalnızca sayılır.
    Verildi,
}

impl KayitToplayici {
    /// Satırları verilen biçimde okuyan bir toplayıcı. `KayitToplayici::default()` Laravel'in
    /// biçimini okur.
    pub fn new(bicim: SatirBicimi) -> Self {
        KayitToplayici {
            bicim,
            ..KayitToplayici::default()
        }
    }

    /// Bir satırı işler. `konum`: satırın girdideki bayt konumu.
    pub fn satir<A: KayitAlici + ?Sized>(&mut self, satir: &str, konum: u64, alici: &mut A) {
        self.sayilar.satir += 1;
        let satir = satir.strip_prefix('\u{feff}').unwrap_or(satir);
        let baslik_satiri = bayt_siniri(satir, BASLIK_SINIRI);
        match self.bicim.tani(baslik_satiri) {
            Tanima::Baslik(baslik) => {
                let yer = baslik.yeri(baslik_satiri);
                self.bitir(alici);
                let bilgi = self.yeni_kayit(konum);
                self.baslik.clear();
                self.baslik.push_str(baslik_satiri);
                self.bekleyen = Bekleyen::Metin(bilgi, yer);
                return;
            }
            Tanima::TaninmayanSeviye => self.sayilar.taninmayan_seviye += 1,
            Tanima::Degil => {}
        }
        if let Some(kayit) = json_satiri_ayristir(satir) {
            self.bitir(alici);
            let bilgi = self.yeni_kayit(konum);
            if kayit.json_istisna.is_some() {
                alici.kayit(&kayit, bilgi);
                self.bekleyen = Bekleyen::Verildi;
            } else {
                self.json_zaman.clear();
                self.json_zaman.push_str(&kayit.zaman);
                self.json_govde.clear();
                self.json_govde.push_str(&kayit.govde);
                self.bekleyen = Bekleyen::Json(bilgi, kayit.seviye);
            }
        } else if matches!(self.bekleyen, Bekleyen::Yok) {
            self.sayilar.yetim += 1;
        } else {
            self.sayilar.devam += 1;
            if matches!(self.bekleyen, Bekleyen::Verildi) || self.devam_kesildi {
                return;
            }
            let (bayt_siniri, satir_siniri) =
                self.sinirlar.unwrap_or((DEVAM_SINIRI, DEVAM_SATIR_SINIRI));
            if self.devam.len() + satir.len() <= bayt_siniri
                && self.devam_sonlari.len() < satir_siniri
            {
                self.devam.push_str(satir);
                self.devam_sonlari
                    .push(u32::try_from(self.devam.len()).expect("devam satırları 4 GB'ı geçmez"));
            } else {
                self.devam_kesildi = true;
            }
        }
    }

    /// Devam satırlarını daha küçük sınırlarla saklayan bir toplayıcı (testler için).
    #[cfg(test)]
    fn sinirli(bayt_siniri: usize, satir_siniri: usize) -> Self {
        KayitToplayici {
            sinirlar: Some((bayt_siniri, satir_siniri)),
            ..KayitToplayici::default()
        }
    }

    /// Toplanmakta olan kaydı tamamlar ve alıcıya verir. Girdi bitince çağrılır; yeni bir kayıt
    /// başlayınca da kendiliğinden çağrılır.
    pub fn bitir<A: KayitAlici + ?Sized>(&mut self, alici: &mut A) {
        // Ödünç alma kuralı: kayıt `self`in alanlarından ödünç alırken `self.bekleyen`
        // değiştirilemez; bu yüzden önce dışarı alınır
        let bekleyen = mem::take(&mut self.bekleyen);
        let mut bas = 0;
        // Devam satırları yalnızca alıcıya verilecek bir kayıt varsa dilimlenir
        let mut devam_satirlari = || -> Vec<&str> {
            self.devam_sonlari
                .iter()
                .map(|&son| {
                    let son = son as usize;
                    let satir = &self.devam[bas..son];
                    bas = son;
                    satir
                })
                .collect()
        };
        match bekleyen {
            Bekleyen::Metin(bilgi, yer) => {
                let mut kayit = Kayit::basliktan(yer.baslik(&self.baslik));
                kayit.devam_satirlari = devam_satirlari();
                kayit.istisna_metnine_bak();
                alici.kayit(&kayit, bilgi);
            }
            Bekleyen::Json(bilgi, seviye) => {
                let mut kayit = Kayit {
                    zaman: Cow::Borrowed(&self.json_zaman),
                    seviye,
                    mesaj: ilk_satir(&Cow::Borrowed(&self.json_govde)),
                    baglam: "",
                    govde: Cow::Borrowed(&self.json_govde),
                    devam_satirlari: devam_satirlari(),
                    json: true,
                    json_istisna: None,
                    istisna_metni: false,
                };
                kayit.istisna_metnine_bak();
                alici.kayit(&kayit, bilgi);
            }
            Bekleyen::Verildi | Bekleyen::Yok => {}
        }
        self.devam.clear();
        self.devam_sonlari.clear();
        self.devam_kesildi = false;
    }

    /// Şimdiye kadarki satır sayıları.
    pub fn sayilar(&self) -> SatirSayilari {
        self.sayilar
    }

    /// Toplanmakta olan, henüz alıcıya verilmemiş bir kayıt var mı? Canlı izlemede girdi bir
    /// süre sessiz kalınca böyle bir kayıt tamamlanmış sayılır (`bitir`).
    pub fn bekliyor(&self) -> bool {
        matches!(self.bekleyen, Bekleyen::Metin(..) | Bekleyen::Json(..))
    }

    /// Yeni başlayan kaydın bilgisi: sıra numarası ve konumu.
    fn yeni_kayit(&mut self, konum: u64) -> KayitBilgisi {
        let bilgi = KayitBilgisi {
            sira: self.sayilar.kayit,
            konum,
        };
        self.sayilar.kayit += 1;
        bilgi
    }
}

/// Satır bir kaydın başlangıcıysa (verilen biçimde bir başlık ya da bir JSON kaydı) o kayıt;
/// devam satırları olmadan. Satırın başındaki BOM atılır.
///
/// Toplayıcıyla aynı kararı verir; başlık satırının da toplayıcıdaki gibi yalnızca ilk
/// `BASLIK_SINIRI` baytına bakılır. Kaydın zamanı ve seviyesi kesindir; mesajı ise metin
/// biçiminde PHP'nin istisna metniyse (o devam satırlarına uzanır) başlıktaki gibidir.
pub fn kayit_baslangici<'a>(bicim: &SatirBicimi, satir: &'a str) -> Option<Kayit<'a>> {
    let satir = satir.strip_prefix('\u{feff}').unwrap_or(satir);
    match bicim.baslik(bayt_siniri(satir, BASLIK_SINIRI)) {
        Some(baslik) => Some(Kayit::basliktan(baslik)),
        None => json_satiri_ayristir(satir),
    }
}

/// Satır Monolog'un JSON biçiminde bir kayıtsa ondan yeni bir kayıt oluşturur.
///
/// Satır bir JSON nesnesi olmalı ve en az şu alanları taşımalıdır: `datetime` (geçerli bir
/// zaman), `level_name` (bilinen bir seviye) ve `message`. Biri eksikse satır kayıt değildir;
/// bir önceki kaydın devam satırı sayılır.
fn json_satiri_ayristir(satir: &str) -> Option<Kayit<'_>> {
    // Okumaya çalışmadan önce ucuz bir ön eleme: JSON kaydı `{` ile başlar ve üç alanın
    // adını taşır. Kaydın devam satırlarındaki JSON (bir HTTP yanıtının gövdesi) böylece hiç
    // ayrıştırılmaz.
    if !satir.starts_with('{')
        || !satir.contains("\"datetime\"")
        || !satir.contains("\"level_name\"")
        || !satir.contains("\"message\"")
    {
        return None;
    }
    let deger = json_okuyucu::kayit_oku(satir).ok()?;
    // Zaman kaçış içeremez (rakamlar, `-`, `:`, `T`, `+`); ham hali doğrudan denetlenir
    let zaman = deger.alan("datetime")?.ham_metin()?;
    if !zaman::kayit_zamani_mi(zaman) {
        return None;
    }
    let seviye = Seviye::ayristir(deger.alan("level_name")?.ham_metin()?)?;
    let govde = match deger.alan("message")?.metin()? {
        Cow::Borrowed(govde) => Cow::Borrowed(bayt_siniri(govde, BASLIK_SINIRI)),
        Cow::Owned(govde) if govde.len() > BASLIK_SINIRI => {
            Cow::Owned(bayt_siniri(&govde, BASLIK_SINIRI).to_owned())
        }
        govde => govde,
    };
    let json_istisna = deger
        .alan("context")
        .and_then(|baglam| baglam.alan("exception"))
        .cloned();

    let mut kayit = Kayit {
        zaman: Cow::Borrowed(zaman),
        seviye,
        mesaj: ilk_satir(&govde),
        baglam: "",
        govde,
        devam_satirlari: Vec::new(),
        json: true,
        json_istisna,
        istisna_metni: false,
    };
    kayit.istisna_metnine_bak();
    Some(kayit)
}

/// Mesajın ilk satırı, baştaki ve sondaki boşluklar atılmış olarak. Metin biçiminde çok
/// satırlı bir mesajın da yalnızca ilk satırı başlıktadır; iki biçimde aynı mesaj böylece aynı
/// kalıba düşer.
fn ilk_satir<'a>(mesaj: &Cow<'a, str>) -> Cow<'a, str> {
    match mesaj {
        Cow::Borrowed(metin) => Cow::Borrowed(metin.lines().next().unwrap_or("").trim()),
        Cow::Owned(metin) => Cow::Owned(metin.lines().next().unwrap_or("").trim().to_owned()),
    }
}

/// Testlerde kayıtları toplamak için: her kaydın sahiplenilmiş bir kopyası.
#[cfg(test)]
pub mod deneme {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    pub struct ToplananKayit {
        pub zaman: String,
        pub seviye: Seviye,
        pub mesaj: String,
        pub baglam: String,
        pub devam: Vec<String>,
        pub json_istisnali: bool,
        pub bilgi: KayitBilgisi,
    }

    impl KayitAlici for Vec<ToplananKayit> {
        fn kayit(&mut self, kayit: &Kayit<'_>, bilgi: KayitBilgisi) {
            self.push(ToplananKayit {
                zaman: kayit.zaman().to_owned(),
                seviye: kayit.seviye(),
                mesaj: kayit.mesaj().to_owned(),
                baglam: kayit.baglam().to_owned(),
                devam: kayit
                    .devam_satirlari()
                    .iter()
                    .map(|satir| satir.to_string())
                    .collect(),
                json_istisnali: kayit.json_istisna().is_some(),
                bilgi,
            });
        }
    }

    /// Metni satır satır toplayıcıdan geçirir: kayıtlar ve satır sayıları.
    pub fn topla(metin: &str) -> (Vec<ToplananKayit>, SatirSayilari) {
        topla_bicimle(metin, SatirBicimi::default())
    }

    /// `topla` gibi, satırları verilen biçimde okuyarak.
    pub fn topla_bicimle(metin: &str, bicim: SatirBicimi) -> (Vec<ToplananKayit>, SatirSayilari) {
        let mut kayitlar = Vec::new();
        let mut toplayici = KayitToplayici::new(bicim);
        let mut konum = 0;
        // `split_inclusive`: satır sonları satırda kalır; konum onlarla birlikte ilerler
        for parca in metin.split_inclusive('\n') {
            let satir = parca
                .strip_suffix('\n')
                .map_or(parca, |satir| satir.strip_suffix('\r').unwrap_or(satir));
            toplayici.satir(satir, konum, &mut kayitlar);
            konum += parca.len() as u64;
        }
        toplayici.bitir(&mut kayitlar);
        (kayitlar, toplayici.sayilar())
    }
}

#[cfg(test)]
mod testler {
    use super::deneme::{ToplananKayit, topla, topla_bicimle};
    use super::*;
    use crate::zaman_bicimi::ZamanBicimi;

    /// Laravel'in biçimindeki bir başlık satırından kayıt, devam satırları olmadan.
    fn baslik_ayristir(satir: &str) -> Option<Kayit<'_>> {
        SatirBicimi::default().baslik(satir).map(Kayit::basliktan)
    }

    #[test]
    fn baslik_alanlarina_ayrilir() {
        let kayit = baslik_ayristir("[2026-10-24 14:03:11] production.ERROR: Saat 10:00 raporu");
        assert_eq!(
            kayit,
            Some(Kayit {
                zaman: Cow::Borrowed("2026-10-24 14:03:11"),
                seviye: Seviye::Error,
                mesaj: Cow::Borrowed("Saat 10:00 raporu"),
                baglam: "",
                govde: Cow::Borrowed("Saat 10:00 raporu"),
                devam_satirlari: vec![],
                json: false,
                json_istisna: None,
                istisna_metni: false,
            })
        );
    }

    #[test]
    fn json_satiri_kayittir() {
        let satir = r#"{"message":"Trait \"App\\Traits\\Etiketli\" not found\nikinci satır","context":{"userId":7,"exception":{"class":"Symfony\\Component\\ErrorHandler\\Error\\FatalError","code":0,"file":"C:\\Users\\İrem\\Herd\\magaza\\app\\Models\\Urun.php:12"}},"level":500,"level_name":"CRITICAL","channel":"local","datetime":"2026-10-24T09:02:13.123456+00:00","extra":{}}"#;
        let kayit = json_satiri_ayristir(satir).unwrap();
        assert_eq!(kayit.zaman, "2026-10-24T09:02:13.123456+00:00");
        assert_eq!(kayit.seviye, Seviye::Critical);
        // Kaçışlar çözülür, yalnızca ilk satır alınır
        assert_eq!(kayit.mesaj, r#"Trait "App\Traits\Etiketli" not found"#);
        assert!(matches!(kayit.mesaj, Cow::Owned(_)));
        assert_eq!(kayit.baglam, "");
        let istisna = kayit.json_istisna().unwrap();
        assert_eq!(
            istisna.alan("class").and_then(Deger::ham_metin),
            Some(r"Symfony\\Component\\ErrorHandler\\Error\\FatalError")
        );

        // Kaçışsız mesaj kopyalanmaz; istisnasız bağlamda istisna yok
        let kayit = json_satiri_ayristir(
            r#"{"message":"Kullanıcı 381 giriş yaptı","context":{"ip":"203.0.113.24"},"level":200,"level_name":"INFO","channel":"local","datetime":"2026-10-24T09:02:13+00:00","extra":{}}"#,
        )
        .unwrap();
        assert!(matches!(
            kayit.mesaj,
            Cow::Borrowed("Kullanıcı 381 giriş yaptı")
        ));
        assert_eq!(kayit.json_istisna(), None);

        // Baştaki ve sondaki boşluklar atılır; kaçış yoksa yine kopyalanmaz
        let kayit = json_satiri_ayristir(
            r#"{"message":"  Boşluklu mesaj  ","level_name":"INFO","datetime":"2026-10-24T09:02:13+00:00"}"#,
        )
        .unwrap();
        assert!(matches!(kayit.mesaj, Cow::Borrowed("Boşluklu mesaj")));
    }

    #[test]
    fn kayit_olmayan_json_satirlari() {
        for satir in [
            // Alanlardan biri eksik
            r#"{"message":"x","level_name":"INFO"}"#,
            r#"{"message":"x","datetime":"2026-10-24T09:02:13+00:00"}"#,
            r#"{"level_name":"INFO","datetime":"2026-10-24T09:02:13+00:00"}"#,
            // Alan var ama geçersiz ya da metin değil
            r#"{"message":"x","level_name":"HATA","datetime":"2026-10-24T09:02:13+00:00"}"#,
            r#"{"message":"x","level_name":"INFO","datetime":"dün"}"#,
            r#"{"message":"x","level_name":"INFO","datetime":1761296533}"#,
            r#"{"message":12,"level_name":"INFO","datetime":"2026-10-24T09:02:13+00:00"}"#,
            // Bozuk JSON, JSON dizisi, başta boşluk
            r#"{"message":"x","level_name":"INFO","datetime":"2026-10-24T09:02:13+00:00""#,
            r#"[{"message":"x","level_name":"INFO","datetime":"2026-10-24T09:02:13+00:00"}]"#,
            r#" {"message":"x","level_name":"INFO","datetime":"2026-10-24T09:02:13+00:00"}"#,
            // Metin biçimindeki bir kaydın bağlamının devamı
            r#"{"exception":"[object] (Exception(code: 0): x at /var/www/app/Kod.php:12)"}"#,
            "\"}",
        ] {
            assert!(
                json_satiri_ayristir(satir).is_none(),
                "kayıt sayıldı: {satir}"
            );
        }
        // En az alanlarla yeter; sonda boşluk olabilir
        assert!(
            json_satiri_ayristir(
                r#"{"message":"x","level_name":"INFO","datetime":"2026-10-24T09:02:13+00:00"}  "#
            )
            .is_some()
        );
    }

    #[test]
    fn metin_ve_json_kayitlari_karisik_olabilir() {
        let metin = [
            "[2026-10-24 10:00:00] local.ERROR: Metin biçiminde bir hata",
            "[stacktrace]",
            "#0 /var/www/app/Kod.php(41)",
            r#"{"message":"JSON biçiminde","context":{},"level":200,"level_name":"INFO","channel":"local","datetime":"2026-10-24T09:00:00.5+00:00","extra":{}}"#,
            "JSON kaydından sonra gelen bir satır",
            "[2026-10-24 11:00:00] local.INFO: Yine metin",
        ]
        .join("\n");
        let (kayitlar, sayilar) = topla(&metin);

        assert_eq!(kayitlar.len(), 3);
        assert_eq!(kayitlar[1].mesaj, "JSON biçiminde");
        assert_eq!(kayitlar[1].zaman, "2026-10-24T09:00:00.5+00:00");
        assert_eq!(kayitlar[0].devam.len(), 2);
        assert_eq!(kayitlar[1].devam, ["JSON kaydından sonra gelen bir satır"]);
        assert_eq!(
            sayilar,
            SatirSayilari {
                satir: 6,
                kayit: 3,
                devam: 3,
                yetim: 0,
                taninmayan_seviye: 0,
            }
        );
        // Sıra numaraları logdaki sıradır; konumlar başlık satırlarının baytı
        let bilgiler: Vec<KayitBilgisi> = kayitlar.iter().map(|kayit| kayit.bilgi).collect();
        let konum = |satir: &str| metin.find(satir).unwrap() as u64;
        assert_eq!(
            bilgiler,
            [
                KayitBilgisi { sira: 0, konum: 0 },
                KayitBilgisi {
                    sira: 1,
                    konum: konum("{\"message\"")
                },
                KayitBilgisi {
                    sira: 2,
                    konum: konum("[2026-10-24 11:00:00]")
                },
            ]
        );
    }

    #[test]
    fn istisnali_json_kaydi_hemen_verilir_devami_sayilir() {
        // İstisnalı JSON kaydının devam satırlarına bakılmaz: istisnası satırın kendisindedir
        let metin = [
            r#"{"message":"Bağlantı yok","context":{"exception":{"class":"RuntimeException","file":"/var/www/magaza/app/A.php:3"}},"level_name":"ERROR","datetime":"2026-10-24T09:00:00+00:00"}"#,
            "bir devam satırı",
            "ikinci devam satırı",
            r#"{"message":"İstisnasız","context":{},"level_name":"INFO","datetime":"2026-10-24T09:00:01+00:00"}"#,
            "[object] (RuntimeException(code: 0): Devamdaki istisna at /var/www/magaza/app/B.php:4)",
        ]
        .join("\n");
        let (kayitlar, sayilar) = topla(&metin);
        assert_eq!(kayitlar.len(), 2);
        assert!(kayitlar[0].json_istisnali);
        assert!(kayitlar[0].devam.is_empty());
        // İstisnasız JSON kaydının devam satırları saklanır: istisna orada olabilir
        assert!(!kayitlar[1].json_istisnali);
        assert_eq!(kayitlar[1].devam.len(), 1);
        assert_eq!(kayitlar[1].baglam, "");
        assert_eq!(sayilar.devam, 3);
    }

    #[test]
    fn mesaj_baglamdan_ve_bosluklardan_ayrilir() {
        let kayit = baslik_ayristir(
            r#"[2026-10-24 09:02:13] production.INFO: Kullanıcı 381 giriş yaptı {"ip":"203.0.113.24"} "#,
        )
        .unwrap();
        assert_eq!(kayit.mesaj, "Kullanıcı 381 giriş yaptı");
        assert_eq!(kayit.baglam, r#"{"ip":"203.0.113.24"}"#);

        // Mesajın içindeki ":" işaretleri bölmeyi bozmamalı; sondaki boşluklar atılmalı
        let kayit = baslik_ayristir(
            "[2026-10-24 08:59:58] production.INFO: Zamanlanmış görev başladı: kurlar:guncelle  ",
        )
        .unwrap();
        assert_eq!(kayit.mesaj, "Zamanlanmış görev başladı: kurlar:guncelle");
        assert_eq!(kayit.baglam, "");
    }

    #[test]
    fn mesaji_bos_istisnada_baglam_kaybolmaz() {
        // `throw new Exception()` boş mesajla loglanır: seviyeden sonra iki boşluk ve bağlam
        let kayit = baslik_ayristir(
            r#"[2026-10-24 10:00:00] local.ERROR:  {"exception":"[object] (Exception(code: 0):  at /var/www/app/Kod.php:12)"} "#,
        )
        .unwrap();
        assert_eq!(kayit.mesaj, "");
        assert!(
            kayit
                .baglam
                .starts_with(r#"{"exception":"[object] (Exception"#)
        );
    }

    #[test]
    fn koseli_parantezli_diger_satirlar_baslik_degildir() {
        assert!(baslik_ayristir("[stacktrace]").is_none());
        assert!(baslik_ayristir("[previous exception] [object] (x)").is_none());
        assert!(baslik_ayristir("#0 /var/www/app/Kod.php(41)").is_none());
        assert!(baslik_ayristir("").is_none());
    }

    #[test]
    fn bozuk_zaman_reddedilir() {
        assert!(baslik_ayristir("[2026-10-24] local.INFO: x").is_none());
        assert!(baslik_ayristir("[2026/10/24 14:03:11] local.INFO: x").is_none());
        assert!(baslik_ayristir("[2026-10-24 14:03:1x] local.INFO: x").is_none());
    }

    #[test]
    fn iso_8601_ve_kesirli_zamanlar_taninir() {
        for satir in [
            "[2026-10-24T14:03:11+03:00] production.ERROR: x",
            "[2026-10-24T14:03:11.123456+03:00] production.ERROR: x",
            "[2026-10-24 14:03:11.123456] production.ERROR: x",
            "[2026-10-24T14:03:11Z] production.ERROR: x",
            "[2026-10-24T14:03:11+0300] production.ERROR: x",
        ] {
            assert!(baslik_ayristir(satir).is_some(), "tanınmadı: {satir}");
        }
    }

    #[test]
    fn takvimde_olmayan_zaman_baslik_sayilmaz() {
        for zaman in [
            "2026-02-29 10:00:00",
            "2026-02-30 10:00:00",
            "2026-04-31 10:00:00",
            "2026-00-10 10:00:00",
            "2026-13-01 10:00:00",
            "2026-10-00 10:00:00",
            "2026-10-24 24:00:00",
            "2026-10-24 23:60:00",
            "2026-10-24T23:59:60+03:00",
            "2026-10-24T14:03:11+24:00",
            "2026-10-24T14:03:11-03:60",
            "2026-10-24T14:03:11+0390",
        ] {
            let satir = format!("[{zaman}] local.ERROR: x");
            assert!(baslik_ayristir(&satir).is_none(), "{satir}");
            let json = format!(r#"{{"message":"x","level_name":"ERROR","datetime":"{zaman}"}}"#);
            assert!(json_satiri_ayristir(&json).is_none(), "{json}");
        }
        // Artık yılın 29 Şubat'ı ve günün son saniyesi geçerli
        assert!(baslik_ayristir("[2024-02-29 23:59:59] local.ERROR: x").is_some());

        // Önceki kaydın devam satırı olur: loglanan bir girdi başlık taklidi yapamaz
        let (kayitlar, _) = topla(
            "[2026-03-01 12:00:00] local.ERROR: Form gönderildi\n[2026-02-30 10:00:00] local.ERROR: sahte",
        );
        assert_eq!(kayitlar.len(), 1);
        assert_eq!(
            kayitlar[0].devam,
            ["[2026-02-30 10:00:00] local.ERROR: sahte"]
        );
    }

    #[test]
    fn bom_her_dosyanin_ilk_satirinda_atilir() {
        // İki BOM'lu dosya birleştirilmiş (`cat a.log b.log`): ikinci dosyanın BOM'u ortada
        let metin = "\u{feff}[2026-10-24 10:00:00] local.INFO: Birinci gün\n\
                     \u{feff}[2026-10-25 10:00:00] local.CRITICAL: İkinci gün\n\
                     \u{feff}{\"message\":\"Üçüncü\",\"level_name\":\"ERROR\",\"datetime\":\"2026-10-26T10:00:00+00:00\"}";
        let (kayitlar, sayilar) = topla(metin);
        let mesajlar: Vec<&str> = kayitlar.iter().map(|kayit| kayit.mesaj.as_str()).collect();
        assert_eq!(mesajlar, ["Birinci gün", "İkinci gün", "Üçüncü"]);
        assert_eq!(kayitlar[1].seviye, Seviye::Critical);
        assert_eq!(sayilar.yetim, 0);
        // Yeniden okurken de aynı karar verilir
        assert!(
            kayit_baslangici(
                &SatirBicimi::default(),
                "\u{feff}[2026-10-25 10:00:00] local.CRITICAL: x"
            )
            .is_some()
        );
    }

    #[test]
    fn bilinmeyen_seviye_baslik_sayilmaz() {
        assert!(baslik_ayristir("[2026-10-24 14:03:11] local.HATA: x").is_none());
        // Bilinen bütün seviyeler adlarından geri bulunur
        for seviye in Seviye::HEPSI {
            assert_eq!(Seviye::ayristir(seviye.ad()), Some(seviye));
        }
        assert_eq!(Seviye::ayristir("error"), None);
    }

    #[test]
    fn seviyeler_onem_sirasina_gore_karsilastirilir() {
        assert!(Seviye::Error > Seviye::Warning);
        assert!(Seviye::Debug < Seviye::Info);
        assert!(Seviye::HEPSI.is_sorted());
    }

    #[test]
    fn devam_satirlari_onceki_kayda_eklenir() {
        let metin = [
            "[2026-10-24 10:00:00] local.ERROR: Bir hata",
            "[stacktrace]",
            "#0 /var/www/app/Kod.php(41)",
            "",
            "[2026-10-24 10:00:05] local.INFO: Tamam",
        ]
        .join("\n");
        let (kayitlar, sayilar) = topla(&metin);

        assert_eq!(kayitlar.len(), 2);
        assert_eq!(
            kayitlar[0].devam,
            ["[stacktrace]", "#0 /var/www/app/Kod.php(41)", ""]
        );
        assert!(kayitlar[1].devam.is_empty());
        assert_eq!(sayilar.yetim, 0);
        assert_eq!(sayilar.devam, 3);
    }

    #[test]
    fn ilk_kayittan_onceki_satirlar_yetim_kalir() {
        let metin = ["#15 {main}", "\"}", "[2026-10-24 10:00:00] local.INFO: x"].join("\n");
        let (kayitlar, sayilar) = topla(&metin);

        assert_eq!(sayilar.yetim, 2);
        assert_eq!(kayitlar.len(), 1);
        // "#15 {main}\n" 11 bayt, "\"}\n" 3 bayt
        assert_eq!(kayitlar[0].bilgi.konum, 14);
    }

    #[test]
    fn cok_uzun_kaydin_devami_sinira_kadar_saklanir() {
        // Sonu gelmeyen bir kayıt: devam satırları sayılır, belli bir boydan sonra saklanmaz
        const BAYT: usize = 1_000;
        const SATIR: usize = 50;
        // Satırlar sınıra tam sığar: onuncu satırla devam tam 1 000 bayt olur
        let satir = "x".repeat(100);
        let mut toplayici = KayitToplayici::sinirli(BAYT, SATIR);
        let mut kayitlar: Vec<ToplananKayit> = Vec::new();
        toplayici.satir("[2026-10-24 10:00:00] local.ERROR: Döküm", 0, &mut kayitlar);
        for _ in 0..20 {
            toplayici.satir(&satir, 0, &mut kayitlar);
        }
        toplayici.bitir(&mut kayitlar);
        assert_eq!(kayitlar.len(), 1);
        assert_eq!(kayitlar[0].devam.len(), BAYT / satir.len());
        assert_eq!(toplayici.sayilar().devam, 20);

        // Sığmayan ilk satırdan sonrakiler, kısa olsalar da saklanmaz: saklananlar kaydın
        // başıdır, araya boşluk girmez
        let mut toplayici = KayitToplayici::sinirli(BAYT, SATIR);
        let mut kayitlar: Vec<ToplananKayit> = Vec::new();
        toplayici.satir("[2026-10-24 10:00:00] local.ERROR: Döküm", 0, &mut kayitlar);
        toplayici.satir("başı", 0, &mut kayitlar);
        toplayici.satir(&"y".repeat(BAYT), 0, &mut kayitlar);
        toplayici.satir("#0 /var/www/magaza/app/A.php(3): b()", 0, &mut kayitlar);
        toplayici.bitir(&mut kayitlar);
        assert_eq!(kayitlar[0].devam, ["başı"]);
        assert_eq!(toplayici.sayilar().devam, 3);

        // Boş satırlar bayt tutmaz ama satır sayısı da sınırlıdır
        let mut toplayici = KayitToplayici::sinirli(BAYT, SATIR);
        let mut kayitlar: Vec<ToplananKayit> = Vec::new();
        toplayici.satir(
            "[2026-10-24 10:00:00] local.ERROR: Boşluk",
            0,
            &mut kayitlar,
        );
        for _ in 0..SATIR + 5 {
            toplayici.satir("", 0, &mut kayitlar);
        }
        toplayici.bitir(&mut kayitlar);
        assert_eq!(kayitlar[0].devam.len(), SATIR);
        // Tamponlar sonraki kayıt için boşaltılır
        toplayici.satir(
            "[2026-10-24 10:00:01] local.INFO: Sonraki",
            0,
            &mut kayitlar,
        );
        toplayici.bitir(&mut kayitlar);
        assert!(kayitlar[1].devam.is_empty());
    }

    #[test]
    fn ornek_log_dogru_ayrilir() {
        let icerik = include_str!("../ornekler/laravel.log");
        let (kayitlar, sayilar) = topla(icerik);

        assert_eq!(kayitlar.len(), 59);
        assert_eq!(sayilar.kayit, 59);
        assert_eq!(sayilar.yetim, 3);
        // Her satır tam olarak bir yere gider: başlık, devam ya da yetim.
        assert_eq!(
            sayilar.kayit + sayilar.devam + sayilar.yetim,
            icerik.lines().count()
        );
        assert_eq!(sayilar.satir, icerik.lines().count());
        let hatalar = kayitlar
            .iter()
            .filter(|kayit| kayit.seviye == Seviye::Error)
            .count();
        assert_eq!(hatalar, 16);
    }

    #[test]
    fn karisik_ornek_log_dogru_ayrilir() {
        // Önce metin biçimi (12 kayıt), 10:00'dan sonra JSON (9 kayıt); yollar Windows'ta
        let icerik = include_str!("../ornekler/karisik.log");
        let (kayitlar, sayilar) = topla(icerik);

        assert_eq!(kayitlar.len(), 21);
        assert_eq!(sayilar.yetim, 0);
        assert_eq!(
            sayilar.kayit + sayilar.devam + sayilar.yetim,
            icerik.lines().count()
        );
        let json_kayitlari = icerik
            .lines()
            .filter(|satir| satir.starts_with('{'))
            .count();
        assert_eq!(json_kayitlari, 9);
        // JSON kayıtlarının devam satırı yok
        assert!(
            kayitlar
                .iter()
                .filter(|kayit| kayit.zaman.contains('T'))
                .all(|kayit| kayit.devam.is_empty())
        );
    }

    /// `Log::error($e)` ile loglanmış, önceki istisnası olan bir hata
    const ISTISNA_METNI: &str = r"[2026-10-27 14:03:11] production.ERROR: PDOException: SQLSTATE[HY000] [2002] Connection refused in /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connectors/Connector.php:66
Stack trace:
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connectors/Connector.php(66): PDO->__construct()
#1 {main}

Next Illuminate\Database\QueryException: SQLSTATE[HY000] [2002] Connection refused (Connection: mysql, SQL: select * from `urunler`) in /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php:825
Stack trace:
#0 /var/www/magaza/app/Http/Controllers/UrunController.php(20): Illuminate\Database\Eloquent\Builder->get()
#1 {main}  ";

    #[test]
    fn istisna_metninin_mesaji_distaki_istisnanin_mesajidir() {
        let (kayitlar, _) = topla(&format!(
            "{ISTISNA_METNI}\n[2026-10-27 14:03:12] production.INFO: Sonraki"
        ));
        assert_eq!(
            kayitlar[0].mesaj,
            "SQLSTATE[HY000] [2002] Connection refused (Connection: mysql, SQL: select * from `urunler`)"
        );
        assert_eq!(kayitlar[0].devam.len(), 8);
        assert_eq!(kayitlar[1].mesaj, "Sonraki");

        // Açıklamalı istisna metni; açıklama yoksa mesaj satırdan ödünç alınır
        let mut toplanan = Vec::new();
        let mut toplayici = KayitToplayici::default();
        let metin = "[2026-10-27 14:03:11] production.ERROR: Ödeme başarısız: RuntimeException: Bağlantı koptu in /var/www/magaza/app/Services/Odeme.php:12\nStack trace:\n#0 {main}";
        for satir in metin.lines() {
            toplayici.satir(satir, 0, &mut toplanan);
        }
        toplayici.bitir(&mut toplanan);
        assert_eq!(toplanan[0].mesaj, "Ödeme başarısız: Bağlantı koptu");

        struct Odunc(Vec<bool>);
        impl KayitAlici for Odunc {
            fn kayit(&mut self, kayit: &Kayit<'_>, _: KayitBilgisi) {
                self.0.push(matches!(kayit.mesaj, Cow::Borrowed(_)));
                assert!(kayit.istisna_metni().is_some());
            }
        }
        let mut odunc = Odunc(Vec::new());
        let mut toplayici = KayitToplayici::default();
        for satir in ISTISNA_METNI.lines() {
            toplayici.satir(satir, 0, &mut odunc);
        }
        toplayici.bitir(&mut odunc);
        assert_eq!(odunc.0, [true]);
    }

    #[test]
    fn json_mesajindaki_istisna_metni() {
        let mesaj = ISTISNA_METNI
            .split_once("ERROR: ")
            .unwrap()
            .1
            .replace('\\', "\\\\")
            .replace('\n', "\\n");
        let satir = format!(
            r#"{{"message":"{mesaj}","context":{{}},"level":400,"level_name":"ERROR","channel":"production","datetime":"2026-10-27T14:03:11+03:00","extra":{{}}}}"#
        );
        let kayit = json_satiri_ayristir(&satir).unwrap();
        assert_eq!(
            kayit.mesaj,
            "SQLSTATE[HY000] [2002] Connection refused (Connection: mysql, SQL: select * from `urunler`)"
        );
        let metin = kayit.istisna_metni().unwrap();
        assert_eq!(metin.sinif, r"Illuminate\Database\QueryException");
        // Toplayıcıda bekleyen JSON kaydı da aynı mesajı alır
        let (kayitlar, _) = topla(&satir);
        assert_eq!(kayitlar[0].mesaj, kayit.mesaj());
        // İstisna metni olmayan kayıtta None
        let kayit = baslik_ayristir("[2026-10-27 14:03:11] production.ERROR: x").unwrap();
        assert_eq!(kayit.istisna_metni(), None);
    }

    #[test]
    fn desenle_okunan_kayitlar() {
        let bicim = SatirBicimi::new(
            Some("%datetime% [%level_name%] %message% %context%"),
            Some(ZamanBicimi::derle("d.m.Y H:i:s").unwrap()),
        )
        .unwrap();
        let metin = [
            "27.10.2026 14:03:11 [error] Ödeme alınamadı {\"siparis\":5}",
            "  ayrıntı satırı",
            "[2026-10-27 14:03:12] production.INFO: Laravel'in biçimi artık devam satırı",
            r#"{"message":"JSON kaydı","level_name":"INFO","datetime":"2026-10-27T14:03:13+03:00"}"#,
            "27.10.2026 14:03:14 [warn] Kuyruk dolu",
        ]
        .join("\n");
        let (kayitlar, sayilar) = topla_bicimle(&metin, bicim.clone());
        let ozet: Vec<(&str, Seviye, &str, &str)> = kayitlar
            .iter()
            .map(|kayit| {
                (
                    kayit.zaman.as_str(),
                    kayit.seviye,
                    kayit.mesaj.as_str(),
                    kayit.baglam.as_str(),
                )
            })
            .collect();
        assert_eq!(
            ozet,
            [
                (
                    "2026-10-27 14:03:11",
                    Seviye::Error,
                    "Ödeme alınamadı",
                    "{\"siparis\":5}"
                ),
                ("2026-10-27T14:03:13+03:00", Seviye::Info, "JSON kaydı", ""),
                ("2026-10-27 14:03:14", Seviye::Warning, "Kuyruk dolu", ""),
            ]
        );
        assert_eq!(kayitlar[0].devam.len(), 2);
        assert_eq!(sayilar.kayit, 3);
        // Yeniden okurken de aynı biçim
        assert!(kayit_baslangici(&bicim, "27.10.2026 14:03:14 [warn] x").is_some());
        assert!(kayit_baslangici(&bicim, "[2026-10-27 14:03:12] production.INFO: x").is_none());
        // Çevrilen zaman kayda aittir
        let kayit = kayit_baslangici(&bicim, "27.10.2026 14:03:14 [warn] x").unwrap();
        assert!(matches!(kayit.zaman, Cow::Owned(_)));

        // Desene ve zamana uyan ama seviyesi tanınmayan satırlar kayıt değildir: önceki kaydın
        // devamı sayılırlar, ayrıca da sayılırlar
        let metin = "\
27.10.2026 14:03:14 [warn] Kuyruk dolu
27.10.2026 14:03:15 [uyari] Bilinmeyen
27.10.2026 14:03:16 [INF] Kısaltma tanınır
27.10.2026 14:03:17 [uyari] Bir daha";
        let (kayitlar, sayilar) = topla_bicimle(metin, bicim);
        assert_eq!(kayitlar.len(), 2);
        assert_eq!(kayitlar[1].seviye, Seviye::Info);
        assert_eq!(
            sayilar,
            SatirSayilari {
                satir: 4,
                kayit: 2,
                devam: 2,
                yetim: 0,
                taninmayan_seviye: 2,
            }
        );
    }

    #[test]
    fn dev_basligin_basi_saklanir() {
        // 3 MB'lık bir başlık: toplayıcı ilk 1 MB'ını saklar; yeniden okurken de aynısına
        // bakılır, kayıt aynıdır
        let baglam = format!("{{\"veri\":\"{}\"}}", "ğ".repeat(3 << 19));
        let satir = format!("[2026-10-24 10:00:00] local.ERROR: Dev bağlam {baglam}");
        let (kayitlar, sayilar) = topla(&format!("{satir}\nsonraki satır"));
        assert_eq!(sayilar.kayit, 1);
        let kayit = &kayitlar[0];
        assert_eq!(kayit.mesaj, "Dev bağlam");
        assert!(kayit.baglam.len() <= BASLIK_SINIRI);
        assert!(baglam.starts_with(&kayit.baglam));
        assert_eq!(kayit.devam, ["sonraki satır"]);
        let yeniden = kayit_baslangici(&SatirBicimi::default(), &satir).unwrap();
        assert_eq!(yeniden.mesaj(), kayit.mesaj);
        assert_eq!(yeniden.baglam(), kayit.baglam);
        // JSON kaydının dev mesajı da
        let json = format!(
            r#"{{"message":"{}","level_name":"INFO","datetime":"2026-10-24T10:00:00+00:00"}}"#,
            "ş".repeat(1 << 20)
        );
        let kayit = kayit_baslangici(&SatirBicimi::default(), &json).unwrap();
        assert_eq!(kayit.govde.len(), BASLIK_SINIRI);
        assert_eq!(kayit.mesaj().len(), MESAJ_SINIRI);
    }

    #[test]
    fn dev_mesajin_basi_kullanilir() {
        // İki baytlık harflerle 128 KB'lık bir mesaj: ilk 64 KB'ı kullanılır
        let mesaj = "ç".repeat(MESAJ_SINIRI);
        let satir = format!("[2026-10-24 10:00:00] local.ERROR: {mesaj} {{\"a\":1}}");
        let kayit = baslik_ayristir(&satir).unwrap();
        assert_eq!(kayit.mesaj().len(), MESAJ_SINIRI);
        assert!(mesaj.starts_with(kayit.mesaj()));
        assert_eq!(kayit.baglam(), r#"{"a":1}"#);
        // Bir harf ortasından bölünmez
        let satir = format!("[2026-10-24 10:00:00] local.ERROR: a{mesaj}");
        let kayit = baslik_ayristir(&satir).unwrap();
        assert_eq!(kayit.mesaj().len(), MESAJ_SINIRI - 1);
        assert_eq!(bayt_siniri("abç", 3), "ab");
        assert_eq!(bayt_siniri("abç", 4), "abç");
        assert_eq!(bayt_siniri("abç", 10), "abç");
        assert_eq!(bayt_siniri("", 0), "");
    }
}
