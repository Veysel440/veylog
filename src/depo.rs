//! Özetin deposu: kayıtlar okunurken her birinden özet için gerekenler alınır.
//!
//! Kayıtlar akış halinde, teker teker gelir; kendileri saklanmaz. Depoda kalanlar:
//!
//! - **Alt gruplar.** Maskeli mesajı, Drain kümesi, istisnası (konumun gösterilen satırıyla) ve
//!   seviyesi aynı olan kayıtlar. Her alt grubun kayıt sayısı, ilk kaydının mesajı (örnek), ilk
//!   ve son görüldüğü zaman, en geç kaydının sırası. Kalıp tablosunun satırları alt
//!   gruplardan kurulur; satırın konumu en geç kaydı olan alt grubunkidir.
//! - **Maskeli mesajlar.** Her farklı maskeli mesaj bir kez, ilk görüldüğü sırayla.
//! - **Drain.** Her kaydın maskeli mesajı, logdaki sırasıyla kümelenir (`drain::Clusterer`):
//!   Drain sıraya bağlıdır, aynı mesaj logun farklı yerlerinde farklı kümelere katılabilir. Bir
//!   mesajın tekrarlarında ağaç değişmediyse önceki sonuç kullanılır; arama yapılmaz.
//! - **Kayıt sütunları.** Her kaydın zamanı ve alt grubu, sıkıştırılmış olarak (`columns`
//!   modülü): kayıt başına çoğunlukla 2–3 bayt. Karşılaştırma ve ani artış bunlarla hesaplanır.
//! - **Seyrek dizin.** Her `DIZIN_ARALIGI` kayıtta bir kaydın girdideki konumu: bir kaydın
//!   mesajı sonradan gerekirse girdinin yalnızca o kısmı yeniden okunur.
//! - **İstisnalar.** Parmak izleri ve projelerin kökleri. Kökü kendi yollarında olmayan bir
//!   kaydın istisnası taslak olarak saklanır; parmak izi okuma bitince çıkarılır. Konum
//!   fonksiyonla belirleniyorsa yığın izlerinden hangi satırın hangi fonksiyonda olduğu da
//!   öğrenilir (`MetotHaritasi`): çağrıları bilinmeyen kayıtların (JSON'un yığın izi)
//!   konumları okuma bitince bununla tamamlanır.
//! - **Sayılar.** Seviyeler, farklı ham mesajlar (`distinct` modülü), en eski ve en yeni
//!   kayıt, kaç kaydın başka bir saat diliminden çevrildiği.
//!
//! Kayıt zamanları raporun saat dilimine çevrilerek alınır (`datetime::ZoneConverter`): sütunlardaki,
//! alt gruplardaki ve filtredeki bütün zamanlar bu dilimdedir.
//!
//! Drain ilk turda bir yaprakta değişken bir konum bulursa (`drain` modülü) kayıtlar okuma
//! bitince ikinci kez kümelenir. Girdi yeniden okunmaz: her kaydın maskeli mesajı sütunlardaki
//! alt grubundan bilinir. Bir alt grubun kayıtları ikinci turda farklı kümelere düşerse alt
//! grup bölünür; bölünen parçanın ilk kaydı alt grubun ilk kaydı değilse mesajı depoda yoktur,
//! girdiden yeniden okunur (`ornekleri_tamamla`).
//!
//! Filtre verildiyse (`filtre` modülü) filtreye uymayan kayıt depoya hiç girmez; yalnızca
//! sayılır. Depodaki kayıtların sıra numaraları da yalnızca filtreye uyanlar arasındadır.
//!
//! Bellek böylece dosyanın boyuyla değil, farklı mesajların ve alt grupların sayısıyla büyür;
//! kayıt sayısıyla yalnızca sütunlar büyür.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::Hash;
use std::io::{self, BufRead};
use std::rc::Rc;

use crate::columns::{ColumnsIter, RecordColumns};
use crate::datetime::{DateTime, RecordZone, TimeZone, ZoneConverter};
use crate::distinct::DistinctCounter;
use crate::drain::{self, Cluster, Clusterer};
use crate::filtre::CozulmusFiltre;
use crate::kayit::{Kayit, KayitAlici, KayitBilgisi, SatirSayilari, Seviye};
use crate::mask::mask;
use crate::okuyucu::{self, DIZIN_ARALIGI, OkumaSonucu, OrnekIstegi, OrnekKaynagi, SeyrekDizin};
use crate::parmak_izi::{Istisna, IstisnaTaslagi, KonumTuru, MetotHaritasi, ParmakIzi};
use crate::satir_bicimi::SatirBicimi;

/// Bir kaydın istisnası: yok, parmak izi belli ya da bütün kökler bilinince çıkarılacak.
/// Numaralar depodaki listelerin sırasıdır.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum IstisnaNo {
    Yok,
    Iz(u32),
    Taslak(u32),
}

/// Alt grubu belirleyenler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct AltGrupAnahtari {
    maskeli: u32,
    /// Drain kümesinin numarası.
    kume: u32,
    istisna: IstisnaNo,
    /// Parmak izinin gösterilen satırı. Konum fonksiyonla belirleniyorsa aynı hatanın kayıtları
    /// farklı satırlarda olabilir (kod değişmiş): her satır ayrı bir alt gruptur, özet en yeni
    /// kaydınkini gösterir. Parmak izi okuma bitince çıkarılanlarda (taslaklar) None: taslağın
    /// bütün kayıtlarının satırları aynıdır.
    satir: Option<u32>,
    seviye: Seviye,
}

/// Maskeli mesajı, Drain kümesi, istisnası (gösterilen satırıyla) ve seviyesi aynı olan
/// kayıtlar.
#[derive(Debug, Clone)]
pub struct AltGrup {
    anahtar: AltGrupAnahtari,
    adet: usize,
    /// İlk kaydının sıra numarası ve maskelenmemiş mesajı. Maskeleme mesajı değiştirmediyse
    /// (mesajda sayı, adres... yoksa) örnek, maskeli mesajla aynı metni paylaşır: her mesajı
    /// farklı bir logda metinler iki kez saklanmaz.
    ilk_kayit: usize,
    ornek: Rc<str>,
    /// Örnek ilk kaydın kendi mesajı mı? İkinci kümeleme turunda bölünen bir alt grubun
    /// parçasında, girdi yeniden okunana kadar aynı maskeli mesajlı bir başka kaydınkidir.
    ornek_kesin: bool,
    /// En erken ve en geç kaydının zamanı.
    ilk: DateTime,
    son: DateTime,
    /// En geç kaydının sıra numarası: zamanı `son` olanlardan logda en sonra geleni. Bir
    /// tablo satırının alt gruplarından en yeni kaydı olanı bununla seçilir.
    son_kayit: usize,
}

impl AltGrup {
    /// Maskeli mesajın numarası: `Depo::maskeli_mesajlar` listesindeki sırası.
    pub fn maskeli(&self) -> usize {
        self.anahtar.maskeli as usize
    }

    /// Drain kümesinin numarası: `Depo::kumeler` listesindeki sırası.
    pub fn kume(&self) -> usize {
        self.anahtar.kume as usize
    }

    pub fn seviye(&self) -> Seviye {
        self.anahtar.seviye
    }

    pub fn adet(&self) -> usize {
        self.adet
    }

    pub fn ilk_kayit(&self) -> usize {
        self.ilk_kayit
    }

    pub fn ornek(&self) -> &str {
        &self.ornek
    }

    /// Örnek ilk kaydın kendi mesajı mı? İkinci kümeleme turunda bölünen bir alt grubun
    /// parçasında girdi yeniden okunana kadar değildir.
    #[cfg(test)]
    pub fn ornek_kesin(&self) -> bool {
        self.ornek_kesin
    }

    pub fn ilk(&self) -> DateTime {
        self.ilk
    }

    pub fn son(&self) -> DateTime {
        self.son
    }

    /// En geç kaydının sıra numarası: zamanı `son` olanlardan logda en sonra geleni.
    pub fn son_kayit(&self) -> usize {
        self.son_kayit
    }
}

/// Kayıtların saat dilimleri: raporun dilimine nasıl alındıkları.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DilimSayilari {
    /// Başka bir dilimde yazılmış, raporun dilimine çevrilen kayıtlar.
    pub cevrilen: usize,
    /// Çevrilen kayıtların dilimleri, küçükten büyüğe.
    pub cevrilen_dilimler: BTreeSet<TimeZone>,
    /// Saat dilimi yazılmamış, raporun diliminde yazılmış sayılan kayıtlar.
    pub yazilmamis: usize,
}

/// Özetin deposu. Kayıtları `KayitAlici` olarak alır.
#[derive(Debug)]
pub struct Depo {
    kumeleyici: Clusterer,
    /// Filtre; None ise bütün kayıtlar alınır.
    filtre: Option<CozulmusFiltre>,
    /// Kayıt zamanlarını raporun saat dilimine çevirir.
    cevirici: ZoneConverter,
    /// Satırların biçimi; kayıtlar yeniden okunurken de gerekir.
    satir_bicimi: SatirBicimi,
    /// Parmak izlerinin konumu neyle belirlenir (`--konum`).
    konum_turu: KonumTuru,
    dilimler: DilimSayilari,
    sayilar: SatirSayilari,
    gecersiz_utf8: bool,
    columns: RecordColumns,
    /// Her `DIZIN_ARALIGI` kayıtta bir kaydın girdideki konumu.
    dizin: SeyrekDizin,
    /// Girdi baştan başladı (`yeni_girdi`): gelen ilk kaydın konumu da dizine yazılır.
    yeni_girdi: bool,
    /// `Seviye`nin sırasıyla her seviyenin kayıt sayısı.
    seviye_sayilari: [usize; 8],
    /// Logun en eski ve en yeni kaydının zamanı.
    en_eski: Option<DateTime>,
    en_yeni: Option<DateTime>,
    /// Okunan en yeni kaydın zamanı, filtreye uymasa da. Canlı izlemede kayan pencere ona göre
    /// çözülür: seviye filtresine uymayan kayıtlar da logun sonunu ilerletir.
    okunan_en_yeni: Option<DateTime>,
    /// Saat dilimi sayılarının kaç kayıttan sayıldığı; None ise depodaki kayıtlardan. Zaman
    /// penceresinde (`pencere`) dilim sayıları okunan bütün kayıtlarındır.
    dilim_kapsami: Option<usize>,
    ham_mesajlar: DistinctCounter,
    /// Maskeli mesajlar, ilk görülme sırasıyla; numaraları bu listedeki sıralarıdır. Metinler
    /// `Rc` ile paylaşılır: liste ile sözlük aynı metni gösterir.
    maskeliler: Vec<Rc<str>>,
    maskeli_sirasi: HashMap<Rc<str>, u32>,
    /// Kayıtların kendi yollarında bulunan proje kökleri.
    kokler: BTreeSet<String>,
    izler: Vec<ParmakIzi>,
    iz_sirasi: HashMap<ParmakIzi, u32>,
    /// Yığın izlerinden öğrenilen: hangi satır hangi fonksiyonun içinde. Yalnızca konum
    /// fonksiyonla belirleniyorsa doldurulur.
    metot_haritasi: MetotHaritasi,
    taslaklar: Vec<IstisnaTaslagi>,
    taslak_sirasi: HashMap<IstisnaTaslagi, u32>,
    alt_gruplar: Vec<AltGrup>,
    /// Okunurken: alt grubun anahtarı → numarası. Okuma bitince boşaltılır.
    alt_grup_sirasi: HashMap<AltGrupAnahtari, u32>,
}

impl KayitAlici for Depo {
    fn kayit(&mut self, kayit: &Kayit<'_>, bilgi: KayitBilgisi) {
        // Zaman raporun saat diliminde: filtre de bu dilimdedir. Dilim, filtreye uymayan
        // kayıtlarla da belirlenir; ön tarama ve yeniden okuma da her kayda bakar.
        let (zaman, dilim) = self.cevirici.convert(kayit.zaman());
        let seviye = kayit.seviye();
        self.okunan_en_yeni = Some(
            self.okunan_en_yeni
                .map_or(zaman, |en_yeni| en_yeni.max(zaman)),
        );
        if let Some(filtre) = &self.filtre
            && !filtre.gecer(zaman.epoch_seconds(), seviye)
        {
            return;
        }
        // Kaydın depodaki sıra numarası: filtreye uyan kayıtlar arasında kaçıncı olduğu
        let sira = self.columns.len();
        self.seviye_sayilari[seviye as usize] += 1;
        match dilim {
            RecordZone::Missing => self.dilimler.yazilmamis += 1,
            RecordZone::Same => {}
            RecordZone::Other(kendi) => {
                self.dilimler.cevrilen += 1;
                self.dilimler.cevrilen_dilimler.insert(kendi);
            }
        }
        self.en_eski = Some(self.en_eski.map_or(zaman, |en_eski| en_eski.min(zaman)));
        self.en_yeni = Some(self.en_yeni.map_or(zaman, |en_yeni| en_yeni.max(zaman)));
        self.ham_mesajlar.insert(kayit.mesaj());

        let (maskeli, maskelendi) = self.maskeli_no(kayit.mesaj());
        let kume = self
            .kumeleyici
            .add(maskeli as usize, &self.maskeliler[maskeli as usize]);
        let (istisna, satir) = self.istisna_no(kayit);
        let anahtar = AltGrupAnahtari {
            maskeli,
            kume: numara(kume),
            istisna,
            satir,
            seviye,
        };
        let no = match self.alt_grup_sirasi.get(&anahtar) {
            Some(&no) => {
                let alt = &mut self.alt_gruplar[no as usize];
                alt.adet += 1;
                alt.ilk = alt.ilk.min(zaman);
                // Aynı andaki kayıtlardan logda sonra gelen daha yenidir
                if zaman >= alt.son {
                    alt.son_kayit = sira;
                }
                alt.son = alt.son.max(zaman);
                no
            }
            None => {
                let no = numara(self.alt_gruplar.len());
                let ornek = if maskelendi {
                    Rc::from(kayit.mesaj())
                } else {
                    Rc::clone(&self.maskeliler[maskeli as usize])
                };
                self.alt_gruplar.push(AltGrup {
                    anahtar,
                    adet: 1,
                    ilk_kayit: sira,
                    ornek,
                    ornek_kesin: true,
                    ilk: zaman,
                    son: zaman,
                    son_kayit: sira,
                });
                self.alt_grup_sirasi.insert(anahtar, no);
                no
            }
        };
        self.columns.push(zaman.epoch_seconds(), no);
        if self.yeni_girdi || sira.is_multiple_of(DIZIN_ARALIGI) {
            self.dizin.ekle(sira, bilgi.konum);
            self.yeni_girdi = false;
        }
    }
}

/// Deponun kayıtları nasıl alacağı.
#[derive(Debug, Clone, Default)]
pub struct DepoAyarlari {
    /// Kayıtların mesajlarının kümeleneceği Drain ayarları.
    pub drain: drain::Settings,
    /// Filtre, logun sonuna göre çözülmüş olarak; None ise bütün kayıtlar alınır.
    pub filtre: Option<CozulmusFiltre>,
    /// Raporun saat dilimi (`--saat-dilimi`); None ise logda saat dilimi yazılı ilk kaydınki.
    pub saat_dilimi: Option<TimeZone>,
    /// Satırların biçimi (`--desen`, `--zaman-bicimi`); varsayılanı Laravel'inki.
    pub satir_bicimi: SatirBicimi,
    /// Parmak izlerinin konumu neyle belirlenir (`--konum`).
    pub konum_turu: KonumTuru,
}

impl Depo {
    /// Boş bir depo.
    pub fn new(ayarlar: DepoAyarlari) -> Depo {
        Depo {
            kumeleyici: Clusterer::new(ayarlar.drain),
            filtre: ayarlar.filtre,
            cevirici: ZoneConverter::new(ayarlar.saat_dilimi),
            satir_bicimi: ayarlar.satir_bicimi,
            konum_turu: ayarlar.konum_turu,
            dilimler: DilimSayilari::default(),
            sayilar: SatirSayilari::default(),
            gecersiz_utf8: false,
            columns: RecordColumns::default(),
            dizin: SeyrekDizin::default(),
            yeni_girdi: false,
            seviye_sayilari: [0; 8],
            en_eski: None,
            en_yeni: None,
            okunan_en_yeni: None,
            dilim_kapsami: None,
            ham_mesajlar: DistinctCounter::default(),
            maskeliler: Vec::new(),
            maskeli_sirasi: HashMap::new(),
            kokler: BTreeSet::new(),
            izler: Vec::new(),
            iz_sirasi: HashMap::new(),
            metot_haritasi: MetotHaritasi::default(),
            taslaklar: Vec::new(),
            taslak_sirasi: HashMap::new(),
            alt_gruplar: Vec::new(),
            alt_grup_sirasi: HashMap::new(),
        }
    }

    /// Girdiyi baştan sona okuyup depoyu doldurur.
    pub fn doldur(ayarlar: DepoAyarlari, girdi: impl BufRead) -> io::Result<Depo> {
        let mut depo = Depo::new(ayarlar);
        // Biçim depodan ödünç alınamaz: depo okuma boyunca alıcı olarak değişir
        let bicim = depo.satir_bicimi.clone();
        let sonuc = okuyucu::oku(girdi, &bicim, &mut depo)?;
        depo.okuma_bitti(sonuc);
        Ok(depo)
    }

    /// Kayıtları süzen filtre; yoksa None.
    pub fn filtre(&self) -> Option<&CozulmusFiltre> {
        self.filtre.as_ref()
    }

    /// Satırların biçimi.
    pub fn satir_bicimi(&self) -> &SatirBicimi {
        &self.satir_bicimi
    }

    /// Okuma bitince satır sayılarını ve geçersiz UTF-8 bilgisini kaydeder. Drain ilk turda
    /// değişken bir konum bulduysa kayıtları ikinci kez kümeler.
    pub fn okuma_bitti(&mut self, sonuc: OkumaSonucu) {
        self.ara_durum(sonuc);
        // Yalnızca okunurken gerekir
        self.alt_grup_sirasi = HashMap::new();
    }

    /// Okuma sürerken bir ara rapor için (canlı izleme): satır sayılarını ve geçersiz UTF-8
    /// bilgisini kaydeder, Drain değişken bir konum bulduysa kayıtları yeniden kümeler.
    /// Okuma durumu korunur: sonra kayıt almaya devam edilebilir.
    ///
    /// Yeniden kümelemede alt grupların küme numaraları (bölünürlerse kendileri) değişir;
    /// okunurken kullanılan anahtar → numara sözlüğü yeni anahtarlarla kurulur. Yeni bir
    /// değişken konum sonraki çizimlerde de bulunabilir: her çizimde en fazla bir tur yapılır,
    /// önceki turlarda öğrenilenler unutulmaz (`Clusterer::ikinci_tur`).
    pub fn ara_durum(&mut self, sonuc: OkumaSonucu) {
        self.sayilar = sonuc.sayilar;
        self.gecersiz_utf8 = sonuc.gecersiz_utf8;
        if self.kumeleyici.needs_second_pass() {
            self.ikinci_tur();
            self.alt_grup_sirasi = self
                .alt_gruplar
                .iter()
                .enumerate()
                .map(|(no, alt)| (alt.anahtar, numara(no)))
                .collect();
        }
    }

    /// Deponun bir filtreye uyan kayıtlardaki görünümü: canlı izlemede `--son 1sa` gibi
    /// zaman sınırları her çizimde o ana kadar okunan en yeni kayda göre uygulanır. Kalıplar,
    /// maskeli mesajlar, parmak izleri, kökler ve fonksiyon haritası okunan bütün
    /// kayıtlardan gelir (kümeleme yinelenmez, pencerenin dışındakiler de öğretir); alt
    /// grupların sayıları ve zamanları, seviyeler ve zaman aralığı yalnızca filtreye uyan
    /// kayıtlardan toplanır. Farklı ham mesaj ve saat dilimi sayıları da okunan bütün
    /// kayıtlar içindir: kayıt başına saklanmazlar.
    ///
    /// Görünümde kayıtlar yeniden numaralanır. Dönen listede görünümün k. kaydının bu depodaki
    /// sıra numarası vardır; girdiyi yeniden okuyan kaynak istekleri bununla çevirir. Görünümün
    /// seyrek dizini yoktur. Örneği kesin olmayan bir alt grup (pencereye ilk kaydıyla
    /// girmeyen) görünümde de kesin değildir: örneği `ornekleri_tamamla` ile yeniden okunur.
    pub fn pencere(&self, filtre: CozulmusFiltre) -> (Depo, Vec<usize>) {
        let mut columns = RecordColumns::default();
        let mut tam_sira = Vec::new();
        let mut alt_gruplar: Vec<AltGrup> = Vec::new();
        let mut yeni_no: Vec<Option<u32>> = vec![None; self.alt_gruplar.len()];
        let mut seviye_sayilari = [0; 8];
        let (mut en_eski, mut en_yeni) = (None, None);
        for (sira, (saniye, eski_no)) in self.columns.iter().enumerate() {
            let eski = &self.alt_gruplar[eski_no as usize];
            let seviye = eski.anahtar.seviye;
            if !filtre.gecer(saniye, seviye) {
                continue;
            }
            let yeni_sira = tam_sira.len();
            tam_sira.push(sira);
            let zaman = DateTime::from_epoch_seconds(saniye);
            seviye_sayilari[seviye as usize] += 1;
            en_eski = Some(en_eski.map_or(zaman, |en_eski: DateTime| en_eski.min(zaman)));
            en_yeni = Some(en_yeni.map_or(zaman, |en_yeni: DateTime| en_yeni.max(zaman)));
            let no = match yeni_no[eski_no as usize] {
                Some(no) => {
                    let alt = &mut alt_gruplar[no as usize];
                    alt.adet += 1;
                    alt.ilk = alt.ilk.min(zaman);
                    if zaman >= alt.son {
                        alt.son_kayit = yeni_sira;
                    }
                    alt.son = alt.son.max(zaman);
                    no
                }
                None => {
                    let no = numara(alt_gruplar.len());
                    alt_gruplar.push(AltGrup {
                        anahtar: eski.anahtar,
                        adet: 1,
                        ilk_kayit: yeni_sira,
                        ornek: Rc::clone(&eski.ornek),
                        ornek_kesin: eski.ornek_kesin && eski.ilk_kayit == sira,
                        ilk: zaman,
                        son: zaman,
                        son_kayit: yeni_sira,
                    });
                    yeni_no[eski_no as usize] = Some(no);
                    no
                }
            };
            columns.push(saniye, no);
        }
        let depo = Depo {
            kumeleyici: self.kumeleyici.clusters_only(),
            filtre: Some(filtre),
            cevirici: self.cevirici,
            satir_bicimi: self.satir_bicimi.clone(),
            konum_turu: self.konum_turu,
            dilimler: self.dilimler.clone(),
            sayilar: self.sayilar,
            gecersiz_utf8: self.gecersiz_utf8,
            columns,
            dizin: SeyrekDizin::default(),
            yeni_girdi: false,
            seviye_sayilari,
            en_eski,
            en_yeni,
            okunan_en_yeni: self.okunan_en_yeni,
            dilim_kapsami: Some(self.dilim_kapsami()),
            ham_mesajlar: self.ham_mesajlar.clone(),
            maskeliler: self.maskeliler.clone(),
            maskeli_sirasi: HashMap::new(),
            kokler: self.kokler.clone(),
            izler: self.izler.clone(),
            iz_sirasi: HashMap::new(),
            metot_haritasi: self.metot_haritasi.clone(),
            taslaklar: self.taslaklar.clone(),
            taslak_sirasi: HashMap::new(),
            alt_gruplar,
            alt_grup_sirasi: HashMap::new(),
        };
        (depo, tam_sira)
    }

    /// İkinci kümeleme turu: kayıtlar, logdaki sırasıyla, ilk turda öğrenilen değişken
    /// konumlarla baştan kümelenir. Her kaydın maskeli mesajı sütunlardaki alt grubundan
    /// bilinir.
    ///
    /// Her alt grubun bütün kayıtları ikinci turda da aynı kümeye düşüyorsa (neredeyse her
    /// zaman) yalnızca alt grupların küme numaraları değişir. Değilse alt gruplar bölünür
    /// (`bolerek_kur`). İki alt grubun anahtarı aynı olursa da alt gruplar yeniden kurulur, bu
    /// kez birleşerek: Drain sıraya bağlı olduğu için aynı maskeli mesaj ilk turda iki kümeye
    /// düşmüş, ikinci turda aynı kümeye düşmüş olabilir.
    fn ikinci_tur(&mut self) {
        let mut kumeleyici = self.kumeleyici.second_pass();
        let mut kumeler: Vec<Option<u32>> = vec![None; self.alt_gruplar.len()];
        for (_, no) in self.columns.iter() {
            let maskeli = self.alt_gruplar[no as usize].anahtar.maskeli;
            let kume = numara(kumeleyici.add(maskeli as usize, &self.maskeliler[maskeli as usize]));
            match kumeler[no as usize] {
                None => kumeler[no as usize] = Some(kume),
                Some(onceki) if onceki == kume => {}
                // Bölünen bir alt grup: tur baştan, alt grupları bölerek yapılır
                Some(_) => return self.bolerek_kur(),
            }
        }
        let anahtarlar: Vec<AltGrupAnahtari> = self
            .alt_gruplar
            .iter()
            .zip(&kumeler)
            .map(|(alt, kume)| AltGrupAnahtari {
                kume: kume.expect("her alt grubun en az bir kaydı var"),
                ..alt.anahtar
            })
            .collect();
        let mut gorulen = HashSet::with_capacity(anahtarlar.len());
        if !anahtarlar.iter().all(|anahtar| gorulen.insert(*anahtar)) {
            return self.bolerek_kur();
        }
        for (alt, anahtar) in self.alt_gruplar.iter_mut().zip(anahtarlar) {
            alt.anahtar = anahtar;
        }
        self.kumeleyici = kumeleyici;
    }

    /// Bölünen ya da birleşen alt gruplarla ikinci tur: alt gruplar yeni kümeleriyle, ilk
    /// kayıtlarının sırasıyla yeniden kurulur; sütunlar yeni numaralarla yeniden yazılır.
    ///
    /// Bölünen bir alt grubun parçası: ilk kaydı alt grubun ilk kaydı değilse mesajı depoda
    /// yoktur; örneği şimdilik alt grubunkidir (`ornek_kesin` false).
    fn bolerek_kur(&mut self) {
        let mut kumeleyici = self.kumeleyici.second_pass();
        let mut alt_gruplar: Vec<AltGrup> = Vec::new();
        let mut sirasi: HashMap<AltGrupAnahtari, u32> = HashMap::new();
        let mut columns = RecordColumns::default();
        for (sira, (saniye, eski_no)) in self.columns.iter().enumerate() {
            let eski = &self.alt_gruplar[eski_no as usize];
            let maskeli = eski.anahtar.maskeli;
            let kume = kumeleyici.add(maskeli as usize, &self.maskeliler[maskeli as usize]);
            let anahtar = AltGrupAnahtari {
                kume: numara(kume),
                ..eski.anahtar
            };
            let zaman = DateTime::from_epoch_seconds(saniye);
            let no = match sirasi.get(&anahtar) {
                Some(&no) => {
                    let alt = &mut alt_gruplar[no as usize];
                    alt.adet += 1;
                    alt.ilk = alt.ilk.min(zaman);
                    if zaman >= alt.son {
                        alt.son_kayit = sira;
                    }
                    alt.son = alt.son.max(zaman);
                    no
                }
                None => {
                    let no = numara(alt_gruplar.len());
                    alt_gruplar.push(AltGrup {
                        anahtar,
                        adet: 1,
                        ilk_kayit: sira,
                        ornek: Rc::clone(&eski.ornek),
                        ornek_kesin: eski.ornek_kesin && eski.ilk_kayit == sira,
                        ilk: zaman,
                        son: zaman,
                        son_kayit: sira,
                    });
                    sirasi.insert(anahtar, no);
                    no
                }
            };
            columns.push(saniye, no);
        }
        self.kumeleyici = kumeleyici;
        self.alt_gruplar = alt_gruplar;
        self.columns = columns;
    }

    /// Örneği kesin olmayan alt grupların (ikinci turda bölünenlerin) ilk kayıtlarını girdiden
    /// yeniden okur. Bulunamayanın örneği aynı maskeli mesajlı ilk kaydınki kalır.
    pub fn ornekleri_tamamla(&mut self, kaynak: &mut dyn OrnekKaynagi) -> io::Result<()> {
        let istekler: Vec<OrnekIstegi<'_>> = self
            .alt_gruplar
            .iter()
            .filter(|alt| !alt.ornek_kesin)
            .map(|alt| OrnekIstegi {
                sira: alt.ilk_kayit,
                maskeli: &self.maskeliler[alt.maskeli()],
                seviye: alt.seviye(),
            })
            .collect();
        if istekler.is_empty() {
            return Ok(());
        }
        let mesajlar = kaynak.mesajlar(&istekler)?;
        for alt in &mut self.alt_gruplar {
            if !alt.ornek_kesin
                && let Some(mesaj) = mesajlar.get(&alt.ilk_kayit)
            {
                alt.ornek = Rc::from(mesaj.as_str());
                alt.ornek_kesin = true;
            }
        }
        Ok(())
    }

    /// Drain kümeleri, açılış sırasıyla; kalıpları son halleriyle.
    pub fn kumeler(&self) -> &[Cluster] {
        self.kumeleyici.clusters()
    }

    /// Satır sayıları (okuma bitince).
    pub fn sayilar(&self) -> SatirSayilari {
        self.sayilar
    }

    pub fn gecersiz_utf8(&self) -> bool {
        self.gecersiz_utf8
    }

    /// Depodaki (filtre varsa filtreye uyan) kayıt sayısı. Dosyadaki bütün kayıtlar:
    /// `sayilar().kayit`.
    pub fn kayit_sayisi(&self) -> usize {
        self.columns.len()
    }

    /// Kayıtlar logdaki sırayla: (zaman, alt grup numarası). Zaman 1970'ten bu yana saniyedir.
    pub fn kayitlar(&self) -> ColumnsIter<'_> {
        self.columns.iter()
    }

    pub fn alt_gruplar(&self) -> &[AltGrup] {
        &self.alt_gruplar
    }

    /// Alt grubun seviyesi; numarası `kayitlar`dan gelir.
    pub fn alt_grup_seviyesi(&self, alt_grup: u32) -> Seviye {
        self.alt_gruplar[alt_grup as usize].anahtar.seviye
    }

    /// Farklı maskeli mesajların sayısı.
    pub fn maskeli_mesaj_sayisi(&self) -> usize {
        self.maskeliler.len()
    }

    /// Numarası verilen maskeli mesaj (`AltGrup::maskeli`).
    pub fn maskeli_mesaj(&self, no: usize) -> &str {
        &self.maskeliler[no]
    }

    /// Farklı maskeli mesajlar, ilk görüldükleri sırayla.
    #[cfg(test)]
    pub fn maskeli_mesajlar(&self) -> Vec<&str> {
        self.maskeliler.iter().map(|mesaj| &**mesaj).collect()
    }

    /// Her alt grubun parmak izi, alt gruplarla aynı sırada. Taslakların parmak izi logun
    /// bütün kökleriyle çıkarılır. Konum fonksiyonla belirleniyorsa satırla kalmış konumlar
    /// (çağrıları bilinmeyen kayıtlar) bütün yığın izlerinden öğrenilenle tamamlanır.
    pub fn parmak_izleri(&self) -> Vec<Option<ParmakIzi>> {
        let taslak_istisnalari: Vec<Istisna<'_>> =
            self.taslaklar.iter().map(IstisnaTaslagi::istisna).collect();
        // Taslakların kökü artık bilinir; her birininki bir kez bulunur
        let taslak_kokleri: Vec<Option<&str>> = taslak_istisnalari
            .iter()
            .map(|istisna| istisna.cozulmus_kok(&self.kokler))
            .collect();
        let harita = match self.konum_turu {
            KonumTuru::Metot => {
                // Taslakların yığın izleri de katılır
                let mut harita = self.metot_haritasi.clone();
                for (istisna, &kok) in taslak_istisnalari.iter().zip(&taslak_kokleri) {
                    harita.ogren(istisna, kok);
                }
                Some(harita)
            }
            KonumTuru::Satir => None,
        };
        let tamamla = |mut iz: ParmakIzi| {
            if let Some(harita) = &harita {
                harita.tamamla(&mut iz);
            }
            iz
        };
        let izler: Vec<ParmakIzi> = self.izler.iter().cloned().map(tamamla).collect();
        let taslak_izleri: Vec<ParmakIzi> = taslak_istisnalari
            .iter()
            .zip(&taslak_kokleri)
            .map(|(istisna, &kok)| tamamla(istisna.koke_gore(kok, self.konum_turu)))
            .collect();
        self.alt_gruplar
            .iter()
            .map(|alt| match alt.anahtar.istisna {
                IstisnaNo::Yok => None,
                IstisnaNo::Iz(no) => {
                    let mut iz = izler[no as usize].clone();
                    iz.satiri_goster(alt.anahtar.satir);
                    Some(iz)
                }
                IstisnaNo::Taslak(no) => Some(taslak_izleri[no as usize].clone()),
            })
            .collect()
    }

    /// Farklı ham (maskelenmemiş) mesajların sayısı.
    pub fn ham_mesajlar(&self) -> &DistinctCounter {
        &self.ham_mesajlar
    }

    /// Her seviyeden kaç kayıt var; en önemli seviye başta, kaydı olmayan seviyeler yok.
    pub fn seviye_sayilari(&self) -> Vec<(Seviye, usize)> {
        Seviye::HEPSI
            .into_iter()
            .rev()
            .map(|seviye| (seviye, self.seviye_sayilari[seviye as usize]))
            .filter(|&(_, adet)| adet > 0)
            .collect()
    }

    /// En eski ve en yeni kaydın zamanı, raporun saat diliminde. Hiç kayıt yoksa None.
    pub fn zaman_araligi(&self) -> Option<(DateTime, DateTime)> {
        Some((self.en_eski?, self.en_yeni?))
    }

    /// En eski kaydın zamanı.
    pub fn en_eski(&self) -> Option<DateTime> {
        self.en_eski
    }

    /// En yeni kaydın zamanı.
    pub fn en_yeni(&self) -> Option<DateTime> {
        self.en_yeni
    }

    /// Okunan en yeni kaydın zamanı, filtreye uymasa da.
    pub fn okunan_en_yeni(&self) -> Option<DateTime> {
        self.okunan_en_yeni
    }

    /// Raporun saat dilimi: verildiyse o, verilmediyse logda saat dilimi yazılı ilk kaydınki.
    /// İkisi de yoksa None: zamanlar logda yazıldığı gibidir.
    pub fn saat_dilimi(&self) -> Option<TimeZone> {
        self.cevirici.zone()
    }

    /// Saat dilimi komut satırında mı verildi?
    pub fn saat_dilimi_verildi(&self) -> bool {
        self.cevirici.is_given()
    }

    /// Depodaki kayıtların saat dilimleri: kaçının çevrildiği, kaçında dilim yazılmadığı.
    /// Zaman penceresinde okunan bütün kayıtlarınkiler (`dilim_kapsami`).
    pub fn dilim_sayilari(&self) -> &DilimSayilari {
        &self.dilimler
    }

    /// Saat dilimi sayılarının kaç kayıttan sayıldığı.
    pub fn dilim_kapsami(&self) -> usize {
        self.dilim_kapsami.unwrap_or_else(|| self.kayit_sayisi())
    }

    /// Seyrek dizin: her `DIZIN_ARALIGI` kayıtta bir kaydın girdideki konumu.
    pub fn dizin(&self) -> &SeyrekDizin {
        &self.dizin
    }

    /// Girdi baştan başlıyor (canlı izlemede döndürülen dosya): bundan sonra gelen kayıtların
    /// konumları yeni girdidedir. Yeni girdinin ilk kaydı dizine yazılır; onun bloğundaki
    /// kayıtlar da yeni girdiden yeniden okunabilsin.
    pub fn yeni_girdi(&mut self) {
        self.yeni_girdi = true
    }

    /// Mesajı maskeler: maskeli mesajın numarası ve maskelemenin mesajı değiştirip
    /// değiştirmediği. Maskeli mesaj ilk kez görülüyorsa yeni bir numara verilir.
    fn maskeli_no(&mut self, mesaj: &str) -> (u32, bool) {
        let maskeli = mask(mesaj);
        let maskelendi = matches!(maskeli, Cow::Owned(_));
        if let Some(&no) = self.maskeli_sirasi.get(&*maskeli) {
            return (no, maskelendi);
        }
        let no = numara(self.maskeliler.len());
        let metin: Rc<str> = Rc::from(maskeli);
        self.maskeliler.push(Rc::clone(&metin));
        self.maskeli_sirasi.insert(metin, no);
        (no, maskelendi)
    }

    /// Kaydın istisnası ve parmak izinde gösterilecek satırı. Kökü kendi yollarındaysa parmak
    /// izi hemen çıkarılır; değilse istisna taslak olarak saklanır. Kaydın kökü, başka
    /// kayıtlar için bilinen köklere eklenir.
    fn istisna_no(&mut self, kayit: &Kayit<'_>) -> (IstisnaNo, Option<u32>) {
        let Some(istisna) = Istisna::kayittan(kayit) else {
            return (IstisnaNo::Yok, None);
        };
        match istisna.kok() {
            Some(kok) => {
                if !self.kokler.contains(kok) {
                    self.kokler.insert(kok.to_owned());
                }
                if self.konum_turu == KonumTuru::Metot {
                    self.metot_haritasi.ogren(&istisna, Some(kok));
                }
                let iz = istisna.koke_gore(Some(kok), self.konum_turu);
                let satir = iz.gosterilen_satir();
                let no = numaralandir(&mut self.iz_sirasi, &mut self.izler, iz);
                (IstisnaNo::Iz(no), satir)
            }
            None => {
                let no = numaralandir(
                    &mut self.taslak_sirasi,
                    &mut self.taslaklar,
                    istisna.taslak(),
                );
                (IstisnaNo::Taslak(no), None)
            }
        }
    }
}

/// Değerin numarası; listede yoksa sona eklenir.
fn numaralandir<T: Hash + Eq + Clone>(
    sira: &mut HashMap<T, u32>,
    liste: &mut Vec<T>,
    deger: T,
) -> u32 {
    if let Some(&no) = sira.get(&deger) {
        return no;
    }
    let no = numara(liste.len());
    liste.push(deger.clone());
    sira.insert(deger, no);
    no
}

/// Listedeki sıradan numara. Numaralar `u32`: depo kayıt başına yer kaplayan yerlerde küçük
/// kalsın diye. Dört milyardan fazla farklı mesaj ya da alt grup belleğe sığmazdı.
fn numara(sira: usize) -> u32 {
    u32::try_from(sira).expect("dört milyardan az farklı değer")
}

/// Testler için: metni okuyup varsayılan Drain ayarlarıyla depoya doldurur.
#[cfg(test)]
pub fn metinden(metin: &str) -> Depo {
    metinden_ayarlarla(metin, &drain::Settings::default())
}

/// Testler için: metni okuyup depoya doldurur.
#[cfg(test)]
pub fn metinden_ayarlarla(metin: &str, ayarlar: &drain::Settings) -> Depo {
    let ayarlar = DepoAyarlari {
        drain: ayarlar.clone(),
        ..DepoAyarlari::default()
    };
    Depo::doldur(ayarlar, metin.as_bytes()).expect("bellekten okuma")
}

#[cfg(test)]
mod testler {
    use super::*;
    use crate::filtre::Filtre;

    /// `DateTime` olarak: "2026-10-24 10:00:00"
    fn zaman(metin: &str) -> DateTime {
        DateTime::from_record(metin)
    }

    #[test]
    fn zaman_araligi_en_eski_ve_en_yeni_kayittir() {
        // Kayıtlar dosyada her zaman sıralı olmayabilir (aynı anda yazan birden fazla işlem)
        let depo = metinden(
            "[2026-10-24 10:00:05] local.INFO: b\n\
             [2026-10-24 09:59:59] local.INFO: a\n\
             [2026-10-24 10:00:01] local.INFO: c",
        );
        assert_eq!(
            depo.zaman_araligi(),
            Some((zaman("2026-10-24 09:59:59"), zaman("2026-10-24 10:00:05")))
        );
        assert_eq!(metinden("").zaman_araligi(), None);
        assert_eq!(metinden("").en_eski(), None);

        // Metin olarak karşılaştırılsaydı en eski 10:00 olurdu: " " < "T". Saat dilimi +00:00
        // (yazılı ilk kayıttan); 10:00+03:00, 07:00+00:00'dır.
        let depo = metinden(
            "[2026-10-24 10:00:00] local.ERROR: x\n\
             {\"message\":\"y\",\"level_name\":\"INFO\",\"datetime\":\"2026-10-24T09:00:00+00:00\"}\n\
             [2026-10-24 09:00:00] local.INFO: z\n\
             [2026-10-24T10:00:00+03:00] local.INFO: w",
        );
        assert_eq!(
            depo.zaman_araligi(),
            Some((zaman("2026-10-24 07:00:00"), zaman("2026-10-24 10:00:00")))
        );
    }

    #[test]
    fn kayitlar_raporun_saat_dilimine_cevrilir() {
        // Yaz saatinden kış saatine geçen bir log: önce +03:00, sonra +02:00. Raporun dilimi
        // logda yazılı ilk dilim (+03:00); +02:00'daki kayıtlar bir saat ileri alınır. Dilimsiz
        // kayıt raporun diliminde sayılır.
        let metin = "[2026-10-24 23:00:00] local.INFO: Gece görevi\n\
                     [2026-10-25T02:30:00+03:00] local.ERROR: Kuyruk 1 dolu\n\
                     [2026-10-25T02:10:00+02:00] local.ERROR: Kuyruk 2 dolu\n\
                     [2026-10-25T02:40:00+02:00] local.ERROR: Kuyruk 3 dolu";
        let depo = metinden(metin);
        assert_eq!(depo.saat_dilimi(), TimeZone::parse("+03:00"));
        assert!(!depo.saat_dilimi_verildi());
        let iki = TimeZone::parse("+02:00").unwrap();
        assert_eq!(
            depo.dilim_sayilari(),
            &DilimSayilari {
                cevrilen: 2,
                cevrilen_dilimler: BTreeSet::from([iki]),
                yazilmamis: 1,
            }
        );
        // 02:10+02:00 = 03:10+03:00: 02:30'dan sonradır
        let alt = depo.alt_gruplar();
        assert_eq!(alt[1].ilk(), zaman("2026-10-25 02:30:00"));
        assert_eq!(alt[1].son(), zaman("2026-10-25 03:40:00"));
        let saniyeler: Vec<i64> = depo.kayitlar().map(|(saniye, _)| saniye).collect();
        assert_eq!(
            saniyeler,
            [
                "2026-10-24 23:00:00",
                "2026-10-25 02:30:00",
                "2026-10-25 03:10:00",
                "2026-10-25 03:40:00"
            ]
            .map(|metin| zaman(metin).epoch_seconds())
        );

        // Dilim verilirse her kayıt ona çevrilir; filtre de bu dilimdedir
        let ayarlar = DepoAyarlari {
            saat_dilimi: TimeZone::parse("Z"),
            filtre: Some(
                crate::filtre::Filtre {
                    baslangic: crate::datetime::TimePoint::parse("2026-10-25 00:00"),
                    ..crate::filtre::Filtre::default()
                }
                .coz(None)
                .unwrap(),
            ),
            ..DepoAyarlari::default()
        };
        let depo = Depo::doldur(ayarlar, metin.as_bytes()).unwrap();
        assert!(depo.saat_dilimi_verildi());
        assert_eq!(depo.kayit_sayisi(), 2);
        assert_eq!(
            depo.zaman_araligi(),
            Some((zaman("2026-10-25 00:10:00"), zaman("2026-10-25 00:40:00")))
        );
        assert_eq!(depo.dilim_sayilari().cevrilen, 2);
        assert_eq!(depo.dilim_sayilari().yazilmamis, 0);
    }

    #[test]
    fn seviyeler_ve_satir_sayilari() {
        let icerik = include_str!("../ornekler/laravel.log");
        let depo = metinden(icerik);
        assert_eq!(depo.kayit_sayisi(), 59);
        assert_eq!(
            depo.sayilar(),
            SatirSayilari {
                satir: 205,
                kayit: 59,
                devam: 143,
                yetim: 3,
                taninmayan_seviye: 0,
            }
        );
        let seviyeler = depo.seviye_sayilari();
        assert_eq!(seviyeler[0], (Seviye::Critical, 2));
        assert_eq!(seviyeler[1], (Seviye::Error, 16));
        assert_eq!(seviyeler.last(), Some(&(Seviye::Debug, 3)));
        assert_eq!(seviyeler.iter().map(|&(_, adet)| adet).sum::<usize>(), 59);
        assert_eq!(
            depo.zaman_araligi(),
            Some((zaman("2026-10-24 08:59:58"), zaman("2026-10-24 18:31:07")))
        );
        // Laravel'in biçiminde saat dilimi yazılmaz
        assert_eq!(depo.saat_dilimi(), None);
        assert_eq!(depo.dilim_sayilari().yazilmamis, 59);
        assert_eq!(depo.ham_mesajlar().count(), 45);
        assert_eq!(depo.maskeli_mesajlar().len(), 27);
        assert!(!depo.gecersiz_utf8());
    }

    #[test]
    fn alt_gruplar_mesaj_istisna_ve_seviyeye_gore() {
        let depo = metinden(
            "[2026-10-24 10:00:05] local.WARNING: Kuyruk 1 dolu\n\
             [2026-10-24 10:00:01] local.WARNING: Kuyruk 2 dolu\n\
             [2026-10-24 10:00:09] local.ERROR: Kuyruk 3 dolu\n\
             [2026-10-24 10:00:09] local.WARNING: Kuyruk 4 dolu\n\
             [2026-10-24 10:00:01] local.WARNING: Kuyruk 5 dolu\n\
             [2026-10-24 10:00:09] local.WARNING: Kuyruk 6 dolu",
        );
        // Aynı maskeli mesaj: seviye alt grupları ayırır
        assert_eq!(depo.maskeli_mesajlar(), ["Kuyruk <NUM> dolu"]);
        let alt = depo.alt_gruplar();
        assert_eq!(alt.len(), 2);
        assert_eq!((alt[0].seviye(), alt[0].adet()), (Seviye::Warning, 5));
        assert_eq!((alt[1].seviye(), alt[1].adet()), (Seviye::Error, 1));
        // Örnek ilk kaydın mesajı; ilk ve son, en erken ve en geç zaman
        assert_eq!(alt[0].ornek(), "Kuyruk 1 dolu");
        assert_eq!(alt[0].ilk_kayit(), 0);
        assert_eq!(
            (alt[0].ilk(), alt[0].son()),
            (zaman("2026-10-24 10:00:01"), zaman("2026-10-24 10:00:09"))
        );
        assert_eq!(alt[1].ilk_kayit(), 2);

        // Sütunlar her kaydın zamanını ve alt grubunu tutar
        let kayitlar: Vec<(i64, u32)> = depo.kayitlar().collect();
        let saniye = |metin| DateTime::from_record(metin).epoch_seconds();
        assert_eq!(
            kayitlar,
            [
                (saniye("2026-10-24 10:00:05"), 0),
                (saniye("2026-10-24 10:00:01"), 0),
                (saniye("2026-10-24 10:00:09"), 1),
                (saniye("2026-10-24 10:00:09"), 0),
                (saniye("2026-10-24 10:00:01"), 0),
                (saniye("2026-10-24 10:00:09"), 0),
            ]
        );
        assert_eq!(depo.alt_grup_seviyesi(1), Seviye::Error);
    }

    /// `SepetHesaplayici->toplam()`'da bir hata: verilen anda, verilen satırda
    fn toplamda(zaman: &str, satir: u32) -> String {
        format!(
            r#"[2026-10-24 {zaman}] production.ERROR: Undefined array key "kargo_ucreti" {{"exception":"[object] (ErrorException(code: 0): Undefined array key \"kargo_ucreti\" at /var/www/magaza/app/Services/SepetHesaplayici.php:{satir})
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Foundation/Bootstrap/HandleExceptions.php(256): Illuminate\\Foundation\\Bootstrap\\HandleExceptions->handleError()
#1 /var/www/magaza/app/Services/SepetHesaplayici.php({satir}): Illuminate\\Foundation\\Bootstrap\\HandleExceptions->Illuminate\\Foundation\\Bootstrap\\{{closure}}()
#2 /var/www/magaza/app/Http/Controllers/SepetController.php(41): App\\Services\\SepetHesaplayici->toplam()
#3 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/ControllerDispatcher.php(46): App\\Http\\Controllers\\SepetController->goster()
#4 {{main}}
"}}"#
        )
    }

    /// Aynı hata Monolog'un JSON biçiminde, yığın izi kapalı: yalnızca fırlatıldığı yer
    fn json_toplamda(zaman: &str, satir: u32) -> String {
        format!(
            r#"{{"message":"Undefined array key \"kargo_ucreti\"","context":{{"exception":{{"class":"ErrorException","message":"Undefined array key \"kargo_ucreti\"","code":0,"file":"/var/www/magaza/app/Services/SepetHesaplayici.php:{satir}"}}}},"level":400,"level_name":"ERROR","channel":"production","datetime":"2026-10-24T{zaman}","extra":{{}}}}"#
        )
    }

    #[test]
    fn ayni_hatanin_her_satiri_ayri_alt_gruptur() {
        // Dağıtımdan sonra hata 91. satırda. Log sıralı değil: en yeni kayıt ortada.
        let log = [
            toplamda("10:00:00", 88),
            toplamda("12:00:00", 91),
            toplamda("11:00:00", 88),
        ]
        .join("\n");
        let depo = metinden(&log);
        // Parmak izi aynı; her satır ayrı bir alt grup. Özet en yeni kaydınkini gösterir.
        let izler: Vec<Option<String>> = depo
            .parmak_izleri()
            .iter()
            .map(|iz| iz.as_ref().map(ToString::to_string))
            .collect();
        assert_eq!(
            izler,
            [
                Some("app/Services/SepetHesaplayici.php:88, toplam() (ErrorException)".to_string()),
                Some("app/Services/SepetHesaplayici.php:91, toplam() (ErrorException)".to_string()),
            ]
        );
        assert_eq!(depo.parmak_izleri()[0], depo.parmak_izleri()[1]);
        let alt = depo.alt_gruplar();
        assert_eq!((alt[0].adet(), alt[0].son_kayit()), (2, 2));
        assert_eq!((alt[1].adet(), alt[1].son_kayit()), (1, 1));
        // Aynı andaki kayıtlardan en geç olanı logda sonra geleni
        let depo = metinden(&[toplamda("10:00:00", 88), toplamda("10:00:00", 88)].join("\n"));
        assert_eq!(depo.alt_gruplar()[0].son_kayit(), 1);

        // `--konum satir`: iki ayrı hata
        let satirla = DepoAyarlari {
            konum_turu: KonumTuru::Satir,
            ..DepoAyarlari::default()
        };
        let depo = Depo::doldur(satirla, log.as_bytes()).unwrap();
        let izler = depo.parmak_izleri();
        assert_eq!(izler.len(), 2);
        assert_ne!(izler[0], izler[1]);
        assert_eq!(
            izler[1].as_ref().map(ToString::to_string),
            Some("app/Services/SepetHesaplayici.php:91 (ErrorException)".to_string())
        );
    }

    #[test]
    fn cagrisiz_kaydin_fonksiyonu_okuma_bitince_bulunur() {
        // JSON kaydı önce geliyor; fonksiyonu sonraki metin kaydının yığın izinden öğrenilir.
        // Kökü de ondan bulunur: JSON kaydı bir taslaktır.
        let log = [
            json_toplamda("09:00:00+00:00", 88),
            toplamda("10:00:00", 88),
        ]
        .join("\n");
        let depo = metinden(&log);
        let izler = depo.parmak_izleri();
        assert_eq!(izler.len(), 2);
        assert_eq!(izler[0], izler[1]);
        assert_eq!(
            izler[0].as_ref().and_then(ParmakIzi::metot),
            Some(r"App\Services\SepetHesaplayici->toplam")
        );
        // Kökü kendi yollarında olan bir JSON kaydı (vendor'da fırlatılmış, uygulama satırı
        // yığın izinde) da tamamlanır
        let vendorlu = json_toplamda("09:00:00+00:00", 88).replace(
            r#""file":"/var/www/magaza/app/Services/SepetHesaplayici.php:88""#,
            r#""file":"/var/www/magaza/vendor/laravel/framework/src/Illuminate/Support/Arr.php:5","trace":["/var/www/magaza/app/Services/SepetHesaplayici.php:88"]"#,
        );
        let depo = metinden(&[vendorlu, toplamda("10:00:00", 88)].join("\n"));
        let izler = depo.parmak_izleri();
        assert_eq!(izler[0], izler[1]);

        // `--konum satir`: hiçbir konum fonksiyonla belirlenmez
        let satirla = DepoAyarlari {
            konum_turu: KonumTuru::Satir,
            ..DepoAyarlari::default()
        };
        let depo = Depo::doldur(satirla, log.as_bytes()).unwrap();
        assert!(
            depo.parmak_izleri()
                .iter()
                .all(|iz| iz.as_ref().unwrap().metot().is_none())
        );
    }

    #[test]
    fn koku_sonradan_bulunan_kaydin_yigin_izi_de_ogrenilir() {
        // Metin kaydının yollarında vendor yok: kökü, dolayısıyla yığın izindeki satırların
        // köke göre yolları okuma bitince bilinir. JSON kaydının fonksiyonu ondan öğrenilir.
        let vendorsuz = r#"[2026-10-24 10:00:00] production.ERROR: Ödeme alınamadı {"exception":"[object] (RuntimeException(code: 0): Ödeme alınamadı at /var/www/magaza/app/Services/OdemeServisi.php:64)
[stacktrace]
#0 /var/www/magaza/app/Http/Controllers/OdemeController.php(31): App\\Services\\OdemeServisi->tahsilEt()
#1 {main}
"} "#;
        let json = r#"{"message":"Ödeme alınamadı","context":{"exception":{"class":"RuntimeException","message":"Ödeme alınamadı","code":0,"file":"/var/www/magaza/app/Services/OdemeServisi.php:64"}},"level":400,"level_name":"ERROR","channel":"production","datetime":"2026-10-24T11:00:00+00:00","extra":{}}"#;
        // İlk kayıt kökü verir
        let log = [
            toplamda("09:00:00", 88),
            vendorsuz.to_string(),
            json.to_string(),
        ]
        .join("\n");
        let depo = metinden(&log);
        let izler = depo.parmak_izleri();
        assert_eq!(izler.len(), 3);
        assert_eq!(izler[1], izler[2]);
        assert_eq!(
            izler[2].as_ref().map(ToString::to_string),
            Some("app/Services/OdemeServisi.php:64, tahsilEt() (RuntimeException)".to_string())
        );

        // `--konum satir`: taslakların yığın izleri de konumu fonksiyonla belirlemez
        let satirla = DepoAyarlari {
            konum_turu: KonumTuru::Satir,
            ..DepoAyarlari::default()
        };
        let depo = Depo::doldur(satirla, log.as_bytes()).unwrap();
        assert!(
            depo.parmak_izleri()
                .iter()
                .all(|iz| iz.as_ref().unwrap().metot().is_none())
        );
    }

    #[test]
    fn taslaklar_sonradan_bulunan_kokle_cozulur() {
        // Yığın izi olmayan ölümcül hata önce geliyor; kökü veren vendor'lu kayıt sonra
        let olumcul = r#"[2026-10-26 13:44:17] local.ERROR: Trait "App\Traits\X" not found {"exception":"[object] (Symfony\\Component\\ErrorHandler\\Error\\FatalError(code: 0): Trait \"App\\Traits\\X\" not found at /var/www/magaza/app/Http/Controllers/RaporController.php:5)
[stacktrace]
#0 {main}
"} "#;
        let vendorlu = r#"[2026-10-26 13:45:00] local.ERROR: Hata {"exception":"[object] (RuntimeException(code: 0): Hata at /var/www/magaza/vendor/firma/paket/A.php:3)
[stacktrace]
#0 /var/www/magaza/app/Services/B.php(7): A->b()
#1 {main}
"} "#;
        let depo = metinden(&format!("{olumcul}\n{vendorlu}\n{olumcul}"));
        let izler: Vec<Option<String>> = depo
            .parmak_izleri()
            .iter()
            .map(|iz| iz.as_ref().map(|iz| iz.to_string()))
            .collect();
        // Aynı taslak iki kayıtta: tek alt grup
        assert_eq!(
            izler,
            [
                Some("app/Http/Controllers/RaporController.php:5 (FatalError)".to_string()),
                Some("app/Services/B.php:7 (RuntimeException)".to_string()),
            ]
        );
        assert_eq!(depo.alt_gruplar()[0].adet(), 2);
        assert_eq!(depo.taslaklar.len(), 1);
        assert_eq!(depo.izler.len(), 1);
    }

    #[test]
    fn filtreye_uymayan_kayit_depoya_girmez() {
        use crate::datetime::TimePoint;
        use crate::filtre::Filtre;
        let metin = "[2026-10-24 10:00:00] local.INFO: Başladı\n\
                     [2026-10-24 10:00:01] local.ERROR: Kuyruk 1 dolu\n\
                     [2026-10-24 11:00:00] local.ERROR: Kuyruk 2 dolu\n\
                     [2026-10-24 10:30:00] local.WARNING: Disk doluyor";
        let filtre = Filtre {
            seviye: Some(Seviye::Warning),
            bitis: TimePoint::parse("2026-10-24 11:00"),
            ..Filtre::default()
        }
        .coz(None)
        .unwrap();
        let ayarlar = DepoAyarlari {
            filtre: Some(filtre),
            ..DepoAyarlari::default()
        };
        let depo = Depo::doldur(ayarlar, metin.as_bytes()).unwrap();
        // Dosyada 4 kayıt; filtreye uyan 2
        assert_eq!(depo.sayilar().kayit, 4);
        assert_eq!(depo.kayit_sayisi(), 2);
        assert_eq!(
            depo.seviye_sayilari(),
            [(Seviye::Error, 1), (Seviye::Warning, 1)]
        );
        assert_eq!(
            depo.zaman_araligi(),
            Some((zaman("2026-10-24 10:00:01"), zaman("2026-10-24 10:30:00")))
        );
        assert_eq!(
            depo.maskeli_mesajlar(),
            ["Kuyruk <NUM> dolu", "Disk doluyor"]
        );
        // Sıra numaraları ve dizin yalnızca depodaki kayıtlar arasında
        let ilkler: Vec<usize> = depo.alt_gruplar().iter().map(AltGrup::ilk_kayit).collect();
        assert_eq!(ilkler, [0, 1]);
        assert_eq!(
            depo.dizin().kayitlar(),
            [(0, metin.find("[2026-10-24 10:00:01]").unwrap() as u64)]
        );
        assert!(depo.filtre().is_some());
    }

    #[test]
    fn seyrek_dizin_her_araliktaki_kaydin_konumu() {
        let satir = "[2026-10-24 10:00:00] local.INFO: x\n";
        let metin = format!("yetim\n{}", satir.repeat(2 * DIZIN_ARALIGI + 1));
        let depo = metinden(&metin);
        let konum = |sira: usize| (sira, (6 + sira * satir.len()) as u64);
        assert_eq!(
            depo.dizin().kayitlar(),
            [konum(0), konum(DIZIN_ARALIGI), konum(2 * DIZIN_ARALIGI)]
        );
    }

    #[test]
    fn yeni_girdinin_ilk_kaydi_dizine_yazilir() {
        // Canlı izlemede döndürülen dosya: konumlar yeni dosyada sıfırdan sayılır. Yeni dosyanın
        // ilk kaydı dizinde olmasa onun bloğundaki kayıtlar eski dosyanın konumlarıyla aranırdı.
        let eski = "[2026-10-24 10:00:00] local.ERROR: Eski 1\n\
                    [2026-10-24 10:00:01] local.ERROR: Eski 2\n\
                    [2026-10-24 10:00:02] local.ERROR: Eski 3\n";
        let yeni = "[2026-10-24 10:00:03] local.INFO: Yeni 1\n\
                    [2026-10-24 10:00:04] local.ERROR: Yeni 2\n\
                    [2026-10-24 10:00:05] local.ERROR: Yeni 3\n";
        let ayarlar = DepoAyarlari {
            filtre: Some(
                Filtre {
                    seviye: Some(Seviye::Error),
                    ..Filtre::default()
                }
                .coz(None)
                .unwrap(),
            ),
            ..DepoAyarlari::default()
        };
        let mut depo = Depo::new(ayarlar);
        let bicim = SatirBicimi::default();
        okuyucu::oku(eski.as_bytes(), &bicim, &mut depo).unwrap();
        depo.yeni_girdi();
        let sonuc = okuyucu::oku(yeni.as_bytes(), &bicim, &mut depo).unwrap();
        depo.okuma_bitti(sonuc);
        // Yeni girdinin filtreye uyan ilk kaydı: 3. sırada, yeni dosyanın ikinci satırı
        let ikinci_satir = yeni.find("[2026-10-24 10:00:04]").unwrap() as u64;
        assert_eq!(depo.dizin().kayitlar(), [(0, 0), (3, ikinci_satir)]);
        let mut kaynak = okuyucu::YenidenOkuyucu::new(
            std::io::Cursor::new(yeni.as_bytes()),
            depo.dizin().clone(),
            depo.filtre().copied(),
            None,
        );
        let istek = |sira: usize| OrnekIstegi {
            sira,
            maskeli: &depo.maskeliler
                [depo.alt_gruplar[depo.columns.iter().nth(sira).unwrap().1 as usize].maskeli()],
            seviye: Seviye::Error,
        };
        let bulunan = kaynak.mesajlar(&[istek(3), istek(4)]).unwrap();
        assert_eq!(bulunan[&3], "Yeni 2");
        assert_eq!(bulunan[&4], "Yeni 3");
        assert!(!kaynak.degisti());
        // Eski girdinin kaydı yeni girdide aranırsa bulunamaz; aynı istekteki yeni kayıtlar
        // yine kendi bloklarından bulunur
        let bulunan = kaynak.mesajlar(&[istek(1), istek(3), istek(4)]).unwrap();
        assert_eq!(bulunan.len(), 2);
        assert_eq!(bulunan[&3], "Yeni 2");
        assert!(kaynak.degisti());
    }

    #[test]
    fn pencere_yalnizca_filtreye_uyan_kayitlari_toplar() {
        let depo = metinden(
            "[2026-10-24 10:00:00] production.ERROR: Ödeme alınamadı: 1\n\
             [2026-10-24 10:20:00] production.WARNING: Yavaş sorgu (120 ms)\n\
             [2026-10-24 10:40:00] production.ERROR: Ödeme alınamadı: 2\n\
             [2026-10-24 11:00:00] production.INFO: Kullanıcı 7 giriş yaptı\n\
             [2026-10-24 11:30:00] production.ERROR: Ödeme alınamadı: 3\n",
        );
        let filtre = Filtre {
            seviye: Some(Seviye::Warning),
            son: Some(3_600),
            ..Filtre::default()
        }
        .coz(depo.en_yeni())
        .unwrap();
        let (pencere, tam_sira) = depo.pencere(filtre);
        // 10:30'dan sonraki WARNING ve üstü: 10:40 ve 11:30'daki hatalar
        assert_eq!(tam_sira, vec![2, 4]);
        assert_eq!(pencere.kayit_sayisi(), 2);
        assert_eq!(pencere.filtre(), Some(&filtre));
        assert_eq!(
            pencere.zaman_araligi(),
            Some((zaman("2026-10-24 10:40:00"), zaman("2026-10-24 11:30:00")))
        );
        assert_eq!(pencere.seviye_sayilari(), vec![(Seviye::Error, 2)]);
        let kayitlar: Vec<(i64, u32)> = pencere.kayitlar().collect();
        assert_eq!(
            kayitlar,
            vec![
                (zaman("2026-10-24 10:40:00").epoch_seconds(), 0),
                (zaman("2026-10-24 11:30:00").epoch_seconds(), 0)
            ]
        );
        // Alt grup pencerenin kayıtlarıyla: 2 kayıt, ilk kaydı pencerenin 0. kaydı
        let alt = &pencere.alt_gruplar()[0];
        assert_eq!(alt.adet(), 2);
        assert_eq!(alt.ilk_kayit(), 0);
        assert_eq!(alt.son_kayit(), 1);
        assert_eq!(alt.ilk(), zaman("2026-10-24 10:40:00"));
        assert_eq!(alt.son(), zaman("2026-10-24 11:30:00"));
        // Örnek depodaki alt grubun ilk kaydınındır (pencere dışında): kesin değil, yeniden
        // okunacak
        assert_eq!(alt.ornek(), "Ödeme alınamadı: 1");
        assert!(!alt.ornek_kesin());
        // Kalıplar, maskeli mesajlar ve kökler paylaşılır; dizin yok
        assert_eq!(pencere.kumeler().len(), depo.kumeler().len());
        assert_eq!(pencere.maskeli_mesaj_sayisi(), depo.maskeli_mesaj_sayisi());
        assert!(pencere.dizin().kayitlar().is_empty());
        assert_eq!(pencere.sayilar(), depo.sayilar());
        // Deponun kendisi değişmedi
        assert_eq!(depo.kayit_sayisi(), 5);
        assert_eq!(depo.alt_gruplar()[0].adet(), 3);
        // İlk kaydıyla pencereye giren alt grubun örneği kesindir
        let hepsi = Filtre::default().coz(None).unwrap();
        let (pencere, tam_sira) = depo.pencere(hepsi);
        assert_eq!(tam_sira, vec![0, 1, 2, 3, 4]);
        assert!(pencere.alt_gruplar().iter().all(AltGrup::ornek_kesin));
    }

    #[test]
    fn ikinci_turdan_sonra_alt_gruplarin_anahtarlari_tekildir() {
        // Drain sıraya bağlıdır: aynı maskeli mesaj ilk turda iki kümeye düşebilir (arada
        // kalıplar değişti). İkinci turda ikisi aynı kümeye düşerse iki alt grubun anahtarı
        // aynı olur; alt gruplar o zaman birleştirilmelidir.
        let mut tohum: u64 = 0x1234_5678_9ABC_DEF1;
        let mut sayi = |ust: usize| {
            tohum ^= tohum >> 12;
            tohum ^= tohum << 25;
            tohum ^= tohum >> 27;
            (tohum.wrapping_mul(0x2545_F491_4F6C_DD1D) % ust as u64) as usize
        };
        let ad = |sira: usize| -> String {
            [sira / 676, sira / 26, sira]
                .iter()
                .map(|basamak| char::from(b'a' + (basamak % 26) as u8))
                .collect()
        };
        let kucuk = ["sepet", "ödeme", "kargo", "web"];
        let mut birlesen = 0;
        for deneme in 0..10 {
            let uzunluk = 4 + sayi(3);
            let buyuk_konum: Vec<usize> = (1..uzunluk).filter(|_| sayi(2) == 0).collect();
            let ad_sayisi = 100 + sayi(300);
            let mut metin = String::new();
            for _ in 0..300 + sayi(1500) {
                let mut parcalar = vec!["X".to_string()];
                for konum in 1..uzunluk {
                    if buyuk_konum.contains(&konum) && sayi(3) != 0 {
                        parcalar.push(ad(sayi(ad_sayisi)));
                    } else {
                        parcalar.push(kucuk[sayi(kucuk.len())].to_string());
                    }
                }
                let saniye = sayi(86_400);
                metin.push_str(&format!(
                    "[2026-10-24 {:02}:{:02}:{:02}] production.{}: {}\n",
                    saniye / 3600,
                    saniye / 60 % 60,
                    saniye % 60,
                    ["INFO", "WARNING"][sayi(2)],
                    parcalar.join(" ")
                ));
            }
            let mut depo = Depo::new(DepoAyarlari::default());
            let sonuc = okuyucu::oku(metin.as_bytes(), &SatirBicimi::default(), &mut depo).unwrap();
            let ilk_tur = depo.alt_gruplar.len();
            depo.ara_durum(sonuc);
            let anahtarlar: HashSet<AltGrupAnahtari> =
                depo.alt_gruplar.iter().map(|alt| alt.anahtar).collect();
            assert_eq!(anahtarlar.len(), depo.alt_gruplar.len(), "deneme {deneme}");
            // Okuma durumu da tutarlı: her anahtar kendi alt grubunu gösterir
            for (no, alt) in depo.alt_gruplar.iter().enumerate() {
                assert_eq!(depo.alt_grup_sirasi.get(&alt.anahtar), Some(&numara(no)));
            }
            let toplam: usize = depo.alt_gruplar.iter().map(AltGrup::adet).sum();
            assert_eq!(toplam, depo.kayit_sayisi(), "deneme {deneme}");
            birlesen += usize::from(depo.alt_gruplar.len() < ilk_tur);
        }
        // Senaryo gerçekten sınandı
        assert!(birlesen > 0);
    }

    #[test]
    fn ara_durumdan_sonra_kayit_alinmaya_devam_edilir() {
        // Değişken konum bulunan bir log: ara durumda ikinci tur yapılır, sonra gelen kayıtlar
        // öğrenilen konumla kümelenir ve alt gruplar yeni küme numaralarıyla bulunur
        let mut depo = Depo::new(DepoAyarlari::default());
        let bicim = SatirBicimi::default();
        // 101 farklı, rakamsız ad: "aaa", "aab", ...
        let ad = |sira: usize| -> String {
            [sira / 676, sira / 26, sira]
                .iter()
                .map(|basamak| char::from(b'a' + (basamak % 26) as u8))
                .collect()
        };
        let mut metin = String::new();
        for sira in 0..101 {
            metin.push_str(&format!(
                "[2026-10-24 10:00:00] production.WARNING: Bilinmeyen kullanıcı: {}\n",
                ad(sira)
            ));
        }
        let sonuc = okuyucu::oku(metin.as_bytes(), &bicim, &mut depo).unwrap();
        assert!(depo.kumeleyici.needs_second_pass());
        depo.ara_durum(sonuc);
        assert!(!depo.alt_grup_sirasi.is_empty(), "okuma durumu korunur");
        let kume_sayisi = depo.kumeler().len();
        assert_eq!(kume_sayisi, 1, "ikinci turda adlar tek kalıpta birleşir");
        // Anahtar sözlüğü yeni küme numaralarıyla
        for (no, alt) in depo.alt_gruplar().iter().enumerate() {
            assert_eq!(depo.alt_grup_sirasi.get(&alt.anahtar), Some(&numara(no)));
        }
        let devam = "[2026-10-24 10:00:01] production.WARNING: Bilinmeyen kullanıcı: aac\n\
                     [2026-10-24 10:00:02] production.WARNING: Bilinmeyen kullanıcı: yeni\n";
        let sonuc = okuyucu::oku(devam.as_bytes(), &bicim, &mut depo).unwrap();
        depo.okuma_bitti(sonuc);
        assert_eq!(depo.kayit_sayisi(), 103);
        assert_eq!(depo.kumeler().len(), 1);
        // Tekrar eden mesaj eski alt grubuna katıldı, yenisi yeni bir alt grup açtı
        assert_eq!(depo.alt_gruplar().len(), 102);
        assert_eq!(depo.alt_gruplar()[2].adet(), 2);
    }
}
