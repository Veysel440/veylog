//! İstatistik: iki zaman aralığı arasındaki bir değişimin tesadüf olup olmadığını sınar; bir
//! sayı dizisinin ortasını ve yayılımını aşırı değerlerden etkilenmeden ölçer.

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

/// Bir sayı dizisinin ortası ve yayılımı.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MedyanMad {
    /// Sıralanmış dizinin ortasındaki eleman; eleman sayısı çiftse ortadaki ikisinin ortalaması.
    pub medyan: f64,
    /// Elemanların medyandan mutlak sapmalarının medyanı (median absolute deviation).
    pub mad: f64,
}

/// Seyrek bir dizinin medyanı ve MAD'ı: dizide `sifir_sayisi` tane 0 ve `sayilar` var. Dizi
/// boşsa None: boş dizinin ortası yoktur.
///
/// Ortalama ve standart sapma tek bir aşırı değerle kayar; medyan ve MAD ise elemanların
/// yarısına kadarı aşırı olsa bile yerinde kalır. Ani artışları ararken gereken budur: aranan
/// patlama, olağan seviyenin ölçüsünü bozmamalı.
///
/// Bir kalıbın dakikalık sayılarının çoğu 0'dır; ayda birkaç kez görülen bir hata için 43 200
/// dakikanın neredeyse hepsi. Sıfırlar tek tek tutulmaz, yalnızca sayılır; bellek, sıfır olmayan
/// eleman sayısı kadardır. Sıralı dizinin ortadaki elemanları `sirada` ile bulunur.
pub fn medyan_ve_mad(sifir_sayisi: usize, sayilar: &[usize]) -> Option<MedyanMad> {
    if sifir_sayisi + sayilar.len() == 0 {
        return None;
    }
    let mut degerler: Vec<f64> = sayilar.iter().map(|&sayi| sayi as f64).collect();
    // f64 `Ord` değildir (NaN hiçbir sayıyla karşılaştırılamaz), bu yüzden `sort` çalışmaz.
    // `total_cmp` NaN'ı da içeren tam bir sıralama tanımlar.
    degerler.sort_unstable_by(f64::total_cmp);
    let medyan = orta(&degerler, 0.0, sifir_sayisi);
    // Her sıfırın medyandan sapması medyanın kendisidir: |0 − medyan|
    let mut sapmalar: Vec<f64> = degerler
        .iter()
        .map(|deger| (deger - medyan).abs())
        .collect();
    sapmalar.sort_unstable_by(f64::total_cmp);
    let mad = orta(&sapmalar, medyan, sifir_sayisi);
    Some(MedyanMad { medyan, mad })
}

/// Sıralı `sirali` dizisine `tekrar` kez `deger` eklenseydi oluşacak dizinin medyanı. Dizi boş
/// olmamalı.
fn orta(sirali: &[f64], deger: f64, tekrar: usize) -> f64 {
    let n = sirali.len() + tekrar;
    // Eleman sayısı tekse iki konum aynıdır: tam ortadaki eleman
    let alt = sirada(sirali, deger, tekrar, (n - 1) / 2);
    let ust = sirada(sirali, deger, tekrar, n / 2);
    // `midpoint`: iki sayının tam ortası, (alt + ust) / 2. Toplam çok büyük sayılarda bile taşmaz.
    alt.midpoint(ust)
}

/// Sıralı `sirali` dizisine `tekrar` kez `deger` eklenseydi, oluşacak sıralı dizinin `k`.
/// elemanı (0'dan sayarak).
///
/// Eklenen değerler, `sirali`da kendilerinden küçük olan elemanların hemen arkasına dizilirdi.
/// Kaç elemanın küçük olduğunu `partition_point` ikili aramayla bulur: dizi sıralı olduğu için
/// şartı sağlayanlar hep baştadır.
///
/// Genel (generic) bir fonksiyondur: `T`, kopyalanabilen ve sıralanabilen her tür olabilir.
/// Medyan için f64 ile, testlerde okunması kolay olsun diye tamsayılarla kullanılır.
fn sirada<T: Copy + PartialOrd>(sirali: &[T], deger: T, tekrar: usize, k: usize) -> T {
    let kucukler = sirali.partition_point(|&eleman| eleman < deger);
    if k < kucukler {
        sirali[k]
    } else if k < kucukler + tekrar {
        deger
    } else {
        sirali[k - tekrar]
    }
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

    #[test]
    fn sirada_eklenen_degerleri_yerine_koyar() {
        // [1, 3, 5] dizisine iki tane 2 eklenseydi: [1, 2, 2, 3, 5]
        let gelen: Vec<i32> = (0..5).map(|k| sirada(&[1, 3, 5], 2, 2, k)).collect();
        assert_eq!(gelen, [1, 2, 2, 3, 5]);
        // En küçükten küçük, en büyükten büyük ve dizideki bir elemana eşit değerler
        let gelen: Vec<i32> = (0..4).map(|k| sirada(&[1, 3], 0, 2, k)).collect();
        assert_eq!(gelen, [0, 0, 1, 3]);
        let gelen: Vec<i32> = (0..4).map(|k| sirada(&[1, 3], 9, 2, k)).collect();
        assert_eq!(gelen, [1, 3, 9, 9]);
        let gelen: Vec<i32> = (0..4).map(|k| sirada(&[1, 3, 3], 3, 1, k)).collect();
        assert_eq!(gelen, [1, 3, 3, 3]);
        // Hiç eleman eklenmezse dizinin kendisi
        let gelen: Vec<i32> = (0..3).map(|k| sirada(&[4, 5, 6], 1, 0, k)).collect();
        assert_eq!(gelen, [4, 5, 6]);
    }

    #[test]
    fn medyan_ve_mad_numpy_ile_ayni() {
        // Beklenenler: numpy.median ve scipy.stats.median_abs_deviation(scale=1), dizinin
        // sıfırları açıkça yazılmış haliyle
        for (sifir_sayisi, sayilar, medyan, mad) in [
            // Wikipedia'daki örnek: (1, 1, 2, 2, 4, 6, 9)
            (0, &[1, 1, 2, 2, 4, 6, 9][..], 2.0, 1.0),
            // Sıfırlar ortaya kadar uzanıyor: medyan ve MAD sıfırlardan gelir
            (7, &[3, 1, 4, 1, 5, 9, 2, 6], 1.0, 1.0),
            // Çift sayıda eleman: medyan ortadaki iki elemanın (1 ve 5) ortalaması
            (2, &[5, 5, 5, 1], 3.0, 2.0),
            (3, &[2, 8, 4], 1.0, 1.0),
            (0, &[10, 20, 30, 40], 25.0, 10.0),
            // Seyrek bir kalıbın 574 dakikası: 570'i boş
            (570, &[6, 1, 1, 2], 0.0, 0.0),
            (5, &[], 0.0, 0.0),
            (0, &[7], 7.0, 0.0),
            // Yarım: [0, 3]
            (1, &[3], 1.5, 1.5),
        ] {
            let gelen = medyan_ve_mad(sifir_sayisi, sayilar).unwrap();
            assert_eq!(
                gelen,
                MedyanMad { medyan, mad },
                "{sifir_sayisi} sıfır ve {sayilar:?}"
            );
        }
        assert_eq!(medyan_ve_mad(0, &[]), None);
    }

    /// Sıfırları da tek tek yazıp sıralayan, kitaptaki gibi doğrudan hesap.
    fn dogrudan(sifir_sayisi: usize, sayilar: &[usize]) -> MedyanMad {
        fn medyan_bul(dizi: &mut [f64]) -> f64 {
            dizi.sort_by(f64::total_cmp);
            let n = dizi.len();
            (dizi[(n - 1) / 2] + dizi[n / 2]) / 2.0
        }
        let mut dizi: Vec<f64> = vec![0.0; sifir_sayisi];
        dizi.extend(sayilar.iter().map(|&sayi| sayi as f64));
        let medyan = medyan_bul(&mut dizi);
        let mut sapmalar: Vec<f64> = dizi.iter().map(|deger| (deger - medyan).abs()).collect();
        MedyanMad {
            medyan,
            mad: medyan_bul(&mut sapmalar),
        }
    }

    #[test]
    fn seyrek_hesap_dogrudan_hesapla_ayni() {
        // Sözde rastgele (her çalışmada aynı) 2 000 dizi: farklı uzunluklar, sıfır oranları ve
        // tekrar eden değerler. Basit bir doğrusal eşlik üreteci (LCG) yeter.
        let mut tohum: u64 = 2026;
        let mut sonraki = |ust: u64| {
            tohum = tohum
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (tohum >> 33) % ust
        };
        for _ in 0..2_000 {
            let sifir_sayisi = sonraki(12) as usize;
            let uzunluk = sonraki(12) as usize;
            let en_buyuk = 1 + sonraki(20);
            let sayilar: Vec<usize> = (0..uzunluk)
                .map(|_| 1 + sonraki(en_buyuk) as usize)
                .collect();
            if sifir_sayisi + uzunluk == 0 {
                continue;
            }
            assert_eq!(
                medyan_ve_mad(sifir_sayisi, &sayilar).unwrap(),
                dogrudan(sifir_sayisi, &sayilar),
                "{sifir_sayisi} sıfır ve {sayilar:?}"
            );
        }
    }
}
