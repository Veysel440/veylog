//! PHP'nin istisna metni: `Throwable::__toString()`.
//!
//! `Log::error($e)` istisnayı bağlama koymaz; metne çevirip mesaj olarak yazar (`report($e)`
//! ise bağlama koyar, onu `parmak_izi` modülü okur). PHP'nin metni şöyledir (yollar
//! kısaltıldı):
//!
//! ```text
//! [2026-10-27 14:03:11] production.ERROR: RuntimeException: Ödeme alınamadı in /var/www/magaza/app/Services/OdemeServisi.php:64
//! Stack trace:
//! #0 /var/www/magaza/app/Http/Controllers/OdemeController.php(31): App\Services\OdemeServisi->tahsilEt()
//! #1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/Controller.php(54): App\Http\Controllers\OdemeController->odeme()
//! #2 {main}
//! ```
//!
//! İstisna bir başkasından doğduysa (`previous`) PHP önce en içtekini yazar, sonra her birini
//! boş bir satır ve `Next` ile. Logu yazdıran istisna en sondadır:
//!
//! ```text
//! PDOException: SQLSTATE[HY000] [2002] Connection refused in /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connectors/Connector.php:66
//! Stack trace:
//! #0 ...
//! #9 {main}
//!
//! Next Illuminate\Database\QueryException: SQLSTATE[HY000] [2002] Connection refused (Connection: mysql, SQL: select * from `urunler`) in /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php:825
//! Stack trace:
//! #0 ...
//! #12 {main}
//! ```
//!
//! Metin biçimindeki bağlamın tersine burada hiçbir şey JSON kaçışlı değildir: sınıf adları
//! `App\Services\X`, Windows yolları `C:\Users\...` diye tek ters bölüyle yazılır.
//!
//! Mesajı satır sonu içeren bir istisnada başlık birkaç satır sürer; fırlatıldığı yer
//! (` in dosya:satır`) `Stack trace:` satırından hemen önceki satırın sonundadır. Mesaj boşsa
//! başlık yalnızca sınıf ve yerdir: `RuntimeException in /yol/X.php:12`.
//!
//! İstisnanın önüne bir açıklama da yazılabilir: `Log::error('Ödeme başarısız: ' . $e)`.
//! Sınıf adları için PHP'nin bir kısıtı yoktur, `Ödeme` de bir sınıf adı olabilir; bu yüzden
//! yalnızca istisna adı gibi görünen bir ad sınıf sayılır: ad alanı içeren
//! (`App\Exceptions\OdemeHatasi`) ya da `Exception` veya `Error` ile biten (`RuntimeException`,
//! `TypeError`). Satır böyle bir adla başlıyorsa açıklama yoktur; başlamıyorsa satırda bir
//! boşluktan sonra gelen ilk böyle ad sınıftır, öncesi açıklamadır.

/// Bir istisna metninden okunanlar: logu yazdıran, en dıştaki istisna.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IstisnaMetni<'a> {
    /// İstisnadan önce yazılan açıklama: `"Ödeme başarısız: "`. Yoksa boş.
    pub aciklama: &'a str,
    /// Sınıfın tam adı: `App\Exceptions\OdemeHatasi`
    pub sinif: &'a str,
    /// Mesajın ilk satırı; mesaj boşsa boş.
    pub mesaj: &'a str,
    /// Başlığın son satırı, fırlatıldığı yer olmadan: mesajın son satırı. View hatalarında
    /// şablonun yolu buradadır.
    pub baslik_sonu: &'a str,
    /// Fırlatıldığı yer.
    pub dosya: &'a str,
    pub satir: u32,
    /// Yığın izinin satırları: `#0 /yol/X.php(31): App\...`, en son `#N {main}`
    pub cerceveler: Vec<&'a str>,
}

impl IstisnaMetni<'_> {
    /// Kaydın mesajı: açıklama ve istisnanın mesajı. `report($e)` ile loglanan bir istisnanın
    /// mesajı da budur; aynı hata iki yolla loglansa da aynı kalıba düşer.
    pub fn kayit_mesaji(&self) -> String {
        format!("{}{}", self.aciklama, self.mesaj)
            .trim()
            .to_string()
    }
}

/// Bir istisnanın başlığı: açıklama, sınıf, mesaj ve yer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Baslik<'a> {
    aciklama: &'a str,
    sinif: &'a str,
    mesaj: &'a str,
    baslik_sonu: &'a str,
    dosya: &'a str,
    satir: u32,
}

/// Kaydın satırlarında (ilki başlıktaki mesaj) PHP'nin istisna metni varsa en dıştaki istisna.
///
/// `Stack trace:` satırı bulunmadan hiçbir şey ayrıştırılmaz: yığın izi olmayan kayıtlar için
/// yalnızca satırlara bakılır.
pub fn ayristir<'a>(satirlar: impl IntoIterator<Item = &'a str>) -> Option<IstisnaMetni<'a>> {
    let mut satirlar = satirlar.into_iter();
    let ilk = satirlar.next()?.trim();
    // Açıklama yalnızca ilk istisnanın önünde olabilir; dıştaki istisna bir `Next` olsa da
    // kaydın açıklamasıdır
    let mut aciklama = "";
    // Okunan son istisna; bir sonrakinin başlığı bozuksa bu kalır
    let mut son: Option<(Baslik<'a>, Vec<&'a str>)> = None;
    // Okunmakta olan istisna: başlığının ilk ve son satırı, başlığı tek satır mı, ilk istisna mı
    let (mut baslik_ilk, mut baslik_son, mut tek_satir, mut ilk_istisna) = (ilk, ilk, true, true);
    // Yığın izindeyse okunmakta olan istisnanın başlığı ve çerçeveleri
    let mut yigin: Option<(Baslik<'a>, Vec<&'a str>)> = None;
    for satir in satirlar {
        match &mut yigin {
            None if satir.trim_end() == "Stack trace:" => {
                let Some(baslik) = baslik_ayristir(baslik_ilk, baslik_son, tek_satir, ilk_istisna)
                else {
                    break;
                };
                if ilk_istisna {
                    aciklama = baslik.aciklama;
                }
                yigin = Some((baslik, Vec::new()));
            }
            None => {
                baslik_son = satir;
                tek_satir = false;
            }
            Some((_, cerceveler)) => {
                if satir.starts_with('#') {
                    cerceveler.push(satir);
                } else if satir.trim().is_empty() {
                    // `Next`'ten önceki boş satır
                } else if let Some(sonraki) = satir.strip_prefix("Next ") {
                    son = yigin.take();
                    (baslik_ilk, baslik_son, tek_satir, ilk_istisna) =
                        (sonraki, sonraki, true, false);
                } else {
                    // İstisna metni bitti (ör. ardından bağlam geliyor)
                    break;
                }
            }
        }
    }
    // Yığın izi okunan son istisna; yığın izine varamayan bir `Next` sayılmaz
    let (dis, cerceveler) = yigin.or(son)?;
    Some(IstisnaMetni {
        aciklama,
        sinif: dis.sinif,
        mesaj: dis.mesaj,
        baslik_sonu: dis.baslik_sonu,
        dosya: dis.dosya,
        satir: dis.satir,
        cerceveler,
    })
}

/// Bir istisnanın başlığını ayrıştırır. `ilk`: başlığın ilk satırı (`Next` olmadan), `son`:
/// `Stack trace:`'ten önceki satır, `tek_satir`: ikisi aynı satır mı. `aciklama_olabilir`:
/// yalnızca ilk istisnanın önünde açıklama olabilir.
fn baslik_ayristir<'a>(
    ilk: &'a str,
    son: &'a str,
    tek_satir: bool,
    aciklama_olabilir: bool,
) -> Option<Baslik<'a>> {
    // Yer son satırın sonundadır. Mesaj " in " içerebilir, dosya yolu genellikle içermez:
    // sondan aranır.
    let (baslik_sonu, yer) = son.trim_end().rsplit_once(" in ")?;
    let (dosya, satir) = yer.rsplit_once(':')?;
    if dosya.is_empty() {
        return None;
    }
    let satir = satir.parse().ok()?;
    // Başlık tek satırsa sınıf ve mesaj yerden öncesidir; değilse ilk satırın tamamı
    let bas = if tek_satir { baslik_sonu } else { ilk };
    let (aciklama, sinif, mesaj) = sinif_bul(bas, aciklama_olabilir)?;
    // Tek satırlık bir başlıkta mesaj boş olabilir (`X in /yol:1`); çok satırlıda olamaz
    if mesaj.is_none() && !tek_satir {
        return None;
    }
    Some(Baslik {
        aciklama,
        sinif,
        mesaj: mesaj.unwrap_or(""),
        baslik_sonu,
        dosya,
        satir,
    })
}

/// Başlığın ilk satırında sınıfı bulur: (açıklama, sınıf, mesaj). Mesaj, sınıftan sonra `:`
/// geliyorsa ondan sonrasıdır; sınıf satırın sonundaysa None (mesaj boş).
///
/// Satır istisna adı gibi görünen bir adla başlıyorsa sınıf odur. Değilse ve açıklama olabilirse
/// bir boşluktan sonra gelen ilk böyle ad aranır. `Next`'ten sonra gelen başlıkta ad ne olursa
/// olsun sınıftır: `Next` yeterince belirgindir.
fn sinif_bul(satir: &str, aciklama_olabilir: bool) -> Option<(&str, &str, Option<&str>)> {
    let dene = |bas: usize, istisna_adi_sart: bool| {
        let (sinif, geri) = sinif_basta(&satir[bas..])?;
        if istisna_adi_sart && !istisna_adi_mi(sinif) {
            return None;
        }
        Some((&satir[..bas], sinif, sinif_sonrasi(geri)?))
    };
    if !aciklama_olabilir {
        return dene(0, false);
    }
    dene(0, true).or_else(|| {
        satir
            .match_indices(' ')
            .find_map(|(bosluk, _)| dene(bosluk + 1, true))
    })
}

/// Sınıftan sonrası: `": mesaj"` → `Some(Some("mesaj"))`, `""` → `Some(None)`. Başka bir şey
/// geliyorsa (`"bir şey"`, `"::metot"`) bu bir sınıf değildir: None.
fn sinif_sonrasi(geri: &str) -> Option<Option<&str>> {
    if geri.is_empty() {
        return Some(None);
    }
    let mesaj = geri.strip_prefix(':')?;
    // `:` ardından boşluk ya da satır sonu: `::` statik bir çağrıdır
    (mesaj.is_empty() || mesaj.starts_with(' ')).then(|| Some(mesaj.trim_start()))
}

/// Metnin başındaki sınıf adı ve sonrası. PHP sınıf adları harf, rakam ve `_` içerir (ASCII
/// dışındaki her karakter de harf sayılır), rakamla başlamaz; ad alanları `\` ile ayrılır.
/// Anonim sınıflar `class@anonymous` diye başlar.
fn sinif_basta(metin: &str) -> Option<(&str, &str)> {
    let son = metin
        .find(|harf: char| !(harf.is_alphanumeric() || matches!(harf, '_' | '\\' | '@')))
        .unwrap_or(metin.len());
    let (sinif, geri) = metin.split_at(son);
    let gecerli = sinif
        .chars()
        .next()
        .is_some_and(|ilk| ilk.is_alphabetic() || ilk == '_')
        && !sinif.ends_with('\\')
        && !sinif.contains(r"\\");
    gecerli.then_some((sinif, geri))
}

/// Ad bir istisna adı gibi mi görünüyor: ad alanı içeriyor ya da `Exception` veya `Error`
/// ile bitiyor (büyük küçük harf fark etmez: `mysqli_sql_exception`)?
fn istisna_adi_mi(sinif: &str) -> bool {
    let biter = |ek: &str| {
        let baytlar = sinif.as_bytes();
        baytlar.len() >= ek.len()
            && baytlar[baytlar.len() - ek.len()..].eq_ignore_ascii_case(ek.as_bytes())
    };
    sinif.contains('\\') || biter("exception") || biter("error")
}

#[cfg(test)]
mod testler {
    use super::*;

    fn ayir(metin: &str) -> Option<IstisnaMetni<'_>> {
        ayristir(metin.lines())
    }

    const BASIT: &str = r"RuntimeException: Ödeme alınamadı in /var/www/magaza/app/Services/OdemeServisi.php:64
Stack trace:
#0 /var/www/magaza/app/Http/Controllers/OdemeController.php(31): App\Services\OdemeServisi->tahsilEt()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/Controller.php(54): App\Http\Controllers\OdemeController->odeme()
#2 {main}  ";

    #[test]
    fn basit_istisna() {
        let metin = ayir(BASIT).unwrap();
        assert_eq!(
            metin,
            IstisnaMetni {
                aciklama: "",
                sinif: "RuntimeException",
                mesaj: "Ödeme alınamadı",
                baslik_sonu: "RuntimeException: Ödeme alınamadı",
                dosya: "/var/www/magaza/app/Services/OdemeServisi.php",
                satir: 64,
                cerceveler: BASIT.lines().skip(2).collect(),
            }
        );
        assert_eq!(metin.kayit_mesaji(), "Ödeme alınamadı");
        // Mesaj " in " içerebilir: yer satırın sonundaki " in "dir
        let in_li =
            "LogicException: Ürün bulunamadı in kategori 5 in /a/b.php:7\nStack trace:\n#0 {main}";
        let metin = ayir(in_li).unwrap();
        assert_eq!(
            (metin.mesaj, metin.dosya, metin.satir),
            ("Ürün bulunamadı in kategori 5", "/a/b.php", 7)
        );
    }

    #[test]
    fn onceki_istisnalar_icten_disa() {
        let metin = r#"PDOException: SQLSTATE[HY000] [2002] Connection refused in /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connectors/Connector.php:66
Stack trace:
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connectors/Connector.php(66): PDO->__construct('mysql:host=127....', 'root', Object(SensitiveParameterValue), Array)
#1 {main}

Next Illuminate\Database\QueryException: SQLSTATE[HY000] [2002] Connection refused (Connection: mysql, SQL: select * from `urunler`) in /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php:825
Stack trace:
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php(779): Illuminate\Database\Connection->runQueryCallback('select * from `...', Array, Object(Closure))
#1 /var/www/magaza/app/Http/Controllers/UrunController.php(20): Illuminate\Database\Eloquent\Builder->get()
#2 {main} {"kullanici":5} "#;
        let metin = ayir(metin).unwrap();
        assert_eq!(metin.sinif, r"Illuminate\Database\QueryException");
        assert_eq!(
            metin.mesaj,
            "SQLSTATE[HY000] [2002] Connection refused (Connection: mysql, SQL: select * from `urunler`)"
        );
        assert_eq!(
            (metin.dosya, metin.satir),
            (
                "/var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php",
                825
            )
        );
        assert_eq!(metin.cerceveler.len(), 3);
        assert!(metin.cerceveler[1].contains("UrunController.php(20)"));
    }

    #[test]
    fn cok_satirli_mesaj() {
        let metin =
            "Illuminate\\Http\\Client\\RequestException: HTTP request returned status code 500:
{\"hata\":\"bakımdayız\"}
 in /var/www/magaza/vendor/laravel/framework/src/Illuminate/Http/Client/Response.php:283
Stack trace:
#0 /var/www/magaza/app/Services/KargoServisi.php(40): Illuminate\\Http\\Client\\Response->throw()
#1 {main}";
        let metin = ayir(metin).unwrap();
        assert_eq!(metin.sinif, r"Illuminate\Http\Client\RequestException");
        assert_eq!(metin.mesaj, "HTTP request returned status code 500:");
        assert_eq!(metin.baslik_sonu, "");
        assert_eq!(metin.satir, 283);
        // Mesajı yeni satırla başlayan istisna: ilk satır yalnızca sınıf ve `:`
        let yeni_satirla = "RuntimeException: \nayrıntı in /a/b.php:3\nStack trace:\n#0 {main}";
        let metin = ayir(yeni_satirla).unwrap();
        assert_eq!((metin.sinif, metin.mesaj), ("RuntimeException", ""));
    }

    #[test]
    fn bos_mesaj_ve_aciklama() {
        let bos = "Symfony\\Component\\HttpKernel\\Exception\\NotFoundHttpException in /a/b.php:7\nStack trace:\n#0 {main}";
        let metin = ayir(bos).unwrap();
        assert_eq!(
            (metin.sinif, metin.mesaj, metin.satir),
            (
                r"Symfony\Component\HttpKernel\Exception\NotFoundHttpException",
                "",
                7
            )
        );

        let aciklamali = format!("Ödeme başarısız: {BASIT}");
        let metin = ayir(&aciklamali).unwrap();
        assert_eq!(metin.aciklama, "Ödeme başarısız: ");
        assert_eq!(metin.sinif, "RuntimeException");
        assert_eq!(metin.kayit_mesaji(), "Ödeme başarısız: Ödeme alınamadı");
        // Açıklama bir sınıf adı gibi başlasa da istisna adı gibi görünmüyorsa açıklamadır
        let hata = format!("Hata: {BASIT}");
        let metin = ayir(&hata).unwrap();
        assert_eq!(
            (metin.aciklama, metin.sinif),
            ("Hata: ", "RuntimeException")
        );
        // Mesajı boş istisna, açıklamayla
        let bulunamadi = format!("Bulunamadı: {bos}");
        let metin = ayir(&bulunamadi).unwrap();
        assert_eq!(
            (metin.aciklama, metin.mesaj, metin.kayit_mesaji().as_str()),
            ("Bulunamadı: ", "", "Bulunamadı:")
        );
        // Dıştaki istisna bir `Next` olsa da açıklama ilk başlıktadır
        let zincir = format!(
            "Ödeme başarısız: {BASIT}\n\nNext App\\Exceptions\\OdemeHatasi: Tahsilat yapılamadı in /var/www/magaza/app/Services/OdemeServisi.php:70\nStack trace:\n#0 {{main}}"
        );
        let metin = ayir(&zincir).unwrap();
        assert_eq!(
            (metin.aciklama, metin.sinif, metin.mesaj),
            (
                "Ödeme başarısız: ",
                r"App\Exceptions\OdemeHatasi",
                "Tahsilat yapılamadı"
            )
        );
        assert_eq!(metin.kayit_mesaji(), "Ödeme başarısız: Tahsilat yapılamadı");
    }

    #[test]
    fn istisna_adlari() {
        for ad in [
            "RuntimeException",
            "TypeError",
            "Error",
            "mysqli_sql_exception",
            r"App\Exceptions\OdemeHatasi",
        ] {
            assert!(istisna_adi_mi(ad), "{ad}");
        }
        for ad in ["Hata", "Ödeme", "SoapFault", "ErrorHandler"] {
            assert!(!istisna_adi_mi(ad), "{ad}");
        }
        assert_eq!(sinif_basta("Ödeme: x"), Some(("Ödeme", ": x")));
        assert_eq!(sinif_basta("9Sinif: x"), None);
        assert_eq!(sinif_basta(r"App\: x"), None);
        assert_eq!(sinif_basta(r"App\\Sinif: x"), None);
        assert_eq!(sinif_sonrasi("::metot"), None);
        assert_eq!(sinif_sonrasi(" bir şey"), None);
        assert_eq!(sinif_sonrasi(":x"), None);
        assert_eq!(sinif_sonrasi(":"), Some(Some("")));
    }

    #[test]
    fn istisna_metni_olmayanlar() {
        for metin in [
            // Yığın izi yok
            "RuntimeException: Ödeme alınamadı in /a/b.php:64",
            // Yer yok ya da bozuk
            "RuntimeException: Ödeme alınamadı\nStack trace:\n#0 {main}",
            "RuntimeException: Ödeme alınamadı in /a/b.php\nStack trace:\n#0 {main}",
            "RuntimeException: Ödeme alınamadı in :12\nStack trace:\n#0 {main}",
            "RuntimeException: Ödeme alınamadı in /a/b.php:x\nStack trace:\n#0 {main}",
            // Sınıf yok: açıklamada istisna adı yok
            "Ödeme alınamadı in /a/b.php:64\nStack trace:\n#0 {main}",
            "Ödeme: alınamadı in /a/b.php:64\nStack trace:\n#0 {main}",
            // Çok satırlı başlığın ilk satırı yalnızca sınıf olamaz
            "RuntimeException\nx in /a/b.php:64\nStack trace:\n#0 {main}",
            // Statik çağrı sınıf değildir
            "App\\Odeme::al bir şey in /a/b.php:64\nStack trace:\n#0 {main}",
            "",
        ] {
            assert_eq!(ayir(metin), None, "{metin}");
        }
    }

    #[test]
    fn bozuk_next_onceki_istisnayi_birakmaz() {
        // `Next`ten sonraki başlık bozuksa okunan son istisna kalır
        let metin = format!("{BASIT}\n\nNext bozuk bir satır\nStack trace:\n#0 {{main}}");
        assert_eq!(ayir(&metin).unwrap().sinif, "RuntimeException");
        // Yığın izine varamayan `Next` de sayılmaz
        let metin = format!("{BASIT}\n\nNext LogicException: yarım kaldı in /a/b.php:1");
        assert_eq!(ayir(&metin).unwrap().sinif, "RuntimeException");
        // Yığın izinden sonra başka bir şey gelirse metin orada biter
        let metin = format!("{BASIT}\n[stacktrace]\n#0 /a/b.php(1): x()");
        assert_eq!(ayir(&metin).unwrap().cerceveler.len(), 3);
    }
}
