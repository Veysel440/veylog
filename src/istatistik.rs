//! İstatistik: iki zaman aralığı arasındaki bir değişimin tesadüf olup olmadığını sınar.

/// Binom dağılımının üst kuyruğu: P(X ≥ k), X ~ Binom(n, p).
///
/// Her biri p olasılıkla "başarılı" olan n bağımsız denemede en az k başarı görme ihtimali.
/// Karşılaştırmada bir kalıbın n kaydının her biri, sıklık değişmediyse "sonra" yarısına
/// o yarının süredeki payı (p) kadar ihtimalle düşer; en az k tanesinin sonraya düşme ihtimali
/// küçükse artış tesadüf değildir.
///
/// Terimler tek tek toplanır: P(X = i + 1) = P(X = i) · (n − i)/(i + 1) · p/(1 − p). Çok
/// küçük olasılıklar (10⁻³⁰⁰ altı) `f64`'te sıfıra yuvarlanacağı için terimlerin
/// logaritmaları tutulur ve en büyük terime göre toplanır.
pub fn binom_ust_kuyruk(n: usize, k: usize, p: f64) -> f64 {
    if k == 0 {
        return 1.0;
    }
    if k > n || p <= 0.0 {
        return 0.0;
    }
    if p >= 1.0 {
        return 1.0;
    }
    let (ln_p, ln_q) = (p.ln(), (1.0 - p).ln());
    // ln P(X = k) = ln C(n, k) + k·ln p + (n − k)·ln(1 − p)
    let mut ln_terim = ln_kombinasyon(n, k) + k as f64 * ln_p + (n - k) as f64 * ln_q;
    // Toplam, şimdiye kadarki en büyük terimin katı olarak tutulur:
    // toplam = (terimlerin toplamı) / exp(en_buyuk)
    let mut en_buyuk = ln_terim;
    let mut toplam = 1.0;
    for i in k..n {
        ln_terim += ((n - i) as f64 / (i + 1) as f64).ln() + ln_p - ln_q;
        if ln_terim > en_buyuk {
            // Yeni en büyük terim: eski toplamı yeni ölçeğe çevir
            toplam = toplam * (en_buyuk - ln_terim).exp() + 1.0;
            en_buyuk = ln_terim;
        } else {
            let goreli = (ln_terim - en_buyuk).exp();
            toplam += goreli;
            // Tepe noktasından sonra terimler hep küçülür; bu kadar küçük bir terim ve
            // ardındakiler toplamı artık değiştirmez
            if goreli < 1e-17 {
                break;
            }
        }
    }
    (en_buyuk + toplam.ln()).exp().min(1.0)
}

/// ln C(n, k): n şeyden k tanesini seçmenin yol sayısının doğal logaritması.
///
/// C(n, k) = C(n, n − k) olduğu için kısa olan taraf toplanır.
fn ln_kombinasyon(n: usize, k: usize) -> f64 {
    let k = k.min(n - k);
    (1..=k).map(|i| ((n - k + i) as f64 / i as f64).ln()).sum()
}

#[cfg(test)]
mod testler {
    use super::*;

    /// Göreli hata 1e-9'dan küçükse eşit say.
    fn yakin(gelen: f64, beklenen: f64) -> bool {
        (gelen - beklenen).abs() <= 1e-9 * beklenen.abs().max(f64::MIN_POSITIVE)
    }

    #[test]
    fn kombinasyon_dogru() {
        assert!(yakin(ln_kombinasyon(6, 5).exp(), 6.0));
        assert!(yakin(ln_kombinasyon(10, 3).exp(), 120.0));
        assert!(yakin(ln_kombinasyon(52, 5).exp(), 2_598_960.0));
        assert_eq!(ln_kombinasyon(7, 0), 0.0);
        assert_eq!(ln_kombinasyon(7, 7), 0.0);
    }

    #[test]
    fn sinir_durumlari() {
        assert_eq!(binom_ust_kuyruk(10, 0, 0.3), 1.0);
        assert_eq!(binom_ust_kuyruk(10, 11, 0.3), 0.0);
        assert_eq!(binom_ust_kuyruk(10, 1, 0.0), 0.0);
        assert_eq!(binom_ust_kuyruk(10, 10, 1.0), 1.0);
        // Hepsi: P(X = n) = pⁿ
        assert!(yakin(binom_ust_kuyruk(4, 4, 0.5), 0.0625));
    }

    #[test]
    fn scipy_ile_ayni_sonuc() {
        // Beklenenler: scipy.stats.binom.sf(k - 1, n, p)
        for (n, k, p, beklenen) in [
            (6, 5, 0.2453, 0.00423959789579656),
            (10, 3, 0.3, 0.6172172136000003),
            (33, 3, 0.0217, 0.03435805459866819),
            (100, 60, 0.5, 0.028443966820490444),
            (1_000, 10, 0.001, 1.074283386846483e-7),
            // Beklenenin biraz altında: terimler önce büyür, tepe noktasından sonra küçülür.
            // Toplamın her yeni en büyük terime göre yeniden ölçeklenmesi burada sınanır.
            (100, 45, 0.5, 0.8643734879630823),
            (10_000, 8_950, 0.9, 0.9530727017180728),
            // Beklenenin çok altında: kuyruk neredeyse 1. Baştaki terim f64'te sıfıra
            // yuvarlanacak kadar küçük; logaritmayla toplamak burada önemli.
            (10_000, 10, 0.9, 1.0),
            // Beklenenin çok üstünde: çok küçük ama sıfır değil
            (10_000, 9_500, 0.9, 9.661759316586807e-75),
        ] {
            let gelen = binom_ust_kuyruk(n, k, p);
            assert!(
                yakin(gelen, beklenen),
                "n={n} k={k} p={p}: {gelen} ≠ {beklenen}"
            );
        }
    }
}
