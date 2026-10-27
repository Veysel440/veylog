//! Kayıt sütunları: her kaydın zamanı ve alt grubu, sıkıştırılmış olarak.
//!
//! Karşılaştırma ve ani artış her kaydın zamanını ve hangi kalıp satırına düştüğünü bilmelidir.
//! Bunlar kayıt başına iki sayıdır; düz tutulsalar 12 bayt ederdi (8 baytlık zaman, 4 baytlık
//! alt grup numarası). Sütunlar onları değişken uzunluklu tamsayılar olarak (LEB128) yazar:
//!
//! - Zaman, bir önceki kaydın zamanından farkı olarak yazılır (delta kodlaması). Kayıtlar
//!   çoğunlukla zaman sırasıyla yazılır; fark çoğu zaman birkaç saniyedir ve tek bayta sığar.
//!   Fark eksi de olabilir (aynı anda yazan iki işlem); zigzag kodlaması küçük eksi sayıları da
//!   küçük artı sayılara çevirir: 0 → 0, −1 → 1, 1 → 2, −2 → 3...
//! - Alt grup numarası olduğu gibi yazılır: ilk 128 alt grup tek bayttır.
//!
//! LEB128'de her bayt sayının 7 bitini taşır; en üst bit "devamı var" demektir. 300 →
//! `0b1_0101100 0b0_0000010`: önce düşük 7 bit (44), sonra kalanı (2 × 128).
//!
//! Böylece bir kayıt çoğunlukla 2 ya da 3 bayt tutar. Sütunlar yalnızca baştan sona okunabilir;
//! karşılaştırma da ani artış da kayıtları zaten baştan sona bir kez dolaşır.

/// Kayıtların zamanları (1970'ten bu yana saniye) ve alt grup numaraları, eklendikleri sırayla.
#[derive(Debug, Default, Clone)]
pub struct KayitSutunlari {
    baytlar: Vec<u8>,
    uzunluk: usize,
    /// Son eklenen kaydın zamanı: bir sonrakinin farkı buna göre yazılır.
    son_zaman: i64,
}

impl KayitSutunlari {
    /// Bir kaydı sona ekler.
    pub fn ekle(&mut self, zaman: i64, alt_grup: u32) {
        // `wrapping_sub`: taşmada program çökmesin. Kayıt zamanları dört basamaklı yıllardır
        // (±3·10¹¹ sn), taşma olmaz; olsa bile okurken `wrapping_add` aynı sayıyı geri verir.
        yaz(
            &mut self.baytlar,
            zigzag(zaman.wrapping_sub(self.son_zaman)),
        );
        yaz(&mut self.baytlar, u64::from(alt_grup));
        self.son_zaman = zaman;
        self.uzunluk += 1;
    }

    /// Kayıt sayısı.
    pub fn len(&self) -> usize {
        self.uzunluk
    }

    /// Sütunların bellekte tuttuğu bayt.
    #[cfg(test)]
    pub fn bayt_sayisi(&self) -> usize {
        self.baytlar.len()
    }

    /// Kayıtlar eklendikleri sırayla: (zaman, alt grup).
    pub fn iter(&self) -> SutunOkuyucu<'_> {
        SutunOkuyucu {
            baytlar: &self.baytlar,
            konum: 0,
            zaman: 0,
            kalan: self.uzunluk,
        }
    }
}

/// Sütunları baştan sona okuyan yineleyici.
pub struct SutunOkuyucu<'a> {
    baytlar: &'a [u8],
    konum: usize,
    zaman: i64,
    kalan: usize,
}

impl Iterator for SutunOkuyucu<'_> {
    type Item = (i64, u32);

    fn next(&mut self) -> Option<(i64, u32)> {
        if self.kalan == 0 {
            return None;
        }
        self.kalan -= 1;
        self.zaman = self
            .zaman
            .wrapping_add(zigzag_coz(oku(self.baytlar, &mut self.konum)));
        let alt_grup = u32::try_from(oku(self.baytlar, &mut self.konum))
            .expect("alt grup numarası u32 olarak yazıldı");
        Some((self.zaman, alt_grup))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.kalan, Some(self.kalan))
    }
}

/// `size_hint` kesin olduğu için: `len()` kalan kayıt sayısını verir.
impl ExactSizeIterator for SutunOkuyucu<'_> {}

/// Zigzag: işaretli sayıyı, mutlak değeri küçükken küçük kalan işaretsiz sayıya çevirir.
/// `n >> 63` eksi sayıda bütün bitleri 1, artıda 0 olan bir maskedir.
fn zigzag(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

fn zigzag_coz(n: u64) -> i64 {
    ((n >> 1) as i64) ^ -((n & 1) as i64)
}

/// Sayıyı LEB128 olarak yazar: düşük 7 bitten başlayarak, her baytta 7 bit.
fn yaz(baytlar: &mut Vec<u8>, mut n: u64) {
    while n >= 0x80 {
        baytlar.push((n as u8) | 0x80);
        n >>= 7;
    }
    baytlar.push(n as u8);
}

/// `konum`daki LEB128 sayısını okur ve `konum`u ilerletir. Baytlar `yaz`ın yazdığı gibidir.
fn oku(baytlar: &[u8], konum: &mut usize) -> u64 {
    let mut sayi = 0;
    let mut kayma = 0;
    loop {
        let bayt = baytlar[*konum];
        *konum += 1;
        sayi |= u64::from(bayt & 0x7f) << kayma;
        if bayt < 0x80 {
            return sayi;
        }
        kayma += 7;
    }
}

#[cfg(test)]
mod testler {
    use super::*;

    #[test]
    fn zigzag_kucuk_sayilari_kucuk_tutar() {
        let ciftler = [
            (0, 0),
            (-1, 1),
            (1, 2),
            (-2, 3),
            (2, 4),
            (63, 126),
            (-64, 127),
        ];
        for (sayi, kod) in ciftler {
            assert_eq!(zigzag(sayi), kod, "{sayi}");
            assert_eq!(zigzag_coz(kod), sayi, "{kod}");
        }
        for sayi in [i64::MIN, i64::MAX, i64::MIN + 1, -1_000_000_007, 1 << 40] {
            assert_eq!(zigzag_coz(zigzag(sayi)), sayi, "{sayi}");
        }
    }

    #[test]
    fn leb128_bayt_sayilari() {
        for (sayi, beklenen) in [
            (0, vec![0]),
            (1, vec![1]),
            (127, vec![0x7f]),
            (128, vec![0x80, 1]),
            (300, vec![0b1010_1100, 0b10]),
            (16_383, vec![0xff, 0x7f]),
            (16_384, vec![0x80, 0x80, 1]),
        ] {
            let mut baytlar = Vec::new();
            yaz(&mut baytlar, sayi);
            assert_eq!(baytlar, beklenen, "{sayi}");
            let mut konum = 0;
            assert_eq!(oku(&baytlar, &mut konum), sayi);
            assert_eq!(konum, baytlar.len());
        }
        // En büyük sayı 10 bayt
        let mut baytlar = Vec::new();
        yaz(&mut baytlar, u64::MAX);
        assert_eq!(baytlar.len(), 10);
        assert_eq!(oku(&baytlar, &mut 0), u64::MAX);
    }

    #[test]
    fn kayitlar_eklendikleri_gibi_okunur() {
        // Sıralı, geri giden, aynı saniyede ve uzak zamanlar; küçük ve büyük alt gruplar
        let mut kayitlar = vec![
            (1_761_300_000, 0),
            (1_761_300_000, 1),
            (1_761_300_007, 0),
            (1_761_299_990, 127),
            (1_761_299_991, 128),
            (-62_167_219_200, 5),
            (253_402_300_799, u32::MAX),
            (0, 3),
        ];
        // Sözde rastgele binlerce kayıt daha (xorshift): sınırları dolaşsın
        let mut durum: u64 = 0x9e37_79b9_7f4a_7c15;
        for _ in 0..5_000 {
            durum ^= durum << 13;
            durum ^= durum >> 7;
            durum ^= durum << 17;
            let zaman = 1_761_300_000 + (durum % 7_200) as i64 - 3_600;
            kayitlar.push((zaman, (durum >> 40) as u32 % 3_000));
        }
        let mut sutunlar = KayitSutunlari::default();
        for &(zaman, alt_grup) in &kayitlar {
            sutunlar.ekle(zaman, alt_grup);
        }
        assert_eq!(sutunlar.len(), kayitlar.len());
        assert_eq!(sutunlar.iter().len(), kayitlar.len());
        assert_eq!(sutunlar.iter().collect::<Vec<_>>(), kayitlar);
    }

    #[test]
    fn sirali_logda_kayit_basina_iki_bayt() {
        // Saniyede birkaç kayıt, 100 alt grup: zaman farkı ve alt grup tek bayt
        let mut sutunlar = KayitSutunlari::default();
        for sira in 0..10_000_u32 {
            sutunlar.ekle(1_761_300_000 + i64::from(sira / 3), sira % 100);
        }
        // İlk kaydın zamanı 1970'ten farkıdır: 5 bayt
        assert_eq!(sutunlar.bayt_sayisi(), 2 * 10_000 + 4);
    }

    #[test]
    fn bos_sutun() {
        let sutunlar = KayitSutunlari::default();
        assert_eq!(sutunlar.len(), 0);
        assert_eq!(sutunlar.iter().next(), None);
    }
}
