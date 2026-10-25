//! Laravel log kayıtlarını tanır: bir satır kayıt başlığı mı, başlıksa seviyesi ne?
//!
//! Bir Laravel kaydı şöyle bir başlık satırıyla başlar:
//!
//! ```text
//! [2026-10-24 14:03:11] production.ERROR: Mesaj
//! ```
//!
//! Stack trace gibi devam satırları başlık sayılmaz.

/// Laravel log seviyeleri. Sıralama önem sırasıdır: Debug en düşük, Emergency en yüksek.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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
    /// Log dosyasındaki yazılışından seviyeye çevirir: "ERROR" → Seviye::Error
    fn ayristir(metin: &str) -> Option<Seviye> {
        match metin {
            "DEBUG" => Some(Seviye::Debug),
            "INFO" => Some(Seviye::Info),
            "NOTICE" => Some(Seviye::Notice),
            "WARNING" => Some(Seviye::Warning),
            "ERROR" => Some(Seviye::Error),
            "CRITICAL" => Some(Seviye::Critical),
            "ALERT" => Some(Seviye::Alert),
            "EMERGENCY" => Some(Seviye::Emergency),
            _ => None,
        }
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

/// Satır bir kayıt başlığıysa seviyesini döndürür, değilse None.
/// Örnek: "[2026-10-24 14:03:11] production.ERROR: ..." → Some(Seviye::Error)
pub fn seviye_bul(satir: &str) -> Option<Seviye> {
    if !baslik_mi(satir) {
        return None;
    }
    let (_, geri) = satir.split_once("] ")?; // "production.ERROR: ..."
    let (kanal_seviye, _) = geri.split_once(':')?; // "production.ERROR"
    let (_, seviye) = kanal_seviye.rsplit_once('.')?; // "ERROR"
    Seviye::ayristir(seviye)
}

/// Satır bir log kaydının başlangıcı mı? Örnek: "[2026-10-24 14:03:11] production.ERROR: ..."
fn baslik_mi(satir: &str) -> bool {
    let Some(geri) = satir.strip_prefix('[') else {
        return false;
    };
    let Some((zaman, _)) = geri.split_once("] ") else {
        return false;
    };
    zaman_gecerli_mi(zaman)
}

/// "2026-10-24 14:03:11" biçiminde mi? (19 karakter: rakamlar, -, boşluk ve :)
fn zaman_gecerli_mi(metin: &str) -> bool {
    let baytlar = metin.as_bytes();
    if baytlar.len() != 19 {
        return false;
    }
    baytlar.iter().enumerate().all(|(sira, &bayt)| match sira {
        4 | 7 => bayt == b'-',
        10 => bayt == b' ',
        13 | 16 => bayt == b':',
        _ => bayt.is_ascii_digit(),
    })
}

#[cfg(test)]
mod testler {
    use super::*;

    #[test]
    fn gercek_baslik_taninir() {
        assert!(baslik_mi("[2026-10-24 14:03:11] production.ERROR: x"));
    }

    #[test]
    fn koseli_parantezli_diger_satirlar_baslik_degildir() {
        assert!(!baslik_mi("[stacktrace]"));
        assert!(!baslik_mi("[previous exception] [object] (x)"));
        assert!(!baslik_mi("#0 /var/www/app/Kod.php(41)"));
        assert!(!baslik_mi(""));
    }

    #[test]
    fn bozuk_zaman_reddedilir() {
        assert!(!baslik_mi("[2026-10-24] local.INFO: x"));
        assert!(!baslik_mi("[2026/10/24 14:03:11] local.INFO: x"));
        assert!(!baslik_mi("[2026-10-24 14:03:1x] local.INFO: x"));
    }

    #[test]
    fn seviye_bulunur() {
        let satir = "[2026-10-24 14:03:11] production.ERROR: Saat 10:00 raporu";
        assert_eq!(seviye_bul(satir), Some(Seviye::Error));
        assert_eq!(
            seviye_bul("[2026-10-24 09:00:01] local.INFO: x"),
            Some(Seviye::Info)
        );
    }

    #[test]
    fn baslik_olmayan_ya_da_bilinmeyen_seviye_none_doner() {
        assert_eq!(seviye_bul("[stacktrace]"), None);
        assert_eq!(seviye_bul("[2026-10-24 14:03:11] local.HATA: x"), None);
    }

    #[test]
    fn seviyeler_onem_sirasina_gore_karsilastirilir() {
        assert!(Seviye::Error > Seviye::Warning);
        assert!(Seviye::Debug < Seviye::Info);
    }
}
