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
//! - *Aynı biçimdeki kodsal değerler.* Koddan gelen bir değer sıradan bir kelimeye benzemez:
//!   `App\Jobs\FaturaGonder` bir sınıf adıdır, `ORDER_CREATED` bir sabit, `odeme.basarisiz`
//!   bir olay adı, `'fiyat'` tırnaklı bir anahtar. İki belirteç farklı ama ikisi de aynı kodsal
//!   biçimdeyse (`Bicim`) fark değişken sayılır: `İşlendi: App\Jobs\FaturaGonder` ile
//!   `İşlendi: App\Jobs\EpostaGonder` birleşir, `Ödeme alındı` ile `Ödeme reddedildi` (iki
//!   sıradan kelime) birleşmez. Böyle bir konum *kategorik* bir konumdur: değerleri bir kümeden
//!   gelir ve rapor o değerleri sayar. Kodsal biçimli bir baş belirteç, rakamlı belirteçler
//!   gibi ortak `<*>` dalına gider; yoksa `ORDER_CREATED olayı işlendi` ile
//!   `ORDER_FAILED olayı işlendi` hiç karşılaştırılmazdı.
//! - *Değişken konumlar.* Rakamsız bir sıradan kelime (kullanıcı adı, ürün adresi) kısa
//!   mesajları birleştirmez: `Bilinmeyen kullanıcı: ahmet` ile `Bilinmeyen kullanıcı: mehmet` üç
//!   belirtecin ikisinde aynıdır (0,67 < 0,7). Her değer ayrı bir küme olur ve yaprak büyüdükçe
//!   her yeni mesaj bütün kümelerle karşılaştırılır: süre mesaj sayısının karesiyle artar.
//!   3. adımdaki dolu düğüm kuralı yaprakta da uygulanır: kümelerin kalıplarında bir konum
//!   `en_fazla_cocuk`'tan fazla farklı belirteç aldıysa o konum değişkendir. Böyle bir konum
//!   bulunursa mesajlar bu bilgiyle bir kez daha, baştan kümelenir (ikinci tur); konum değişken
//!   sayılmadan önce açılmış kümeler de böylece birleşir.
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
//!
//! **Sıra ve önbellek.** Drain sıraya bağlıdır: aynı mesaj logun başında bir kümeye, sonunda
//! (arada daha benzer bir küme açıldıysa) başka bir kümeye katılabilir. Bu yüzden her kayıt,
//! mesajı daha önce görülmüş olsa da, logdaki sırasıyla kümelenir (`Kumeleyici`). Bir logdaki
//! kayıtların çoğu daha önce görülmüş bir mesajın tekrarıdır; her tekrarda ağaçta arama
//! yapmamak için her mesajın son sonucu saklanır. Ağacın her uzunluktaki kısmının bir sürümü
//! vardır: o uzunlukta bir küme açılınca ya da bir kalıp değişince artar. Mesaj geldiğinde
//! kendi uzunluğunun sürümü son gelişinden bu yana değişmediyse arama aynı kümeyi bulurdu ve
//! kalıp (mesaja zaten uyduğu için) değişmezdi: sonuç aramadan verilir. Bir mesajın aramasını
//! etkileyen her şey (ağacın o uzunluktaki dalları, oradaki kümelerin kalıpları ve konumları,
//! tek belirteçli kümeler) yalnızca o uzunlukta değişir.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

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

/// Bir yaprakta en fazla bu kadar küme × `en_fazla_cocuk` küme açılır. Her konumu en fazla
/// `en_fazla_cocuk` farklı değer alan ama değerlerinin birleşimleri sayısız olan mesajlar
/// (`<ad> <eylem> <nesne>`) hiçbir konumu değişken yapmaz: her biri ayrı bir küme açar, her
/// yeni mesaj hepsiyle karşılaştırılır ve süre mesaj sayısının karesiyle artardı. Yaprak
/// dolunca hiçbir kümeye benzemeyen mesajlar yaprağın taşma kümesinde toplanır; böylece bir
/// mesaj en fazla bu kadar kümeyle karşılaştırılır. Gerçek loglarda yapraklar bunun çok
/// altındadır.
const YAPRAK_KATSAYISI: usize = 10;

/// Aynı kalıba uyan mesajlar.
///
/// Kalıp yalnızca jokere döner (`guncelle`): joker olmayan belirteçleri hep kümeyi açan
/// mesajınkilerdir. Bu yüzden belirteçler ayrı ayrı saklanmaz; kalıp, açan mesajın metni
/// (depodaki maskeli mesajla paylaşılır) ve belirteçlerin o metindeki yerleridir. Belirteç
/// başına sekiz bayt: uzun SQL mesajlarıyla dolu bir logda kalıplar, her belirteç ayrı bir
/// metin olsaydı tutacakları belleğin yedide birini tutar.
#[derive(Debug, Clone)]
pub struct Kume {
    /// Kümeyi açan mesaj.
    acan: Rc<str>,
    /// Kalıbın belirteçlerinin `acan`daki yerleri (başı ve sonu, bayt); jokerde
    /// `JOKER_YERI`. Gösterilecek hali `metin`.
    yerler: Box<[(u32, u32)]>,
    /// Kümeye katılan mesaj sayısı.
    pub adet: usize,
}

/// Kalıpta jokerin yeri: hiçbir metinde olmayan bir aralık.
const JOKER_YERI: (u32, u32) = (u32::MAX, u32::MAX);

impl Kume {
    /// `acan` mesajıyla açılan küme; `sinirlar`: belirteçlerinin yerleri (`sinirlar`).
    fn new(acan: &Rc<str>, sinirlar: &[(usize, usize)]) -> Kume {
        let yer = |konum: usize| u32::try_from(konum).expect("mesaj 4 GB'tan kısadır");
        Kume {
            acan: Rc::clone(acan),
            yerler: sinirlar
                .iter()
                .map(|&(bas, son)| (yer(bas), yer(son)))
                .collect(),
            adet: 1,
        }
    }

    /// Kalıbın belirteçleri sırayla; jokerlerde iç joker işareti (`JOKER`).
    fn belirtecler(&self) -> impl Iterator<Item = &str> + '_ {
        self.yerler.iter().map(|&(bas, son)| {
            if (bas, son) == JOKER_YERI {
                JOKER
            } else {
                &self.acan[bas as usize..son as usize]
            }
        })
    }

    /// Kalıpta mesajdan farklı olan konumları jokere çevirir. Kalıp değiştiyse true.
    fn guncelle(&mut self, belirtecler: &[&str]) -> bool {
        let mut degisti = false;
        for (yer, b) in self.yerler.iter_mut().zip(belirtecler) {
            if *yer != JOKER_YERI && &self.acan[yer.0 as usize..yer.1 as usize] != *b {
                *yer = JOKER_YERI;
                degisti = true;
            }
        }
        degisti
    }

    /// Kalıptaki joker konumların sıraları (0'dan). Mesajda yazılı bir `<*>` joker değildir.
    pub fn joker_konumlari(&self) -> impl Iterator<Item = usize> + '_ {
        self.yerler
            .iter()
            .enumerate()
            .filter(|&(_, &yer)| yer == JOKER_YERI)
            .map(|(sira, _)| sira)
    }

    /// Kalıbın okunabilir hali: "Zamanlanmış görev başladı: <*>"
    pub fn metin(&self) -> String {
        let belirtecler: Vec<&str> = self
            .belirtecler()
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

/// Mesajın belirteçlerinin yerleri (başı ve sonu, bayt): `split_whitespace` gibi Unicode
/// boşluklarından bölünür.
fn sinirlar(mesaj: &str) -> Vec<(usize, usize)> {
    let mut sinirlar = Vec::new();
    let mut bas = None;
    for (konum, harf) in mesaj.char_indices() {
        if harf.is_whitespace() {
            if let Some(bas) = bas.take() {
                sinirlar.push((bas, konum));
            }
        } else if bas.is_none() {
            bas = Some(konum);
        }
    }
    if let Some(bas) = bas {
        sinirlar.push((bas, mesaj.len()));
    }
    sinirlar
}

/// Ağacın bir düğümü.
#[derive(Debug, Default)]
struct Dugum {
    /// Belirteç → alt düğüm.
    cocuklar: HashMap<String, Dugum>,
    /// Sadece yapraklarda dolu: bu yaprağa düşen kümelerin numaraları.
    kume_numaralari: Vec<usize>,
    /// Sadece dolmuş yapraklarda: hiçbir kümeye benzemeyen mesajların kümesi
    /// (`YAPRAK_KATSAYISI`). `kume_numaralari`nda değildir.
    tasma: Option<usize>,
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

/// Ağaçta bir mesaja benzeyen kümenin aranması.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arama {
    /// Eşiği geçen en benzer küme.
    Bulundu(usize),
    /// Benzeyen küme yok: yeni bir küme açılır.
    Yok,
    /// Benzeyen küme yok ve yaprak dolu: mesaj yaprağın taşma kümesine gider.
    Dolu,
}

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
    /// Belirteç sayısı → ağacın o uzunluktaki kısmının sürümü: bir küme açılınca ya da bir
    /// kalıp değişince artar (`Kumeleyici`'nin önbelleği için).
    surumler: HashMap<usize, u64>,
}

/// Mesajları teker teker, geldikleri sırayla kümeler; her mesajın o anki küme numarasını verir.
///
/// Her mesajın bir numarası vardır: aynı metin hep aynı numarayla gelir (depo maskeli mesajları
/// böyle numaralandırır). Mesajın son gelişinin sonucu numarasıyla saklanır; ağacın mesajın
/// uzunluğundaki kısmı o zamandan beri değişmediyse sonuç aramadan verilir (modül belgesindeki
/// "Sıra ve önbellek"). Sonuç, her mesajı aramayla eklemekle birebir aynıdır.
///
/// Kümeleme sırasında değişken bir konum bulunursa (`ikinci_tur_gerekir`) mesajlar bu bilgiyle
/// baştan, aynı sırayla bir kez daha kümelenmelidir (`ikinci_tur`).
#[derive(Debug)]
pub struct Kumeleyici {
    drain: Drain,
    /// Mesaj numarası → son gelişinin sonucu.
    onbellek: Vec<Option<OncekiSonuc>>,
}

/// Bir mesajın son kümelenişi.
#[derive(Debug, Clone, Copy)]
struct OncekiSonuc {
    /// Mesajın belirteç sayısı: ağacın hangi kısmının sürümüne bakılacağı.
    uzunluk: usize,
    /// Kümelendikten hemen sonra ağacın o kısmının sürümü.
    surum: u64,
    kume: usize,
}

impl Kumeleyici {
    pub fn new(ayarlar: Ayarlar) -> Self {
        Kumeleyici {
            drain: Drain::new(ayarlar),
            onbellek: Vec::new(),
        }
    }

    /// Mesajı kümeler; küme numarasını döndürür. `no`: mesajın numarası. Mesaj bir küme
    /// açarsa kalıbı onun metnini paylaşır.
    pub fn ekle(&mut self, no: usize, mesaj: &Rc<str>) -> usize {
        if let Some(Some(onceki)) = self.onbellek.get(no)
            && self.drain.surum(onceki.uzunluk) == onceki.surum
        {
            self.drain.kumeler[onceki.kume].adet += 1;
            return onceki.kume;
        }
        let sinirlar = sinirlar(mesaj);
        let belirtecler: Vec<&str> = sinirlar
            .iter()
            .map(|&(bas, son)| &mesaj[bas..son])
            .collect();
        let kume = self.drain.belirtecleri_ekle(mesaj, &sinirlar, &belirtecler);
        if self.onbellek.len() <= no {
            self.onbellek.resize(no + 1, None);
        }
        self.onbellek[no] = Some(OncekiSonuc {
            uzunluk: belirtecler.len(),
            surum: self.drain.surum(belirtecler.len()),
            kume,
        });
        kume
    }

    /// Testlerde: mesaj bir metinden.
    #[cfg(test)]
    fn ekle_metin(&mut self, no: usize, mesaj: &str) -> usize {
        self.ekle(no, &Rc::from(mesaj))
    }

    /// Kümelemede değişken bir konum bulundu mu? Bulunduysa mesajlar `ikinci_tur` ile baştan
    /// kümelenmelidir.
    pub fn ikinci_tur_gerekir(&self) -> bool {
        !self.drain.degiskenler.is_empty()
    }

    /// Boş bir kümeleyici; bu turda öğrenilen değişken konumları (ve bu kümeleyicinin
    /// önceki turlardan bildiklerini) bilir. Log bir kerede okunurken ikinci turun kendi
    /// bulduğu değişken konumlar kullanılmaz: üçüncü bir tur yoktur. Canlı izlemede ise
    /// kümeleyici her çizimde gerekiyorsa yeniden kurulur; bir yaprakta sonradan bulunan bir
    /// değişken konum o yaprağın bütün konumlarıyla birlikte kaydedildiği için
    /// (`degiskenler`), bilinenlerin üstüne yazılması hiçbirini unutturmaz.
    pub fn ikinci_tur(&self) -> Kumeleyici {
        let mut ogrenilen = self.drain.ogrenilen.clone();
        for (yol, degiskenler) in &self.drain.degiskenler {
            ogrenilen.insert(yol.clone(), degiskenler.clone());
        }
        Kumeleyici {
            drain: Drain {
                ogrenilen,
                ..Drain::new(self.drain.ayarlar.clone())
            },
            onbellek: Vec::new(),
        }
    }

    /// Yalnızca kümeleri (kalıplarını ve adetlerini) taşıyan bir kopya: ağacı yoktur, mesaj
    /// eklenemez. Deponun bir zaman penceresindeki görünümü (`Depo::pencere`) kalıpları
    /// buradan okur; ağacı kopyalamak gerekmez.
    pub fn kumelerin_kopyasi(&self) -> Kumeleyici {
        Kumeleyici {
            drain: Drain {
                kumeler: self.drain.kumeler.clone(),
                ..Drain::new(self.drain.ayarlar.clone())
            },
            onbellek: Vec::new(),
        }
    }

    /// Kümeler, açılış sırasıyla. Kalıpları yeni mesajlar geldikçe değişir; son halleri bütün
    /// mesajlar kümelenince alınmalıdır.
    pub fn kumeler(&self) -> &[Kume] {
        &self.drain.kumeler
    }
}

/// Mesajları sırayla, önbelleksiz kümeler. Kümeleri (ilk görüldükleri sırayla) ve her mesajın
/// küme numarasını (mesajlarla aynı sırada) döndürür. Değişken bir konum bulunursa mesajlar
/// ikinci kez kümelenir. `Kumeleyici`'nin sonuçlarının denetlendiği başvuru uygulamasıdır.
#[cfg(test)]
pub fn kumele(ayarlar: &Ayarlar, mesajlar: &[impl AsRef<str>]) -> (Vec<Kume>, Vec<usize>) {
    let mesajlar: Vec<Rc<str>> = mesajlar
        .iter()
        .map(|mesaj| Rc::from(mesaj.as_ref()))
        .collect();
    let mut ilk_tur = Drain::new(ayarlar.clone());
    let numaralar = mesajlar
        .iter()
        .map(|mesaj| ilk_tur.ekle_rc(mesaj))
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
        .map(|mesaj| ikinci_tur.ekle_rc(mesaj))
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
            surumler: HashMap::new(),
        }
    }

    /// Mesajı benzediği kümeye ekler, yoksa yeni küme açar. Kümenin numarasını döndürür.
    #[cfg(test)]
    fn ekle(&mut self, mesaj: &str) -> usize {
        self.ekle_rc(&Rc::from(mesaj))
    }

    /// `ekle`; mesaj bir küme açarsa kalıbı onun metnini paylaşır.
    #[cfg(test)]
    fn ekle_rc(&mut self, mesaj: &Rc<str>) -> usize {
        // 1. Belirteçlere böl
        let sinirlar = sinirlar(mesaj);
        let belirtecler: Vec<&str> = sinirlar
            .iter()
            .map(|&(bas, son)| &mesaj[bas..son])
            .collect();
        self.belirtecleri_ekle(mesaj, &sinirlar, &belirtecler)
    }

    /// Belirteçlerine bölünmüş mesajı ekler (`ekle`). `sinirlar`: belirteçlerin mesajdaki
    /// yerleri.
    fn belirtecleri_ekle(
        &mut self,
        mesaj: &Rc<str>,
        sinirlar: &[(usize, usize)],
        belirtecler: &[&str],
    ) -> usize {
        // 2–4. Ağaçta en benzer kümeyi ara
        match self.ara(belirtecler) {
            // 5. Bulunan kümeye kat ve kalıbını güncelle
            Arama::Bulundu(numara) => self.kat(numara, belirtecler),
            // Yeterince benzer küme yok: yeni küme aç ve ağaca yerleştir
            Arama::Yok => {
                let numara = self.kumeler.len();
                self.kumeler.push(Kume::new(mesaj, sinirlar));
                self.agaca_ekle(belirtecler, numara);
                self.degisti(belirtecler.len());
                numara
            }
            // Yaprak dolu: taşma kümesine
            Arama::Dolu => {
                let yeni = self.kumeler.len();
                let yaprak = self.yaprak_mut(belirtecler).expect("dolu yaprak ağaçtadır");
                let tasma = *yaprak.tasma.get_or_insert(yeni);
                if tasma == yeni {
                    self.kumeler.push(Kume::new(mesaj, sinirlar));
                    self.degisti(belirtecler.len());
                    tasma
                } else {
                    self.kat(tasma, belirtecler)
                }
            }
        }
    }

    /// Mesajı kümeye katar, kalıbını günceller.
    fn kat(&mut self, numara: usize, belirtecler: &[&str]) -> usize {
        let kume = &mut self.kumeler[numara];
        kume.adet += 1;
        if kume.guncelle(belirtecler) {
            self.degisti(belirtecler.len());
        }
        numara
    }

    /// Mesajın yaprağı: `ara`nın indiği yer, değiştirilebilir.
    fn yaprak_mut(&mut self, belirtecler: &[&str]) -> Option<&mut Dugum> {
        let yonlendiren = self.yonlendiren_sayisi(belirtecler.len());
        let mut dugum = self.uzunluklar.get_mut(&belirtecler.len())?;
        for belirtec in &belirtecler[..yonlendiren] {
            dugum = if dugum.cocuklar.contains_key(*belirtec) {
                dugum.cocuklar.get_mut(*belirtec)?
            } else {
                dugum.cocuklar.get_mut(JOKER)?
            };
        }
        Some(dugum)
    }

    /// Ağacın bu uzunluktaki kısmının sürümü.
    fn surum(&self, uzunluk: usize) -> u64 {
        self.surumler.get(&uzunluk).copied().unwrap_or(0)
    }

    /// Ağacın bu uzunluktaki kısmı değişti: bir küme açıldı ya da bir kalıp değişti.
    fn degisti(&mut self, uzunluk: usize) {
        *self.surumler.entry(uzunluk).or_default() += 1;
    }

    /// Mesaja yeterince benzeyen en iyi kümeyi bulur. Ağacı değiştirmez.
    fn ara(&self, belirtecler: &[&str]) -> Arama {
        // 2. İlk katman: belirteç sayısı
        let Some(mut dugum) = self.uzunluklar.get(&belirtecler.len()) else {
            return Arama::Yok;
        };

        // 3. Baş belirteçlere göre aşağı in: önce birebir aynı dal, yoksa joker dalı
        for belirtec in &belirtecler[..self.yonlendiren_sayisi(belirtecler.len())] {
            dugum = match dugum.cocuklar.get(*belirtec).or(dugum.cocuklar.get(JOKER)) {
                Some(cocuk) => cocuk,
                None => return Arama::Yok,
            };
        }

        // Boş mesajlar (ör. Log::info('')) hep aynı kümede toplanır
        if belirtecler.is_empty() {
            return dugum
                .kume_numaralari
                .first()
                .map_or(Arama::Yok, |&numara| Arama::Bulundu(numara));
        }

        // 4. Yapraktaki en benzer küme. Tek belirteçlilerde yaprak dolmaz: her mesaj en fazla
        // iki kümeyle karşılaştırılır (`tek_belirtecli_adaylar`)
        let bulunan = if let [belirtec] = belirtecler
            && self.ayarlar.benzerlik_esigi > 0.0
        {
            let adaylar = self.tek_belirtecli_adaylar(belirtec);
            self.en_benzer(&adaylar, belirtecler, &dugum.konumlar)
        } else {
            self.en_benzer(&dugum.kume_numaralari, belirtecler, &dugum.konumlar)
        };
        match bulunan {
            Some(numara) => Arama::Bulundu(numara),
            None if belirtecler.len() >= 2
                && dugum.kume_numaralari.len()
                    >= YAPRAK_KATSAYISI * self.ayarlar.en_fazla_cocuk =>
            {
                Arama::Dolu
            }
            None => Arama::Yok,
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
        let mut bicimler = Bicimler::new(belirtecler);
        for &numara in adaylar {
            let sonuc = karsilastir(&self.kumeler[numara], belirtecler, konumlar, &mut bicimler);
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
            for (konum, belirtec) in dugum
                .konumlar
                .iter_mut()
                .zip(self.kumeler[kume].belirtecler())
            {
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
    // İçinde rakam olan (ör. "a1b2c3") ya da kodsal biçimli (ör. "ORDER_CREATED") belirteç
    // büyük ihtimalle değişkendir: ayrı dal açma
    if rakam_iceriyor(belirtec) || bicim(belirtec).kodsal() {
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
    /// tarafında da rakam var (jokerde mesajın tarafında), iki taraf da aynı kodsal biçimde
    /// (jokerde mesajın tarafı kodsal) ya da konum yaprakta değişken.
    degisken_fark: usize,
    /// Değişken farkların, rakamdan dolayı değişken olanları.
    rakamli_fark: usize,
}

/// Kümenin kalıbı ile mesajı konum konum karşılaştırır. `konumlar`: yaprağın konumları; esnek
/// eşik kapalıyken boştur.
///
/// Joker, kalıbın o konumda farklı değerler gördüğünü söyler; o değerlerin rakamlı ya da kodsal
/// olup olmadığı bilinmez. Jokerin karşısındaki belirteçte rakam varsa fark rakamlı, belirteç
/// kodsal biçimdeyse fark kodsal sayılır. `bicimler`: mesajın belirteçlerinin biçimleri,
/// gerektikçe bulunur.
fn karsilastir(
    kume: &Kume,
    belirtecler: &[&str],
    konumlar: &[Konum],
    bicimler: &mut Bicimler<'_>,
) -> Karsilastirma {
    let mut sonuc = Karsilastirma {
        esit: 0,
        joker: 0,
        degisken_fark: 0,
        rakamli_fark: 0,
    };
    for (sira, (k, b)) in kume.belirtecler().zip(belirtecler).enumerate() {
        let (rakamli, kodsal) = if k == JOKER {
            sonuc.joker += 1;
            (rakam_iceriyor(b), bicimler.bicim(sira).kodsal())
        } else if k == *b {
            sonuc.esit += 1;
            continue;
        } else {
            // İki farklı belirteç aynı kodsal biçimde mi? `ORDER_CREATED` ile `ORDER_FAILED`
            // evet; `alındı` ile `reddedildi` (sıradan kelimeler) ve `ORDER_FAILED` ile `hazır`
            // hayır. Mesajın belirteci kodsal değilse (çoğu zaman) kalıbınkine bakılmaz.
            let bicim_b = bicimler.bicim(sira);
            (
                rakam_iceriyor(k) && rakam_iceriyor(b),
                bicim_b.kodsal() && bicim(k) == bicim_b,
            )
        };
        if rakamli {
            sonuc.rakamli_fark += 1;
        }
        if rakamli || kodsal || konumlar.get(sira).is_some_and(Konum::degisken_mi) {
            sonuc.degisken_fark += 1;
        }
    }
    sonuc
}

/// Bir mesajın belirteçlerinin biçimleri, gerektikçe bulunur. Mesaj yapraktaki her kümeyle
/// karşılaştırılırken bir belirtecin biçimi bir kez hesaplanır; hiç farklı olmayan bir
/// belirtecinki hiç hesaplanmaz.
struct Bicimler<'a> {
    belirtecler: &'a [&'a str],
    bicimler: Vec<Option<Bicim>>,
}

impl<'a> Bicimler<'a> {
    fn new(belirtecler: &'a [&'a str]) -> Self {
        Bicimler {
            belirtecler,
            bicimler: vec![None; belirtecler.len()],
        }
    }

    /// `sira`. belirtecin biçimi.
    fn bicim(&mut self, sira: usize) -> Bicim {
        *self.bicimler[sira].get_or_insert_with(|| bicim(self.belirtecler[sira]))
    }
}

/// Bir belirtecin biçimi: koddan gelen bir değer mi, sıradan bir kelime mi?
///
/// Sıradan kelimeler (`Ödeme`, `alındı`) yalnızca harflerden oluşur. Koddan gelen değerlerin
/// (sınıf adı, sabit, tanımlayıcı, olay adı, tırnaklı anahtar) biçimi bellidir ve bir mesajın
/// sabit kısmında böyle bir belirteç olmaz: iki mesaj yalnızca aynı biçimdeki iki kodsal değerde
/// ayrılıyorsa o konum kategoriktir. Kısa büyük harfli kelimeler (`GET`, `OK`, `WARNING`)
/// temkinle sıradan kelime sayılır: `GET /api` ile `POST /api` farklı olaylardır.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bicim {
    /// Ad alanlı PHP sınıf adı: `App\Jobs\FaturaGonder`, `\App\Models\Urun`, JSON kaçışlı
    /// `App\\Jobs\\X`
    Sinif,
    /// Büyük harfli sabit, en az bir alt çizgiyle: `ORDER_CREATED`
    Sabit,
    /// deveAdı ya da PascalAdı (`FaturaGonder`), yilan_adi (`fatura_gonder`), PHP değişkeni
    /// (`$urun`)
    Tanimlayici,
    /// Noktayla ya da iki noktayla parçalanmış ad: `odeme.basarisiz`, `kurlar:guncelle`,
    /// `ornek.test`
    Noktali,
    /// Tek ya da çift tırnakla sarılı tek kelime: `'fiyat'`, `"dsn"`. SQL'in ters tırnaklı
    /// tablo ve sütun adları (`` `urunler` ``) sayılmaz: iki sorgu tablosuyla ayrılıyorsa ayrı
    /// sorgulardır.
    Tirnakli,
    /// Yalnızca harflerden oluşan sıradan kelime: `Ödeme`, `alındı`, `GET`
    Sozcuk,
    /// Geri kalan her şey: yer tutucular (`<SAYI>`), noktalama, harfle rakamın karıştığı
    /// belirteçler (`oauth2`), üye çağrıları (`X::handle()`)
    Diger,
}

impl Bicim {
    /// Koddan gelen bir değer mi? Sıradan kelimeler ve belirsiz belirteçler değildir.
    fn kodsal(self) -> bool {
        !matches!(self, Bicim::Sozcuk | Bicim::Diger)
    }
}

/// Belirtecin cümleden gelen kısmı soyulmuş hali: baştaki açan ayraçlar (`([{`) ve sondaki
/// kapatan ayraçlarla noktalama (`)]},;:.`) belirtecin kendisinden değildir. `[App\Jobs\X]`
/// → `App\Jobs\X`, `(Ayşe,` → `Ayşe`. Soyulunca bir şey kalmıyorsa (`,`) belirtecin kendisi.
pub fn cekirdek(belirtec: &str) -> &str {
    let cekirdek = belirtec
        .trim_start_matches(['(', '[', '{'])
        .trim_end_matches([')', ']', '}', ',', ';', ':', '.']);
    if cekirdek.is_empty() {
        belirtec
    } else {
        cekirdek
    }
}

/// Belirtecin biçimi; cümleden gelen ayraç ve noktalama soyularak bakılır (`cekirdek`):
/// `[App\Jobs\X]` bir sınıf adı, `ORDER_CREATED:` bir sabittir.
fn bicim(belirtec: &str) -> Bicim {
    let cekirdek = cekirdek(belirtec);
    if let Some(ic) = tirnakli_ic(cekirdek) {
        // Tırnağın içinde tırnak yoksa tek bir tırnaklı kelimedir
        return if !ic.is_empty() && !ic.contains(['\'', '"']) {
            Bicim::Tirnakli
        } else {
            Bicim::Diger
        };
    }
    // Bitişik tırnak cümleden gelebilir: `'Ali` ile `Veli'` iki kelimelik tırnaklı bir değer
    let cekirdek = cekirdek
        .trim_start_matches(['\'', '"'])
        .trim_end_matches(['\'', '"']);
    if cekirdek.is_empty() {
        return Bicim::Diger;
    }
    if cekirdek.contains('\\') {
        return if sinif_adi_mi(cekirdek) {
            Bicim::Sinif
        } else {
            Bicim::Diger
        };
    }
    if let Some(ad) = cekirdek.strip_prefix('$') {
        return if tanimlayici_mi(ad) {
            Bicim::Tanimlayici
        } else {
            Bicim::Diger
        };
    }
    if cekirdek.chars().all(char::is_alphabetic) {
        return if deve_adi_mi(cekirdek) {
            Bicim::Tanimlayici
        } else {
            Bicim::Sozcuk
        };
    }
    if tanimlayici_mi(cekirdek) {
        // Harfler ve rakamlar, en az bir alt çizgiyle (alt çizgisiz olsaydı yukarıda sıradan
        // kelime sayılır ya da rakamlı olurdu)
        return if cekirdek.contains('_') {
            if cekirdek
                .chars()
                .filter(|c| c.is_alphabetic())
                .all(char::is_uppercase)
            {
                Bicim::Sabit
            } else {
                Bicim::Tanimlayici
            }
        } else if deve_adi_mi(cekirdek) {
            Bicim::Tanimlayici
        } else {
            Bicim::Diger
        };
    }
    if cekirdek.contains(['.', ':']) {
        let mut parcalar = cekirdek.split(['.', ':']);
        if parcalar.all(tanimlayici_mi) {
            return Bicim::Noktali;
        }
    }
    Bicim::Diger
}

/// Belirteç aynı tırnakla (`'` ya da `"`) başlayıp bitiyorsa tırnağın içi.
fn tirnakli_ic(belirtec: &str) -> Option<&str> {
    let ilk = belirtec.chars().next()?;
    if !matches!(ilk, '\'' | '"') || belirtec.len() < 2 || !belirtec.ends_with(ilk) {
        return None;
    }
    Some(&belirtec[1..belirtec.len() - 1])
}

/// Harf, rakam ve alt çizgiden oluşan, en az bir harfi olan bir ad mı?
fn tanimlayici_mi(ad: &str) -> bool {
    !ad.is_empty()
        && ad.chars().all(|c| c.is_alphanumeric() || c == '_')
        && ad.chars().any(char::is_alphabetic)
}

/// Küçük bir harften hemen sonra büyük bir harf geliyor mu? `FaturaGonder`, `hizSiniri` evet;
/// `Ödeme`, `SQLSTATE` hayır.
fn deve_adi_mi(ad: &str) -> bool {
    ad.chars()
        .zip(ad.chars().skip(1))
        .any(|(onceki, sonraki)| onceki.is_lowercase() && sonraki.is_uppercase())
}

/// Ters bölüyle (JSON'da iki ters bölüyle) ayrılmış en az iki ASCII tanımlayıcı mı? Başta bir
/// ters bölü olabilir. `App\Jobs\X` evet; `resources\views\x.blade.php` (uzantılı, bir yol)
/// ve `App\Jobs\X::handle()` hayır.
fn sinif_adi_mi(ad: &str) -> bool {
    let parcalar: Vec<&str> = ad
        .strip_prefix('\\')
        .unwrap_or(ad)
        .split('\\')
        .filter(|parca| !parca.is_empty())
        .collect();
    parcalar.len() >= 2
        && parcalar.iter().all(|parca| {
            parca.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
                && parca.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
}

#[cfg(test)]
mod testler {
    use super::*;
    use crate::kayit::deneme::topla;
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

        // Derinlik 2: ikinci belirteç de dal seçer. Sıradan kelimelerle ayrılan mesajlar farklı
        // yapraklara düşüp hiç karşılaşmaz
        let derin = Ayarlar {
            onek_derinligi: 2,
            ..Ayarlar::default()
        };
        let sayfalar = ["Sayfa sepet için bulunamadı", "Sayfa iade için bulunamadı"];
        assert_eq!(
            kumele_tek_tur(Ayarlar::default(), &sayfalar),
            ["2 × Sayfa <*> için bulunamadı"]
        );
        assert_eq!(kumele_tek_tur(derin.clone(), &sayfalar).len(), 2);
        // Kodsal biçimli ikinci belirteç (`[emails.siparis]`) rakamlılar gibi joker dalına
        // gider: şablon adları derinlik 2'de de buluşur
        assert_eq!(
            kumele_tek_tur(derin, &mesajlar),
            ["2 × View <*> not found."]
        );
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
        let (kayitlar, _) = topla(include_str!("../ornekler/laravel.log"));
        let maskeli: Vec<_> = kayitlar.iter().map(|kayit| maskele(&kayit.mesaj)).collect();
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
    fn belirtec_bicimleri() {
        use Bicim::*;
        let ornekler = [
            (r"App\Jobs\FaturaGonder", Sinif),
            (r"\App\Models\Urun", Sinif),
            (r"App\\Jobs\\FaturaGonder", Sinif),
            (r"[App\Jobs\FaturaGonder]", Sinif),
            (r"App\Jobs\FaturaGonder:", Sinif),
            (r"resources\views\urun.blade.php", Diger),
            (r"App\Jobs\X::handle()", Diger),
            (r"\", Diger),
            ("ORDER_CREATED", Sabit),
            ("ORDER_CREATED,", Sabit),
            ("HTTP_2", Sabit),
            ("FaturaGonder", Tanimlayici),
            ("hizSiniri", Tanimlayici),
            ("fatura_gonder", Tanimlayici),
            ("Order_Created", Tanimlayici),
            ("$urun", Tanimlayici),
            ("$urun2", Tanimlayici),
            ("odeme.basarisiz", Noktali),
            ("kurlar:guncelle", Noktali),
            ("ornek.test", Noktali),
            ("mail.from.address", Noktali),
            ("(siparis.olustur)", Noktali),
            ("'fiyat'", Tirnakli),
            ("\"dsn\"", Tirnakli),
            ("'App\\Jobs\\X'", Tirnakli),
            ("`urunler`", Diger),
            ("''", Diger),
            ("\"a'b\"", Diger),
            ("\"'\"", Diger),
            ("siparisV2Olustur", Tanimlayici),
            ("ApiV2Client", Tanimlayici),
            ("Fatura2Gonder", Diger),
            (r"\App", Diger),
            (r"App\", Diger),
            (r"App\\", Diger),
            ("'Ali", Sozcuk),
            ("Veli'", Sozcuk),
            ("Ödeme", Sozcuk),
            ("alındı", Sozcuk),
            ("GET", Sozcuk),
            ("SQLSTATE", Sozcuk),
            ("İşlendi:", Sozcuk),
            ("<SAYI>", Diger),
            ("<*>", Diger),
            (",", Diger),
            ("(,)", Diger),
            ("oauth2", Diger),
            ("a1b2", Diger),
            ("=", Diger),
            ("", Diger),
            ("$", Diger),
            ("e-posta", Diger),
            ("..", Diger),
            (".a", Diger),
        ];
        for (belirtec, beklenen) in ornekler {
            assert_eq!(bicim(belirtec), beklenen, "{belirtec:?}");
        }
        assert!(Sinif.kodsal() && Sabit.kodsal() && Tanimlayici.kodsal());
        assert!(Noktali.kodsal() && Tirnakli.kodsal());
        assert!(!Sozcuk.kodsal() && !Diger.kodsal());
    }

    #[test]
    fn ayni_bicimdeki_kodsal_degerler_birlesir() {
        // Kategorik konum: iki mesaj yalnızca aynı biçimdeki iki kodsal değerde ayrılıyor.
        // Hepsi 2–3 belirteçli: rakamsız bir fark olsa ayrı kalırlardı (0,5 ya da 0,67 < 0,7)
        let ciftler: [(&str, &str, &str); 7] = [
            (
                r"İşlendi: App\Jobs\FaturaGonder",
                r"İşlendi: App\Jobs\EpostaGonder",
                "İşlendi: <*>",
            ),
            ("Durum: ORDER_CREATED", "Durum: ORDER_FAILED", "Durum: <*>"),
            ("Olay: odeme.basarisiz", "Olay: odeme.basarili", "Olay: <*>"),
            (
                "Undefined variable $urun",
                "Undefined variable $siparis",
                "Undefined variable <*>",
            ),
            (
                "Undefined array key \"fiyat\"",
                "Undefined array key \"stok\"",
                "Undefined array key <*>",
            ),
            (
                "Kuyruk: fatura_gonder",
                "Kuyruk: eposta_gonder",
                "Kuyruk: <*>",
            ),
            (
                "Dinleyici: SiparisOlusturuldu",
                "Dinleyici: OdemeAlindi",
                "Dinleyici: <*>",
            ),
        ];
        for (birinci, ikinci, kalip) in ciftler {
            assert_eq!(
                kumele_tek_tur(Ayarlar::default(), &[birinci, ikinci]),
                [format!("2 × {kalip}")],
                "{birinci:?} / {ikinci:?}"
            );
        }
        // Klasik Drain'de birleşmezler
        let klasik = Ayarlar {
            esnek_esik: false,
            ..Ayarlar::default()
        };
        assert_eq!(
            kumele_tek_tur(klasik, &["Durum: ORDER_CREATED", "Durum: ORDER_FAILED"]).len(),
            2
        );
    }

    #[test]
    fn siradan_kelimeler_ve_farkli_bicimler_birlesmez() {
        let ciftler = [
            // Sıradan kelimeler farklı olaylardır
            ("Kuyruk emails dolu", "Kuyruk default dolu"),
            ("GET /api/urunler yavaş", "POST /api/urunler yavaş"),
            // Kodsal değerle sıradan kelime
            ("Durum: ORDER_FAILED", "Durum: hazır"),
            // Farklı kodsal biçimler
            ("Durum: ORDER_FAILED", "Durum: odeme.bekliyor"),
            // Ters tırnaklı SQL adları: farklı tablolar farklı sorgulardır
            ("Tablo `urunler` kilitli", "Tablo `siparisler` kilitli"),
            // Harfle rakamın karıştığı belirteçlerin yalnızca birinde rakam var
            ("oauth2 jetonu yenilendi", "oauth jetonu yenilendi"),
        ];
        for (birinci, ikinci) in ciftler {
            assert_eq!(
                kumele_tek_tur(Ayarlar::default(), &[birinci, ikinci]).len(),
                2,
                "{birinci:?} / {ikinci:?}"
            );
        }
    }

    #[test]
    fn kodsal_bas_belirtec_joker_dalina_gider() {
        // Baş belirteç dal seçer; kodsal bir baş belirteç ayrı dal açsaydı iki olay hiç
        // karşılaştırılmazdı
        let sonuc = kumele_tek_tur(
            Ayarlar::default(),
            &[
                "ORDER_CREATED olayı işlendi",
                "ORDER_FAILED olayı işlendi",
                r"App\Jobs\FaturaGonder işlendi",
                r"App\Jobs\EpostaGonder işlendi",
            ],
        );
        assert_eq!(sonuc, ["2 × <*> olayı işlendi", "2 × <*> işlendi"]);
        // Sıradan kelimeler kendi dallarını açar; aynı yaprakta değiller
        let sonuc = kumele_tek_tur(
            Ayarlar::default(),
            &["Ödeme olayı işlendi", "Kargo olayı işlendi"],
        );
        assert_eq!(sonuc.len(), 2);
    }

    #[test]
    fn jokerin_karsisindaki_kodsal_deger_degisken_sayilir() {
        // Kalıp "Durum: <*>" olduktan sonra kodsal bir değer katılır, sıradan bir kelime ayrı
        // küme açar
        let sonuc = kumele_tek_tur(
            Ayarlar::default(),
            &[
                "Durum: ORDER_CREATED",
                "Durum: ORDER_FAILED",
                "Durum: odeme.bekliyor",
                "Durum: hazır",
            ],
        );
        assert_eq!(sonuc, ["3 × Durum: <*>", "1 × Durum: hazır"]);
    }

    #[test]
    fn kodsal_fark_tek_basina_tutunacak_yer_olmaz() {
        // Hiçbir belirteci tutmayan bir birleşme yalnızca rakamlı farklarla olur
        let sonuc = kumele_tek_tur(Ayarlar::default(), &["A_B C_D", "E_F G_H"]);
        assert_eq!(sonuc.len(), 2);
        // Tek kelimelik kodsal mesajlar da birleşmez: her olay adı ayrı bir olaydır
        let sonuc = kumele_tek_tur(
            Ayarlar::default(),
            &["odeme.basarisiz", "odeme.basarili", "ORDER_FAILED"],
        );
        assert_eq!(sonuc.len(), 3);
    }

    #[test]
    fn belirtecin_cekirdegi() {
        assert_eq!(cekirdek("(Ayşe,"), "Ayşe");
        assert_eq!(cekirdek(r"[App\Jobs\X]"), r"App\Jobs\X");
        assert_eq!(cekirdek("\"fiyat\":"), "\"fiyat\"");
        assert_eq!(cekirdek("giriş"), "giriş");
        assert_eq!(cekirdek(","), ",");
        assert_eq!(cekirdek("([{}])"), "([{}])");
        assert_eq!(cekirdek(""), "");
    }

    #[test]
    fn joker_konumlari_kaliptan_okunur() {
        let mut drain = Drain::new(Ayarlar::default());
        drain.ekle("Şablon <*> eksik: a1 x");
        drain.ekle("Şablon <*> eksik: b2 y");
        let kume = &drain.kumeler[0];
        assert_eq!(kume.metin(), "Şablon <*> eksik: <*> <*>");
        // Mesajda yazılı "<*>" (1. konum) joker değildir
        assert_eq!(kume.joker_konumlari().collect::<Vec<_>>(), [3, 4]);
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
    fn sonraki_turlar_oncekilerde_ogrenilenleri_unutmaz() {
        // Canlı izleme: ilk turda bir yaprakta değişken konum bulunur, ikinci tur kurulur;
        // sonra gelen mesajlar başka bir yaprakta değişken konum bulur, üçüncü tur kurulur.
        // Üçüncü tur ikisini de bilmeli.
        let mut birinci = Kumeleyici::new(Ayarlar::default());
        for (no, mesaj) in bilinmeyen_kullanicilar(101).iter().enumerate() {
            birinci.ekle_metin(no, mesaj);
        }
        assert!(birinci.ikinci_tur_gerekir());
        let mut ikinci = birinci.ikinci_tur();
        assert_eq!(ikinci.drain.ogrenilen.len(), 1);
        assert!(!ikinci.ikinci_tur_gerekir());
        let gorevler: Vec<String> = (0..101)
            .map(|sira| format!("Görev bitti: {}", ad(sira)))
            .collect();
        for (no, mesaj) in gorevler.iter().enumerate() {
            ikinci.ekle_metin(200 + no, mesaj);
        }
        assert!(ikinci.ikinci_tur_gerekir());
        let ucuncu = ikinci.ikinci_tur();
        assert_eq!(ucuncu.drain.ogrenilen.len(), 2);
        // Sonuç: iki aile de baştan tek kalıpta
        let mut sonuc = ucuncu;
        for (no, mesaj) in bilinmeyen_kullanicilar(101).iter().enumerate() {
            sonuc.ekle_metin(no, mesaj);
        }
        for (no, mesaj) in gorevler.iter().enumerate() {
            sonuc.ekle_metin(200 + no, mesaj);
        }
        assert_eq!(
            ozetle(sonuc.kumeler()),
            ["101 × Bilinmeyen kullanıcı: <*>", "101 × Görev bitti: <*>"]
        );
        // Yalnızca kümeleri taşıyan kopya: kalıplar aynı, ağaç boş
        let kopya = sonuc.kumelerin_kopyasi();
        assert_eq!(ozetle(kopya.kumeler()), ozetle(sonuc.kumeler()));
        assert!(kopya.drain.uzunluklar.is_empty());
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

    /// Mesajları `Kumeleyici` ile kümeler; `kumele` gibi gerekirse iki tur. Aynı metinler aynı
    /// numarayı alır.
    fn onbellekli_kumele(ayarlar: &Ayarlar, mesajlar: &[String]) -> (Vec<String>, Vec<usize>) {
        let mut numaralar: HashMap<&str, usize> = HashMap::new();
        let nolar: Vec<usize> = mesajlar
            .iter()
            .map(|mesaj| {
                let yeni = numaralar.len();
                *numaralar.entry(mesaj.as_str()).or_insert(yeni)
            })
            .collect();
        let tur = |kumeleyici: &mut Kumeleyici| -> Vec<usize> {
            mesajlar
                .iter()
                .zip(&nolar)
                .map(|(mesaj, &no)| kumeleyici.ekle_metin(no, mesaj))
                .collect()
        };
        let mut kumeleyici = Kumeleyici::new(ayarlar.clone());
        let mut sonuc = tur(&mut kumeleyici);
        if kumeleyici.ikinci_tur_gerekir() {
            kumeleyici = kumeleyici.ikinci_tur();
            sonuc = tur(&mut kumeleyici);
        }
        (ozetle(kumeleyici.kumeler()), sonuc)
    }

    #[test]
    fn ayni_mesaj_sonradan_daha_benzer_kumeye_katilir() {
        // Drain sıraya bağlıdır: dördüncü mesaj ikincinin aynısıdır ama arada açılan küme ona
        // daha çok benzer (7 belirtecin 6'sı, ilk kümede 5'i). Her kayıt kendi sırasında
        // kümelenmeli; aynı mesaj bir kez kümelenseydi dördüncü de ilk kümeye düşerdi.
        let mesajlar: Vec<String> = [
            "Sepet güncellenemedi kullanıcı oturumu kapalı mobil uygulama",
            "Sepet güncellenemedi kullanıcı oturumu kapalı web tarayıcı",
            "Sepet boşaltılamadı kullanıcı oturumu kapalı web tarayıcı",
            "Sepet güncellenemedi kullanıcı oturumu kapalı web tarayıcı",
        ]
        .map(String::from)
        .into();
        let (kumeler, numaralar) = onbellekli_kumele(&Ayarlar::default(), &mesajlar);
        assert_eq!(numaralar, [0, 0, 1, 1]);
        assert_eq!(
            kumeler,
            [
                "2 × Sepet güncellenemedi kullanıcı oturumu kapalı <*> <*>",
                "2 × Sepet <*> kullanıcı oturumu kapalı web tarayıcı",
            ]
        );
        let (referans, referans_numaralari) = kumele(&Ayarlar::default(), &mesajlar);
        assert_eq!(
            (ozetle(&referans), referans_numaralari),
            (kumeler, numaralar)
        );
    }

    #[test]
    fn onbellekli_kumeleme_onbelleksizle_ayni() {
        // Önbellek yalnızca hızlandırır: her ayarda ve her mesaj dizisinde sonuç, her mesajı
        // aramayla eklemekle (iki turuyla birlikte) birebir aynı olmalı
        let ayarlar_listesi = [
            Ayarlar::default(),
            Ayarlar {
                onek_derinligi: 2,
                en_fazla_cocuk: 3,
                ..Ayarlar::default()
            },
            Ayarlar {
                onek_derinligi: 3,
                en_fazla_cocuk: 4,
                benzerlik_esigi: 0.4,
                ..Ayarlar::default()
            },
            Ayarlar {
                esnek_esik: false,
                benzerlik_esigi: 0.5,
                en_fazla_cocuk: 5,
                ..Ayarlar::default()
            },
            Ayarlar {
                benzerlik_esigi: 1.0,
                ..Ayarlar::default()
            },
            Ayarlar {
                benzerlik_esigi: 0.0,
                ..Ayarlar::default()
            },
        ];
        let kelimeler = [
            "sepet", "ödeme", "kargo", "web", "mobil", "api", "açık", "kapalı",
        ];
        let mut rastgele = Rastgele(0x2545_F491_4F6C_DD1D);
        let mut iki_turlu = 0;
        for deneme in 0..60 {
            let ayarlar = &ayarlar_listesi[deneme % ayarlar_listesi.len()];
            // Her denemede küçük bir mesaj havuzu: tekrarlar önbelleği, rastgele sıra Drain'in
            // sıraya bağlılığını, çok sayıda farklı ad ikinci turu sınar
            let havuz_boyu = 2 + rastgele.sayi(40);
            let havuz: Vec<String> = (0..havuz_boyu)
                .map(|_| match rastgele.sayi(6) {
                    0 => format!("Bilinmeyen kullanıcı: {}", ad(rastgele.sayi(400))),
                    1 => ad(rastgele.sayi(20)),
                    2 => format!("Yanıt süresi {}ms", rastgele.sayi(500)),
                    3 => String::new(),
                    _ => {
                        let uzunluk = 2 + rastgele.sayi(5);
                        let belirtecler: Vec<&str> = (0..uzunluk)
                            .map(|_| kelimeler[rastgele.sayi(kelimeler.len())])
                            .collect();
                        belirtecler.join(" ")
                    }
                })
                .collect();
            let adet = 50 + rastgele.sayi(400);
            let mut mesajlar: Vec<String> = (0..adet)
                .map(|_| havuz[rastgele.sayi(havuz.len())].clone())
                .collect();
            // Bazı denemelerde yüzlerce farklı ad: yaprakta değişken konum, ikinci tur
            if deneme % 4 == 0 {
                for sira in 0..250 {
                    let yer = rastgele.sayi(mesajlar.len());
                    mesajlar.insert(yer, format!("Bilinmeyen kullanıcı: {}", ad(sira)));
                }
            }
            let (referans, referans_numaralari) = kumele(ayarlar, &mesajlar);
            let (kumeler, numaralar) = onbellekli_kumele(ayarlar, &mesajlar);
            assert_eq!(numaralar, referans_numaralari, "deneme {deneme}");
            assert_eq!(kumeler, ozetle(&referans), "deneme {deneme}");
            let mut drain = Kumeleyici::new(ayarlar.clone());
            for (no, mesaj) in mesajlar.iter().enumerate() {
                drain.ekle_metin(no, mesaj);
            }
            iki_turlu += usize::from(drain.ikinci_tur_gerekir());
        }
        // İkinci tur gerçekten sınandı
        assert!(iki_turlu >= 5, "{iki_turlu}");
    }

    #[test]
    fn belirtec_sinirlari_split_whitespace_ile_ayni() {
        for mesaj in [
            "",
            "   ",
            "tek",
            "  iki  kelime ",
            "sekme\tve\nsatır sonu\r\n",
            "Unicode\u{00a0}boşluk\u{2003}ve\u{3000}ideografik",
            "çok baytlı ğüşıöç İ 🚀",
            "\u{feff}BOM boşluk değildir",
        ] {
            let beklenen: Vec<&str> = mesaj.split_whitespace().collect();
            let gelen: Vec<&str> = sinirlar(mesaj)
                .iter()
                .map(|&(bas, son)| &mesaj[bas..son])
                .collect();
            assert_eq!(gelen, beklenen, "{mesaj:?}");
        }
    }

    #[test]
    fn kalip_acan_mesajin_metnini_paylasir() {
        let mesaj: Rc<str> = Rc::from("Kullanıcı <SAYI> giriş yaptı");
        let mut kumeleyici = Kumeleyici::new(Ayarlar::default());
        kumeleyici.ekle(0, &mesaj);
        kumeleyici.ekle_metin(1, "Kullanıcı <SAYI> çıkış yaptı");
        let kume = &kumeleyici.kumeler()[0];
        assert!(Rc::ptr_eq(&kume.acan, &mesaj));
        assert_eq!(kume.metin(), "Kullanıcı <SAYI> <*> yaptı");
        assert_eq!(kume.joker_konumlari().collect::<Vec<_>>(), [2]);
        // Mesajda yazılı `<*>` joker değildir
        kumeleyici.ekle_metin(2, "Değer: <*>");
        assert_eq!(kumeleyici.kumeler()[1].joker_konumlari().count(), 0);
    }

    #[test]
    fn dolu_yaprakta_benzemeyenler_tasma_kumesine_gider() {
        // Her konumu az değer alan ama birleşimleri sayısız mesajlar: hiçbiri bir başkasına
        // benzemez (3 belirtecin en fazla 1'i aynı). Yaprak dolunca yenileri taşma kümesinde
        // toplanır; bir mesaj en fazla yapraktaki kümelerle karşılaştırılır.
        for esnek_esik in [false, true] {
            let ayarlar = Ayarlar {
                en_fazla_cocuk: 3,
                esnek_esik,
                ..Ayarlar::default()
            };
            let sinir = YAPRAK_KATSAYISI * ayarlar.en_fazla_cocuk;
            let mesajlar: Vec<String> = (0..sinir + 20)
                .map(|sira| format!("olay {} {}", ad(sira), ad(sira + 7_000)))
                .collect();
            let mut drain = Drain::new(ayarlar.clone());
            for mesaj in &mesajlar {
                drain.ekle(mesaj);
            }
            if esnek_esik {
                // Esnek eşikte konumlar `en_fazla_cocuk`'tan fazla değer alınca değişkendir:
                // ikinci turda hepsi tek kalıptır
                assert!(!drain.degiskenler.is_empty());
                let (kumeler, _) = kumele(&ayarlar, &mesajlar);
                assert_eq!(ozetle(&kumeler), [format!("{} × olay <*> <*>", sinir + 20)]);
                continue;
            }
            assert_eq!(drain.kumeler.len(), sinir + 1);
            let tasma = &drain.kumeler[sinir];
            assert_eq!((tasma.adet, tasma.metin().as_str()), (20, "olay <*> <*>"));
            let yaprak = drain.yaprak_mut(&["olay", "x", "y"]).unwrap();
            assert_eq!(yaprak.kume_numaralari.len(), sinir);
            assert_eq!(yaprak.tasma, Some(sinir));
            // Yapraktaki bir kümeye benzeyen mesaj yine ona katılır
            assert_eq!(drain.ekle(&mesajlar[3]), 3);
            // Önbellekli kümeleme aynı sonucu verir
            let mut kumeleyici = Kumeleyici::new(ayarlar.clone());
            let numaralar: Vec<usize> = mesajlar
                .iter()
                .chain(&mesajlar)
                .enumerate()
                .map(|(no, mesaj)| kumeleyici.ekle_metin(no % mesajlar.len(), mesaj))
                .collect();
            let iki_kez: Vec<&String> = mesajlar.iter().chain(&mesajlar).collect();
            let (referans, referans_numaralari) = kumele(&ayarlar, &iki_kez);
            assert_eq!(numaralar, referans_numaralari);
            assert_eq!(ozetle(kumeleyici.kumeler()), ozetle(&referans));
        }
        // Tek belirteçli mesajların yaprağı dolmaz (her mesaj zaten en fazla iki kümeyle
        // karşılaştırılır): her farklı kelime, yaprağın sınırından sonra da kendi kümesidir
        for esnek_esik in [false, true] {
            let ayarlar = Ayarlar {
                en_fazla_cocuk: 3,
                esnek_esik,
                ..Ayarlar::default()
            };
            let kelimeler: Vec<String> = (0..YAPRAK_KATSAYISI * 3 + 20).map(ad).collect();
            let (kumeler, _) = kumele(&ayarlar, &kelimeler);
            assert_eq!(kumeler.len(), kelimeler.len());
        }
    }

    #[test]
    fn onbellek_agac_degisince_yeniden_arar() {
        // "a b c" önce kendi kümesini açar; sonra aynı uzunlukta bir mesaj o kümenin kalıbını
        // değiştirir. "a b c" yeniden geldiğinde önbellekteki sonuç eskidir; yeniden aranır.
        let mut kumeleyici = Kumeleyici::new(Ayarlar::default());
        assert_eq!(kumeleyici.ekle_metin(0, "a b c d"), 0);
        assert_eq!(kumeleyici.ekle_metin(1, "a b c e"), 0);
        assert_eq!(kumeleyici.ekle_metin(0, "a b c d"), 0);
        // Başka uzunluktaki bir değişiklik önbelleği bozmaz
        assert_eq!(kumeleyici.ekle_metin(2, "x y"), 1);
        assert_eq!(kumeleyici.ekle_metin(0, "a b c d"), 0);
        assert_eq!(ozetle(kumeleyici.kumeler()), ["4 × a b c <*>", "1 × x y"]);
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
                assert_eq!(
                    drain.ara(&belirtecler),
                    tam_tarama.map_or(Arama::Yok, Arama::Bulundu),
                    "{mesaj}"
                );
                drain.ekle(&mesaj);
            }
        }
    }
}
