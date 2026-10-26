//! Mesajlardaki değişken kısımları (sayı, IP, e-posta...) yer tutucularla değiştirir.
//!
//! Amaç, aynı türden mesajları aynı metne dönüştürmek:
//!
//! ```text
//! Kullanıcı 381 giriş yaptı   →  Kullanıcı <SAYI> giriş yaptı
//! Kullanıcı 1024 giriş yaptı  →  Kullanıcı <SAYI> giriş yaptı
//! ```
//!
//! Drain algoritmasının ilk aşaması budur: değişkenler önceden temizlenirse kalıplar
//! çok daha doğru bulunur.

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;

/// Bir maskeleme kuralı: desene uyan her parça yer tutucuyla değiştirilir.
struct Kural {
    desen: Regex,
    /// Desenin gruplarına `${1}` diye başvurabilir: grubun yakaladığı metin yerinde kalır.
    yer_tutucu: &'static str,
}

/// Kurallar yukarıdan aşağıya sırayla uygulanır ve sıra önemlidir: önce bütün yapılar
/// (URL, e-posta, IP...) yakalanmalı, tek tek sayılar en sona kalmalı. Yoksa
/// "10.0.3.12" bir IP olarak değil, sayılara bölünmüş olarak maskelenirdi.
///
/// `LazyLock`: düzenli ifadeler ilk kullanımda bir kez derlenir, sonra hep aynısı kullanılır.
///
/// URL boşlukta, kapanan parantezde (`(see https://...)`), tırnakta ve açılı ayraçta biter:
/// RFC 3986'ya göre `"`, `<` ve `>` bir URL'de kaçışsız yazılamaz, `'` ise mesajlarda URL'yi
/// çevreler (`'https://...'`).
///
/// Dosya yolları üç kuralla bulunur. Laravel view hatalarının sonuna şablonun yolunu ekler:
/// `... (View: C:\Users\İrem\Herd\magaza\resources\views\vitrin.blade.php)`. Yol
/// maskelenmese aynı hata her şablon ve her bilgisayar için ayrı bir kalıp olurdu. Bu ekin
/// yolu `.php)` ile biter; boşluk içerse bile (`C:\Users\Ali Veli\...`) tamamı maskelenir.
/// Öteki yollar: Windows'ta bir sürücü harfiyle başlar (`C:\Users\...`, `C:/Users/...`).
/// Linux ve macOS'ta `/` ile başlar ve bir dosya uzantısıyla biter
/// (`/var/www/magaza/app/X.php`); uzantı şartı, `/api/urunler/15` gibi URL yollarını dışarıda
/// bırakır. Yol bir boşluktan, parantezden, tırnaktan ya da `=` ve `,`'den sonra başlar; o
/// karakter (`${1}`) yerinde kalır.
static KURALLAR: LazyLock<Vec<Kural>> = LazyLock::new(|| {
    vec![
        yeni_kural(r#"https?://[^\s)"'<>]+"#, "<URL>"),
        // `.*?`: tembel; ilk `.php)`'de durur, iç içe şablonların ekleri ayrı ayrı maskelenir
        yeni_kural(r"\(View: .*?\.php\)", "(View: <YOL>)"),
        yeni_kural(
            r#"(^|[\s(\['"=,])[A-Za-z]:[\\/][^\s()'"<>|?*,;:]*"#,
            "${1}<YOL>",
        ),
        yeni_kural(
            r#"(^|[\s(\['"=,])/(?:[^\s/()'"<>|?*,;:]+/)+[^\s/()'"<>|?*,;:]*\.[A-Za-z0-9]+\b"#,
            "${1}<YOL>",
        ),
        yeni_kural(r"[\w.+-]+@[\w-]+(?:\.[\w-]+)+", "<EPOSTA>"),
        yeni_kural(
            r"\b[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}\b",
            "<UUID>",
        ),
        yeni_kural(r"\b\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}:\d{2}\b", "<ZAMAN>"),
        yeni_kural(r"\b(?:\d{1,3}\.){3}\d{1,3}\b", "<IP>"),
        yeni_kural(r"\b\d+(?:\.\d+)*\b", "<SAYI>"),
    ]
});

fn yeni_kural(desen: &str, yer_tutucu: &'static str) -> Kural {
    Kural {
        // Desenler sabit; derlenemiyorsa bu bir programlama hatasıdır ve testler hemen yakalar.
        desen: Regex::new(desen).expect("geçersiz düzenli ifade"),
        yer_tutucu,
    }
}

/// Mesajdaki değişken kısımları yer tutucularla değiştirir.
///
/// Hiçbir kural eşleşmezse mesaj kopyalanmaz, olduğu gibi ödünç döner (`Cow::Borrowed`).
pub fn maskele(mesaj: &str) -> Cow<'_, str> {
    let mut sonuc = Cow::Borrowed(mesaj);
    for kural in KURALLAR.iter() {
        if let Cow::Owned(degisen) = kural.desen.replace_all(&sonuc, kural.yer_tutucu) {
            sonuc = Cow::Owned(degisen);
        }
    }
    sonuc
}

#[cfg(test)]
mod testler {
    use super::*;

    #[test]
    fn sayilar_maskelenir() {
        assert_eq!(
            maskele("Kullanıcı 381 giriş yaptı"),
            "Kullanıcı <SAYI> giriş yaptı"
        );
        assert_eq!(
            maskele("Görev tamamlandı (2.41 sn)"),
            "Görev tamamlandı (<SAYI> sn)"
        );
        assert_eq!(
            maskele("Uygulama sürümü 2.4.0 yayına alındı"),
            "Uygulama sürümü <SAYI> yayına alındı"
        );
    }

    #[test]
    fn kelimenin_icindeki_rakam_maskelenmez() {
        assert_eq!(
            maskele("oauth2 jetonu yenilendi"),
            "oauth2 jetonu yenilendi"
        );
    }

    #[test]
    fn ip_sayilara_bolunmez() {
        assert_eq!(
            maskele("Connection refused [tcp://10.0.3.12:6379]"),
            "Connection refused [tcp://<IP>:<SAYI>]"
        );
    }

    #[test]
    fn eposta_url_uuid_ve_zaman_maskelenir() {
        assert_eq!(
            maskele("Duplicate entry 'ayse@ornek.test'"),
            "Duplicate entry '<EPOSTA>'"
        );
        assert_eq!(
            maskele(
                "(see https://curl.haxx.se/libcurl/c/libcurl-errors.html) for https://api.odeme.test/v1/tahsilat"
            ),
            "(see <URL>) for <URL>"
        );
        // Tırnaktaki URL: kapanan tırnak URL'ye katılmaz
        assert_eq!(
            maskele(
                r#"The option "dsn" with value "https://anahtar@o1.ingest.test/2" is invalid."#
            ),
            r#"The option "dsn" with value "<URL>" is invalid."#
        );
        assert_eq!(
            maskele("Webhook 'https://ornek.test/kanca?x=1' yanıt vermedi"),
            "Webhook '<URL>' yanıt vermedi"
        );
        assert_eq!(maskele("<https://ornek.test/a>"), "<<URL>>");
        assert_eq!(
            maskele("Sipariş 3f2b8c1e-9d4a-4e7b-8a6f-1c2d3e4f5a6b oluşturuldu"),
            "Sipariş <UUID> oluşturuldu"
        );
        assert_eq!(
            maskele("values (Ayşe, 2026-10-24 09:14:20)"),
            "values (Ayşe, <ZAMAN>)"
        );
    }

    #[test]
    fn dosya_yollari_maskelenir() {
        // Windows: view hatasının şablonu, include ve "passed in ... on line"
        assert_eq!(
            maskele(
                r"Route [kampanya.liste] not defined. (View: C:\Users\İrem\Herd\magaza\resources\views\kampanyalar.blade.php)"
            ),
            "Route [kampanya.liste] not defined. (View: <YOL>)"
        );
        assert_eq!(
            maskele(r"include(C:\Users\İrem\Herd\magaza\public\ust.php): Failed to open stream"),
            "include(<YOL>): Failed to open stream"
        );
        assert_eq!(
            maskele(
                r"Too few arguments to function abort_if(), 1 passed in C:\Users\İrem\Herd\magaza\app\Http\Middleware\YoneticiMi.php on line 18 and at least 2 expected"
            ),
            "Too few arguments to function abort_if(), <SAYI> passed in <YOL> on line <SAYI> and at least <SAYI> expected"
        );
        assert_eq!(
            maskele("Dosya 'C:/Users/İrem/Herd/magaza/.env' okunamadı"),
            "Dosya '<YOL>' okunamadı"
        );
        // Kullanıcı adında boşluk: view ekinin yolu yine bütün olarak maskelenir
        assert_eq!(
            maskele(
                r"Undefined variable $urun (View: C:\Users\Ali Veli\Herd\magaza\resources\views\urun.blade.php)"
            ),
            "Undefined variable $urun (View: <YOL>)"
        );
        assert_eq!(
            maskele("x (View: /home/ali veli/magaza/a.blade.php) (View: /srv/b.blade.php)"),
            "x (View: <YOL>) (View: <YOL>)"
        );
        // Yol mesajın başında da olabilir; iç içe şablonlarda iki yol art arda gelir
        assert_eq!(
            maskele(r"C:\a\b.php (View: C:\a\c.blade.php) (View: C:\a\d.blade.php)"),
            "<YOL> (View: <YOL>) (View: <YOL>)"
        );
        // Satır numarası yola katılmaz
        assert_eq!(
            maskele(r"Hata C:\magaza\app\X.php:12 satırında"),
            "Hata <YOL>:<SAYI> satırında"
        );
        // Linux ve macOS: uzantıyla biten yollar
        assert_eq!(
            maskele(
                "Undefined variable $urun (View: /var/www/magaza/resources/views/vitrin.blade.php)"
            ),
            "Undefined variable $urun (View: <YOL>)"
        );
        assert_eq!(
            maskele(
                r#"The stream or file "/var/www/magaza/storage/logs/laravel.log" could not be opened"#
            ),
            r#"The stream or file "<YOL>" could not be opened"#
        );
        assert_eq!(
            maskele("Dosya /var/www/magaza/app/X.php:12 içinde"),
            "Dosya <YOL>:<SAYI> içinde"
        );
        // Uzantısız yollar (URL yolları, klasörler), göreli yollar ve kesirler maskelenmez
        assert_eq!(
            maskele("GET /api/urunler/15 yavaş"),
            "GET /api/urunler/<SAYI> yavaş"
        );
        assert_eq!(
            maskele("Klasör /var/www/magaza/storage yazılamıyor"),
            "Klasör /var/www/magaza/storage yazılamıyor"
        );
        assert_eq!(
            maskele("include(ust.php): Failed"),
            "include(ust.php): Failed"
        );
        assert_eq!(maskele("Adım 1/2 tamam"), "Adım <SAYI>/<SAYI> tamam");
        // Bir kelimenin sonundaki harf sürücü sanılmaz; URL'ler kendi kuralıyla maskelenir
        assert_eq!(maskele(r"regex:\d+ hatalı"), r"regex:\d+ hatalı");
        assert_eq!(
            maskele("Bkz. https://ornek.test/belge/kilavuz.pdf"),
            "Bkz. <URL>"
        );
    }

    #[test]
    fn degisken_yoksa_kopyalanmaz() {
        assert!(matches!(maskele("Önbellek temizlendi"), Cow::Borrowed(_)));
        assert!(matches!(
            maskele("Kullanıcı 381 giriş yaptı"),
            Cow::Owned(_)
        ));
    }
}
