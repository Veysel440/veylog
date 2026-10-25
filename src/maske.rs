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
    yer_tutucu: &'static str,
}

/// Kurallar yukarıdan aşağıya sırayla uygulanır ve sıra önemlidir: önce bütün yapılar
/// (URL, e-posta, IP...) yakalanmalı, tek tek sayılar en sona kalmalı. Yoksa
/// "10.0.3.12" bir IP olarak değil, sayılara bölünmüş olarak maskelenirdi.
///
/// `LazyLock`: düzenli ifadeler ilk kullanımda bir kez derlenir, sonra hep aynısı kullanılır.
static KURALLAR: LazyLock<Vec<Kural>> = LazyLock::new(|| {
    vec![
        yeni_kural(r"https?://[^\s)]+", "<URL>"),
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
    fn degisken_yoksa_kopyalanmaz() {
        assert!(matches!(maskele("Önbellek temizlendi"), Cow::Borrowed(_)));
        assert!(matches!(
            maskele("Kullanıcı 381 giriş yaptı"),
            Cow::Owned(_)
        ));
    }
}
