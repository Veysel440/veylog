//! Özetin deposu: kayıtlar okunurken her birinden özet için gerekenler alınır.
//!
//! Kayıtlar akış halinde, teker teker gelir; kendileri saklanmaz. Depoda kalanlar:
//!
//! - **Alt gruplar.** Maskeli mesajı, Drain kümesi, istisnası ve seviyesi aynı olan kayıtlar.
//!   Her alt grubun kayıt sayısı, ilk kaydının mesajı (örnek), ilk ve son görüldüğü zaman.
//!   Kalıp tablosunun satırları alt gruplardan kurulur.
//! - **Maskeli mesajlar.** Her farklı maskeli mesaj bir kez, ilk görüldüğü sırayla.
//! - **Drain.** Her kaydın maskeli mesajı, logdaki sırasıyla kümelenir (`drain::Kumeleyici`):
//!   Drain sıraya bağlıdır, aynı mesaj logun farklı yerlerinde farklı kümelere katılabilir. Bir
//!   mesajın tekrarlarında ağaç değişmediyse önceki sonuç kullanılır; arama yapılmaz.
//! - **Kayıt sütunları.** Her kaydın zamanı ve alt grubu, sıkıştırılmış olarak (`sutunlar`
//!   modülü): kayıt başına çoğunlukla 2–3 bayt. Karşılaştırma ve ani artış bunlarla hesaplanır.
//! - **Seyrek dizin.** Her `DIZIN_ARALIGI` kayıtta bir kaydın girdideki konumu: bir kaydın
//!   mesajı sonradan gerekirse girdinin yalnızca o kısmı yeniden okunur.
//! - **İstisnalar.** Parmak izleri ve projelerin kökleri. Kökü kendi yollarında olmayan bir
//!   kaydın istisnası taslak olarak saklanır; parmak izi okuma bitince çıkarılır.
//! - **Sayılar.** Seviyeler, farklı ham mesajlar (`farkli_sayac` modülü), en eski ve en yeni
//!   kayıt, kaç kaydın başka bir saat diliminden çevrildiği.
//!
//! Kayıt zamanları raporun saat dilimine çevrilerek alınır (`zaman::DilimCevirici`): sütunlardaki,
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
use std::collections::{BTreeSet, HashMap};
use std::hash::Hash;
use std::io::{self, BufRead};
use std::rc::Rc;

use crate::drain::{self, Kume, Kumeleyici};
use crate::farkli_sayac::FarkliSayaci;
use crate::filtre::CozulmusFiltre;
use crate::kayit::{Kayit, KayitAlici, KayitBilgisi, SatirSayilari, Seviye};
use crate::maske::maskele;
use crate::okuyucu::{self, DIZIN_ARALIGI, OkumaSonucu, OrnekIstegi, OrnekKaynagi};
use crate::parmak_izi::{Istisna, IstisnaTaslagi, ParmakIzi};
use crate::satir_bicimi::SatirBicimi;
use crate::sutunlar::{KayitSutunlari, SutunOkuyucu};
use crate::zaman::{DilimCevirici, KayitDilimi, SaatDilimi, Zaman};

/// Bir kaydın istisnası: yok, parmak izi belli ya da bütün kökler bilinince çıkarılacak.
/// Numaralar depodaki listelerin sırasıdır.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum IstisnaNo {
    Yok,
    Iz(u32),
    Taslak(u32),
}

/// Alt grubu belirleyen dörtlü.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct AltGrupAnahtari {
    maskeli: u32,
    /// Drain kümesinin numarası.
    kume: u32,
    istisna: IstisnaNo,
    seviye: Seviye,
}

/// Maskeli mesajı, Drain kümesi, istisnası ve seviyesi aynı olan kayıtlar.
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
    ilk: Zaman,
    son: Zaman,
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

    pub fn ilk(&self) -> Zaman {
        self.ilk
    }

    pub fn son(&self) -> Zaman {
        self.son
    }
}

/// Kayıtların saat dilimleri: raporun dilimine nasıl alındıkları.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DilimSayilari {
    /// Başka bir dilimde yazılmış, raporun dilimine çevrilen kayıtlar.
    pub cevrilen: usize,
    /// Çevrilen kayıtların dilimleri, küçükten büyüğe.
    pub cevrilen_dilimler: BTreeSet<SaatDilimi>,
    /// Saat dilimi yazılmamış, raporun diliminde yazılmış sayılan kayıtlar.
    pub yazilmamis: usize,
}

/// Özetin deposu. Kayıtları `KayitAlici` olarak alır.
#[derive(Debug)]
pub struct Depo {
    kumeleyici: Kumeleyici,
    /// Filtre; None ise bütün kayıtlar alınır.
    filtre: Option<CozulmusFiltre>,
    /// Kayıt zamanlarını raporun saat dilimine çevirir.
    cevirici: DilimCevirici,
    /// Satırların biçimi; kayıtlar yeniden okunurken de gerekir.
    satir_bicimi: SatirBicimi,
    dilimler: DilimSayilari,
    sayilar: SatirSayilari,
    gecersiz_utf8: bool,
    sutunlar: KayitSutunlari,
    /// `dizin[i]`: `i × DIZIN_ARALIGI`. kaydın başlık satırının girdideki konumu.
    dizin: Vec<u64>,
    /// `Seviye`nin sırasıyla her seviyenin kayıt sayısı.
    seviye_sayilari: [usize; 8],
    /// Logun en eski ve en yeni kaydının zamanı.
    en_eski: Option<Zaman>,
    en_yeni: Option<Zaman>,
    ham_mesajlar: FarkliSayaci,
    /// Maskeli mesajlar, ilk görülme sırasıyla; numaraları bu listedeki sıralarıdır. Metinler
    /// `Rc` ile paylaşılır: liste ile sözlük aynı metni gösterir.
    maskeliler: Vec<Rc<str>>,
    maskeli_sirasi: HashMap<Rc<str>, u32>,
    /// Kayıtların kendi yollarında bulunan proje kökleri.
    kokler: BTreeSet<String>,
    izler: Vec<ParmakIzi>,
    iz_sirasi: HashMap<ParmakIzi, u32>,
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
        let (zaman, dilim) = self.cevirici.cevir(kayit.zaman());
        let seviye = kayit.seviye();
        if let Some(filtre) = &self.filtre
            && !filtre.gecer(zaman.saniye(), seviye)
        {
            return;
        }
        // Kaydın depodaki sıra numarası: filtreye uyan kayıtlar arasında kaçıncı olduğu
        let sira = self.sutunlar.len();
        self.seviye_sayilari[seviye as usize] += 1;
        match dilim {
            KayitDilimi::Yazilmamis => self.dilimler.yazilmamis += 1,
            KayitDilimi::Ayni => {}
            KayitDilimi::Baska(kendi) => {
                self.dilimler.cevrilen += 1;
                self.dilimler.cevrilen_dilimler.insert(kendi);
            }
        }
        self.en_eski = Some(self.en_eski.map_or(zaman, |en_eski| en_eski.min(zaman)));
        self.en_yeni = Some(self.en_yeni.map_or(zaman, |en_yeni| en_yeni.max(zaman)));
        self.ham_mesajlar.ekle(kayit.mesaj());

        let (maskeli, maskelendi) = self.maskeli_no(kayit.mesaj());
        let kume = self
            .kumeleyici
            .ekle(maskeli as usize, &self.maskeliler[maskeli as usize]);
        let anahtar = AltGrupAnahtari {
            maskeli,
            kume: numara(kume),
            istisna: self.istisna_no(kayit),
            seviye,
        };
        let no = match self.alt_grup_sirasi.get(&anahtar) {
            Some(&no) => {
                let alt = &mut self.alt_gruplar[no as usize];
                alt.adet += 1;
                alt.ilk = alt.ilk.min(zaman);
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
                });
                self.alt_grup_sirasi.insert(anahtar, no);
                no
            }
        };
        self.sutunlar.ekle(zaman.saniye(), no);
        if sira.is_multiple_of(DIZIN_ARALIGI) {
            self.dizin.push(bilgi.konum);
        }
    }
}

/// Deponun kayıtları nasıl alacağı.
#[derive(Debug, Clone, Default)]
pub struct DepoAyarlari {
    /// Kayıtların mesajlarının kümeleneceği Drain ayarları.
    pub drain: drain::Ayarlar,
    /// Filtre, logun sonuna göre çözülmüş olarak; None ise bütün kayıtlar alınır.
    pub filtre: Option<CozulmusFiltre>,
    /// Raporun saat dilimi (`--saat-dilimi`); None ise logda saat dilimi yazılı ilk kaydınki.
    pub saat_dilimi: Option<SaatDilimi>,
    /// Satırların biçimi (`--desen`, `--zaman-bicimi`); varsayılanı Laravel'inki.
    pub satir_bicimi: SatirBicimi,
}

impl Depo {
    /// Boş bir depo.
    pub fn new(ayarlar: DepoAyarlari) -> Depo {
        Depo {
            kumeleyici: Kumeleyici::new(ayarlar.drain),
            filtre: ayarlar.filtre,
            cevirici: DilimCevirici::new(ayarlar.saat_dilimi),
            satir_bicimi: ayarlar.satir_bicimi,
            dilimler: DilimSayilari::default(),
            sayilar: SatirSayilari::default(),
            gecersiz_utf8: false,
            sutunlar: KayitSutunlari::default(),
            dizin: Vec::new(),
            seviye_sayilari: [0; 8],
            en_eski: None,
            en_yeni: None,
            ham_mesajlar: FarkliSayaci::default(),
            maskeliler: Vec::new(),
            maskeli_sirasi: HashMap::new(),
            kokler: BTreeSet::new(),
            izler: Vec::new(),
            iz_sirasi: HashMap::new(),
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
        self.sayilar = sonuc.sayilar;
        self.gecersiz_utf8 = sonuc.gecersiz_utf8;
        // Yalnızca okunurken gerekir
        self.alt_grup_sirasi = HashMap::new();
        if self.kumeleyici.ikinci_tur_gerekir() {
            self.ikinci_tur();
        }
    }

    /// İkinci kümeleme turu: kayıtlar, logdaki sırasıyla, ilk turda öğrenilen değişken
    /// konumlarla baştan kümelenir. Her kaydın maskeli mesajı sütunlardaki alt grubundan
    /// bilinir.
    ///
    /// Her alt grubun bütün kayıtları ikinci turda da aynı kümeye düşüyorsa (neredeyse her
    /// zaman) yalnızca alt grupların küme numaraları değişir. Değilse alt gruplar bölünür
    /// (`bolerek_kur`).
    fn ikinci_tur(&mut self) {
        let mut kumeleyici = self.kumeleyici.ikinci_tur();
        let mut kumeler: Vec<Option<u32>> = vec![None; self.alt_gruplar.len()];
        for (_, no) in self.sutunlar.iter() {
            let maskeli = self.alt_gruplar[no as usize].anahtar.maskeli;
            let kume =
                numara(kumeleyici.ekle(maskeli as usize, &self.maskeliler[maskeli as usize]));
            match kumeler[no as usize] {
                None => kumeler[no as usize] = Some(kume),
                Some(onceki) if onceki == kume => {}
                // Bölünen bir alt grup: tur baştan, alt grupları bölerek yapılır
                Some(_) => return self.bolerek_kur(),
            }
        }
        for (alt, kume) in self.alt_gruplar.iter_mut().zip(kumeler) {
            alt.anahtar.kume = kume.expect("her alt grubun en az bir kaydı var");
        }
        self.kumeleyici = kumeleyici;
    }

    /// Bölünen alt gruplarla ikinci tur: alt gruplar yeni kümeleriyle, ilk kayıtlarının
    /// sırasıyla yeniden kurulur; sütunlar yeni numaralarla yeniden yazılır.
    ///
    /// Bölünen bir alt grubun parçası: ilk kaydı alt grubun ilk kaydı değilse mesajı depoda
    /// yoktur; örneği şimdilik alt grubunkidir (`ornek_kesin` false).
    fn bolerek_kur(&mut self) {
        let mut kumeleyici = self.kumeleyici.ikinci_tur();
        let mut alt_gruplar: Vec<AltGrup> = Vec::new();
        let mut sirasi: HashMap<AltGrupAnahtari, u32> = HashMap::new();
        let mut sutunlar = KayitSutunlari::default();
        for (sira, (saniye, eski_no)) in self.sutunlar.iter().enumerate() {
            let eski = &self.alt_gruplar[eski_no as usize];
            let maskeli = eski.anahtar.maskeli;
            let kume = kumeleyici.ekle(maskeli as usize, &self.maskeliler[maskeli as usize]);
            let anahtar = AltGrupAnahtari {
                kume: numara(kume),
                ..eski.anahtar
            };
            let zaman = Zaman::saniyeden(saniye);
            let no = match sirasi.get(&anahtar) {
                Some(&no) => {
                    let alt = &mut alt_gruplar[no as usize];
                    alt.adet += 1;
                    alt.ilk = alt.ilk.min(zaman);
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
                    });
                    sirasi.insert(anahtar, no);
                    no
                }
            };
            sutunlar.ekle(saniye, no);
        }
        self.kumeleyici = kumeleyici;
        self.alt_gruplar = alt_gruplar;
        self.sutunlar = sutunlar;
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
    pub fn kumeler(&self) -> &[Kume] {
        self.kumeleyici.kumeler()
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
        self.sutunlar.len()
    }

    /// Kayıtlar logdaki sırayla: (zaman, alt grup numarası). Zaman 1970'ten bu yana saniyedir.
    pub fn kayitlar(&self) -> SutunOkuyucu<'_> {
        self.sutunlar.iter()
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
    /// bütün kökleriyle çıkarılır.
    pub fn parmak_izleri(&self) -> Vec<Option<ParmakIzi>> {
        let taslak_izleri: Vec<ParmakIzi> = self
            .taslaklar
            .iter()
            .map(|taslak| taslak.istisna().parmak_izi(&self.kokler))
            .collect();
        self.alt_gruplar
            .iter()
            .map(|alt| match alt.anahtar.istisna {
                IstisnaNo::Yok => None,
                IstisnaNo::Iz(no) => Some(self.izler[no as usize].clone()),
                IstisnaNo::Taslak(no) => Some(taslak_izleri[no as usize].clone()),
            })
            .collect()
    }

    /// Farklı ham (maskelenmemiş) mesajların sayısı.
    pub fn ham_mesajlar(&self) -> &FarkliSayaci {
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
    pub fn zaman_araligi(&self) -> Option<(Zaman, Zaman)> {
        Some((self.en_eski?, self.en_yeni?))
    }

    /// En eski kaydın zamanı.
    pub fn en_eski(&self) -> Option<Zaman> {
        self.en_eski
    }

    /// En yeni kaydın zamanı.
    pub fn en_yeni(&self) -> Option<Zaman> {
        self.en_yeni
    }

    /// Raporun saat dilimi: verildiyse o, verilmediyse logda saat dilimi yazılı ilk kaydınki.
    /// İkisi de yoksa None: zamanlar logda yazıldığı gibidir.
    pub fn saat_dilimi(&self) -> Option<SaatDilimi> {
        self.cevirici.dilim()
    }

    /// Saat dilimi komut satırında mı verildi?
    pub fn saat_dilimi_verildi(&self) -> bool {
        self.cevirici.verildi()
    }

    /// Depodaki kayıtların saat dilimleri: kaçının çevrildiği, kaçında dilim yazılmadığı.
    pub fn dilim_sayilari(&self) -> &DilimSayilari {
        &self.dilimler
    }

    /// Seyrek dizin: her `DIZIN_ARALIGI` kayıtta bir kaydın girdideki konumu.
    pub fn dizin(&self) -> &[u64] {
        &self.dizin
    }

    /// Mesajı maskeler: maskeli mesajın numarası ve maskelemenin mesajı değiştirip
    /// değiştirmediği. Maskeli mesaj ilk kez görülüyorsa yeni bir numara verilir.
    fn maskeli_no(&mut self, mesaj: &str) -> (u32, bool) {
        let maskeli = maskele(mesaj);
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

    /// Kaydın istisnası. Kökü kendi yollarındaysa parmak izi hemen çıkarılır; değilse istisna
    /// taslak olarak saklanır. Kaydın kökü, başka kayıtlar için bilinen köklere eklenir.
    fn istisna_no(&mut self, kayit: &Kayit<'_>) -> IstisnaNo {
        let Some(istisna) = Istisna::kayittan(kayit) else {
            return IstisnaNo::Yok;
        };
        match istisna.kok() {
            Some(kok) => {
                if !self.kokler.contains(kok) {
                    self.kokler.insert(kok.to_owned());
                }
                let iz = istisna.koke_gore(Some(kok));
                IstisnaNo::Iz(numaralandir(&mut self.iz_sirasi, &mut self.izler, iz))
            }
            None => IstisnaNo::Taslak(numaralandir(
                &mut self.taslak_sirasi,
                &mut self.taslaklar,
                istisna.taslak(),
            )),
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
    metinden_ayarlarla(metin, &drain::Ayarlar::default())
}

/// Testler için: metni okuyup depoya doldurur.
#[cfg(test)]
pub fn metinden_ayarlarla(metin: &str, ayarlar: &drain::Ayarlar) -> Depo {
    let ayarlar = DepoAyarlari {
        drain: ayarlar.clone(),
        ..DepoAyarlari::default()
    };
    Depo::doldur(ayarlar, metin.as_bytes()).expect("bellekten okuma")
}

#[cfg(test)]
mod testler {
    use super::*;

    /// `Zaman` olarak: "2026-10-24 10:00:00"
    fn zaman(metin: &str) -> Zaman {
        Zaman::kayittan(metin)
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
        assert_eq!(depo.saat_dilimi(), SaatDilimi::ayristir("+03:00"));
        assert!(!depo.saat_dilimi_verildi());
        let iki = SaatDilimi::ayristir("+02:00").unwrap();
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
            .map(|metin| zaman(metin).saniye())
        );

        // Dilim verilirse her kayıt ona çevrilir; filtre de bu dilimdedir
        let ayarlar = DepoAyarlari {
            saat_dilimi: SaatDilimi::ayristir("Z"),
            filtre: Some(
                crate::filtre::Filtre {
                    baslangic: crate::zaman::ZamanNoktasi::ayristir("2026-10-25 00:00"),
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
                satir: 203,
                kayit: 59,
                devam: 141,
                yetim: 3
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
        assert_eq!(depo.ham_mesajlar().sayi(), 45);
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
        assert_eq!(depo.maskeli_mesajlar(), ["Kuyruk <SAYI> dolu"]);
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
        let saniye = |metin| Zaman::kayittan(metin).saniye();
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
        use crate::filtre::Filtre;
        use crate::zaman::ZamanNoktasi;
        let metin = "[2026-10-24 10:00:00] local.INFO: Başladı\n\
                     [2026-10-24 10:00:01] local.ERROR: Kuyruk 1 dolu\n\
                     [2026-10-24 11:00:00] local.ERROR: Kuyruk 2 dolu\n\
                     [2026-10-24 10:30:00] local.WARNING: Disk doluyor";
        let filtre = Filtre {
            seviye: Some(Seviye::Warning),
            bitis: ZamanNoktasi::ayristir("2026-10-24 11:00"),
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
            ["Kuyruk <SAYI> dolu", "Disk doluyor"]
        );
        // Sıra numaraları ve dizin yalnızca depodaki kayıtlar arasında
        let ilkler: Vec<usize> = depo.alt_gruplar().iter().map(AltGrup::ilk_kayit).collect();
        assert_eq!(ilkler, [0, 1]);
        assert_eq!(
            depo.dizin(),
            [metin.find("[2026-10-24 10:00:01]").unwrap() as u64]
        );
        assert!(depo.filtre().is_some());
    }

    #[test]
    fn seyrek_dizin_her_araliktaki_kaydin_konumu() {
        let satir = "[2026-10-24 10:00:00] local.INFO: x\n";
        let metin = format!("yetim\n{}", satir.repeat(2 * DIZIN_ARALIGI + 1));
        let depo = metinden(&metin);
        let konum = |sira: usize| (6 + sira * satir.len()) as u64;
        assert_eq!(
            depo.dizin(),
            [konum(0), konum(DIZIN_ARALIGI), konum(2 * DIZIN_ARALIGI)]
        );
    }
}
