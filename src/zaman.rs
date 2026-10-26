//! Zaman: kayıt zamanlarını ve `--karsilastir` değerini karşılaştırılabilir hale getirir,
//! iki an arasındaki süreyi hesaplar.
//!
//! Kayıt zamanları metin olarak tutulur (`2026-10-24 14:03:11`). İki an arasında kaç saniye
//! olduğunu bulmak için tarih bir sayıya çevrilir: 1970-01-01'den bu yana geçen gün. Saat
//! dilimi ve saniyenin kesri dikkate alınmaz; zamanın ilk 19 karakteri kullanılır.

use std::fmt;
use std::ops::RangeInclusive;

/// Bir an: tarih ve saniye hassasiyetinde saat.
///
/// `Ord` türetilince alanlar yazıldıkları sırayla karşılaştırılır: önce yıl, eşitse ay,
/// eşitse gün... Bu, iki anın kronolojik karşılaştırmasıdır; alanların sırası bu yüzden
/// önemlidir. Alanlar `u16`: en büyüğü olan yıl bile 9999'u geçmez; bir an 12 bayttır.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Zaman {
    yil: u16,
    ay: u16,
    gun: u16,
    saat: u16,
    dakika: u16,
    saniye: u16,
}

impl Zaman {
    /// Bir kaydın zamanından: `2026-10-24 14:03:11` ya da `2026-10-24T14:03:11.123456+03:00`.
    ///
    /// Kayıt zamanının biçimi kayıt ayrıştırılırken denetlendiği için (`kayit` modülü) en az
    /// 19 karakter olduğu ve parçaların yerinde rakamlar bulunduğu bilinir.
    pub fn kayittan(zaman: &str) -> Zaman {
        let baytlar = zaman.as_bytes();
        // Parçalar hep rakamdır; olmasaydı 0 olurdu, program çökmezdi
        let parca = |bas: usize, son: usize| rakamlar(&baytlar[bas..son]).unwrap_or(0);
        Zaman {
            yil: parca(0, 4),
            ay: parca(5, 7),
            gun: parca(8, 10),
            saat: parca(11, 13),
            dakika: parca(14, 16),
            saniye: parca(17, 19),
        }
    }

    /// 1970-01-01 00:00:00'dan bu yana geçen saniye. İki anın farkı aralarındaki süredir.
    ///
    /// `i64::from`: küçük tamsayı türünü kayıpsız büyüğe çevirir (`as`'ın aksine taşma ya da
    /// kırpma ihtimali olmadığı derleyicice bilinir).
    pub fn saniye(self) -> i64 {
        let gun = gun_numarasi(i64::from(self.yil), i64::from(self.ay), i64::from(self.gun));
        gun * 86_400
            + i64::from(self.saat) * 3_600
            + i64::from(self.dakika) * 60
            + i64::from(self.saniye)
    }
}

/// `2026-10-24 15:00:00`
impl fmt::Display for Zaman {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.yil, self.ay, self.gun, self.saat, self.dakika, self.saniye
        )
    }
}

/// `--karsilastir` değeri: logun ikiye bölüneceği an.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZamanNoktasi {
    /// Yalnızca saat (`15:00`): logun son gününe uygulanır.
    Saat { saat: u16, dakika: u16, saniye: u16 },
    /// Tarih ve saat (`2026-10-24 15:00`). Yalnızca tarih verildiyse günün başı.
    Tam(Zaman),
}

impl ZamanNoktasi {
    /// Komut satırındaki yazılışından: `15:00`, `9:30:15`, `2026-10-24`, `2026-10-24 15:00`,
    /// `2026-10-24T15:00:30`. Geçersiz bir tarih ya da saatte (`2026-02-30`, `24:00`) None.
    pub fn ayristir(metin: &str) -> Option<ZamanNoktasi> {
        // Tarih ile saat arasında boşluk ya da (ISO 8601'deki gibi) T olabilir
        if let Some((tarih, saat)) = metin.split_once([' ', 'T']) {
            let (yil, ay, gun) = tarih_ayristir(tarih)?;
            let (saat, dakika, saniye) = saat_ayristir(saat)?;
            Some(ZamanNoktasi::Tam(Zaman {
                yil,
                ay,
                gun,
                saat,
                dakika,
                saniye,
            }))
        } else if metin.contains('-') {
            let (yil, ay, gun) = tarih_ayristir(metin)?;
            Some(ZamanNoktasi::Tam(Zaman {
                yil,
                ay,
                gun,
                saat: 0,
                dakika: 0,
                saniye: 0,
            }))
        } else {
            let (saat, dakika, saniye) = saat_ayristir(metin)?;
            Some(ZamanNoktasi::Saat {
                saat,
                dakika,
                saniye,
            })
        }
    }

    /// Tam bir ana çevirir. Yalnızca saat verildiyse tarih logun son kaydından alınır.
    pub fn coz(self, son_kayit: Zaman) -> Zaman {
        match self {
            // Yapı güncelleme sözdizimi: saat alanları bizden, tarih alanları son kayıttan
            ZamanNoktasi::Saat {
                saat,
                dakika,
                saniye,
            } => Zaman {
                saat,
                dakika,
                saniye,
                ..son_kayit
            },
            ZamanNoktasi::Tam(zaman) => zaman,
        }
    }
}

/// Süreyi okunur yazar: `45 sn`, `12 dk 5 sn`, `6 sa 0 dk`, `2 gün 3 sa`
pub fn sure_yaz(saniye: usize) -> String {
    let (gun, kalan) = (saniye / 86_400, saniye % 86_400);
    let (saat, kalan) = (kalan / 3_600, kalan % 3_600);
    let (dakika, saniye) = (kalan / 60, kalan % 60);
    if gun > 0 {
        format!("{gun} gün {saat} sa")
    } else if saat > 0 {
        format!("{saat} sa {dakika} dk")
    } else if dakika > 0 {
        format!("{dakika} dk {saniye} sn")
    } else {
        format!("{saniye} sn")
    }
}

/// 1970-01-01'den bu yana geçen gün; öncesi için eksi.
///
/// Howard Hinnant'ın `days_from_civil` algoritması. Yıl Mart'ta başlıyormuş gibi sayılır:
/// Şubat yılın son ayı olur ve artık yıldaki 29 Şubat yılın en sonuna düşer. Böylece bir ayın
/// yıl içindeki ilk günü, Şubat'ın uzunluğundan bağımsız tek bir formülle bulunur. Takvim
/// 400 yılda bir kendini tekrarlar (146 097 gün), hesap bu dönemler üzerinden yapılır.
fn gun_numarasi(yil: i64, ay: i64, gun: i64) -> i64 {
    // Ocak ve Şubat bir önceki "Mart yılına" aittir
    let yil = if ay <= 2 { yil - 1 } else { yil };
    // `div_euclid` ve `rem_euclid` eksi sayılarda da aşağı yuvarlar: -1 / 400 = -1, kalan 399
    let donem = yil.div_euclid(400);
    let donem_yili = yil.rem_euclid(400); // 0..=399
    let mart_ayi = (ay + 9) % 12; // Mart 0, Nisan 1, ..., Şubat 11
    // Mart'tan bu ayın başına kadar geçen gün: 153 gün her 5 ayda tekrarlanan 31-30 düzenidir
    let yilin_gunu = (153 * mart_ayi + 2) / 5 + gun - 1; // 0..=365
    // Her 4 yılda bir artık gün; 100'ün katlarında yok, 400'ün katlarında var
    let donemin_gunu = donem_yili * 365 + donem_yili / 4 - donem_yili / 100 + yilin_gunu;
    // 719 468: 0000-03-01 ile 1970-01-01 arasındaki gün sayısı
    donem * 146_097 + donemin_gunu - 719_468
}

/// `2026-10-24` → (2026, 10, 24). Ay 1–12, gün o ayın gün sayısıyla sınırlı.
fn tarih_ayristir(metin: &str) -> Option<(u16, u16, u16)> {
    let mut parcalar = metin.split('-');
    let yil = sayi(parcalar.next()?, 4..=4)?;
    let ay = sayi(parcalar.next()?, 2..=2)?;
    let gun = sayi(parcalar.next()?, 2..=2)?;
    let gecerli = parcalar.next().is_none()
        && (1..=12).contains(&ay)
        && (1..=ayin_gun_sayisi(yil, ay)).contains(&gun);
    gecerli.then_some((yil, ay, gun))
}

/// `15:00` ya da `9:30:15` → (saat, dakika, saniye). Saniye verilmezse 0.
fn saat_ayristir(metin: &str) -> Option<(u16, u16, u16)> {
    let mut parcalar = metin.split(':');
    let saat = sayi(parcalar.next()?, 1..=2)?;
    let dakika = sayi(parcalar.next()?, 2..=2)?;
    let saniye = match parcalar.next() {
        Some(parca) => sayi(parca, 2..=2)?,
        None => 0,
    };
    let gecerli = parcalar.next().is_none() && saat <= 23 && dakika <= 59 && saniye <= 59;
    gecerli.then_some((saat, dakika, saniye))
}

/// Basamak sayısı `basamak` aralığında olan bir sayı: `sayi("09", 2..=2)` → 9
fn sayi(metin: &str, basamak: RangeInclusive<usize>) -> Option<u16> {
    if !basamak.contains(&metin.len()) {
        return None;
    }
    rakamlar(metin.as_bytes())
}

/// 1–4 ASCII rakamını sayıya çevirir: `b"2026"` → 2026. Rakam olmayan bir bayt varsa None.
///
/// `fold`: bir başlangıç değeriyle (0) başlar, her rakamda önceki sonucu 10 ile çarpıp rakamı
/// ekler. En çok 4 basamak olduğu için sonuç 9999'u geçmez, `u16` taşmaz.
fn rakamlar(baytlar: &[u8]) -> Option<u16> {
    let gecerli = (1..=4).contains(&baytlar.len()) && baytlar.iter().all(u8::is_ascii_digit);
    gecerli.then(|| {
        baytlar
            .iter()
            .fold(0, |sonuc, bayt| sonuc * 10 + u16::from(bayt - b'0'))
    })
}

fn ayin_gun_sayisi(yil: u16, ay: u16) -> u16 {
    match ay {
        2 if artik_yil(yil) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Her 4 yılda bir artık yıl; 100'ün katları değil, 400'ün katları yine artık yıl.
fn artik_yil(yil: u16) -> bool {
    (yil.is_multiple_of(4) && !yil.is_multiple_of(100)) || yil.is_multiple_of(400)
}

#[cfg(test)]
mod testler {
    use super::*;

    fn tam(metin: &str) -> Zaman {
        match ZamanNoktasi::ayristir(metin) {
            Some(ZamanNoktasi::Tam(zaman)) => zaman,
            diger => panic!("{metin}: tam zaman bekleniyordu, gelen: {diger:?}"),
        }
    }

    #[test]
    fn gun_numarasi_takvimle_uyusur() {
        // Python'un datetime modülüyle hesaplanan değerler
        for (tarih, beklenen) in [
            ((1970, 1, 1), 0),
            ((1969, 12, 31), -1),
            ((2000, 3, 1), 11_017),
            ((2024, 2, 29), 19_782),
            ((2024, 3, 1), 19_783),
            ((2026, 10, 24), 20_750),
            ((2100, 3, 1), 47_541),
            // Hesabın başlangıç noktası: 0000-03-01
            ((0, 3, 1), -719_468),
            // Ocak, bir önceki "Mart yılına", -1. yıla aittir: `/` burada 0 verirdi,
            // `div_euclid` doğru dönemi (-1) verir
            ((0, 1, 1), -719_528),
        ] {
            let (yil, ay, gun) = tarih;
            assert_eq!(gun_numarasi(yil, ay, gun), beklenen, "{tarih:?}");
        }
    }

    #[test]
    fn kayit_zamani_okunur() {
        let zaman = Zaman::kayittan("2026-10-24 14:03:11");
        assert_eq!(zaman.to_string(), "2026-10-24 14:03:11");
        // Kesir ve saat dilimi yok sayılır; T ile boşluk aynı anı verir
        assert_eq!(Zaman::kayittan("2026-10-24T14:03:11.123456+03:00"), zaman);
        assert_eq!(zaman.saniye(), 20_750 * 86_400 + 14 * 3_600 + 3 * 60 + 11);
    }

    #[test]
    fn zamanlar_kronolojik_siralanir() {
        let sirali = [
            "2025-12-31 23:59:59",
            "2026-01-01 00:00:00",
            "2026-01-01 00:00:01",
            "2026-01-01 00:01:00",
            "2026-01-01 01:00:00",
            "2026-01-02 00:00:00",
            "2026-02-01 00:00:00",
        ];
        for ikili in sirali.windows(2) {
            let (once, sonra) = (Zaman::kayittan(ikili[0]), Zaman::kayittan(ikili[1]));
            assert!(once < sonra, "{ikili:?}");
            assert!(once.saniye() < sonra.saniye(), "{ikili:?}");
        }
        // Yılın son saniyesi ile yeni yılın ilk saniyesi arasında tam 1 saniye var
        assert_eq!(
            Zaman::kayittan(sirali[1]).saniye() - Zaman::kayittan(sirali[0]).saniye(),
            1
        );
    }

    #[test]
    fn zaman_noktasi_yazilislari() {
        assert_eq!(
            ZamanNoktasi::ayristir("15:00"),
            Some(ZamanNoktasi::Saat {
                saat: 15,
                dakika: 0,
                saniye: 0
            })
        );
        assert_eq!(
            ZamanNoktasi::ayristir("9:30:15"),
            Some(ZamanNoktasi::Saat {
                saat: 9,
                dakika: 30,
                saniye: 15
            })
        );
        assert_eq!(tam("2026-10-24").to_string(), "2026-10-24 00:00:00");
        assert_eq!(tam("2026-10-24 15:00").to_string(), "2026-10-24 15:00:00");
        assert_eq!(
            tam("2026-10-24T15:00:30").to_string(),
            "2026-10-24 15:00:30"
        );
        assert_eq!(
            tam("2024-02-29 23:59:59").to_string(),
            "2024-02-29 23:59:59"
        );
    }

    #[test]
    fn gecersiz_zamanlar_reddedilir() {
        for metin in [
            "",
            "15",
            "15:0",
            "15:000",
            "15:00:0",
            "123:00",
            "015:00",
            "24:00",
            "15:60",
            "15:00:60",
            "15:00:00:00",
            "ab:cd",
            // Sıfır yerine büyük O harfi
            "15:0O",
            "2026-10",
            "2026-13-01",
            "2026-00-10",
            "2026-02-29",
            "2025-04-31",
            "2026-10-00",
            "26-10-24",
            "2026-10-24-1",
            "2026-10-24 15",
            "2026-10-24 25:00",
            "2026-10-24T",
            "15:00 ",
        ] {
            assert_eq!(ZamanNoktasi::ayristir(metin), None, "{metin:?}");
        }
    }

    #[test]
    fn yalnizca_saat_son_kaydin_gunune_uygulanir() {
        let son_kayit = Zaman::kayittan("2026-10-25 08:12:40");
        let nokta = ZamanNoktasi::ayristir("15:00").unwrap();
        assert_eq!(nokta.coz(son_kayit).to_string(), "2026-10-25 15:00:00");
        // Tam zaman verildiyse son kayıt değiştirmez
        let tam_nokta = ZamanNoktasi::ayristir("2026-10-24 15:00").unwrap();
        assert_eq!(tam_nokta.coz(son_kayit).to_string(), "2026-10-24 15:00:00");
    }

    #[test]
    fn sure_okunur_yazilir() {
        assert_eq!(sure_yaz(45), "45 sn");
        assert_eq!(sure_yaz(60), "1 dk 0 sn");
        assert_eq!(sure_yaz(12 * 60 + 5), "12 dk 5 sn");
        assert_eq!(sure_yaz(3_600), "1 sa 0 dk");
        assert_eq!(sure_yaz(21_602), "6 sa 0 dk");
        assert_eq!(sure_yaz(12_668), "3 sa 31 dk");
        assert_eq!(sure_yaz(86_400), "1 gün 0 sa");
        assert_eq!(sure_yaz(2 * 86_400 + 3 * 3_600 + 59), "2 gün 3 sa");
    }

    #[test]
    fn ay_uzunluklari_ve_artik_yillar() {
        let uzunluklar: Vec<u16> = (1..=12).map(|ay| ayin_gun_sayisi(2026, ay)).collect();
        assert_eq!(uzunluklar, [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]);
        // 4'e bölünen yıllar artık yıldır; 100'e bölünenler değil, 400'e bölünenler yine artık
        for (yil, subat) in [
            (2024, 29),
            (2026, 28),
            (1900, 28),
            (2000, 29),
            (2100, 28),
            (2400, 29),
        ] {
            assert_eq!(ayin_gun_sayisi(yil, 2), subat, "{yil}");
        }
    }
}
