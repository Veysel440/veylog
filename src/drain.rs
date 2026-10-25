//! Drain: sabit derinlikli ağaçla log kalıbı çıkarma.
//!
//! Kaynak: He, Zhu, Zheng ve Lyu, "Drain: An Online Log Parsing Approach with Fixed
//! Depth Tree", ICWS 2017.
//!
//! Mesajlar teker teker işlenir. Her mesaj ya benzediği bir kümeye katılır ya da yeni bir
//! küme açar. Kümenin kalıbında, mesajdan mesaja değişen konumlar `<*>` olur:
//!
//! ```text
//! Zamanlanmış görev başladı: kurlar:guncelle  ┐
//! Zamanlanmış görev başladı: yedek:al         ┘→  Zamanlanmış görev başladı: <*>
//! ```
//!
//! Kümeler sabit derinlikli bir ağaçta tutulur; böylece her mesaj bütün kümelerle değil,
//! sadece kendisine benzeme ihtimali olan birkaç kümeyle karşılaştırılır:
//!
//! ```text
//! kök
//! ├── 3 belirteç
//! │   ├── "Sepet"        → [Sepet önbellekten okundu]
//! │   └── "Sipariş"      → [Sipariş <SAYI> oluşturuldu]
//! └── 4 belirteç
//!     ├── "Kullanıcı"    → [Kullanıcı <SAYI> <*> yaptı]
//!     └── "Zamanlanmış"  → [Zamanlanmış görev başladı: <*>]
//! ```
//!
//! Bir mesaj için yapılanlar:
//!
//! 1. Mesaj boşluklardan belirteçlere bölünür.
//! 2. Belirteç sayısı, ağacın ilk katmanında dalı seçer.
//! 3. İlk `onek_derinligi` belirtece göre aşağı inilir. İçinde rakam olan belirteçler ve
//!    dolmuş bir düğüme gelen yeni belirteçler ortak `<*>` dalına gider.
//! 4. Varılan yapraktaki kümelerden en benzeri bulunur. Benzerlik eşiği geçiyorsa mesaj o
//!    kümeye katılır, geçmiyorsa yeni bir küme açılır.
//! 5. Kümeye katılan mesajla kalıp arasında farklı olan konumlar `<*>` yapılır.
//!
//! Rakama duyarlı eşik: 4. adımdaki eşik, rakam içeren farklılıkların oranında gevşer.
//! Fikir Drain'in genişletilmiş sürümünden gelir (He ve ark., "A Directed Acyclic Graph
//! Approach to Online Log Parsing", 2018): rakam içeren belirteçler büyük ihtimalle
//! değişkendir. Böylece `Yanıt süresi 120ms` ile `Yanıt süresi 95ms` gibi kısa mesajlar da
//! birleşebilir.

use std::collections::HashMap;

/// Kalıpta değişken konumların yerine konan işaret.
pub const JOKER: &str = "<*>";

/// Drain'in ayarları.
#[derive(Debug, Clone)]
pub struct Ayarlar {
    /// Ağaçta mesajın kaç baş belirtecine göre dallanılacağı.
    pub onek_derinligi: usize,
    /// Bir mesajın kümeye katılması için gereken en düşük benzerlik (0 ile 1 arası).
    pub benzerlik_esigi: f64,
    /// Bir düğümün en fazla çocuk sayısı; dolunca yeni belirteçler `<*>` dalına gider.
    pub en_fazla_cocuk: usize,
    /// Eşik, rakam içeren farklılıkların oranında gevşesin mi? Kapalıyken davranış klasik
    /// Drain (ve Drain3) ile birebir aynıdır.
    pub rakama_duyarli_esik: bool,
}

impl Default for Ayarlar {
    fn default() -> Self {
        Ayarlar {
            onek_derinligi: 1,
            benzerlik_esigi: 0.7,
            en_fazla_cocuk: 100,
            rakama_duyarli_esik: true,
        }
    }
}

/// Aynı kalıba uyan mesajlar.
#[derive(Debug)]
pub struct Kume {
    /// Kalıbın belirteçleri; değişen konumlarda `<*>`.
    pub kalip: Vec<String>,
    /// Kümeye katılan mesaj sayısı.
    pub adet: usize,
}

impl Kume {
    /// Kalıbın okunabilir hali: "Zamanlanmış görev başladı: <*>"
    pub fn metin(&self) -> String {
        self.kalip.join(" ")
    }
}

/// Ağacın bir düğümü.
#[derive(Debug, Default)]
struct Dugum {
    /// Belirteç → alt düğüm.
    cocuklar: HashMap<String, Dugum>,
    /// Sadece yapraklarda dolu: bu yaprağa düşen kümelerin numaraları.
    kume_numaralari: Vec<usize>,
}

/// Drain kalıp çıkarıcısı.
#[derive(Debug)]
pub struct Drain {
    ayarlar: Ayarlar,
    /// Ağacın ilk katmanı: belirteç sayısı → o uzunluktaki mesajların alt ağacı.
    uzunluklar: HashMap<usize, Dugum>,
    /// Bütün kümeler. Ağaç kümelere bu listedeki sıra numarasıyla (indeks) işaret eder.
    kumeler: Vec<Kume>,
}

impl Drain {
    pub fn new(ayarlar: Ayarlar) -> Self {
        Drain {
            ayarlar,
            uzunluklar: HashMap::new(),
            kumeler: Vec::new(),
        }
    }

    /// Şimdiye kadar bulunan kümeler, ilk görüldükleri sırayla.
    pub fn kumeler(&self) -> &[Kume] {
        &self.kumeler
    }

    /// Mesajı benzediği kümeye ekler, yoksa yeni küme açar. Kümenin numarasını döndürür.
    pub fn ekle(&mut self, mesaj: &str) -> usize {
        // 1. Belirteçlere böl
        let belirtecler: Vec<&str> = mesaj.split_whitespace().collect();

        // 2–4. Ağaçta en benzer kümeyi ara
        match self.ara(&belirtecler) {
            // 5. Bulunan kümeye kat ve kalıbını güncelle
            Some(numara) => {
                let kume = &mut self.kumeler[numara];
                kalibi_guncelle(&mut kume.kalip, &belirtecler);
                kume.adet += 1;
                numara
            }
            // Yeterince benzer küme yok: yeni küme aç ve ağaca yerleştir
            None => {
                let numara = self.kumeler.len();
                self.kumeler.push(Kume {
                    kalip: belirtecler.iter().map(|b| b.to_string()).collect(),
                    adet: 1,
                });
                self.agaca_ekle(&belirtecler, numara);
                numara
            }
        }
    }

    /// Mesaja yeterince benzeyen en iyi kümeyi bulur. Ağacı değiştirmez.
    fn ara(&self, belirtecler: &[&str]) -> Option<usize> {
        // 2. İlk katman: belirteç sayısı
        let mut dugum = self.uzunluklar.get(&belirtecler.len())?;

        // 3. Baş belirteçlere göre aşağı in: önce birebir aynı dal, yoksa joker dalı
        for belirtec in &belirtecler[..self.yonlendiren_sayisi(belirtecler.len())] {
            dugum = match dugum.cocuklar.get(*belirtec) {
                Some(cocuk) => cocuk,
                None => dugum.cocuklar.get(JOKER)?,
            };
        }

        // Boş mesajlar (ör. Log::info('')) hep aynı kümede toplanır
        if belirtecler.is_empty() {
            return dugum.kume_numaralari.first().copied();
        }

        // 4. Yapraktaki en benzer küme. Bir yapraktaki bütün kalıplar aynı uzunlukta olduğu
        // için eşit belirteç sayısını karşılaştırmak, oranları karşılaştırmakla aynı şey.
        let mut en_iyi: Option<(usize, Karsilastirma)> = None;
        for &numara in &dugum.kume_numaralari {
            let sonuc = karsilastir(&self.kumeler[numara].kalip, belirtecler);
            // Eşitlikte daha çok jokeri olan, yani daha genel kalıp seçilir
            if en_iyi.is_none_or(|(_, e)| (sonuc.esit, sonuc.joker) > (e.esit, e.joker)) {
                en_iyi = Some((numara, sonuc));
            }
        }
        let (numara, sonuc) = en_iyi?;

        // Rakam içeren farklılıklar büyük ihtimalle değişkendir; eşik onların oranında gevşer:
        // eşik × (n − rakamlı fark) / n
        let n = belirtecler.len() as f64;
        let rakamli_fark = if self.ayarlar.rakama_duyarli_esik {
            sonuc.rakamli_fark as f64
        } else {
            0.0
        };
        let esik = self.ayarlar.benzerlik_esigi * (n - rakamli_fark) / n;
        let benzerlik = sonuc.esit as f64 / n;
        (benzerlik >= esik).then_some(numara)
    }

    /// Yeni kümeyi ağaca yerleştirir; yol üzerindeki eksik düğümleri oluşturur.
    fn agaca_ekle(&mut self, belirtecler: &[&str], numara: usize) {
        let yonlendiren = self.yonlendiren_sayisi(belirtecler.len());
        let en_fazla_cocuk = self.ayarlar.en_fazla_cocuk;

        let mut dugum = self.uzunluklar.entry(belirtecler.len()).or_default();
        for belirtec in &belirtecler[..yonlendiren] {
            let anahtar = dal_anahtari(&dugum.cocuklar, belirtec, en_fazla_cocuk);
            dugum = dugum.cocuklar.entry(anahtar).or_default();
        }
        dugum.kume_numaralari.push(numara);
    }

    /// Kaç baş belirtece göre dallanılacağı. Son belirteç dallanmaya hiç katılmaz;
    /// böylece benzerlik karşılaştırmasına her zaman en az bir belirteç kalır.
    fn yonlendiren_sayisi(&self, uzunluk: usize) -> usize {
        self.ayarlar.onek_derinligi.min(uzunluk.saturating_sub(1))
    }
}

/// Yeni bir küme yerleştirilirken belirtecin hangi dala gideceği.
fn dal_anahtari(
    cocuklar: &HashMap<String, Dugum>,
    belirtec: &str,
    en_fazla_cocuk: usize,
) -> String {
    if cocuklar.contains_key(belirtec) {
        return belirtec.to_string();
    }
    // İçinde rakam olan belirteç büyük ihtimalle değişkendir (ör. "a1b2c3"): ayrı dal açma
    if rakam_iceriyor(belirtec) {
        return JOKER.to_string();
    }
    // Düğüm doluysa yeni belirteç joker dalına gider. Joker dalına her zaman yer ayrılır.
    let joker_haric = cocuklar.len() - usize::from(cocuklar.contains_key(JOKER));
    if joker_haric + 1 >= en_fazla_cocuk {
        return JOKER.to_string();
    }
    belirtec.to_string()
}

/// İçinde en az bir rakam var mı? "120ms", "a1b2", "S3" → evet; "<*>", "giriş" → hayır.
fn rakam_iceriyor(belirtec: &str) -> bool {
    belirtec.chars().any(|c| c.is_ascii_digit())
}

/// Bir kalıpla bir mesajın konum konum karşılaştırılmasının sonucu.
#[derive(Debug, Clone, Copy)]
struct Karsilastirma {
    /// Kalıpta ve mesajda aynı olan konumlar. Joker konumlar eşit sayılmaz; makaledeki
    /// benzerlik (simSeq) böyle tanımlanır.
    esit: usize,
    /// Kalıptaki joker konumlar.
    joker: usize,
    /// Farklı olan (ya da kalıpta joker olan) ve iki taraftan en az birinde rakam bulunan
    /// konumlar: büyük ihtimalle değişken.
    rakamli_fark: usize,
}

/// Kalıp ile mesajı konum konum karşılaştırır.
fn karsilastir(kalip: &[String], belirtecler: &[&str]) -> Karsilastirma {
    let mut sonuc = Karsilastirma {
        esit: 0,
        joker: 0,
        rakamli_fark: 0,
    };
    for (k, b) in kalip.iter().zip(belirtecler) {
        let rakamli = rakam_iceriyor(k) || rakam_iceriyor(b);
        if k == JOKER {
            sonuc.joker += 1;
            if rakamli {
                sonuc.rakamli_fark += 1;
            }
        } else if k == b {
            sonuc.esit += 1;
        } else if rakamli {
            sonuc.rakamli_fark += 1;
        }
    }
    sonuc
}

/// Kalıpta mesajdan farklı olan konumları jokere çevirir.
fn kalibi_guncelle(kalip: &mut [String], belirtecler: &[&str]) {
    for (k, b) in kalip.iter_mut().zip(belirtecler) {
        if k != JOKER && k != b {
            *k = JOKER.to_string();
        }
    }
}

#[cfg(test)]
mod testler {
    use super::*;
    use crate::kayit::kayitlari_ayir;
    use crate::maske::maskele;

    /// Mesajları sırayla ekler; her kümeyi "adet × kalıp" olarak, açılış sırasıyla döndürür.
    fn kumele(ayarlar: Ayarlar, mesajlar: &[&str]) -> Vec<String> {
        let mut drain = Drain::new(ayarlar);
        for mesaj in mesajlar {
            drain.ekle(mesaj);
        }
        drain
            .kumeler()
            .iter()
            .map(|kume| format!("{} × {}", kume.adet, kume.metin()))
            .collect()
    }

    #[test]
    fn ayni_mesajlar_tek_kumede_toplanir() {
        let sonuc = kumele(Ayarlar::default(), &["Sepet önbellekten okundu"; 3]);
        assert_eq!(sonuc, ["3 × Sepet önbellekten okundu"]);
    }

    #[test]
    fn farkli_konum_jokere_donusur() {
        let sonuc = kumele(
            Ayarlar::default(),
            &[
                "Zamanlanmış görev başladı: kurlar:guncelle",
                "Zamanlanmış görev başladı: yedek:al",
            ],
        );
        assert_eq!(sonuc, ["2 × Zamanlanmış görev başladı: <*>"]);
    }

    #[test]
    fn farkli_uzunluktaki_mesajlar_karismaz() {
        let sonuc = kumele(
            Ayarlar::default(),
            &[
                "Sipariş <SAYI> oluşturuldu",
                "Sipariş <SAYI> için ödeme alındı",
            ],
        );
        assert_eq!(
            sonuc,
            [
                "1 × Sipariş <SAYI> oluşturuldu",
                "1 × Sipariş <SAYI> için ödeme alındı"
            ]
        );
    }

    #[test]
    fn benzerlik_esigi_birlesmeyi_belirler() {
        // 16 belirtecin 10'u aynı: benzerlik 10/16 = 0,625
        let sorgular = [
            "Yavaş sorgu (<SAYI> ms): select * from `urunler` where `kategori_id` = <SAYI> and `stok` > <SAYI>",
            "Yavaş sorgu (<SAYI> ms): select * from `siparisler` where `kullanici_id` = <SAYI> order by `created_at` desc",
        ];
        // Varsayılan eşik 0,7: iki farklı sorgu ayrı kalır
        assert_eq!(kumele(Ayarlar::default(), &sorgular).len(), 2);

        // Eşik 0,6: birleşirler ama kalıp neredeyse anlamsızlaşır
        let gevsek = Ayarlar {
            benzerlik_esigi: 0.6,
            ..Ayarlar::default()
        };
        assert_eq!(
            kumele(gevsek, &sorgular),
            ["2 × Yavaş sorgu (<SAYI> ms): select * from <*> where <*> = <SAYI> <*> <*> <*> <*>"]
        );
    }

    #[test]
    fn ilk_belirteci_farkli_mesajlar_karsilastirilmaz() {
        // 3 belirtecin 2'si aynı (0,67) ve eşik düşük; ama farklı dallarda oldukları için
        // hiç karşılaştırılmazlar
        let dusuk_esik = Ayarlar {
            benzerlik_esigi: 0.3,
            ..Ayarlar::default()
        };
        let sonuc = kumele(
            dusuk_esik,
            &["Redis bağlantısı koptu", "MySQL bağlantısı koptu"],
        );
        assert_eq!(sonuc.len(), 2);
    }

    #[test]
    fn onek_derinligi_ikinci_belirteci_de_kullanir() {
        // Laravel mesajlarında değişken çoğu zaman ikinci sırada olur
        let mesajlar = [
            "View [emails.siparis] not found.",
            "View [emails.iade] not found.",
        ];
        // Derinlik 1: aynı yaprakta karşılaştırılırlar, 3/4 = 0,75 → birleşirler
        assert_eq!(
            kumele(Ayarlar::default(), &mesajlar),
            ["2 × View <*> not found."]
        );

        // Derinlik 2: ikinci belirteç de dal seçer; farklı yapraklara düşüp hiç karşılaşmazlar
        let derin = Ayarlar {
            onek_derinligi: 2,
            ..Ayarlar::default()
        };
        assert_eq!(kumele(derin, &mesajlar).len(), 2);
    }

    #[test]
    fn rakamli_belirtec_joker_dalina_gider() {
        // "a1b2" ve "c3d4" kendi dallarını açmaz; ikisi de joker dalında buluşup birleşir
        let sonuc = kumele(
            Ayarlar::default(),
            &[
                "a1b2 dosyası başarıyla silindi",
                "c3d4 dosyası başarıyla silindi",
            ],
        );
        assert_eq!(sonuc, ["2 × <*> dosyası başarıyla silindi"]);
    }

    #[test]
    fn dolu_dugumde_yeni_belirtec_joker_dalina_gider() {
        // En fazla 2 çocuk: biri "a" için, son yer joker dalına ayrılır
        let ayarlar = Ayarlar {
            en_fazla_cocuk: 2,
            ..Ayarlar::default()
        };
        let sonuc = kumele(ayarlar, &["a x y z", "b x y z", "c x y z"]);
        assert_eq!(sonuc, ["1 × a x y z", "2 × <*> x y z"]);
    }

    #[test]
    fn bos_mesajlar_tek_kumede_toplanir() {
        let mut drain = Drain::new(Ayarlar::default());
        drain.ekle("");
        drain.ekle("   ");
        assert_eq!(drain.kumeler().len(), 1);
        assert_eq!(drain.kumeler()[0].adet, 2);
    }

    #[test]
    fn ornek_logdaki_kaliplar() {
        let log = kayitlari_ayir(include_str!("../ornekler/laravel.log"));
        let maskeli: Vec<_> = log
            .kayitlar
            .iter()
            .map(|kayit| maskele(kayit.mesaj))
            .collect();
        let mesajlar: Vec<&str> = maskeli.iter().map(|mesaj| mesaj.as_ref()).collect();
        let kaliplar = kumele(Ayarlar::default(), &mesajlar);

        // Maskelemenin birleştiremediklerini Drain birleştirir
        assert!(
            kaliplar
                .iter()
                .any(|k| k == "2 × Zamanlanmış görev başladı: <*>")
        );
        // İsimler farklı (Ayşe, Zeynep, Mehmet): "(Ayşe," belirtecinin tamamı joker olur
        assert!(
            kaliplar.iter().any(
                |k| k.starts_with("3 × SQLSTATE[<SAYI>]") && k.contains("values <*> <EPOSTA>,")
            )
        );
        // Sözdizimi aynı olduğu için giriş ve çıkış da birleşir (3/4 = 0,75)
        assert!(
            kaliplar
                .iter()
                .any(|k| k == "9 × Kullanıcı <SAYI> <*> yaptı")
        );
        // Farklı yavaş sorgular ayrı kalır
        assert_eq!(
            kaliplar
                .iter()
                .filter(|k| k.contains("Yavaş sorgu"))
                .count(),
            3
        );
    }

    #[test]
    fn rakamli_degisken_kisa_mesajda_birlesir() {
        // "120ms" maskelenmez (birime yapışık); her değer ayrı kalıp olmamalı
        let sonuc = kumele(
            Ayarlar::default(),
            &[
                "Yanıt süresi 120ms",
                "Yanıt süresi 95ms",
                "Yanıt süresi 300ms",
            ],
        );
        assert_eq!(sonuc, ["3 × Yanıt süresi <*>"]);
    }

    #[test]
    fn rakamsiz_fark_kisa_mesajda_birlesmez() {
        // "alındı" ile "reddedildi" farklı olaylar; rakam yok, eşik gevşememeli
        let sonuc = kumele(Ayarlar::default(), &["Ödeme alındı", "Ödeme reddedildi"]);
        assert_eq!(sonuc.len(), 2);
    }

    #[test]
    fn rakamli_sabit_kelime_esigi_gevsetmez() {
        // "S3" iki mesajda da aynı; farklı olan kelimede rakam yok, eşik gevşememeli
        let sonuc = kumele(
            Ayarlar::default(),
            &["S3 yüklemesi tamamlandı", "S3 yüklemesi başarısız"],
        );
        assert_eq!(sonuc.len(), 2);
    }

    #[test]
    fn rakama_duyarli_esik_kapatilabilir() {
        // Kapalıyken klasik Drain: 2/3 = 0,67 < 0,7 olduğu için her değer ayrı kalır
        let klasik = Ayarlar {
            rakama_duyarli_esik: false,
            ..Ayarlar::default()
        };
        let sonuc = kumele(klasik, &["Yanıt süresi 120ms", "Yanıt süresi 95ms"]);
        assert_eq!(sonuc.len(), 2);
    }
}
