//! Laravel log metnini kayıtlara ayırır.
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

use std::collections::BTreeMap;

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

/// Tek bir log kaydı: başlık satırından okunanlar ve ona ait devam satırları.
///
/// `'a`: kayıt, log metninin parçalarını kopyalamaz, onlardan ödünç alır.
/// Bu yüzden log metni yaşadığı sürece yaşayabilir, daha uzun değil.
///
/// Alanlar gizli, dışarıdan yalnızca okunabilir (aşağıdaki metotlarla). Böylece bir kayıt
/// yalnızca bu modülde, denetlenerek oluşturulabilir.
#[derive(Debug, PartialEq)]
pub struct Kayit<'a> {
    /// "2026-10-24 14:03:11"
    zaman: &'a str,
    seviye: Seviye,
    /// Bağlam JSON'u olmadan mesajın kendisi: "Kullanıcı 381 giriş yaptı"
    mesaj: &'a str,
    /// Başlıktan sonra gelen satırlar: stack trace, çok satırlı mesajlar.
    devam_satirlari: Vec<&'a str>,
}

impl<'a> Kayit<'a> {
    /// Kaydın zamanı. İlk 19 karakteri her zaman geçerli bir tarih-saattir
    /// (`2026-10-24 14:03:11` ya da `2026-10-24T14:03:11`); `zaman_gecerli_mi` bunu denetler.
    pub fn zaman(&self) -> &'a str {
        self.zaman
    }

    pub fn seviye(&self) -> Seviye {
        self.seviye
    }

    /// Bağlam JSON'u olmadan mesajın kendisi.
    pub fn mesaj(&self) -> &'a str {
        self.mesaj
    }
}

/// Bir log metninin kayıtlara ayrılmış hali.
#[derive(Debug)]
pub struct Log<'a> {
    pub kayitlar: Vec<Kayit<'a>>,
    /// İlk kayıttan önce gelen, hiçbir kayda ait olmayan satırlar.
    /// Dosya bir kaydın ortasından başlıyorsa olur (ör. log dosyası döndürüldüğünde).
    pub yetim_satir: usize,
}

impl Log<'_> {
    /// Tüm kayıtlardaki devam satırlarının toplamı.
    pub fn devam_satiri_sayisi(&self) -> usize {
        self.kayitlar
            .iter()
            .map(|kayit| kayit.devam_satirlari.len())
            .sum()
    }

    /// En eski ve en yeni kaydın zamanı. Hiç kayıt yoksa None.
    ///
    /// Zamanlar sabit genişlikte ve büyükten küçüğe (yıl, ay, gün, saat...) yazıldığı için
    /// metin olarak karşılaştırmak, zaman olarak karşılaştırmakla aynı sonucu verir. Bunun
    /// için dosyadaki bütün kayıtların aynı biçimi ve aynı saat dilimini kullanması gerekir.
    pub fn zaman_araligi(&self) -> Option<(&str, &str)> {
        let en_eski = self.kayitlar.iter().map(|kayit| kayit.zaman).min()?;
        let en_yeni = self.kayitlar.iter().map(|kayit| kayit.zaman).max()?;
        Some((en_eski, en_yeni))
    }

    /// Her seviyeden kaç kayıt var? BTreeMap anahtarları sıralı tutar.
    pub fn seviye_sayilari(&self) -> BTreeMap<Seviye, usize> {
        let mut sayaclar = BTreeMap::new();
        for kayit in &self.kayitlar {
            *sayaclar.entry(kayit.seviye).or_insert(0) += 1;
        }
        sayaclar
    }
}

/// Log metnini kayıtlara ayırır. Başlık satırı yeni bir kayıt başlatır; sonraki satırlar
/// bir sonraki başlığa kadar o kayda eklenir.
pub fn kayitlari_ayir(icerik: &str) -> Log<'_> {
    let mut log = Log {
        kayitlar: Vec::new(),
        yetim_satir: 0,
    };

    for satir in icerik.lines() {
        if let Some(kayit) = baslik_ayristir(satir) {
            log.kayitlar.push(kayit);
        } else if let Some(son_kayit) = log.kayitlar.last_mut() {
            son_kayit.devam_satirlari.push(satir);
        } else {
            log.yetim_satir += 1;
        }
    }

    log
}

/// Satır bir kayıt başlığıysa ondan yeni bir kayıt oluşturur (henüz devam satırı yok).
fn baslik_ayristir(satir: &str) -> Option<Kayit<'_>> {
    let geri = satir.strip_prefix('[')?; // "2026-10-24 14:03:11] production.ERROR: ..."
    let (zaman, kalan) = geri.split_once("] ")?; // "2026-10-24 14:03:11" ve "production.ERROR: ..."
    if !zaman_gecerli_mi(zaman) {
        return None;
    }
    let (kanal_seviye, mesaj_ve_baglam) = kalan.split_once(':')?; // "production.ERROR" ve " Mesaj {...}"
    let (_, seviye_metni) = kanal_seviye.rsplit_once('.')?; // "ERROR"
    let seviye = Seviye::ayristir(seviye_metni)?;

    Some(Kayit {
        zaman,
        seviye,
        mesaj: mesaji_ayir(mesaj_ve_baglam),
        devam_satirlari: Vec::new(),
    })
}

/// Başlıkta seviyeden sonra gelen kısımdan mesajı ayırır.
///
/// Laravel, mesajın arkasına bağlam bilgisini JSON olarak ekler:
/// ` Kullanıcı 381 giriş yaptı {"ip":"203.0.113.24"} ` → `Kullanıcı 381 giriş yaptı`
/// Bağlam ` {"` ile başlar; ondan öncesi mesajdır. Baştaki ve sondaki boşluklar atılır.
fn mesaji_ayir(metin: &str) -> &str {
    let mesaj = match metin.find(" {\"") {
        Some(konum) => &metin[..konum],
        None => metin,
    };
    mesaj.trim()
}

/// Kayıt zamanı geçerli mi? İki biçim kabul edilir:
///
/// - Laravel'in varsayılanı: `2026-10-24 14:03:11`
/// - Monolog'un varsayılanı olan ISO 8601: `2026-10-24T14:03:11+03:00`
///
/// Saniyeden sonra kesir (`.123456`) ve saat dilimi (`Z`, `+03:00`, `+0300`) gelebilir.
fn zaman_gecerli_mi(metin: &str) -> bool {
    let baytlar = metin.as_bytes();
    if baytlar.len() < 19 {
        return false;
    }
    let (tarih_saat, ek) = baytlar.split_at(19);
    let tarih_saat_gecerli = tarih_saat
        .iter()
        .enumerate()
        .all(|(sira, &bayt)| match sira {
            4 | 7 => bayt == b'-',
            10 => bayt == b' ' || bayt == b'T',
            13 | 16 => bayt == b':',
            _ => bayt.is_ascii_digit(),
        });
    tarih_saat_gecerli && zaman_eki_gecerli_mi(ek)
}

/// Saniyeden sonra gelen kısım: isteğe bağlı kesir, ardından isteğe bağlı saat dilimi.
fn zaman_eki_gecerli_mi(mut ek: &[u8]) -> bool {
    if let Some(kesir) = ek.strip_prefix(b".") {
        let rakam_sayisi = kesir
            .iter()
            .take_while(|bayt| bayt.is_ascii_digit())
            .count();
        if rakam_sayisi == 0 {
            return false;
        }
        ek = &kesir[rakam_sayisi..];
    }
    match ek {
        [] | [b'Z'] => true,
        [b'+' | b'-', s1, s2, b':', d1, d2] | [b'+' | b'-', s1, s2, d1, d2] => {
            [s1, s2, d1, d2].iter().all(|bayt| bayt.is_ascii_digit())
        }
        _ => false,
    }
}

#[cfg(test)]
mod testler {
    use super::*;

    #[test]
    fn baslik_alanlarina_ayrilir() {
        let kayit = baslik_ayristir("[2026-10-24 14:03:11] production.ERROR: Saat 10:00 raporu");
        assert_eq!(
            kayit,
            Some(Kayit {
                zaman: "2026-10-24 14:03:11",
                seviye: Seviye::Error,
                mesaj: "Saat 10:00 raporu",
                devam_satirlari: vec![],
            })
        );
    }

    #[test]
    fn mesaj_baglamdan_ve_bosluklardan_ayrilir() {
        let kayit = baslik_ayristir(
            r#"[2026-10-24 09:02:13] production.INFO: Kullanıcı 381 giriş yaptı {"ip":"203.0.113.24"} "#,
        );
        assert_eq!(kayit.map(|k| k.mesaj), Some("Kullanıcı 381 giriş yaptı"));

        // Mesajın içindeki ":" işaretleri bölmeyi bozmamalı; sondaki boşluklar atılmalı
        let kayit = baslik_ayristir(
            "[2026-10-24 08:59:58] production.INFO: Zamanlanmış görev başladı: kurlar:guncelle  ",
        );
        assert_eq!(
            kayit.map(|k| k.mesaj),
            Some("Zamanlanmış görev başladı: kurlar:guncelle")
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
    fn bozuk_zaman_ekleri_reddedilir() {
        for satir in [
            "[2026-10-24 14:03:11.] local.INFO: x",
            "[2026-10-24 14:03:11+3] local.INFO: x",
            "[2026-10-24 14:03:11abc] local.INFO: x",
            "[2026-10-24X14:03:11] local.INFO: x",
        ] {
            assert!(baslik_ayristir(satir).is_none(), "kabul edildi: {satir}");
        }
    }

    #[test]
    fn bilinmeyen_seviye_baslik_sayilmaz() {
        assert!(baslik_ayristir("[2026-10-24 14:03:11] local.HATA: x").is_none());
    }

    #[test]
    fn seviyeler_onem_sirasina_gore_karsilastirilir() {
        assert!(Seviye::Error > Seviye::Warning);
        assert!(Seviye::Debug < Seviye::Info);
    }

    #[test]
    fn devam_satirlari_onceki_kayda_eklenir() {
        let metin = [
            "[2026-10-24 10:00:00] local.ERROR: Bir hata",
            "[stacktrace]",
            "#0 /var/www/app/Kod.php(41)",
            "[2026-10-24 10:00:05] local.INFO: Tamam",
        ]
        .join("\n");
        let log = kayitlari_ayir(&metin);

        assert_eq!(log.kayitlar.len(), 2);
        assert_eq!(
            log.kayitlar[0].devam_satirlari,
            vec!["[stacktrace]", "#0 /var/www/app/Kod.php(41)"]
        );
        assert!(log.kayitlar[1].devam_satirlari.is_empty());
        assert_eq!(log.yetim_satir, 0);
    }

    #[test]
    fn ilk_kayittan_onceki_satirlar_yetim_kalir() {
        let metin = ["#15 {main}", "\"}", "[2026-10-24 10:00:00] local.INFO: x"].join("\n");
        let log = kayitlari_ayir(&metin);

        assert_eq!(log.yetim_satir, 2);
        assert_eq!(log.kayitlar.len(), 1);
    }

    #[test]
    fn zaman_araligi_en_eski_ve_en_yeni_kayittir() {
        // Kayıtlar dosyada her zaman sıralı olmayabilir (aynı anda yazan birden fazla işlem)
        let metin = [
            "[2026-10-24 10:00:05] local.INFO: b",
            "[2026-10-24 09:59:59] local.INFO: a",
            "[2026-10-24 10:00:01] local.INFO: c",
        ]
        .join("\n");
        let log = kayitlari_ayir(&metin);

        assert_eq!(
            log.zaman_araligi(),
            Some(("2026-10-24 09:59:59", "2026-10-24 10:00:05"))
        );
        assert_eq!(kayitlari_ayir("").zaman_araligi(), None);
    }

    #[test]
    fn ornek_log_dogru_ayrilir() {
        let icerik = include_str!("../ornekler/laravel.log");
        let log = kayitlari_ayir(icerik);

        assert_eq!(log.kayitlar.len(), 59);
        assert_eq!(log.yetim_satir, 3);
        // Her satır tam olarak bir yere gider: başlık, devam ya da yetim.
        assert_eq!(
            log.kayitlar.len() + log.devam_satiri_sayisi() + log.yetim_satir,
            icerik.lines().count()
        );
        assert_eq!(
            log.zaman_araligi(),
            Some(("2026-10-24 08:59:58", "2026-10-24 18:31:07"))
        );

        let sayilar = log.seviye_sayilari();
        assert_eq!(sayilar[&Seviye::Error], 16);
        assert_eq!(sayilar[&Seviye::Critical], 2);
        assert_eq!(sayilar.get(&Seviye::Emergency), None);
    }
}
