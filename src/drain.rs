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
//! **Esnek eşik.** 4. adımdaki eşik, büyük ihtimalle değişken olan farklılıkların oranında
//! gevşer: eşik × (n − değişken fark) / n. İki tür farklılık değişken sayılır:
//!
//! - *İki tarafında da rakam olan farklılıklar.* Fikir Drain'in genişletilmiş sürümünden gelir
//!   (He ve ark., "A Directed Acyclic Graph Approach to Online Log Parsing", 2018). Böylece
//!   `Yanıt süresi 120ms` ile `Yanıt süresi 95ms` gibi kısa mesajlar da birleşebilir. Rakam
//!   yalnızca bir taraftaysa fark değişken sayılmaz: `Yanıt süresi uzun` başka bir olaydır.
//! - *Değişken konumlar.* Rakamsız bir değer (kullanıcı adı, ürün adresi) kısa mesajları
//!   birleştirmez: `Bilinmeyen kullanıcı: ahmet` ile `Bilinmeyen kullanıcı: mehmet` üç
//!   belirtecin ikisinde aynıdır (0,67 < 0,7). Her değer ayrı bir küme olur ve yaprak büyüdükçe
//!   her yeni mesaj bütün kümelerle karşılaştırılır: süre mesaj sayısının karesiyle artar.
//!   3. adımdaki dolu düğüm kuralı yaprakta da uygulanır: kümelerin kalıplarında bir konum
//!   `en_fazla_cocuk`'tan fazla farklı belirteç aldıysa o konum değişkendir. `kumele` böyle bir
//!   konum bulursa mesajları bu bilgiyle bir kez daha kümeler; konum değişken sayılmadan önce
//!   açılmış kümeler de böylece birleşir.
//!
//! Gevşeyen eşiğin iki sınırı vardır. Değişken konumlar tek belirteçli mesajlarda aranmaz:
//! orada konum mesajın kendisidir ve bütün tek kelimelik mesajlar (`Unauthenticated.`,
//! `odeme.basarisiz`) aynı yaprakta durur; yüzden fazla farklı olay adı onları değişken yapmaz.
//! Ve hiçbir belirteci tutmayan bir birleşme ancak bütün farklılıkların iki tarafında da rakam
//! varsa olur: tutunacak tek yeri bir değişken konum olan iki mesaj birleşmez.
//!
//! Kümelerin eşikleri böylece farklı olabilir. Mesaj, eşiği geçen kümelerin en benzerine
//! katılır: en benzer küme eşiği geçmese de bir başkası geçebilir. Klasik Drain'de eşik
//! herkes için aynıdır; en benzer küme geçemiyorsa hiçbiri geçemez ve iki kural aynı sonucu
//! verir.
//!
//! **Joker.** Kalıplardaki joker, içinde boşluk olan bir iç işarettir. Belirteçler boşluklardan
//! bölündüğü için hiçbir belirteçle karışmaz: mesajda yazılı bir `<*>` sıradan bir kelimedir ve
//! birebir aynı mesajlar birleşir. Kalıp gösterilirken joker `<*>` diye yazılır.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};

/// Kalıpta değişken bir konumun iç işareti. Boşluk içerdiği için mesajdaki hiçbir belirteçle
/// karışmaz. Gösterilen hali `JOKER_GORUNUMU`.
const JOKER: &str = "< * >";

/// Jokerin kalıp gösterilirken yazılan hali: `Zamanlanmış görev başladı: <*>`
const JOKER_GORUNUMU: &str = "<*>";

/// Drain'in ayarları.
#[derive(Debug, Clone)]
pub struct Ayarlar {
    /// Ağaçta mesajın kaç baş belirtecine göre dallanılacağı.
    pub onek_derinligi: usize,
    /// Bir mesajın kümeye katılması için gereken en düşük benzerlik (0 ile 1 arası).
    pub benzerlik_esigi: f64,
    /// Bir düğümün en fazla çocuk sayısı; dolunca yeni belirteçler `<*>` dalına gider. Bir
    /// yaprakta bundan fazla farklı belirteç alan konum değişken sayılır.
    pub en_fazla_cocuk: usize,
    /// Eşik, büyük ihtimalle değişken olan farklılıkların oranında gevşesin mi? Kapalıyken
    /// davranış klasik Drain (ve Drain3) ile aynıdır.
    pub esnek_esik: bool,
}

impl Default for Ayarlar {
    fn default() -> Self {
        Ayarlar {
            onek_derinligi: 1,
            benzerlik_esigi: 0.7,
            en_fazla_cocuk: 100,
            esnek_esik: true,
        }
    }
}

/// Aynı kalıba uyan mesajlar.
#[derive(Debug)]
pub struct Kume {
    /// Kalıbın belirteçleri; değişen konumlarda iç joker işareti. Gösterilecek hali `metin`.
    kalip: Vec<String>,
    /// Kümeye katılan mesaj sayısı.
    pub adet: usize,
}

impl Kume {
    /// Kalıbın okunabilir hali: "Zamanlanmış görev başladı: <*>"
    pub fn metin(&self) -> String {
        let belirtecler: Vec<&str> = self
            .kalip
            .iter()
            .map(|belirtec| {
                if belirtec == JOKER {
                    JOKER_GORUNUMU
                } else {
                    belirtec
                }
            })
            .collect();
        belirtecler.join(" ")
    }
}

/// Ağacın bir düğümü.
#[derive(Debug, Default)]
struct Dugum {
    /// Belirteç → alt düğüm.
    cocuklar: HashMap<String, Dugum>,
    /// Sadece yapraklarda dolu: bu yaprağa düşen kümelerin numaraları.
    kume_numaralari: Vec<usize>,
    /// Sadece yapraklarda ve esnek eşikte dolu: kümelerin kalıplarındaki her konum. Yaprakta
    /// `en_fazla_cocuk`'tan fazla küme olunca (ya da önceki turda değişken konumu
    /// öğrenildiyse) dolar; o zamana kadar boştur.
    konumlar: Vec<Konum>,
}

/// Bir yapraktaki kümelerin kalıplarında bir konum.
#[derive(Debug)]
enum Konum {
    /// Şimdiye kadar görülen farklı belirteçlerin özetleri. Belirteçlerin kendisi saklanmaz:
    /// yalnızca kaç farklı değer görüldüğü sayılır.
    Degerler(HashSet<u64>),
    /// `en_fazla_cocuk`'tan fazla farklı belirteç görüldü: bu konum değişkendir.
    Degisken,
}

impl Konum {
    fn degisken_mi(&self) -> bool {
        matches!(self, Konum::Degisken)
    }
}

/// Bir yaprağın ağaçtaki yeri: belirteç sayısı ve köke inerken seçilen dallar.
type YaprakYolu = (usize, Vec<String>);

/// Drain kalıp çıkarıcısı. Dışarıdan `kumele` ile kullanılır.
#[derive(Debug)]
struct Drain {
    ayarlar: Ayarlar,
    /// Ağacın ilk katmanı: belirteç sayısı → o uzunluktaki mesajların alt ağacı.
    uzunluklar: HashMap<usize, Dugum>,
    /// Bütün kümeler. Ağaç kümelere bu listedeki sıra numarasıyla (indeks) işaret eder.
    kumeler: Vec<Kume>,
    /// Değişken konumu bulunan yapraklar: her konumun değişken olup olmadığı.
    degiskenler: HashMap<YaprakYolu, Vec<bool>>,
    /// Önceki turda öğrenilen değişken konumlar: yeni açılan bir yaprak bunlarla başlar.
    ogrenilen: HashMap<YaprakYolu, Vec<bool>>,
    /// Tek belirteçli kümeler: kümeyi açan belirteç → küme numarası (`tek_belirtecli_adaylar`).
    tek_belirtecliler: HashMap<String, usize>,
    /// Esnek eşikte rakamlı tek belirteçli mesajların kümesi. Rakamlı iki tek belirteç
    /// birbirine katıldığı için en fazla bir tane olur.
    rakamli_tek: Option<usize>,
}

/// Mesajları sırayla kümeler. Kümeleri (ilk görüldükleri sırayla) ve her mesajın küme
/// numarasını (mesajlarla aynı sırada) döndürür.
///
/// Kümeleme sırasında değişken bir konum bulunursa mesajlar bu bilgiyle bir kez daha, baştan
/// kümelenir. Değişken konum yoksa (logların çoğunda) ilk turun sonucu döner.
pub fn kumele(ayarlar: &Ayarlar, mesajlar: &[impl AsRef<str>]) -> (Vec<Kume>, Vec<usize>) {
    let mut ilk_tur = Drain::new(ayarlar.clone());
    let numaralar = mesajlar
        .iter()
        .map(|mesaj| ilk_tur.ekle(mesaj.as_ref()))
        .collect();
    if ilk_tur.degiskenler.is_empty() {
        return (ilk_tur.kumeler, numaralar);
    }
    // Yapı güncelleme sözdizimi: `ogrenilen` dışındaki alanlar yeni bir Drain'den gelir
    let mut ikinci_tur = Drain {
        ogrenilen: ilk_tur.degiskenler,
        ..Drain::new(ayarlar.clone())
    };
    let numaralar = mesajlar
        .iter()
        .map(|mesaj| ikinci_tur.ekle(mesaj.as_ref()))
        .collect();
    (ikinci_tur.kumeler, numaralar)
}

impl Drain {
    fn new(ayarlar: Ayarlar) -> Self {
        Drain {
            ayarlar,
            uzunluklar: HashMap::new(),
            kumeler: Vec::new(),
            degiskenler: HashMap::new(),
            ogrenilen: HashMap::new(),
            tek_belirtecliler: HashMap::new(),
            rakamli_tek: None,
        }
    }

    /// Mesajı benzediği kümeye ekler, yoksa yeni küme açar. Kümenin numarasını döndürür.
    fn ekle(&mut self, mesaj: &str) -> usize {
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

        // 4. Yapraktaki en benzer küme
        if let [belirtec] = belirtecler
            && self.ayarlar.benzerlik_esigi > 0.0
        {
            let adaylar = self.tek_belirtecli_adaylar(belirtec);
            self.en_benzer(&adaylar, belirtecler, &dugum.konumlar)
        } else {
            self.en_benzer(&dugum.kume_numaralari, belirtecler, &dugum.konumlar)
        }
    }

    /// Tek belirteçli bir mesajın katılabileceği kümeler: kalıbı birebir aynı olan ve rakamlı
    /// tek belirteçlilerin kümesi (yalnızca esnek eşikte vardır).
    ///
    /// Bütün tek belirteçli mesajlar aynı yapraktadır. Eşik 0'dan büyükken yapraktaki öteki
    /// kümelerin hiçbiri eşiği geçemez: kalıpları mesajdan farklı tek bir belirteçtir, ortak
    /// belirteç yoktur. Esnek eşikte fark yalnızca iki tarafta da rakam varsa değişken sayılır
    /// ve rakamlı tek belirteçliler tek bir kümede toplanır. Böylece binlerce farklı tek
    /// kelimelik mesaj olsa da her mesaj en fazla iki kümeyle karşılaştırılır.
    fn tek_belirtecli_adaylar(&self, belirtec: &str) -> Vec<usize> {
        self.tek_belirtecliler
            .get(belirtec)
            .copied()
            .into_iter()
            .chain(self.rakamli_tek)
            .collect()
    }

    /// Adaylardan, eşiği geçenlerin en benzeri. Eşitlikte daha çok jokeri olan, yani daha genel
    /// kalıp seçilir; o da eşitse önce açılan.
    ///
    /// Bir yapraktaki bütün kalıplar aynı uzunlukta olduğu için eşit belirteç sayısını
    /// karşılaştırmak, oranları karşılaştırmakla aynı şey.
    fn en_benzer(
        &self,
        adaylar: &[usize],
        belirtecler: &[&str],
        konumlar: &[Konum],
    ) -> Option<usize> {
        let mut en_iyi: Option<(usize, (usize, usize))> = None;
        for &numara in adaylar {
            let sonuc = karsilastir(&self.kumeler[numara].kalip, belirtecler, konumlar);
            if !self.esigi_gecer(&sonuc, belirtecler.len()) {
                continue;
            }
            let anahtar = (sonuc.esit, sonuc.joker);
            if en_iyi.is_none_or(|(_, en_iyi_anahtar)| anahtar > en_iyi_anahtar) {
                en_iyi = Some((numara, anahtar));
            }
        }
        en_iyi.map(|(numara, _)| numara)
    }

    /// Karşılaştırılan küme eşiği geçiyor mu? `uzunluk`: mesajın belirteç sayısı (n).
    fn esigi_gecer(&self, sonuc: &Karsilastirma, uzunluk: usize) -> bool {
        let esik = self.ayarlar.benzerlik_esigi;
        let n = uzunluk as f64;
        let benzerlik = sonuc.esit as f64 / n;
        if !self.ayarlar.esnek_esik {
            return benzerlik >= esik;
        }
        // Değişken farklılıklar büyük ihtimalle değişkendir; eşik onların oranında gevşer:
        // eşik × (n − değişken fark) / n
        let gevsek_esik = esik * (n - sonuc.degisken_fark as f64) / n;
        // Hiçbir belirteci tutmayan bir birleşme ancak bütün farklılıkların iki tarafında da
        // rakam varsa olur. Eşik 0'sa klasik Drain'deki gibi her şey birleşir.
        let tutunacak_yer = sonuc.esit > 0 || sonuc.rakamli_fark == uzunluk || esik <= 0.0;
        benzerlik >= gevsek_esik && tutunacak_yer
    }

    /// Yeni kümeyi ağaca yerleştirir; yol üzerindeki eksik düğümleri oluşturur. Esnek eşikte
    /// yaprağın konumlarını da günceller.
    fn agaca_ekle(&mut self, belirtecler: &[&str], numara: usize) {
        let yonlendiren = self.yonlendiren_sayisi(belirtecler.len());
        let en_fazla_cocuk = self.ayarlar.en_fazla_cocuk;

        if let [belirtec] = belirtecler {
            self.tek_belirtecliler
                .entry(belirtec.to_string())
                .or_insert(numara);
            if self.ayarlar.esnek_esik && rakam_iceriyor(belirtec) && self.rakamli_tek.is_none() {
                self.rakamli_tek = Some(numara);
            }
        }

        let mut dallar = Vec::with_capacity(yonlendiren);
        let mut dugum = self.uzunluklar.entry(belirtecler.len()).or_default();
        for belirtec in &belirtecler[..yonlendiren] {
            let anahtar = dal_anahtari(&dugum.cocuklar, belirtec, en_fazla_cocuk);
            dallar.push(anahtar.clone());
            dugum = dugum.cocuklar.entry(anahtar).or_default();
        }
        dugum.kume_numaralari.push(numara);
        // Değişken konumlar tek belirteçli mesajlarda aranmaz: orada konum mesajın kendisidir
        // ve bütün tek kelimelik mesajlar aynı yapraktadır
        if !self.ayarlar.esnek_esik || belirtecler.len() < 2 {
            return;
        }

        let yol = (belirtecler.len(), dallar);
        let kume_sayisi = dugum.kume_numaralari.len();
        // Yeni açılan yaprak: önceki turda değişken konumu öğrenildiyse onlarla başlar
        if kume_sayisi == 1
            && let Some(degiskenler) = self.ogrenilen.get(&yol)
        {
            dugum.konumlar = degiskenler
                .iter()
                .map(|&degisken| {
                    if degisken {
                        Konum::Degisken
                    } else {
                        Konum::Degerler(HashSet::new())
                    }
                })
                .collect();
        }
        // Bir konum, yaprakta `en_fazla_cocuk`'tan fazla küme olmadan o kadar farklı belirteç
        // alamaz. Sayım ancak o zaman başlar; yaprakların çoğunda hiç yapılmaz.
        if kume_sayisi <= en_fazla_cocuk {
            return;
        }
        let sayilacaklar = if kume_sayisi == en_fazla_cocuk + 1 {
            // Sayım şimdi başlıyor: yapraktaki bütün kalıplar sayılır
            if dugum.konumlar.is_empty() {
                dugum.konumlar = (0..belirtecler.len())
                    .map(|_| Konum::Degerler(HashSet::new()))
                    .collect();
            }
            &dugum.kume_numaralari[..]
        } else {
            // Sayım sürüyor: yalnızca yeni küme
            &dugum.kume_numaralari[kume_sayisi - 1..]
        };
        let mut yeni_degisken = false;
        for &kume in sayilacaklar {
            for (konum, belirtec) in dugum.konumlar.iter_mut().zip(&self.kumeler[kume].kalip) {
                if let Konum::Degerler(degerler) = konum
                    && belirtec != JOKER
                {
                    degerler.insert(ozet(belirtec));
                    // Değerler artık tutulmaz: konumun değişken olduğunu bilmek yeter
                    if degerler.len() > en_fazla_cocuk {
                        *konum = Konum::Degisken;
                        yeni_degisken = true;
                    }
                }
            }
        }
        if yeni_degisken {
            let degiskenler = dugum.konumlar.iter().map(Konum::degisken_mi).collect();
            self.degiskenler.insert(yol, degiskenler);
        }
    }

    /// Kaç baş belirtece göre dallanılacağı. Son belirteç dallanmaya hiç katılmaz;
    /// böylece benzerlik karşılaştırmasına her zaman en az bir belirteç kalır.
    fn yonlendiren_sayisi(&self, uzunluk: usize) -> usize {
        self.ayarlar.onek_derinligi.min(uzunluk.saturating_sub(1))
    }
}

/// Belirtecin 64 bitlik özeti (hash). Farklı değerleri saymak için belirtecin kendisini
/// kopyalamak gerekmez; çakışma ihtimali (yüz değer için ~10⁻¹⁵) sayımı etkilemez.
fn ozet(belirtec: &str) -> u64 {
    let mut ozetleyici = DefaultHasher::new();
    belirtec.hash(&mut ozetleyici);
    ozetleyici.finish()
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

/// İçinde en az bir rakam var mı? "120ms", "a1b2", "S3" → evet; "<SAYI>", "giriş" → hayır.
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
    /// Farklı olan (ya da kalıpta joker olan) ve büyük ihtimalle değişken olan konumlar: iki
    /// tarafında da rakam var (jokerde mesajın tarafında) ya da konum yaprakta değişken.
    degisken_fark: usize,
    /// Değişken farkların, rakamdan dolayı değişken olanları.
    rakamli_fark: usize,
}

/// Kalıp ile mesajı konum konum karşılaştırır. `konumlar`: yaprağın konumları; esnek eşik
/// kapalıyken boştur.
///
/// Joker, kalıbın o konumda farklı değerler gördüğünü söyler; o değerlerin rakamlı olup
/// olmadığı bilinmez. Jokerin karşısındaki belirteçte rakam varsa fark rakamlı sayılır.
fn karsilastir(kalip: &[String], belirtecler: &[&str], konumlar: &[Konum]) -> Karsilastirma {
    let mut sonuc = Karsilastirma {
        esit: 0,
        joker: 0,
        degisken_fark: 0,
        rakamli_fark: 0,
    };
    for (sira, (k, b)) in kalip.iter().zip(belirtecler).enumerate() {
        let rakamli = if k == JOKER {
            sonuc.joker += 1;
            rakam_iceriyor(b)
        } else if k == b {
            sonuc.esit += 1;
            continue;
        } else {
            rakam_iceriyor(k) && rakam_iceriyor(b)
        };
        if rakamli {
            sonuc.rakamli_fark += 1;
        }
        if rakamli || konumlar.get(sira).is_some_and(Konum::degisken_mi) {
            sonuc.degisken_fark += 1;
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
    fn kumele_tek_tur(ayarlar: Ayarlar, mesajlar: &[&str]) -> Vec<String> {
        let mut drain = Drain::new(ayarlar);
        for mesaj in mesajlar {
            drain.ekle(mesaj);
        }
        ozetle(&drain.kumeler)
    }

    /// Kümeler "adet × kalıp" olarak, açılış sırasıyla.
    fn ozetle(kumeler: &[Kume]) -> Vec<String> {
        kumeler
            .iter()
            .map(|kume| format!("{} × {}", kume.adet, kume.metin()))
            .collect()
    }

    /// Sıra numarasından rakamsız, üç harfli bir ad: 0 → "aaa", 1 → "aab", 27 → "abb"
    fn ad(sira: usize) -> String {
        [sira / 676, sira / 26, sira]
            .iter()
            .map(|basamak| char::from(b'a' + (basamak % 26) as u8))
            .collect()
    }

    /// `adet` farklı kullanıcı için "Bilinmeyen kullanıcı: <ad>" mesajları.
    fn bilinmeyen_kullanicilar(adet: usize) -> Vec<String> {
        (0..adet)
            .map(|sira| format!("Bilinmeyen kullanıcı: {}", ad(sira)))
            .collect()
    }

    #[test]
    fn ayni_mesajlar_tek_kumede_toplanir() {
        let sonuc = kumele_tek_tur(Ayarlar::default(), &["Sepet önbellekten okundu"; 3]);
        assert_eq!(sonuc, ["3 × Sepet önbellekten okundu"]);
    }

    #[test]
    fn farkli_konum_jokere_donusur() {
        let sonuc = kumele_tek_tur(
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
        let sonuc = kumele_tek_tur(
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
        assert_eq!(kumele_tek_tur(Ayarlar::default(), &sorgular).len(), 2);

        // Eşik 0,6: birleşirler ama kalıp neredeyse anlamsızlaşır
        let gevsek = Ayarlar {
            benzerlik_esigi: 0.6,
            ..Ayarlar::default()
        };
        assert_eq!(
            kumele_tek_tur(gevsek, &sorgular),
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
        let sonuc = kumele_tek_tur(
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
            kumele_tek_tur(Ayarlar::default(), &mesajlar),
            ["2 × View <*> not found."]
        );

        // Derinlik 2: ikinci belirteç de dal seçer; farklı yapraklara düşüp hiç karşılaşmazlar
        let derin = Ayarlar {
            onek_derinligi: 2,
            ..Ayarlar::default()
        };
        assert_eq!(kumele_tek_tur(derin, &mesajlar).len(), 2);
    }

    #[test]
    fn rakamli_belirtec_joker_dalina_gider() {
        // "a1b2" ve "c3d4" kendi dallarını açmaz; ikisi de joker dalında buluşup birleşir
        let sonuc = kumele_tek_tur(
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
        let sonuc = kumele_tek_tur(ayarlar, &["a x y z", "b x y z", "c x y z"]);
        assert_eq!(sonuc, ["1 × a x y z", "2 × <*> x y z"]);
    }

    #[test]
    fn bos_mesajlar_tek_kumede_toplanir() {
        let mut drain = Drain::new(Ayarlar::default());
        drain.ekle("");
        drain.ekle("   ");
        assert_eq!(drain.kumeler.len(), 1);
        assert_eq!(drain.kumeler[0].adet, 2);
    }

    #[test]
    fn ornek_logdaki_kaliplar() {
        let log = kayitlari_ayir(include_str!("../ornekler/laravel.log"));
        let maskeli: Vec<_> = log
            .kayitlar
            .iter()
            .map(|kayit| maskele(kayit.mesaj()))
            .collect();
        let mesajlar: Vec<&str> = maskeli.iter().map(|mesaj| mesaj.as_ref()).collect();
        let kaliplar = kumele_tek_tur(Ayarlar::default(), &mesajlar);

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

        // Hiçbir konum değişken sayılmaz: iki turlu kümeleme tek turla aynı sonucu verir
        let (kumeler, numaralar) = kumele(&Ayarlar::default(), &mesajlar);
        assert_eq!(ozetle(&kumeler), kaliplar);
        assert_eq!(numaralar.len(), mesajlar.len());
    }

    #[test]
    fn rakamli_degisken_kisa_mesajda_birlesir() {
        // "120ms" maskelenmez (birime yapışık); her değer ayrı kalıp olmamalı
        let sonuc = kumele_tek_tur(
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
        let sonuc = kumele_tek_tur(Ayarlar::default(), &["Ödeme alındı", "Ödeme reddedildi"]);
        assert_eq!(sonuc.len(), 2);
    }

    #[test]
    fn rakamli_sabit_kelime_esigi_gevsetmez() {
        // "S3" iki mesajda da aynı; farklı olan kelimede rakam yok, eşik gevşememeli
        let sonuc = kumele_tek_tur(
            Ayarlar::default(),
            &["S3 yüklemesi tamamlandı", "S3 yüklemesi başarısız"],
        );
        assert_eq!(sonuc.len(), 2);
    }

    #[test]
    fn yuzden_fazla_degeri_olan_konum_degiskendir() {
        // 100 farklı ad: sınır aşılmadı, her ad ayrı kalıp
        let mesajlar = bilinmeyen_kullanicilar(101);
        let (kumeler, _) = kumele(&Ayarlar::default(), &mesajlar[..100]);
        assert_eq!(kumeler.len(), 100);

        // 101. ad konumu değişken yapar. İkinci turda bu bilinir: ilk 100 ad da birleşir.
        // Başka bir yaprak (Ödeme) etkilenmez.
        let mut hepsi = mesajlar.clone();
        hepsi.extend(["Ödeme alındı".to_string(), "Ödeme reddedildi".to_string()]);
        let (kumeler, numaralar) = kumele(&Ayarlar::default(), &hepsi);
        assert_eq!(
            ozetle(&kumeler),
            [
                "101 × Bilinmeyen kullanıcı: <*>",
                "1 × Ödeme alındı",
                "1 × Ödeme reddedildi"
            ]
        );
        assert!(numaralar[..101].iter().all(|&numara| numara == 0));
        assert_eq!(numaralar[101..], [1, 2]);
    }

    #[test]
    fn sinir_yuzden_fazla_farkli_degerdir() {
        // 101 küme ama son konumda tam 100 farklı değer: "aaa" iki kümede geçiyor (ikincisi
        // ortadaki kelimeyle ayrılıyor). 100'ü aşmadığı için konum değişken sayılmaz.
        let mut mesajlar = bilinmeyen_kullanicilar(100);
        mesajlar.push("Bilinmeyen kişi: aaa".to_string());
        let (kumeler, _) = kumele(&Ayarlar::default(), &mesajlar);
        assert_eq!(kumeler.len(), 101);
        // Bir ad daha: 101 farklı değer, konum değişken
        mesajlar.push(format!("Bilinmeyen kullanıcı: {}", ad(100)));
        let (kumeler, _) = kumele(&Ayarlar::default(), &mesajlar);
        assert_eq!(
            ozetle(&kumeler),
            [
                "101 × Bilinmeyen kullanıcı: <*>",
                "1 × Bilinmeyen kişi: aaa"
            ]
        );
    }

    #[test]
    fn degisken_konum_yapragin_buyumesini_durdurur() {
        // Tek turda: ilk 101 ad ayrı kümeler açar, sonrakiler değişken konum sayesinde ilk
        // kümede birleşir. Yaprak büyümez; her yeni mesaj en fazla 101 kümeyle karşılaştırılır.
        // Kural olmasa 5 000 küme açılır, son mesaj 4 999 kümeyle karşılaştırılırdı.
        let mut drain = Drain::new(Ayarlar::default());
        for mesaj in bilinmeyen_kullanicilar(5_000) {
            drain.ekle(&mesaj);
        }
        let kumeler = &drain.kumeler;
        assert_eq!(kumeler.len(), 101);
        assert_eq!(kumeler[0].metin(), "Bilinmeyen kullanıcı: <*>");
        assert_eq!(kumeler[0].adet, 4_900);
        assert!(kumeler[1..].iter().all(|kume| kume.adet == 1));
    }

    #[test]
    fn degisken_konum_esige_rakamli_fark_gibi_katilir() {
        // Sınır 2 farklı değer: üçüncü ad konumu değişken yapar. İkinci turda "ahmet"in
        // kümesine katılan "veli"nin farkı değişken sayılır: eşik 0,7 × 2/3 = 0,47 ≤ 0,67
        let ayarlar = Ayarlar {
            en_fazla_cocuk: 2,
            ..Ayarlar::default()
        };
        let mesajlar = [
            "Bilinmeyen kullanıcı: ahmet",
            "Bilinmeyen kullanıcı: ayse",
            "Bilinmeyen kullanıcı: veli",
        ];
        let (kumeler, numaralar) = kumele(&ayarlar, &mesajlar);
        assert_eq!(ozetle(&kumeler), ["3 × Bilinmeyen kullanıcı: <*>"]);
        assert_eq!(numaralar, [0, 0, 0]);
        // Tek turda ilk iki ad konum değişken sayılmadan önce ayrı kümeler açar
        assert_eq!(kumele_tek_tur(ayarlar, &mesajlar).len(), 3);
    }

    #[test]
    fn esnek_esik_kapatilabilir() {
        // Kapalıyken klasik Drain: 2/3 = 0,67 < 0,7 olduğu için her değer ayrı kalır
        let klasik = Ayarlar {
            esnek_esik: false,
            ..Ayarlar::default()
        };
        let sonuc = kumele_tek_tur(klasik.clone(), &["Yanıt süresi 120ms", "Yanıt süresi 95ms"]);
        assert_eq!(sonuc.len(), 2);
        // Değişken konum da yok: 101 ad 101 kalıp
        let (kumeler, _) = kumele(&klasik, &bilinmeyen_kullanicilar(101));
        assert_eq!(kumeler.len(), 101);
    }

    #[test]
    fn adlar_rakamsiz_ve_farkli() {
        let adlar: HashSet<String> = (0..17_576).map(ad).collect();
        assert_eq!(adlar.len(), 17_576);
        assert!(adlar.iter().all(|ad| !rakam_iceriyor(ad)));
        assert_eq!(
            (ad(0), ad(1), ad(27)),
            ("aaa".into(), "aab".into(), "abb".into())
        );
    }

    /// `adet` farklı, rakamsız, tek kelimelik olay adı: "olay.aaa", "olay.aab"...
    fn olaylar(adet: usize) -> Vec<String> {
        (0..adet).map(|sira| format!("olay.{}", ad(sira))).collect()
    }

    #[test]
    fn tek_kelimelik_mesajlarda_konum_degisken_sayilmaz() {
        // Bütün tek kelimelik mesajlar aynı yapraktadır. Yüzden fazla farklı olay adı onları
        // birleştirmemeli; birleştirseydi aralarındaki iki ayrı olay da kaybolurdu.
        let mut mesajlar = olaylar(101);
        mesajlar.extend(["VeritabaniCoktu".to_string(), "DiskDolu".to_string()]);
        let (kumeler, numaralar) = kumele(&Ayarlar::default(), &mesajlar);
        assert_eq!(kumeler.len(), 103);
        assert_eq!(numaralar, (0..103).collect::<Vec<_>>());
        // Konumlar hiç sayılmaz: ikinci tura da gerek kalmaz
        let mut drain = Drain::new(Ayarlar::default());
        for mesaj in &mesajlar {
            drain.ekle(mesaj);
        }
        assert!(drain.degiskenler.is_empty());
    }

    #[test]
    fn esitlikte_daha_cok_jokeri_olan_secilir() {
        // Son mesaj iki kümeyle de iki belirteç paylaşır (eşik 0,5). Drain3 gibi daha genel
        // kalıbı, yani daha çok jokeri olanı seçer; önce açılan küme bu yüzden değişmez.
        // Sonuç Drain3 0.9.11 ile aynıdır (depth=4, sim_th=0,5).
        let ayarlar = Ayarlar {
            benzerlik_esigi: 0.5,
            esnek_esik: false,
            ..Ayarlar::default()
        };
        let mesajlar = ["a b c e", "a x y d", "a z w d", "a b q d"];
        assert_eq!(
            kumele_tek_tur(ayarlar.clone(), &mesajlar),
            ["1 × a b c e", "3 × a <*> <*> d"]
        );
        // Esnek eşikte de (farklarda rakam yok)
        let esnek = Ayarlar {
            esnek_esik: true,
            ..ayarlar
        };
        assert_eq!(
            kumele_tek_tur(esnek, &mesajlar),
            ["1 × a b c e", "3 × a <*> <*> d"]
        );
    }

    #[test]
    fn esigi_gecenlerin_en_benzeri_secilir() {
        // Üçüncü mesaj ilk kümeyle iki belirteci paylaşır ama farkları değişken değil: eşik
        // 0,7, benzerlik 0,5. İkinci kümeyle tek belirteci paylaşır ama öteki üç farkın iki
        // tarafında da rakam var: eşik 0,7 × 1/4 = 0,175, benzerlik 0,25. En benzer küme eşiği
        // geçmese de ikinci küme geçer.
        let sonuc = kumele_tek_tur(
            Ayarlar::default(),
            &["Ödeme x y c3", "Ödeme a9 b9 c9", "Ödeme a1 b2 c3"],
        );
        assert_eq!(sonuc, ["1 × Ödeme x y c3", "2 × Ödeme <*> <*> <*>"]);
    }

    #[test]
    fn rakam_yalnizca_bir_taraftaysa_fark_degisken_degildir() {
        let sonuc = kumele_tek_tur(Ayarlar::default(), &["Unauthenticated.", "kullanici42"]);
        assert_eq!(sonuc, ["1 × Unauthenticated.", "1 × kullanici42"]);
        let sonuc = kumele_tek_tur(
            Ayarlar::default(),
            &["Yanıt süresi 120ms", "Yanıt süresi uzun"],
        );
        assert_eq!(sonuc.len(), 2);
    }

    #[test]
    fn rakamli_tek_kelimeler_birlesir() {
        // İki tarafında da rakam olan fark değişkendir: tek kelimelik rakamlı mesajlar
        // (kimlikler, özetler) bir kümede toplanır; rakamsızlar yalnızca birebir aynıysa
        // birleşir
        let sonuc = kumele_tek_tur(
            Ayarlar::default(),
            &[
                "Unauthenticated.",
                "kullanici42",
                "kullanici43",
                "Forbidden",
                "a1b2c3",
                "Unauthenticated.",
            ],
        );
        assert_eq!(sonuc, ["2 × Unauthenticated.", "3 × <*>", "1 × Forbidden"]);
        // Hiçbir belirteci tutmayan birleşme, bütün farklar rakamlıysa olur
        let sonuc = kumele_tek_tur(Ayarlar::default(), &["a1b2 c3d4", "e5f6 g7h8"]);
        assert_eq!(sonuc, ["2 × <*> <*>"]);
        // Eşik 0: klasik Drain'deki gibi aynı yapraktaki her şey birleşir
        let sifir = Ayarlar {
            benzerlik_esigi: 0.0,
            ..Ayarlar::default()
        };
        let sonuc = kumele_tek_tur(sifir, &["Unauthenticated.", "Forbidden", "kullanici42"]);
        assert_eq!(sonuc, ["3 × <*>"]);
    }

    #[test]
    fn joker_yapraginda_tek_ortak_kelime_yetmez() {
        // "Kod7" rakamlı olduğu için bu mesajlar joker dalındadır; 101 ad son konumu değişken
        // yapar. Aynı yaprağa düşen "Rapor dosyası hazır" yalnızca "dosyası"nı paylaşır:
        // "Rapor" ile "Kod7" arasındaki farkın yalnızca bir tarafında rakam var.
        let mut mesajlar: Vec<String> = (0..101)
            .map(|sira| format!("Kod7 dosyası {}", ad(sira)))
            .collect();
        mesajlar.push("Rapor dosyası hazır".to_string());
        let (kumeler, _) = kumele(&Ayarlar::default(), &mesajlar);
        assert_eq!(
            ozetle(&kumeler),
            ["101 × Kod7 dosyası <*>", "1 × Rapor dosyası hazır"]
        );
    }

    #[test]
    fn degisken_konum_tek_basina_tutunacak_yer_olmaz() {
        // İki belirteçli mesajlar, joker dalında: 101 ad ikinci konumu değişken yapar. "S3 hata"
        // ile "Kod7 <*>" arasında aynı belirteç yok: birinci konumun iki tarafında rakam var,
        // ikincisi yalnızca değişken bir konum. Birleşselerdi hata kaydı yüz bir başka mesajın
        // arasında kaybolurdu.
        let mut mesajlar: Vec<String> = (0..101).map(|sira| format!("Kod7 {}", ad(sira))).collect();
        mesajlar.push("S3 hata".to_string());
        let (kumeler, _) = kumele(&Ayarlar::default(), &mesajlar);
        assert_eq!(ozetle(&kumeler), ["101 × Kod7 <*>", "1 × S3 hata"]);
    }

    #[test]
    fn mesajda_yazili_joker_siradan_kelimedir() {
        // Kalıplardaki joker mesajdaki bir kelimeyle karışmaz: yazılı "<*>" içeren aynı
        // mesajlar birleşir. Karışsaydı her biri ayrı küme açar, süre mesaj sayısının karesiyle
        // artardı.
        let mesajlar = vec!["Şablon <*> eksik"; 5_000];
        let (kumeler, _) = kumele(&Ayarlar::default(), &mesajlar);
        assert_eq!(ozetle(&kumeler), ["5000 × Şablon <*> eksik"]);
        // Baştaki "<*>" joker dalına gitmez, kendi dalını açar
        let sonuc = kumele_tek_tur(Ayarlar::default(), &["<*> hazır", "<*> hazır", "x hazır"]);
        assert_eq!(sonuc, ["2 × <*> hazır", "1 × x hazır"]);
        // Klasik Drain'de de
        let klasik = Ayarlar {
            esnek_esik: false,
            ..Ayarlar::default()
        };
        assert_eq!(kumele(&klasik, &["Şablon <*> eksik"; 3]).0.len(), 1);
    }

    /// Tekrarlanabilir sözde rastgele sayılar (xorshift64*).
    struct Rastgele(u64);

    impl Rastgele {
        /// 0 ile `ust` arasında (`ust` hariç) bir sayı.
        fn sayi(&mut self, ust: usize) -> usize {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            (x.wrapping_mul(0x2545_F491_4F6C_DD1D) % ust as u64) as usize
        }
    }

    #[test]
    fn tek_belirtecli_aday_kisaltmasi_tam_taramayla_ayni() {
        // Tek belirteçli bir mesaj, yapraktaki bütün kümeler yerine en fazla iki adayla
        // karşılaştırılır. Her ayarda sonuç, bütün kümeleri taramakla aynı olmalı.
        for (esnek_esik, benzerlik_esigi) in [
            (true, 0.7),
            (true, 1.0),
            (true, 0.3),
            (true, 0.0),
            (false, 0.7),
            (false, 0.0),
        ] {
            let ayarlar = Ayarlar {
                esnek_esik,
                benzerlik_esigi,
                ..Ayarlar::default()
            };
            let mut rastgele = Rastgele(0x9E37_79B9_7F4A_7C15);
            let mut drain = Drain::new(ayarlar);
            for _ in 0..3_000 {
                // Rakamsız adlar, rakamlı kimlikler ve birkaç sabit kelime
                let mesaj = match rastgele.sayi(4) {
                    0 => ad(rastgele.sayi(300)),
                    1 => format!("k{}", rastgele.sayi(60)),
                    2 => ["Unauthenticated.", "<*>", "a1b2", "ok"][rastgele.sayi(4)].to_string(),
                    _ => format!("{}{}", ad(rastgele.sayi(5)), rastgele.sayi(3)),
                };
                let belirtecler = [mesaj.as_str()];
                let tam_tarama = drain.uzunluklar.get(&1).and_then(|yaprak| {
                    drain.en_benzer(&yaprak.kume_numaralari, &belirtecler, &yaprak.konumlar)
                });
                assert_eq!(drain.ara(&belirtecler), tam_tarama, "{mesaj}");
                drain.ekle(&mesaj);
            }
        }
    }
}
