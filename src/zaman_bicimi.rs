//! Kayıt zamanının biçimi: PHP'nin `date()` biçim dizgesi (`--zaman-bicimi`).
//!
//! Laravel zamanı Monolog'a `Y-m-d H:i:s` biçimiyle yazdırır; Monolog'un kendi varsayılanı ISO
//! 8601'dir (`Y-m-d\TH:i:sP`). veylog ikisini kendiliğinden tanır. Uygulama başka bir biçim
//! seçtiyse (`config/logging.php`'de `formatter_with` ile verilen `dateFormat`), aynı dizge
//! `--zaman-bicimi` ile verilir: `d.m.Y H:i:s`, `d/M/Y:H:i:s O`, `U`...
//!
//! Biçim dizgesi bir kez derlenir: her harf bir parçaya dönüşür (`d` iki basamaklı gün, `M` ayın
//! kısa İngilizce adı, `P` saat dilimi...). Harf olmayan karakterler ve PHP'nin tanımadığı
//! harfler olduğu gibi yazılır, `\` kendinden sonrakini harf olmaktan çıkarır: PHP de böyle
//! yazar. Bir kaydın zamanı bu parçalarla soldan sağa okunur ve veylog'un iç biçimine çevrilir:
//! `2026-10-27 14:03:11`, varsa saniyenin kesri ve saat dilimiyle (`.123456+03:00`). Kayıt
//! bundan sonra Laravel'in biçiminde yazılmış gibi işlenir.
//!
//! PHP ay ve gün adlarını her zaman İngilizce yazar (`date()` dile göre değişmez); bu yüzden
//! adlar İngilizce okunur.

use std::fmt::{self, Write};

use crate::dil::{self, Dil};
use crate::zaman::{SaatDilimi, Zaman};

/// Ayların İngilizce adları; kısa adları ilk üç harfleridir.
const AYLAR: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

/// Derlenmiş bir zaman biçimi.
#[derive(Debug, Clone, PartialEq)]
pub struct ZamanBicimi {
    parcalar: Vec<Parca>,
}

/// Biçimin bir parçası: bir harfin okuduğu şey.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Parca {
    /// Bir sayı alanı, en az ve en çok basamağıyla: `d` iki basamaklı gün, `j` bir ya da iki
    Sayi(Alan, usize, usize),
    /// Ayın adı: `M` kısa (`Oct`), `F` uzun (`October`)
    AyAdi { uzun: bool },
    /// Günün adı: `D` kısa (`Mon`), `l` uzun (`Monday`). Okunur ama kullanılmaz: tarih zaten
    /// gün, ay ve yıldan bellidir.
    GunAdi { uzun: bool },
    /// Günün sıra eki (`S`): `st`, `nd`, `rd`, `th`
    SiraEki,
    /// Öğleden önce ya da sonra (`A`, `a`): `AM`, `pm`
    OgleEki,
    /// Saniyenin kesri: `u` 6 basamak (mikrosaniye), `v` 3 basamak (milisaniye)
    Kesir(usize),
    /// Saat dilimi
    Dilim(DilimYazimi),
    /// Unix zamanı (`U`): 1970-01-01 00:00:00 UTC'den bu yana geçen saniye
    Unix,
    /// Okunup atlanan bir sayı: haftanın günü, yılın günü, hafta numarası... Zamanı
    /// değiştirmez.
    Atla(usize, usize),
    /// Olduğu gibi yazılan karakter
    Harf(char),
}

/// Bir sayının hangi alana yazıldığı.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Alan {
    Yil,
    /// İki basamaklı yıl (`y`): 00–69 2000'li, 70–99 1900'lü yıllar
    KisaYil,
    Ay,
    Gun,
    /// 24 saatlik saat (`H`, `G`)
    Saat,
    /// 12 saatlik saat (`h`, `g`); öğle eki (`A`) ile birlikte okunur
    Saat12,
    Dakika,
    Saniye,
}

/// Saat diliminin yazılışı.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DilimYazimi {
    /// `P`: `+03:00`
    IkiNoktali,
    /// `p`: `P` gibi, ama UTC `Z` diye yazılır
    IkiNoktaliYaDaZ,
    /// `O`: `+0300`
    Bitisik,
    /// `T`: kısaltma. `UTC`, `GMT` ve sayıyla yazılanlar (`+03`) okunur; `CET`, `EST` gibi
    /// kısaltmalar birden çok dilime karşılık gelebildiği için okunur ama çevrilmez.
    Kisaltma,
    /// `Z`: UTC'den fark, saniye (`10800`)
    Saniye,
    /// `e`: dilimin adı (`Europe/Istanbul`). Ad saat farkını vermez (yaz saati); okunur ama
    /// çevrilmez. `UTC` ve sayıyla yazılmış adlar okunur.
    Ad,
}

/// Biçim derlenemedi.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZamanBicimiHatasi {
    /// Zamanı belirlemek için gereken alanlardan bazıları biçimde yok.
    Eksik(Vec<&'static str>),
    /// 12 saatlik saat var ama öğle eki yok: `h:i` 09:30'u 21:30'dan ayıramaz.
    OgleEkiYok,
}

impl fmt::Display for ZamanBicimiHatasi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self, dil::simdiki()) {
            (ZamanBicimiHatasi::Eksik(alanlar), Dil::Tr) => write!(
                f,
                "zaman biçiminde {} yok; yıl (Y), ay (m), gün (d), saat (H) ve dakika (i) gerekir, ya da Unix zamanı (U)",
                dil::liste(alanlar)
            ),
            (ZamanBicimiHatasi::Eksik(alanlar), Dil::En) => {
                let alanlar: Vec<&str> = alanlar.iter().map(|alan| alan_ingilizce(alan)).collect();
                write!(
                    f,
                    "the time format has no {}; year (Y), month (m), day (d), hour (H) and minute (i) are required, or a Unix time (U)",
                    dil::liste(&alanlar)
                )
            }
            (ZamanBicimiHatasi::OgleEkiYok, Dil::Tr) => write!(
                f,
                "12 saatlik saat (h, g) öğleden önce/sonra işareti (A ya da a) olmadan okunamaz"
            ),
            (ZamanBicimiHatasi::OgleEkiYok, Dil::En) => write!(
                f,
                "a 12-hour clock (h, g) cannot be read without an AM/PM marker (A or a)"
            ),
        }
    }
}

/// Eksik alanın İngilizce adı: `yıl` → `year`.
fn alan_ingilizce(alan: &str) -> &str {
    match alan {
        "yıl" => "year",
        "ay" => "month",
        "gün" => "day",
        "saat" => "hour",
        "dakika" => "minute",
        diger => diger,
    }
}

impl ZamanBicimi {
    /// PHP'nin biçim dizgesini derler.
    pub fn derle(bicim: &str) -> Result<ZamanBicimi, ZamanBicimiHatasi> {
        let mut parcalar = Vec::new();
        parcalari_ekle(bicim, &mut parcalar);
        let var = |aranan: &dyn Fn(&Parca) -> bool| parcalar.iter().any(aranan);
        let alan_var = |alanlar: &[Alan]| {
            var(&|parca| matches!(parca, Parca::Sayi(alan, _, _) if alanlar.contains(alan)))
        };
        if !var(&|parca| *parca == Parca::Unix) {
            let mut eksik = Vec::new();
            if !alan_var(&[Alan::Yil, Alan::KisaYil]) {
                eksik.push("yıl");
            }
            if !alan_var(&[Alan::Ay]) && !var(&|parca| matches!(parca, Parca::AyAdi { .. })) {
                eksik.push("ay");
            }
            if !alan_var(&[Alan::Gun]) {
                eksik.push("gün");
            }
            if !alan_var(&[Alan::Saat, Alan::Saat12]) {
                eksik.push("saat");
            }
            if !alan_var(&[Alan::Dakika]) {
                eksik.push("dakika");
            }
            if !eksik.is_empty() {
                return Err(ZamanBicimiHatasi::Eksik(eksik));
            }
            if !alan_var(&[Alan::Saat]) && !var(&|parca| *parca == Parca::OgleEki) {
                return Err(ZamanBicimiHatasi::OgleEkiYok);
            }
        }
        Ok(ZamanBicimi { parcalar })
    }

    /// Bir zamanı okur ve veylog'un iç biçimine çevirir: `2026-10-27 14:03:11`, varsa
    /// saniyenin kesri ve saat dilimiyle. Zaman biçime uymuyorsa ya da takvimde yoksa None.
    pub fn coz(&self, metin: &str) -> Option<String> {
        let mut okunan = Okunan { metin, konum: 0 };
        let mut degerler = Degerler::default();
        for parca in &self.parcalar {
            parca.oku(&mut okunan, &mut degerler)?;
        }
        // Zamanın tamamı okunmalı: fazlası biçime uymuyor demektir
        if okunan.konum != metin.len() {
            return None;
        }
        degerler.zaman_metni()
    }

    /// Biçime uyan zamanları bulan düzenli ifade (yakalayan grubu olmadan). Monolog'un satır
    /// biçimi `--desen`'e verildiğinde `%datetime%`'ın yerine bu konur.
    pub fn duzenli_ifade(&self) -> String {
        let mut ifade = String::new();
        for parca in &self.parcalar {
            ifade.push_str(&parca.duzenli_ifade());
        }
        ifade
    }
}

/// Biçim dizgesindeki her harfi parçasına çevirir. `c` ve `r` kısaltmadır: ISO 8601 ve
/// RFC 2822 biçimlerinin tamamı.
fn parcalari_ekle(bicim: &str, parcalar: &mut Vec<Parca>) {
    let mut harfler = bicim.chars();
    while let Some(harf) = harfler.next() {
        let parca = match harf {
            // Sondaki tek `\` kendisi olarak yazılır
            '\\' => Parca::Harf(harfler.next().unwrap_or('\\')),
            'c' => {
                parcalari_ekle(r"Y-m-d\TH:i:sP", parcalar);
                continue;
            }
            'r' => {
                parcalari_ekle("D, d M Y H:i:s O", parcalar);
                continue;
            }
            'd' => Parca::Sayi(Alan::Gun, 2, 2),
            'j' => Parca::Sayi(Alan::Gun, 1, 2),
            'D' => Parca::GunAdi { uzun: false },
            'l' => Parca::GunAdi { uzun: true },
            'S' => Parca::SiraEki,
            'N' | 'w' | 'L' | 'I' => Parca::Atla(1, 1),
            'z' => Parca::Atla(1, 3),
            'W' => Parca::Atla(1, 2),
            't' => Parca::Atla(2, 2),
            'B' => Parca::Atla(3, 3),
            'o' => Parca::Atla(4, 4),
            'F' => Parca::AyAdi { uzun: true },
            'M' => Parca::AyAdi { uzun: false },
            'm' => Parca::Sayi(Alan::Ay, 2, 2),
            'n' => Parca::Sayi(Alan::Ay, 1, 2),
            // `X` ve `x` dört basamaktan uzun yılları işaretle yazar; dört basamaklı yıllarda
            // `Y` ile aynıdır
            'Y' | 'X' | 'x' => Parca::Sayi(Alan::Yil, 4, 4),
            'y' => Parca::Sayi(Alan::KisaYil, 2, 2),
            'a' | 'A' => Parca::OgleEki,
            'g' => Parca::Sayi(Alan::Saat12, 1, 2),
            'h' => Parca::Sayi(Alan::Saat12, 2, 2),
            'G' => Parca::Sayi(Alan::Saat, 1, 2),
            'H' => Parca::Sayi(Alan::Saat, 2, 2),
            'i' => Parca::Sayi(Alan::Dakika, 2, 2),
            's' => Parca::Sayi(Alan::Saniye, 2, 2),
            'u' => Parca::Kesir(6),
            'v' => Parca::Kesir(3),
            'e' => Parca::Dilim(DilimYazimi::Ad),
            'O' => Parca::Dilim(DilimYazimi::Bitisik),
            'P' => Parca::Dilim(DilimYazimi::IkiNoktali),
            'p' => Parca::Dilim(DilimYazimi::IkiNoktaliYaDaZ),
            'T' => Parca::Dilim(DilimYazimi::Kisaltma),
            'Z' => Parca::Dilim(DilimYazimi::Saniye),
            'U' => Parca::Unix,
            diger => Parca::Harf(diger),
        };
        parcalar.push(parca);
    }
}

/// Okunmakta olan zaman ve okunduğu yer.
struct Okunan<'m> {
    metin: &'m str,
    konum: usize,
}

impl<'m> Okunan<'m> {
    /// Koşula uyan en az `en_az`, en çok `en_cok` ASCII karakteri okur. ASCII karakterler tek
    /// bayttır; konum karakter sınırında kalır.
    fn al(&mut self, en_az: usize, en_cok: usize, kosul: fn(&u8) -> bool) -> Option<&'m str> {
        let kalan = &self.metin.as_bytes()[self.konum..];
        let uzunluk = kalan
            .iter()
            .take(en_cok)
            .take_while(|bayt| kosul(bayt))
            .count();
        if uzunluk < en_az {
            return None;
        }
        let parca = &self.metin[self.konum..self.konum + uzunluk];
        self.konum += uzunluk;
        Some(parca)
    }

    fn rakamlar(&mut self, en_az: usize, en_cok: usize) -> Option<&'m str> {
        self.al(en_az, en_cok, u8::is_ascii_digit)
    }

    fn harfler(&mut self, en_az: usize, en_cok: usize) -> Option<&'m str> {
        self.al(en_az, en_cok, u8::is_ascii_alphabetic)
    }

    /// Sıradaki karakter `beklenen`se okur.
    fn karakter(&mut self, beklenen: char) -> Option<()> {
        let geri = self.metin[self.konum..].strip_prefix(beklenen)?;
        self.konum = self.metin.len() - geri.len();
        Some(())
    }

    /// Sıradaki karakter `beklenen`se okur ve true döner; değilse bir şey okumaz.
    fn varsa(&mut self, beklenen: char) -> bool {
        self.karakter(beklenen).is_some()
    }

    /// Okunan kısmın `bas`tan bu yana olanı.
    fn bastan(&self, bas: usize) -> &'m str {
        &self.metin[bas..self.konum]
    }
}

/// Okunan alanlar.
#[derive(Debug, Default)]
struct Degerler<'m> {
    yil: Option<u16>,
    ay: Option<u16>,
    gun: Option<u16>,
    saat: Option<u16>,
    saat12: Option<u16>,
    ogleden_sonra: Option<bool>,
    dakika: Option<u16>,
    saniye: Option<u16>,
    kesir: Option<&'m str>,
    dilim: Option<SaatDilimi>,
    unix: Option<i64>,
}

impl Parca {
    /// Parçayı okur, değerini `degerler`e yazar. Uymuyorsa None.
    fn oku<'m>(self, okunan: &mut Okunan<'m>, degerler: &mut Degerler<'m>) -> Option<()> {
        match self {
            Parca::Sayi(alan, en_az, en_cok) => {
                let deger: u16 = okunan.rakamlar(en_az, en_cok)?.parse().ok()?;
                match alan {
                    Alan::Yil => degerler.yil = Some(deger),
                    Alan::KisaYil => {
                        let yuzyil = if deger < 70 { 2000 } else { 1900 };
                        degerler.yil = Some(yuzyil + deger);
                    }
                    Alan::Ay => degerler.ay = Some(deger),
                    Alan::Gun => degerler.gun = Some(deger),
                    Alan::Saat => degerler.saat = Some(deger),
                    Alan::Saat12 => degerler.saat12 = Some(deger),
                    Alan::Dakika => degerler.dakika = Some(deger),
                    Alan::Saniye => degerler.saniye = Some(deger),
                }
            }
            Parca::AyAdi { uzun } => {
                let ad = if uzun {
                    okunan.harfler(3, usize::MAX)?
                } else {
                    okunan.harfler(3, 3)?
                };
                // Uzun adın yerine kısası da kabul edilir: `May` ikisidir
                let sira = AYLAR.iter().position(|ay| {
                    ay.eq_ignore_ascii_case(ad) || ay[..3].eq_ignore_ascii_case(ad)
                })?;
                degerler.ay = Some(u16::try_from(sira + 1).ok()?);
            }
            Parca::GunAdi { uzun } => {
                if uzun {
                    okunan.harfler(3, usize::MAX)?;
                } else {
                    okunan.harfler(3, 3)?;
                }
            }
            Parca::SiraEki => {
                okunan.harfler(2, 2)?;
            }
            Parca::OgleEki => {
                let ek = okunan.harfler(2, 2)?;
                degerler.ogleden_sonra = Some(if ek.eq_ignore_ascii_case("pm") {
                    true
                } else if ek.eq_ignore_ascii_case("am") {
                    false
                } else {
                    return None;
                });
            }
            Parca::Kesir(basamak) => degerler.kesir = Some(okunan.rakamlar(basamak, basamak)?),
            Parca::Dilim(yazim) => degerler.dilim = dilim_oku(yazim, okunan)?,
            Parca::Unix => {
                let bas = okunan.konum;
                okunan.varsa('-');
                okunan.rakamlar(1, 12)?;
                degerler.unix = Some(okunan.bastan(bas).parse().ok()?);
            }
            Parca::Atla(en_az, en_cok) => {
                okunan.rakamlar(en_az, en_cok)?;
            }
            Parca::Harf(harf) => okunan.karakter(harf)?,
        }
        Some(())
    }

    /// Parçaya uyan metni bulan düzenli ifade. `\d` Unicode rakamlarını da kabul ettiği için
    /// `[0-9]` yazılır.
    fn duzenli_ifade(self) -> String {
        let rakam = |en_az: usize, en_cok: usize| {
            if en_az == en_cok {
                format!("[0-9]{{{en_az}}}")
            } else {
                format!("[0-9]{{{en_az},{en_cok}}}")
            }
        };
        match self {
            Parca::Sayi(_, en_az, en_cok) | Parca::Atla(en_az, en_cok) => rakam(en_az, en_cok),
            Parca::AyAdi { uzun: false } | Parca::GunAdi { uzun: false } => "[A-Za-z]{3}".into(),
            Parca::AyAdi { uzun: true } | Parca::GunAdi { uzun: true } => "[A-Za-z]{3,}".into(),
            Parca::SiraEki => "[A-Za-z]{2}".into(),
            Parca::OgleEki => "[AaPp][Mm]".into(),
            Parca::Kesir(basamak) => rakam(basamak, basamak),
            Parca::Dilim(DilimYazimi::IkiNoktali) => "[+-][0-9]{2}:[0-9]{2}".into(),
            Parca::Dilim(DilimYazimi::IkiNoktaliYaDaZ) => "(?:Z|[+-][0-9]{2}:[0-9]{2})".into(),
            Parca::Dilim(DilimYazimi::Bitisik) => "[+-][0-9]{4}".into(),
            Parca::Dilim(DilimYazimi::Kisaltma) => {
                "(?:[A-Za-z]{1,6}|[+-][0-9]{2}(?::?[0-9]{2})?)".into()
            }
            Parca::Dilim(DilimYazimi::Saniye) => "[+-]?[0-9]{1,5}".into(),
            Parca::Dilim(DilimYazimi::Ad) => "[A-Za-z0-9_/+:-]+".into(),
            Parca::Unix => "-?[0-9]{1,12}".into(),
            Parca::Harf(harf) => regex::escape(harf.encode_utf8(&mut [0; 4])),
        }
    }
}

/// Saat dilimini okur. Okunamazsa None; okundu ama farkı bilinmiyorsa (`CET`, `Europe/Istanbul`)
/// `Some(None)`.
fn dilim_oku(yazim: DilimYazimi, okunan: &mut Okunan<'_>) -> Option<Option<SaatDilimi>> {
    let bas = okunan.konum;
    // İşaret ve iki basamaklı saat; ardından `:` ile ya da bitişik iki basamaklı dakika
    let isaretli = |okunan: &mut Okunan<'_>, iki_nokta: bool| -> Option<()> {
        if !okunan.varsa('+') {
            okunan.karakter('-')?;
        }
        okunan.rakamlar(2, 2)?;
        if iki_nokta {
            okunan.karakter(':')?;
        }
        okunan.rakamlar(2, 2).map(|_| ())
    };
    match yazim {
        DilimYazimi::IkiNoktali => isaretli(okunan, true)?,
        DilimYazimi::IkiNoktaliYaDaZ => {
            if okunan.varsa('Z') {
                return Some(Some(SaatDilimi::UTC));
            }
            isaretli(okunan, true)?;
        }
        DilimYazimi::Bitisik => isaretli(okunan, false)?,
        DilimYazimi::Saniye => {
            if !okunan.varsa('+') {
                okunan.varsa('-');
            }
            okunan.rakamlar(1, 5)?;
            let saniye = okunan.bastan(bas).parse().ok()?;
            return SaatDilimi::saniyeden(saniye).map(Some);
        }
        DilimYazimi::Kisaltma => {
            if okunan.varsa('+') || okunan.varsa('-') {
                // `+03`, `+0330` ya da `+03:30`
                okunan.rakamlar(2, 2)?;
                let dakika_bas = okunan.konum;
                okunan.varsa(':');
                if okunan.rakamlar(2, 2).is_none() {
                    okunan.konum = dakika_bas;
                }
            } else {
                return Some(utc_mu(okunan.harfler(1, 6)?));
            }
        }
        DilimYazimi::Ad => {
            let ad = okunan.al(1, usize::MAX, |bayt| {
                bayt.is_ascii_alphanumeric() || b"_/+:-".contains(bayt)
            })?;
            return Some(utc_mu(ad).or_else(|| SaatDilimi::ayristir(ad)));
        }
    }
    SaatDilimi::ayristir(okunan.bastan(bas)).map(Some)
}

/// `UTC`, `GMT` ya da `Z` ise UTC.
fn utc_mu(ad: &str) -> Option<SaatDilimi> {
    ["UTC", "GMT", "Z"]
        .iter()
        .any(|utc| ad.eq_ignore_ascii_case(utc))
        .then_some(SaatDilimi::UTC)
}

impl Degerler<'_> {
    /// Okunan alanlardan iç biçimdeki zaman. Unix zamanı verildiyse o; değilse alanlar.
    fn zaman_metni(self) -> Option<String> {
        let (zaman, dilim) = match self.unix {
            Some(saniye) => {
                // Unix zamanı UTC'dir; yanında bir dilim yazılıysa zaman o dilimde gösterilir
                let dilim = self.dilim.unwrap_or(SaatDilimi::UTC);
                (Zaman::saniyeden(saniye + dilim.saniye()), Some(dilim))
            }
            None => {
                let saat = match (self.saat, self.saat12, self.ogleden_sonra) {
                    (Some(saat), _, _) => saat,
                    // 12 AM gece yarısı, 12 PM öğlendir
                    (None, Some(saat), Some(ogleden_sonra)) if (1..=12).contains(&saat) => {
                        saat % 12 + if ogleden_sonra { 12 } else { 0 }
                    }
                    _ => return None,
                };
                let zaman = Zaman::yeni(
                    self.yil?,
                    self.ay?,
                    self.gun?,
                    saat,
                    self.dakika?,
                    self.saniye.unwrap_or(0),
                )?;
                (zaman, self.dilim)
            }
        };
        let mut metin = zaman.to_string();
        if let Some(kesir) = self.kesir {
            metin.push('.');
            metin.push_str(kesir);
        }
        if let Some(dilim) = dilim {
            write!(metin, "{dilim}").expect("String'e yazmak başarısız olmaz");
        }
        Some(metin)
    }
}

#[cfg(test)]
mod testler {
    use super::*;

    fn coz(bicim: &str, metin: &str) -> Option<String> {
        ZamanBicimi::derle(bicim).unwrap().coz(metin)
    }

    #[test]
    fn laravelin_ve_monologun_bicimleri() {
        assert_eq!(
            coz("Y-m-d H:i:s", "2026-10-27 14:03:11").as_deref(),
            Some("2026-10-27 14:03:11")
        );
        assert_eq!(
            coz(r"Y-m-d\TH:i:sP", "2026-10-27T14:03:11+03:00").as_deref(),
            Some("2026-10-27 14:03:11+03:00")
        );
        assert_eq!(
            coz(r"Y-m-d\TH:i:s.uP", "2026-10-27T14:03:11.123456-05:30").as_deref(),
            Some("2026-10-27 14:03:11.123456-05:30")
        );
        assert_eq!(
            coz("c", "2026-10-27T14:03:11+00:00").as_deref(),
            Some("2026-10-27 14:03:11+00:00")
        );
    }

    #[test]
    fn baska_siralar_ve_ayiraclar() {
        assert_eq!(
            coz("d.m.Y H:i:s", "27.10.2026 14:03:11").as_deref(),
            Some("2026-10-27 14:03:11")
        );
        // Apache'nin biçimi: ayın kısa adı ve bitişik saat dilimi
        assert_eq!(
            coz("d/M/Y:H:i:s O", "27/Oct/2026:14:03:11 +0300").as_deref(),
            Some("2026-10-27 14:03:11+03:00")
        );
        // RFC 2822: günün adı okunur, kullanılmaz
        assert_eq!(
            coz("r", "Tue, 27 Oct 2026 14:03:11 -0500").as_deref(),
            Some("2026-10-27 14:03:11-05:00")
        );
        // Tek basamaklı gün, ay ve saat; uzun ay adı; saniyesiz
        assert_eq!(
            coz("j F Y G:i", "7 March 2026 9:05").as_deref(),
            Some("2026-03-07 09:05:00")
        );
        assert_eq!(
            coz("j F Y G:i", "17 mar 2026 19:05").as_deref(),
            Some("2026-03-17 19:05:00")
        );
        // İki basamaklı yıl, milisaniye
        assert_eq!(
            coz("y-m-d H:i:s.v", "26-10-27 14:03:11.042").as_deref(),
            Some("2026-10-27 14:03:11.042")
        );
        assert_eq!(
            coz("y-m-d H:i", "99-12-31 23:59").as_deref(),
            Some("1999-12-31 23:59:00")
        );
        // Kaçışlı harfler olduğu gibi okunur; PHP'nin tanımadığı harfler de. Kaçışsız `a`
        // öğle ekidir: "Tarih" yazmak için her harf kaçışlanır.
        assert_eq!(
            coz(r"\T\a\r\i\h: Y-m-d H:i:s", "Tarih: 2026-10-27 14:03:11").as_deref(),
            Some("2026-10-27 14:03:11")
        );
        assert_eq!(
            coz(r"\Ta\r\i\h: Y-m-d H:i:s", "Tarih: 2026-10-27 14:03:11"),
            None
        );
        assert_eq!(
            coz("Y-m-d H:i:s Q", "2026-10-27 14:03:11 Q").as_deref(),
            Some("2026-10-27 14:03:11")
        );
    }

    #[test]
    fn on_iki_saatlik_saat() {
        let bicim = "Y-m-d h:i:s A";
        assert_eq!(
            coz(bicim, "2026-10-27 12:15:00 AM").as_deref(),
            Some("2026-10-27 00:15:00")
        );
        assert_eq!(
            coz(bicim, "2026-10-27 12:15:00 PM").as_deref(),
            Some("2026-10-27 12:15:00")
        );
        assert_eq!(
            coz(bicim, "2026-10-27 09:15:00 pm").as_deref(),
            Some("2026-10-27 21:15:00")
        );
        assert_eq!(coz(bicim, "2026-10-27 13:15:00 PM"), None);
        assert_eq!(coz(bicim, "2026-10-27 00:15:00 AM"), None);
        assert_eq!(coz(bicim, "2026-10-27 09:15:00 XM"), None);
    }

    #[test]
    fn saat_dilimi_yazilislari() {
        let dilim = |bicim: &str, metin: &str| coz(&format!("Y-m-d H:i:s {bicim}"), metin);
        assert_eq!(
            dilim("p", "2026-10-27 14:03:11 Z").as_deref(),
            Some("2026-10-27 14:03:11+00:00")
        );
        assert_eq!(
            dilim("p", "2026-10-27 14:03:11 +01:00").as_deref(),
            Some("2026-10-27 14:03:11+01:00")
        );
        assert_eq!(
            dilim("Z", "2026-10-27 14:03:11 10800").as_deref(),
            Some("2026-10-27 14:03:11+03:00")
        );
        assert_eq!(
            dilim("Z", "2026-10-27 14:03:11 -19800").as_deref(),
            Some("2026-10-27 14:03:11-05:30")
        );
        // Tam dakika olmayan fark okunmaz
        assert_eq!(dilim("Z", "2026-10-27 14:03:11 10801"), None);
        // Kısaltmalar: UTC ve sayıyla yazılanlar okunur, ötekiler okunur ama çevrilmez
        assert_eq!(
            dilim("T", "2026-10-27 14:03:11 UTC").as_deref(),
            Some("2026-10-27 14:03:11+00:00")
        );
        assert_eq!(
            dilim("T", "2026-10-27 14:03:11 +03").as_deref(),
            Some("2026-10-27 14:03:11+03:00")
        );
        assert_eq!(
            dilim("T", "2026-10-27 14:03:11 +0530").as_deref(),
            Some("2026-10-27 14:03:11+05:30")
        );
        assert_eq!(
            dilim("T", "2026-10-27 14:03:11 CET").as_deref(),
            Some("2026-10-27 14:03:11")
        );
        // Dilimin adı: farkı bilinmez
        assert_eq!(
            dilim("e", "2026-10-27 14:03:11 Europe/Istanbul").as_deref(),
            Some("2026-10-27 14:03:11")
        );
        assert_eq!(
            dilim("e", "2026-10-27 14:03:11 UTC").as_deref(),
            Some("2026-10-27 14:03:11+00:00")
        );
        assert_eq!(
            dilim("e", "2026-10-27 14:03:11 -04:00").as_deref(),
            Some("2026-10-27 14:03:11-04:00")
        );
        // Geçersiz farklar
        assert_eq!(dilim("P", "2026-10-27 14:03:11 +25:00"), None);
        assert_eq!(dilim("P", "2026-10-27 14:03:11 +0300"), None);
        assert_eq!(dilim("O", "2026-10-27 14:03:11 +03:00"), None);
    }

    #[test]
    fn unix_zamani() {
        // 1 800 000 000: 2027-01-15 08:00:00 UTC
        assert_eq!(
            coz("U", "1800000000").as_deref(),
            Some("2027-01-15 08:00:00+00:00")
        );
        assert_eq!(
            coz("U.u", "1800000000.250000").as_deref(),
            Some("2027-01-15 08:00:00.250000+00:00")
        );
        // Yanında dilim yazılıysa zaman o dilimde
        assert_eq!(
            coz("U P", "1800000000 +03:00").as_deref(),
            Some("2027-01-15 11:00:00+03:00")
        );
        assert_eq!(coz("U", "-1").as_deref(), Some("1969-12-31 23:59:59+00:00"));
    }

    #[test]
    fn uymayan_zamanlar() {
        let bicim = "d.m.Y H:i:s";
        for metin in [
            "",
            "27.10.2026 14:03",
            "27.10.2026 14:03:11 ",
            "27-10-2026 14:03:11",
            "2026-10-27 14:03:11",
            "7.10.2026 14:03:11",
            // Takvimde yok
            "30.02.2026 14:03:11",
            "27.13.2026 14:03:11",
            "27.10.2026 24:00:00",
            "27.10.2026 14:60:00",
        ] {
            assert_eq!(coz(bicim, metin), None, "{metin:?}");
        }
        assert_eq!(coz("d M Y H:i", "27 Okt 2026 14:03"), None);
        assert_eq!(coz("d M Y H:i", "27 October 2026 14:03"), None);
        // Unicode rakamları sayı değildir
        assert_eq!(coz("Y-m-d H:i", "٢٠٢٦-10-27 14:03"), None);
    }

    #[test]
    fn eksik_alanlar_ve_ogle_eki() {
        assert_eq!(
            ZamanBicimi::derle("H:i:s"),
            Err(ZamanBicimiHatasi::Eksik(vec!["yıl", "ay", "gün"]))
        );
        assert_eq!(
            ZamanBicimi::derle("Y-m-d"),
            Err(ZamanBicimiHatasi::Eksik(vec!["saat", "dakika"]))
        );
        assert_eq!(
            ZamanBicimi::derle("d M H:i"),
            Err(ZamanBicimiHatasi::Eksik(vec!["yıl"]))
        );
        assert_eq!(
            ZamanBicimi::derle("Y-m-d h:i:s"),
            Err(ZamanBicimiHatasi::OgleEkiYok)
        );
        assert!(ZamanBicimi::derle("U").is_ok());
        assert_eq!(
            ZamanBicimiHatasi::Eksik(vec!["yıl", "ay", "gün"]).to_string(),
            "zaman biçiminde yıl, ay ve gün yok; yıl (Y), ay (m), gün (d), saat (H) ve dakika (i) gerekir, ya da Unix zamanı (U)"
        );
    }

    #[test]
    fn duzenli_ifade_bicime_uyar() {
        for (bicim, metin) in [
            ("Y-m-d H:i:s", "2026-10-27 14:03:11"),
            (r"Y-m-d\TH:i:s.uP", "2026-10-27T14:03:11.123456+03:00"),
            ("d/M/Y:H:i:s O", "27/Oct/2026:14:03:11 +0300"),
            ("r", "Tue, 27 Oct 2026 14:03:11 -0500"),
            ("D, d F Y g:i a T", "Tue, 27 October 2026 2:03 pm CET"),
            ("U.v e", "1800000000.123 Europe/Istanbul"),
            ("Y-m-d H:i:s p Z", "2026-10-27 14:03:11 Z -19800"),
            ("jS F Y H:i", "27th October 2026 14:03"),
            (r"[Y.m.d] H:i (\N)", "[2026.10.27] 14:03 (N)"),
        ] {
            let derlenmis = ZamanBicimi::derle(bicim).unwrap();
            let ifade = regex::Regex::new(&format!("^{}$", derlenmis.duzenli_ifade())).unwrap();
            assert!(ifade.is_match(metin), "{bicim}: {metin}");
            assert!(derlenmis.coz(metin).is_some(), "{bicim}: {metin}");
        }
    }
}
