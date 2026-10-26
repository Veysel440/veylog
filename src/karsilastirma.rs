//! İki zaman aralığının karşılaştırılması: yeni çıkan, kaybolan ve artan kalıplar.
//!
//! Log bir anda (`--karsilastir 15:00`) ikiye bölünür: "önce" ilk kayıttan o ana kadar,
//! "sonra" o andan son kayda kadar. Her kalıp satırının kayıtları iki yarıya sayılır:
//!
//! - yalnızca sonra görülenler: yeni çıkan
//! - yalnızca önce görülenler: kaybolan
//! - ikisinde de görülenler: sonra en az iki kat sık görülüyor, sonrada en az iki kaydı var ve
//!   artış tesadüf değilse artan
//!
//! Sıklık saat başına sayılır, çünkü iki yarı farklı uzunlukta olabilir: 6 saatte 3 kez ile
//! 1 saatte 3 kez aynı şey değildir.
//!
//! Artışın tesadüf olup olmadığı, iki Poisson sıklığını karşılaştıran koşullu binom testiyle
//! (Przyborowski ve Wilenski, 1940) sınanır. Sıklık değişmediyse bir kalıbın n kaydının her
//! biri, "sonra" yarısına o yarının toplam süredeki payı kadar ihtimalle düşer. Sonraya en az
//! gözlenen kadar kaydın düşme ihtimali (p değeri) %1'den küçükse artış tesadüf sayılmaz.

use std::fmt;

use crate::istatistik::binom_ust_kuyruk;
use crate::kayit::Log;
use crate::rapor::KalipAnalizi;
use crate::zaman::{Zaman, ZamanNoktasi};

/// Bir kalıbın "artan" sayılması için sonraki saatlik sıklığın öncekine en az oranı.
pub const ARTIS_KATI: f64 = 2.0;
/// Artışın tesadüf olma ihtimali (p değeri) bundan küçük olmalı: %1.
pub const ANLAMLILIK: f64 = 0.01;
/// "Artan" sayılmak için sonra yarısında en az bu kadar kayıt olmalı. Tek bir kayıttan sıklık
/// çıkarılamaz: sonra yarısı kısaysa (nokta logun sonuna yakınsa) oraya düşen tek bir kayıt
/// bile yüzlerce kat artış gibi görünür. Son kayıt ise pencerenin sonunu kendisi belirlediği
/// için sonraya her zaman düşer.
pub const EN_AZ_KAYIT: usize = 2;

/// Logun iki yarısından biri.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pencere {
    pub baslangic: Zaman,
    pub bitis: Zaman,
    /// Yarının süresi, saniye.
    pub saniye: usize,
    pub kayit_sayisi: usize,
}

impl Pencere {
    /// `adet` kaydın bu yarıda saatte kaç kez görüldüğü.
    pub fn saatlik(&self, adet: usize) -> f64 {
        adet as f64 * 3_600.0 / self.saniye as f64
    }
}

/// Bir kalıp satırının iki yarı arasındaki değişimi.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Degisim {
    /// Satırın `KalipAnalizi::satirlar` içindeki sırası.
    pub satir: usize,
    pub once: usize,
    pub sonra: usize,
    /// Sonraki saatlik sıklığın öncekine oranı. Önce hiç görülmediyse sonsuz, sonra hiç
    /// görülmediyse 0.
    pub kat: f64,
    /// Sıklık değişmeseydi en az bu kadar büyük bir değişimi tesadüfen görme ihtimali.
    pub p_degeri: f64,
}

/// Karşılaştırmanın sonucu. Listeler kalıp satırlarının sırasını (`--siralama`) korur.
#[derive(Debug)]
pub struct Karsilastirma {
    /// Logun ikiye bölündüğü an.
    pub nokta: Zaman,
    pub once: Pencere,
    pub sonra: Pencere,
    pub yeni: Vec<Degisim>,
    pub kaybolan: Vec<Degisim>,
    pub artan: Vec<Degisim>,
}

/// Karşılaştırma yapılamadı.
#[derive(Debug, PartialEq)]
pub enum KarsilastirmaHatasi {
    /// Logda hiç kayıt yok.
    KayitYok,
    /// Nokta ilk kayıttan sonra ve en geç son kayıtla aynı anda değil: yarılardan biri boş
    /// kalırdı.
    AralikDisi {
        nokta: Zaman,
        ilk: Zaman,
        son: Zaman,
    },
}

impl fmt::Display for KarsilastirmaHatasi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KarsilastirmaHatasi::KayitYok => write!(f, "karşılaştırma için logda kayıt yok"),
            KarsilastirmaHatasi::AralikDisi { nokta, ilk, son } => write!(
                f,
                "karşılaştırma noktası {nokta} logun içinde değil: kayıtlar {ilk} ile {son} arasında"
            ),
        }
    }
}

impl std::error::Error for KarsilastirmaHatasi {}

impl Karsilastirma {
    /// Logu `nokta`da ikiye bölüp kalıp satırlarını karşılaştırır.
    pub fn hesapla(
        log: &Log<'_>,
        analiz: &KalipAnalizi<'_>,
        nokta: ZamanNoktasi,
    ) -> Result<Karsilastirma, KarsilastirmaHatasi> {
        let zamanlar: Vec<Zaman> = log
            .kayitlar
            .iter()
            .map(|kayit| Zaman::kayittan(kayit.zaman()))
            .collect();
        // let-else: desen tutmazsa `else` bloğu çalışır ve fonksiyondan çıkar
        let (Some(&ilk), Some(&son)) = (zamanlar.iter().min(), zamanlar.iter().max()) else {
            return Err(KarsilastirmaHatasi::KayitYok);
        };
        let nokta = nokta.coz(son);
        if nokta <= ilk || nokta > son {
            return Err(KarsilastirmaHatasi::AralikDisi { nokta, ilk, son });
        }

        // Tam noktadaki kayıt "sonra"dadır: önce [ilk, nokta), sonra [nokta, son]
        let sonrada = |zaman: Zaman| zaman >= nokta;
        let sonra_kayit = zamanlar.iter().filter(|&&zaman| sonrada(zaman)).count();
        // Son kaydın saniyesi de sonra yarısına dahil olduğu için bir saniye eklenir
        let once = Pencere {
            baslangic: ilk,
            bitis: nokta,
            saniye: saniye_farki(ilk, nokta),
            kayit_sayisi: zamanlar.len() - sonra_kayit,
        };
        let sonra = Pencere {
            baslangic: nokta,
            bitis: son,
            saniye: saniye_farki(nokta, son) + 1,
            kayit_sayisi: sonra_kayit,
        };
        // Sıklık değişmediyse bir kaydın sonraya düşme ihtimali: sonranın süredeki payı
        let sonra_payi = sonra.saniye as f64 / (once.saniye + sonra.saniye) as f64;

        let mut sonuc = Karsilastirma {
            nokta,
            once,
            sonra,
            yeni: Vec::new(),
            kaybolan: Vec::new(),
            artan: Vec::new(),
        };
        for (sira, satir) in analiz.satirlar.iter().enumerate() {
            let sonra_adet = satir
                .kayitlar
                .iter()
                .filter(|&&kayit| sonrada(zamanlar[kayit]))
                .count();
            let once_adet = satir.kayitlar.len() - sonra_adet;
            // Önce 0 ise sonsuz, sonra 0 ise 0
            let kat = sonra.saatlik(sonra_adet) / once.saatlik(once_adet);
            // Kapanış (closure): satırın bilgilerini yakalar, yalnızca p değerini bekler
            let degisim = |p_degeri| Degisim {
                satir: sira,
                once: once_adet,
                sonra: sonra_adet,
                kat,
                p_degeri,
            };
            if once_adet == 0 {
                // Kayıtların hepsinin tesadüfen sonraya düşme ihtimali
                let p_degeri = binom_ust_kuyruk(sonra_adet, sonra_adet, sonra_payi);
                sonuc.yeni.push(degisim(p_degeri));
            } else if sonra_adet == 0 {
                // Hepsinin tesadüfen önceye düşme ihtimali: önceye düşme ihtimali 1 − pay
                let p_degeri = binom_ust_kuyruk(once_adet, once_adet, 1.0 - sonra_payi);
                sonuc.kaybolan.push(degisim(p_degeri));
            } else if sonra_adet >= EN_AZ_KAYIT && kat >= ARTIS_KATI {
                let p_degeri = binom_ust_kuyruk(once_adet + sonra_adet, sonra_adet, sonra_payi);
                if p_degeri < ANLAMLILIK {
                    sonuc.artan.push(degisim(p_degeri));
                }
            }
        }
        Ok(sonuc)
    }
}

/// `bas`'tan `son`'a kadar geçen saniye.
///
/// Kayıt zamanları ve karşılaştırma noktası takvimde var olan anlardır (`kayit` ve `zaman`
/// modülleri denetler); böyle anlarda sıralama ile saniye uyuşur. Bu yüzden `son`, `bas`'tan önce
/// değilse fark eksi olamaz. `as usize` eksi bir sayıyı sessizce dev bir sayıya çevirirdi;
/// `try_from` çevirmez ve beklenti bozulursa ne olduğunu söyleyerek durur.
fn saniye_farki(bas: Zaman, son: Zaman) -> usize {
    usize::try_from(son.saniye() - bas.saniye()).expect("bitiş başlangıçtan önce olamaz")
}

#[cfg(test)]
mod testler {
    use super::*;
    use crate::kayit::kayitlari_ayir;
    use crate::rapor::{RaporAyarlari, kaliplari_cikar};

    /// Saat ve mesajdan bir log metni: her eleman (zaman, seviye, mesaj, kaç kez, aralık dk).
    /// Aynı mesaj `kac` kez, `aralik` dakika arayla yazılır.
    fn log_metni(kayitlar: &[(&str, &str, &str, u32, u32)]) -> String {
        let mut satirlar = Vec::new();
        for &(baslangic, seviye, mesaj, kac, aralik) in kayitlar {
            let zaman = Zaman::kayittan(&format!("2026-10-24 {baslangic}"));
            for tekrar in 0..kac {
                let dakika_sonra = zaman.saniye() + i64::from(tekrar * aralik * 60);
                satirlar.push(format!(
                    "[{}] local.{seviye}: {mesaj}",
                    Zaman::saniyeden(dakika_sonra)
                ));
            }
        }
        satirlar.join("\n")
    }

    /// İki yarı tam 2'şer saat: önce 10:00:00–12:00:00, sonra 12:00:00–13:59:59. Bir kaydın
    /// sonraya düşme ihtimali 0,5; p değerleri elle hesaplanabilir.
    fn ornek() -> String {
        log_metni(&[
            // Önce
            ("10:00:00", "INFO", "Kuyruk dolu", 2, 40),
            ("10:10:00", "ERROR", "Eski hata", 2, 40),
            ("10:20:00", "WARNING", "Disk doluyor", 1, 0),
            ("10:30:00", "WARNING", "Ağır sorgu", 2, 30),
            // Sonra: tam 12:00:00'daki kayıt sonraya sayılır
            ("12:00:00", "ERROR", "Yeni hata", 3, 20),
            ("12:05:00", "WARNING", "Disk doluyor", 10, 5),
            ("12:30:00", "WARNING", "Ağır sorgu", 5, 10),
            ("13:00:00", "INFO", "Kuyruk dolu", 2, 10),
            ("13:59:59", "INFO", "Kuyruk dolu", 1, 0),
        ])
    }

    fn karsilastir(metin: &str, nokta: &str) -> Result<Karsilastirma, KarsilastirmaHatasi> {
        let log = kayitlari_ayir(metin);
        let analiz = kaliplari_cikar(&log, &RaporAyarlari::default());
        let nokta = ZamanNoktasi::ayristir(nokta).expect("geçerli nokta");
        Karsilastirma::hesapla(&log, &analiz, nokta)
    }

    /// Kayan noktalı sayılar son basamakta yuvarlanabilir: yakınlıkla karşılaştır.
    fn yakin(gelen: f64, beklenen: f64) -> bool {
        (gelen - beklenen).abs() < 1e-12
    }

    /// Bir listedeki satırların kalıpları ve sayıları.
    fn ozetle(metin: &str, liste: &[Degisim]) -> Vec<(String, usize, usize)> {
        let log = kayitlari_ayir(metin);
        let analiz = kaliplari_cikar(&log, &RaporAyarlari::default());
        liste
            .iter()
            .map(|degisim| {
                (
                    analiz.satirlar[degisim.satir].kalip.clone(),
                    degisim.once,
                    degisim.sonra,
                )
            })
            .collect()
    }

    #[test]
    fn yarilar_ve_sureleri() {
        let sonuc = karsilastir(&ornek(), "12:00").unwrap();
        assert_eq!(sonuc.nokta.to_string(), "2026-10-24 12:00:00");
        assert_eq!(sonuc.once.baslangic.to_string(), "2026-10-24 10:00:00");
        assert_eq!(sonuc.once.bitis.to_string(), "2026-10-24 12:00:00");
        assert_eq!(sonuc.sonra.bitis.to_string(), "2026-10-24 13:59:59");
        assert_eq!((sonuc.once.saniye, sonuc.sonra.saniye), (7_200, 7_200));
        assert_eq!((sonuc.once.kayit_sayisi, sonuc.sonra.kayit_sayisi), (7, 21));
        assert_eq!(sonuc.sonra.saatlik(21), 10.5);
    }

    #[test]
    fn yeni_kaybolan_ve_artan_ayrilir() {
        let metin = ornek();
        let sonuc = karsilastir(&metin, "12:00").unwrap();

        assert_eq!(
            ozetle(&metin, &sonuc.yeni),
            [("Yeni hata".to_string(), 0, 3)]
        );
        assert_eq!(
            ozetle(&metin, &sonuc.kaybolan),
            [("Eski hata".to_string(), 2, 0)]
        );
        // Disk doluyor 1 → 10: 10 kat, tesadüf ihtimali (11 + 1)/2¹¹ ≈ %0,59
        assert_eq!(
            ozetle(&metin, &sonuc.artan),
            [("Disk doluyor".to_string(), 1, 10)]
        );
        // Ağır sorgu 2 → 5 de 2,5 kat ama tesadüf olabilir (29/128 ≈ %23); Kuyruk dolu 2 → 3
        // ise yalnızca 1,5 kat. İkisi de hiçbir listede yok.
    }

    #[test]
    fn p_degerleri_ve_katlar() {
        let sonuc = karsilastir(&ornek(), "12:00").unwrap();
        // Yeni: 3 kaydın hepsinin sonraya düşme ihtimali 0,5³
        assert!(yakin(sonuc.yeni[0].p_degeri, 0.125));
        assert_eq!(sonuc.yeni[0].kat, f64::INFINITY);
        // Kaybolan: 2 kaydın hepsinin önceye düşme ihtimali 0,5²
        assert!(yakin(sonuc.kaybolan[0].p_degeri, 0.25));
        assert_eq!(sonuc.kaybolan[0].kat, 0.0);
        // Artan: P(X ≥ 10), X ~ Binom(11; 0,5) = 12/2048
        assert!(yakin(sonuc.artan[0].p_degeri, 12.0 / 2048.0));
        assert_eq!(sonuc.artan[0].kat, 10.0);
    }

    #[test]
    fn yarilarin_uzunlugu_farkliysa_sure_payi_kullanilir() {
        // Önce 3 saat (09:00–12:00), sonra 1 saat (12:00–12:59:59). Sıklık değişmediyse bir
        // kayıt sonraya 1/4, önceye 3/4 ihtimalle düşer.
        let metin = log_metni(&[
            ("09:00:00", "ERROR", "Eski hata", 2, 60),
            ("11:00:00", "WARNING", "Disk doluyor", 1, 0),
            ("12:00:00", "WARNING", "Disk doluyor", 6, 10),
            ("12:30:00", "ERROR", "Yeni hata", 1, 0),
            ("12:59:59", "ERROR", "Yeni hata", 1, 0),
        ]);
        let sonuc = karsilastir(&metin, "12:00").unwrap();
        assert_eq!((sonuc.once.saniye, sonuc.sonra.saniye), (10_800, 3_600));

        // Yeni: iki kaydın ikisi de sonraya düştü: (1/4)²
        assert_eq!(sonuc.yeni[0].sonra, 2);
        assert!(yakin(sonuc.yeni[0].p_degeri, 0.0625));
        // Kaybolan: ikisi de önceye düştü: (3/4)²
        assert_eq!(sonuc.kaybolan[0].once, 2);
        assert!(yakin(sonuc.kaybolan[0].p_degeri, 0.5625));
        // Artan: saatte 1/3'ten 6'ya, 18 kat. P(X ≥ 6), X ~ Binom(7; 1/4) = 22/16384
        assert_eq!((sonuc.artan[0].once, sonuc.artan[0].sonra), (1, 6));
        assert!(yakin(sonuc.artan[0].kat, 18.0));
        assert!(yakin(sonuc.artan[0].p_degeri, 22.0 / 16_384.0));
    }

    #[test]
    fn artan_icin_iki_kat_ve_anlamlilik_birlikte_gerekir() {
        // Eşit yarılar: 10:00–12:00 ve 12:00–13:59:59
        let metin = log_metni(&[
            ("10:00:00", "ERROR", "Sınırda hata", 20, 6),
            ("10:00:00", "WARNING", "Yavaş istek", 30, 4),
            ("12:00:00", "ERROR", "Sınırda hata", 40, 3),
            ("12:00:00", "WARNING", "Yavaş istek", 55, 2),
            ("13:59:59", "INFO", "Son kayıt", 1, 0),
        ]);
        let sonuc = karsilastir(&metin, "12:00").unwrap();
        // 20 → 40: tam 2 kat, tesadüf ihtimali ≈ %0,67. "En az 2 kat" olduğu için artan.
        // 30 → 55: tesadüf ihtimali ≈ %0,44, yani tesadüf değil; ama 1,8 kat: artan değil.
        assert_eq!(
            ozetle(&metin, &sonuc.artan),
            [("Sınırda hata".to_string(), 20, 40)]
        );
        assert_eq!(sonuc.artan[0].kat, 2.0);
        assert!(sonuc.artan[0].p_degeri < ANLAMLILIK);
    }

    #[test]
    fn tek_kayittan_artis_cikarilmaz() {
        // Son dakika karşılaştırılıyor: önce 10:00–13:59 (14 340 sn), sonra 13:59:00–13:59:59
        // (60 sn). Bir kaydın sonraya düşme ihtimali 1/240.
        let metin = log_metni(&[
            ("10:00:00", "INFO", "Gece görevi", 1, 0),
            ("10:00:00", "WARNING", "Disk doluyor", 1, 0),
            ("13:59:30", "WARNING", "Disk doluyor", 2, 0),
            ("13:59:59", "INFO", "Gece görevi", 1, 0),
        ]);
        let sonuc = karsilastir(&metin, "13:59").unwrap();
        // Gece görevi 1 → 1: saatte 0,25'ten 60'a, 239 kat; tesadüf ihtimali 1 − (239/240)²
        // ≈ %0,83. Ama sonrada tek kayıt var, üstelik pencerenin sonunu o kayıt belirliyor.
        // Disk doluyor 1 → 2: iki kayıt, tesadüf ihtimali ≈ %0,005: artan.
        assert_eq!(
            ozetle(&metin, &sonuc.artan),
            [("Disk doluyor".to_string(), 1, 2)]
        );
    }

    #[test]
    fn sirasiz_kayitlarda_ilk_ve_son_zamanla_bolunur() {
        // Aynı anda yazan birden fazla işlem yüzünden kayıtlar dosyada sırasız olabilir: ilk
        // satır en eski, son satır en yeni kayıt olmayabilir
        let metin = "[2026-10-24 10:05:00] local.INFO: Sonra yazıldı\n\
                     [2026-10-24 10:00:00] local.INFO: Önce yazıldı\n\
                     [2026-10-24 10:10:00] local.INFO: Sonra yazıldı\n\
                     [2026-10-24 10:02:00] local.INFO: Önce yazıldı";
        let sonuc = karsilastir(metin, "10:03").unwrap();
        assert_eq!(sonuc.once.baslangic.to_string(), "2026-10-24 10:00:00");
        assert_eq!(sonuc.sonra.bitis.to_string(), "2026-10-24 10:10:00");
        assert_eq!(
            ozetle(metin, &sonuc.yeni),
            [("Sonra yazıldı".to_string(), 0, 2)]
        );
        assert_eq!(
            ozetle(metin, &sonuc.kaybolan),
            [("Önce yazıldı".to_string(), 2, 0)]
        );
    }

    #[test]
    fn nokta_logun_icinde_olmali() {
        let metin = ornek();
        // İlk kayıtla aynı an: önce yarısı boş kalırdı
        assert_eq!(
            karsilastir(&metin, "10:00").unwrap_err().to_string(),
            "karşılaştırma noktası 2026-10-24 10:00:00 logun içinde değil: kayıtlar 2026-10-24 10:00:00 ile 2026-10-24 13:59:59 arasında"
        );
        assert!(karsilastir(&metin, "14:00").is_err());
        assert!(karsilastir(&metin, "2026-10-23 12:00").is_err());
        // Son kayıtla aynı an geçerli: sonra yarısında yalnızca o kayıt var
        let sonuc = karsilastir(&metin, "13:59:59").unwrap();
        assert_eq!((sonuc.sonra.kayit_sayisi, sonuc.sonra.saniye), (1, 1));
        assert_eq!(
            karsilastir("", "12:00").unwrap_err(),
            KarsilastirmaHatasi::KayitYok
        );
    }

    #[test]
    fn takvimde_olmayan_zaman_sureleri_bozmaz() {
        // 30 Şubat sıralamada 1 Mart'tan önce gelir ama saniyesi 2 Mart'ınkidir. Başlık
        // sayılsaydı "önce" yarısı ilk kayıttan geriye giden eksi bir süre olurdu. Takvimde
        // olmadığı için başlık değildir: ilk kaydın önündeki yetim satır olur.
        let metin = "[2026-02-30 10:00:00] local.ERROR: Eski hata\n\
                     [2026-03-01 12:00:00] local.ERROR: Yeni hata\n\
                     [2026-03-03 12:00:00] local.ERROR: Yeni hata";
        let log = kayitlari_ayir(metin);
        assert_eq!((log.kayitlar.len(), log.yetim_satir), (2, 1));
        let sonuc = karsilastir(metin, "2026-03-02").unwrap();
        assert_eq!((sonuc.once.saniye, sonuc.sonra.saniye), (43_200, 129_601));
        assert_eq!(saniye_farki(sonuc.once.baslangic, sonuc.nokta), 43_200);
    }

    #[test]
    #[should_panic(expected = "bitiş başlangıçtan önce olamaz")]
    fn ters_aralik_sessizce_dev_sayiya_donmez() {
        let zaman = Zaman::kayittan;
        saniye_farki(zaman("2026-03-02 00:00:00"), zaman("2026-03-01 00:00:00"));
    }

    #[test]
    fn yalnizca_saat_logun_son_gunune_uygulanir() {
        let metin = "[2026-10-24 23:00:00] local.ERROR: Gece hatası\n\
                     [2026-10-25 01:00:00] local.ERROR: Sabah hatası";
        // 00:30 son kaydın günü olan 25 Ekim'e uygulanır: iki kayıt iki yarıya düşer
        let sonuc = karsilastir(metin, "00:30").unwrap();
        assert_eq!(sonuc.nokta.to_string(), "2026-10-25 00:30:00");
        assert_eq!((sonuc.once.kayit_sayisi, sonuc.sonra.kayit_sayisi), (1, 1));
        // 23:30 da 25 Ekim'e uygulanır ve son kayıttan sonraya düşer
        assert!(karsilastir(metin, "23:30").is_err());
    }
}
