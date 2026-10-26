//! JSON okuyucu: Monolog'un JSON biçiminde yazdığı log satırlarını okur.
//!
//! Laravel'de bir kanalın biçimlendiricisi Monolog'un `JsonFormatter`'ı yapılırsa her kayıt tek
//! satırlık bir JSON nesnesi olur:
//!
//! ```text
//! {"message":"Kullanıcı 381 giriş yaptı","context":{"ip":"203.0.113.24"},"level":200,"level_name":"INFO","channel":"local","datetime":"2026-10-24T09:02:13.123456+00:00","extra":{}}
//! ```
//!
//! Okuyucu RFC 8259'u izler ve kütüphanesiz yazıldı: JSON yazıcı gibi küçük, kuralları kısa.
//! Okunan değerler satırı kopyalamaz, ondan ödünç alır. Metinler tırnakların arasındaki ham
//! halleriyle tutulur; kaçışları (`\"`, `\\`, `\n`, `\u00e7`...) yalnızca gerektiğinde
//! `metni_coz` çözer. Doğruluğu testlerde serde_json ile karşılaştırılarak denetlenir.

use std::borrow::Cow;
use std::fmt;
use std::str::Chars;

/// İç içe en fazla bu kadar dizi ve nesne. Okuyucu özyinelemelidir: her düzey yığında (stack)
/// yer kaplar. Sınır olmasa `[[[[...` diye on binlerce düzey açan tek bir log satırı yığını
/// taşırıp programı çökertebilirdi. serde_json'un sınırı da budur: 127 düzeyi okur, 128'i
/// reddeder.
const EN_FAZLA_DERINLIK: usize = 127;

/// Okunan bir JSON değeri. Metinler ve sayılar satırdan ödünç alınır (`'a`).
#[derive(Debug, Clone, PartialEq)]
pub enum Deger<'a> {
    /// `null`
    Yok,
    Mantiksal(bool),
    /// Sayının satırdaki yazılışı: `400`, `-1.5e3`. Değeri gerekmediği için çevrilmez.
    Sayi(&'a str),
    /// Tırnakların arasındaki ham metin, kaçışları çözülmemiş: `Trait \"X\" not found`
    Metin(&'a str),
    Dizi(Vec<Deger<'a>>),
    /// Alanlar satırdaki sırasıyla. Anahtarlar da ham haldedir.
    Nesne(Vec<(&'a str, Deger<'a>)>),
}

impl<'a> Deger<'a> {
    /// Nesnenin `ad` anahtarlı alanı. Değer nesne değilse ya da alan yoksa None; aynı anahtar
    /// birden çok kez geçiyorsa ilki. Ham anahtarla karşılaştırılır, bu yüzden `ad` kaçış
    /// içermemelidir (Monolog'un anahtarları içermez).
    pub fn alan(&self, ad: &str) -> Option<&Deger<'a>> {
        match self {
            Deger::Nesne(alanlar) => alanlar
                .iter()
                .find(|(anahtar, _)| *anahtar == ad)
                .map(|(_, deger)| deger),
            _ => None,
        }
    }

    /// Değer bir metinse ham hali: tırnakların arası, kaçışları çözülmemiş.
    pub fn ham_metin(&self) -> Option<&'a str> {
        match self {
            Deger::Metin(ham) => Some(ham),
            _ => None,
        }
    }

    /// Değer bir metinse kaçışları çözülmüş hali.
    pub fn metin(&self) -> Option<Cow<'a, str>> {
        self.ham_metin().map(metni_coz)
    }

    /// Değer bir diziyse ögeleri.
    pub fn ogeler(&self) -> Option<&[Deger<'a>]> {
        match self {
            Deger::Dizi(ogeler) => Some(ogeler),
            _ => None,
        }
    }
}

/// Okuma hatası: nerede ve neden.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OkumaHatasi {
    /// Hatanın bulunduğu bayt, 0'dan başlayarak.
    pub konum: usize,
    pub neden: &'static str,
}

impl fmt::Display for OkumaHatasi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}. baytta {}", self.konum, self.neden)
    }
}

impl std::error::Error for OkumaHatasi {}

/// Metnin tamamını tek bir JSON değeri olarak okur. Değerden önce ve sonra yalnızca boşluk
/// (boşluk, sekme, satır sonu) olabilir.
pub fn oku(metin: &str) -> Result<Deger<'_>, OkumaHatasi> {
    let mut okuyucu = Okuyucu {
        kaynak: metin,
        konum: 0,
    };
    let deger = okuyucu.deger(0)?;
    okuyucu.bosluk_atla();
    if okuyucu.konum < metin.len() {
        return Err(okuyucu.hata("değerden sonra fazladan karakter var"));
    }
    Ok(deger)
}

/// JSON metnindeki kaçışları çözer: `Trait \"X\" bulunamadı` → `Trait "X" bulunamadı`
///
/// Kaçış yoksa metin kopyalanmaz, ödünç döner (`Cow::Borrowed`). `\u` kaçışları UTF-16 birimi
/// yazar: U+FFFF'in üstündeki karakterler (emoji gibi) iki birimle, bir vekil (surrogate)
/// çiftiyle yazılır: `\ud83d\ude80` → 🚀. Eşi olmayan bir vekil, karşılığı olmadığı için
/// U+FFFD (�) olur.
///
/// Okuyucunun denetlemediği metinler de verilebilir: geçersiz bir kaçış hata vermez, olduğu
/// gibi yazılır.
///
/// Eşi olmayan bir vekil RFC 8259'un söz dizimine uyar; okuyucu onu reddetmez, burada � olur.
/// serde_json ise reddeder. Monolog geçerli UTF-8'den yazdığı için böyle bir kaçış üretmez.
pub fn metni_coz(ham: &str) -> Cow<'_, str> {
    if !ham.contains('\\') {
        return Cow::Borrowed(ham);
    }
    let mut sonuc = String::with_capacity(ham.len());
    let mut harfler = ham.chars();
    while let Some(harf) = harfler.next() {
        if harf != '\\' {
            sonuc.push(harf);
            continue;
        }
        match harfler.next() {
            Some('"') => sonuc.push('"'),
            Some('\\') => sonuc.push('\\'),
            Some('/') => sonuc.push('/'),
            Some('b') => sonuc.push('\u{8}'),
            Some('f') => sonuc.push('\u{c}'),
            Some('n') => sonuc.push('\n'),
            Some('r') => sonuc.push('\r'),
            Some('t') => sonuc.push('\t'),
            Some('u') => match onaltilik(&mut harfler) {
                Some(yuksek @ 0xD800..=0xDBFF) => {
                    // Yüksek vekilin ardından düşük vekil gelirse ikisi tek bir karakterdir
                    let mut ileri = harfler.clone();
                    let dusuk = if ileri.next() == Some('\\') && ileri.next() == Some('u') {
                        onaltilik(&mut ileri)
                    } else {
                        None
                    };
                    match dusuk {
                        Some(dusuk @ 0xDC00..=0xDFFF) => {
                            harfler = ileri;
                            let kod = 0x10000 + ((yuksek - 0xD800) << 10) + (dusuk - 0xDC00);
                            sonuc.push(char::from_u32(kod).unwrap_or(char::REPLACEMENT_CHARACTER));
                        }
                        _ => sonuc.push(char::REPLACEMENT_CHARACTER),
                    }
                }
                // Tek başına bir düşük vekil de geçerli bir karakter değildir: from_u32 None verir
                Some(birim) => {
                    sonuc.push(char::from_u32(birim).unwrap_or(char::REPLACEMENT_CHARACTER));
                }
                None => sonuc.push_str("\\u"),
            },
            Some(diger) => {
                sonuc.push('\\');
                sonuc.push(diger);
            }
            None => sonuc.push('\\'),
        }
    }
    Cow::Owned(sonuc)
}

/// `\u`'dan sonraki dört onaltılık rakam. Rakamlar eksik ya da geçersizse None; o zaman
/// yineleyici ilerlemez.
fn onaltilik(harfler: &mut Chars<'_>) -> Option<u32> {
    let mut ileri = harfler.clone();
    let mut deger = 0;
    for _ in 0..4 {
        deger = deger * 16 + ileri.next()?.to_digit(16)?;
    }
    *harfler = ileri;
    Some(deger)
}

/// Okurken metindeki yer. Her değer için bir metot vardır; bir değer okunurken içindeki
/// değerler için aynı metotlar çağrılır (özyinelemeli iniş, recursive descent).
struct Okuyucu<'a> {
    kaynak: &'a str,
    /// Sıradaki baytın yeri.
    konum: usize,
}

impl<'a> Okuyucu<'a> {
    fn siradaki(&self) -> Option<u8> {
        self.kaynak.as_bytes().get(self.konum).copied()
    }

    fn hata(&self, neden: &'static str) -> OkumaHatasi {
        OkumaHatasi {
            konum: self.konum,
            neden,
        }
    }

    /// RFC 8259'un boşlukları: boşluk, sekme, LF ve CR.
    fn bosluk_atla(&mut self) {
        while matches!(self.siradaki(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.konum += 1;
        }
    }

    /// `derinlik`: değerin içinde bulunduğu dizi ve nesne sayısı.
    fn deger(&mut self, derinlik: usize) -> Result<Deger<'a>, OkumaHatasi> {
        self.bosluk_atla();
        match self.siradaki() {
            Some(b'{') => self.nesne(derinlik + 1),
            Some(b'[') => self.dizi(derinlik + 1),
            Some(b'"') => self.metin().map(Deger::Metin),
            Some(b'-' | b'0'..=b'9') => self.sayi(),
            Some(b't') => self.sabit("true", Deger::Mantiksal(true)),
            Some(b'f') => self.sabit("false", Deger::Mantiksal(false)),
            Some(b'n') => self.sabit("null", Deger::Yok),
            Some(_) => Err(self.hata("beklenmeyen karakter var")),
            None => Err(self.hata("bir değer bekleniyordu")),
        }
    }

    fn nesne(&mut self, derinlik: usize) -> Result<Deger<'a>, OkumaHatasi> {
        if derinlik > EN_FAZLA_DERINLIK {
            return Err(self.hata("iç içe çok fazla dizi ya da nesne var"));
        }
        self.konum += 1; // {
        let mut alanlar = Vec::new();
        self.bosluk_atla();
        if self.siradaki() == Some(b'}') {
            self.konum += 1;
            return Ok(Deger::Nesne(alanlar));
        }
        loop {
            self.bosluk_atla();
            if self.siradaki() != Some(b'"') {
                return Err(self.hata("tırnaklı bir anahtar bekleniyordu"));
            }
            let anahtar = self.metin()?;
            self.bosluk_atla();
            if self.siradaki() != Some(b':') {
                return Err(self.hata("`:` bekleniyordu"));
            }
            self.konum += 1;
            alanlar.push((anahtar, self.deger(derinlik)?));
            self.bosluk_atla();
            match self.siradaki() {
                Some(b',') => self.konum += 1,
                Some(b'}') => {
                    self.konum += 1;
                    return Ok(Deger::Nesne(alanlar));
                }
                _ => return Err(self.hata("`,` ya da `}` bekleniyordu")),
            }
        }
    }

    fn dizi(&mut self, derinlik: usize) -> Result<Deger<'a>, OkumaHatasi> {
        if derinlik > EN_FAZLA_DERINLIK {
            return Err(self.hata("iç içe çok fazla dizi ya da nesne var"));
        }
        self.konum += 1; // [
        let mut ogeler = Vec::new();
        self.bosluk_atla();
        if self.siradaki() == Some(b']') {
            self.konum += 1;
            return Ok(Deger::Dizi(ogeler));
        }
        loop {
            ogeler.push(self.deger(derinlik)?);
            self.bosluk_atla();
            match self.siradaki() {
                Some(b',') => self.konum += 1,
                Some(b']') => {
                    self.konum += 1;
                    return Ok(Deger::Dizi(ogeler));
                }
                _ => return Err(self.hata("`,` ya da `]` bekleniyordu")),
            }
        }
    }

    /// Tırnaklı bir metin. Kaçışlar denetlenir ama çözülmez; tırnakların arası ödünç döner.
    ///
    /// Baytlarla ilerlemek güvenlidir: UTF-8'de çok baytlı bir harfin hiçbir baytı ASCII
    /// değildir, bu yüzden `"` ya da `\` sanılamaz ve dilimin sınırları harf sınırına düşer.
    fn metin(&mut self) -> Result<&'a str, OkumaHatasi> {
        self.konum += 1; // "
        let bas = self.konum;
        loop {
            match self.siradaki() {
                None => return Err(self.hata("metin kapanmadan satır bitti")),
                Some(b'"') => {
                    let ham = &self.kaynak[bas..self.konum];
                    self.konum += 1;
                    return Ok(ham);
                }
                Some(b'\\') => {
                    self.konum += 1;
                    match self.siradaki() {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                            self.konum += 1;
                        }
                        Some(b'u') => {
                            let rakamlar =
                                self.kaynak.as_bytes().get(self.konum + 1..self.konum + 5);
                            if !rakamlar
                                .is_some_and(|rakamlar| rakamlar.iter().all(u8::is_ascii_hexdigit))
                            {
                                return Err(
                                    self.hata("`\\u`'dan sonra dört onaltılık rakam bekleniyordu")
                                );
                            }
                            self.konum += 5;
                        }
                        _ => return Err(self.hata("geçersiz kaçış var")),
                    }
                }
                // RFC 8259: U+0000–U+001F arası metinde kaçışsız yazılamaz
                Some(0x00..=0x1F) => {
                    return Err(self.hata("metinde kaçışsız bir kontrol karakteri var"));
                }
                Some(_) => self.konum += 1,
            }
        }
    }

    /// Sayı: isteğe bağlı eksi, tam kısım (başında gereksiz 0 olmadan), isteğe bağlı kesir ve
    /// üs. `01`, `1.`, `.5`, `+1` geçersizdir.
    fn sayi(&mut self) -> Result<Deger<'a>, OkumaHatasi> {
        let bas = self.konum;
        if self.siradaki() == Some(b'-') {
            self.konum += 1;
        }
        match self.siradaki() {
            Some(b'0') => self.konum += 1,
            Some(b'1'..=b'9') => self.rakamlari_atla(),
            _ => return Err(self.hata("rakam bekleniyordu")),
        }
        if self.siradaki() == Some(b'.') {
            self.konum += 1;
            if !self.siradaki().is_some_and(|bayt| bayt.is_ascii_digit()) {
                return Err(self.hata("noktadan sonra rakam bekleniyordu"));
            }
            self.rakamlari_atla();
        }
        if matches!(self.siradaki(), Some(b'e' | b'E')) {
            self.konum += 1;
            if matches!(self.siradaki(), Some(b'+' | b'-')) {
                self.konum += 1;
            }
            if !self.siradaki().is_some_and(|bayt| bayt.is_ascii_digit()) {
                return Err(self.hata("üsten sonra rakam bekleniyordu"));
            }
            self.rakamlari_atla();
        }
        Ok(Deger::Sayi(&self.kaynak[bas..self.konum]))
    }

    fn rakamlari_atla(&mut self) {
        while self.siradaki().is_some_and(|bayt| bayt.is_ascii_digit()) {
            self.konum += 1;
        }
    }

    /// `true`, `false` ya da `null`.
    fn sabit(&mut self, kelime: &str, deger: Deger<'a>) -> Result<Deger<'a>, OkumaHatasi> {
        if self.kaynak[self.konum..].starts_with(kelime) {
            self.konum += kelime.len();
            Ok(deger)
        } else {
            Err(self.hata("beklenmeyen karakter var"))
        }
    }
}

#[cfg(test)]
mod testler {
    use super::*;
    use serde_json::Value;

    /// Okunan değerin serde_json karşılığı: iki okuyucunun sonuçlarını karşılaştırmak için.
    /// Sayılar serde_json'a kendi yazılışlarıyla okutulur.
    fn serdeye(deger: &Deger<'_>) -> Value {
        match deger {
            Deger::Yok => Value::Null,
            Deger::Mantiksal(mantiksal) => Value::Bool(*mantiksal),
            Deger::Sayi(yazilis) => serde_json::from_str(yazilis).unwrap(),
            Deger::Metin(ham) => Value::String(metni_coz(ham).into_owned()),
            Deger::Dizi(ogeler) => Value::Array(ogeler.iter().map(serdeye).collect()),
            Deger::Nesne(alanlar) => Value::Object(
                alanlar
                    .iter()
                    .map(|(anahtar, deger)| (metni_coz(anahtar).into_owned(), serdeye(deger)))
                    .collect(),
            ),
        }
    }

    #[test]
    fn monolog_satiri_okunur() {
        let satir = r#"{"message":"Trait \"App\\Traits\\Etiketli\" not found","context":{"userId":2210,"exception":{"class":"Symfony\\Component\\ErrorHandler\\Error\\FatalError","code":0,"file":"C:\\Users\\İrem\\Herd\\magaza\\app\\Models\\Urun.php:12"}},"level":500,"level_name":"CRITICAL","channel":"local","datetime":"2026-10-24T09:02:13.123456+00:00","extra":{}}"#;
        let deger = oku(satir).unwrap();
        assert_eq!(
            deger.alan("message").and_then(Deger::metin).as_deref(),
            Some(r#"Trait "App\Traits\Etiketli" not found"#)
        );
        assert_eq!(deger.alan("level"), Some(&Deger::Sayi("500")));
        assert_eq!(
            deger.alan("datetime").and_then(Deger::ham_metin),
            Some("2026-10-24T09:02:13.123456+00:00")
        );
        let istisna = deger
            .alan("context")
            .and_then(|baglam| baglam.alan("exception"));
        // Ham metin kaçışlarıyla birlikte: Windows yolunun ayracı `\\`
        assert_eq!(
            istisna
                .and_then(|istisna| istisna.alan("file"))
                .and_then(Deger::ham_metin),
            Some(r"C:\\Users\\İrem\\Herd\\magaza\\app\\Models\\Urun.php:12")
        );
        assert_eq!(deger.alan("extra"), Some(&Deger::Nesne(vec![])));
        assert_eq!(deger.alan("yok"), None);
        // Nesne olmayan bir değerin alanı yoktur
        assert_eq!(Deger::Sayi("1").alan("message"), None);
    }

    #[test]
    fn kacislar_cozulur() {
        for (ham, beklenen) in [
            (
                r#"tırnak \" ters bölü \\ bölü \/"#,
                "tırnak \" ters bölü \\ bölü /",
            ),
            (r"\b\f\n\r\t", "\u{8}\u{c}\n\r\t"),
            (r"\u00e7\u011F\u0130", "çğİ"),
            // U+FFFF'in üstü bir vekil çiftiyle yazılır
            (r"roket \ud83d\ude80", "roket 🚀"),
            // Eşi olmayan vekiller: yüksek tek başına, düşük tek başına, ters sırada
            (r"a\ud83db", "a\u{FFFD}b"),
            (r"a\ude80b", "a\u{FFFD}b"),
            (r"\ude80\ud83d", "\u{FFFD}\u{FFFD}"),
            // Yüksek vekilden sonra düşük olmayan bir \u: ikisi ayrı ayrı çözülür
            (r"\ud83d\u0041", "\u{FFFD}A"),
            // Okuyucunun denetlemediği metinde geçersiz kaçışlar olduğu gibi kalır
            (r"\x \u12 sonda\", "\\x \\u12 sonda\\"),
        ] {
            assert_eq!(metni_coz(ham), beklenen, "{ham}");
        }
        // Kaçış yoksa kopyalanmaz
        assert!(matches!(metni_coz("düz metin"), Cow::Borrowed("düz metin")));
    }

    #[test]
    fn gecersiz_json_reddedilir() {
        for (metin, konum, neden) in [
            ("", 0, "bir değer bekleniyordu"),
            ("   ", 3, "bir değer bekleniyordu"),
            ("{", 1, "tırnaklı bir anahtar bekleniyordu"),
            (r#"{"a":}"#, 5, "beklenmeyen karakter var"),
            (r#"{"a" 1}"#, 5, "`:` bekleniyordu"),
            (r#"{"a":1,}"#, 7, "tırnaklı bir anahtar bekleniyordu"),
            (r#"{"a":1 "b":2}"#, 7, "`,` ya da `}` bekleniyordu"),
            ("{'a':1}", 1, "tırnaklı bir anahtar bekleniyordu"),
            ("[1,]", 3, "beklenmeyen karakter var"),
            ("[1 2]", 3, "`,` ya da `]` bekleniyordu"),
            ("[", 1, "bir değer bekleniyordu"),
            ("01", 1, "değerden sonra fazladan karakter var"),
            ("1.", 2, "noktadan sonra rakam bekleniyordu"),
            (".5", 0, "beklenmeyen karakter var"),
            ("-", 1, "rakam bekleniyordu"),
            ("+1", 0, "beklenmeyen karakter var"),
            ("1e", 2, "üsten sonra rakam bekleniyordu"),
            ("1e+", 3, "üsten sonra rakam bekleniyordu"),
            ("tru", 0, "beklenmeyen karakter var"),
            ("nul", 0, "beklenmeyen karakter var"),
            ("NaN", 0, "beklenmeyen karakter var"),
            (r#""\x""#, 2, "geçersiz kaçış var"),
            (
                r#""\u12""#,
                2,
                "`\\u`'dan sonra dört onaltılık rakam bekleniyordu",
            ),
            (
                r#""\u12g4""#,
                2,
                "`\\u`'dan sonra dört onaltılık rakam bekleniyordu",
            ),
            (
                r#""\u123g""#,
                2,
                "`\\u`'dan sonra dört onaltılık rakam bekleniyordu",
            ),
            ("\"a\nb\"", 2, "metinde kaçışsız bir kontrol karakteri var"),
            (
                "\"a\u{1b}b\"",
                2,
                "metinde kaçışsız bir kontrol karakteri var",
            ),
            // ı iki bayttır: metin 11 bayt
            (r#""kapanmadı"#, 11, "metin kapanmadan satır bitti"),
            (r#"{"a":1}x"#, 7, "değerden sonra fazladan karakter var"),
            ("truex", 4, "değerden sonra fazladan karakter var"),
        ] {
            assert_eq!(oku(metin), Err(OkumaHatasi { konum, neden }), "{metin:?}");
        }
        assert_eq!(
            oku("[1,]").unwrap_err().to_string(),
            "3. baytta beklenmeyen karakter var"
        );
    }

    #[test]
    fn bosluklar_ve_sinir_degerler() {
        assert_eq!(
            oku(" \t\r\n{ \"a\" : [ 1 , -0.5e+3 , true , false , null ] }\r\n "),
            Ok(Deger::Nesne(vec![(
                "a",
                Deger::Dizi(vec![
                    Deger::Sayi("1"),
                    Deger::Sayi("-0.5e+3"),
                    Deger::Mantiksal(true),
                    Deger::Mantiksal(false),
                    Deger::Yok,
                ])
            )]))
        );
        assert_eq!(oku("[]"), Ok(Deger::Dizi(vec![])));
        assert_eq!(oku("\"\""), Ok(Deger::Metin("")));
        assert_eq!(oku("0"), Ok(Deger::Sayi("0")));
        assert_eq!(oku("1e-5"), Ok(Deger::Sayi("1e-5")));
        assert_eq!(oku("-2.5E-3"), Ok(Deger::Sayi("-2.5E-3")));
        // DEL ve C1 karakterleri JSON'da kaçışsız yazılabilir
        assert_eq!(oku("\"\u{7f}\u{9b}\""), Ok(Deger::Metin("\u{7f}\u{9b}")));
    }

    #[test]
    fn ayni_anahtardan_ilki_alinir() {
        let deger = oku(r#"{"a":1,"a":2}"#).unwrap();
        assert_eq!(deger.alan("a"), Some(&Deger::Sayi("1")));
    }

    #[test]
    fn derinlik_sinirli() {
        let ic_ice = |derinlik: usize| format!("{}{}", "[".repeat(derinlik), "]".repeat(derinlik));
        assert!(oku(&ic_ice(EN_FAZLA_DERINLIK)).is_ok());
        assert_eq!(
            oku(&ic_ice(EN_FAZLA_DERINLIK + 1)),
            Err(OkumaHatasi {
                konum: EN_FAZLA_DERINLIK,
                neden: "iç içe çok fazla dizi ya da nesne var"
            })
        );
        // Sınır olmasa bu satır yığını taşırıp programı çökertirdi
        assert!(oku(&"[".repeat(1_000_000)).is_err());
        // Nesneler de sayılır
        let ic_ice_nesne =
            |derinlik: usize| format!("{}1{}", r#"{"a":"#.repeat(derinlik), "}".repeat(derinlik));
        assert!(oku(&ic_ice_nesne(EN_FAZLA_DERINLIK)).is_ok());
        assert!(oku(&ic_ice_nesne(EN_FAZLA_DERINLIK + 1)).is_err());
    }

    #[test]
    fn serde_json_ile_ayni_sonuc() {
        // Elle seçilmiş metinler: ikisi de kabul etmeli ya da ikisi de reddetmeli; kabul
        // ettiklerinde aynı değeri okumalı
        for metin in [
            r#"{"message":"a","context":{"b":[1,2.5,-3e2,{"c":null}]},"level":200}"#,
            r#"["\"\\\/\b\f\n\r\t", "\u00e7\ud83d\ude80", "İstanbul ğüşıöç"]"#,
            "  [ ]  ",
            "-0",
            "123456789012345678901234567890",
            r#"{"":""}"#,
            "[1,]",
            "{,}",
            "[01]",
            r#""\'""#,
            "\"\t\"",
            "[\"a\"]x",
            "{\"a\":1}}",
            "\u{feff}{}",
            "[-]",
            "[1.e5]",
            "[.1]",
            "nan",
            "[Infinity]",
        ] {
            let bizimki = oku(metin);
            let serdeninki = serde_json::from_str::<Value>(metin);
            assert_eq!(bizimki.is_ok(), serdeninki.is_ok(), "{metin:?}");
            if let (Ok(bizimki), Ok(serdeninki)) = (bizimki, serdeninki) {
                assert_eq!(serdeye(&bizimki), serdeninki, "{metin:?}");
            }
        }
        // Bilinen iki fark, ikisi de RFC'ye göre geçerli metinlerde. serde_json f64'e sığmayan
        // bir sayıyı f64'e çevirmeye çalıştığı için reddeder; bu okuyucu sayıları çevirmez.
        // Eşi olmayan bir vekil kaçışını serde_json reddeder; bu okuyucu � olarak okur.
        for metin in ["1E400", r#""\ud800""#, r#""a\udc00b""#] {
            assert!(oku(metin).is_ok(), "{metin}");
            assert!(serde_json::from_str::<Value>(metin).is_err(), "{metin}");
        }
        assert_eq!(
            oku(r#""a\udc00b""#).map(|deger| serdeye(&deger)),
            Ok(Value::String("a\u{FFFD}b".to_string()))
        );

        // Derinlik sınırı da aynı: 127 düzey okunur, 128 reddedilir
        for derinlik in [EN_FAZLA_DERINLIK, EN_FAZLA_DERINLIK + 1] {
            let metin = format!("{}{}", "[".repeat(derinlik), "]".repeat(derinlik));
            assert_eq!(
                oku(&metin).is_ok(),
                serde_json::from_str::<Value>(&metin).is_ok(),
                "{derinlik}"
            );
        }
        assert_eq!(EN_FAZLA_DERINLIK, 127);
    }

    /// Sözde rastgele (her çalışmada aynı) iç içe JSON değerleri üretir. Metinlerde tırnak,
    /// ters bölü, kontrol karakterleri, Türkçe harfler ve emoji vardır.
    fn rastgele_deger(sonraki: &mut impl FnMut(u64) -> u64, derinlik: usize) -> Value {
        let turu = if derinlik > 3 { sonraki(4) } else { sonraki(6) };
        match turu {
            0 => Value::Null,
            1 => Value::Bool(sonraki(2) == 1),
            2 => match sonraki(3) {
                0 => Value::from(sonraki(1_000_000) as i64 - 500_000),
                1 => Value::from(sonraki(1_000_000) as f64 / 64.0 - 7_000.0),
                _ => Value::from(u64::MAX - sonraki(1_000)),
            },
            3 => Value::String(rastgele_metin(sonraki)),
            4 => Value::Array(
                (0..sonraki(5))
                    .map(|_| rastgele_deger(sonraki, derinlik + 1))
                    .collect(),
            ),
            _ => Value::Object(
                (0..sonraki(5))
                    .map(|_| {
                        (
                            rastgele_metin(sonraki),
                            rastgele_deger(sonraki, derinlik + 1),
                        )
                    })
                    .collect(),
            ),
        }
    }

    fn rastgele_metin(sonraki: &mut impl FnMut(u64) -> u64) -> String {
        const HARFLER: [char; 16] = [
            'a', 'Z', '0', ' ', '"', '\\', '/', '\n', '\t', '\u{1}', '\u{1f}', '\u{7f}', 'ç', 'İ',
            '🚀', '\u{2028}',
        ];
        (0..sonraki(12))
            .map(|_| HARFLER[sonraki(HARFLER.len() as u64) as usize])
            .collect()
    }

    #[test]
    fn rastgele_degerler_geri_okunur() {
        let mut tohum: u64 = 2026;
        let mut sonraki = |ust: u64| {
            tohum = tohum
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (tohum >> 33) % ust
        };
        for _ in 0..2_000 {
            let deger = rastgele_deger(&mut sonraki, 0);
            // serde_json yazar, biz okuruz: aynı değer çıkmalı. İki yazılışı da dene: sıkışık
            // ve girintili (satır sonları ve boşluklarla)
            for metin in [
                serde_json::to_string(&deger).unwrap(),
                serde_json::to_string_pretty(&deger).unwrap(),
            ] {
                let okunan = oku(&metin).unwrap_or_else(|hata| panic!("{hata}: {metin}"));
                assert_eq!(serdeye(&okunan), deger, "{metin}");
            }
        }
    }
}
