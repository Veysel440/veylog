//! Filtreler: özetin hangi kayıtlarla çıkarılacağı. Seviyeye göre (`--seviye warning`: WARNING
//! ve üstü) ve zamana göre (`--baslangic`, `--bitis`, `--son 2sa`).
//!
//! Filtre kayıtlar okunurken uygulanır: filtreye uymayan kayıt depoya hiç girmez. Kalıplar,
//! seviyeler, zaman aralığı, karşılaştırma ve ani artışlar böylece yalnızca filtreye uyan
//! kayıtlardan çıkar; log sanki yalnızca o kayıtlardan oluşuyormuş gibi özetlenir. Dosyanın
//! satır ve kayıt sayıları ise dosyanın tamamınındır.
//!
//! Bazı sınırlar logun sonunu bilmeyi gerektirir: `--son 2sa` en yeni kayıttan iki saat
//! öncesidir, yalnızca saat verilmiş bir sınır (`--baslangic 15:00`) logun son gününe
//! uygulanır (`--karsilastir` gibi). O zaman log önce bir kez taranıp en yeni kaydı bulunur
//! (`on_tarama`), sonra filtreyle okunur.

use std::fmt;

use crate::dil::{self, Dil};
use crate::kayit::Seviye;
use crate::zaman::{Zaman, ZamanNoktasi};

/// Komut satırında verilen filtre; zaman sınırları henüz logun sonuna göre çözülmemiştir.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Filtre {
    /// En düşük seviye: bu ve üstündeki kayıtlar.
    pub seviye: Option<Seviye>,
    /// Bu andan (dahil) sonraki kayıtlar.
    pub baslangic: Option<ZamanNoktasi>,
    /// Bu andan (hariç) önceki kayıtlar.
    pub bitis: Option<ZamanNoktasi>,
    /// En yeni kayıttan bu kadar saniye öncesinden (dahil) başlayan kayıtlar.
    pub son: Option<u64>,
}

impl Filtre {
    /// Hiçbir filtre verilmemiş mi?
    pub fn bos_mu(&self) -> bool {
        *self == Filtre::default()
    }

    /// Zaman sınırı var mı (`--baslangic`, `--bitis`, `--son`)?
    pub fn zaman_sinirli(&self) -> bool {
        self.baslangic.is_some() || self.bitis.is_some() || self.son.is_some()
    }

    /// Sınırları çözmek için logun sonunu (en yeni kaydı) bilmek gerekiyor mu?
    pub fn sonu_gerekir(&self) -> bool {
        self.son.is_some()
            || matches!(self.baslangic, Some(ZamanNoktasi::Saat { .. }))
            || matches!(self.bitis, Some(ZamanNoktasi::Saat { .. }))
    }

    /// Sınırları çözer. `en_yeni`: logun en yeni kaydının zamanı; yalnızca `sonu_gerekir`
    /// ise gerekir. Başlangıç bitişten önce değilse hiçbir kayıt uyamaz: kullanım hatası.
    pub fn coz(&self, en_yeni: Option<Zaman>) -> Result<CozulmusFiltre, FiltreHatasi> {
        let cozulmus = self.coz_bos_olabilir(en_yeni);
        if let (Some(bas), Some(bit)) = (cozulmus.baslangic, cozulmus.bitis)
            && bas >= bit
        {
            return Err(FiltreHatasi::BosAralik {
                baslangic: Zaman::saniyeden(bas),
                bitis: Zaman::saniyeden(bit),
            });
        }
        Ok(cozulmus)
    }

    /// Sınırları çözer; aralık boş olsa da hata vermez, hiçbir kayıt uymaz. Canlı izlemede:
    /// log ilerledikçe pencere boşalabilir (`--baslangic 09:00 --bitis 2026-10-25`).
    ///
    /// Logda kayıt yoksa (`en_yeni` None) logun sonu da yoktur: sona göre verilen sınırlar
    /// (`--son`, yalnızca saat) çözülmez, yoktur. Gelecek bir kayıt da olmadığı için sonuç
    /// değişmez; rapor da 1970'e göre çözülmüş bir tarih göstermez.
    pub fn coz_bos_olabilir(&self, en_yeni: Option<Zaman>) -> CozulmusFiltre {
        let coz = |nokta: ZamanNoktasi| match (nokta, en_yeni) {
            (ZamanNoktasi::Tam(zaman), _) => Some(zaman.saniye()),
            (nokta @ ZamanNoktasi::Saat { .. }, Some(son)) => Some(nokta.coz(son).saniye()),
            (ZamanNoktasi::Saat { .. }, None) => None,
        };
        let baslangic = match (self.son, en_yeni) {
            (Some(sure), Some(son)) => {
                let sure = i64::try_from(sure).unwrap_or(i64::MAX);
                Some(son.saniye().saturating_sub(sure))
            }
            (Some(_), None) => None,
            (None, _) => self.baslangic.and_then(coz),
        };
        CozulmusFiltre {
            seviye: self.seviye.unwrap_or(Seviye::Debug),
            baslangic,
            bitis: self.bitis.and_then(coz),
        }
    }
}

/// Zaman sınırları saniyeye çevrilmiş filtre: her kayıt için uyup uymadığı söylenebilir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CozulmusFiltre {
    seviye: Seviye,
    /// 1970'ten bu yana saniye: başlangıç dahil, bitiş hariç.
    baslangic: Option<i64>,
    bitis: Option<i64>,
}

impl CozulmusFiltre {
    /// Kayıt filtreye uyuyor mu? `saniye`: kaydın zamanı, 1970'ten bu yana.
    pub fn gecer(&self, saniye: i64, seviye: Seviye) -> bool {
        seviye >= self.seviye
            && self.baslangic.is_none_or(|bas| saniye >= bas)
            && self.bitis.is_none_or(|bit| saniye < bit)
    }

    pub fn baslangic(&self) -> Option<Zaman> {
        self.baslangic.map(Zaman::saniyeden)
    }

    pub fn bitis(&self) -> Option<Zaman> {
        self.bitis.map(Zaman::saniyeden)
    }
}

/// Filtre çözülemedi.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FiltreHatasi {
    /// Başlangıç bitişten önce değil: hiçbir kayıt uyamaz.
    BosAralik { baslangic: Zaman, bitis: Zaman },
}

impl fmt::Display for FiltreHatasi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FiltreHatasi::BosAralik { baslangic, bitis } => match dil::simdiki() {
                Dil::Tr => write!(
                    f,
                    "filtre aralığı boş: başlangıç ({baslangic}) bitişten ({bitis}) önce olmalı"
                ),
                Dil::En => write!(
                    f,
                    "the filter range is empty: the start ({baslangic}) must be before the end ({bitis})"
                ),
            },
        }
    }
}

impl std::error::Error for FiltreHatasi {}

/// `--son` değeri: bir süre, saniye olarak. Birimler: `sn`/`s` saniye, `dk`/`m` dakika,
/// `sa`/`h` saat, `g`/`d` gün; birleştirilebilir: `1sa30dk`. En az bir saniye; saniyenin
/// kesri (`500ms`) kabul edilmez.
pub fn sure_ayristir(metin: &str) -> Option<u64> {
    let milisaniye = milisaniye_ayristir(metin)?;
    (milisaniye % 1_000 == 0).then_some(milisaniye / 1_000)
}

/// Bir süre, milisaniye olarak. `sure_ayristir`in birimlerine ek olarak `ms` milisaniye
/// (`--aralik 500ms`). En az bir milisaniye.
pub fn milisaniye_ayristir(metin: &str) -> Option<u64> {
    let mut kalan = metin.trim();
    if kalan.is_empty() {
        return None;
    }
    let mut toplam: u64 = 0;
    while !kalan.is_empty() {
        let rakamlar = kalan
            .find(|harf: char| !harf.is_ascii_digit())
            .unwrap_or(kalan.len());
        if rakamlar == 0 {
            return None;
        }
        let sayi: u64 = kalan[..rakamlar].parse().ok()?;
        kalan = &kalan[rakamlar..];
        let birim_sonu = kalan
            .find(|harf: char| harf.is_ascii_digit())
            .unwrap_or(kalan.len());
        let carpan = match kalan[..birim_sonu].to_lowercase().as_str() {
            "ms" | "msn" | "milisaniye" => 1,
            "s" | "sn" | "saniye" => 1_000,
            "m" | "dk" | "dakika" => 60_000,
            "h" | "sa" | "saat" => 3_600_000,
            "d" | "g" | "gun" | "gün" => 86_400_000,
            _ => return None,
        };
        toplam = toplam.checked_add(sayi.checked_mul(carpan)?)?;
        kalan = &kalan[birim_sonu..];
    }
    (toplam > 0).then_some(toplam)
}

#[cfg(test)]
mod testler {
    use super::*;

    #[test]
    fn bos_aralik_iki_dilde() {
        let hata = Filtre {
            baslangic: ZamanNoktasi::ayristir("15:00"),
            bitis: ZamanNoktasi::ayristir("14:00"),
            ..Filtre::default()
        }
        .coz(Some(Zaman::kayittan("2026-10-24 18:00:00")))
        .unwrap_err();
        assert_eq!(
            hata.to_string(),
            "filtre aralığı boş: başlangıç (2026-10-24 15:00:00) bitişten (2026-10-24 14:00:00) önce olmalı"
        );
        let _dil = dil::gecici(Dil::En);
        assert_eq!(
            hata.to_string(),
            "the filter range is empty: the start (2026-10-24 15:00:00) must be before the end (2026-10-24 14:00:00)"
        );
    }

    fn zaman(metin: &str) -> Zaman {
        Zaman::kayittan(metin)
    }

    #[test]
    fn milisaniyeli_sureler_okunur() {
        for (metin, beklenen) in [
            ("500ms", 500),
            ("200MS", 200),
            ("1sn500ms", 1_500),
            ("2sn", 2_000),
            ("1dk", 60_000),
            ("1sa", 3_600_000),
            ("1g", 86_400_000),
            ("1milisaniye", 1),
        ] {
            assert_eq!(milisaniye_ayristir(metin), Some(beklenen), "{metin}");
        }
        for metin in ["", "0ms", "ms", "1.5sn", "500 ms", "99999999999999999999ms"] {
            assert_eq!(milisaniye_ayristir(metin), None, "{metin:?}");
        }
    }

    #[test]
    fn sureler_okunur() {
        for (metin, beklenen) in [
            ("30dk", 1_800),
            ("2sa", 7_200),
            ("1g", 86_400),
            ("1sa30dk", 5_400),
            ("45sn", 45),
            ("1g2sa3dk4sn", 93_784),
            // İngilizce kısaltmalar ve büyük harf
            ("2h", 7_200),
            ("90m", 5_400),
            ("1d", 86_400),
            ("30s", 30),
            ("2SA", 7_200),
            ("1gun", 86_400),
            ("1gün", 86_400),
            ("2saat", 7_200),
            (" 15dk ", 900),
        ] {
            assert_eq!(sure_ayristir(metin), Some(beklenen), "{metin}");
        }
        for metin in [
            "",
            "0",
            "0dk",
            "dk",
            "2",
            "2x",
            "1.5sa",
            "1,5sa",
            "-1sa",
            "sa2",
            "2 sa",
            "1sa 30dk",
            // Taşma
            "99999999999999999999g",
            "999999999999999g",
            // Saniyenin kesri
            "500ms",
            "1sn500ms",
        ] {
            assert_eq!(sure_ayristir(metin), None, "{metin:?}");
        }
    }

    #[test]
    fn sinirlar_logun_sonuna_gore_cozulur() {
        let en_yeni = Some(zaman("2026-10-25 08:12:40"));
        // --son: en yeni kayıttan geriye
        let filtre = Filtre {
            son: Some(7_200),
            ..Filtre::default()
        };
        assert!(filtre.sonu_gerekir());
        let cozulmus = filtre.coz(en_yeni).unwrap();
        assert_eq!(cozulmus.baslangic(), Some(zaman("2026-10-25 06:12:40")));
        assert_eq!(cozulmus.bitis(), None);
        // Yalnızca saat: son günün o saati; tam zaman olduğu gibi
        let filtre = Filtre {
            baslangic: ZamanNoktasi::ayristir("07:00"),
            bitis: ZamanNoktasi::ayristir("2026-10-26"),
            ..Filtre::default()
        };
        let cozulmus = filtre.coz(en_yeni).unwrap();
        assert_eq!(cozulmus.baslangic(), Some(zaman("2026-10-25 07:00:00")));
        assert_eq!(cozulmus.bitis(), Some(zaman("2026-10-26 00:00:00")));
        // Boş aralık
        let filtre = Filtre {
            baslangic: ZamanNoktasi::ayristir("09:00"),
            bitis: ZamanNoktasi::ayristir("09:00"),
            ..Filtre::default()
        };
        assert_eq!(
            filtre.coz(en_yeni).unwrap_err().to_string(),
            "filtre aralığı boş: başlangıç (2026-10-25 09:00:00) bitişten (2026-10-25 09:00:00) önce olmalı"
        );
        // Yalnızca saat verilmiş bir bitiş de logun sonunu gerektirir
        let filtre = Filtre {
            bitis: ZamanNoktasi::ayristir("09:00"),
            ..Filtre::default()
        };
        assert!(filtre.sonu_gerekir());
        // Yalnızca seviye: logun sonu gerekmez
        let filtre = Filtre {
            seviye: Some(Seviye::Error),
            ..Filtre::default()
        };
        assert!(!filtre.sonu_gerekir());
        assert!(!filtre.bos_mu());
        assert!(Filtre::default().bos_mu());
        let cozulmus = filtre.coz(None).unwrap();
        assert_eq!((cozulmus.baslangic(), cozulmus.bitis()), (None, None));
    }

    #[test]
    fn kayitsiz_logda_sona_gore_sinirlar_cozulmez() {
        // Logda kayıt yok: `--son` ve yalnızca saat verilmiş sınırlar yoktur (1970'e göre
        // çözülmez); tam zamanlar olduğu gibi. Aralık boş sayılmaz.
        for (baslangic, bitis, son, beklenen) in [
            (None, None, Some(3_600), (None, None)),
            (Some("15:00"), None, None, (None, None)),
            (
                Some("2026-10-24"),
                Some("18:00"),
                None,
                (Some(zaman("2026-10-24 00:00:00")), None),
            ),
            (
                None,
                Some("2026-10-25 06:00"),
                Some(60),
                (None, Some(zaman("2026-10-25 06:00:00"))),
            ),
        ] {
            let filtre = Filtre {
                baslangic: baslangic.and_then(ZamanNoktasi::ayristir),
                bitis: bitis.and_then(ZamanNoktasi::ayristir),
                son,
                ..Filtre::default()
            };
            let cozulmus = filtre.coz(None).unwrap();
            assert_eq!((cozulmus.baslangic(), cozulmus.bitis()), beklenen);
        }
    }

    #[test]
    fn izlemede_bos_aralik_hata_degildir() {
        let filtre = Filtre {
            baslangic: ZamanNoktasi::ayristir("09:00"),
            bitis: ZamanNoktasi::ayristir("2026-10-25"),
            ..Filtre::default()
        };
        // Log 25 Ekim'e geçti: başlangıç (25 Ekim 09:00) bitişten sonra
        let en_yeni = Some(zaman("2026-10-25 10:00:00"));
        assert!(filtre.coz(en_yeni).is_err());
        let cozulmus = filtre.coz_bos_olabilir(en_yeni);
        assert_eq!(cozulmus.baslangic(), Some(zaman("2026-10-25 09:00:00")));
        // Hiçbir kayıt uymaz
        for an in ["2026-10-24 12:00:00", "2026-10-25 09:30:00"] {
            assert!(
                !cozulmus.gecer(zaman(an).saniye(), Seviye::Emergency),
                "{an}"
            );
        }
    }

    #[test]
    fn kayit_filtreye_uyar_ya_da_uymaz() {
        let filtre = Filtre {
            seviye: Some(Seviye::Warning),
            baslangic: ZamanNoktasi::ayristir("2026-10-24 10:00"),
            bitis: ZamanNoktasi::ayristir("2026-10-24 11:00"),
            son: None,
        }
        .coz(None)
        .unwrap();
        let saniye = |metin| zaman(metin).saniye();
        // Başlangıç dahil, bitiş hariç
        assert!(filtre.gecer(saniye("2026-10-24 10:00:00"), Seviye::Warning));
        assert!(filtre.gecer(saniye("2026-10-24 10:59:59"), Seviye::Emergency));
        assert!(!filtre.gecer(saniye("2026-10-24 11:00:00"), Seviye::Error));
        assert!(!filtre.gecer(saniye("2026-10-24 09:59:59"), Seviye::Error));
        // Seviye
        assert!(!filtre.gecer(saniye("2026-10-24 10:30:00"), Seviye::Notice));
        // Filtresiz çözülen her kayda uyar
        let hepsi = Filtre::default().coz(None).unwrap();
        assert!(hepsi.gecer(i64::MIN, Seviye::Debug));
    }
}
