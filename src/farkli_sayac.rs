//! Farklı değer sayacı: bir akışta kaç farklı değer geçtiğini sayar.
//!
//! Farklı ham mesajları tam saymak için hepsini (ya da özetlerini) saklamak gerekir. Her kaydın
//! mesajı başkaysa (bir kimlik, bir zaman damgası) bu, kayıt sayısıyla büyüyen bir bellek
//! demektir. Sayaç önce değerlerin 64 bitlik özetlerini saklar ve tam sayar. Farklı değer
//! `TAM_SINIR`'ı geçince HyperLogLog'a geçer (Flajolet, Fusy, Gandouet ve Meunier, 2007):
//! 64 KB'lık sabit bir bellekle, ±%0,4 standart hatayla tahmin eder.
//!
//! HyperLogLog'un fikri: iyi bir özet rastgele bitler gibidir; başı k sıfırla başlayan bir özete
//! ortalama 2^k farklı değerde bir rastlanır. Görülen en uzun sıfır dizisi böylece farklı değer
//! sayısını kabaca söyler. Tek bir ölçüm çok oynak olduğu için özetler ilk 16 bitlerine göre
//! 65 536 kovaya dağıtılır; her kova kendi en uzun dizisini tutar.
//!
//! Kovalardan tahmin Ertl'in geliştirilmiş tahmincisiyle yapılır ("New cardinality estimation
//! algorithms for HyperLogLog sketches", 2017). Asıl HyperLogLog tahmini küçük sayılarda
//! sapar; bu yüzden orada doğrusal sayıma geçilir ve iki yöntemin arası deneysel düzeltme
//! tablolarıyla kapatılır (HyperLogLog++). Ertl'in tahmincisi her aralıkta sapmasızdır ve
//! tablo gerektirmez: kovaların değerlerinin dağılımından (kaç kovada 0, kaçında 1...) tek bir
//! formülle hesaplanır.

use std::collections::HashSet;
use std::hash::{BuildHasherDefault, DefaultHasher, Hash, Hasher};

/// Bu kadar farklı değere kadar tam sayılır (özetler 4–8 MB tutar); ötesi tahmindir.
pub const TAM_SINIR: usize = 1 << 18;
/// Kovayı seçen bit sayısı: 2^16 = 65 536 kova. Standart hata 1,04 / √65 536 ≈ %0,41.
const DUYARLIK: u32 = 16;
const KOVA_SAYISI: usize = 1 << DUYARLIK;
/// Kovadan sonra kalan bit sayısı. Bir kovanın değeri en fazla bunun bir fazlası olur.
const KALAN_BIT: u32 = 64 - DUYARLIK;

/// Farklı değer sayacı.
#[derive(Debug, Clone)]
pub struct FarkliSayaci {
    durum: Durum,
}

#[derive(Debug, Clone)]
enum Durum {
    /// Görülen değerlerin özetleri.
    Tam(HashSet<u64, BuildHasherDefault<Gecirgen>>),
    /// HyperLogLog kovaları: her kovada görülen en büyük "sıra", yani özetin kovadan sonraki
    /// bitlerinde ilk 1'in yeri.
    Yaklasik(Box<[u8]>),
}

impl Default for FarkliSayaci {
    fn default() -> Self {
        FarkliSayaci {
            durum: Durum::Tam(HashSet::default()),
        }
    }
}

impl FarkliSayaci {
    /// Bir değeri sayar.
    pub fn ekle(&mut self, deger: &str) {
        let mut ozetleyici = DefaultHasher::new();
        deger.hash(&mut ozetleyici);
        self.ozet_ekle(ozetleyici.finish());
    }

    /// Özeti zaten hesaplanmış bir değeri sayar.
    fn ozet_ekle(&mut self, ozet: u64) {
        match &mut self.durum {
            Durum::Tam(ozetler) => {
                ozetler.insert(ozet);
                if ozetler.len() > TAM_SINIR {
                    let mut kovalar = vec![0; KOVA_SAYISI].into_boxed_slice();
                    for &ozet in ozetler.iter() {
                        kovaya_ekle(&mut kovalar, ozet);
                    }
                    self.durum = Durum::Yaklasik(kovalar);
                }
            }
            Durum::Yaklasik(kovalar) => kovaya_ekle(kovalar, ozet),
        }
    }

    /// Farklı değer sayısı; `TAM_SINIR`'dan fazlaysa tahmini. Tahmin `TAM_SINIR`'ın altına
    /// inmez: sayaç ancak o kadar farklı değer görünce tahmine geçer; bir değer daha eklenince
    /// sayı (tahminin hatası yüzünden) gerilemesin.
    pub fn sayi(&self) -> usize {
        match &self.durum {
            Durum::Tam(ozetler) => ozetler.len(),
            Durum::Yaklasik(kovalar) => (tahmin(kovalar).round() as usize).max(TAM_SINIR + 1),
        }
    }

    /// Sayı bir tahmin mi?
    pub fn yaklasik_mi(&self) -> bool {
        matches!(self.durum, Durum::Yaklasik(_))
    }
}

/// Özeti kovasına ekler. Kova, özetin ilk `DUYARLIK` bitidir; sıra, kalan bitlerde baştaki
/// sıfırların sayısının bir fazlası (kalan bitler hep sıfırsa en büyük sıra).
fn kovaya_ekle(kovalar: &mut [u8], ozet: u64) {
    let kova = (ozet >> (64 - DUYARLIK)) as usize;
    let kalan = ozet << DUYARLIK;
    let sira = (kalan.leading_zeros() + 1).min(KALAN_BIT + 1) as u8;
    kovalar[kova] = kovalar[kova].max(sira);
}

/// Ertl'in geliştirilmiş tahmincisi: α∞·m² / z, α∞ = 1 / (2 ln 2). `z`, kovaların değer
/// dağılımından hesaplanır: C[k], değeri k olan kova sayısı. Asıl formüldeki Σ 2^(−değer)
/// toplamının sapmasız karşılığıdır; uçlardaki iki terim (hiç dolmamış kovalar ve en büyük
/// değere ulaşmış kovalar) σ ve τ serileriyle düzeltilir.
fn tahmin(kovalar: &[u8]) -> f64 {
    let m = kovalar.len() as f64;
    let mut dagilim = [0_usize; KALAN_BIT as usize + 2];
    for &deger in kovalar {
        dagilim[usize::from(deger)] += 1;
    }
    let son = KALAN_BIT as usize + 1;
    let mut z = m * tau(1.0 - dagilim[son] as f64 / m);
    for k in (1..son).rev() {
        z = 0.5 * (z + dagilim[k] as f64);
    }
    z += m * sigma(dagilim[0] as f64 / m);
    m * m / (2.0 * std::f64::consts::LN_2 * z)
}

/// σ(x) = x + Σ_{k≥1} x^(2^k) · 2^(k−1). Hiç dolmamış kovaların düzeltmesi; bütün kovalar boşsa
/// (x = 1) sonsuzdur ve tahmin 0 olur.
fn sigma(mut x: f64) -> f64 {
    if x == 1.0 {
        return f64::INFINITY;
    }
    let mut y = 1.0;
    let mut z = x;
    loop {
        x *= x;
        let onceki = z;
        z += x * y;
        y += y;
        if z == onceki {
            return z;
        }
    }
}

/// τ(x) = (1 − x − Σ_{k≥1} (1 − x^(2^−k))² · 2^−k) / 3. En büyük değere ulaşmış kovaların
/// düzeltmesi; 64 bitlik özetlerle neredeyse hiç gerekmez.
fn tau(mut x: f64) -> f64 {
    if x == 0.0 || x == 1.0 {
        return 0.0;
    }
    let mut y = 1.0;
    let mut z = 1.0 - x;
    loop {
        x = x.sqrt();
        let onceki = z;
        y *= 0.5;
        z -= (1.0 - x).powi(2) * y;
        if z == onceki {
            return z / 3.0;
        }
    }
}

/// Özetleri yeniden özetlemeyen özetleyici: kümedeki değerler zaten iyi karışmış 64 bitlik
/// özetlerdir.
#[derive(Debug, Default, Clone, Copy)]
struct Gecirgen(u64);

impl Hasher for Gecirgen {
    fn write(&mut self, baytlar: &[u8]) {
        for &bayt in baytlar {
            self.0 = self.0.rotate_left(8) ^ u64::from(bayt);
        }
    }

    fn write_u64(&mut self, sayi: u64) {
        self.0 = sayi;
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod testler {
    use super::*;

    fn say(adet: usize) -> FarkliSayaci {
        let mut sayac = FarkliSayaci::default();
        for sira in 0..adet {
            sayac.ekle(&format!("Kullanıcı {sira} giriş yaptı"));
        }
        sayac
    }

    #[test]
    fn sinira_kadar_tam_sayilir() {
        let mut sayac = FarkliSayaci::default();
        assert_eq!(sayac.sayi(), 0);
        for mesaj in ["a", "b", "a", "", "b", ""] {
            sayac.ekle(mesaj);
        }
        assert_eq!(sayac.sayi(), 3);
        assert!(!sayac.yaklasik_mi());

        let sayac = say(TAM_SINIR);
        assert_eq!(sayac.sayi(), TAM_SINIR);
        assert!(!sayac.yaklasik_mi());
    }

    #[test]
    fn siniri_gecince_tahmin_eder() {
        // Standart hata %0,41: üç standart hata (%1,2) içinde kalmalı. Özetler sabit olduğu için
        // sonuç her çalıştırmada aynıdır.
        for adet in [TAM_SINIR + 1, 1_000_000] {
            let sayac = say(adet);
            assert!(sayac.yaklasik_mi());
            let hata = (sayac.sayi() as f64 - adet as f64).abs() / adet as f64;
            assert!(hata < 0.012, "{adet}: {} (hata {hata})", sayac.sayi());
        }
        // Tahmine geçince sayı gerilemez; özetler kovalara kötü dağılsa da (hepsi aynı kovada,
        // tahmin 1 civarı) tam sayımın bıraktığı sayının altına inmez
        assert!(say(TAM_SINIR + 1).sayi() > TAM_SINIR);
        let mut kotu = FarkliSayaci::default();
        for ozet in 0..=TAM_SINIR as u64 {
            kotu.ozet_ekle(ozet);
        }
        assert!(kotu.yaklasik_mi());
        assert_eq!(kotu.sayi(), TAM_SINIR + 1);
        // Tekrarlar sayıyı değiştirmez
        let mut sayac = say(TAM_SINIR + 1);
        let once = sayac.sayi();
        for sira in 0..1_000 {
            sayac.ekle(&format!("Kullanıcı {sira} giriş yaptı"));
        }
        assert_eq!(sayac.sayi(), once);
    }

    #[test]
    fn kova_ve_sira() {
        let mut kovalar = vec![0; KOVA_SAYISI];
        // İlk 16 bit kova; kalanın başında 3 sıfır var: sıra 4
        kovaya_ekle(&mut kovalar, (5 << 48) | (1 << 44));
        assert_eq!(kovalar[5], 4);
        // Daha küçük sıra kovayı değiştirmez
        kovaya_ekle(&mut kovalar, (5 << 48) | (1 << 47));
        assert_eq!(kovalar[5], 4);
        // Kalan bitler hep sıfır: en büyük sıra
        kovaya_ekle(&mut kovalar, 7 << 48);
        assert_eq!(kovalar[7], 49);

        // Hiç değer yoksa 0
        assert_eq!(tahmin(&vec![0; KOVA_SAYISI]), 0.0);
    }

    #[test]
    fn tahmin_her_aralikta_sapmasiz() {
        // Tahminci tek başına: az değerden (çoğu kova boş) çok değere kadar. Küçük sayılarda
        // hata mutlak olarak da küçüktür; büyüklerde üç standart hata (%1,2) içinde kalır.
        for adet in [1_usize, 10, 1_000, 50_000, 300_000, 3_000_000] {
            let mut kovalar = vec![0; KOVA_SAYISI];
            for sira in 0..adet {
                let mut ozetleyici = DefaultHasher::new();
                format!("Sipariş {sira} oluşturuldu").hash(&mut ozetleyici);
                kovaya_ekle(&mut kovalar, ozetleyici.finish());
            }
            let tahmin = tahmin(&kovalar);
            let hata = (tahmin - adet as f64).abs();
            assert!(
                hata <= 1.0 || hata / (adet as f64) < 0.012,
                "{adet}: {tahmin}"
            );
        }
        // Seriler: σ(0) = 0, τ(0) = τ(1) = 0, σ(1) sonsuz. Ara değerler serilerin doğrudan
        // toplamıyla (Python) hesaplandı.
        assert_eq!(sigma(0.0), 0.0);
        assert_eq!((tau(0.0), tau(1.0)), (0.0, 0.0));
        assert_eq!(sigma(1.0), f64::INFINITY);
        let yakin = |a: f64, b: f64| (a - b).abs() < 1e-12;
        assert!(yakin(sigma(0.5), 0.890_747_074_037_790_3));
        assert!(yakin(sigma(0.9), 6.813_691_958_633_901));
        assert!(yakin(tau(0.5), 0.149_929_495_864_088_1));
        assert!(yakin(tau(0.25), 0.199_964_747_932_044));
    }
}
