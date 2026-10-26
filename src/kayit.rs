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
//!
//! Kanalın biçimlendiricisi Monolog'un `JsonFormatter`'ı ise kayıt tek satırlık bir JSON
//! nesnesidir; iki biçim aynı dosyada karışık olabilir:
//!
//! ```text
//! {"message":"Mesaj","context":{},"level":400,"level_name":"ERROR","channel":"production","datetime":"2026-10-24T14:03:11.123456+00:00","extra":{}}
//! ```

use std::borrow::Cow;
use std::collections::BTreeMap;

use crate::json_okuyucu::{self, Deger};
use crate::zaman::Zaman;

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

/// Tek bir log kaydı: başlık satırından (ya da JSON satırından) okunanlar ve ona ait devam
/// satırları.
///
/// `'a`: kayıt, log metninin parçalarını kopyalamaz, onlardan ödünç alır.
/// Bu yüzden log metni yaşadığı sürece yaşayabilir, daha uzun değil. Tek istisna JSON
/// satırlarındaki kaçışlı mesajlardır (`Trait \"X\" not found`): çözülmüş halleri kayda aittir.
///
/// Alanlar gizli, dışarıdan yalnızca okunabilir (aşağıdaki metotlarla). Böylece bir kayıt
/// yalnızca bu modülde, denetlenerek oluşturulabilir.
#[derive(Debug, PartialEq)]
pub struct Kayit<'a> {
    /// "2026-10-24 14:03:11"
    zaman: &'a str,
    seviye: Seviye,
    /// Bağlam JSON'u olmadan mesajın kendisi: "Kullanıcı 381 giriş yaptı". Çoğu zaman log
    /// metninden ödünç alınır (`Cow::Borrowed`); JSON satırındaki mesaj kaçış içeriyorsa
    /// çözülmüş hali kayda aittir (`Cow::Owned`).
    mesaj: Cow<'a, str>,
    /// Başlık satırında mesajdan sonra gelen kısım: `{"ip":"203.0.113.24"}`. Yoksa ya da kayıt
    /// bir JSON satırıysa boş.
    baglam: &'a str,
    /// Başlıktan sonra gelen satırlar: stack trace, çok satırlı mesajlar.
    devam_satirlari: Vec<&'a str>,
    /// Kayıt bir JSON satırıysa bağlamdaki istisna: `{"class":"...","file":"/yol/X.php:26",...}`
    json_istisna: Option<Deger<'a>>,
}

impl<'a> Kayit<'a> {
    /// Kaydın zamanı. İlk 19 karakteri her zaman takvimde var olan bir tarih-saattir
    /// (`2026-10-24 14:03:11` ya da `2026-10-24T14:03:11`); `zaman_gecerli_mi` bunu denetler.
    pub fn zaman(&self) -> &'a str {
        self.zaman
    }

    pub fn seviye(&self) -> Seviye {
        self.seviye
    }

    /// Bağlam JSON'u olmadan mesajın kendisi.
    ///
    /// Dönen metin kayda bağlıdır (`&self` kadar yaşar), log metnine değil: JSON satırındaki
    /// kaçışlı bir mesajın çözülmüş hali yalnızca kayıtta vardır.
    pub fn mesaj(&self) -> &str {
        &self.mesaj
    }

    /// Başlık satırında mesajdan sonra gelen bağlam: `{"exception":"[object] (...`
    /// Bağlam yoksa boş metin.
    pub fn baglam(&self) -> &'a str {
        self.baglam
    }

    /// Başlıktan sonra gelen satırlar.
    ///
    /// Dönen dilim kayda aittir (`&self` kadar yaşar); içindeki metinler ise log metnine
    /// (`'a` kadar yaşar).
    pub fn devam_satirlari(&self) -> &[&'a str] {
        &self.devam_satirlari
    }

    /// Kayıt bir JSON satırıysa ve bağlamında bir istisna varsa o istisna: Monolog onu bir
    /// nesne olarak yazar (`class`, `message`, `file`, belki `trace` ve `previous`).
    pub fn json_istisna(&self) -> Option<&Deger<'a>> {
        self.json_istisna.as_ref()
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

    /// En eski ve en yeni kaydın zamanı, logda yazıldığı gibi. Hiç kayıt yoksa None.
    ///
    /// Zamanlar metin olarak değil `Zaman` olarak karşılaştırılır: metin karşılaştırması
    /// `2026-10-24 10:00:00` ile `2026-10-24T09:00:00+00:00`'ı yanlış sıralardı (boşluk, `T`'den
    /// önce gelir). Saat dilimi yok sayılır.
    pub fn zaman_araligi(&self) -> Option<(&str, &str)> {
        let zamanlar = || self.kayitlar.iter().map(|kayit| kayit.zaman);
        let en_eski = zamanlar().min_by_key(|zaman| Zaman::kayittan(zaman))?;
        let en_yeni = zamanlar().max_by_key(|zaman| Zaman::kayittan(zaman))?;
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

/// Log metnini kayıtlara ayırır. Başlık satırı ya da bir JSON kaydı yeni bir kayıt başlatır;
/// sonraki satırlar bir sonraki kayda kadar o kayda eklenir.
///
/// Bazı Windows araçları dosyanın başına bir UTF-8 BOM'u (U+FEFF) yazar. Atılmazsa satır `[`
/// ile başlamaz ve kayıt bir önceki kaydın devam satırı sayılır. BOM her satırın başında
/// atılır: birden çok dosya birleştirildiğinde (`cat a.log b.log | veylog -`) her dosyanın
/// BOM'u kendi ilk satırındadır.
pub fn kayitlari_ayir(icerik: &str) -> Log<'_> {
    let mut log = Log {
        kayitlar: Vec::new(),
        yetim_satir: 0,
    };

    for satir in icerik.lines() {
        let satir = satir.strip_prefix('\u{feff}').unwrap_or(satir);
        if let Some(kayit) = baslik_ayristir(satir).or_else(|| json_satiri_ayristir(satir)) {
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
    let (mesaj, baglam) = mesaji_ayir(mesaj_ve_baglam);

    Some(Kayit {
        zaman,
        seviye,
        mesaj: Cow::Borrowed(mesaj),
        baglam,
        devam_satirlari: Vec::new(),
        json_istisna: None,
    })
}

/// Satır Monolog'un JSON biçiminde bir kayıtsa ondan yeni bir kayıt oluşturur.
///
/// Satır bir JSON nesnesi olmalı ve en az şu alanları taşımalıdır: `datetime` (geçerli bir
/// zaman), `level_name` (bilinen bir seviye) ve `message`. Biri eksikse satır kayıt değildir;
/// bir önceki kaydın devam satırı sayılır.
fn json_satiri_ayristir(satir: &str) -> Option<Kayit<'_>> {
    // Okumaya çalışmadan önce ucuz bir ön eleme: JSON kaydı `{` ile başlar
    if !satir.starts_with('{') {
        return None;
    }
    let deger = json_okuyucu::oku(satir).ok()?;
    // Zaman kaçış içeremez (rakamlar, `-`, `:`, `T`, `+`); ham hali doğrudan denetlenir
    let zaman = deger.alan("datetime")?.ham_metin()?;
    if !zaman_gecerli_mi(zaman) {
        return None;
    }
    let seviye = Seviye::ayristir(deger.alan("level_name")?.ham_metin()?)?;
    let mesaj = ilk_satir(deger.alan("message")?.metin()?);
    let json_istisna = deger
        .alan("context")
        .and_then(|baglam| baglam.alan("exception"))
        .cloned();

    Some(Kayit {
        zaman,
        seviye,
        mesaj,
        baglam: "",
        devam_satirlari: Vec::new(),
        json_istisna,
    })
}

/// Mesajın ilk satırı, baştaki ve sondaki boşluklar atılmış olarak. Metin biçiminde çok
/// satırlı bir mesajın da yalnızca ilk satırı başlıktadır; iki biçimde aynı mesaj böylece aynı
/// kalıba düşer.
fn ilk_satir(mesaj: Cow<'_, str>) -> Cow<'_, str> {
    match mesaj {
        Cow::Borrowed(metin) => Cow::Borrowed(metin.lines().next().unwrap_or("").trim()),
        Cow::Owned(metin) => Cow::Owned(metin.lines().next().unwrap_or("").trim().to_owned()),
    }
}

/// Başlıkta seviyeden sonra gelen kısmı mesaj ve bağlam olarak ikiye ayırır.
///
/// Laravel, mesajın arkasına bağlam bilgisini JSON olarak ekler:
/// ` Kullanıcı 381 giriş yaptı {"ip":"203.0.113.24"} `
/// → `Kullanıcı 381 giriş yaptı` ve `{"ip":"203.0.113.24"}`
///
/// Bağlam ` {"` ile başlar; ondan öncesi mesajdır. Baştaki ve sondaki boşluklar atılır.
fn mesaji_ayir(metin: &str) -> (&str, &str) {
    let (mesaj, baglam) = match metin.find(" {\"") {
        Some(konum) => metin.split_at(konum),
        None => (metin, ""),
    };
    (mesaj.trim(), baglam.trim())
}

/// Kayıt zamanı geçerli mi? İki biçim kabul edilir:
///
/// - Laravel'in varsayılanı: `2026-10-24 14:03:11`
/// - Monolog'un varsayılanı olan ISO 8601: `2026-10-24T14:03:11+03:00`
///
/// Saniyeden sonra kesir (`.123456`) ve saat dilimi (`Z`, `+03:00`, `+0300`) gelebilir.
///
/// Biçimi doğru ama takvimde olmayan bir zaman (`2026-02-30`, `25:00:00`) reddedilir. Laravel
/// böyle bir zaman yazmaz; ama mesajlar satır sonu içerebildiği için, loglanan bir kullanıcı
/// girdisindeki satır bir başlık gibi görünebilir. Kabul edilseydi süre hesapları bozulurdu.
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
    // `&&` kısa devre yapar: `Zaman::kayittan` yalnızca biçim doğruysa çağrılır
    tarih_saat_gecerli && zaman_eki_gecerli_mi(ek) && Zaman::kayittan(metin).takvimde_var_mi()
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
                mesaj: Cow::Borrowed("Saat 10:00 raporu"),
                baglam: "",
                devam_satirlari: vec![],
                json_istisna: None,
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
        let log = kayitlari_ayir(&metin);

        assert_eq!(log.kayitlar.len(), 3);
        assert_eq!(log.kayitlar[1].mesaj(), "JSON biçiminde");
        assert_eq!(log.kayitlar[0].devam_satirlari.len(), 2);
        assert_eq!(
            log.kayitlar[1].devam_satirlari,
            vec!["JSON kaydından sonra gelen bir satır"]
        );
        // Metin olarak karşılaştırılsaydı en eski 10:00 olurdu: " " < "T"
        assert_eq!(
            log.zaman_araligi(),
            Some(("2026-10-24T09:00:00.5+00:00", "2026-10-24 11:00:00"))
        );
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
        ] {
            let satir = format!("[{zaman}] local.ERROR: x");
            assert!(baslik_ayristir(&satir).is_none(), "{satir}");
            let json = format!(r#"{{"message":"x","level_name":"ERROR","datetime":"{zaman}"}}"#);
            assert!(json_satiri_ayristir(&json).is_none(), "{json}");
        }
        // Artık yılın 29 Şubat'ı ve günün son saniyesi geçerli
        assert!(baslik_ayristir("[2024-02-29 23:59:59] local.ERROR: x").is_some());

        // Önceki kaydın devam satırı olur: loglanan bir girdi başlık taklidi yapamaz
        let log = kayitlari_ayir(
            "[2026-03-01 12:00:00] local.ERROR: Form gönderildi\n[2026-02-30 10:00:00] local.ERROR: sahte",
        );
        assert_eq!(log.kayitlar.len(), 1);
        assert_eq!(
            log.kayitlar[0].devam_satirlari(),
            ["[2026-02-30 10:00:00] local.ERROR: sahte"]
        );
    }

    #[test]
    fn bom_her_dosyanin_ilk_satirinda_atilir() {
        // İki BOM'lu dosya birleştirilmiş (`cat a.log b.log`): ikinci dosyanın BOM'u ortada
        let metin = "\u{feff}[2026-10-24 10:00:00] local.INFO: Birinci gün\n\
                     \u{feff}[2026-10-25 10:00:00] local.CRITICAL: İkinci gün\n\
                     \u{feff}{\"message\":\"Üçüncü\",\"level_name\":\"ERROR\",\"datetime\":\"2026-10-26T10:00:00+00:00\"}";
        let log = kayitlari_ayir(metin);
        let mesajlar: Vec<&str> = log.kayitlar.iter().map(Kayit::mesaj).collect();
        assert_eq!(mesajlar, ["Birinci gün", "İkinci gün", "Üçüncü"]);
        assert_eq!(log.kayitlar[1].seviye(), Seviye::Critical);
        assert_eq!(log.yetim_satir, 0);
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

    #[test]
    fn karisik_ornek_log_dogru_ayrilir() {
        // Önce metin biçimi (12 kayıt), 10:00'dan sonra JSON (9 kayıt); yollar Windows'ta
        let icerik = include_str!("../ornekler/karisik.log");
        let log = kayitlari_ayir(icerik);

        assert_eq!(log.kayitlar.len(), 21);
        assert_eq!(log.yetim_satir, 0);
        assert_eq!(
            log.kayitlar.len() + log.devam_satiri_sayisi() + log.yetim_satir,
            icerik.lines().count()
        );
        let json_kayitlari = icerik
            .lines()
            .filter(|satir| satir.starts_with('{'))
            .count();
        assert_eq!(json_kayitlari, 9);
        // JSON kayıtlarının devam satırı yok
        assert!(
            log.kayitlar
                .iter()
                .filter(|kayit| kayit.zaman.contains('T'))
                .all(|kayit| kayit.devam_satirlari.is_empty())
        );
        let (en_eski, en_yeni) = log.zaman_araligi().unwrap();
        assert_eq!(en_eski, "2026-10-25 09:00:12");
        assert!(en_yeni.starts_with("2026-10-25T11:58:44."), "{en_yeni}");
    }
}
