//! Hata parmak izi: istisna sınıfı ve uygulama kodundaki ilk satır.
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
//! oluştuğuna bakar: istisnanın sınıfı ve uygulama kodunda ilk göründüğü dosya:satır.
//!
//! Monolog'un JSON biçiminde istisna bağlamda bir nesnedir. Yığın izi (`trace`) yalnızca
//! biçimlendiricide `includeStacktraces` açıksa yazılır; kapalıysa yalnızca fırlatıldığı yer
//! bilinir:
//!
//! ```text
//! {"message":"...","context":{"exception":{"class":"Illuminate\\Database\\QueryException","message":"...","code":2002,"file":"/var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php:864","trace":["/var/www/magaza/app/Services/SiparisDeposu.php:11"]}},...}
//! ```
//!
//! Windows'ta (Herd, Laragon) yolların ayracı `\`'dir ve bağlam JSON olduğu için logda `\\`
//! diye yazılır: `C:\\Users\\İrem\\Herd\\magaza\\app\\Models\\Urun.php`. Konum gösterilirken
//! kaçış çözülür ve ayraç `/` olur: `app/Models/Urun.php`.
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
//!   kayıtları) logun başka kayıtlarında bulunan kökler denenir (`parmak_izleri`).
//!
//! Derlenmiş şablonlar uygulama kodu sayılmaz. Şablondaki sıradan bir hata `ViewException` olarak
//! loglanır; derlenmiş şablonda kalan hatalar yalnızca ölümcül hatalardır (`Maximum execution
//! time of 30 seconds exceeded`) ve yerleri rastgeledir: süre o an nerede dolduysa.

use std::borrow::Cow;
use std::fmt;
use std::iter;

use crate::json_okuyucu::{Deger, metni_coz};
use crate::kayit::{Kayit, Log};

/// Her isteğin ya da komutun geçtiği giriş dosyaları. Yığın izinin en dibinde hep bunlar
/// bulunur; hatanın nerede oluştuğu hakkında bilgi vermezler. `server.php` Laravel 8 ve
/// öncesinde `php artisan serve`'ün proje kökündeki giriş dosyasıdır.
const GIRIS_NOKTALARI: [&str; 3] = ["artisan", "public/index.php", "server.php"];

/// Proje kökünü bulmak için aranan vendor klasörü ve ondan önceki ayraç: Linux ve macOS'ta
/// `/`, Windows'ta logda `\\` diye yazılan `\`.
const VENDOR_KLASORLERI: [(&str, &str); 2] = [("/", "/vendor/"), (r"\\", r"\\vendor\\")];

/// Laravel'in derlenmiş şablonları koyduğu klasör ve ondan önceki ayraç. Kaydın hiçbir yolunda
/// vendor yoksa kök bundan bulunur.
const DERLENMIS_SABLON_KLASORLERI: [(&str, &str); 2] = [
    ("/", "/storage/framework/views/"),
    (r"\\", r"\\storage\\framework\\views\\"),
];

/// Köke göre yolu bunlarla başlayan dosyalar uygulama kodu değildir: Composer'ın kurduğu
/// paketler ve Blade'in derlenmiş şablonları.
const UYGULAMA_DISI: [&str; 2] = ["vendor/", "storage/framework/views/"];

/// Kök bilinmediğinde şablonun köke göre yolunu bulmak için aranan klasör ve ondan önceki
/// ayraç. Laravel şablonları varsayılan olarak burada tutar.
const SABLON_KLASORLERI: [(&str, &str); 2] =
    [("/", "/resources/views/"), (r"\\", r"\\resources\\views\\")];

/// Bir dosyadaki bir yer: `app/Services/SepetHesaplayici.php:88`. Şablonlarda satır yoktur:
/// `resources/views/vitrin.blade.php`
///
/// Dosya logda yazıldığı gibi, JSON kaçışlarıyla tutulur (Windows'ta `app\\Models\\Urun.php`).
/// Böylece log metninden ödünç alınabilir ve parmak izi kopyalanabilir (`Copy`) kalır.
/// Gösterilecek hali `dosya_yolu` verir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Konum<'a> {
    dosya: &'a str,
    satir: Option<u32>,
}

impl<'a> Konum<'a> {
    fn dosya_yolu(&self) -> Cow<'a, str> {
        yolu_coz(self.dosya)
    }
}

impl fmt::Display for Konum<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.satir {
            Some(satir) => write!(f, "{}:{satir}", self.dosya_yolu()),
            None => write!(f, "{}", self.dosya_yolu()),
        }
    }
}

/// Konum adayı: istisnanın fırlatıldığı yer ya da yığın izindeki bir satır.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Aday<'a> {
    konum: Konum<'a>,
    /// Satır isteği yalnızca bir sonraki adıma mı iletiyor (`return $next($request);`)?
    iletici: bool,
}

/// Bir hatanın kimliği: hangi istisna, uygulama kodunun neresinde?
///
/// `Hash` ve `Eq` sayesinde bir HashMap'te anahtar olarak kullanılabilir: iki kayıt aynı
/// sınıftan ve aynı konumdansa parmak izleri eşittir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParmakIzi<'a> {
    /// Log'da yazıldığı gibi: `Illuminate\\Database\\QueryException`. Bağlam JSON olduğu
    /// için `\` işareti `\\` olarak yazılır.
    sinif: &'a str,
    /// Uygulama kodundaki ilk satır. Hata tamamen vendor içinde oluştuysa None.
    konum: Option<Konum<'a>>,
}

impl<'a> ParmakIzi<'a> {
    /// Ad alanı olmadan sınıf adı: `Illuminate\\Database\\QueryException` → `QueryException`
    pub fn kisa_sinif(&self) -> &'a str {
        kisa_ad(self.sinif)
    }

    /// Sınıfın tam adı, JSON kaçışı çözülmüş olarak: `Illuminate\Database\QueryException`
    ///
    /// Log'da `\` işareti `\\` diye yazılır; sınıf adında başka kaçış olamayacağı için
    /// yalnızca bu çözülür.
    pub fn sinif(&self) -> String {
        self.sinif.replace("\\\\", "\\")
    }

    /// Uygulama kodundaki yer: dosya ve satır. Hata tamamen vendor içindeyse None. Dosyanın
    /// ayracı her sistemde `/`'dir: `app/Models/Urun.php`. Şablonlarda satır yoktur.
    pub fn konum(&self) -> Option<(Cow<'a, str>, Option<u32>)> {
        self.konum.map(|konum| (konum.dosya_yolu(), konum.satir))
    }
}

/// Raporda gösterilen hali: `app/Services/SepetHesaplayici.php:88 (ErrorException)`
impl fmt::Display for ParmakIzi<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.konum {
            Some(konum) => write!(f, "{konum} ({})", self.kisa_sinif()),
            None => write!(f, "uygulama kodunda bulunamadı ({})", self.kisa_sinif()),
        }
    }
}

/// Logdaki bütün kayıtların parmak izleri, kayıtlarla aynı sırada; istisnası olmayan kayıtlar
/// için None.
///
/// Önce her kaydın istisnası okunur, sonra karar verilir: bir kaydın proje kökü kendi
/// yollarında bulunamazsa logun başka kayıtlarında bulunan kökler denenir.
pub fn parmak_izleri<'a>(log: &Log<'a>) -> Vec<Option<ParmakIzi<'a>>> {
    let istisnalar: Vec<Option<Istisna<'a>>> = log.kayitlar.iter().map(Istisna::kayittan).collect();
    let mut kokler: Vec<&'a str> = istisnalar
        .iter()
        .flatten()
        .filter_map(Istisna::kok)
        .collect();
    kokler.sort_unstable();
    kokler.dedup();
    istisnalar
        .iter()
        .map(|istisna| istisna.as_ref().map(|istisna| istisna.parmak_izi(&kokler)))
        .collect()
}

/// Bir kaydın istisnasından parmak izi için okunanlar. Konuma henüz karar verilmemiştir:
/// kök gerekirse logun başka kayıtlarından gelir.
struct Istisna<'a> {
    sinif: &'a str,
    /// İçten dışa: önce istisnanın fırlatıldığı yer, sonra sırayla oraya giden çağrılar.
    adaylar: Vec<Aday<'a>>,
    /// İstisna bir view hatasıysa şablonun yolu, logda yazıldığı gibi.
    sablon: Option<&'a str>,
}

impl<'a> Istisna<'a> {
    /// Kayıtta bir istisna yoksa None.
    fn kayittan(kayit: &Kayit<'a>) -> Option<Istisna<'a>> {
        match kayit.json_istisna() {
            Some(istisna) => Self::json_istisnasindan(istisna),
            None => Self::metinden(kayit),
        }
    }

    /// Metin biçimi: istisna bağlamda `"[object] (Sınıf(code: 0): ..."` diye başlar, yığın izi
    /// devam satırlarındadır.
    fn metinden(kayit: &Kayit<'a>) -> Option<Istisna<'a>> {
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
        let firlatildigi = firlatildigi_yer(baslik_sonu).map(|konum| Aday {
            konum,
            iletici: false,
        });
        // View hatasında şablon, mesajın son satırındadır; o da başlığın son satırıdır
        let sablon = sablon_yolu(baslik_sonu).filter(|_| view_hatasi_mi(sinif));

        Some(Istisna {
            sinif,
            adaylar: firlatildigi.into_iter().chain(cerceveler).collect(),
            sablon,
        })
    }

    /// JSON biçimi: istisna bir nesnedir. Sınıf `class`'tan, fırlatıldığı yer `file`'dan
    /// (`/yol/X.php:26`), çağrılar varsa `trace` dizisinden okunur. Önceki istisna
    /// (`previous`) metin biçimindeki gibi okunmaz.
    fn json_istisnasindan(istisna: &Deger<'a>) -> Option<Istisna<'a>> {
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
            })
            .collect();
        let sablon = istisna
            .alan("message")
            .and_then(Deger::ham_metin)
            .and_then(sablon_yolu)
            .filter(|_| view_hatasi_mi(sinif));

        Some(Istisna {
            sinif,
            adaylar,
            sablon,
        })
    }

    /// Kaydın kendi yollarından bulunan proje kökü: vendor klasörünü içeren ilk yoldan,
    /// "/var/www/magaza/". Hiçbir yolda vendor yoksa derlenmiş şablonların klasöründen.
    /// Derlenmiş şablon yığın izinde vendor'dan önce gelse de önce bütün yollarda vendor
    /// aranır: paylaşılan `storage` klasörü dağıtımın klasörünün dışında olabilir. Yollar
    /// zaten köke göre yazılmışsa ("vendor/laravel/...") kök yoktur, gerek de yoktur.
    fn kok(&self) -> Option<&'a str> {
        let ara = |klasorler: &[(&str, &str)]| {
            self.adaylar
                .iter()
                .find_map(|aday| klasorden_kok(aday.konum.dosya, klasorler))
        };
        ara(&VENDOR_KLASORLERI).or_else(|| ara(&DERLENMIS_SABLON_KLASORLERI))
    }

    /// Bilinen köklerden kaydın yollarından birinin başında olanı. Birden çoğu uyuyorsa en
    /// uzunu: iç içe iki projede içteki kök daha doğrudur.
    fn bilinen_kok(&self, kokler: &[&'a str]) -> Option<&'a str> {
        kokler
            .iter()
            .copied()
            .filter(|kok| {
                self.adaylar
                    .iter()
                    .any(|aday| aday.konum.dosya.starts_with(kok))
            })
            .max_by_key(|kok| kok.len())
    }

    /// Parmak izi. `kokler`: logun bütün kayıtlarında bulunan kökler; kaydın kendi kökü
    /// yoksa denenir.
    fn parmak_izi(&self, kokler: &[&'a str]) -> ParmakIzi<'a> {
        let kok = self.kok().or_else(|| self.bilinen_kok(kokler));
        // View hatasında konum şablondur; şablon uygulamada değilse (bir paketin şablonu)
        // yığın izine bakılır
        let konum = self
            .sablon
            .and_then(|sablon| sablon_konumu(sablon, kok))
            .or_else(|| uygulamadaki_ilk(&self.adaylar, kok));
        ParmakIzi {
            sinif: self.sinif,
            konum,
        }
    }
}

/// Adaylardan uygulama kodundaki ilki. Köke göre yolu "vendor/" ya da
/// "storage/framework/views/" ile başlamayan ve giriş noktası olmayan ilk aday; iletici
/// satırlar atlanır. Kök varsa onun dışındaki yollar (`strip_prefix` None verir) de atlanır.
fn uygulamadaki_ilk<'a>(adaylar: &[Aday<'a>], kok: Option<&'a str>) -> Option<Konum<'a>> {
    adaylar
        .iter()
        .filter(|aday| !aday.iletici)
        .find_map(|aday| {
            let dosya = match kok {
                Some(kok) => aday.konum.dosya.strip_prefix(kok)?,
                None => aday.konum.dosya,
            };
            let yol = yolu_coz(dosya);
            let uygulamada = !UYGULAMA_DISI.iter().any(|klasor| yol.starts_with(klasor))
                && !GIRIS_NOKTALARI.contains(&&*yol);
            uygulamada.then_some(Konum {
                dosya,
                satir: aday.konum.satir,
            })
        })
}

/// Şablonun köke göre yolu, satırsız. Kök bilinmiyorsa şablon klasöründen (`resources/views`)
/// bulunur; o da yoksa yol olduğu gibi kalır. Şablon bir vendor klasöründeyse (bir paketin
/// şablonu) uygulama kodu değildir: None.
fn sablon_konumu<'a>(sablon: &'a str, kok: Option<&'a str>) -> Option<Konum<'a>> {
    if VENDOR_KLASORLERI
        .iter()
        .any(|(_, vendor)| sablon.contains(vendor))
    {
        return None;
    }
    let dosya = match kok {
        Some(kok) => sablon.strip_prefix(kok)?,
        None => sablon_klasorunden(sablon).unwrap_or(sablon),
    };
    Some(Konum { dosya, satir: None })
}

/// İstisna bir view hatası mı? Laravel'inki `Illuminate\View\ViewException`; Ignition kendi
/// sınıfıyla sarar ama adı aynıdır.
fn view_hatasi_mi(sinif: &str) -> bool {
    kisa_ad(sinif) == "ViewException"
}

/// Ad alanı olmadan sınıf adı: `Illuminate\\Database\\QueryException` → `QueryException`
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

/// İstisna başlığından sınıf adını alır:
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
fn firlatildigi_yer(satir: &str) -> Option<Konum<'_>> {
    let (_, sonrasi) = satir.rsplit_once(" at ")?; // "/yol/Dosya.php:88)" ve belki "\"}"
    let (yer, _) = sonrasi.split_once(')')?; // "/yol/Dosya.php:88"
    let (dosya, satir_no) = yer.rsplit_once(':')?; // "/yol/Dosya.php" ve "88"
    Some(Konum {
        dosya,
        satir: Some(satir_no.parse().ok()?),
    })
}

/// Yığın izindeki bir çerçeveyi ayrıştırır:
/// `#2 /yol/SepetController.php(41): App\\...->toplam(...)` → `/yol/SepetController.php`, 41
///
/// Dosyası olmayan çerçeveler (`#5 [internal function]: ...`, `#9 {main}`) için None.
fn cerceve_ayristir(satir: &str) -> Option<Aday<'_>> {
    let (_, geri) = satir.strip_prefix('#')?.split_once(' ')?; // "/yol/X.php(41): ..."
    let (yer, cagri) = geri.split_once("): ")?; // "/yol/X.php(41" ve "App\\...->toplam(...)"
    let (dosya, satir_no) = yer.rsplit_once('(')?; // "/yol/X.php" ve "41"
    Some(Aday {
        konum: Konum {
            dosya,
            satir: Some(satir_no.parse().ok()?),
        },
        iletici: iletici_mi(cagri),
    })
}

/// Çağrı, Laravel'in Pipeline'ında bir sonraki adım mı? Middleware'deki `$next($request)`
/// yığın izinde böyle yazılır:
///
/// - PHP 8.3 ve öncesi: `Illuminate\\Pipeline\\Pipeline->Illuminate\\Pipeline\\{closure}(...)`
/// - PHP 8.4'ten beri: `Illuminate\\Pipeline\\Pipeline->{closure:Illuminate\\Pipeline\\Pipeline::carry():184}(...)`
///
/// Laravel 5'te route middleware'leri `Illuminate\Routing\Pipeline`'dan geçerdi.
fn iletici_mi(cagri: &str) -> bool {
    let Some((sinif, metot)) = cagri.split_once("->") else {
        return false;
    };
    let pipeline = matches!(
        sinif,
        r"Illuminate\\Pipeline\\Pipeline" | r"Illuminate\\Routing\\Pipeline"
    );
    // Metot adı ilk parantezden öncesidir. PHP 8.4'te closure'ın adı da parantez içerir
    // (`carry()`), ama `{closure` ondan önce gelir.
    let ad = metot.split('(').next().unwrap_or_default();
    pipeline && ad.contains("{closure")
}

/// JSON biçiminde bir yer: `/yol/Dosya.php:26`. Dosyası olmayan çerçeveler için None:
/// Monolog bunları `internal[Sinif->{closure}]:0` diye yazar.
fn json_yeri(yer: &str) -> Option<Konum<'_>> {
    let (dosya, satir_no) = yer.rsplit_once(':')?;
    if dosya.is_empty() || dosya.starts_with("internal[") {
        return None;
    }
    Some(Konum {
        dosya,
        satir: Some(satir_no.parse().ok()?),
    })
}

/// Yolda verilen klasörlerden biri geçiyorsa proje kökü: klasörden önceki kısım, sonundaki
/// ayraçla birlikte. `/var/www/magaza/vendor/laravel/...` → `/var/www/magaza/`,
/// `C:\\Users\\İrem\\Herd\\magaza\\vendor\\laravel\\...` → `C:\\Users\\İrem\\Herd\\magaza\\`
fn klasorden_kok<'a>(dosya: &'a str, klasorler: &[(&str, &str)]) -> Option<&'a str> {
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

/// Logdaki yolun gösterilecek hali: JSON kaçışları çözülür, Windows'un `\` ayracı `/` olur.
/// `database\\migrations\\x.php` → `database/migrations/x.php`. Ters bölü yoksa yol
/// kopyalanmaz.
fn yolu_coz(dosya: &str) -> Cow<'_, str> {
    if dosya.contains('\\') {
        Cow::Owned(metni_coz(dosya).replace('\\', "/"))
    } else {
        Cow::Borrowed(dosya)
    }
}

#[cfg(test)]
mod testler {
    use super::*;
    use crate::kayit::kayitlari_ayir;

    /// Metindeki ilk kaydın parmak izi. Kök gerekirse metnin başka kayıtlarından da bulunur.
    fn iz(metin: &str) -> Option<ParmakIzi<'_>> {
        parmak_izleri(&kayitlari_ayir(metin))[0]
    }

    /// Testlerde parmak izini kısaca yazmak için: (kısa sınıf, "dosya:satır")
    fn ozet(iz: Option<ParmakIzi<'_>>) -> Option<(&str, Option<String>)> {
        iz.map(|iz| (iz.kisa_sinif(), iz.konum.map(|konum| konum.to_string())))
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
#3 /var/www/magaza/public/index.php(17): Illuminate\\Foundation\\Application->handleRequest(Object(Illuminate\\Http\\Request))
#4 {main}
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
            ozet(iz(UYGULAMADA_FIRLATILAN)),
            Some((
                "ErrorException",
                Some("app/Services/SepetHesaplayici.php:88".to_string())
            ))
        );
    }

    #[test]
    fn vendorda_firlatilan_istisnada_uygulamadaki_ilk_cagri_secilir() {
        assert_eq!(
            ozet(iz(VENDORDA_FIRLATILAN)),
            Some((
                "UniqueConstraintViolationException",
                Some("app/Http/Controllers/KayitController.php:38".to_string())
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
            ozet(iz(&eski_sunucu)),
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
        assert_eq!(ozet(iz(metin)), Some(("ConnectionException", None)));
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
            ozet(iz(metin)),
            Some((
                "OdemeHatasi",
                Some("app/Services/OdemeServisi.php:80".to_string())
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
            ozet(iz(&metin)),
            Some((
                "UniqueConstraintViolationException",
                Some("app/Http/Controllers/KayitController.php:38".to_string())
            ))
        );
    }

    #[test]
    fn yigin_izi_yoksa_firlatildigi_yer_kullanilir() {
        // Stack trace kapalı bir yapılandırma: başlık tek satırda biter. Kök bulunamadığı
        // için yol olduğu gibi kalır.
        let metin = r#"[2026-10-24 10:00:00] local.ERROR: Sipariş bulunamadı {"exception":"[object] (App\\Exceptions\\SiparisBulunamadi(code: 404): Sipariş bulunamadı at /srv/magaza/app/Services/SiparisServisi.php:30)"} "#;
        assert_eq!(
            ozet(iz(metin)),
            Some((
                "SiparisBulunamadi",
                Some("/srv/magaza/app/Services/SiparisServisi.php:30".to_string())
            ))
        );
    }

    #[test]
    fn cerceveler_dosya_ve_satira_ayrilir() {
        assert_eq!(
            cerceve_ayristir("#2 /var/www/app/Kod.php(41): App\\Sinif->metot('a): b')"),
            Some(Aday {
                konum: Konum {
                    dosya: "/var/www/app/Kod.php",
                    satir: Some(41)
                },
                iletici: false
            })
        );
        assert_eq!(
            cerceve_ayristir("#1 [internal function]: App\\Sinif->metot()"),
            None
        );
        assert_eq!(cerceve_ayristir("#9 {main}"), None);
        assert_eq!(cerceve_ayristir("\"} "), None);
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
            "database/migrations/2026_10_20_083604_create_urunler_table.php:11 (QueryException)"
        );
        // Kopyalanabilir kalsın diye konum logdaki haliyle tutulur, gösterilirken çözülür
        assert_eq!(
            parmak_izi.konum.unwrap().dosya,
            r"database\\migrations\\2026_10_20_083604_create_urunler_table.php"
        );

        // Uygulama kodunda yalnızca giriş noktaları varsa konum yok: `public\\index.php`
        // giriş noktası olarak tanınmalı
        let girisler = WINDOWS_MIGRATION.replace(
            r"database\\migrations\\2026_10_20_083604_create_urunler_table.php",
            r"public\\index.php",
        );
        assert_eq!(ozet(iz(&girisler)), Some(("QueryException", None)));
    }

    #[test]
    fn proje_koku_ve_yol_cozulur() {
        let vendor = |dosya| klasorden_kok(dosya, &VENDOR_KLASORLERI);
        assert_eq!(
            vendor("/var/www/magaza/vendor/laravel/x.php"),
            Some("/var/www/magaza/")
        );
        assert_eq!(
            vendor(r"C:\\Users\\İrem\\Herd\\magaza\\vendor\\laravel\\x.php"),
            Some(r"C:\\Users\\İrem\\Herd\\magaza\\")
        );
        // Windows'ta ayraç / da olabilir; Composer bazen ikisini karıştırır
        assert_eq!(
            vendor("C:/Users/İrem/Herd/magaza/vendor/laravel/x.php"),
            Some("C:/Users/İrem/Herd/magaza/")
        );
        assert_eq!(
            vendor(r"C:\\magaza\\vendor\\composer/../laravel/x.php"),
            Some(r"C:\\magaza\\")
        );
        assert_eq!(vendor("/var/www/magaza/app/vendorlar.php"), None);
        assert_eq!(
            vendor("/srv/a/vendor/paket/storage/framework/views/x.php"),
            Some("/srv/a/")
        );
        // Derlenmiş şablonların klasörü de kökü verir
        assert_eq!(
            klasorden_kok(
                r"C:\\magaza\\storage\\framework\\views\\83012915.php",
                &DERLENMIS_SABLON_KLASORLERI
            ),
            Some(r"C:\\magaza\\")
        );

        assert_eq!(yolu_coz(r"app\\Models\\Urun.php"), "app/Models/Urun.php");
        // Başka JSON kaçışları da çözülür
        assert_eq!(yolu_coz(r"app\\\u00dcr\u00fcn.php"), "app/Ürün.php");
        assert!(matches!(yolu_coz("app/Models/Urun.php"), Cow::Borrowed(_)));
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
        assert_eq!(ozet(iz(&izsiz)), Some(("QueryException", None)));

        // Uygulamada fırlatıldıysa ve logda kök başka yerde de geçmiyorsa yol olduğu gibi
        // (ayracı / olarak) yazılır
        let uygulamada = izsiz.replace(
            "/var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php:864",
            r"C:\\Users\\İrem\\Herd\\magaza\\app\\Models\\Urun.php:12",
        );
        assert_eq!(
            ozet(iz(&uygulamada)),
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
            ozet(iz(&istisnali(
                r#"{"class":"RuntimeException","file":"satırsız","trace":"dizi değil"}"#
            ))),
            Some(("RuntimeException", None))
        );
        assert_eq!(
            json_yeri("/a/b.php:12"),
            Some(Konum {
                dosya: "/a/b.php",
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
        assert_eq!(ozet(iz(MIDDLEWARE_ARDINDA)), Some(("Error", None)));

        // PHP 8.4'ün closure adlarıyla da: `{closure:Sinif::metot():satır}`
        let php84 = MIDDLEWARE_ARDINDA.replace(
            r"YoneticiMi.php(13): Illuminate\\Pipeline\\Pipeline->Illuminate\\Pipeline\\{closure}()",
            r"YoneticiMi.php(13): Illuminate\\Pipeline\\Pipeline->{closure:Illuminate\\Pipeline\\Pipeline::carry():184}(Object(Illuminate\\Http\\Request))",
        );
        assert_ne!(php84, MIDDLEWARE_ARDINDA);
        assert_eq!(ozet(iz(&php84)), Some(("Error", None)));

        // Middleware'in başka bir çağrısı hatanın yeridir: ör. `abort_if()` yanlış kullanıldı
        let kendi_hatasi = MIDDLEWARE_ARDINDA.replace(
            r"YoneticiMi.php(13): Illuminate\\Pipeline\\Pipeline->Illuminate\\Pipeline\\{closure}()",
            "YoneticiMi.php(13): abort_if()",
        );
        assert_eq!(
            ozet(iz(&kendi_hatasi)),
            Some((
                "Error",
                Some("app/Http/Middleware/YoneticiMi.php:13".to_string())
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
        assert_eq!(ozet(iz(&baska)), Some(("RuntimeException", None)));

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
            "app/Support/Fiyat.php:14 (RuntimeException)",
        ];
        let izler = |metin: &str| -> Vec<String> {
            let log = kayitlari_ayir(metin);
            parmak_izleri(&log)
                .iter()
                .map(|iz| iz.map(|iz| iz.to_string()).unwrap_or_default())
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
        assert_eq!(ozet(iz(sablonda)), Some(("FatalError", None)));
        assert_eq!(iz(sablonda), iz(&vendorda));
    }

    #[test]
    fn sablon_uygulamada_degilse_yigin_izine_bakilir() {
        // Bir paketin şablonu: vendor klasöründe
        let paket = VIEW_HATASI.replace(
            "/var/www/magaza/resources/views/panel/kampanyalar.blade.php",
            "/var/www/magaza/vendor/firma/panel/resources/views/liste.blade.php",
        );
        assert_eq!(ozet(iz(&paket)), Some(("ViewException", None)));
        // Bozuk ek: yol yok
        let bos = VIEW_HATASI.replace(
            "(View: /var/www/magaza/resources/views/panel/kampanyalar.blade.php)",
            "(View: )",
        );
        assert_eq!(ozet(iz(&bos)), Some(("ViewException", None)));
        // View hatası olmayan bir istisnanın mesajındaki `(View: ...)` şablon sayılmaz
        let baska = VIEW_HATASI.replace(r"Illuminate\\View\\ViewException", "RuntimeException");
        assert_eq!(ozet(iz(&baska)), Some(("RuntimeException", None)));
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
            sablon_klasorunden(r"C:\\magaza\\resources\\views\\a.blade.php"),
            Some(r"resources\\views\\a.blade.php")
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
            ozet(iz(OLUMCUL)),
            Some((
                "FatalError",
                Some("/var/www/magaza/app/Http/Controllers/RaporController.php:5".to_string())
            ))
        );

        // Aynı logda vendor'dan geçen bir kayıt varsa kök oradan bilinir
        let log_metni = format!("{OLUMCUL}\n{VENDORDA_FIRLATILAN}");
        let log = kayitlari_ayir(&log_metni);
        let izler = parmak_izleri(&log);
        assert_eq!(
            izler[0].map(|iz| iz.to_string()),
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
        let log = kayitlari_ayir(&log_metni);
        let izler = parmak_izleri(&log);
        assert_eq!(izler.len(), 5);
        assert_eq!(izler[0], izler[2]);
        assert_eq!(
            izler[4].map(|iz| iz.to_string()),
            Some("/srv/baska/app/Http/Controllers/RaporController.php:5 (FatalError)".to_string())
        );
    }

    #[test]
    fn ic_ice_koklerden_en_uzunu_secilir() {
        let istisna = Istisna {
            sinif: "FatalError",
            adaylar: vec![Aday {
                konum: Konum {
                    dosya: "/srv/is/magaza/app/A.php",
                    satir: Some(3),
                },
                iletici: false,
            }],
            sablon: None,
        };
        assert_eq!(
            istisna.bilinen_kok(&["/srv/is/", "/srv/is/magaza/", "/srv/baska/"]),
            Some("/srv/is/magaza/")
        );
        assert_eq!(istisna.bilinen_kok(&["/srv/baska/"]), None);
        // Kök yolun başında olmalı: bir yedeğin içindeki aynı klasör adı kök değildir
        assert_eq!(istisna.bilinen_kok(&["/is/magaza/"]), None);
    }

    #[test]
    fn parmak_izi_okunabilir_yazilir() {
        let parmak_izi = iz(UYGULAMADA_FIRLATILAN).unwrap();
        assert_eq!(
            parmak_izi.to_string(),
            "app/Services/SepetHesaplayici.php:88 (ErrorException)"
        );
    }
}
