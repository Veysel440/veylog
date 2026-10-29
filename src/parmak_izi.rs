//! Hata parmak izi: istisna sınıfı, uygulama kodundaki ilk satır ve o satırın fonksiyonu.
//!
//! Laravel yakalanmayan bir istisnayı şöyle loglar (satırlar kısaltıldı):
//!
//! ```text
//! [2026-10-24 15:02:11] production.ERROR: Undefined array key "kargo_ucreti" {"userId":2210,"exception":"[object] (ErrorException(code: 0): Undefined array key \"kargo_ucreti\" at /var/www/magaza/app/Services/SepetHesaplayici.php:88)
//! [stacktrace]
//! #0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Foundation/Bootstrap/HandleExceptions.php(256): ...
//! #1 /var/www/magaza/app/Services/SepetHesaplayici.php(88): ...
//! #2 /var/www/magaza/app/Http/Controllers/SepetController.php(41): App\\Services\\SepetHesaplayici->toplam(...)
//! #3 {main}
//! "}
//! ```
//!
//! Aynı PHP mesajı (`Attempt to read property "ad" on null`) kodun birçok yerinden gelebilir.
//! Mesaja bakan Drain bunları tek kalıpta birleştirir. Parmak izi ise hatanın kodun neresinde
//! oluştuğuna bakar: istisnanın sınıfı, uygulama kodunda ilk göründüğü dosya ve o satırın
//! içinde bulunduğu fonksiyon.
//!
//! **Satır değil fonksiyon.** Satır numarası, dosyanın üst kısmına bir satır eklenince değişir:
//! bir dağıtımdan önceki ve sonraki aynı hata iki ayrı hata gibi görünür, `--karsilastir` onu
//! hem "kaybolan" hem "yeni çıkan" sayar. Bu yüzden konum dosya ve fonksiyondur (`--konum satir`
//! ile dosya ve satır). Yığın izinin her satırı bir yeri ve o yerde çağrılan fonksiyonu yazar;
//! bir yerin içinde bulunduğu fonksiyon, bir sonraki satırda çağrılan fonksiyondur:
//!
//! ```text
//! #1 /var/www/magaza/app/Services/SepetHesaplayici.php(88): ...->{closure}(...)
//! #2 /var/www/magaza/app/Http/Controllers/SepetController.php(41): App\\Services\\SepetHesaplayici->toplam(...)
//! ```
//!
//! `SepetHesaplayici.php:88`, `SepetHesaplayici->toplam()`'ın içindedir. Gösterilen satır o
//! konumdaki en yeni kaydınkidir (kod değiştiyse güncel satır); kimliğin parçası değildir.
//!
//! - **Closure.** Closure'ın adı yerini söylemez: bir sınıftaki bütün closure'lar
//!   `App\Services\SiparisServisi->{closure}` diye yazılır. Ada closure'ın tanımlandığı
//!   fonksiyon da katılır: `{closure:olustur()}`. PHP 8.4 onu closure'ın adına yazar
//!   (`{closure:App\Services\SiparisServisi::olustur():25}`). Öncesinde yığın izinden bulunur:
//!   closure çoğu zaman tanımlandığı yerden bir fonksiyona verilir
//!   (`DB::transaction(function () {...})`), o da closure'ı çağırır. Yığın izinde closure'dan
//!   dışa doğru aynı dosyadaki ilk satır böylece onu tanımlayan fonksiyonun içindedir.
//! - **En üst düzey.** Dahil edilen bir dosyanın (`routes/web.php`, `require(...)`) ya da
//!   çalıştırılan betiğin (`{main}`) en üst düzeyindeki kod bir fonksiyonun içinde değildir:
//!   konum satırla belirlenir.
//! - **Çağrısız yığın izi.** JSON'un yığın izinde çağrılar yazılmaz; yığın izi kapalıysa
//!   yalnızca fırlatıldığı yer bilinir. Aynı satır logun başka bir kaydının yığın izinde
//!   geçiyorsa fonksiyon oradan öğrenilir (`MetotHaritasi`); geçmiyorsa konum satırla
//!   belirlenir.
//!
//! Monolog'un JSON biçiminde istisna bağlamda bir nesnedir. Yığın izi (`trace`) yalnızca
//! biçimlendiricide `includeStacktraces` açıksa yazılır; kapalıysa yalnızca fırlatıldığı yer
//! bilinir:
//!
//! ```text
//! {"message":"...","context":{"exception":{"class":"Illuminate\\Database\\QueryException","message":"...","code":2002,"file":"/var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php:864","trace":["/var/www/magaza/app/Services/SiparisDeposu.php:11"]}},...}
//! ```
//!
//! İstisna bağlama konmadan metne çevrilip loglandıysa (`Log::error($e)`) PHP'nin istisna metni
//! okunur (`istisna_metni` modülü): sınıf, fırlatıldığı yer ve `Stack trace:` satırları.
//!
//! Bağlam JSON olduğu için orada `\` işareti `\\` diye yazılır: sınıf adları
//! `Illuminate\\Database\\QueryException`, Windows'ta (Herd, Laragon) yollar
//! `C:\\Users\\İrem\\Herd\\magaza\\app\\Models\\Urun.php`. PHP'nin istisna metninde ise tek `\`
//! vardır. Okunan her ad ve yol bu yüzden önce çözülür; aynı hata hangi yolla loglanmış olursa
//! olsun aynı parmak izini taşır. Konum gösterilirken ayraç `/` olur: `app/Models/Urun.php`.
//!
//! Üç durum ayrıca ele alınır:
//!
//! - **View hataları.** Blade şablonundaki bir hata `ViewException` olarak loglanır; Laravel
//!   mesajın sonuna şablonun yolunu ekler: `Undefined variable $urun (View: /var/www/magaza/resources/views/vitrin.blade.php)`.
//!   Hata derlenmiş şablonda (`storage/framework/views/8748fa....php`) oluşur; o dosyanın adı bir
//!   özettir ve satırı şablondaki satıra karşılık gelmez. Bu yüzden konum şablonun kendisidir,
//!   satır numarası olmadan: `resources/views/vitrin.blade.php`.
//! - **Middleware.** `return $next($request);` isteği bir sonraki adıma iletir; sonraki
//!   adımlarda oluşan her hata yığın izinde bu satırdan geçer. Hatanın yeri orası değildir,
//!   atlanır. Metin biçiminde yığın izinin her satırı o satırdaki çağrıyı da yazar, böylece
//!   ayırt edilebilir. JSON biçiminin yığın izinde çağrı yoktur; orada atlanamaz.
//! - **Kök.** Proje kökü vendor klasöründen bulunur; kaydın hiçbir yolunda vendor yoksa
//!   derlenmiş şablonların klasöründen (`storage/framework/views`). Sıra önemlidir: sıfır
//!   kesintili dağıtımlarda (Forge, Envoyer, Deployer) kod her dağıtımda yeni bir klasördedir
//!   (`releases/20261024090000/`), `storage` ise bütün dağıtımların paylaştığı bir klasöre
//!   bağlantıdır ve derlenmiş şablonun yolu bağlantı çözülerek yazılır. Kaydın kendi
//!   yollarında ikisi de yoksa (yığın izi olmayan ölümcül hatalar, yığın izi kapalı JSON
//!   kayıtları) logun başka kayıtlarında bulunan kökler denenir. Kayıtlar akış halinde
//!   okunduğu için böyle bir kaydın istisnası bir taslak olarak saklanır (`IstisnaTaslagi`);
//!   parmak izi, logun bütün kökleri bilinince çıkarılır.
//!
//! Derlenmiş şablonlar uygulama kodu sayılmaz. Şablondaki sıradan bir hata `ViewException` olarak
//! loglanır; derlenmiş şablonda kalan hatalar yalnızca ölümcül hatalardır (`Maximum execution
//! time of 30 seconds exceeded`) ve yerleri rastgeledir: süre o an nerede dolduysa.

use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::{BTreeSet, HashMap};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::iter;

use crate::istisna_metni::IstisnaMetni;
use crate::json_okuyucu::{Deger, metni_coz};
use crate::kayit::Kayit;
use crate::lang;

/// Her isteğin ya da komutun geçtiği giriş dosyaları. Yığın izinin en dibinde hep bunlar
/// bulunur; hatanın nerede oluştuğu hakkında bilgi vermezler. `server.php` Laravel 8 ve
/// öncesinde `php artisan serve`'ün proje kökündeki giriş dosyasıdır.
const GIRIS_NOKTALARI: [&str; 3] = ["artisan", "public/index.php", "server.php"];

/// Yoldaki vendor klasörlerinin yerleri, soldan sağa: `/vendor/` ya da `\vendor\` içindeki
/// `vendor`'ın başı. Yolun bu yere kadarki kısmı proje kökü adayıdır, sonundaki ayraçla:
/// `/var/www/magaza/vendor/laravel/...` → `/var/www/magaza/`. Ayraç Linux ve macOS'ta `/`,
/// Windows'ta `\`; iki yanındaki aynı olmalıdır.
///
/// `vendor` kelimesi `v` harfinden aranır: tek bir harfi aramak bir alt dizgi aramasının her
/// seferindeki hazırlığını gerektirmez. Her istisnalı kayıtta her çerçevenin yoluna bakıldığı
/// için fark ölçülür: yığın izli loglarda toplam sürenin beşte biri kadardı.
fn vendor_yerleri(yol: &str) -> impl Iterator<Item = usize> + '_ {
    let baytlar = yol.as_bytes();
    yol.match_indices('v').filter_map(move |(konum, _)| {
        let ayrac = *baytlar.get(konum.checked_sub(1)?)?;
        (matches!(ayrac, b'/' | b'\\')
            && baytlar[konum..].starts_with(b"vendor")
            && baytlar.get(konum + "vendor".len()) == Some(&ayrac))
        .then_some(konum)
    })
}

/// Proje kökü için denenen en fazla aday (`vendor_koku`).
const KOK_ADAYI_SINIRI: usize = 8;

/// Laravel'in derlenmiş şablonları koyduğu klasör ve ondan önceki ayraç. Kaydın hiçbir yolunda
/// vendor yoksa kök bundan bulunur.
const DERLENMIS_SABLON_KLASORLERI: [(&str, &str); 2] = [
    ("/", "/storage/framework/views/"),
    ("\\", "\\storage\\framework\\views\\"),
];

/// Köke göre yolu bunlarla başlayan dosyalar uygulama kodu değildir: Composer'ın kurduğu
/// paketler ve Blade'in derlenmiş şablonları.
const UYGULAMA_DISI: [&str; 2] = ["vendor/", "storage/framework/views/"];

/// Yığın izinde bir dosyanın dahil edildiği çağrılar. Dahil edilen dosyanın en üst düzeyindeki
/// kod bir fonksiyonun içinde değildir: yığın izinde bu çağrının içindeymiş gibi görünür.
const DAHIL_ETMELER: [&str; 4] = ["require", "require_once", "include", "include_once"];

/// Kök bilinmediğinde şablonun köke göre yolunu bulmak için aranan klasör ve ondan önceki
/// ayraç. Laravel şablonları varsayılan olarak burada tutar.
const SABLON_KLASORLERI: [(&str, &str); 2] =
    [("/", "/resources/views/"), ("\\", "\\resources\\views\\")];

/// Bir dosyadaki bir yer: `app/Services/SepetHesaplayici.php:88`. Şablonlarda satır yoktur:
/// `resources/views/vitrin.blade.php`
///
/// Dosya çözülmüş haliyle, logdaki ayraçlarıyla tutulur (Windows'ta `app\Models\Urun.php`).
/// Kaçış içermeyen yollar satırlardan ödünç alınır; parmak izinin sakladığı hali `Yer`dir.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Konum<'a> {
    dosya: Cow<'a, str>,
    satir: Option<u32>,
}

impl Konum<'_> {
    /// Parmak izinin yeri. `metot`: yerin içinde bulunduğu fonksiyon, biliniyorsa.
    fn sahiplen(&self, metot: Option<&str>, tur: KonumTuru) -> Yer {
        let metot = match tur {
            KonumTuru::Metot => metot,
            KonumTuru::Satir => None,
        };
        Yer {
            dosya: ayraci_duzelt(&self.dosya).into_owned(),
            kimlik_satiri: if metot.is_some() { None } else { self.satir },
            uye: metot.map(|metot| metot_kimligi(metot).to_owned()),
            metot: Gosterilen(metot.map(str::to_owned)),
            satir: Gosterilen(self.satir),
        }
    }
}

/// Parmak izinin konumunun neyle belirlendiği (`--konum`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum KonumTuru {
    /// Dosya ve satırın içinde bulunduğu fonksiyon: satır numaraları değişse de aynı hata
    /// aynı kalır. Fonksiyon bilinmiyorsa (JSON'un yığın izi) satır.
    #[default]
    Metot,
    /// Dosya ve satır.
    Satir,
}

impl KonumTuru {
    /// Bütün türler; yardım metni ve hata mesajları bu listeden üretilir.
    pub const HEPSI: [KonumTuru; 2] = [KonumTuru::Metot, KonumTuru::Satir];

    /// Komut satırındaki Türkçe adı.
    pub fn ad(self) -> &'static str {
        match self {
            KonumTuru::Metot => "metot",
            KonumTuru::Satir => "satir",
        }
    }

    /// Komut satırındaki İngilizce adı; JSON'da da bu yazılır.
    pub fn ingilizce_ad(self) -> &'static str {
        match self {
            KonumTuru::Metot => "method",
            KonumTuru::Satir => "line",
        }
    }

    /// Seçili dildeki adı: yardım metni ve hata mesajları için.
    pub fn gosterilen_ad(self) -> &'static str {
        lang::pick(self.ad(), self.ingilizce_ad())
    }

    /// Yardım metnindeki açıklaması.
    pub fn aciklama(self) -> &'static str {
        match self {
            KonumTuru::Metot => lang::pick(
                "dosya ve fonksiyon: satırlar kaysa da hata aynı kalır",
                "file and function: survives shifted line numbers",
            ),
            KonumTuru::Satir => lang::pick("dosya ve satır", "file and line"),
        }
    }

    /// Komut satırındaki adından (Türkçe ya da İngilizce): "satir" → KonumTuru::Satir
    pub fn ayristir(ad: &str) -> Option<KonumTuru> {
        KonumTuru::HEPSI
            .into_iter()
            .find(|tur| tur.ad() == ad || tur.ingilizce_ad() == ad)
    }
}

/// Parmak izindeki yer: gösterilecek hali, ayracı `/`. `Konum`un satırlardan bağımsız kopyası.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Yer {
    dosya: String,
    /// İçinde bulunduğu fonksiyonun kimlikteki adı (`metot_kimligi`): sınıfsız üye adı
    /// (`toplam`, `{closure:olustur()}`) ya da sınıfsız bir fonksiyonun tam adı
    /// (`App\yardimci`); konum satırla belirleniyorsa None. Sınıf kimliğe katılmaz: trait'teki
    /// bir metot yığın izinde onu kullanan sınıfın adıyla yazılır (`App\Models\Urun->kaydet`,
    /// `App\Models\Siparis->kaydet`), dosya ve satır ise aynıdır.
    uye: Option<String>,
    /// İçinde bulunduğu fonksiyonun tam adı (`App\Services\SepetHesaplayici->toplam`):
    /// kimliğe katılmaz. Trait'teki bir metotta kayıtların ilkinin sınıfıyladır.
    metot: Gosterilen<Option<String>>,
    /// Kimliğin parçası olan satır: konum satırla belirleniyorsa satır, fonksiyonla
    /// belirleniyorsa None.
    kimlik_satiri: Option<u32>,
    /// Gösterilen satır. Fonksiyonla belirlenen bir konumda karşılaştırmaya katılmaz: aynı
    /// fonksiyondaki iki satır aynı yerdir. Özette o konumdaki en yeni kaydınkidir.
    satir: Gosterilen<Option<u32>>,
}

/// `app/Services/SepetHesaplayici.php:88, toplam()`
impl fmt::Display for Yer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.dosya)?;
        if let Some(satir) = self.satir.0 {
            write!(f, ":{satir}")?;
        }
        if let Some(uye) = &self.uye {
            write!(f, ", {}", kisa_metot(uye))?;
        }
        Ok(())
    }
}

/// Yalnızca gösterilen bir değer: eşitliğe katılmaz, her değer her değere eşittir. Bir
/// `HashMap` anahtarında ilk eklenen anahtarın değeri kalır.
#[derive(Debug, Clone, Copy)]
struct Gosterilen<T>(T);

impl<T> PartialEq for Gosterilen<T> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl<T> Eq for Gosterilen<T> {}

impl<T> Hash for Gosterilen<T> {
    fn hash<H: Hasher>(&self, _: &mut H) {}
}

/// Konum adayı: istisnanın fırlatıldığı yer ya da yığın izindeki bir satır.
#[derive(Debug, Clone, PartialEq)]
struct Aday<'a> {
    konum: Konum<'a>,
    /// Satır isteği yalnızca bir sonraki adıma mı iletiyor (`return $next($request);`)?
    iletici: bool,
    /// Satırın içinde bulunduğu fonksiyonun tam adı (`kapsayan`). Satır bir dosyanın en üst
    /// düzeyindeyse ya da çağrılar bilinmiyorsa (JSON'un yığın izi) None.
    metot: Option<Cow<'a, str>>,
}

/// Yığın izinin bir satırı: yeri (`[internal function]` ve `{main}` satırlarında dosya yok) ve
/// o yerde çağrılan fonksiyon, logda yazıldığı gibi.
#[derive(Debug, Clone, PartialEq)]
struct Cerceve<'a> {
    yer: Option<Konum<'a>>,
    cagri: &'a str,
}

/// Yığın izindeki bir çağrının fonksiyonu, çözülmüş.
#[derive(Debug, Clone, PartialEq)]
enum Fonksiyon<'a> {
    /// Adı olan bir fonksiyon ya da metot: `App\Services\Sepet->toplam`, `App\Models\Urun::bul`,
    /// `App\yardimci`.
    Adli(Cow<'a, str>),
    /// Bir closure. `sinif`: bağlı olduğu sınıf ve ayraç (`App\Services\Siparis->`), yoksa
    /// boş. `sahibi`: tanımlandığı fonksiyon, PHP 8.4'ün verdiği addan okunabildiyse.
    Closure {
        sinif: Cow<'a, str>,
        sahibi: Option<Cow<'a, str>>,
    },
    /// Bir fonksiyon değil, bir dosyanın en üst düzeyi: `{main}` çalıştırılan betik,
    /// `require(...)` ve benzerleri dahil edilen dosya.
    EnUst,
}

/// Bir hatanın kimliği: hangi istisna, uygulama kodunun neresinde?
///
/// `Hash` ve `Eq` sayesinde bir HashMap'te anahtar olarak kullanılabilir: iki kayıt aynı
/// sınıftan ve aynı konumdansa parmak izleri eşittir.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParmakIzi {
    /// Sınıfın tam adı: `Illuminate\Database\QueryException`
    sinif: String,
    /// Uygulama kodundaki ilk satır. Hata tamamen vendor içinde oluştuysa None.
    konum: Option<Yer>,
}

impl ParmakIzi {
    /// Ad alanı olmadan sınıf adı: `Illuminate\Database\QueryException` → `QueryException`
    pub fn kisa_sinif(&self) -> &str {
        kisa_ad(&self.sinif)
    }

    /// Sınıfın tam adı: `Illuminate\Database\QueryException`
    pub fn sinif(&self) -> String {
        self.sinif.clone()
    }

    /// Uygulama kodundaki yer: dosya ve satır. Hata tamamen vendor içindeyse None. Dosyanın
    /// ayracı her sistemde `/`'dir: `app/Models/Urun.php`. Şablonlarda satır yoktur.
    pub fn konum(&self) -> Option<(Cow<'_, str>, Option<u32>)> {
        self.konum
            .as_ref()
            .map(|konum| (Cow::Borrowed(konum.dosya.as_str()), konum.satir.0))
    }

    /// Konumun içinde bulunduğu fonksiyonun tam adı: `App\Services\SepetHesaplayici->toplam`,
    /// `App\Services\SiparisServisi->{closure:olustur()}`. Konum satırla belirleniyorsa None.
    /// Trait'teki bir metotta, hatanın kayıtlarının ilkinde onu kullanan sınıfın adıyla.
    pub fn metot(&self) -> Option<&str> {
        self.konum.as_ref()?.metot.0.as_deref()
    }

    /// Konumun raporda gösterilen hali: `app/Services/SepetHesaplayici.php:88, toplam()`.
    /// Hata tamamen vendor içindeyse None.
    pub fn konum_metni(&self) -> Option<String> {
        self.konum.as_ref().map(ToString::to_string)
    }

    /// Gösterilen satır: konum satırla belirleniyorsa kimliğin parçası, fonksiyonla
    /// belirleniyorsa bir kaydın satırı.
    pub fn gosterilen_satir(&self) -> Option<u32> {
        self.konum.as_ref()?.satir.0
    }

    /// Aynı hatanın başka bir kaydının satırını gösterir: kimlik değişmez. Özet, bir konumda
    /// en yeni kaydın satırını gösterir.
    pub fn satiri_goster(&mut self, satir: Option<u32>) {
        if let Some(konum) = &mut self.konum {
            konum.satir = Gosterilen(satir);
        }
    }
}

/// Raporda gösterilen hali: `app/Services/SepetHesaplayici.php:88, toplam() (ErrorException)`
impl fmt::Display for ParmakIzi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.konum {
            Some(konum) => write!(f, "{konum} ({})", self.kisa_sinif()),
            None => write!(
                f,
                "{} ({})",
                lang::pick(
                    "uygulama kodunda bulunamadı",
                    "not found in application code"
                ),
                self.kisa_sinif()
            ),
        }
    }
}

/// Bir kaydın istisnasından parmak izi için okunanlar. Konuma henüz karar verilmemiştir:
/// kök gerekirse logun başka kayıtlarından gelir. Adlar ve yollar çözülmüş haldedir.
#[derive(Debug)]
pub struct Istisna<'k> {
    sinif: Cow<'k, str>,
    /// İçten dışa: önce istisnanın fırlatıldığı yer, sonra sırayla oraya giden çağrılar.
    adaylar: Vec<Aday<'k>>,
    /// İstisna bir view hatasıysa şablonun yolu.
    sablon: Option<Cow<'k, str>>,
}

impl<'k> Istisna<'k> {
    /// Kayıtta bir istisna yoksa None. Sırayla: JSON bağlamındaki istisna, metin biçimindeki
    /// bağlamın istisnası, PHP'nin istisna metni.
    pub fn kayittan(kayit: &'k Kayit<'_>) -> Option<Istisna<'k>> {
        match kayit.json_istisna() {
            Some(istisna) => Self::json_istisnasindan(istisna),
            None => {
                Self::metinden(kayit).or_else(|| kayit.istisna_metni().map(Self::php_metninden))
            }
        }
    }

    /// Metin biçimi: istisna bağlamda `"[object] (Sınıf(code: 0): ..."` diye başlar, yığın izi
    /// devam satırlarındadır. Bağlam JSON olduğu için ad ve yollar kaçışlıdır.
    fn metinden(kayit: &'k Kayit<'_>) -> Option<Istisna<'k>> {
        // İstisna bağlamda başlar; mesaj çok satırlıysa bağlam bir devam satırına kayar
        let mut satirlar =
            iter::once(kayit.baglam()).chain(kayit.devam_satirlari().iter().copied());

        // 1. İstisna başlığı: "[object] (Sınıf(code: 0): mesaj at /yol/Dosya.php:88)"
        //    `find_map` yineleyiciyi tüketmez, ödünç alır (`&mut`): bulduğu satırda durur,
        //    kalan satırlar aşağıdaki döngüye kalır.
        let (sinif, mut baslik_sonu) =
            satirlar.find_map(|satir| sinif_bul(satir).map(|sinif| (sinif, satir)))?;

        // 2. Başlık "[stacktrace]" satırına kadar sürer: istisna mesajı da çok satırlı
        //    olabilir ve fırlatıldığı yer başlığın son satırının sonundadır. Ardından yığın
        //    izinin çerçeveleri gelir. "[previous exception]" ile başlayan kısım bu istisnaya
        //    yol açan önceki istisnadır; onun izi bu hatanın yerini değiştirmez.
        let mut cerceveler = Vec::new();
        let mut yigin_izinde = false;
        for satir in satirlar {
            let kirpilmis = satir.trim();
            if kirpilmis.starts_with("[previous exception]") {
                break;
            } else if kirpilmis == "[stacktrace]" {
                yigin_izinde = true;
            } else if yigin_izinde {
                cerceveler.extend(cerceve_ayristir(kirpilmis));
            } else {
                // Kırpılmamış hali saklanır: " at /yol/Dosya.php:88)" satırındaki baştaki
                // boşluk, " at " aramasının parçasıdır
                baslik_sonu = satir;
            }
        }

        // 3. Adaylar içten dışa sıralı: önce istisnanın fırlatıldığı yer, sonra sırayla
        //    oraya giden çağrılar
        let sinif = metni_coz(sinif);
        // View hatasında şablon, mesajın son satırındadır; o da başlığın son satırıdır
        let sablon = sablon_yolu(baslik_sonu)
            .filter(|_| view_hatasi_mi(&sinif))
            .map(metni_coz);

        Some(Istisna {
            sinif,
            adaylar: adaylari_kur(firlatildigi_yer(baslik_sonu), &cerceveler, true),
            sablon,
        })
    }

    /// JSON biçimi: istisna bir nesnedir. Sınıf `class`'tan, fırlatıldığı yer `file`'dan
    /// (`/yol/X.php:26`), çağrılar varsa `trace` dizisinden okunur. Önceki istisna
    /// (`previous`) metin biçimindeki gibi okunmaz.
    fn json_istisnasindan(istisna: &'k Deger<'_>) -> Option<Istisna<'k>> {
        let sinif = istisna
            .alan("class")?
            .ham_metin()
            .filter(|sinif| sinif_gecerli(sinif))?;
        let yer = istisna.alan("file").and_then(Deger::ham_metin);
        let cagrilar = istisna
            .alan("trace")
            .and_then(Deger::ogeler)
            .unwrap_or_default()
            .iter()
            .filter_map(Deger::ham_metin);
        // JSON'un yığın izinde çağrılar yazılmaz; iletici satırlar ayırt edilemez
        let adaylar = yer
            .into_iter()
            .chain(cagrilar)
            .filter_map(json_yeri)
            .map(|konum| Aday {
                konum,
                iletici: false,
                metot: None,
            })
            .collect();
        let sinif = metni_coz(sinif);
        let sablon = istisna
            .alan("message")
            .and_then(Deger::ham_metin)
            .and_then(sablon_yolu)
            .filter(|_| view_hatasi_mi(&sinif))
            .map(metni_coz);

        Some(Istisna {
            sinif,
            adaylar,
            sablon,
        })
    }

    /// PHP'nin istisna metni (`Log::error($e)`): kaçış yoktur, adlar ve yollar olduğu gibi
    /// alınır.
    fn php_metninden(metin: IstisnaMetni<'k>) -> Istisna<'k> {
        let firlatildigi = Konum {
            dosya: Cow::Borrowed(metin.dosya),
            satir: Some(metin.satir),
        };
        let cerceveler: Vec<Cerceve<'k>> = metin
            .cerceveler
            .iter()
            .filter_map(|satir| cerceve_ayristir(satir))
            .collect();
        let sablon = sablon_yolu(metin.baslik_sonu)
            .filter(|_| view_hatasi_mi(metin.sinif))
            .map(Cow::Borrowed);
        Istisna {
            sinif: Cow::Borrowed(metin.sinif),
            adaylar: adaylari_kur(Some(firlatildigi), &cerceveler, false),
            sablon,
        }
    }

    /// Kaydın kendi yollarından bulunan proje kökü: vendor klasöründen önceki kısım,
    /// "/var/www/magaza/" (`vendor_koku`). Hiçbir yolda vendor yoksa derlenmiş şablonların
    /// klasöründen. Derlenmiş şablon yığın izinde vendor'dan önce gelse de önce bütün yollarda
    /// vendor aranır: paylaşılan `storage` klasörü dağıtımın klasörünün dışında olabilir. Yollar
    /// zaten köke göre yazılmışsa ("vendor/laravel/...") kök yoktur, gerek de yoktur.
    pub fn kok(&self) -> Option<&str> {
        vendor_koku(self.adaylar.iter().map(|aday| &*aday.konum.dosya)).or_else(|| {
            self.adaylar
                .iter()
                .find_map(|aday| klasorden_kok(&aday.konum.dosya, &DERLENMIS_SABLON_KLASORLERI))
        })
    }

    /// Bilinen köklerden kaydın yollarından birinin başında olanı. Birden çoğu uyuyorsa en
    /// uzunu: iç içe iki projede içteki kök daha doğrudur. Uzunluklar eşitse sıralamada
    /// sonuncusu.
    ///
    /// Kökler bir ayraçla biter (`/var/www/magaza/`): her yolun ayraçla biten başları
    /// köklerde aranır. Her kökü her yolla karşılaştırmak, binlerce kökü (sıfır kesintili
    /// dağıtımların klasörleri) ve binlerce köksüz istisnası olan bir logda süreyi ikisinin
    /// çarpımıyla büyütürdü.
    fn bilinen_kok<'b>(&self, kokler: &'b BTreeSet<String>) -> Option<&'b str> {
        let mut bulunan: Option<&'b str> = None;
        for aday in &self.adaylar {
            let dosya: &str = &aday.konum.dosya;
            for (konum, ayrac) in dosya.match_indices(['/', '\\']) {
                let Some(kok) = kokler.get(&dosya[..konum + ayrac.len()]) else {
                    continue;
                };
                if bulunan.is_none_or(|onceki| (kok.len(), kok.as_str()) > (onceki.len(), onceki)) {
                    bulunan = Some(kok);
                }
            }
        }
        bulunan
    }

    /// Kaydın proje kökü: kendi yollarında bulunan ya da yoksa `kokler`den biri (logun bütün
    /// kayıtlarında bulunan kökler).
    pub fn cozulmus_kok<'b>(&'b self, kokler: &'b BTreeSet<String>) -> Option<&'b str> {
        self.kok().or_else(|| self.bilinen_kok(kokler))
    }

    /// Testlerde parmak izi. `kokler`: logun bütün kayıtlarında bulunan kökler; kaydın kendi
    /// kökü yoksa denenir. Depo kökü bir kez bulup `koke_gore`yi çağırır.
    #[cfg(test)]
    pub fn parmak_izi(&self, kokler: &BTreeSet<String>, tur: KonumTuru) -> ParmakIzi {
        self.koke_gore(self.cozulmus_kok(kokler), tur)
    }

    /// Verilen köke göre parmak izi. Kaydın kendi kökü varsa (`kok`) parmak izi başka
    /// kayıtlara bakmadan böyle bulunur.
    pub fn koke_gore(&self, kok: Option<&str>, tur: KonumTuru) -> ParmakIzi {
        // View hatasında konum şablondur; şablon uygulamada değilse (bir paketin şablonu)
        // yığın izine bakılır
        let sablon = self
            .sablon
            .as_deref()
            .and_then(|sablon| sablon_konumu(sablon, kok));
        let konum = match sablon {
            Some(sablon) => Some(sablon.sahiplen(None, tur)),
            None => uygulamadaki_ilk(&self.adaylar, kok)
                .map(|(konum, metot)| konum.sahiplen(metot, tur)),
        };
        ParmakIzi {
            sinif: self.sinif.to_string(),
            konum,
        }
    }

    /// İstisnanın satırlardan bağımsız bir kopyası; parmak izi sonradan çıkarılabilsin.
    pub fn taslak(&self) -> IstisnaTaslagi {
        IstisnaTaslagi {
            sinif: self.sinif.to_string(),
            adaylar: self
                .adaylar
                .iter()
                .map(|aday| TaslakAdayi {
                    dosya: aday.konum.dosya.to_string(),
                    satir: aday.konum.satir,
                    iletici: aday.iletici,
                    metot: aday.metot.as_deref().map(str::to_owned),
                })
                .collect(),
            sablon: self.sablon.as_deref().map(str::to_owned),
        }
    }
}

/// Kökü kendi yollarında bulunamayan bir kaydın istisnası. Kayıtlar akış halinde okunurken
/// logun bütün kökleri henüz bilinmez; parmak izi okuma bitince çıkarılır. Aynı taslaklar
/// bir kez saklansın diye `Hash` ve `Eq` taşır.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IstisnaTaslagi {
    sinif: String,
    adaylar: Vec<TaslakAdayi>,
    sablon: Option<String>,
}

/// Taslaktaki bir aday: `Aday`ın satırlardan bağımsız kopyası.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TaslakAdayi {
    /// Çözülmüş haliyle
    dosya: String,
    satir: Option<u32>,
    iletici: bool,
    metot: Option<String>,
}

impl IstisnaTaslagi {
    /// Taslağın istisnası, taslaktan ödünç alarak.
    pub fn istisna(&self) -> Istisna<'_> {
        Istisna {
            sinif: Cow::Borrowed(&self.sinif),
            adaylar: self
                .adaylar
                .iter()
                .map(|aday| Aday {
                    konum: Konum {
                        dosya: Cow::Borrowed(&aday.dosya),
                        satir: aday.satir,
                    },
                    iletici: aday.iletici,
                    metot: aday.metot.as_deref().map(Cow::Borrowed),
                })
                .collect(),
            sablon: self.sablon.as_deref().map(Cow::Borrowed),
        }
    }
}

/// Yığın izlerinden öğrenilen: uygulama kodunun hangi satırı hangi fonksiyonun içinde.
///
/// JSON'un yığın izinde çağrılar yazılmaz; yığın izi kapalıysa yalnızca fırlatıldığı yer
/// bilinir. Böyle bir kaydın konumu satırla belirlenir. Aynı satır logun başka bir kaydının
/// yığın izinde geçiyorsa (log biçimi değiştirilmiş, bir kanal JSON yazıyor) fonksiyonu
/// bilinir: konum fonksiyonla belirlenir, kayıt aynı hatanın metin biçimindeki kayıtlarıyla
/// birleşir.
///
/// Log bir dağıtımı aşıyorsa bir satır, kod değiştiği için başka bir fonksiyona kaymış olabilir.
/// İki farklı fonksiyonda görülen satır belirsizdir, kullanılmaz.
#[derive(Debug, Clone, Default)]
pub struct MetotHaritasi {
    /// Dosya, parmak izindeki gibi köke göre ve ayracı `/` → satır → fonksiyonun tam adı;
    /// satır belirsizse None.
    dosyalar: HashMap<Box<str>, HashMap<u32, Option<Box<str>>>>,
}

impl MetotHaritasi {
    /// İstisnanın yığın izinden öğrenir. `kok`: kaydın proje kökü.
    pub fn ogren(&mut self, istisna: &Istisna<'_>, kok: Option<&str>) {
        for aday in &istisna.adaylar {
            let (Some(metot), Some(satir)) = (aday.metot.as_deref(), aday.konum.satir) else {
                continue;
            };
            let Some(dosya) = uygulama_yolu(&aday.konum.dosya, kok) else {
                continue;
            };
            let dosya = ayraci_duzelt(dosya);
            // Dosya çoğu zaman bilinir: tek arama
            match self.dosyalar.get_mut(&*dosya) {
                Some(satirlar) => satira_yaz(satirlar, satir, metot),
                None => {
                    let mut satirlar = HashMap::new();
                    satira_yaz(&mut satirlar, satir, metot);
                    self.dosyalar.insert(Box::from(&*dosya), satirlar);
                }
            }
        }
    }

    /// Satırın içinde bulunduğu fonksiyon; bilinmiyorsa ya da belirsizse None.
    fn bul(&self, dosya: &str, satir: u32) -> Option<&str> {
        self.dosyalar.get(dosya)?.get(&satir)?.as_deref()
    }

    /// Parmak izinin konumu satırla belirlenmişse ve satırın fonksiyonu biliniyorsa konumu
    /// fonksiyonla belirler. Fonksiyonla belirlenmiş konumlar ve şablonlar (satırları yok)
    /// değişmez.
    pub fn tamamla(&self, iz: &mut ParmakIzi) {
        let Some(konum) = &mut iz.konum else { return };
        let (None, Some(satir)) = (&konum.uye, konum.kimlik_satiri) else {
            return;
        };
        if let Some(metot) = self.bul(&konum.dosya, satir) {
            konum.uye = Some(metot_kimligi(metot).to_owned());
            konum.metot = Gosterilen(Some(metot.to_owned()));
            konum.kimlik_satiri = None;
        }
    }
}

/// Satırın fonksiyonunu dosyanın satırlarına yazar. Satır başka bir fonksiyonda görülmüşse
/// belirsizdir: None. Fonksiyonlar kimlikleriyle karşılaştırılır: trait'teki bir satır onu
/// kullanan her sınıfın adıyla görülür, yine de aynı fonksiyondadır.
fn satira_yaz(satirlar: &mut HashMap<u32, Option<Box<str>>>, satir: u32, metot: &str) {
    match satirlar.entry(satir) {
        Entry::Vacant(bos) => {
            bos.insert(Some(Box::from(metot)));
        }
        Entry::Occupied(mut dolu) => {
            if dolu
                .get()
                .as_deref()
                .is_some_and(|bilinen| metot_kimligi(bilinen) != metot_kimligi(metot))
            {
                dolu.insert(None);
            }
        }
    }
}

/// Adaylardan uygulama kodundaki ilki ve içinde bulunduğu fonksiyon; iletici satırlar atlanır.
fn uygulamadaki_ilk<'b>(
    adaylar: &'b [Aday<'_>],
    kok: Option<&str>,
) -> Option<(Konum<'b>, Option<&'b str>)> {
    adaylar
        .iter()
        .filter(|aday| !aday.iletici)
        .find_map(|aday| {
            let konum = Konum {
                dosya: Cow::Borrowed(uygulama_yolu(&aday.konum.dosya, kok)?),
                satir: aday.konum.satir,
            };
            Some((konum, aday.metot.as_deref()))
        })
}

/// Yol uygulama kodundaysa köke göre hali, ayraçları logdaki gibi. Köke göre yolu "vendor/"
/// ya da "storage/framework/views/" ile başlayan dosyalar ve giriş noktaları uygulama kodu
/// değildir. Kök verildiyse onun dışındaki yollar (`strip_prefix` None verir) da değildir.
fn uygulama_yolu<'b>(dosya: &'b str, kok: Option<&str>) -> Option<&'b str> {
    let dosya = match kok {
        Some(kok) => dosya.strip_prefix(kok)?,
        None => dosya,
    };
    let yol = ayraci_duzelt(dosya);
    let uygulamada = !UYGULAMA_DISI.iter().any(|klasor| yol.starts_with(klasor))
        && !GIRIS_NOKTALARI.contains(&&*yol);
    uygulamada.then_some(dosya)
}

/// Yolların proje kökü: vendor klasöründen önceki kısım, sonundaki ayraçla
/// (`/var/www/magaza/vendor/laravel/...` → `/var/www/magaza/`). Yollarda vendor yoksa None.
///
/// Yollarda birden çok vendor geçebilir: proje `vendor` adlı bir klasörün içinde olabilir
/// (`/home/ali/vendor/magaza/vendor/laravel/...`) ya da bir paket kendi vendor klasörünü
/// taşıyabilir (`/var/www/magaza/vendor/acme/paket/vendor/...`). Her vendor'dan önceki kısım
/// bir adaydır; kök, hemen altında bir giriş noktası (`public/index.php`, `artisan`) olan
/// aday, yoksa altında uygulama kodu olan en kısa aday, o da yoksa en kısa adaydır. Adayı
/// tek olan yollarda (hemen hepsi) bunlara bakılmaz.
///
/// Yollarda ilk görülen `KOK_ADAYI_SINIRI` aday denenir: gerçek yığın izlerinde aday birkaç
/// tanedir, her biri yolların çoğunda geçer. Sınır olmasa her çerçevesi başka bir köke sahip
/// dev bir yığın izi (bir milyon çerçeveye kadar saklanır) her adayı her yolla karşılaştırtır,
/// süre çerçeve sayısının karesiyle artardı.
fn vendor_koku<'b>(yollar: impl Iterator<Item = &'b str> + Clone) -> Option<&'b str> {
    let mut adaylar: Vec<&'b str> = Vec::new();
    'yollar: for yol in yollar.clone() {
        for son in vendor_yerleri(yol) {
            let kok = &yol[..son];
            if !adaylar.contains(&kok) {
                if adaylar.len() == KOK_ADAYI_SINIRI {
                    break 'yollar;
                }
                adaylar.push(kok);
            }
        }
    }
    if adaylar.len() <= 1 {
        return adaylar.first().copied();
    }
    adaylar.sort_by_key(|kok| kok.len());
    // Kökün altındaki bir yolun köke göre hali bir koşula uyuyor mu?
    let altinda = |kok: &str, kosul: fn(&str) -> bool| {
        yollar.clone().any(|yol| {
            yol.strip_prefix(kok)
                .is_some_and(|kalan| kosul(&ayraci_duzelt(kalan)))
        })
    };
    let giris_noktasi = |yol: &str| GIRIS_NOKTALARI.contains(&yol);
    let uygulama_kodu = |yol: &str| {
        !UYGULAMA_DISI.iter().any(|klasor| yol.starts_with(klasor))
            && !GIRIS_NOKTALARI.contains(&yol)
    };
    adaylar
        .iter()
        .find(|kok| altinda(kok, giris_noktasi))
        .or_else(|| adaylar.iter().find(|kok| altinda(kok, uygulama_kodu)))
        .or(adaylar.first())
        .copied()
}

/// Yol bir vendor klasöründe mi? Proje kökü bilinmeden de söylenebilir: yolda `/vendor/`
/// geçer ya da yol köke göre yazılmışsa (`vendor/laravel/...`) onunla başlar. Vendor'daki bir
/// yer parmak izinin konumu olamaz; içinde bulunduğu fonksiyon da aranmaz.
fn vendor_yolu_mu(dosya: &str) -> bool {
    vendor_yerleri(dosya).next().is_some()
        || dosya.starts_with("vendor/")
        || dosya.starts_with("vendor\\")
}

/// Şablonun köke göre yolu, satırsız. Kök bilinmiyorsa şablon klasöründen (`resources/views`)
/// bulunur; o da yoksa yol olduğu gibi kalır. Şablon bir vendor klasöründeyse (bir paketin
/// şablonu) uygulama kodu değildir: None.
fn sablon_konumu<'b>(sablon: &'b str, kok: Option<&str>) -> Option<Konum<'b>> {
    if vendor_yolu_mu(sablon) {
        return None;
    }
    let dosya = match kok {
        Some(kok) => sablon.strip_prefix(kok)?,
        None => sablon_klasorunden(sablon).unwrap_or(sablon),
    };
    Some(Konum {
        dosya: Cow::Borrowed(dosya),
        satir: None,
    })
}

/// İstisna bir view hatası mı? Laravel'inki `Illuminate\View\ViewException`; Ignition kendi
/// sınıfıyla sarar ama adı aynıdır.
fn view_hatasi_mi(sinif: &str) -> bool {
    kisa_ad(sinif) == "ViewException"
}

/// Ad alanı olmadan sınıf adı: `Illuminate\Database\QueryException` → `QueryException`
fn kisa_ad(sinif: &str) -> &str {
    sinif.rsplit_once('\\').map_or(sinif, |(_, kisa)| kisa)
}

/// Mesajdaki ilk `(View: ...)` içindeki şablon yolu. Şablon başka bir şablonun içindeyse
/// Laravel her düzey için bir tane ekler; ilki hatanın oluştuğu şablondur.
///
/// Yol `.php)` ile biter (Blade şablonları `.blade.php`), bu yüzden klasör adlarındaki
/// parantezler yolu bölmez.
fn sablon_yolu(metin: &str) -> Option<&str> {
    let (_, sonrasi) = metin.split_once("(View: ")?;
    let son = sonrasi.find(".php)")?;
    let yol = &sonrasi[..son + ".php".len()];
    // `(View: )` gibi bozuk bir ek bir sonrakinin yoluna taşmasın
    (!yol.contains("(View: ")).then_some(yol)
}

/// İstisna başlığından sınıf adını alır, logda yazıldığı gibi (kaçışlı):
/// `{"exception":"[object] (ErrorException(code: 0): ...` → `ErrorException`
fn sinif_bul(satir: &str) -> Option<&str> {
    let (_, sonrasi) = satir.split_once("[object] (")?;
    let (sinif, _) = sonrasi.split_once("(code: ")?;
    sinif_gecerli(sinif).then_some(sinif)
}

/// PHP sınıf adı harf, rakam, _ ve ad alanı ayracı \ içerir; anonim sınıflar
/// `class@anonymous` diye yazılır. Başka bir karakter varsa bu bir sınıf adı değildir.
fn sinif_gecerli(sinif: &str) -> bool {
    !sinif.is_empty()
        && sinif
            .chars()
            .all(|harf| harf.is_alphanumeric() || matches!(harf, '_' | '\\' | '@'))
}

/// Başlığın sonundaki `at /yol/Dosya.php:88)` kısmından istisnanın fırlatıldığı yeri alır.
/// Yol bağlamın JSON'unda yazıldığı için kaçışlıdır; çözülür.
///
/// Yol parantez (`C:\\Program Files (x86)\\...`) ve ` at ` (`/home/ali/is at ev/...`)
/// içerebilir; mesaj da ` at ` içerebilir. Bu yüzden her ` at ` sağdan denenir: ardından
/// `dosya:satır)` gelenlerden yolu mutlak olan (`/`, `\`, sürücü harfi) sağdaki ilki seçilir;
/// hiçbirinin yolu mutlak değilse sağdaki ilki. Bir ` at `'in yeri, ondan sonraki ilk
/// `:satır)`'dır.
///
/// Satırdaki `:satır)`'lar önce bir kez, soldan sağa bulunur; her ` at ` için ardındakilerin
/// ilki ikili aramayla seçilir. Her ` at `'te satırın geri kalanını yeniden taramak, ` at ` ve
/// `)` ile dolu dev bir istisna mesajında (ör. 50 KB'lık ` at x)` tekrarı) programı dakikalarca
/// meşgul ederdi.
fn firlatildigi_yer(satir: &str) -> Option<Konum<'_>> {
    // `:satır)` biçimindeki bitişler: iki noktanın yeri ve satır numarası. Her `)`'ten geriye
    // yalnızca hemen önündeki rakamlara bakılır; bir rakam dizisi tek bir `)`'in önündedir,
    // bu yüzden bütün bakılanlar satırın boyu kadardır.
    let baytlar = satir.as_bytes();
    let bitisler: Vec<(usize, u32)> = satir
        .match_indices(')')
        .filter_map(|(son, _)| {
            let rakamlar = baytlar[..son]
                .iter()
                .rev()
                .take_while(|bayt| bayt.is_ascii_digit())
                .count();
            let iki_nokta = son.checked_sub(rakamlar + 1)?;
            if rakamlar == 0 || baytlar[iki_nokta] != b':' {
                return None;
            }
            Some((iki_nokta, satir[iki_nokta + 1..son].parse().ok()?))
        })
        .collect();
    let mut ilk = None;
    for (konum, _) in satir.rmatch_indices(" at ") {
        let bas = konum + " at ".len();
        // ` at `'ten sonraki ilk bitiş: iki noktalar da `)`'ler gibi soldan sağa sıralıdır
        let sira = bitisler.partition_point(|&(iki_nokta, _)| iki_nokta < bas);
        let Some(&(iki_nokta, satir_no)) = bitisler.get(sira) else {
            continue;
        };
        let dosya = &satir[bas..iki_nokta];
        let mutlak = dosya.starts_with(['/', '\\'])
            || dosya
                .as_bytes()
                .get(..2)
                .is_some_and(|bas| bas[0].is_ascii_alphabetic() && bas[1] == b':');
        if mutlak {
            ilk = Some((dosya, satir_no));
            break;
        }
        ilk.get_or_insert((dosya, satir_no));
    }
    let (dosya, satir_no) = ilk?;
    Some(Konum {
        dosya: metni_coz(dosya),
        satir: Some(satir_no),
    })
}

/// Yığın izindeki bir satırı ayrıştırır:
/// `#2 /yol/SepetController.php(41): App\\...->toplam(...)` → yer `/yol/SepetController.php:41`,
/// çağrı `App\\...->toplam(...)`. Dosyası olmayan çerçevelerin (`#5 [internal function]: ...`,
/// `#9 {main}`) yalnızca çağrısı vardır. Yol logdaki gibidir; çözülmesi `adaylari_kur`un
/// işidir.
fn cerceve_ayristir(satir: &str) -> Option<Cerceve<'_>> {
    let (_, geri) = satir.strip_prefix('#')?.split_once(' ')?; // "/yol/X.php(41): ..."
    if let Some(cagri) = geri.strip_prefix("[internal function]: ") {
        return Some(Cerceve { yer: None, cagri });
    }
    // Son satırın arkasında bağlam olabilir: `#3 {main} {"kullanici":5}`
    if geri.starts_with("{main}") {
        return Some(Cerceve {
            yer: None,
            cagri: "{main}",
        });
    }
    let (yer, cagri) = geri.split_once("): ")?; // "/yol/X.php(41" ve "App\\...->toplam(...)"
    let (dosya, satir_no) = yer.rsplit_once('(')?; // "/yol/X.php" ve "41"
    Some(Cerceve {
        yer: Some(Konum {
            dosya: Cow::Borrowed(dosya),
            satir: Some(satir_no.parse().ok()?),
        }),
        cagri,
    })
}

/// Fırlatıldığı yerden ve yığın izinin satırlarından konum adayları, içten dışa.
///
/// Bir yerin içinde bulunduğu fonksiyon, yığın izinde o yerden sonraki çerçevenin çağrısıdır
/// (`kapsayan`). Fırlatıldığı yer çoğu zaman yığın izinde yoktur; o zaman ilk çerçevenin
/// çağrısıdır. PHP'nin bir uyarısından doğan `ErrorException`'da ise yer bir çerçevenin yeridir
/// (hata işleyicisi oradan çağrılmış gibi görünür): fonksiyonu da o çerçeveninkidir.
///
/// Fonksiyonlar dıştan içe bulunur: bir closure'ın sahibi daha dıştaki bir çerçeveden
/// öğrenilir. Vendor'daki çerçevelerin fonksiyonu aranmaz; vendor'daki bir yer konum olamaz.
///
/// `kacisli`: satırlar bağlamın JSON'unda mı yazıldı (Laravel'in `[stacktrace]`'i)? Öyleyse
/// yollar ve fonksiyon adları çözülür; PHP'nin istisna metninde olduğu gibi alınır.
/// Fırlatıldığı yer zaten çözülmüş gelir.
fn adaylari_kur<'k>(
    firlatildigi: Option<Konum<'k>>,
    cerceveler: &[Cerceve<'k>],
    kacisli: bool,
) -> Vec<Aday<'k>> {
    let yerler: Vec<Option<Konum<'k>>> = cerceveler
        .iter()
        .map(|cerceve| {
            cerceve.yer.as_ref().map(|yer| Konum {
                dosya: match &yer.dosya {
                    Cow::Borrowed(dosya) => cozulmus(dosya, kacisli),
                    dosya => dosya.clone(),
                },
                satir: yer.satir,
            })
        })
        .collect();

    // Vendor'daki çerçevelerin fonksiyonu aranmaz. Yolların çoğu aynı kökten başlar: kök bir kez
    // bulunur ve yollar önce onunla karşılaştırılır; her yolda alt dizgi aranmaz. Yollar köke
    // göre yazılmışsa (`vendor/laravel/...`) kök boştur.
    let onek = match yerler
        .iter()
        .flatten()
        .find(|yer| vendor_yolu_mu(&yer.dosya))
    {
        Some(yer) if yer.dosya.starts_with("vendor/") || yer.dosya.starts_with("vendor\\") => {
            Some("")
        }
        Some(_) => vendor_koku(yerler.iter().flatten().map(|yer| &*yer.dosya)),
        None => None,
    };
    let vendorda = |dosya: &str| match onek {
        Some(onek) if dosya.starts_with(onek) => {
            let kalan = &dosya[onek.len()..];
            kalan.starts_with("vendor/") || kalan.starts_with("vendor\\")
        }
        _ => vendor_yolu_mu(dosya),
    };

    // `icinde[i]`: i. çerçevenin yerinin içinde bulunduğu fonksiyon
    let mut icinde: Vec<Option<Cow<'k, str>>> = vec![None; cerceveler.len()];
    // Dosya → o dosyadaki çerçevelerden şimdiye kadar bakılanların en içteki. Dıştan içe
    // gidildiği için bir çerçeveye gelindiğinde, aynı dosyada dışa doğru ilk çerçevedir.
    let mut disaridaki: HashMap<&str, usize> = HashMap::new();
    for (sira, yer) in yerler.iter().enumerate().rev() {
        let Some(yer) = yer else { continue };
        if vendorda(&yer.dosya) {
            continue;
        }
        // Dıştaki çerçevelerin fonksiyonları bulundu; `dis` (sira + 1). çerçeveden başlar
        let (ic, dis) = icinde.split_at_mut(sira + 1);
        let sahibi = disaridaki
            .insert(&yer.dosya, sira)
            .and_then(|onceki| dis[onceki - sira - 1].as_deref());
        ic[sira] = kapsayan(cerceveler.get(sira + 1), sahibi, kacisli);
    }

    let mut adaylar = Vec::with_capacity(yerler.len() + 1);
    if let Some(konum) = firlatildigi {
        let metot = if vendorda(&konum.dosya) {
            None
        } else {
            match yerler.iter().position(|yer| yer.as_ref() == Some(&konum)) {
                Some(sira) => icinde[sira].clone(),
                None => {
                    let sahibi = yerler
                        .iter()
                        .position(|yer| yer.as_ref().is_some_and(|yer| yer.dosya == konum.dosya))
                        .and_then(|sira| icinde[sira].as_deref());
                    kapsayan(cerceveler.first(), sahibi, kacisli)
                }
            }
        };
        adaylar.push(Aday {
            konum,
            iletici: false,
            metot,
        });
    }
    for ((cerceve, yer), metot) in cerceveler.iter().zip(yerler).zip(icinde) {
        if let Some(konum) = yer {
            adaylar.push(Aday {
                konum,
                iletici: iletici_mi(cerceve.cagri),
                metot,
            });
        }
    }
    adaylar
}

/// Logda yazıldığı gibi bir ad ya da yol, çözülmüş. `kacisli`: bağlamın JSON'unda mı yazıldı?
fn cozulmus(metin: &str, kacisli: bool) -> Cow<'_, str> {
    if kacisli {
        metni_coz(metin)
    } else {
        Cow::Borrowed(metin)
    }
}

/// Bir yerin içinde bulunduğu fonksiyonun tam adı. `cagiran`: yığın izinde yerden sonraki
/// çerçeve; onun çağrısı yerin içinde bulunduğu fonksiyondur. Yer bir dosyanın en üst
/// düzeyindeyse ya da yığın izi orada bittiyse None.
///
/// Fonksiyon bir closure'sa adına sahibi de katılır. PHP 8.4'te sahibi closure'ın adındadır;
/// öncesinde `disaridaki`dir: aynı dosyada, yığın izinde dışa doğru ilk çerçevenin içinde
/// bulunduğu fonksiyon.
fn kapsayan<'k>(
    cagiran: Option<&Cerceve<'k>>,
    disaridaki: Option<&str>,
    kacisli: bool,
) -> Option<Cow<'k, str>> {
    match fonksiyon(cagiran?.cagri, kacisli)? {
        Fonksiyon::Adli(ad) => Some(ad),
        Fonksiyon::EnUst => None,
        Fonksiyon::Closure { sinif, sahibi } => {
            let sahibi = sahibi.as_deref().or_else(|| disaridaki.and_then(sahip_adi));
            Some(Cow::Owned(closure_adi(&sinif, sahibi)))
        }
    }
}

/// Bir çağrının fonksiyonu; argümanlar atılır.
///
/// - `App\Services\Sepet->toplam(Object(...))` → `App\Services\Sepet->toplam`
/// - `App\Services\Siparis->App\Services\{closure}(...)` (PHP 8.3) ve
///   `App\Services\Siparis->{closure:App\Services\Siparis::olustur():25}(...)` (PHP 8.4):
///   sınıfı `App\Services\Siparis->` olan bir closure; 8.4'te sahibi de bilinir: `olustur`
/// - `{main}`, `require('/var/www/...')` → en üst düzey
///
/// Ad, süslü parantezlerin dışındaki ilk `(`'e kadardır: PHP 8.4'te closure'ın adı parantez
/// içerir. `kacisli`: çağrı bağlamın JSON'unda yazıldıysa ters bölüler çözülür.
fn fonksiyon(cagri: &str, kacisli: bool) -> Option<Fonksiyon<'_>> {
    let son = parantez_disinda(cagri, |kalan| kalan.starts_with('(')).unwrap_or(cagri.len());
    let ad = cagri[..son].trim();
    if ad.is_empty() {
        return None;
    }
    if ad == "{main}" || DAHIL_ETMELER.contains(&ad) {
        return Some(Fonksiyon::EnUst);
    }
    // Sınıf ile üyeyi ayıran `->` ya da `::`, süslü parantezlerin dışında
    let ayrac = parantez_disinda(ad, |kalan| {
        kalan.starts_with("->") || kalan.starts_with("::")
    });
    let (sinif, uye) = match ayrac {
        Some(konum) => ad.split_at(konum + 2),
        None => ("", ad),
    };
    Some(match closure_konumu(uye) {
        Some(konum) => Fonksiyon::Closure {
            sinif: cozulmus(sinif, kacisli),
            sahibi: closure_sahibi(&uye[konum..]).map(|sahibi| cozulmus(sahibi, kacisli)),
        },
        None => Fonksiyon::Adli(cozulmus(ad, kacisli)),
    })
}

/// PHP 8.4'ün closure adından closure'ın tanımlandığı fonksiyon:
/// `{closure:App\X::olustur():25}` → `olustur`, `{closure:App\yardimci():5}` → `App\yardimci`.
/// İç içe closure'da dıştakinin sahibi: `{closure:{closure:App\X::olustur():25}:26}` →
/// `olustur`. Closure bir dosyanın en üst düzeyinde tanımlandıysa
/// (`{closure:/var/www/magaza/routes/web.php:21}`) ya da ad PHP 8.4'ten eskiyse
/// (`App\Services\{closure}`) None.
fn closure_sahibi(uye: &str) -> Option<&str> {
    let ic = uye.strip_prefix("{closure:")?.strip_suffix('}')?; // "App\X::olustur():25"
    let (sahibi, satir) = ic.rsplit_once(':')?; // "App\X::olustur()" ve "25"
    if satir.is_empty() || !satir.bytes().all(|bayt| bayt.is_ascii_digit()) {
        return None;
    }
    if let Some(konum) = closure_konumu(sahibi) {
        let distaki = &sahibi[konum..];
        return closure_sahibi(distaki.strip_suffix("()").unwrap_or(distaki));
    }
    // En üst düzeydeki closure'da sahibin yerinde dosyanın yolu vardır, "()" yoktur
    Some(uye_adi(sahibi.strip_suffix("()")?))
}

/// Bir closure'ın sahibi olarak, closure'ı tanımlayan satırın içinde bulunduğu fonksiyonun
/// adı: `App\X->olustur` → `olustur`. O da bir closure'sa onun sahibi:
/// `App\X->{closure:olustur()}` → `olustur`; sahibi bilinmeyen bir closure'da None.
fn sahip_adi(ad: &str) -> Option<&str> {
    match closure_konumu(ad) {
        Some(konum) => ad[konum..].strip_prefix("{closure:")?.strip_suffix("()}"),
        None => Some(uye_adi(ad)),
    }
}

/// Tam addaki üyenin adı: `App\X->olustur` → `olustur`, `App\X::bul` → `bul`. Sınıfsız bir
/// fonksiyonda adın tamamı: `App\yardimci`.
fn uye_adi(ad: &str) -> &str {
    ["->", "::"]
        .into_iter()
        .filter_map(|ayrac| ad.rfind(ayrac).map(|konum| konum + ayrac.len()))
        .max()
        .map_or(ad, |konum| &ad[konum..])
}

/// Metinde `{closure`'ın başladığı yer. Önce `{` karakteri aranır: alt dizgi aramaktan ucuzdur
/// ve yığın izinin uygulama satırlarının her birinde çağrılır.
fn closure_konumu(metin: &str) -> Option<usize> {
    metin
        .match_indices('{')
        .map(|(konum, _)| konum)
        .find(|&konum| metin[konum..].starts_with("{closure"))
}

/// Closure'ın tam adı: `App\X->{closure:olustur()}`; sahibi bilinmiyorsa `App\X->{closure}`.
/// `sinif`: bağlı olduğu sınıf ve ayraç (`App\X->`), yoksa boş.
fn closure_adi(sinif: &str, sahibi: Option<&str>) -> String {
    match sahibi {
        Some(sahibi) => format!("{sinif}{{closure:{sahibi}()}}"),
        None => format!("{sinif}{{closure}}"),
    }
}

/// Metinde süslü parantezlerin dışında, `kosul`un sağlandığı ilk konum. `kosul` o konumdan
/// başlayan kalan metne bakar.
fn parantez_disinda(metin: &str, kosul: impl Fn(&str) -> bool) -> Option<usize> {
    let mut derinlik = 0usize;
    for (konum, harf) in metin.char_indices() {
        match harf {
            '{' => derinlik += 1,
            '}' => derinlik = derinlik.saturating_sub(1),
            _ if derinlik == 0 && kosul(&metin[konum..]) => return Some(konum),
            _ => {}
        }
    }
    None
}

/// Fonksiyonun kimlikteki adı: sınıfı atılmış. `App\X->toplam` → `toplam`, `App\X::bul` →
/// `bul`, `App\X->{closure:olustur()}` → `{closure:olustur()}`. Sınıfsız bir fonksiyonda adın
/// tamamı: `App\yardimci`. Dosya parmak izinde zaten vardır; sınıf, trait'teki bir metodu
/// onu kullanan her sınıf için ayrı bir yer yapardı.
fn metot_kimligi(metot: &str) -> &str {
    match closure_konumu(metot) {
        Some(konum) => &metot[konum..],
        None => uye_adi(metot),
    }
}

/// Fonksiyonun raporda gösterilen kısa adı, kimliğinden (`metot_kimligi`): `toplam()`,
/// `bul()`; bir fonksiyonda ad alanı atılır: `yardimci()`. Closure'lar olduğu gibi:
/// `{closure:olustur()}`, `{closure}`. Sınıf yazılmaz: dosya gösterilir, trait'teki bir metodun
/// sınıfı ise kayıttan kayda değişir. Anonim sınıfların (migration'lar) adı da bir şey
/// söylemezdi.
fn kisa_metot(uye: &str) -> String {
    if closure_konumu(uye).is_some() {
        return uye.to_string();
    }
    format!("{}()", kisa_ad(uye))
}

/// Çağrı, Laravel'in Pipeline'ında bir sonraki adım mı? Middleware'deki `$next($request)`
/// yığın izinde böyle yazılır:
///
/// - PHP 8.3 ve öncesi: `Illuminate\Pipeline\Pipeline->Illuminate\Pipeline\{closure}(...)`
/// - PHP 8.4'ten beri: `Illuminate\Pipeline\Pipeline->{closure:Illuminate\Pipeline\Pipeline::carry():184}(...)`
///
/// Laravel 5'te route middleware'leri `Illuminate\Routing\Pipeline`'dan geçerdi. Bağlamın
/// JSON'unda ters bölüler ikilenir (`Illuminate\\Pipeline\\Pipeline`); iki yazılış da tanınır.
fn iletici_mi(cagri: &str) -> bool {
    let Some((sinif, metot)) = cagri.split_once("->") else {
        return false;
    };
    let pipeline = matches!(
        sinif,
        r"Illuminate\Pipeline\Pipeline"
            | r"Illuminate\Routing\Pipeline"
            | r"Illuminate\\Pipeline\\Pipeline"
            | r"Illuminate\\Routing\\Pipeline"
    );
    // Metot adı ilk parantezden öncesidir. PHP 8.4'te closure'ın adı da parantez içerir
    // (`carry()`), ama `{closure` ondan önce gelir.
    let ad = metot.split('(').next().unwrap_or_default();
    pipeline && ad.contains("{closure")
}

/// JSON biçiminde bir yer: `/yol/Dosya.php:26`, kaçışlı. Dosyası olmayan çerçeveler için None:
/// Monolog bunları `internal[Sinif->{closure}]:0` diye yazar.
fn json_yeri(yer: &str) -> Option<Konum<'_>> {
    let (dosya, satir_no) = yer.rsplit_once(':')?;
    if dosya.is_empty() || dosya.starts_with("internal[") {
        return None;
    }
    Some(Konum {
        dosya: metni_coz(dosya),
        satir: Some(satir_no.parse().ok()?),
    })
}

/// Yolda verilen klasörlerden biri geçiyorsa proje kökü: klasörden önceki kısım, sonundaki
/// ayraçla birlikte. `/var/www/magaza/vendor/laravel/...` → `/var/www/magaza/`,
/// `C:\Users\İrem\Herd\magaza\vendor\laravel\...` → `C:\Users\İrem\Herd\magaza\`
fn klasorden_kok<'b>(dosya: &'b str, klasorler: &[(&str, &str)]) -> Option<&'b str> {
    klasorler.iter().find_map(|(ayrac, klasor)| {
        let konum = dosya.find(klasor)?;
        Some(&dosya[..konum + ayrac.len()])
    })
}

/// Şablonun şablon klasöründen başlayan yolu:
/// `/srv/magaza/resources/views/vitrin.blade.php` → `resources/views/vitrin.blade.php`
fn sablon_klasorunden(sablon: &str) -> Option<&str> {
    SABLON_KLASORLERI.iter().find_map(|(ayrac, klasor)| {
        let konum = sablon.find(klasor)?;
        Some(&sablon[konum + ayrac.len()..])
    })
}

/// Yolun gösterilecek hali: Windows'un `\` ayracı `/` olur. `database\migrations\x.php` →
/// `database/migrations/x.php`. Ters bölü yoksa yol kopyalanmaz.
fn ayraci_duzelt(dosya: &str) -> Cow<'_, str> {
    if dosya.contains('\\') {
        Cow::Owned(dosya.replace('\\', "/"))
    } else {
        Cow::Borrowed(dosya)
    }
}

#[cfg(test)]
mod testler {
    use super::*;
    use crate::kayit::{KayitAlici, KayitBilgisi};
    use crate::okuyucu;
    use crate::satir_bicimi::SatirBicimi;

    /// Kayıtların istisnalarını toplar: özetin deposu gibi, kökü kendi yollarında olmayanların
    /// parmak izi bütün kökler bilinince çıkarılır.
    #[derive(Default)]
    struct Toplayan {
        taslaklar: Vec<Option<IstisnaTaslagi>>,
        kokler: BTreeSet<String>,
    }

    impl KayitAlici for Toplayan {
        fn kayit(&mut self, kayit: &Kayit<'_>, _: KayitBilgisi) {
            let istisna = Istisna::kayittan(kayit);
            if let Some(kok) = istisna.as_ref().and_then(Istisna::kok) {
                self.kokler.insert(kok.to_owned());
            }
            self.taslaklar.push(istisna.map(|istisna| istisna.taslak()));
        }
    }

    /// Metindeki kayıtların parmak izleri, kayıtlarla aynı sırada. Konum fonksiyonla
    /// belirlenir; çağrıları bilinmeyen kayıtlar da özetteki gibi bütün yığın izlerinden
    /// öğrenilenle tamamlanır.
    fn izler(metin: &str) -> Vec<Option<ParmakIzi>> {
        izler_turle(metin, KonumTuru::Metot)
    }

    /// Metindeki kayıtların parmak izleri, konum verilen türle belirlenerek.
    fn izler_turle(metin: &str, tur: KonumTuru) -> Vec<Option<ParmakIzi>> {
        let mut toplayan = Toplayan::default();
        okuyucu::oku(metin.as_bytes(), &SatirBicimi::default(), &mut toplayan).unwrap();
        let istisnalar: Vec<Option<Istisna<'_>>> = toplayan
            .taslaklar
            .iter()
            .map(|taslak| taslak.as_ref().map(IstisnaTaslagi::istisna))
            .collect();
        let mut harita = MetotHaritasi::default();
        for istisna in istisnalar.iter().flatten() {
            harita.ogren(istisna, istisna.cozulmus_kok(&toplayan.kokler));
        }
        istisnalar
            .iter()
            .map(|istisna| {
                istisna.as_ref().map(|istisna| {
                    let mut iz = istisna.parmak_izi(&toplayan.kokler, tur);
                    if tur == KonumTuru::Metot {
                        harita.tamamla(&mut iz);
                    }
                    iz
                })
            })
            .collect()
    }

    /// Metindeki ilk kaydın parmak izi. Kök gerekirse metnin başka kayıtlarından da bulunur.
    fn iz(metin: &str) -> Option<ParmakIzi> {
        izler(metin).into_iter().next().flatten()
    }

    /// Testlerde parmak izini kısaca yazmak için: (kısa sınıf, "dosya:satır, metot()")
    fn ozet(iz: &Option<ParmakIzi>) -> Option<(&str, Option<String>)> {
        iz.as_ref().map(|iz| (iz.kisa_sinif(), iz.konum_metni()))
    }

    const UYGULAMADA_FIRLATILAN: &str = r#"[2026-10-24 15:02:11] production.ERROR: Undefined array key "kargo_ucreti" {"userId":2210,"exception":"[object] (ErrorException(code: 0): Undefined array key \"kargo_ucreti\" at /var/www/magaza/app/Services/SepetHesaplayici.php:88)
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Foundation/Bootstrap/HandleExceptions.php(256): Illuminate\\Foundation\\Bootstrap\\HandleExceptions->handleError(2, 'Undefined array...', '/var/www/magaza...', 88)
#1 /var/www/magaza/app/Services/SepetHesaplayici.php(88): Illuminate\\Foundation\\Bootstrap\\HandleExceptions->Illuminate\\Foundation\\Bootstrap\\{closure}(2, 'Undefined array...', '/var/www/magaza...', 88)
#2 /var/www/magaza/app/Http/Controllers/SepetController.php(41): App\\Services\\SepetHesaplayici->toplam(Object(App\\Models\\Sepet))
#3 {main}
"} "#;

    const VENDORDA_FIRLATILAN: &str = r#"[2026-10-24 09:14:20] production.ERROR: SQLSTATE[23000]: Integrity constraint violation {"exception":"[object] (Illuminate\\Database\\UniqueConstraintViolationException(code: 23000): SQLSTATE[23000]: Integrity constraint violation at /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php:819)
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php(779): Illuminate\\Database\\Connection->runQueryCallback('insert into `ku...', Array, Object(Closure))
#1 [internal function]: Illuminate\\Database\\Connection->insert('insert into `ku...', Array)
#2 /var/www/magaza/app/Http/Controllers/KayitController.php(38): Illuminate\\Database\\Eloquent\\Model->save()
#3 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/ControllerDispatcher.php(46): App\\Http\\Controllers\\KayitController->store(Object(Illuminate\\Http\\Request))
#4 /var/www/magaza/public/index.php(17): Illuminate\\Foundation\\Application->handleRequest(Object(Illuminate\\Http\\Request))
#5 {main}
"} "#;

    #[test]
    fn istisnasiz_kayitta_parmak_izi_yok() {
        assert_eq!(
            iz(
                r#"[2026-10-24 09:02:13] production.INFO: Kullanıcı 381 giriş yaptı {"ip":"203.0.113.24"} "#
            ),
            None
        );
    }

    #[test]
    fn uygulamada_firlatilan_istisna_kendi_satirini_verir() {
        assert_eq!(
            ozet(&iz(UYGULAMADA_FIRLATILAN)),
            Some((
                "ErrorException",
                Some("app/Services/SepetHesaplayici.php:88, toplam()".to_string())
            ))
        );
    }

    #[test]
    fn vendorda_firlatilan_istisnada_uygulamadaki_ilk_cagri_secilir() {
        assert_eq!(
            ozet(&iz(VENDORDA_FIRLATILAN)),
            Some((
                "UniqueConstraintViolationException",
                Some("app/Http/Controllers/KayitController.php:38, store()".to_string())
            ))
        );
    }

    #[test]
    fn yalnizca_vendor_ve_giris_noktasi_varsa_konum_yok() {
        let metin = r#"[2026-10-24 11:30:44] production.ERROR: App\Jobs\FaturaGonder has been attempted too many times. {"exception":"[object] (Illuminate\\Queue\\MaxAttemptsExceededException(code: 0): App\\Jobs\\FaturaGonder has been attempted too many times. at /var/www/magaza/vendor/laravel/framework/src/Illuminate/Queue/MaxAttemptsExceededException.php:24)
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Queue/Worker.php(817): Illuminate\\Queue\\MaxAttemptsExceededException::forJob(Object(Illuminate\\Queue\\Jobs\\RedisJob))
#1 /var/www/magaza/artisan(16): Illuminate\\Foundation\\Application->handleCommand(Object(Symfony\\Component\\Console\\Input\\ArgvInput))
#2 {main}
"} "#;
        let parmak_izi = iz(metin).unwrap();
        assert_eq!(parmak_izi.kisa_sinif(), "MaxAttemptsExceededException");
        assert_eq!(parmak_izi.konum, None);
        assert_eq!(
            parmak_izi.to_string(),
            "uygulama kodunda bulunamadı (MaxAttemptsExceededException)"
        );
        // Laravel 8 ve öncesinde `php artisan serve`'ün giriş dosyası proje kökündeydi
        let eski_sunucu = metin.replace(
            "/var/www/magaza/artisan(16)",
            "/var/www/magaza/server.php(21)",
        );
        assert_eq!(
            ozet(&iz(&eski_sunucu)),
            Some(("MaxAttemptsExceededException", None))
        );
    }

    #[test]
    fn onceki_istisnanin_izi_kullanilmaz() {
        // Dıştaki istisnanın izinde uygulama kodu yok; öncekinin izindeki satır alınmamalı
        let metin = r#"[2026-10-24 16:10:02] production.ERROR: cURL error 28 {"exception":"[object] (Illuminate\\Http\\Client\\ConnectionException(code: 0): cURL error 28 at /var/www/magaza/vendor/laravel/framework/src/Illuminate/Http/Client/PendingRequest.php:1641)
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Http/Client/PendingRequest.php(1013): Illuminate\\Http\\Client\\PendingRequest->marshalConnectionException(Object(GuzzleHttp\\Exception\\ConnectException))
#1 {main}

[previous exception] [object] (GuzzleHttp\\Exception\\ConnectException(code: 0): cURL error 28 at /var/www/magaza/app/Services/OdemeServisi.php:64)
[stacktrace]
#0 /var/www/magaza/app/Services/OdemeServisi.php(64): GuzzleHttp\\Client->send(Object(GuzzleHttp\\Psr7\\Request))
#1 {main}
"} "#;
        assert_eq!(ozet(&iz(metin)), Some(("ConnectionException", None)));
    }

    #[test]
    fn cok_satirli_istisna_mesajinda_yer_son_baslik_satirindan_alinir() {
        // Mesaj satır sonu içeriyorsa (Laravel'in HTTP istemcisinin hataları da böyledir) log
        // mesajı da çok satırlı olur: bağlam başlık satırında değil, bir devam satırında başlar.
        // İstisna başlığı da birkaç satıra yayılır ve fırlatıldığı yer son satırdadır.
        let metin = r#"[2026-10-24 16:20:00] production.ERROR: Ödeme sağlayıcısı geçersiz yanıt döndü:
<html>Bakımdayız</html>
 {"exception":"[object] (App\\Exceptions\\OdemeHatasi(code: 0): Ödeme sağlayıcısı geçersiz yanıt döndü:
<html>Bakımdayız</html>
 at /var/www/magaza/app/Services/OdemeServisi.php:80)
[stacktrace]
#0 /var/www/magaza/app/Http/Controllers/OdemeController.php(31): App\\Services\\OdemeServisi->tahsilEt()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/ControllerDispatcher.php(46): App\\Http\\Controllers\\OdemeController->odeme()
#2 {main}
"} "#;
        assert_eq!(
            ozet(&iz(metin)),
            Some((
                "OdemeHatasi",
                Some("app/Services/OdemeServisi.php:80, tahsilEt()".to_string())
            ))
        );
    }

    #[test]
    fn farkli_surum_klasorlerinde_parmak_izi_ayni() {
        // Sıfır kesintili dağıtımda (Envoyer, Deployer) her sürüm ayrı bir klasörde çalışır.
        // Kök her kayıtta yeniden bulunduğu için konum köke göre aynı kalır.
        let eski = UYGULAMADA_FIRLATILAN
            .replace("/var/www/magaza/", "/home/forge/magaza/releases/20261023/");
        let yeni = UYGULAMADA_FIRLATILAN
            .replace("/var/www/magaza/", "/home/forge/magaza/releases/20261024/");
        assert!(iz(&eski).is_some());
        assert_eq!(iz(&eski), iz(&yeni));
    }

    #[test]
    fn koke_gore_yazilmis_yollar_taninir() {
        // Monolog'un setBasePath ayarı yolların başındaki proje kökünü siler
        let metin = VENDORDA_FIRLATILAN.replace("/var/www/magaza/", "");
        assert_eq!(
            ozet(&iz(&metin)),
            Some((
                "UniqueConstraintViolationException",
                Some("app/Http/Controllers/KayitController.php:38, store()".to_string())
            ))
        );
    }

    #[test]
    fn yigin_izi_yoksa_firlatildigi_yer_kullanilir() {
        // Stack trace kapalı bir yapılandırma: başlık tek satırda biter. Kök bulunamadığı
        // için yol olduğu gibi kalır.
        let metin = r#"[2026-10-24 10:00:00] local.ERROR: Sipariş bulunamadı {"exception":"[object] (App\\Exceptions\\SiparisBulunamadi(code: 404): Sipariş bulunamadı at /srv/magaza/app/Services/SiparisServisi.php:30)"} "#;
        assert_eq!(
            ozet(&iz(metin)),
            Some((
                "SiparisBulunamadi",
                Some("/srv/magaza/app/Services/SiparisServisi.php:30".to_string())
            ))
        );
    }

    #[test]
    fn cerceveler_yer_ve_cagriya_ayrilir() {
        assert_eq!(
            cerceve_ayristir("#2 /var/www/app/Kod.php(41): App\\Sinif->metot('a): b')"),
            Some(Cerceve {
                yer: Some(Konum {
                    dosya: Cow::Borrowed("/var/www/app/Kod.php"),
                    satir: Some(41)
                }),
                cagri: "App\\Sinif->metot('a): b')",
            })
        );
        // Dosyası olmayan çerçevelerin yalnızca çağrısı var; son satırın arkasında bağlam
        // olabilir
        assert_eq!(
            cerceve_ayristir("#1 [internal function]: App\\Sinif->metot()"),
            Some(Cerceve {
                yer: None,
                cagri: "App\\Sinif->metot()"
            })
        );
        for satir in ["#9 {main}", r#"#3 {main} {"kullanici":5}"#] {
            assert_eq!(
                cerceve_ayristir(satir),
                Some(Cerceve {
                    yer: None,
                    cagri: "{main}"
                }),
                "{satir}"
            );
        }
        assert_eq!(cerceve_ayristir("\"} "), None);
        assert_eq!(cerceve_ayristir("#2 /var/www/app/Kod.php(x): a()"), None);
        assert_eq!(cerceve_ayristir("#2 /var/www/app/Kod.php: a()"), None);

        // Yol logdaki gibi okunur; bağlamın JSON'undaysa adaylar kurulurken çözülür, PHP'nin
        // istisna metnindeyse olduğu gibi kalır
        let kacisli = r"#0 C:\\Program Files (x86)\\magaza\\app\\A.php(7): App\\A->b()";
        let cerceve = cerceve_ayristir(kacisli).unwrap();
        assert_eq!(
            cerceve.yer.as_ref().unwrap().dosya,
            r"C:\\Program Files (x86)\\magaza\\app\\A.php"
        );
        let adaylar = adaylari_kur(None, &[cerceve], true);
        assert_eq!(
            adaylar[0].konum.dosya,
            r"C:\Program Files (x86)\magaza\app\A.php"
        );
        let ham = r"#0 C:\Program Files (x86)\magaza\app\A.php(7): App\A->b()";
        let adaylar = adaylari_kur(None, &[cerceve_ayristir(ham).unwrap()], false);
        assert_eq!(
            adaylar[0].konum.dosya,
            r"C:\Program Files (x86)\magaza\app\A.php"
        );
        assert!(matches!(adaylar[0].konum.dosya, Cow::Borrowed(_)));
    }

    #[test]
    fn istisna_olmayan_object_ifadesi_sinif_sayilmaz() {
        assert_eq!(sinif_bul("[object] (Monolog bir şey yazdı(code: 1)"), None);
        assert_eq!(
            sinif_bul(r#"{"exception":"[object] (class@anonymous(code: 0): x"#),
            Some("class@anonymous")
        );
    }

    #[test]
    fn tam_sinif_adi_ve_konum_okunur() {
        let parmak_izi = iz(VENDORDA_FIRLATILAN).unwrap();
        assert_eq!(
            parmak_izi.sinif(),
            r"Illuminate\Database\UniqueConstraintViolationException"
        );
        assert_eq!(
            parmak_izi.konum(),
            Some((
                Cow::from("app/Http/Controllers/KayitController.php"),
                Some(38)
            ))
        );
        // Ters bölü yoksa yol kopyalanmaz
        assert!(matches!(parmak_izi.konum(), Some((Cow::Borrowed(_), _))));
    }

    /// Windows'ta (Herd) bir migration hatası: yollar `C:\\...\\` ve istisna vendor'da
    /// fırlatıldı.
    const WINDOWS_MIGRATION: &str = r#"[2026-10-20 08:37:53] local.ERROR: SQLSTATE[HY000]: General error: 1824 Failed to open the referenced table 'kategoriler' {"exception":"[object] (Illuminate\\Database\\QueryException(code: HY000): SQLSTATE[HY000]: General error: 1824 Failed to open the referenced table 'kategoriler' at C:\\Users\\İrem\\Herd\\magaza\\vendor\\laravel\\framework\\src\\Illuminate\\Database\\Connection.php:822)
[stacktrace]
#0 C:\\Users\\İrem\\Herd\\magaza\\vendor\\laravel\\framework\\src\\Illuminate\\Database\\Connection.php(776): Illuminate\\Database\\Connection->runQueryCallback('alter table `ur...', Array, Object(Closure))
#1 C:\\Users\\İrem\\Herd\\magaza\\vendor\\laravel\\framework\\src\\Illuminate\\Support\\Facades\\Facade.php(361): Illuminate\\Database\\Schema\\Builder->create('urunler', Object(Closure))
#2 C:\\Users\\İrem\\Herd\\magaza\\database\\migrations\\2026_10_20_083604_create_urunler_table.php(11): Illuminate\\Support\\Facades\\Facade::__callStatic('create', Array)
#3 C:\\Users\\İrem\\Herd\\magaza\\vendor\\laravel\\framework\\src\\Illuminate\\Database\\Migrations\\Migrator.php(514): Illuminate\\Database\\Migrations\\Migration@anonymous->up()
#4 C:\\Users\\İrem\\Herd\\magaza\\artisan(16): Illuminate\\Foundation\\Application->handleCommand(Object(Symfony\\Component\\Console\\Input\\ArgvInput))
#5 {main}
"} "#;

    #[test]
    fn windows_yollari_taninir() {
        let parmak_izi = iz(WINDOWS_MIGRATION).unwrap();
        assert_eq!(
            parmak_izi.to_string(),
            "database/migrations/2026_10_20_083604_create_urunler_table.php:11, up() (QueryException)"
        );
        // Konum çözülmüş ve ayracı `/` olarak tutulur
        assert_eq!(
            parmak_izi.konum.as_ref().unwrap().dosya,
            "database/migrations/2026_10_20_083604_create_urunler_table.php"
        );

        // Uygulama kodunda yalnızca giriş noktaları varsa konum yok: `public\\index.php`
        // giriş noktası olarak tanınmalı
        let girisler = WINDOWS_MIGRATION.replace(
            r"database\\migrations\\2026_10_20_083604_create_urunler_table.php",
            r"public\\index.php",
        );
        assert_eq!(ozet(&iz(&girisler)), Some(("QueryException", None)));
    }

    #[test]
    fn proje_koku_ve_yol_cozulur() {
        let vendor = |dosya: &'static str| vendor_yerleri(dosya).next().map(|son| &dosya[..son]);
        assert_eq!(
            vendor("/var/www/magaza/vendor/laravel/x.php"),
            Some("/var/www/magaza/")
        );
        assert_eq!(
            vendor(r"C:\Users\İrem\Herd\magaza\vendor\laravel\x.php"),
            Some(r"C:\Users\İrem\Herd\magaza\")
        );
        // Windows'ta ayraç / da olabilir; Composer bazen ikisini karıştırır
        assert_eq!(
            vendor("C:/Users/İrem/Herd/magaza/vendor/laravel/x.php"),
            Some("C:/Users/İrem/Herd/magaza/")
        );
        assert_eq!(
            vendor(r"C:\magaza\vendor\composer/../laravel/x.php"),
            Some(r"C:\magaza\")
        );
        assert_eq!(vendor("/var/www/magaza/app/vendorlar.php"), None);
        assert_eq!(
            vendor("/srv/a/vendor/paket/storage/framework/views/x.php"),
            Some("/srv/a/")
        );
        // Derlenmiş şablonların klasörü de kökü verir
        assert_eq!(
            klasorden_kok(
                r"C:\magaza\storage\framework\views\83012915.php",
                &DERLENMIS_SABLON_KLASORLERI
            ),
            Some(r"C:\magaza\")
        );

        assert_eq!(ayraci_duzelt(r"app\Models\Urun.php"), "app/Models/Urun.php");
        assert!(matches!(
            ayraci_duzelt("app/Models/Urun.php"),
            Cow::Borrowed(_)
        ));
        // JSON kaçışları okunurken çözülür: `\u00dc` Ü'dür
        assert_eq!(
            json_yeri(r"C:\\app\\\u00dcr\u00fcn.php:3")
                .map(|konum| konum.sahiplen(None, KonumTuru::Metot).to_string()),
            Some("C:/app/Ürün.php:3".to_string())
        );
    }

    /// JSON satırının istisnası, `includeStacktraces` açıkken (yığın izi `trace`'te)
    const JSON_IZLI: &str = r#"{"message":"SQLSTATE[HY000] [2002] Connection refused","context":{"userId":2210,"exception":{"class":"Illuminate\\Database\\QueryException","message":"SQLSTATE[HY000] [2002] Connection refused","code":2002,"file":"/var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php:864","trace":["/var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php:1052","internal[Illuminate\\Support\\HigherOrderCollectionProxy->{closure}]:0","/var/www/magaza/app/Services/SiparisDeposu.php:11","/var/www/magaza/app/Http/Controllers/SiparisController.php:20","/var/www/magaza/public/index.php:17"],"previous":{"class":"PDOException","message":"SQLSTATE[HY000] [2002] Connection refused","code":2002,"file":"/var/www/magaza/app/Services/BaskaBirYer.php:5"}}},"level":400,"level_name":"ERROR","channel":"production","datetime":"2026-10-24T09:14:20.123456+00:00","extra":{}}"#;

    #[test]
    fn json_istisnasi_okunur() {
        // Fırlatıldığı yer vendor'da; trace'teki ilk uygulama satırı seçilir, dosyası olmayan
        // çerçeve ve önceki istisna atlanır
        assert_eq!(
            iz(JSON_IZLI).map(|iz| iz.to_string()),
            Some("app/Services/SiparisDeposu.php:11 (QueryException)".to_string())
        );
        assert_eq!(
            iz(JSON_IZLI).unwrap().sinif(),
            r"Illuminate\Database\QueryException"
        );

        // Yığın izi yoksa yalnızca fırlatıldığı yer bilinir: vendor'daysa konum yok
        let izsiz = JSON_IZLI.replace(
            r#""trace":["/var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php:1052","internal[Illuminate\\Support\\HigherOrderCollectionProxy->{closure}]:0","/var/www/magaza/app/Services/SiparisDeposu.php:11","/var/www/magaza/app/Http/Controllers/SiparisController.php:20","/var/www/magaza/public/index.php:17"],"#,
            "",
        );
        assert_eq!(ozet(&iz(&izsiz)), Some(("QueryException", None)));

        // Uygulamada fırlatıldıysa ve logda kök başka yerde de geçmiyorsa yol olduğu gibi
        // (ayracı / olarak) yazılır
        let uygulamada = izsiz.replace(
            "/var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php:864",
            r"C:\\Users\\İrem\\Herd\\magaza\\app\\Models\\Urun.php:12",
        );
        assert_eq!(
            ozet(&iz(&uygulamada)),
            Some((
                "QueryException",
                Some("C:/Users/İrem/Herd/magaza/app/Models/Urun.php:12".to_string())
            ))
        );
    }

    #[test]
    fn json_istisnasi_olmayan_degerler() {
        let istisnali = |istisna: &str| {
            format!(
                r#"{{"message":"x","context":{{"exception":{istisna}}},"level":400,"level_name":"ERROR","datetime":"2026-10-24T09:14:20+00:00"}}"#
            )
        };
        // Sınıf adı geçersiz, sınıf yok, istisna bir nesne değil
        for istisna in [
            r#"{"class":"Bu bir sınıf değil","file":"/a/b.php:1"}"#,
            r#"{"file":"/a/b.php:1"}"#,
            r#""[object] (ErrorException(code: 0): x at /a/b.php:1)""#,
        ] {
            assert_eq!(iz(&istisnali(istisna)), None, "{istisna}");
        }
        // Yer ve iz okunamıyorsa sınıf yine bilinir
        assert_eq!(
            ozet(&iz(&istisnali(
                r#"{"class":"RuntimeException","file":"satırsız","trace":"dizi değil"}"#
            ))),
            Some(("RuntimeException", None))
        );
        assert_eq!(
            json_yeri("/a/b.php:12"),
            Some(Konum {
                dosya: Cow::Borrowed("/a/b.php"),
                satir: Some(12)
            })
        );
        assert_eq!(json_yeri(":12"), None);
        assert_eq!(json_yeri("/a/b.php"), None);
        assert_eq!(json_yeri("internal[X->y]:0"), None);
    }

    /// Bir route middleware'inden geçen istekte tanımsız bir denetleyici metodu (Laravel 13,
    /// PHP 8.3, istisna parametreleri kapalı). Uygulama kodundaki tek satır middleware'in
    /// `return $next($request);` satırı.
    const MIDDLEWARE_ARDINDA: &str = r#"[2026-10-26 13:44:16] local.ERROR: Call to undefined method App\Http\Controllers\KampanyaController::duzenle() {"exception":"[object] (Error(code: 0): Call to undefined method App\\Http\\Controllers\\KampanyaController::duzenle() at /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/ControllerDispatcher.php:46)
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/Route.php(276): Illuminate\\Routing\\ControllerDispatcher->dispatch()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/Router.php(834): Illuminate\\Routing\\Route->run()
#2 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Pipeline/Pipeline.php(180): Illuminate\\Routing\\Router->Illuminate\\Routing\\{closure}()
#3 /var/www/magaza/app/Http/Middleware/YoneticiMi.php(13): Illuminate\\Pipeline\\Pipeline->Illuminate\\Pipeline\\{closure}()
#4 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Pipeline/Pipeline.php(219): App\\Http\\Middleware\\YoneticiMi->handle()
#5 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/Middleware/SubstituteBindings.php(52): Illuminate\\Pipeline\\Pipeline->Illuminate\\Pipeline\\{closure}()
#6 /var/www/magaza/public/index.php(20): Illuminate\\Foundation\\Application->handleRequest()
#7 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Foundation/resources/server.php(23): require_once('...')
#8 {main}
"} "#;

    #[test]
    fn middleware_satiri_hatanin_yeri_sayilmaz() {
        assert_eq!(ozet(&iz(MIDDLEWARE_ARDINDA)), Some(("Error", None)));

        // PHP 8.4'ün closure adlarıyla da: `{closure:Sinif::metot():satır}`
        let php84 = MIDDLEWARE_ARDINDA.replace(
            r"YoneticiMi.php(13): Illuminate\\Pipeline\\Pipeline->Illuminate\\Pipeline\\{closure}()",
            r"YoneticiMi.php(13): Illuminate\\Pipeline\\Pipeline->{closure:Illuminate\\Pipeline\\Pipeline::carry():184}(Object(Illuminate\\Http\\Request))",
        );
        assert_ne!(php84, MIDDLEWARE_ARDINDA);
        assert_eq!(ozet(&iz(&php84)), Some(("Error", None)));

        // Middleware'in başka bir çağrısı hatanın yeridir: ör. `abort_if()` yanlış kullanıldı
        let kendi_hatasi = MIDDLEWARE_ARDINDA.replace(
            r"YoneticiMi.php(13): Illuminate\\Pipeline\\Pipeline->Illuminate\\Pipeline\\{closure}()",
            "YoneticiMi.php(13): abort_if()",
        );
        assert_eq!(
            ozet(&iz(&kendi_hatasi)),
            Some((
                "Error",
                Some("app/Http/Middleware/YoneticiMi.php:13, handle()".to_string())
            ))
        );
    }

    #[test]
    fn iletici_cagrilar_taninir() {
        for cagri in [
            r"Illuminate\\Pipeline\\Pipeline->Illuminate\\Pipeline\\{closure}(Object(Illuminate\\Http\\Request))",
            r"Illuminate\\Pipeline\\Pipeline->Illuminate\\Pipeline\\{closure}()",
            r"Illuminate\\Pipeline\\Pipeline->{closure:Illuminate\\Pipeline\\Pipeline::carry():184}(Object(Illuminate\\Http\\Request))",
            r"Illuminate\\Pipeline\\Pipeline->{closure:Illuminate\\Pipeline\\Pipeline::prepareDestination():142}()",
            r"Illuminate\\Routing\\Pipeline->Illuminate\\Routing\\{closure}(Object(Illuminate\\Http\\Request))",
            // PHP'nin istisna metninde ters bölüler tektir
            r"Illuminate\Pipeline\Pipeline->Illuminate\Pipeline\{closure}(Object(Illuminate\Http\Request))",
            r"Illuminate\Routing\Pipeline->{closure:Illuminate\Routing\Pipeline::carry():184}()",
        ] {
            assert!(iletici_mi(cagri), "{cagri}");
        }
        for cagri in [
            // Pipeline'ı başlatmak, closure'ı bir parametre olarak vermek
            r"Illuminate\\Pipeline\\Pipeline->then(Object(Closure))",
            r"Illuminate\\Pipeline\\Pipeline->then(Object(Illuminate\\Pipeline\\{closure}))",
            // Başka bir sınıfın closure'ı
            r"Illuminate\\Routing\\Router->Illuminate\\Routing\\{closure}(Object(Illuminate\\Http\\Request))",
            r"App\\Pipeline->Illuminate\\Pipeline\\{closure}()",
            "abort_if()",
            "",
        ] {
            assert!(!iletici_mi(cagri), "{cagri}");
        }
    }

    /// Bir middleware'den geçen istekte şablondaki `route()` çağrısı tanımsız bir rotayı
    /// istiyor. Hata, denetleyici döndükten sonra yanıt hazırlanırken oluştuğu için yığın
    /// izinde denetleyici yok; uygulama kodundaki tek satır middleware'in iletici satırı.
    const VIEW_HATASI: &str = r#"[2026-10-26 13:44:16] local.ERROR: Route [panel.kampanya.olustur] not defined. (View: /var/www/magaza/resources/views/panel/kampanyalar.blade.php) {"exception":"[object] (Illuminate\\View\\ViewException(code: 0): Route [panel.kampanya.olustur] not defined. (View: /var/www/magaza/resources/views/panel/kampanyalar.blade.php) at /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/UrlGenerator.php:542)
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/View/Engines/PhpEngine.php(59): Illuminate\\View\\Engines\\CompilerEngine->handleViewException()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/View/Engines/CompilerEngine.php(76): Illuminate\\View\\Engines\\PhpEngine->evaluatePath()
#2 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/Router.php(918): Illuminate\\Routing\\Router::toResponse()
#3 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Pipeline/Pipeline.php(180): Illuminate\\Routing\\Router->Illuminate\\Routing\\{closure}()
#4 /var/www/magaza/app/Http/Middleware/YoneticiMi.php(13): Illuminate\\Pipeline\\Pipeline->Illuminate\\Pipeline\\{closure}()
#5 /var/www/magaza/public/index.php(20): Illuminate\\Foundation\\Application->handleRequest()
#6 {main}

[previous exception] [object] (Symfony\\Component\\Routing\\Exception\\RouteNotFoundException(code: 0): Route [panel.kampanya.olustur] not defined. at /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/UrlGenerator.php:542)
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Foundation/helpers.php(870): Illuminate\\Routing\\UrlGenerator->route()
#1 /var/www/magaza/storage/framework/views/8748faba003f4077f51e09160dbe4ea7.php(2): route()
#2 {main}
"} "#;

    #[test]
    fn view_hatasinda_konum_sablondur() {
        let parmak_izi = iz(VIEW_HATASI).unwrap();
        assert_eq!(
            parmak_izi.to_string(),
            "resources/views/panel/kampanyalar.blade.php (ViewException)"
        );
        assert_eq!(
            parmak_izi.konum(),
            Some((
                Cow::from("resources/views/panel/kampanyalar.blade.php"),
                None
            ))
        );

        // Windows'ta da: ayraçlar `\\`, gösterilirken `/`
        let windows = r#"[2026-10-26 13:44:16] local.ERROR: Route [panel.kampanya.olustur] not defined. (View: C:\Users\İrem\Herd\magaza\resources\views\panel\kampanyalar.blade.php) {"exception":"[object] (Illuminate\\View\\ViewException(code: 0): Route [panel.kampanya.olustur] not defined. (View: C:\\Users\\İrem\\Herd\\magaza\\resources\\views\\panel\\kampanyalar.blade.php) at C:\\Users\\İrem\\Herd\\magaza\\vendor\\laravel\\framework\\src\\Illuminate\\Routing\\UrlGenerator.php:542)
[stacktrace]
#0 C:\\Users\\İrem\\Herd\\magaza\\vendor\\laravel\\framework\\src\\Illuminate\\View\\Engines\\PhpEngine.php(59): Illuminate\\View\\Engines\\CompilerEngine->handleViewException()
#1 C:\\Users\\İrem\\Herd\\magaza\\app\\Http\\Middleware\\YoneticiMi.php(13): Illuminate\\Pipeline\\Pipeline->Illuminate\\Pipeline\\{closure}()
#2 C:\\Users\\İrem\\Herd\\magaza\\public\\index.php(20): Illuminate\\Foundation\\Application->handleRequest()
#3 {main}
"} "#;
        assert_eq!(
            iz(windows).map(|iz| iz.to_string()),
            Some("resources/views/panel/kampanyalar.blade.php (ViewException)".to_string())
        );
    }

    #[test]
    fn ic_ice_sablonda_hatanin_oldugu_sablon_alinir() {
        // Hata vitrin'in içindeki ürün kartında: Laravel önce içteki şablonu ekler
        let metin = r#"[2026-10-26 13:44:16] local.ERROR: Undefined variable $urun (View: /var/www/magaza/resources/views/partials/urun-karti.blade.php) (View: /var/www/magaza/resources/views/vitrin.blade.php) {"exception":"[object] (Illuminate\\View\\ViewException(code: 0): Undefined variable $urun (View: /var/www/magaza/resources/views/partials/urun-karti.blade.php) (View: /var/www/magaza/resources/views/vitrin.blade.php) at /var/www/magaza/storage/framework/views/83012915d91cdfaaefee3b54235ba486.php:2)
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/View/Engines/PhpEngine.php(59): Illuminate\\View\\Engines\\CompilerEngine->handleViewException()
#1 /var/www/magaza/public/index.php(20): Illuminate\\Foundation\\Application->handleRequest()
#2 {main}
"} "#;
        assert_eq!(
            iz(metin).map(|iz| iz.to_string()),
            Some("resources/views/partials/urun-karti.blade.php (ViewException)".to_string())
        );
    }

    #[test]
    fn json_view_hatasi_ve_derlenmis_sablon() {
        // Yığın izi kapalı bir JSON kaydı: tek yol derlenmiş şablon, vendor geçmiyor. Kök
        // derlenmiş şablonun klasöründen bulunur.
        let metin = r#"{"message":"Undefined variable $urun (View: C:\\Users\\İrem\\Herd\\magaza\\resources\\views\\vitrin.blade.php)","context":{"exception":{"class":"Illuminate\\View\\ViewException","message":"Undefined variable $urun (View: C:\\Users\\İrem\\Herd\\magaza\\resources\\views\\vitrin.blade.php)","code":0,"file":"C:\\Users\\İrem\\Herd\\magaza\\storage\\framework\\views\\83012915d91cdfaaefee3b54235ba486.php:2"}},"level":400,"level_name":"ERROR","channel":"local","datetime":"2026-10-26T13:44:16.000000+00:00","extra":{}}"#;
        assert_eq!(
            iz(metin).map(|iz| iz.to_string()),
            Some("resources/views/vitrin.blade.php (ViewException)".to_string())
        );
        // View hatası olmayan bir istisnada mesajdaki `(View: ...)` şablon sayılmaz. Tek yer
        // derlenmiş şablon: uygulama kodu değil
        let baska = metin.replace(r"Illuminate\\View\\ViewException", "RuntimeException");
        assert_eq!(ozet(&iz(&baska)), Some(("RuntimeException", None)));

        // Derlenmiş şablonlar başka bir klasördeyse (VIEW_COMPILED_PATH) kök bilinmez; şablonun
        // yolu `resources\\views`'ten başlatılır
        let baska_klasor = metin.replace(
            r"C:\\Users\\İrem\\Herd\\magaza\\storage\\framework\\views\\",
            r"C:\\Temp\\gorunumler\\",
        );
        assert_ne!(baska_klasor, metin);
        assert_eq!(
            iz(&baska_klasor).map(|iz| iz.to_string()),
            Some("resources/views/vitrin.blade.php (ViewException)".to_string())
        );
    }

    #[test]
    fn paylasilan_storage_koku_bozmaz() {
        // Sıfır kesintili dağıtım: kod her dağıtımda yeni bir klasörde, `storage` bütün
        // dağıtımların paylaştığı bir klasöre bağlantı. Derlenmiş şablon yığın izinde
        // vendor'dan önce gelir ama dağıtımın klasörünün dışındadır; kök vendor'dan bulunur.
        // İki dağıtımdaki aynı hatalar aynı yeri gösterir.
        //
        // `kod`: kodun klasörü, `storage`: storage klasörü; ikisi de sonundaki ayraçla
        let kayitlar = |kod: &str, storage: &str| {
            format!(
                r#"[2026-10-24 10:00:00] production.ERROR: Undefined variable $urun (View: {kod}resources/views/vitrin.blade.php) {{"exception":"[object] (Illuminate\\View\\ViewException(code: 0): Undefined variable $urun (View: {kod}resources/views/vitrin.blade.php) at {storage}framework/views/8748fa1c.php:2)
[stacktrace]
#0 {kod}vendor/laravel/framework/src/Illuminate/View/Engines/PhpEngine.php(58): Illuminate\\View\\Engines\\CompilerEngine->handleViewException(Object(ErrorException), 0)
#1 {kod}public/index.php(17): Illuminate\\Foundation\\Application->handleRequest()
#2 {{main}}
"}}
[2026-10-24 10:05:00] production.ERROR: Fiyat bulunamadı {{"exception":"[object] (RuntimeException(code: 0): Fiyat bulunamadı at {kod}app/Support/Fiyat.php:14)
[stacktrace]
#0 {storage}framework/views/8748fa1c.php(5): App\\Support\\Fiyat::bul()
#1 {kod}vendor/laravel/framework/src/Illuminate/Filesystem/Filesystem.php(123): require()
#2 {{main}}
"}}"#
            )
        };
        let beklenen = [
            "resources/views/vitrin.blade.php (ViewException)",
            "app/Support/Fiyat.php:14, bul() (RuntimeException)",
        ];
        let izler = |metin: &str| -> Vec<String> {
            izler(metin)
                .iter()
                .map(|iz| iz.as_ref().map(|iz| iz.to_string()).unwrap_or_default())
                .collect()
        };

        let paylasilan = "/home/deploy/magaza/storage/";
        let metin = [
            kayitlar("/home/deploy/magaza/releases/20261024090000/", paylasilan),
            kayitlar("/home/deploy/magaza/releases/20261024113000/", paylasilan),
        ]
        .join("\n");
        assert_eq!(izler(&metin), [beklenen, beklenen].concat());

        // Vapor: `storage` /tmp'de, kod /var/task'ta
        assert_eq!(izler(&kayitlar("/var/task/", "/tmp/storage/")), beklenen);
        // Olağan kurulum: ikisi de proje kökünde
        assert_eq!(
            izler(&kayitlar("/var/www/magaza/", "/var/www/magaza/storage/")),
            beklenen
        );
    }

    #[test]
    fn derlenmis_sablondaki_olumcul_hata_konumsuzdur() {
        // Süre sınırı şablon işlenirken doldu: yer rastgele, derlenmiş dosyanın adı bir özet.
        // Aynı hatanın vendor'da kalan kaydıyla aynı parmak izini taşır.
        let sablonda = r#"[2026-10-26 13:44:17] local.ERROR: Maximum execution time of 30 seconds exceeded {"exception":"[object] (Symfony\\Component\\ErrorHandler\\Error\\FatalError(code: 0): Maximum execution time of 30 seconds exceeded at /var/www/magaza/storage/framework/views/6d7695d270d0f086ac44cf85aac26053.php:1)
[stacktrace]
#0 {main}
"} "#;
        let vendorda = sablonda.replace(
            "/var/www/magaza/storage/framework/views/6d7695d270d0f086ac44cf85aac26053.php:1",
            "/var/www/magaza/vendor/composer/ClassLoader.php:429",
        );
        assert_eq!(ozet(&iz(sablonda)), Some(("FatalError", None)));
        assert_eq!(iz(sablonda), iz(&vendorda));
    }

    #[test]
    fn sablon_uygulamada_degilse_yigin_izine_bakilir() {
        // Bir paketin şablonu: vendor klasöründe
        let paket = VIEW_HATASI.replace(
            "/var/www/magaza/resources/views/panel/kampanyalar.blade.php",
            "/var/www/magaza/vendor/firma/panel/resources/views/liste.blade.php",
        );
        assert_eq!(ozet(&iz(&paket)), Some(("ViewException", None)));
        // Bozuk ek: yol yok
        let bos = VIEW_HATASI.replace(
            "(View: /var/www/magaza/resources/views/panel/kampanyalar.blade.php)",
            "(View: )",
        );
        assert_eq!(ozet(&iz(&bos)), Some(("ViewException", None)));
        // View hatası olmayan bir istisnanın mesajındaki `(View: ...)` şablon sayılmaz
        let baska = VIEW_HATASI.replace(r"Illuminate\\View\\ViewException", "RuntimeException");
        assert_eq!(ozet(&iz(&baska)), Some(("RuntimeException", None)));
    }

    #[test]
    fn sablon_yolu_ve_klasoru_bulunur() {
        assert_eq!(
            sablon_yolu("x (View: /a/resources/views/b.blade.php) at /c.php:1)"),
            Some("/a/resources/views/b.blade.php")
        );
        // Klasör adındaki parantez yolu bölmez
        assert_eq!(
            sablon_yolu(r"x (View: C:\\Projeler (eski)\\magaza\\resources\\views\\a.blade.php)"),
            Some(r"C:\\Projeler (eski)\\magaza\\resources\\views\\a.blade.php")
        );
        assert_eq!(sablon_yolu("x (View: ) (View: /a/b.blade.php)"), None);
        assert_eq!(sablon_yolu("x (View: /a/b.html)"), None);
        assert_eq!(sablon_yolu("View: /a/b.blade.php)"), None);

        assert_eq!(
            sablon_klasorunden("/srv/magaza/resources/views/a/b.blade.php"),
            Some("resources/views/a/b.blade.php")
        );
        assert_eq!(
            sablon_klasorunden(r"C:\magaza\resources\views\a.blade.php"),
            Some(r"resources\views\a.blade.php")
        );
        assert_eq!(
            sablon_klasorunden("/srv/magaza/sablonlar/a.blade.php"),
            None
        );
    }

    /// Yığın izi olmayan ölümcül hata: sınıf tanımında eksik bir trait. Yalnızca fırlatıldığı
    /// yer bilinir ve orada vendor geçmez.
    const OLUMCUL: &str = r#"[2026-10-26 13:44:17] local.ERROR: Trait "App\Traits\Disariaktarilabilir" not found {"exception":"[object] (Symfony\\Component\\ErrorHandler\\Error\\FatalError(code: 0): Trait \"App\\Traits\\Disariaktarilabilir\" not found at /var/www/magaza/app/Http/Controllers/RaporController.php:5)
[stacktrace]
#0 {main}
"} "#;

    #[test]
    fn kok_logun_baska_kayitlarindan_bulunur() {
        // Tek başına: kök bilinmez, yol olduğu gibi
        assert_eq!(
            ozet(&iz(OLUMCUL)),
            Some((
                "FatalError",
                Some("/var/www/magaza/app/Http/Controllers/RaporController.php:5".to_string())
            ))
        );

        // Aynı logda vendor'dan geçen bir kayıt varsa kök oradan bilinir
        let log_metni = format!("{OLUMCUL}\n{VENDORDA_FIRLATILAN}");
        let izler = izler(&log_metni);
        assert_eq!(
            izler[0].as_ref().map(|iz| iz.to_string()),
            Some("app/Http/Controllers/RaporController.php:5 (FatalError)".to_string())
        );

        // İki farklı kökten (iki bilgisayar) aynı hata: köke göre aynı yer, aynı parmak izi.
        // Kökü bilinmeyen bir yoldaki kayıt olduğu gibi kalır.
        let ikinci_bilgisayar =
            |metin: &str| metin.replace("/var/www/magaza/", "/home/can/magaza/");
        let log_metni = [
            OLUMCUL.to_string(),
            VENDORDA_FIRLATILAN.to_string(),
            ikinci_bilgisayar(OLUMCUL),
            ikinci_bilgisayar(VENDORDA_FIRLATILAN),
            OLUMCUL.replace("/var/www/magaza/", "/srv/baska/"),
        ]
        .join("\n");
        let izler = super::testler::izler(&log_metni);
        assert_eq!(izler.len(), 5);
        assert_eq!(izler[0], izler[2]);
        assert_eq!(
            izler[4].as_ref().map(|iz| iz.to_string()),
            Some("/srv/baska/app/Http/Controllers/RaporController.php:5 (FatalError)".to_string())
        );
    }

    #[test]
    fn ic_ice_koklerden_en_uzunu_secilir() {
        let istisna = Istisna {
            sinif: Cow::Borrowed("FatalError"),
            adaylar: vec![Aday {
                konum: Konum {
                    dosya: Cow::Borrowed("/srv/is/magaza/app/A.php"),
                    satir: Some(3),
                },
                iletici: false,
                metot: None,
            }],
            sablon: None,
        };
        let kokler = |kokler: &[&str]| -> BTreeSet<String> {
            kokler.iter().map(|kok| kok.to_string()).collect()
        };
        assert_eq!(
            istisna.bilinen_kok(&kokler(&["/srv/is/", "/srv/is/magaza/", "/srv/baska/"])),
            Some("/srv/is/magaza/")
        );
        assert_eq!(istisna.bilinen_kok(&kokler(&["/srv/baska/"])), None);
        // Kök yolun başında olmalı: bir yedeğin içindeki aynı klasör adı kök değildir
        assert_eq!(istisna.bilinen_kok(&kokler(&["/is/magaza/"])), None);
        // Eşit uzunlukta iki kök uyuyorsa sıralamada sonuncusu
        let iki_proje = Istisna {
            adaylar: vec![
                Aday {
                    konum: Konum {
                        dosya: Cow::Borrowed("/srv/aa/A.php"),
                        satir: Some(1),
                    },
                    iletici: false,
                    metot: None,
                },
                Aday {
                    konum: Konum {
                        dosya: Cow::Borrowed("/srv/bb/B.php"),
                        satir: Some(2),
                    },
                    iletici: false,
                    metot: None,
                },
            ],
            ..istisna
        };
        assert_eq!(
            iki_proje.bilinen_kok(&kokler(&["/srv/bb/", "/srv/aa/"])),
            Some("/srv/bb/")
        );
    }

    /// `UYGULAMADA_FIRLATILAN` ile aynı hata, `Log::error($e)` ile loglanmış
    const ISTISNA_METNI: &str = r#"[2026-10-24 15:02:11] production.ERROR: ErrorException: Undefined array key "kargo_ucreti" in /var/www/magaza/app/Services/SepetHesaplayici.php:88
Stack trace:
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Foundation/Bootstrap/HandleExceptions.php(256): Illuminate\Foundation\Bootstrap\HandleExceptions->handleError(2, 'Undefined array...', '/var/www/magaza...', 88)
#1 /var/www/magaza/app/Services/SepetHesaplayici.php(88): Illuminate\Foundation\Bootstrap\HandleExceptions->Illuminate\Foundation\Bootstrap\{closure}(2, 'Undefined array...', '/var/www/magaza...', 88)
#2 /var/www/magaza/app/Http/Controllers/SepetController.php(41): App\Services\SepetHesaplayici->toplam(Object(App\Models\Sepet))
#3 {main}  "#;

    #[test]
    fn istisna_metninin_parmak_izi_baglamdakiyle_ayni() {
        assert!(iz(ISTISNA_METNI).is_some());
        assert_eq!(iz(ISTISNA_METNI), iz(UYGULAMADA_FIRLATILAN));
        // Windows'ta, vendor'da fırlatılmış: yollar tek ters bölüyle
        let windows = r#"[2026-10-20 08:37:53] local.ERROR: Illuminate\Database\QueryException: SQLSTATE[HY000]: General error: 1824 Failed to open the referenced table 'kategoriler' in C:\Users\İrem\Herd\magaza\vendor\laravel\framework\src\Illuminate\Database\Connection.php:822
Stack trace:
#0 C:\Users\İrem\Herd\magaza\vendor\laravel\framework\src\Illuminate\Database\Connection.php(776): Illuminate\Database\Connection->runQueryCallback('alter table `ur...', Array, Object(Closure))
#1 C:\Users\İrem\Herd\magaza\vendor\laravel\framework\src\Illuminate\Support\Facades\Facade.php(361): Illuminate\Database\Schema\Builder->create('urunler', Object(Closure))
#2 C:\Users\İrem\Herd\magaza\database\migrations\2026_10_20_083604_create_urunler_table.php(11): Illuminate\Support\Facades\Facade::__callStatic('create', Array)
#3 C:\Users\İrem\Herd\magaza\vendor\laravel\framework\src\Illuminate\Database\Migrations\Migrator.php(514): Illuminate\Database\Migrations\Migration@anonymous->up()
#4 C:\Users\İrem\Herd\magaza\artisan(16): Illuminate\Foundation\Application->handleCommand(Object(Symfony\Component\Console\Input\ArgvInput))
#5 {main}  "#;
        assert!(iz(windows).is_some());
        assert_eq!(iz(windows), iz(WINDOWS_MIGRATION));
        assert_eq!(
            iz(windows).unwrap().sinif(),
            r"Illuminate\Database\QueryException"
        );
        // Yollar JSON kaçışı sayılmaz: `\r`, `\t`, `\b` yolun harfleridir
        let rota = r"[2026-10-25 17:38:17] local.ERROR: RuntimeException: Dize olarak loglanan istisna in C:\Users\İrem\Herd\magaza\routes\web.php:24
Stack trace:
#0 C:\Users\İrem\Herd\magaza\vendor\laravel\framework\src\Illuminate\Routing\CallableDispatcher.php(39): Illuminate\Routing\RouteFileRegistrar->{closure}()
#1 C:\Users\İrem\Herd\magaza\tests\bootstrap.php(5): x()
#2 {main}  ";
        assert_eq!(
            iz(rota).map(|iz| iz.to_string()),
            Some("routes/web.php:24, {closure} (RuntimeException)".to_string())
        );
        // Yığın izindeki yollar da: istisna vendor'da fırlatıldı, uygulama kodu bir çerçevede
        let cercevede = r"[2026-10-25 17:38:17] local.ERROR: Illuminate\Database\QueryException: x in C:\Users\İrem\Herd\magaza\vendor\laravel\framework\src\Illuminate\Database\Connection.php:825
Stack trace:
#0 C:\Users\İrem\Herd\magaza\routes\web.php(24): Illuminate\Database\Connection->select()
#1 {main}";
        assert_eq!(
            iz(cercevede).map(|iz| iz.to_string()),
            Some("routes/web.php:24 (QueryException)".to_string())
        );
        // Metin biçimindeki bağlam istisnası önce gelir: ikisi birden varsa ondan okunur
        let baglam = &UYGULAMADA_FIRLATILAN[UYGULAMADA_FIRLATILAN.find(" {").unwrap()..];
        let ikisi = format!("{}{baglam}", ISTISNA_METNI.trim_end());
        assert_eq!(iz(&ikisi), iz(UYGULAMADA_FIRLATILAN));
    }

    #[test]
    fn istisna_metninde_middleware_ve_sablon() {
        let middleware = r"[2026-10-26 13:44:16] local.ERROR: Error: Call to undefined method App\Http\Controllers\KampanyaController::duzenle() in /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/ControllerDispatcher.php:46
Stack trace:
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/Route.php(276): Illuminate\Routing\ControllerDispatcher->dispatch()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Pipeline/Pipeline.php(180): Illuminate\Routing\Router->Illuminate\Routing\{closure}()
#2 /var/www/magaza/app/Http/Middleware/YoneticiMi.php(13): Illuminate\Pipeline\Pipeline->Illuminate\Pipeline\{closure}()
#3 /var/www/magaza/public/index.php(20): Illuminate\Foundation\Application->handleRequest()
#4 {main}";
        assert_eq!(ozet(&iz(middleware)), Some(("Error", None)));
        assert_eq!(iz(middleware), iz(MIDDLEWARE_ARDINDA));

        let sablon = r"[2026-10-26 13:44:16] local.ERROR: Illuminate\View\ViewException: Route [panel.kampanya.olustur] not defined. (View: C:\Users\İrem\Herd\magaza\resources\views\panel\kampanyalar.blade.php) in C:\Users\İrem\Herd\magaza\vendor\laravel\framework\src\Illuminate\Routing\UrlGenerator.php:542
Stack trace:
#0 C:\Users\İrem\Herd\magaza\app\Http\Middleware\YoneticiMi.php(13): Illuminate\Pipeline\Pipeline->Illuminate\Pipeline\{closure}()
#1 {main}";
        assert_eq!(
            iz(sablon).map(|iz| iz.to_string()),
            Some("resources/views/panel/kampanyalar.blade.php (ViewException)".to_string())
        );
    }

    #[test]
    fn istisna_metninde_distaki_istisna_kullanilir() {
        // `onceki_istisnanin_izi_kullanilmaz` ile aynı hata: içteki istisnanın izindeki
        // uygulama satırı alınmamalı
        let metin = r"[2026-10-24 16:10:02] production.ERROR: GuzzleHttp\Exception\ConnectException: cURL error 28 in /var/www/magaza/app/Services/OdemeServisi.php:64
Stack trace:
#0 /var/www/magaza/app/Services/OdemeServisi.php(64): GuzzleHttp\Client->send(Object(GuzzleHttp\Psr7\Request))
#1 {main}

Next Illuminate\Http\Client\ConnectionException: cURL error 28 in /var/www/magaza/vendor/laravel/framework/src/Illuminate/Http/Client/PendingRequest.php:1641
Stack trace:
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Http/Client/PendingRequest.php(1013): Illuminate\Http\Client\PendingRequest->marshalConnectionException(Object(GuzzleHttp\Exception\ConnectException))
#1 {main}";
        assert_eq!(ozet(&iz(metin)), Some(("ConnectionException", None)));
    }

    #[test]
    fn parmak_izi_okunabilir_yazilir() {
        let parmak_izi = iz(UYGULAMADA_FIRLATILAN).unwrap();
        assert_eq!(
            parmak_izi.to_string(),
            "app/Services/SepetHesaplayici.php:88, toplam() (ErrorException)"
        );
    }

    #[test]
    fn satir_kaysa_da_parmak_izi_ayni() {
        // Dosyanın üstüne üç satır eklendi: hata aynı fonksiyonda, satırı 91 oldu
        let kaymis = UYGULAMADA_FIRLATILAN
            .replace("SepetHesaplayici.php:88", "SepetHesaplayici.php:91")
            .replace("SepetHesaplayici.php(88)", "SepetHesaplayici.php(91)");
        assert_ne!(kaymis, UYGULAMADA_FIRLATILAN);
        let (once, sonra) = (iz(UYGULAMADA_FIRLATILAN).unwrap(), iz(&kaymis).unwrap());
        assert_eq!(once, sonra);
        // Gösterilen satır her kaydın kendi satırı; kimliğe katılmaz
        assert_eq!(
            sonra.to_string(),
            "app/Services/SepetHesaplayici.php:91, toplam() (ErrorException)"
        );
        assert_eq!(once.gosterilen_satir(), Some(88));
        assert_eq!(sonra.gosterilen_satir(), Some(91));
        assert_eq!(once.metot(), Some(r"App\Services\SepetHesaplayici->toplam"));

        // `--konum satir`: dosya ve satır, 0.1 sürümündeki gibi
        let satirla = |metin: &str| izler_turle(metin, KonumTuru::Satir).remove(0).unwrap();
        assert_ne!(satirla(UYGULAMADA_FIRLATILAN), satirla(&kaymis));
        assert_eq!(
            satirla(UYGULAMADA_FIRLATILAN).to_string(),
            "app/Services/SepetHesaplayici.php:88 (ErrorException)"
        );
        assert_eq!(satirla(UYGULAMADA_FIRLATILAN).metot(), None);
    }

    #[test]
    fn ayni_dosyada_baska_fonksiyon_baska_hatadir() {
        let baska = UYGULAMADA_FIRLATILAN.replace(
            r"App\\Services\\SepetHesaplayici->toplam(",
            r"App\\Services\\SepetHesaplayici->kargoUcreti(",
        );
        assert_ne!(baska, UYGULAMADA_FIRLATILAN);
        assert_ne!(iz(&baska), iz(UYGULAMADA_FIRLATILAN));
        assert_eq!(
            ozet(&iz(&baska)),
            Some((
                "ErrorException",
                Some("app/Services/SepetHesaplayici.php:88, kargoUcreti()".to_string())
            ))
        );
        // Başka bir istisna sınıfı da başka bir hatadır
        let sinif = UYGULAMADA_FIRLATILAN.replace("(ErrorException(code", "(TypeError(code");
        assert_ne!(iz(&sinif), iz(UYGULAMADA_FIRLATILAN));
    }

    /// Bir closure'da fırlatılan istisna (Laravel 13, PHP 8.3): `DB::transaction()`'a verilen
    /// closure `SiparisServisi->olustur()`'da tanımlı.
    const CLOSURE_ICINDE: &str = r#"[2026-10-26 23:37:06] local.ERROR: Sipariş işlenemedi {"exception":"[object] (RuntimeException(code: 0): Sipariş işlenemedi at /var/www/magaza/app/Services/SiparisServisi.php:15)
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Concerns/ManagesTransactions.php(35): App\\Services\\SiparisServisi->App\\Services\\{closure}()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/DatabaseManager.php(502): Illuminate\\Database\\Connection->transaction()
#2 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Support/Facades/Facade.php(364): Illuminate\\Database\\DatabaseManager->__call()
#3 /var/www/magaza/app/Services/SiparisServisi.php(12): Illuminate\\Support\\Facades\\Facade::__callStatic()
#4 /var/www/magaza/app/Http/Controllers/SiparisController.php(11): App\\Services\\SiparisServisi->olustur()
#5 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/ControllerDispatcher.php(46): App\\Http\\Controllers\\SiparisController->olustur()
#6 /var/www/magaza/public/index.php(20): Illuminate\\Foundation\\Application->handleRequest()
#7 {main}
"} "#;

    /// Aynı sınıfın başka bir metodundaki closure: `Collection->map()`'e verilmiş, PHP'nin
    /// kendisi çağırıyor (`[internal function]`).
    const BASKA_CLOSURE: &str = r#"[2026-10-26 23:37:10] local.ERROR: Sipariş işlenemedi {"exception":"[object] (RuntimeException(code: 0): Sipariş işlenemedi at /var/www/magaza/app/Services/SiparisServisi.php:25)
[stacktrace]
#0 [internal function]: App\\Services\\SiparisServisi->App\\Services\\{closure}()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Collections/Arr.php(868): array_map()
#2 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Collections/Collection.php(844): Illuminate\\Support\\Arr::map()
#3 /var/www/magaza/app/Services/SiparisServisi.php(23): Illuminate\\Support\\Collection->map()
#4 /var/www/magaza/app/Http/Controllers/SiparisController.php(18): App\\Services\\SiparisServisi->iptalEt()
#5 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/ControllerDispatcher.php(46): App\\Http\\Controllers\\SiparisController->iptal()
#6 {main}
"} "#;

    #[test]
    fn closure_tanimlandigi_fonksiyonla_ayrilir() {
        // İki closure da `App\Services\SiparisServisi->App\Services\{closure}` diye yazılıyor;
        // tanımlandıkları fonksiyonlar yığın izinden bulunur
        let olustur = iz(CLOSURE_ICINDE).unwrap();
        let iptal = iz(BASKA_CLOSURE).unwrap();
        assert_eq!(
            olustur.to_string(),
            "app/Services/SiparisServisi.php:15, {closure:olustur()} (RuntimeException)"
        );
        assert_eq!(
            iptal.to_string(),
            "app/Services/SiparisServisi.php:25, {closure:iptalEt()} (RuntimeException)"
        );
        assert_ne!(olustur, iptal);
        assert_eq!(
            olustur.metot(),
            Some(r"App\Services\SiparisServisi->{closure:olustur()}")
        );

        // Closure'ı tanımlayan satır yığın izinde yoksa sahibi bilinmez
        let sahipsiz = CLOSURE_ICINDE.replace(
            r"#3 /var/www/magaza/app/Services/SiparisServisi.php(12): Illuminate\\Support\\Facades\\Facade::__callStatic()",
            r"#3 /var/www/magaza/app/Services/Baska.php(12): Illuminate\\Support\\Facades\\Facade::__callStatic()",
        );
        assert_ne!(sahipsiz, CLOSURE_ICINDE);
        assert_eq!(
            ozet(&iz(&sahipsiz)),
            Some((
                "RuntimeException",
                Some("app/Services/SiparisServisi.php:15, {closure}".to_string())
            ))
        );
    }

    #[test]
    fn vendorda_firlatilan_istisnada_closure_cagrisinin_sahibi() {
        // Sorgu vendor'da hata veriyor; uygulama kodundaki ilk satır bir closure'ın içinde
        // (`#1`). Closure, `iptalEt()`'in çağırdığı `durumGuncelle()`'de `map()`'e verilmiş:
        // dışa doğru aynı dosyadaki ilk satır (`#5`) onu tanımlayan fonksiyonun içindedir.
        let metin = r#"[2026-10-26 23:40:00] local.ERROR: SQLSTATE[HY000]: General error {"exception":"[object] (Illuminate\\Database\\QueryException(code: HY000): SQLSTATE[HY000]: General error at /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php:824)
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php(778): Illuminate\\Database\\Connection->runQueryCallback()
#1 /var/www/magaza/app/Services/SiparisServisi.php(43): Illuminate\\Database\\Query\\Builder->update()
#2 [internal function]: App\\Services\\SiparisServisi->App\\Services\\{closure}()
#3 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Collections/Arr.php(868): array_map()
#4 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Collections/Collection.php(844): Illuminate\\Support\\Arr::map()
#5 /var/www/magaza/app/Services/SiparisServisi.php(40): Illuminate\\Support\\Collection->map()
#6 /var/www/magaza/app/Services/SiparisServisi.php(23): App\\Services\\SiparisServisi->durumGuncelle()
#7 /var/www/magaza/app/Http/Controllers/SiparisController.php(18): App\\Services\\SiparisServisi->iptalEt()
#8 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/ControllerDispatcher.php(46): App\\Http\\Controllers\\SiparisController->iptal()
#9 {main}
"} "#;
        assert_eq!(
            iz(metin).map(|iz| iz.to_string()),
            Some(
                "app/Services/SiparisServisi.php:43, {closure:durumGuncelle()} (QueryException)"
                    .to_string()
            )
        );
    }

    #[test]
    fn php84_closure_adlari_ayni_parmak_izini_verir() {
        // PHP 8.4 closure'ın adına tanımlandığı fonksiyonu ve satırı yazar; satır değişse de
        // parmak izi değişmez, 8.3'teki gibidir
        for (metin, sahibi) in [
            (CLOSURE_ICINDE, "olustur():12"),
            (BASKA_CLOSURE, "iptalEt():23"),
        ] {
            let php84 = metin.replace(
                r"App\\Services\\SiparisServisi->App\\Services\\{closure}()",
                &format!(r"App\\Services\\SiparisServisi->{{closure:App\\Services\\SiparisServisi::{sahibi}}}()"),
            );
            assert_ne!(php84, metin);
            assert_eq!(iz(&php84), iz(metin), "{sahibi}");
            assert_eq!(
                iz(&php84).unwrap().metot(),
                iz(metin).unwrap().metot(),
                "{sahibi}"
            );
            // 8.4'te sahibi addan okunur: yığın izinde tanımlandığı satır olmasa da
            let yalniz = php84.replace("/app/Services/SiparisServisi.php(", "/app/Baska.php(");
            assert_eq!(iz(&yalniz), iz(metin), "{sahibi}");
        }
    }

    /// Rota dosyasındaki bir closure (Laravel 13, PHP 8.3). Rota dosyası
    /// `RouteFileRegistrar`'ın içinde dahil edilir: closure'larının sınıfı odur.
    const ROTA_CLOSURE: &str = r#"[2026-10-26 23:37:10] local.ERROR: Kapanış rotasında hata {"exception":"[object] (RuntimeException(code: 0): Kapanış rotasında hata at /var/www/magaza/routes/web.php:22)
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/CallableDispatcher.php(39): Illuminate\\Routing\\RouteFileRegistrar->{closure}()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/Route.php(254): Illuminate\\Routing\\CallableDispatcher->dispatch()
#2 /var/www/magaza/public/index.php(20): Illuminate\\Foundation\\Application->handleRequest()
#3 {main}
"} "#;

    #[test]
    fn rota_closure_ve_en_ust_duzeydeki_kod() {
        assert_eq!(
            iz(ROTA_CLOSURE).map(|iz| iz.to_string()),
            Some("routes/web.php:22, {closure} (RuntimeException)".to_string())
        );
        // PHP 8.4: rota dosyasının en üst düzeyinde tanımlanan closure'ın adında dosyanın yolu
        // var; sahibi yok
        let php84 = ROTA_CLOSURE.replace(
            r"RouteFileRegistrar->{closure}()",
            r"RouteFileRegistrar->{closure:/var/www/magaza/routes/web.php:21}()",
        );
        assert_ne!(php84, ROTA_CLOSURE);
        assert_eq!(iz(&php84), iz(ROTA_CLOSURE));

        // Rota dosyası yüklenirken en üst düzeyde oluşan hata: kod bir fonksiyonda değil,
        // konum satırla belirlenir
        let yuklenirken = r#"[2026-10-26 23:40:00] local.ERROR: Class "App\Http\Controllers\Yok" not found {"exception":"[object] (Error(code: 0): Class \"App\\Http\\Controllers\\Yok\" not found at /var/www/magaza/routes/web.php:30)
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/RouteFileRegistrar.php(35): require()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/Router.php(511): Illuminate\\Routing\\RouteFileRegistrar->register()
#2 {main}
"} "#;
        assert_eq!(
            iz(yuklenirken).map(|iz| iz.to_string()),
            Some("routes/web.php:30 (Error)".to_string())
        );
        // Çalıştırılan bir betiğin en üst düzeyi: `{main}`
        let betik = UYGULAMADA_FIRLATILAN.replace(
            r"#2 /var/www/magaza/app/Http/Controllers/SepetController.php(41): App\\Services\\SepetHesaplayici->toplam(Object(App\\Models\\Sepet))",
            "#2 {main}",
        );
        assert_ne!(betik, UYGULAMADA_FIRLATILAN);
        assert_eq!(
            iz(&betik).map(|iz| iz.to_string()),
            Some("app/Services/SepetHesaplayici.php:88 (ErrorException)".to_string())
        );
    }

    #[test]
    fn cagrilarin_fonksiyonu_ayristirilir() {
        let adli = |ad: &'static str| Some(Fonksiyon::Adli(Cow::Borrowed(ad)));
        let closure = |sinif: &'static str, sahibi: Option<&'static str>| {
            Some(Fonksiyon::Closure {
                sinif: Cow::Borrowed(sinif),
                sahibi: sahibi.map(Cow::Borrowed),
            })
        };
        assert_eq!(
            fonksiyon(r"App\Sepet->toplam(Object(App\Models\Sepet), 'a(b')", false),
            adli(r"App\Sepet->toplam")
        );
        assert_eq!(fonksiyon(r"App\Urun::bul()", false), adli(r"App\Urun::bul"));
        assert_eq!(fonksiyon(r"App\yardimci()", false), adli(r"App\yardimci"));
        assert_eq!(fonksiyon("array_map()", false), adli("array_map"));
        // Bağlamın JSON'unda ters bölüler ikilenir
        assert_eq!(
            fonksiyon(r"App\\Sepet->toplam()", true),
            adli(r"App\Sepet->toplam")
        );
        assert_eq!(
            fonksiyon(r"App\\X->App\\{closure}()", true),
            closure(r"App\X->", None)
        );
        assert_eq!(
            fonksiyon(r"App\X::App\{closure}()", false),
            closure(r"App\X::", None)
        );
        assert_eq!(fonksiyon("{closure}()", false), closure("", None));
        assert_eq!(
            fonksiyon(
                r"App\X->{closure:App\X::olustur():25}(Object(Closure))",
                false
            ),
            closure(r"App\X->", Some("olustur"))
        );
        assert_eq!(
            fonksiyon(r"{closure:App\yardimci():5}()", false),
            closure("", Some(r"App\yardimci"))
        );
        // İç içe closure: PHP dıştakinin adını yazar
        assert_eq!(
            fonksiyon(
                r"App\X->{closure:{closure:App\X::olustur():25}:26}()",
                false
            ),
            closure(r"App\X->", Some("olustur"))
        );
        assert_eq!(
            fonksiyon(r"{closure:{closure:/var/www/routes/web.php:3}:4}()", false),
            closure("", None)
        );
        // Windows yolu: sürücü harfinden sonraki `:` satır sanılmaz
        assert_eq!(
            fonksiyon(
                r"{closure:C:\Users\İrem\Herd\magaza\routes\web.php:21}()",
                false
            ),
            closure("", None)
        );
        for en_ust in [
            "{main}",
            "require('/var/www/x.php')",
            "include_once()",
            "require()",
        ] {
            assert_eq!(fonksiyon(en_ust, false), Some(Fonksiyon::EnUst), "{en_ust}");
        }
        assert_eq!(fonksiyon("", false), None);
        assert_eq!(fonksiyon("(x)", false), None);

        assert_eq!(closure_sahibi("{closure:App\\X::y():3"), None);
        assert_eq!(closure_sahibi("{closure:App\\X::y():}"), None);
        assert_eq!(closure_sahibi("{closure:App\\X::y():3a}"), None);
        assert_eq!(closure_sahibi("{closure}"), None);
        assert_eq!(sahip_adi(r"App\X->olustur"), Some("olustur"));
        assert_eq!(sahip_adi(r"App\X::bul"), Some("bul"));
        assert_eq!(sahip_adi(r"App\yardimci"), Some(r"App\yardimci"));
        assert_eq!(sahip_adi(r"App\X->{closure:olustur()}"), Some("olustur"));
        assert_eq!(sahip_adi(r"App\X->{closure}"), None);
    }

    #[test]
    fn metodun_kimligi_ve_kisa_adi() {
        for (metot, kimlik, kisa) in [
            (
                r"App\Services\SepetHesaplayici->toplam",
                "toplam",
                "toplam()",
            ),
            (r"App\Services\Odeme->al", "al", "al()"),
            (r"App\Models\Urun::bul", "bul", "bul()"),
            (
                r"App\Support\yardimci",
                r"App\Support\yardimci",
                "yardimci()",
            ),
            (
                r"Illuminate\Database\Migrations\Migration@anonymous->up",
                "up",
                "up()",
            ),
            (
                r"Illuminate\Routing\RouteFileRegistrar->{closure}",
                "{closure}",
                "{closure}",
            ),
            (
                r"App\Services\SiparisServisi->{closure:olustur()}",
                "{closure:olustur()}",
                "{closure:olustur()}",
            ),
            (
                "{closure:App\\yardimci()}",
                "{closure:App\\yardimci()}",
                "{closure:App\\yardimci()}",
            ),
        ] {
            assert_eq!(metot_kimligi(metot), kimlik, "{metot}");
            assert_eq!(kisa_metot(kimlik), kisa, "{metot}");
        }
    }

    /// `UYGULAMADA_FIRLATILAN` ile aynı hata, Monolog'un JSON biçiminde: yığın izinde çağrılar
    /// yok
    const JSON_UYGULAMADA: &str = r#"{"message":"Undefined array key \"kargo_ucreti\"","context":{"exception":{"class":"ErrorException","message":"Undefined array key \"kargo_ucreti\"","code":0,"file":"/var/www/magaza/app/Services/SepetHesaplayici.php:88","trace":["/var/www/magaza/vendor/laravel/framework/src/Illuminate/Foundation/Bootstrap/HandleExceptions.php:256","/var/www/magaza/app/Services/SepetHesaplayici.php:88","/var/www/magaza/app/Http/Controllers/SepetController.php:41"]}},"level":400,"level_name":"ERROR","channel":"production","datetime":"2026-10-24T15:05:00.000000+00:00","extra":{}}"#;

    #[test]
    fn cagrisiz_kayit_baska_kaydin_yigin_izinden_tamamlanir() {
        // Tek başına: fonksiyon bilinmez, konum satırla belirlenir
        assert_eq!(
            ozet(&iz(JSON_UYGULAMADA)),
            Some((
                "ErrorException",
                Some("app/Services/SepetHesaplayici.php:88".to_string())
            ))
        );
        // Aynı logda metin biçiminde bir kayıt: satırın fonksiyonu ondan öğrenilir, iki kayıt
        // aynı hatadır. Sıra önemli değil.
        for log in [
            format!("{UYGULAMADA_FIRLATILAN}\n{JSON_UYGULAMADA}"),
            format!("{JSON_UYGULAMADA}\n{UYGULAMADA_FIRLATILAN}"),
        ] {
            let ikisi = izler(&log);
            assert_eq!(ikisi[0], ikisi[1]);
            assert_eq!(
                ikisi[0].as_ref().unwrap().metot(),
                Some(r"App\Services\SepetHesaplayici->toplam")
            );
        }
        // Metin kaydı başka bir hatanın: satır onun yığın izindeki bir çerçeve olsa da öğrenilir
        let cagiran = JSON_UYGULAMADA.replace(
            r#""file":"/var/www/magaza/app/Services/SepetHesaplayici.php:88""#,
            r#""file":"/var/www/magaza/app/Http/Controllers/SepetController.php:41""#,
        );
        let gecen = UYGULAMADA_FIRLATILAN.replace(
            r"#2 /var/www/magaza/app/Http/Controllers/SepetController.php(41): App\\Services\\SepetHesaplayici->toplam(Object(App\\Models\\Sepet))",
            r"#2 /var/www/magaza/app/Http/Controllers/SepetController.php(41): App\\Services\\SepetHesaplayici->toplam(Object(App\\Models\\Sepet))
#3 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/ControllerDispatcher.php(46): App\\Http\\Controllers\\SepetController->goster()",
        );
        let gecenle = izler(&format!("{gecen}\n{cagiran}"));
        assert_eq!(
            gecenle[1].as_ref().map(|iz| iz.to_string()),
            Some(
                "app/Http/Controllers/SepetController.php:41, goster() (ErrorException)"
                    .to_string()
            )
        );

        // Satır iki farklı fonksiyonda görüldüyse (log bir dağıtımı aşıyor) belirsizdir:
        // kullanılmaz
        let baska_fonksiyon = UYGULAMADA_FIRLATILAN.replace(
            r"App\\Services\\SepetHesaplayici->toplam(",
            r"App\\Services\\SepetHesaplayici->kargoUcreti(",
        );
        let belirsiz = izler(&format!(
            "{UYGULAMADA_FIRLATILAN}\n{baska_fonksiyon}\n{JSON_UYGULAMADA}"
        ));
        assert_eq!(
            ozet(&belirsiz[2]),
            Some((
                "ErrorException",
                Some("app/Services/SepetHesaplayici.php:88".to_string())
            ))
        );
        // Fonksiyonu bilinen kayıtlar haritaya bakmaz: 88. satır sonradan başka bir fonksiyona
        // kaymış olsa da (belirsiz) `toplam()`'daki iki satır aynı hatadır
        let kaymis = UYGULAMADA_FIRLATILAN
            .replace("SepetHesaplayici.php:88", "SepetHesaplayici.php:91")
            .replace("SepetHesaplayici.php(88)", "SepetHesaplayici.php(91)");
        let ucu = izler(&format!(
            "{UYGULAMADA_FIRLATILAN}\n{kaymis}\n{baska_fonksiyon}"
        ));
        assert_eq!(ucu[0], ucu[1]);
        assert_ne!(ucu[0], ucu[2]);
        // `--konum satir`: haritaya bakılmaz
        let satirla = izler_turle(
            &format!("{UYGULAMADA_FIRLATILAN}\n{JSON_UYGULAMADA}"),
            KonumTuru::Satir,
        );
        assert_eq!(satirla[0], satirla[1]);
        assert_eq!(satirla[1].as_ref().unwrap().metot(), None);
    }

    /// Trait'teki bir metotta fırlatılan istisna; `{sinif}` metodu kullanan sınıf.
    fn trait_hatasi(sinif: &str, denetleyici: &str, saniye: u32) -> String {
        format!(
            r#"[2026-10-24 10:00:{saniye:02}] production.ERROR: Kaydedilemedi {{"exception":"[object] (RuntimeException(code: 0): Kaydedilemedi at /var/www/magaza/app/Traits/Kaydedilebilir.php:12)
[stacktrace]
#0 /var/www/magaza/app/Http/Controllers/{denetleyici}.php(20): App\\Models\\{sinif}->kaydet()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/Controller.php(54): App\\Http\\Controllers\\{denetleyici}->store()
#2 {{main}}
"}} "#
        )
    }

    #[test]
    fn trait_metodundaki_hata_tek_yerdir() {
        // PHP trait'teki metodu onu kullanan sınıfın adıyla yazar: dosya ve satır aynı, sınıf
        // farklı. Aynı hatadır; çağrısız JSON kaydı da ona katılır.
        let json = r#"{"message":"Kaydedilemedi","context":{"exception":{"class":"RuntimeException","message":"Kaydedilemedi","code":0,"file":"/var/www/magaza/app/Traits/Kaydedilebilir.php:12"}},"level":400,"level_name":"ERROR","channel":"production","datetime":"2026-10-24T10:00:02+00:00","extra":{}}"#;
        let log = format!(
            "{}\n{}\n{json}",
            trait_hatasi("Urun", "UrunController", 0),
            trait_hatasi("Siparis", "SiparisController", 1)
        );
        let ucu = izler(&log);
        assert_eq!(ucu[0], ucu[1]);
        assert_eq!(ucu[1], ucu[2]);
        assert_eq!(
            ucu[0].as_ref().map(ToString::to_string),
            Some("app/Traits/Kaydedilebilir.php:12, kaydet() (RuntimeException)".to_string())
        );
        // Tam ad kayıtların ilkinden
        assert_eq!(
            ucu[0].as_ref().unwrap().metot(),
            Some(r"App\Models\Urun->kaydet")
        );
    }

    #[test]
    fn yolda_parantez_ya_da_at_olsa_da_yer_bulunur() {
        for (baslik, dosya, satir) in [
            (
                "x at /home/ali/projeler (eski)/magaza/app/X.php:12)",
                "/home/ali/projeler (eski)/magaza/app/X.php",
                12,
            ),
            (
                r"x at C:\\Program Files (x86)\\magaza\\app\\X.php:7)",
                r"C:\Program Files (x86)\magaza\app\X.php",
                7,
            ),
            (
                "x at /home/ali/is at ev/magaza/app/X.php:3)",
                "/home/ali/is at ev/magaza/app/X.php",
                3,
            ),
            ("x at y at /srv/a.php:5)", "/srv/a.php", 5),
            ("a.php:5) at /srv/b.php:6)", "/srv/b.php", 6),
            // Mesajda da mutlak bir yer olsa sağdaki seçilir: fırlatıldığı yer en sondadır
            ("x at /srv/a.php:5) at /srv/b.php:6)", "/srv/b.php", 6),
            // Mutlak yolu olmayan bir yer: sağdaki ` at `'inki
            ("x at a.php:5) at b.php:6)", "b.php", 6),
            // `:satır)`'dan önceki iki nokta yolun parçasıdır
            ("x at a:b.php:1:2)", "a:b.php:1", 2),
            ("x at :12)", "", 12),
            // Satır numarası `u32`'ye sığmıyorsa sonraki `)` denenir
            (
                "x at /a.php:99999999999) /b.php:7)",
                "/a.php:99999999999) /b.php",
                7,
            ),
        ] {
            assert_eq!(
                firlatildigi_yer(baslik),
                Some(Konum {
                    dosya: Cow::Borrowed(dosya),
                    satir: Some(satir)
                }),
                "{baslik}"
            );
        }
        for baslik in [
            "x at /srv/a.php)",
            "at yok",
            "x at /a.php:)",
            "x:5 at 6)",
            "",
        ] {
            assert_eq!(firlatildigi_yer(baslik), None, "{baslik}");
        }
        // ` at ` ve `)` ile dolu dev bir mesaj: her ` at `'te satırın geri kalanı yeniden
        // taranmaz (taransa bu satır dakikalar sürerdi)
        let dev = format!("x{} at /a.php:5)", " at y)".repeat(200_000));
        let bas = std::time::Instant::now();
        assert_eq!(
            firlatildigi_yer(&dev),
            Some(Konum {
                dosya: Cow::Borrowed("/a.php"),
                satir: Some(5)
            })
        );
        assert_eq!(firlatildigi_yer(&" at y)".repeat(200_000)), None);
        assert!(bas.elapsed().as_secs() < 5, "{:?}", bas.elapsed());
    }

    #[test]
    fn vendor_adli_bir_klasordeki_proje() {
        // Proje kökü yoldaki son değil, uygulama kodunu kapsayan vendor'dan bulunur
        let log = UYGULAMADA_FIRLATILAN.replace("/var/www/magaza/", "/home/ali/vendor/magaza/");
        assert_eq!(ozet(&iz(&log)), ozet(&iz(UYGULAMADA_FIRLATILAN)));
        // Bir paketin kendi vendor klasörü kökü değiştirmez
        let ic_ice = VENDORDA_FIRLATILAN.replace(
            "#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php(779)",
            "#0 /var/www/magaza/vendor/acme/paket/vendor/baska/x/src/Y.php(5): Y->z()\n#1 /var/www/magaza/vendor/acme/paket/src/A.php(9): A->b()\n#2 /var/www/magaza/vendor/acme/paket/src/A.php(19): A->c()\n#3 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php(779)",
        );
        assert_eq!(ozet(&iz(&ic_ice)), ozet(&iz(VENDORDA_FIRLATILAN)));
        assert!(ozet(&iz(&ic_ice)).unwrap().1.unwrap().starts_with("app/"));
        // Uygulama kodu olmayan bir iz: kök, altında giriş noktası olan adaydır. Paketin kendi
        // vendor'ından önceki kısım (`paket/`) kök sayılsaydı paketin kodu (`src/A.php`)
        // uygulama kodu sanılırdı
        let paketli = r#"[2026-10-24 09:14:20] production.ERROR: Hata {"exception":"[object] (RuntimeException(code: 0): Hata at /var/www/magaza/vendor/acme/paket/vendor/baska/x/src/Y.php:5)
[stacktrace]
#0 /var/www/magaza/vendor/acme/paket/src/A.php(9): Baska\\X\\Y->z()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/ControllerDispatcher.php(46): Acme\\Paket\\A->b()
#2 /var/www/magaza/public/index.php(17): Illuminate\\Foundation\\Application->handleRequest(Object(Illuminate\\Http\\Request))
#3 {main}
"} "#;
        assert_eq!(ozet(&iz(paketli)), Some(("RuntimeException", None)));
    }

    #[test]
    fn bilinen_koklerden_en_uzunu_secilir() {
        // Yollarında vendor olmayan istisnalar: kökleri logun öteki kayıtlarında bulunanlardan
        // seçilir. Yolun başındaki köklerden en uzunu; uzunluklar eşitse sıralamada sonuncusu.
        let log = r#"[2026-10-24 10:00:00] local.ERROR: Hata {"exception":"[object] (RuntimeException(code: 0): Hata at /srv/a/b/app/X.php:3)"}
[2026-10-24 10:00:01] local.ERROR: Hata {"exception":"[object] (RuntimeException(code: 0): Hata at /srv/x/app/A.php:1)
[stacktrace]
#0 /srv/y/app/B.php(2): f()
#1 {main}
"}
[2026-10-24 10:00:02] local.ERROR: Hata {"exception":"[object] (RuntimeException(code: 0): Hata at /baska/app/C.php:5)"}
"#;
        let mut toplayan = Toplayan::default();
        okuyucu::oku(log.as_bytes(), &SatirBicimi::default(), &mut toplayan).unwrap();
        let istisnalar: Vec<Istisna<'_>> = toplayan
            .taslaklar
            .iter()
            .flatten()
            .map(IstisnaTaslagi::istisna)
            .collect();
        assert_eq!(istisnalar.len(), 3);
        let kokler: BTreeSet<String> = [
            "/srv/",
            "/srv/a/",
            "/srv/a/b/",
            "/srv/a/c/",
            "/srv/x/",
            "/srv/y/",
            "/srv/a/b/app/X.php/",
        ]
        .map(String::from)
        .into();
        let bulunan: Vec<Option<&str>> = istisnalar
            .iter()
            .map(|istisna| istisna.bilinen_kok(&kokler))
            .collect();
        assert_eq!(bulunan, [Some("/srv/a/b/"), Some("/srv/y/"), None]);
    }

    #[test]
    fn kok_adaylari_sinirlidir() {
        // Her çerçevesi başka bir köke sahip dev bir yığın izi: ilk adaylar denenir, her aday
        // her yolla karşılaştırılmaz (karşılaştırılsa bu iz dakikalar sürerdi)
        let mut yollar: Vec<String> = (0..200_000)
            .map(|sira| format!("/r{sira}/vendor/x/Y.php"))
            .collect();
        yollar.push("/r0/app/X.php".to_owned());
        let bas = std::time::Instant::now();
        assert_eq!(vendor_koku(yollar.iter().map(String::as_str)), Some("/r0/"));
        assert!(bas.elapsed().as_secs() < 5, "{:?}", bas.elapsed());
    }

    #[test]
    fn konum_turleri() {
        assert_eq!(KonumTuru::default(), KonumTuru::Metot);
        for tur in KonumTuru::HEPSI {
            assert_eq!(KonumTuru::ayristir(tur.ad()), Some(tur));
            assert!(!tur.aciklama().is_empty());
        }
        assert_eq!(KonumTuru::ayristir("dosya"), None);
    }
}
