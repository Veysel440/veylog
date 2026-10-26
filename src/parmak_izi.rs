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

use std::fmt;
use std::iter;

use crate::kayit::Kayit;

/// Her isteğin ya da komutun geçtiği giriş dosyaları. Yığın izinin en dibinde hep bunlar
/// bulunur; hatanın nerede oluştuğu hakkında bilgi vermezler.
const GIRIS_NOKTALARI: [&str; 2] = ["artisan", "public/index.php"];

/// Bir dosyadaki bir satır: `app/Services/SepetHesaplayici.php:88`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Konum<'a> {
    dosya: &'a str,
    satir: u32,
}

impl fmt::Display for Konum<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.dosya, self.satir)
    }
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
    /// Kaydın bağlamından ve devam satırlarından parmak izini çıkarır.
    /// Kayıtta bir istisna yoksa None.
    pub fn kayittan(kayit: &Kayit<'a>) -> Option<ParmakIzi<'a>> {
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
        let adaylar: Vec<Konum<'a>> = firlatildigi_yer(baslik_sonu)
            .into_iter()
            .chain(cerceveler)
            .collect();

        // 4. Proje kökü, "/vendor/" içeren ilk yoldan bulunur: "/var/www/magaza/". Yollar
        //    zaten köke göre yazılmışsa ("vendor/laravel/...") kök yoktur, gerek de yoktur.
        let kok = adaylar.iter().find_map(|aday| proje_koku(aday.dosya));

        // 5. Uygulama kodundaki ilk aday: köke göre yolu "vendor/" ile başlamayan ve giriş
        //    noktası olmayan. Kökün dışındaki yollar (`strip_prefix` None verir) atlanır.
        let konum = adaylar.iter().find_map(|aday| {
            let dosya = match kok {
                Some(kok) => aday.dosya.strip_prefix(kok)?,
                None => aday.dosya,
            };
            let uygulamada = !dosya.starts_with("vendor/") && !GIRIS_NOKTALARI.contains(&dosya);
            uygulamada.then_some(Konum {
                dosya,
                satir: aday.satir,
            })
        });

        Some(ParmakIzi { sinif, konum })
    }

    /// Ad alanı olmadan sınıf adı: `Illuminate\\Database\\QueryException` → `QueryException`
    pub fn kisa_sinif(&self) -> &'a str {
        self.sinif
            .rsplit_once('\\')
            .map_or(self.sinif, |(_, kisa)| kisa)
    }

    /// Sınıfın tam adı, JSON kaçışı çözülmüş olarak: `Illuminate\Database\QueryException`
    ///
    /// Log'da `\` işareti `\\` diye yazılır; sınıf adında başka kaçış olamayacağı için
    /// yalnızca bu çözülür.
    pub fn sinif(&self) -> String {
        self.sinif.replace("\\\\", "\\")
    }

    /// Uygulama kodundaki yer: dosya ve satır. Hata tamamen vendor içindeyse None.
    pub fn konum(&self) -> Option<(&'a str, u32)> {
        self.konum.map(|konum| (konum.dosya, konum.satir))
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

/// İstisna başlığından sınıf adını alır:
/// `{"exception":"[object] (ErrorException(code: 0): ...` → `ErrorException`
fn sinif_bul(satir: &str) -> Option<&str> {
    let (_, sonrasi) = satir.split_once("[object] (")?;
    let (sinif, _) = sonrasi.split_once("(code: ")?;
    // PHP sınıf adı harf, rakam, _ ve ad alanı ayracı \ içerir; anonim sınıflar
    // `class@anonymous` diye yazılır. Başka bir karakter varsa bu bir istisna başlığı değildir.
    let gecerli = !sinif.is_empty()
        && sinif
            .chars()
            .all(|harf| harf.is_alphanumeric() || matches!(harf, '_' | '\\' | '@'));
    gecerli.then_some(sinif)
}

/// Başlığın sonundaki `at /yol/Dosya.php:88)` kısmından istisnanın fırlatıldığı yeri alır.
fn firlatildigi_yer(satir: &str) -> Option<Konum<'_>> {
    let (_, sonrasi) = satir.rsplit_once(" at ")?; // "/yol/Dosya.php:88)" ve belki "\"}"
    let (yer, _) = sonrasi.split_once(')')?; // "/yol/Dosya.php:88"
    let (dosya, satir_no) = yer.rsplit_once(':')?; // "/yol/Dosya.php" ve "88"
    Some(Konum {
        dosya,
        satir: satir_no.parse().ok()?,
    })
}

/// Yığın izindeki bir çerçeveyi ayrıştırır:
/// `#2 /yol/SepetController.php(41): App\\...->toplam(...)` → `/yol/SepetController.php`, 41
///
/// Dosyası olmayan çerçeveler (`#5 [internal function]: ...`, `#9 {main}`) için None.
fn cerceve_ayristir(satir: &str) -> Option<Konum<'_>> {
    let (_, geri) = satir.strip_prefix('#')?.split_once(' ')?; // "/yol/X.php(41): ..."
    let (yer, _) = geri.split_once("): ")?; // "/yol/X.php(41"
    let (dosya, satir_no) = yer.rsplit_once('(')?; // "/yol/X.php" ve "41"
    Some(Konum {
        dosya,
        satir: satir_no.parse().ok()?,
    })
}

/// Proje kökünü vendor klasöründen bulur, sonundaki "/" ile birlikte:
/// `/var/www/magaza/vendor/laravel/...` → `/var/www/magaza/`
fn proje_koku(dosya: &str) -> Option<&str> {
    let konum = dosya.find("/vendor/")?;
    Some(&dosya[..=konum])
}

#[cfg(test)]
mod testler {
    use super::*;
    use crate::kayit::kayitlari_ayir;

    /// Metindeki ilk kaydın parmak izi.
    fn iz(metin: &str) -> Option<ParmakIzi<'_>> {
        let log = kayitlari_ayir(metin);
        ParmakIzi::kayittan(&log.kayitlar[0])
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
            Some(Konum {
                dosya: "/var/www/app/Kod.php",
                satir: 41
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
            Some(("app/Http/Controllers/KayitController.php", 38))
        );
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
