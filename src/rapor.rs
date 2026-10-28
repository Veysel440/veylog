//! Özet raporu: genel sayılar, seviyeler ve kalıp tablosu.
//!
//! Kayıtlar okunurken depoya alınır (`depo` modülü): maskeli mesajları logdaki sırayla Drain'le
//! kümelenir; maskeli mesajı, Drain kümesi, istisnası ve seviyesi aynı olan kayıtlar bir alt
//! grup olur. Drain yalnızca metinle ilgilenir; seviyeyi, zamanı ve yığın izini bilmez. Bu modül
//! alt grupları iki şeye göre birleştirir: Drain'deki küme numaraları ve parmak izleri. Aynı
//! kalıptaki kayıtlar kodun farklı yerlerinden geliyorsa böylece ayrı satırlarda görünür.
//!
//! Özet (`Ozet`) bir kez hesaplanır; metin, JSON ve Markdown çıktıları aynı özeti yazar.
//! `--karsilastir` verildiyse özet, logun iki yarısının karşılaştırmasını da içerir
//! (`karsilastirma` modülü); `--ani-artis` verildiyse kalıpların olağan seviyesinden sıçradığı
//! anları (`ani_artis` modülü).
//!
//! Logdan gelen metinler (mesajlar, yollar) güvenilmezdir: kullanıcının bir forma yazdığı şey
//! olduğu gibi loga düşebilir. Metin ve Markdown çıktısı bu metinleri `gorunur` işlevinden
//! geçirir; içlerindeki kontrol karakterleri terminale komut olarak ulaşmaz.

use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::HashMap;
use std::fmt;
use std::io::{self, Write};

use crate::ani_artis::{AniArtis, AniArtislar, EN_AZ_DILIM, EN_DUSUK_SEVIYE, ESIK};
use crate::depo::{AltGrup, Depo, DilimSayilari};
use crate::drain::{Ayarlar, cekirdek};
use crate::filtre::Filtre;
use crate::karsilastirma::{
    ANLAMLILIK, ARTIS_KATI, Degisim, Karsilastirma, KarsilastirmaHatasi, Pencere,
};
use crate::kayit::Seviye;
use crate::okuyucu::OrnekKaynagi;
use crate::parmak_izi::{KonumTuru, ParmakIzi};
use crate::satir_bicimi::SatirBicimi;
use crate::zaman::{SaatDilimi, Zaman, ZamanNoktasi, sure_tam_yaz, sure_yaz};

/// Tabloda varsayılan olarak gösterilecek satır sayısı.
pub const VARSAYILAN_ILK: usize = 10;
/// Uzun kalıplar ve örnekler bu kadar karakterden sonra kesilir (metin ve Markdown).
pub const KALIP_GENISLIGI: usize = 80;
/// Bir `<*>` konumu için gösterilecek en fazla değer sayısı; kalanı "+N başka" olur.
pub const EN_COK_DEGER: usize = 5;
/// Metin çıktısında "değerler: " başlığının genişliği; taşan satırlar bu kadar içeriden başlar.
const DEGERLER_GENISLIGI: usize = 10;

/// Tablonun sırası.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Siralama {
    /// Önce seviye, aynı seviyede adet: en ciddi sorun en üstte.
    Onem,
    /// Önce adet, aynı adette seviye: en sık tekrar eden en üstte.
    Adet,
    /// İlk görülme zamanı en yeni olan en üstte: en son ortaya çıkan sorunlar.
    Yeni,
}

impl Siralama {
    /// Bütün sıralamalar. Yardım metni ve hata mesajları bu listeden üretilir.
    pub const HEPSI: [Siralama; 3] = [Siralama::Onem, Siralama::Adet, Siralama::Yeni];

    /// Komut satırındaki adı.
    pub fn ad(self) -> &'static str {
        match self {
            Siralama::Onem => "onem",
            Siralama::Adet => "adet",
            Siralama::Yeni => "yeni",
        }
    }

    /// Yardım metnindeki açıklaması.
    pub fn aciklama(self) -> &'static str {
        match self {
            Siralama::Onem => "önce seviye, sonra adet",
            Siralama::Adet => "önce adet, sonra seviye",
            Siralama::Yeni => "ilk görülme zamanı en yeni olan önce",
        }
    }

    /// Komut satırındaki adından sıralamaya çevirir: "adet" → Siralama::Adet
    ///
    /// Adlar `ad` metodundan alınır; böylece ad yalnızca bir yerde yazılır.
    pub fn ayristir(ad: &str) -> Option<Siralama> {
        Siralama::HEPSI
            .into_iter()
            .find(|siralama| siralama.ad() == ad)
    }

    /// Satırların sırasını bu sıralamaya göre dizer: `sira`, `satirlar` içindeki sıra
    /// numaralarıdır. Sıralama kararlıdır: eşit olanlar ilk görüldükleri sırada kalır.
    fn uygula(self, sira: &mut [usize], satirlar: &[KalipSatiri]) {
        let satir = |numara: &usize| &satirlar[*numara];
        match self {
            Siralama::Onem => sira.sort_by_key(|numara| {
                let satir = satir(numara);
                Reverse((satir.seviye, satir.adet))
            }),
            Siralama::Adet => sira.sort_by_key(|numara| {
                let satir = satir(numara);
                Reverse((satir.adet, satir.seviye))
            }),
            Siralama::Yeni => sira.sort_by_key(|numara| {
                let satir = satir(numara);
                Reverse((satir.ilk, satir.seviye))
            }),
        }
    }
}

/// Raporun nasıl üretileceği: kalıp çıkarma ayarları, tablonun sırası ve uzunluğu.
#[derive(Debug, Clone)]
pub struct RaporAyarlari {
    pub drain: Ayarlar,
    pub siralama: Siralama,
    /// Tabloda gösterilecek en fazla satır; None ise hepsi.
    pub ilk: Option<usize>,
    /// Logun ikiye bölünüp karşılaştırılacağı an; None ise karşılaştırma yok.
    pub karsilastir: Option<ZamanNoktasi>,
    /// Ani artışların arandığı dilimlerin uzunluğu, dakika; None ise ani artış aranmaz.
    pub ani_artis: Option<u16>,
    /// Hangi kayıtların özetleneceği; boşsa hepsi.
    pub filtre: Filtre,
    /// Raporun saat dilimi (`--saat-dilimi`); None ise logda saat dilimi yazılı ilk kaydınki.
    pub saat_dilimi: Option<SaatDilimi>,
    /// Satırların biçimi (`--desen`, `--zaman-bicimi`); varsayılanı Laravel'inki.
    pub satir_bicimi: SatirBicimi,
    /// Parmak izlerinin konumu neyle belirlenir (`--konum`).
    pub konum_turu: KonumTuru,
}

impl Default for RaporAyarlari {
    fn default() -> Self {
        RaporAyarlari {
            drain: Ayarlar::default(),
            siralama: Siralama::Onem,
            ilk: Some(VARSAYILAN_ILK),
            karsilastir: None,
            ani_artis: None,
            filtre: Filtre::default(),
            saat_dilimi: None,
            satir_bicimi: SatirBicimi::default(),
            konum_turu: KonumTuru::default(),
        }
    }
}

/// Tablodaki bir satır: bir kalıp, bir parmak izi ve onlara uyan kayıtlar hakkındaki bilgiler.
#[derive(Debug)]
pub struct KalipSatiri {
    /// Kalıbın son hali: "Zamanlanmış görev başladı: <*>"
    pub kalip: String,
    pub adet: usize,
    /// Satırdaki kayıtların en önemli seviyesi.
    pub seviye: Seviye,
    /// İlk ve son görülme zamanı, raporun saat diliminde.
    pub ilk: Zaman,
    pub son: Zaman,
    /// Satırdaki ilk kaydın maskelenmemiş mesajı: "Kullanıcı 381 giriş yaptı"
    pub ornek: String,
    /// Kayıtlar bir istisnadan geliyorsa: istisnanın sınıfı ve uygulama kodundaki yeri.
    pub parmak_izi: Option<ParmakIzi>,
    /// Kalıptaki her `<*>` konumunun satırdaki kayıtlarda aldığı değerler, kalıptaki sırayla.
    /// Kalıpta `<*>` yoksa boş.
    pub degiskenler: Vec<DegiskenKonum>,
}

/// Kalıptaki bir `<*>` konumunun aldığı değerler: en çok görülen `EN_COK_DEGER` değer ve kaç
/// farklı değerin daha görüldüğü.
///
/// Değerler depodaki maskeli mesajlardan rapor zamanında sayılır: her farklı maskeli mesaj zaten
/// bir alt gruptur, ek bellek gerekmez. Belirtecin cümleden gelen ayraç ve noktalaması
/// (`drain::cekirdek`) değere katılmaz: `(Ayşe,` değeri `Ayşe`dir. Kategorik bir konumun (`drain` modülündeki "aynı
/// biçimdeki kodsal değerler") değerleri hangi işlerin, olayların ya da durumların görüldüğünü
/// söyler; rakamlı bir konumda (`Yanıt süresi <*>`) en sık değerler görünür.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DegiskenKonum {
    /// Konumun kalıptaki sırası, 1'den: `Kullanıcı <SAYI> <*> yaptı` kalıbında 3.
    pub konum: usize,
    /// Değer ve kayıt adedi, adede göre azalan; eşitlikte logda önce görülen önce.
    pub degerler: Vec<(String, usize)>,
    /// Listelenmeyen farklı değer sayısı.
    pub baska: usize,
}

/// Bir `<*>` konumunun değerlerini sayar.
#[derive(Debug, Default)]
struct DegerSayaci<'d> {
    /// Değerler ilk görüldükleri sırayla.
    sira: Vec<&'d str>,
    adetler: HashMap<&'d str, usize>,
}

impl<'d> DegerSayaci<'d> {
    fn say(&mut self, deger: &'d str, adet: usize) {
        match self.adetler.get_mut(deger) {
            Some(toplam) => *toplam += adet,
            None => {
                self.sira.push(deger);
                self.adetler.insert(deger, adet);
            }
        }
    }

    /// En çok görülen değerler. `konum`: konumun kalıptaki sırası, 1'den.
    fn ozetle(&self, konum: usize) -> DegiskenKonum {
        let mut sirali: Vec<(&str, usize)> = self
            .sira
            .iter()
            .map(|&deger| (deger, self.adetler[deger]))
            .collect();
        // Kararlı sıralama: aynı adetteki değerler ilk görülme sırasını korur
        sirali.sort_by_key(|&(_, adet)| Reverse(adet));
        let gosterilen = sirali.len().min(EN_COK_DEGER);
        DegiskenKonum {
            konum,
            degerler: sirali[..gosterilen]
                .iter()
                .map(|&(deger, adet)| (deger.to_owned(), adet))
                .collect(),
            baska: sirali.len() - gosterilen,
        }
    }
}

/// Kalıp çıkarmanın sonucu.
#[derive(Debug)]
pub struct KalipAnalizi {
    /// Maskelemeden önce kaç farklı mesaj vardı?
    pub ham_mesaj_sayisi: usize,
    /// Farklı mesaj sayısı bir tahmin mi? Çok sayıda farklı mesajda sayı HyperLogLog ile
    /// tahmin edilir (`farkli_sayac` modülü).
    pub ham_mesaj_yaklasik: bool,
    /// Maskelemeden sonra kaç farklı mesaj kaldı?
    pub maskeli_mesaj_sayisi: usize,
    /// Drain kaç kalıp buldu? Parmak izi bir kalıbı birden çok satıra bölebilir.
    pub kalip_sayisi: usize,
    /// Ayarlardaki sıralamaya göre dizilmiş satırlar.
    pub satirlar: Vec<KalipSatiri>,
    /// Her alt grubun satırı: `satirlar` içindeki sırası. Karşılaştırma ve ani artış kayıtları
    /// bununla satırlara sayar.
    pub alt_grup_satiri: Vec<usize>,
}

/// Kalıp çıkarılırken her grup için biriktirilen bilgiler. Bir grup, mesajları aynı Drain
/// kümesine düşen ve aynı parmak izini taşıyan alt gruplardır.
struct Grup<'d> {
    /// Drain'deki küme numarası; kalıp metni en sonda bununla alınır.
    kume: usize,
    parmak_izi: Option<&'d ParmakIzi>,
    adet: usize,
    seviye: Seviye,
    ilk: Zaman,
    son: Zaman,
    /// En geç kaydının sıra numarası: zamanı `son` olanlardan logda en sonra geleni.
    son_kayit: usize,
    ornek: &'d str,
    /// Kalıptaki her `<*>` konumunun değerleri, `jokerler` ile aynı sırada.
    degerler: Vec<DegerSayaci<'d>>,
}

impl<'d> Grup<'d> {
    /// Grubu ilk alt grubuyla açar. Alt gruplar ilk kayıtlarının sırasıyla geldiği için grubun
    /// ilk kaydı, dolayısıyla örneği de bu alt gruptadır. Alt grubun kendisini saymak
    /// `ekle`nin işidir. `joker_sayisi`: kalıptaki `<*>` sayısı.
    fn yeni(
        kume: usize,
        parmak_izi: Option<&'d ParmakIzi>,
        ilk_alt_grup: &'d AltGrup,
        joker_sayisi: usize,
    ) -> Grup<'d> {
        Grup {
            kume,
            parmak_izi,
            adet: 0,
            seviye: ilk_alt_grup.seviye(),
            ilk: ilk_alt_grup.ilk(),
            son: ilk_alt_grup.son(),
            son_kayit: ilk_alt_grup.son_kayit(),
            ornek: ilk_alt_grup.ornek(),
            degerler: (0..joker_sayisi).map(|_| DegerSayaci::default()).collect(),
        }
    }

    /// Alt grubu gruba katar: adedi artırır, en yüksek seviyeyi ve zaman aralığını günceller,
    /// `<*>` konumlarındaki değerlerini sayar. `parmak_izi`: alt grubun parmak izi.
    /// Gruptakilerin hepsi aynı hatanındır; satırları farklı olabilir (konum fonksiyonla
    /// belirlenir). Gösterilen satır en yeni kaydınki olsun: kod değiştiyse güncel satır. Aynı
    /// andaki kayıtlardan logda sonra gelen daha yenidir. `maskeli`: alt grubun maskeli mesajı;
    /// `jokerler`: kalıptaki `<*>` konumları, artan sırada.
    fn ekle(
        &mut self,
        alt_grup: &'d AltGrup,
        parmak_izi: Option<&'d ParmakIzi>,
        maskeli: &'d str,
        jokerler: &[usize],
    ) {
        if (alt_grup.son(), alt_grup.son_kayit()) >= (self.son, self.son_kayit) {
            self.parmak_izi = parmak_izi;
            self.son_kayit = alt_grup.son_kayit();
        }
        self.adet += alt_grup.adet();
        self.seviye = self.seviye.max(alt_grup.seviye());
        self.ilk = self.ilk.min(alt_grup.ilk());
        self.son = self.son.max(alt_grup.son());
        if jokerler.is_empty() {
            return;
        }
        // Maskeli mesaj kalıpla aynı sayıda belirteçtir (Drain yalnızca aynı uzunluktakileri
        // bir kümeye koyar); belirteçler sırayla gezilir, joker konumlarındakiler sayılır
        let mut jokerler = jokerler.iter().zip(&mut self.degerler).peekable();
        for (sira, belirtec) in maskeli.split_whitespace().enumerate() {
            if let Some(&(&konum, _)) = jokerler.peek()
                && konum == sira
                && let Some((_, sayac)) = jokerler.next()
            {
                // Değer cümleden gelen ayraç ve noktalama olmadan sayılır: "(Ayşe," → "Ayşe"
                sayac.say(cekirdek(belirtec), alt_grup.adet());
            }
        }
    }
}

/// Alt grupları kalıp satırlarına toplar: küme ve parmak izine göre birleştirir, satırları
/// ayarlardaki sıraya dizer.
pub fn kaliplari_cikar(depo: &Depo, ayarlar: &RaporAyarlari) -> KalipAnalizi {
    // Kayıtlar okunurken kümelendi; kalıplar bütün kayıtlar kümelenince son hallerindedir
    let kumeler = depo.kumeler();
    // Parmak izleri de bütün log okununca çıkarılır: bir kaydın proje kökü başka kayıtlardan
    // bulunabilir
    let parmak_izleri = depo.parmak_izleri();

    // Her kümenin kalıbındaki `<*>` konumları; kalıplar son hallerinde
    let jokerler: Vec<Vec<usize>> = kumeler
        .iter()
        .map(|kume| kume.joker_konumlari().collect())
        .collect();

    let mut gruplar: Vec<Grup<'_>> = Vec::new();
    // (küme numarası, parmak izi) → grubun `gruplar` listesindeki sırası
    let mut grup_sirasi: HashMap<(usize, Option<&ParmakIzi>), usize> = HashMap::new();
    let mut alt_grup_grubu = Vec::with_capacity(depo.alt_gruplar().len());
    for (alt_grup, parmak_izi) in depo.alt_gruplar().iter().zip(&parmak_izleri) {
        let kume = alt_grup.kume();
        let parmak_izi = parmak_izi.as_ref();
        // Bu küme ve parmak izi ilk kez görülüyorsa yeni bir grup aç, sonra alt grubu ona kat
        let sira = *grup_sirasi.entry((kume, parmak_izi)).or_insert_with(|| {
            gruplar.push(Grup::yeni(kume, parmak_izi, alt_grup, jokerler[kume].len()));
            gruplar.len() - 1
        });
        gruplar[sira].ekle(
            alt_grup,
            parmak_izi,
            depo.maskeli_mesaj(alt_grup.maskeli()),
            &jokerler[kume],
        );
        alt_grup_grubu.push(sira);
    }

    // Kalıp metni yeni mesajlar geldikçe değiştiği için bütün mesajlar kümelendikten sonra alınır
    let satirlar: Vec<KalipSatiri> = gruplar
        .iter()
        .map(|grup| KalipSatiri {
            kalip: kumeler[grup.kume].metin(),
            adet: grup.adet,
            seviye: grup.seviye,
            ilk: grup.ilk,
            son: grup.son,
            ornek: grup.ornek.to_owned(),
            parmak_izi: grup.parmak_izi.cloned(),
            degiskenler: grup
                .degerler
                .iter()
                .zip(&jokerler[grup.kume])
                .map(|(sayac, &konum)| sayac.ozetle(konum + 1))
                .collect(),
        })
        .collect();
    let mut sira: Vec<usize> = (0..satirlar.len()).collect();
    ayarlar.siralama.uygula(&mut sira, &satirlar);
    // Grubun sıralanmış listedeki yeri
    let mut yeri = vec![0; satirlar.len()];
    for (yer, &grup) in sira.iter().enumerate() {
        yeri[grup] = yer;
    }
    let mut satirlar: Vec<Option<KalipSatiri>> = satirlar.into_iter().map(Some).collect();
    let sirali = sira
        .iter()
        .map(|&grup| satirlar[grup].take().expect("her grup bir kez"))
        .collect();

    KalipAnalizi {
        ham_mesaj_sayisi: depo.ham_mesajlar().sayi(),
        ham_mesaj_yaklasik: depo.ham_mesajlar().yaklasik_mi(),
        maskeli_mesaj_sayisi: depo.maskeli_mesaj_sayisi(),
        kalip_sayisi: kumeler.len(),
        satirlar: sirali,
        alt_grup_satiri: alt_grup_grubu.iter().map(|&grup| yeri[grup]).collect(),
    }
}

/// Özet çıkarılamadı.
#[derive(Debug)]
pub enum OzetHatasi {
    /// `--karsilastir` noktası bu loga uymuyor: bir kullanım hatası.
    Karsilastirma(KarsilastirmaHatasi),
    /// Bir kaydın mesajı için girdi yeniden okunamadı.
    Okuma(io::Error),
}

impl fmt::Display for OzetHatasi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OzetHatasi::Karsilastirma(hata) => write!(f, "{hata}"),
            OzetHatasi::Okuma(hata) => write!(f, "{hata}"),
        }
    }
}

impl std::error::Error for OzetHatasi {}

/// Raporun hangi kayıtlarla çıkarıldığı: verilen filtre, logun sonuna göre çözülmüş
/// sınırları ve filtreye uyan kayıt sayısı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FiltreOzeti {
    /// `--seviye` verildiyse en düşük seviye.
    pub seviye: Option<Seviye>,
    /// Zaman sınırları: başlangıç dahil, bitiş hariç.
    pub baslangic: Option<Zaman>,
    pub bitis: Option<Zaman>,
    /// `--son` verildiyse süresi, saniye.
    pub son: Option<u64>,
    pub kayit_sayisi: usize,
}

impl FiltreOzeti {
    /// Metin ve Markdown'daki açıklaması:
    /// "WARNING ve üstü, 2026-10-24 16:31:07 ve sonrası (son 2 sa 0 dk): 12 kayıt"
    pub fn aciklama(&self) -> String {
        let mut parcalar = Vec::new();
        if let Some(seviye) = self.seviye {
            parcalar.push(format!("{} ve üstü", seviye.ad()));
        }
        let son = self
            .son
            .map(|saniye| format!(" (son {})", sure_tam_yaz(saniye)))
            .unwrap_or_default();
        match (self.baslangic, self.bitis) {
            (Some(bas), Some(bit)) => parcalar.push(format!("{bas} ile {bit} arası")),
            (Some(bas), None) => parcalar.push(format!("{bas} ve sonrası{son}")),
            (None, Some(bit)) => parcalar.push(format!("{bit} öncesi")),
            (None, None) => {}
        }
        format!("{}: {} kayıt", parcalar.join(", "), self.kayit_sayisi)
    }
}

/// Raporun saat dilimi ve kayıtların ona nasıl alındığı.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaatDilimiOzeti {
    /// Raporun saat dilimi: bütün zamanlar bu dilimdedir.
    pub dilim: SaatDilimi,
    /// Komut satırında mı verildi (`--saat-dilimi`), logda yazılı ilk dilim mi?
    pub verildi: bool,
    /// Kayıtların dilimleri: kaçı çevrildi, kaçında dilim yazılmamış.
    pub sayilar: DilimSayilari,
    /// Rapordaki (filtre varsa filtreye uyan) kayıt sayısı.
    pub kayit_sayisi: usize,
}

impl SaatDilimiOzeti {
    /// Metin ve Markdown'daki açıklaması:
    /// "+03:00 (--saat-dilimi); 1534597 kayıt +00:00 diliminden çevrildi"
    pub fn aciklama(&self) -> String {
        let mut parcalar = vec![if self.verildi {
            format!("{} (--saat-dilimi)", self.dilim)
        } else {
            self.dilim.to_string()
        }];
        let sayilar = &self.sayilar;
        if sayilar.cevrilen > 0 {
            let dilimler: Vec<String> = sayilar
                .cevrilen_dilimler
                .iter()
                .map(ToString::to_string)
                .collect();
            parcalar.push(match dilimler.as_slice() {
                [dilim] => format!("{} kayıt {dilim} diliminden çevrildi", sayilar.cevrilen),
                _ => format!(
                    "{} kayıt başka dilimlerden çevrildi ({})",
                    sayilar.cevrilen,
                    liste_yaz(&dilimler, 3)
                ),
            });
        }
        if sayilar.yazilmamis > 0 {
            parcalar.push(if sayilar.yazilmamis == self.kayit_sayisi {
                "kayıtlarda saat dilimi yazılı değil, zamanlar logdaki gibi".to_string()
            } else {
                format!(
                    "{} kaydın saat dilimi yazılmamış, bu dilimde sayıldı",
                    sayilar.yazilmamis
                )
            });
        }
        parcalar.join("; ")
    }
}

/// Bir listeyi Türkçe yazar: "a", "a ve b", "a, b ve c". `sinir`'dan fazla öge varsa ilk
/// `sinir` tanesi ve kalanların sayısı: "a, b, c ve 2 dilim daha". Yalnızca bir öge fazlaysa
/// o da yazılır; "1 dilim daha" demek onu yazmaktan uzundur.
fn liste_yaz(ogeler: &[String], sinir: usize) -> String {
    match ogeler {
        [] => String::new(),
        [tek] => tek.clone(),
        _ if ogeler.len() > sinir + 1 => format!(
            "{} ve {} dilim daha",
            ogeler[..sinir].join(", "),
            ogeler.len() - sinir
        ),
        [bas @ .., son] => format!("{} ve {son}", bas.join(", ")),
    }
}

/// Raporun bütün bilgileri. Bir kez hesaplanır; metin, JSON ve Markdown çıktıları aynı özeti
/// farklı biçimlerde yazar. Böylece üç biçimdeki sayılar hep aynıdır.
pub struct Ozet {
    /// Dosyanın yolu, ekranda gösterileceği haliyle.
    pub dosya: String,
    /// Dosyanın satır ve kayıt sayıları; filtre verilse de dosyanın tamamınındır.
    pub satir_sayisi: usize,
    pub kayit_sayisi: usize,
    pub devam_satiri_sayisi: usize,
    pub yetim_satir_sayisi: usize,
    /// Filtre verildiyse açıklaması; özetin geri kalanı yalnızca filtreye uyan kayıtlardandır.
    pub filtre: Option<FiltreOzeti>,
    /// En eski ve en yeni kaydın zamanı, raporun saat diliminde; hiç kayıt yoksa None.
    pub zaman_araligi: Option<(Zaman, Zaman)>,
    /// Raporun saat dilimi; logda saat dilimi yazılmamışsa ve verilmediyse None.
    pub saat_dilimi: Option<SaatDilimiOzeti>,
    /// Her seviyeden kaç kayıt var; en önemli seviye başta.
    pub seviyeler: Vec<(Seviye, usize)>,
    pub analiz: KalipAnalizi,
    /// `--karsilastir` verildiyse logun iki yarısının karşılaştırması.
    pub karsilastirma: Option<Karsilastirma>,
    /// `--ani-artis` verildiyse kalıpların ani artışları.
    pub ani_artislar: Option<AniArtislar>,
    pub ayarlar: RaporAyarlari,
}

impl Ozet {
    /// Deponun özeti. `dosya`: dosyanın ekranda gösterilecek yolu. `ornekler`: bir örnek mesaj
    /// depoda yoksa (ani artışlar, ikinci kümeleme turunda bölünen alt gruplar) girdiyi yeniden
    /// okuyan kaynak.
    ///
    /// Karşılaştırma noktası logun dışındaysa (ör. bütün kayıtlardan önce) özet çıkarılamaz.
    pub fn hesapla(
        dosya: String,
        depo: &mut Depo,
        ayarlar: &RaporAyarlari,
        ornekler: &mut dyn OrnekKaynagi,
    ) -> Result<Ozet, OzetHatasi> {
        depo.ornekleri_tamamla(ornekler)
            .map_err(OzetHatasi::Okuma)?;
        let depo = &*depo;
        let analiz = kaliplari_cikar(depo, ayarlar);
        // Option<Result<K, H>> → Result<Option<K>, H>: `transpose`. Böylece `?` yalnızca
        // karşılaştırma istendiyse ve başarısız olduysa erken döner.
        let karsilastirma = ayarlar
            .karsilastir
            .map(|nokta| Karsilastirma::hesapla(depo, &analiz, nokta))
            .transpose()
            .map_err(OzetHatasi::Karsilastirma)?;
        // Log kısaysa ani artış bölümü bunu söyler; yalnızca örnekler için girdi yeniden
        // okunamazsa başarısız olur
        let ani_artislar = ayarlar
            .ani_artis
            .map(|dakika| AniArtislar::hesapla(depo, &analiz, dakika, ornekler))
            .transpose()
            .map_err(OzetHatasi::Okuma)?;
        let sayilar = depo.sayilar();
        Ok(Ozet {
            dosya,
            satir_sayisi: sayilar.satir,
            kayit_sayisi: sayilar.kayit,
            devam_satiri_sayisi: sayilar.devam,
            yetim_satir_sayisi: sayilar.yetim,
            filtre: depo.filtre().map(|cozulmus| FiltreOzeti {
                seviye: ayarlar.filtre.seviye,
                baslangic: cozulmus.baslangic(),
                bitis: cozulmus.bitis(),
                son: ayarlar.filtre.son,
                kayit_sayisi: depo.kayit_sayisi(),
            }),
            zaman_araligi: depo.zaman_araligi(),
            saat_dilimi: depo.saat_dilimi().map(|dilim| SaatDilimiOzeti {
                dilim,
                verildi: depo.saat_dilimi_verildi(),
                sayilar: depo.dilim_sayilari().clone(),
                kayit_sayisi: depo.kayit_sayisi(),
            }),
            seviyeler: depo.seviye_sayilari(),
            analiz,
            karsilastirma,
            ani_artislar,
            ayarlar: ayarlar.clone(),
        })
    }

    /// Tabloda zamanların nasıl gösterileceği: bütün kayıtlar aynı gündeyse saat yeter, aynı
    /// yıldaysa ay ve gün de, birden çok yıla yayılıyorsa yıl da gösterilir.
    pub fn zaman_gosterimi(&self) -> ZamanGosterimi {
        match self.zaman_araligi {
            Some((en_eski, en_yeni)) if en_eski.ayni_gun(en_yeni) => ZamanGosterimi::Saat,
            Some((en_eski, en_yeni)) if !en_eski.ayni_yil(en_yeni) => ZamanGosterimi::Tarih,
            _ => ZamanGosterimi::AyGun,
        }
    }

    /// Bir listenin gösterilecek kısmı: en fazla `--ilk` kadarı (`--hepsi` ile hepsi).
    ///
    /// Genel (generic) bir metottur: `T` kalıp satırı da olabilir, karşılaştırmadaki bir
    /// değişim de. Dönen dilim `self`'ten değil listeden ödünç alındığı için yaşam süresi
    /// (`'b`) açıkça yazılır; yazılmasa derleyici onu `&self`'e bağlardı.
    pub fn sinirla<'b, T>(&self, liste: &'b [T]) -> &'b [T] {
        // None (--hepsi): sınır yok
        let sinir = self.ayarlar.ilk.unwrap_or(usize::MAX);
        &liste[..liste.len().min(sinir)]
    }

    /// `--ilk` sınırı yüzünden listenin dışarıda kalan kısmının uzunluğu.
    pub fn disarida_kalan<T>(&self, liste: &[T]) -> usize {
        liste.len() - self.sinirla(liste).len()
    }

    /// Tabloda gösterilecek satırlar: en fazla `--ilk` kadar.
    pub fn gosterilecek_satirlar(&self) -> &[KalipSatiri] {
        self.sinirla(&self.analiz.satirlar)
    }

    /// `--ilk` sınırı yüzünden gösterilmeyen satır sayısı.
    pub fn gosterilmeyen(&self) -> usize {
        self.disarida_kalan(&self.analiz.satirlar)
    }

    /// Kalıp çıkarmanın adımları tek cümlede: "45 farklı mesaj → maskeleyince 27 → ..."
    /// Farklı mesaj sayısı bir tahminse önünde ≈ yazar.
    pub fn kalip_cumlesi(&self) -> String {
        let analiz = &self.analiz;
        let yaklasik = if analiz.ham_mesaj_yaklasik { "≈" } else { "" };
        format!(
            "{yaklasik}{} farklı mesaj → maskeleyince {} → Drain ile {} kalıp (eşik {}) → parmak iziyle {} grup",
            analiz.ham_mesaj_sayisi,
            analiz.maskeli_mesaj_sayisi,
            analiz.kalip_sayisi,
            ondalik(self.ayarlar.drain.benzerlik_esigi),
            analiz.satirlar.len()
        )
    }
}

/// Özeti terminal için düz metin olarak yazar: ekrana (stdout) ya da testlerde bir bayt dizisine.
///
/// Logdan ve dosya adından gelen her metin `gorunur`dan geçer. Sayılar, seviye adları ve
/// (biçimi denetlenmiş) zamanlar kontrol karakteri içeremez.
pub fn metin_yaz(cikti: &mut impl Write, ozet: &Ozet) -> io::Result<()> {
    writeln!(
        cikti,
        "{}: {} satır",
        gorunur(&ozet.dosya),
        ozet.satir_sayisi
    )?;
    writeln!(cikti, "  Kayıt         : {}", ozet.kayit_sayisi)?;
    writeln!(cikti, "  Devam satırı  : {}", ozet.devam_satiri_sayisi)?;
    writeln!(cikti, "  Yetim satır   : {}", ozet.yetim_satir_sayisi)?;
    if let Some(filtre) = &ozet.filtre {
        writeln!(cikti, "  Filtre        : {}", filtre.aciklama())?;
    }
    if let Some((en_eski, en_yeni)) = &ozet.zaman_araligi {
        writeln!(cikti, "  Zaman aralığı : {en_eski} → {en_yeni}")?;
    }
    if let Some(dilim) = &ozet.saat_dilimi {
        writeln!(cikti, "  Saat dilimi   : {}", dilim.aciklama())?;
    }

    writeln!(cikti)?;
    writeln!(cikti, "Seviyeler:")?;
    // Sayılar sağa yaslanır; sütun en az 4, en uzun sayı kadar geniştir
    let genislik = ozet
        .seviyeler
        .iter()
        .map(|(_, adet)| adet.to_string().len())
        .fold(4, usize::max);
    for (seviye, adet) in &ozet.seviyeler {
        writeln!(cikti, "  {:<10}{adet:>genislik$}", seviye.ad())?;
    }

    writeln!(cikti)?;
    writeln!(cikti, "Kalıplar: {}", ozet.kalip_cumlesi())?;

    let satirlar = ozet.gosterilecek_satirlar();
    let gosterim = ozet.zaman_gosterimi();
    if !satirlar.is_empty() {
        let hucreler: Vec<[String; 1]> = satirlar
            .iter()
            .map(|satir| [satir.adet.to_string()])
            .collect();
        let (baslik, sayilar) = sayi_sutunlari(["adet"], &hucreler);
        baslik_yaz(cikti, &baslik, gosterim)?;
        for (satir, sayilar) in satirlar.iter().zip(&sayilar) {
            satir_yaz(
                cikti,
                sayilar,
                satir,
                &Hucreler::satirdan(satir),
                None,
                gosterim,
            )?;
        }
        gizlenen_yaz(cikti, ozet.gosterilmeyen())?;
    }

    if let Some(karsilastirma) = &ozet.karsilastirma {
        karsilastirma_yaz(cikti, ozet, karsilastirma, gosterim)?;
    }
    if let Some(ani_artislar) = &ozet.ani_artislar {
        ani_artis_yaz(cikti, ozet, ani_artislar, gosterim)?;
    }
    Ok(())
}

/// Karşılaştırma bölümü: iki yarının özeti, sonra yeni çıkan, kaybolan ve artan kalıplar.
fn karsilastirma_yaz(
    cikti: &mut impl Write,
    ozet: &Ozet,
    karsilastirma: &Karsilastirma,
    gosterim: ZamanGosterimi,
) -> io::Result<()> {
    writeln!(cikti)?;
    writeln!(
        cikti,
        "Karşılaştırma: {} öncesi ve sonrası",
        karsilastirma.nokta
    )?;
    for (ad, pencere) in [
        ("önce", &karsilastirma.once),
        ("sonra", &karsilastirma.sonra),
    ] {
        writeln!(
            cikti,
            "  {ad:<6}: {} → {}   {:<11}{:>5} kayıt, saatte {}",
            zaman_goster(pencere.baslangic, gosterim),
            zaman_goster(pencere.bitis, gosterim),
            sure_yaz(pencere.saniye),
            pencere.kayit_sayisi,
            ondalik_kisa(pencere.saatlik(pencere.kayit_sayisi))
        )?;
    }

    for (baslik, degisimler) in karsilastirma_bolumleri(karsilastirma) {
        writeln!(cikti)?;
        if degisimler.is_empty() {
            writeln!(cikti, "{baslik}: yok")?;
            continue;
        }
        writeln!(cikti, "{baslik}: {}", degisimler.len())?;
        let gosterilenler = ozet.sinirla(degisimler);
        let hucreler: Vec<[String; 2]> = gosterilenler
            .iter()
            .map(|degisim| [degisim.once.to_string(), degisim.sonra.to_string()])
            .collect();
        let (sutun_basligi, sayilar) = sayi_sutunlari(["önce", "sonra"], &hucreler);
        baslik_yaz(cikti, &sutun_basligi, gosterim)?;
        for (degisim, sayilar) in gosterilenler.iter().zip(&sayilar) {
            let satir = &ozet.analiz.satirlar[degisim.satir];
            // Artanlarda sıklığın nasıl değiştiği de yazılır
            let artis = (degisim.once > 0 && degisim.sonra > 0)
                .then(|| artis_yaz(degisim, &karsilastirma.once, &karsilastirma.sonra));
            satir_yaz(
                cikti,
                sayilar,
                satir,
                &Hucreler::satirdan(satir),
                artis.as_deref(),
                gosterim,
            )?;
        }
        gizlenen_yaz(cikti, ozet.disarida_kalan(degisimler))?;
    }
    Ok(())
}

/// Karşılaştırmanın üç bölümü, başlıklarıyla. Metin ve Markdown aynı başlıkları kullanır.
pub fn karsilastirma_bolumleri(karsilastirma: &Karsilastirma) -> [(String, &[Degisim]); 3] {
    [
        (
            "Yeni çıkanlar (yalnızca sonra)".to_string(),
            karsilastirma.yeni.as_slice(),
        ),
        (
            "Kaybolanlar (yalnızca önce)".to_string(),
            karsilastirma.kaybolan.as_slice(),
        ),
        (
            // Eşikler sabitlerden okunur; metin onlarla çelişemez
            format!(
                "Artanlar (sonra ≥ {} kat sık, tesadüf ihtimali < %{})",
                ondalik(ARTIS_KATI),
                ondalik(ANLAMLILIK * 100.0)
            ),
            karsilastirma.artan.as_slice(),
        ),
    ]
}

/// Bir artışın açıklaması: "saatte 0,1 → 2,1 (15,4 kat)"
pub fn artis_yaz(degisim: &Degisim, once: &Pencere, sonra: &Pencere) -> String {
    format!(
        "saatte {} → {} ({} kat)",
        ondalik_kisa(once.saatlik(degisim.once)),
        ondalik_kisa(sonra.saatlik(degisim.sonra)),
        ondalik_kisa(degisim.kat)
    )
}

/// Ani artış bölümü: puanı en yüksek olan artışlar. Satırın seviyesi, ilk ve son zamanı ve
/// örneği kalıbın değil, artışın kayıtlarındandır.
fn ani_artis_yaz(
    cikti: &mut impl Write,
    ozet: &Ozet,
    ani_artislar: &AniArtislar,
    gosterim: ZamanGosterimi,
) -> io::Result<()> {
    writeln!(cikti)?;
    writeln!(cikti, "{}", ani_artis_basligi(ani_artislar))?;
    if ani_artislar.artislar.is_empty() {
        return Ok(());
    }
    let gosterilenler = ozet.sinirla(&ani_artislar.artislar);
    let hucreler: Vec<[String; 3]> = gosterilenler
        .iter()
        .map(|artis| {
            [
                artis.zirve_adet.to_string(),
                ondalik(artis.olagan),
                ondalik_kisa(artis.puan),
            ]
        })
        .collect();
    let (baslik, sayilar) = sayi_sutunlari(["zirve", "olağan", "puan"], &hucreler);
    baslik_yaz(cikti, &baslik, gosterim)?;
    for (artis, sayilar) in gosterilenler.iter().zip(&sayilar) {
        satir_yaz(
            cikti,
            sayilar,
            &ozet.analiz.satirlar[artis.satir],
            &Hucreler::artistan(artis),
            Some(&ani_artis_aciklamasi(artis, gosterim)),
            gosterim,
        )?;
    }
    gizlenen_yaz(cikti, ozet.disarida_kalan(&ani_artislar.artislar))
}

/// Ani artış bölümünün başlığı; metin ve Markdown aynı başlığı kullanır. Eşikler sabitlerden
/// okunur:
/// "Ani artışlar (WARNING ve üstü, 10 dakikalık 59 dilim, puan > 3,5): 1"
pub fn ani_artis_basligi(ani_artislar: &AniArtislar) -> String {
    let kapsam = format!(
        "{} ve üstü, {} dakikalık",
        EN_DUSUK_SEVIYE.ad(),
        ani_artislar.dilim_dakika
    );
    let dilim_sayisi = ani_artislar.dilim_sayisi;
    if ani_artislar.aralik.is_none() {
        let neden = if ani_artislar.filtreli {
            "filtreye uyan kayıt yok"
        } else {
            "logda kayıt yok"
        };
        format!("Ani artışlar ({kapsam} dilimler): {neden}")
    } else if !ani_artislar.yeterli() {
        format!(
            "Ani artışlar ({kapsam} {dilim_sayisi} dilim): olağan seviyeyi ölçmek için en az {EN_AZ_DILIM} dilim gerekir"
        )
    } else {
        let sonuc = match ani_artislar.artislar.len() {
            0 => "yok".to_string(),
            sayi => sayi.to_string(),
        };
        format!(
            "Ani artışlar ({kapsam} {dilim_sayisi} dilim, puan > {}): {sonuc}",
            ondalik(ESIK)
        )
    }
}

/// Bir artışın açıklaması: "16:10–16:20 arası 6 kayıt". Artış birden çok dilime yayılıyorsa
/// zirvesi de yazılır: "16:10–16:40 arası 3 dilimde 25 kayıt, zirve 16:20–16:30"
fn ani_artis_aciklamasi(artis: &AniArtis, gosterim: ZamanGosterimi) -> String {
    let aralik = dilim_araligi(artis.baslangic, artis.bitis, gosterim);
    if artis.dilim_sayisi == 1 {
        format!("{aralik} arası {} kayıt", artis.kayit_sayisi)
    } else {
        format!(
            "{aralik} arası {} dilimde {} kayıt, zirve {}",
            artis.dilim_sayisi,
            artis.kayit_sayisi,
            dilim_araligi(artis.zirve_baslangic, artis.zirve_bitis, gosterim)
        )
    }
}

/// Dilim sınırlarından bir aralık. Sınırlar hep dakika başı olduğu için saniye yazılmaz. Tek
/// günlük logda "16:10–16:20"; çok günlükte "10-24 16:10–16:20", gün değişiyorsa
/// "10-24 23:50–10-25 00:10". Birden çok yıla yayılan logda tarih yılıyla yazılır.
///
/// `Zaman`'ın metni hep 19 ASCII karakterdir (`2026-10-24 16:10:00`); bayt konumlarıyla
/// dilimlemek güvenlidir.
pub fn dilim_araligi(baslangic: Zaman, bitis: Zaman, gosterim: ZamanGosterimi) -> String {
    let (bas, son) = (baslangic.to_string(), bitis.to_string());
    let tarih = |zaman: &str| match gosterim {
        ZamanGosterimi::Tarih => zaman[..10].to_string(),
        _ => zaman[5..10].to_string(),
    };
    if gosterim == ZamanGosterimi::Saat {
        format!("{}–{}", &bas[11..16], &son[11..16])
    } else if bas[..10] == son[..10] {
        format!("{} {}–{}", tarih(&bas), &bas[11..16], &son[11..16])
    } else {
        format!(
            "{} {}–{} {}",
            tarih(&bas),
            &bas[11..16],
            tarih(&son),
            &son[11..16]
        )
    }
}

/// Tablonun satır başındaki sayı sütunları: başlık satırı ve her satırın sütunları. Sütunlar
/// sağa yaslanır, aralarında iki boşluk olur. Her sütun başlığı ya da en geniş değeri kadar
/// geniştir: 12 345 kayıtlı bir kalıp "adet" sütununu taşırmaz, hizayı bozmaz.
///
/// `N`: sütun sayısı. Sabit bir genel (const generic) parametredir: her satırın tam `N` sütunu
/// olduğu derleyicice bilinir.
fn sayi_sutunlari<const N: usize>(
    basliklar: [&str; N],
    satirlar: &[[String; N]],
) -> (String, Vec<String>) {
    let genislikler: [usize; N] = std::array::from_fn(|sutun| {
        satirlar
            .iter()
            .map(|satir| satir[sutun].chars().count())
            .fold(basliklar[sutun].chars().count(), usize::max)
    });
    let yaz = |hucreler: [&str; N]| {
        let sutunlar: Vec<String> = hucreler
            .iter()
            .zip(genislikler)
            .map(|(hucre, genislik)| format!("{hucre:>genislik$}"))
            .collect();
        sutunlar.join("  ")
    };
    let sayilar = satirlar
        .iter()
        .map(|satir| yaz(satir.each_ref().map(String::as_str)))
        .collect();
    (yaz(basliklar), sayilar)
}

/// Tablonun başlık satırı. `sayilar`: satır başındaki sayı sütunlarının başlıkları.
fn baslik_yaz(cikti: &mut impl Write, sayilar: &str, gosterim: ZamanGosterimi) -> io::Result<()> {
    let zaman_genisligi = gosterim.genislik();
    writeln!(
        cikti,
        "  {sayilar}  {:<9}  {:<zaman_genisligi$}  {:<zaman_genisligi$}  kalıp",
        "seviye", "ilk", "son"
    )
}

/// Bir tablo satırının seviye, ilk, son ve örnek hücreleri. Kalıp ve karşılaştırma
/// tablolarında kalıp satırının kendisinden gelir; ani artış tablosunda artışın kayıtlarından.
/// Metin ve Markdown aynı hücreleri yazar.
pub struct Hucreler<'s> {
    pub seviye: Seviye,
    pub ilk: Zaman,
    pub son: Zaman,
    /// Maskelenmemiş bir mesaj: "Kullanıcı 381 giriş yaptı"
    pub ornek: &'s str,
}

impl<'s> Hucreler<'s> {
    /// Kalıp satırının kendi hücreleri.
    pub fn satirdan(satir: &'s KalipSatiri) -> Hucreler<'s> {
        Hucreler {
            seviye: satir.seviye,
            ilk: satir.ilk,
            son: satir.son,
            ornek: &satir.ornek,
        }
    }

    /// Ani artışın hücreleri: zamanlar artışın ilk ve son kaydınınkiler.
    pub fn artistan(artis: &'s AniArtis) -> Hucreler<'s> {
        Hucreler {
            seviye: artis.seviye,
            ilk: artis.ilk,
            son: artis.son,
            ornek: &artis.ornek,
        }
    }
}

/// Tablonun bir satırı ve altındaki açıklama, örnek ve konum satırları.
///
/// `sayilar`: satır başındaki sayı sütunları (`"   4"` ya da `"   0      2"`). Alttaki satırlar
/// kalıp sütununun hizasından başlar; hiza, sayı sütunlarının genişliğinden hesaplanır.
/// Kalıp ve konum `satir`dan, seviye, zamanlar ve örnek `hucreler`den gelir.
fn satir_yaz(
    cikti: &mut impl Write,
    sayilar: &str,
    satir: &KalipSatiri,
    hucreler: &Hucreler<'_>,
    aciklama: Option<&str>,
    gosterim: ZamanGosterimi,
) -> io::Result<()> {
    let zaman_genisligi = gosterim.genislik();
    let girinti = " ".repeat(
        2 + sayilar.chars().count() + 2 + 9 + 2 + zaman_genisligi + 2 + zaman_genisligi + 2,
    );
    writeln!(
        cikti,
        "  {sayilar}  {:<9}  {:<zaman_genisligi$}  {:<zaman_genisligi$}  {}",
        hucreler.seviye.ad(),
        zaman_goster(hucreler.ilk, gosterim),
        zaman_goster(hucreler.son, gosterim),
        // Önce temizle, sonra kısalt: genişlik ekranda görünen karakterlerle sayılsın
        kisalt(&gorunur(&satir.kalip), KALIP_GENISLIGI)
    )?;
    if let Some(aciklama) = aciklama {
        writeln!(cikti, "{girinti}artış: {aciklama}")?;
    }
    // Kalıp maskelenmiş ya da <*> içeriyorsa arkasındaki gerçek değerleri göster
    if hucreler.ornek != satir.kalip {
        writeln!(
            cikti,
            "{girinti}örnek: {}",
            kisalt(&gorunur(hucreler.ornek), KALIP_GENISLIGI - 7)
        )?;
    }
    // <*> konumlarının en sık değerleri: hangi işler, olaylar, kullanıcılar. Liste kalıp
    // genişliğine sığmıyorsa değerler bölünmeden alt satırlara taşar.
    const DEGERLER: &str = "değerler: ";
    let satirlar = degerler_satirlari(
        &satir.degiskenler,
        KALIP_GENISLIGI - DEGERLER.chars().count(),
    );
    for (sira, parca) in satirlar.iter().enumerate() {
        if sira == 0 {
            writeln!(cikti, "{girinti}{DEGERLER}{parca}")?;
        } else {
            writeln!(cikti, "{girinti}{:DEGERLER_GENISLIGI$}{parca}", "")?;
        }
    }
    // Kayıtlar bir istisnadan geliyorsa nerede oluştuğunu göster. Dosya yolu kısaltılmaz:
    // terminalde tıklanıp doğrudan açılabilsin.
    if let Some(parmak_izi) = &satir.parmak_izi {
        writeln!(
            cikti,
            "{girinti}konum: {}",
            gorunur(&parmak_izi.to_string())
        )?;
    }
    Ok(())
}

/// `<*>` konumlarının değerleri tek satırda: `giriş ×6, çıkış ×3, +2 başka`. Kalıpta birden
/// çok `<*>` varsa her konum sırasıyla: `2: ahmet ×4, veli ×1; 4: giriş ×5`. `bicimle`: her
/// değerin yazılışı (Markdown'da kod olarak).
pub fn degerler_metni(degiskenler: &[DegiskenKonum], bicimle: impl Fn(&str) -> String) -> String {
    deger_parcalari(degiskenler, bicimle).concat()
}

/// Değerler, her biri en fazla `genislik` karakterlik satırlara bölünmüş. Bir değer satır
/// ortasında bölünmez: sığmayan değer sonraki satıra geçer, ayracı önceki satırın sonunda
/// kalır. Tek başına sığmayan bir değer kısaltılır. Kontrol karakterleri sembole çevrilir.
pub fn degerler_satirlari(degiskenler: &[DegiskenKonum], genislik: usize) -> Vec<String> {
    let mut satirlar: Vec<String> = Vec::new();
    let mut satir = String::new();
    for parca in deger_parcalari(degiskenler, |deger| gorunur(deger).into_owned()) {
        let parca = parca.trim_end();
        let parca = kisalt(parca, genislik);
        let uzunluk = satir.chars().count();
        if uzunluk > 0 && uzunluk + 1 + parca.chars().count() > genislik {
            satirlar.push(std::mem::take(&mut satir));
        }
        if !satir.is_empty() {
            satir.push(' ');
        }
        satir.push_str(&parca);
    }
    if !satir.is_empty() {
        satirlar.push(satir);
    }
    satirlar
}

/// Değerlerin parçaları, sırayla: her parça bir değer adediyle (`giriş ×6`) ya da `+N başka`,
/// sonunda ayracı (`, `; konumun son parçasında `; `; en sonda hiç). Birden çok konumda her
/// konumun ilk parçası konumun sırasıyla başlar (`2: ahmet ×4`). `bicimle`: her değerin
/// yazılışı (Markdown'da kod olarak).
fn deger_parcalari(degiskenler: &[DegiskenKonum], bicimle: impl Fn(&str) -> String) -> Vec<String> {
    let mut parcalar = Vec::new();
    for (konum_sirasi, degisken) in degiskenler.iter().enumerate() {
        let mut konumun: Vec<String> = degisken
            .degerler
            .iter()
            .map(|(deger, adet)| format!("{} ×{adet}", bicimle(deger)))
            .collect();
        if degisken.baska > 0 {
            konumun.push(format!("+{} başka", degisken.baska));
        }
        let son = konumun.len() - 1;
        for (sira, mut parca) in konumun.into_iter().enumerate() {
            if sira == 0 && degiskenler.len() > 1 {
                parca = format!("{}: {parca}", degisken.konum);
            }
            if sira < son {
                parca.push_str(", ");
            } else if konum_sirasi + 1 < degiskenler.len() {
                parca.push_str("; ");
            }
            parcalar.push(parca);
        }
    }
    parcalar
}

/// Gösterilmeyen satırlar varsa söyler; yoksa kullanıcı tablonun bittiğini sanır.
fn gizlenen_yaz(cikti: &mut impl Write, gizlenen: usize) -> io::Result<()> {
    if gizlenen > 0 {
        writeln!(
            cikti,
            "  … {gizlenen} satır daha; tümünü görmek için --hepsi"
        )?;
    }
    Ok(())
}

/// Ondalık sayıyı Türkçedeki gibi virgülle yazar: 0.7 → "0,7"
pub fn ondalik(sayi: f64) -> String {
    sayi.to_string().replace('.', ",")
}

/// Ondalık sayıyı virgülden sonra tek basamakla yazar: 15.43 → "15,4"
///
/// 0,05'ten küçük (sıfır olmayan) sayılar tek basamakta "0,0" görünürdü; onlar iki anlamlı
/// basamakla yazılır: 0.0483 → "0,048". Seyrek bir logda saatlik sıklık böyle küçük olur.
pub fn ondalik_kisa(sayi: f64) -> String {
    let basamak = if sayi != 0.0 && sayi.abs() < 0.05 {
        // log10(0,0483) = −1,3 → aşağı yuvarlanınca −2 → virgülden sonra 3 basamak
        let ust = sayi.abs().log10().floor();
        (1.0 - ust).min(12.0) as usize
    } else {
        1
    };
    format!("{sayi:.basamak$}").replace('.', ",")
}

/// Tabloda zamanın nasıl gösterileceği; logun kapsadığı süreye göre seçilir
/// (`Ozet::zaman_gosterimi`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZamanGosterimi {
    /// Bütün kayıtlar aynı günde: "14:03:11"
    Saat,
    /// Aynı yılın birden çok gününde: "10-24 14:03:11"
    AyGun,
    /// Birden çok yılda: "2025-07-23 14:03:11". Yıl yazılmasa 2025'in Temmuz'u 2026'nın
    /// Ağustos'undan sonra gelmiş gibi görünebilirdi.
    Tarih,
}

impl ZamanGosterimi {
    /// Gösterilen zamanın karakter sayısı; tablonun sütun genişliği.
    pub fn genislik(self) -> usize {
        match self {
            ZamanGosterimi::Saat => 8,
            ZamanGosterimi::AyGun => 14,
            ZamanGosterimi::Tarih => 19,
        }
    }
}

/// Zamanı tabloda gösterilecek hale getirir: "14:03:11", "10-24 14:03:11" ya da
/// "2025-07-23 14:03:11".
///
/// `Zaman`'ın metni hep 19 ASCII karakterdir (`2025-07-23 14:03:11`); bayt konumlarıyla
/// dilimlemek güvenlidir.
pub fn zaman_goster(zaman: Zaman, gosterim: ZamanGosterimi) -> String {
    let metin = zaman.to_string();
    match gosterim {
        ZamanGosterimi::Saat => metin[11..].to_string(),
        ZamanGosterimi::AyGun => metin[5..].to_string(),
        ZamanGosterimi::Tarih => metin,
    }
}

/// Metni en fazla `sinir` karaktere indirir; kısaltırsa sonuna "…" koyar.
///
/// Bayt değil karakter sayılır: "ş" gibi Türkçe harfler UTF-8'de 2 bayttır ve bir harfi
/// ortasından kesmeye çalışmak programı çökertir.
pub fn kisalt(metin: &str, sinir: usize) -> String {
    if metin.chars().count() <= sinir {
        return metin.to_string();
    }
    let kisa: String = metin.chars().take(sinir - 1).collect();
    format!("{kisa}…")
}

/// Metindeki kontrol karakterlerini görünür sembollere çevirir: ESC → ␛, CR → ␍, sekme → ␉
///
/// Terminal, ESC (U+001B) ile başlayan dizileri yazı olarak değil komut olarak okur: renk
/// değiştirir, ekranı siler, imleci taşır, pencere başlığını değiştirir. Bir log mesajı bunları
/// içeriyorsa rapor, terminalde mesajı yazanın istediği gibi görünür: önceki satırlar silinip
/// bir hata gizlenebilir. CR (U+000D) imleci satır başına götürür, ardından gelen yazı öncekinin
/// üstüne yazılır; BS (U+0008) harf harf geri gider.
///
/// - U+0000–U+001F: Unicode'un "Control Pictures" bloğunda aynı sırayla dizilmiş sembolleri
///   vardır, U+2400'den başlar: sembol = U+2400 + kod. Bir karakter yine tek karakter olur.
/// - U+007F (DEL): ␡
/// - U+0080–U+009F (C1 kontrol karakterleri): sembolleri yoktur, `\u{9b}` diye yazılır. VTE
///   tabanlı terminaller, kitty ve WezTerm bunları da komut olarak okur.
///
/// Metinde kontrol karakteri yoksa (neredeyse her zaman) kopya oluşturulmaz: `Cow::Borrowed`.
pub fn gorunur(metin: &str) -> Cow<'_, str> {
    if !metin.chars().any(char::is_control) {
        return Cow::Borrowed(metin);
    }
    let mut sonuc = String::with_capacity(metin.len());
    for harf in metin.chars() {
        match harf {
            '\u{0}'..='\u{1f}' => {
                let sembol = char::from_u32(0x2400 + u32::from(harf))
                    .expect("U+2400–U+241F arası geçerli karakterlerdir");
                sonuc.push(sembol);
            }
            '\u{7f}' => sonuc.push('␡'),
            // Geriye kalan kontrol karakterleri C1'dir
            kontrol if kontrol.is_control() => sonuc.extend(kontrol.escape_unicode()),
            diger => sonuc.push(diger),
        }
    }
    Cow::Owned(sonuc)
}

#[cfg(test)]
mod testler {
    use super::*;
    use crate::ani_artis::{deneme_logu, karisik_seviyeli_log};
    use crate::depo::{self, DepoAyarlari};
    use crate::okuyucu::YenidenOkuyucu;
    use std::io::Cursor;

    const ORNEK_LOG: &str = include_str!("../ornekler/laravel.log");

    /// Kontrol karakterleriyle dolu bir log: satır başı (CR), ekran silme ve renk (ESC), geri
    /// silme (BS), C1 CSI (U+009B) ve yığın izindeki bir yolda pencere başlığı dizisi (ESC ] … BEL).
    const KONTROLLU_LOG: &str = "\
[2026-10-24 10:00:00] local.ERROR: Odeme tamam\rHATA: kart reddedildi
[2026-10-24 10:00:01] local.INFO: \u{1b}[2J\u{1b}[31mEkran temizlendi\u{1b}[0m
[2026-10-24 10:00:02] local.WARNING: Gizli\u{8}\u{8}\u{8}\u{8}\u{8}Acik ve \u{9b}31m C1
[2026-10-24 10:00:03] local.ERROR: Bozuk yol bulundu {\"exception\":\"[object] (RuntimeException(code: 0): Bozuk yol bulundu at /var/www/app/K\u{1b}]0;baslik\u{7}otu.php:5)\"}
";

    #[test]
    fn kisa_metin_oldugu_gibi_kalir() {
        assert_eq!(kisalt("Şüpheli giriş", 20), "Şüpheli giriş");
    }

    #[test]
    fn uzun_metin_karakterden_kesilir() {
        // 5 karakter: 4 harf + "…". "Ş" ve "ü" 2'şer bayt olsa da harf bölünmez.
        assert_eq!(kisalt("Şüpheli giriş", 5), "Şüph…");
    }

    #[test]
    fn kontrol_karakterleri_sembole_cevrilir() {
        assert_eq!(
            gorunur("Ödeme\rHATA\t\u{1b}[31mkırmızı\u{8}\u{0}"),
            "Ödeme␍HATA␉␛[31mkırmızı␈␀"
        );
        // Sınırlar: U+001F son C0 karakteri, boşluk ve ~ kontrol değil, U+007F DEL,
        // U+0080–U+009F C1, U+00A0 (bölünmez boşluk) kontrol değil
        assert_eq!(
            gorunur("\u{1f} ~\u{7f}\u{80}\u{9f}\u{a0}"),
            "␟ ~␡\\u{80}\\u{9f}\u{a0}"
        );
        // Kontrol karakteri yoksa kopya oluşturulmaz
        assert!(matches!(
            gorunur("Şüpheli giriş"),
            Cow::Borrowed("Şüpheli giriş")
        ));
        assert!(matches!(gorunur(""), Cow::Borrowed("")));
    }

    #[test]
    fn metin_raporu_terminale_kontrol_karakteri_gondermez() {
        let ozet = Ozet::metinden(
            "kötü\u{1b}[2J.log",
            KONTROLLU_LOG,
            &RaporAyarlari::default(),
        )
        .unwrap();
        let mut cikti = Vec::new();
        metin_yaz(&mut cikti, &ozet).unwrap();
        let metin = String::from_utf8(cikti).unwrap();

        // Satır sonları dışında tek bir kontrol karakteri bile kalmamalı
        let kalanlar: Vec<char> = metin
            .chars()
            .filter(|&harf| harf.is_control() && harf != '\n')
            .collect();
        assert!(kalanlar.is_empty(), "{kalanlar:?}");

        // Her alan ayrı ayrı: dosya adı, örnek, kalıp, konum
        assert!(metin.starts_with("kötü␛[2J.log: 4 satır\n"), "{metin}");
        assert!(
            metin.contains("örnek: Odeme tamam␍HATA: kart reddedildi\n"),
            "{metin}"
        );
        assert!(
            metin.contains("  ␛[2J␛[31mEkran temizlendi␛[0m\n"),
            "{metin}"
        );
        assert!(
            metin.contains("  Gizli␈␈␈␈␈Acik ve \\u{9b}31m C1\n"),
            "{metin}"
        );
        assert!(
            metin.contains("konum: /var/www/app/K␛]0;baslik␇otu.php:5 (RuntimeException)\n"),
            "{metin}"
        );
    }

    #[test]
    fn genislik_temizlenmis_metinle_sayilir() {
        // 79 harf ve bir C1 karakteri: ham hali tam 80 karakter, sığar. Temizlenmiş hali 85
        // karakter (`\u{9b}`), kısaltılmalı. Önce kısaltıp sonra temizlemek satırı taşırırdı.
        let metin = format!("[2026-10-24 10:00:00] local.INFO: {}\u{9b}", "a".repeat(79));
        let rapor = metin_raporu(&metin, &RaporAyarlari::default());
        assert!(
            rapor.contains(&format!("  {}…\n", "a".repeat(79))),
            "{rapor}"
        );
    }

    #[test]
    fn cok_gunlu_logda_tarih_de_gosterilir() {
        // Aynı ayın iki ayrı günü: yalnızca ay değil gün de karşılaştırılmalı
        let metin = "[2026-10-24 23:59:59] local.ERROR: Gece yarısı hatası\n\
                     [2026-10-25 00:00:01] local.ERROR: Gece yarısı hatası";
        let ozet = Ozet::metinden("a.log", metin, &RaporAyarlari::default()).unwrap();
        assert_eq!(ozet.zaman_gosterimi(), ZamanGosterimi::AyGun);

        let mut cikti = Vec::new();
        metin_yaz(&mut cikti, &ozet).unwrap();
        let rapor = String::from_utf8(cikti).unwrap();
        assert!(
            rapor.contains(
                "\n     2  ERROR      10-24 23:59:59  10-25 00:00:01  Gece yarısı hatası\n"
            ),
            "{rapor}"
        );
    }

    #[test]
    fn karsilastirma_bolumu_yazilir() {
        let ayarlar = RaporAyarlari {
            karsilastir: ZamanNoktasi::ayristir("16:11"),
            ilk: Some(2),
            ..RaporAyarlari::default()
        };
        let metin = metin_raporu(ORNEK_LOG, &ayarlar);

        // Normal rapor değişmeden başta, karşılaştırma sonda. --ilk her bölüme ayrı uygulanır.
        // Yalnızca artanın altında sıklığın değişimi yazılır; açıklama satırları kalıp
        // sütununun hizasında başlar.
        let beklenen = r#"
  … 22 satır daha; tümünü görmek için --hepsi

Karşılaştırma: 2026-10-24 16:11:00 öncesi ve sonrası
  önce  : 08:59:58 → 16:11:00   7 sa 11 dk    39 kayıt, saatte 5,4
  sonra : 16:11:00 → 18:31:07   2 sa 20 dk    20 kayıt, saatte 8,6

Yeni çıkanlar (yalnızca sonra): 3
  önce  sonra  seviye     ilk       son       kalıp
     0      2  CRITICAL   17:05:13  17:06:45  Redis bağlantısı kurulamadı: Connection refused [tcp://<IP>:<SAYI>]
                                              örnek: Redis bağlantısı kurulamadı: Connection refused [tcp://10.0.3.12:6379]
                                              konum: app/Http/Middleware/HizSiniri.php:22, handle() (RedisException)
     0      1  INFO       16:24:10  16:24:10  Ödeme sağlayıcısı yeniden yanıt veriyor
  … 1 satır daha; tümünü görmek için --hepsi

Kaybolanlar (yalnızca önce): 11
  önce  sonra  seviye     ilk       son       kalıp
     1      0  ERROR      09:47:12  09:47:12  Attempt to read property "ad" on null
                                              konum: app/Http/Controllers/KategoriController.php:54, goster() (ErrorException)
     1      0  ERROR      11:30:44  11:30:44  App\Jobs\FaturaGonder has been attempted too many times.
                                              konum: uygulama kodunda bulunamadı (MaxAttemptsExceededException)
  … 9 satır daha; tümünü görmek için --hepsi

Artanlar (sonra ≥ 2 kat sık, tesadüf ihtimali < %1): 1
  önce  sonra  seviye     ilk       son       kalıp
     1      5  ERROR      16:10:02  16:16:02  cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> by…
                                              artış: saatte 0,1 → 2,1 (15,4 kat)
                                              örnek: cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes…
                                              konum: app/Services/OdemeServisi.php:64, tahsilEt() (ConnectionException)
"#;
        assert!(metin.ends_with(beklenen), "{metin}");
    }

    #[test]
    fn bos_bolum_ve_cok_gunlu_karsilastirma() {
        let ayarlar = RaporAyarlari {
            karsilastir: ZamanNoktasi::ayristir("15:00"),
            ..RaporAyarlari::default()
        };
        assert!(
            metin_raporu(ORNEK_LOG, &ayarlar)
                .ends_with("\nArtanlar (sonra ≥ 2 kat sık, tesadüf ihtimali < %1): yok\n")
        );

        // İki güne yayılan logda zamanlar tarihle yazılır
        let metin = "[2026-10-24 23:00:00] local.ERROR: Gece hatası\n\
                     [2026-10-25 01:00:00] local.ERROR: Sabah hatası";
        let ayarlar = RaporAyarlari {
            karsilastir: ZamanNoktasi::ayristir("00:30"),
            ..RaporAyarlari::default()
        };
        let ozet = Ozet::metinden("a.log", metin, &ayarlar).unwrap();
        let mut cikti = Vec::new();
        metin_yaz(&mut cikti, &ozet).unwrap();
        let rapor = String::from_utf8(cikti).unwrap();
        assert!(
            rapor.contains(
                "\n  önce  : 10-24 23:00:00 → 10-25 00:30:00   1 sa 30 dk     1 kayıt, saatte 0,7\n  sonra : 10-25 00:30:00 → 10-25 01:00:00   30 dk 1 sn     1 kayıt, saatte 2,0\n"
            ),
            "{rapor}"
        );
    }

    #[test]
    fn ani_artis_satirinda_seviye_ve_ornek_artistan() {
        // Kalıp tablosunda satırın seviyesi (CRITICAL) ve örneği; artış tablosunda artışın
        let metin = karisik_seviyeli_log();
        let ayarlar = RaporAyarlari {
            ani_artis: Some(1),
            ..RaporAyarlari::default()
        };
        let ozet = Ozet::metinden("a.log", &metin, &ayarlar).unwrap();
        let mut cikti = Vec::new();
        metin_yaz(&mut cikti, &ozet).unwrap();
        let metin = String::from_utf8(cikti).unwrap();

        let (kaliplar, artislar) = metin.split_once("Ani artışlar").unwrap();
        assert!(
            kaliplar.contains("CRITICAL   10:00:30  11:50:30  Kuyruk <SAYI> dolu\n"),
            "{kaliplar}"
        );
        assert!(kaliplar.contains("örnek: Kuyruk 2 dolu\n"), "{kaliplar}");
        assert!(
            artislar.contains("ERROR      11:00:00  11:01:15  Kuyruk <SAYI> dolu\n"),
            "{artislar}"
        );
        assert!(artislar.contains("örnek: Kuyruk 3 dolu\n"), "{artislar}");
    }

    #[test]
    fn ani_artis_bolumu_yazilir() {
        let ayarlar = RaporAyarlari {
            ani_artis: Some(10),
            ilk: Some(2),
            ..RaporAyarlari::default()
        };
        let metin = metin_raporu(ORNEK_LOG, &ayarlar);

        // Normal rapor değişmeden başta, ani artışlar sonda. İlk ve son sütunları artışın ilk
        // ve son kaydıdır; açıklama satırı dilimleri söyler.
        let beklenen = r#"
  … 22 satır daha; tümünü görmek için --hepsi

Ani artışlar (WARNING ve üstü, 10 dakikalık 59 dilim, puan > 3,5): 1
  zirve  olağan  puan  seviye     ilk       son       kalıp
      6       0   4,0  ERROR      16:10:02  16:16:02  cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> by…
                                                      artış: 16:10–16:20 arası 6 kayıt
                                                      örnek: cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes…
                                                      konum: app/Services/OdemeServisi.php:64, tahsilEt() (ConnectionException)
"#;
        assert!(metin.ends_with(beklenen), "{metin}");
    }

    #[test]
    fn ani_artislar_puana_gore_ve_ilk_kadar() {
        let metin = deneme_logu();
        let ayarlar = RaporAyarlari {
            ani_artis: Some(1),
            ilk: Some(2),
            ..RaporAyarlari::default()
        };
        let ozet = Ozet::metinden("a.log", &metin, &ayarlar).unwrap();
        let mut cikti = Vec::new();
        metin_yaz(&mut cikti, &ozet).unwrap();
        let rapor = String::from_utf8(cikti).unwrap();

        // Üç artışın puanı en yüksek ikisi. İlk ve son, kalıbın değil artışın kayıtları
        // (kuyruk kalıbının ilk kaydı 10:05:30); olağan, dilim başına medyan.
        let beklenen = r#"
Ani artışlar (WARNING ve üstü, 1 dakikalık 120 dilim, puan > 3,5): 3
  zirve  olağan  puan  seviye     ilk       son       kalıp
      9       0   6,1  ERROR      10:30:30  10:32:36  Kuyruk dolu
                                                      artış: 10:30–10:33 arası 3 dilimde 22 kayıt, zirve 10:31–10:32
      9       2   4,7  WARNING    10:45:10  10:45:18  Yavaş istek
                                                      artış: 10:45–10:46 arası 9 kayıt
  … 1 satır daha; tümünü görmek için --hepsi
"#;
        assert!(rapor.ends_with(beklenen), "{rapor}");
    }

    #[test]
    fn ani_artis_basligi_durumu_soyler() {
        for (dakika, beklenen) in [
            (
                1,
                "\nAni artışlar (WARNING ve üstü, 1 dakikalık 573 dilim, puan > 3,5): yok\n",
            ),
            (
                120,
                "\nAni artışlar (WARNING ve üstü, 120 dakikalık 6 dilim): olağan seviyeyi ölçmek için en az 10 dilim gerekir\n",
            ),
        ] {
            let ayarlar = RaporAyarlari {
                ani_artis: Some(dakika),
                ..RaporAyarlari::default()
            };
            let metin = metin_raporu(ORNEK_LOG, &ayarlar);
            assert!(metin.ends_with(beklenen), "{metin}");
        }

        let ayarlar = RaporAyarlari {
            ani_artis: Some(1),
            ..RaporAyarlari::default()
        };
        let ozet = Ozet::metinden("bos.log", "", &ayarlar).unwrap();
        let mut cikti = Vec::new();
        metin_yaz(&mut cikti, &ozet).unwrap();
        let metin = String::from_utf8(cikti).unwrap();
        assert!(
            metin.ends_with(
                "\nAni artışlar (WARNING ve üstü, 1 dakikalık dilimler): logda kayıt yok\n"
            ),
            "{metin}"
        );
    }

    #[test]
    fn karsilastirma_ve_ani_artis_birlikte() {
        // İki bölüm de istenirse karşılaştırma önce, ani artışlar sonra; --ilk ikisine de ayrı
        let ayarlar = RaporAyarlari {
            karsilastir: ZamanNoktasi::ayristir("16:11"),
            ani_artis: Some(10),
            ilk: Some(1),
            ..RaporAyarlari::default()
        };
        let metin = metin_raporu(ORNEK_LOG, &ayarlar);
        let karsilastirma = metin.find("\nKarşılaştırma: ").unwrap();
        let ani_artis = metin.find("\nAni artışlar (").unwrap();
        assert!(karsilastirma < ani_artis, "{metin}");
        assert!(metin[ani_artis..].contains("cURL error <SAYI>"), "{metin}");
    }

    #[test]
    fn cok_gunlu_ani_artis_tarihle_yazilir() {
        // 23:59 ve 00:00 dilimlerinde 8'er kayıt: gece yarısını aşan tek artış
        let mut satirlar = vec!["[2026-10-24 22:00:00] local.INFO: Başladı".to_string()];
        for (dakika, saat) in [(59, "23"), (0, "00")] {
            let gun = if saat == "23" { 24 } else { 25 };
            for saniye in 0..8 {
                satirlar.push(format!(
                    "[2026-10-{gun} {saat}:{dakika:02}:{saniye:02}] local.ERROR: Kuyruk dolu"
                ));
            }
        }
        satirlar.push("[2026-10-25 02:00:00] local.INFO: Başladı".to_string());
        let metin = satirlar.join("\n");
        let ayarlar = RaporAyarlari {
            ani_artis: Some(1),
            ..RaporAyarlari::default()
        };
        let ozet = Ozet::metinden("a.log", &metin, &ayarlar).unwrap();
        let mut cikti = Vec::new();
        metin_yaz(&mut cikti, &ozet).unwrap();
        let rapor = String::from_utf8(cikti).unwrap();
        let beklenen = r#"
Ani artışlar (WARNING ve üstü, 1 dakikalık 241 dilim, puan > 3,5): 1
  zirve  olağan  puan  seviye     ilk             son             kalıp
      8       0   5,4  ERROR      10-24 23:59:00  10-25 00:00:07  Kuyruk dolu
                                                                  artış: 10-24 23:59–10-25 00:01 arası 2 dilimde 16 kayıt, zirve 10-24 23:59–10-25 00:00
"#;
        assert!(rapor.ends_with(beklenen), "{rapor}");
    }

    #[test]
    fn dilim_araligi_ve_ani_artis_aciklamasi() {
        let zaman = Zaman::kayittan;
        let (bas, son) = (zaman("2026-10-24 16:10:00"), zaman("2026-10-24 16:20:00"));
        assert_eq!(dilim_araligi(bas, son, ZamanGosterimi::Saat), "16:10–16:20");
        assert_eq!(
            dilim_araligi(bas, son, ZamanGosterimi::AyGun),
            "10-24 16:10–16:20"
        );
        assert_eq!(
            dilim_araligi(bas, son, ZamanGosterimi::Tarih),
            "2026-10-24 16:10–16:20"
        );
        let (gece_bas, gece_son) = (zaman("2026-12-31 23:50:00"), zaman("2027-01-01 00:10:00"));
        assert_eq!(
            dilim_araligi(gece_bas, gece_son, ZamanGosterimi::AyGun),
            "12-31 23:50–01-01 00:10"
        );
        assert_eq!(
            dilim_araligi(gece_bas, gece_son, ZamanGosterimi::Tarih),
            "2026-12-31 23:50–2027-01-01 00:10"
        );

        let mut artis = AniArtis {
            satir: 0,
            seviye: Seviye::Error,
            ornek: "cURL error 28".to_string(),
            olagan: 0.0,
            mad: 0.0,
            baslangic: bas,
            bitis: son,
            dilim_sayisi: 1,
            kayit_sayisi: 6,
            ilk: zaman("2026-10-24 16:10:02"),
            son: zaman("2026-10-24 16:16:02"),
            zirve_baslangic: bas,
            zirve_bitis: son,
            zirve_adet: 6,
            puan: 4.047,
        };
        assert_eq!(
            ani_artis_aciklamasi(&artis, ZamanGosterimi::Saat),
            "16:10–16:20 arası 6 kayıt"
        );
        // Birden çok dilim: zirve de yazılır
        artis.bitis = zaman("2026-10-24 16:40:00");
        artis.dilim_sayisi = 3;
        artis.kayit_sayisi = 25;
        artis.zirve_baslangic = zaman("2026-10-24 16:20:00");
        artis.zirve_bitis = zaman("2026-10-24 16:30:00");
        assert_eq!(
            ani_artis_aciklamasi(&artis, ZamanGosterimi::Saat),
            "16:10–16:40 arası 3 dilimde 25 kayıt, zirve 16:20–16:30"
        );
    }

    #[test]
    fn ondalik_virgulle_yazilir() {
        assert_eq!(ondalik(0.7), "0,7");
        assert_eq!(ondalik(0.85), "0,85");
        assert_eq!(ondalik(1.0), "1");
        assert_eq!(ondalik_kisa(15.379), "15,4");
        assert_eq!(ondalik_kisa(0.0), "0,0");
        assert_eq!(ondalik_kisa(0.05), "0,1");
        // 0,05'ten küçükler iki anlamlı basamakla: seyrek bir logda saatlik sıklık
        assert_eq!(ondalik_kisa(0.0483), "0,048");
        assert_eq!(ondalik_kisa(0.00214), "0,0021");
        assert_eq!(ondalik_kisa(0.0099), "0,0099");
    }

    #[test]
    fn sayi_sutunlari_en_genis_degere_gore_hizalanir() {
        // Başlık daha genişse değerler başlığa göre sağa yaslanır
        assert_eq!(
            sayi_sutunlari(["önce", "sonra"], &[["1".into(), "22".into()]]),
            ("önce  sonra".to_string(), vec!["   1     22".to_string()])
        );
        // Değer daha genişse başlık da ona göre
        assert_eq!(
            sayi_sutunlari(["adet"], &[["12345".into()], ["7".into()]]),
            (
                " adet".to_string(),
                vec!["12345".to_string(), "    7".to_string()]
            )
        );
    }

    #[test]
    fn buyuk_adetler_tabloyu_kaydirmaz() {
        let metin = vec!["[2026-10-24 10:00:00] local.ERROR: Kuyruk dolu"; 12_345].join("\n");
        let ozet = Ozet::metinden("a.log", &metin, &RaporAyarlari::default()).unwrap();
        let mut cikti = Vec::new();
        metin_yaz(&mut cikti, &ozet).unwrap();
        let rapor = String::from_utf8(cikti).unwrap();
        assert!(
            rapor.ends_with(
                "\n   adet  seviye     ilk       son       kalıp\n  12345  ERROR      10:00:00  10:00:00  Kuyruk dolu\n"
            ),
            "{rapor}"
        );
        assert!(
            rapor.contains("\nSeviyeler:\n  ERROR     12345\n"),
            "{rapor}"
        );
    }

    #[test]
    fn karisik_zaman_bicimlerinde_ilk_son_ve_siralama_dogru() {
        // Aynı günde metin ve JSON kayıtları: metin olarak karşılaştırılsa " " < "T" olduğu
        // için metin biçimindeki her kayıt JSON'dakilerden eski sayılırdı. Raporun saat dilimi
        // JSON'daki +00:00; dilimsiz kayıtlar bu dilimde sayılır.
        let metin = [
            "[2026-10-24 10:00:00] local.ERROR: Önbellek dolu",
            r#"{"message":"Önbellek dolu","context":{},"level_name":"ERROR","datetime":"2026-10-24T09:00:00+00:00"}"#,
            r#"{"message":"Kuyruk durdu","context":{},"level_name":"ERROR","datetime":"2026-10-24T11:00:00+00:00"}"#,
            "[2026-10-24 09:30:00] local.ERROR: Disk doldu",
        ]
        .join("\n");
        let ayarlar = RaporAyarlari {
            siralama: Siralama::Yeni,
            ..RaporAyarlari::default()
        };
        let analiz = kaliplari_cikar(&depo::metinden(&metin), &ayarlar);
        let satirlar: Vec<(&str, String, String)> = analiz
            .satirlar
            .iter()
            .map(|satir| {
                (
                    satir.kalip.as_str(),
                    satir.ilk.to_string(),
                    satir.son.to_string(),
                )
            })
            .collect();
        let satir = |kalip, ilk: &str, son: &str| (kalip, ilk.to_string(), son.to_string());
        assert_eq!(
            satirlar,
            [
                satir("Kuyruk durdu", "2026-10-24 11:00:00", "2026-10-24 11:00:00"),
                satir("Disk doldu", "2026-10-24 09:30:00", "2026-10-24 09:30:00"),
                satir(
                    "Önbellek dolu",
                    "2026-10-24 09:00:00",
                    "2026-10-24 10:00:00"
                ),
            ]
        );
    }

    #[test]
    fn saat_dilimi_aciklamasi() {
        let ozet = |verildi, cevrilen, dilimler: &[&str], yazilmamis, kayit_sayisi| {
            SaatDilimiOzeti {
                dilim: SaatDilimi::ayristir("+03:00").unwrap(),
                verildi,
                sayilar: DilimSayilari {
                    cevrilen,
                    cevrilen_dilimler: dilimler
                        .iter()
                        .map(|dilim| SaatDilimi::ayristir(dilim).unwrap())
                        .collect(),
                    yazilmamis,
                },
                kayit_sayisi,
            }
            .aciklama()
        };
        assert_eq!(ozet(false, 0, &[], 0, 5), "+03:00");
        assert_eq!(ozet(true, 0, &[], 0, 5), "+03:00 (--saat-dilimi)");
        assert_eq!(
            ozet(false, 420, &["+02:00"], 0, 500),
            "+03:00; 420 kayıt +02:00 diliminden çevrildi"
        );
        // Dilimler küçükten büyüğe; üçten fazlası sayılır
        assert_eq!(
            ozet(false, 7, &["+02:00", "+01:00"], 0, 9),
            "+03:00; 7 kayıt başka dilimlerden çevrildi (+01:00 ve +02:00)"
        );
        assert_eq!(
            ozet(false, 7, &["+04:00", "-05:00", "+00:00", "+01:00"], 0, 9),
            "+03:00; 7 kayıt başka dilimlerden çevrildi (-05:00, +00:00, +01:00 ve +04:00)"
        );
        assert_eq!(
            ozet(
                false,
                7,
                &["+04:00", "-05:00", "+00:00", "+02:00", "+01:00"],
                0,
                9
            ),
            "+03:00; 7 kayıt başka dilimlerden çevrildi (-05:00, +00:00, +01:00 ve 2 dilim daha)"
        );
        assert_eq!(
            ozet(true, 0, &[], 5, 5),
            "+03:00 (--saat-dilimi); kayıtlarda saat dilimi yazılı değil, zamanlar logdaki gibi"
        );
        assert_eq!(
            ozet(false, 3, &["+02:00"], 2, 9),
            "+03:00; 3 kayıt +02:00 diliminden çevrildi; 2 kaydın saat dilimi yazılmamış, bu dilimde sayıldı"
        );
    }

    #[test]
    fn son_suresi_raporun_dilimindeki_en_yeni_kayittan() {
        // 12:00+03:00 en yeni gibi görünür ama 09:00+00:00'dır. En yeni kayıt 11:00; son bir
        // saat 10:00'dan başlar.
        let metin = "[2026-10-24T10:00:00+00:00] local.ERROR: Bir\n\
                     [2026-10-24T12:00:00+03:00] local.ERROR: İki\n\
                     [2026-10-24T11:00:00+00:00] local.ERROR: Üç";
        let ayarlar = RaporAyarlari {
            filtre: Filtre {
                son: Some(3_600),
                ..Filtre::default()
            },
            ..RaporAyarlari::default()
        };
        let ozet = Ozet::metinden("a.log", metin, &ayarlar).unwrap();
        let filtre = ozet.filtre.as_ref().unwrap();
        assert_eq!(
            filtre.baslangic,
            Some(Zaman::kayittan("2026-10-24 10:00:00"))
        );
        assert_eq!(filtre.kayit_sayisi, 2);
        // Dilim verilirse sınır da o dilimdedir
        let ayarlar = RaporAyarlari {
            saat_dilimi: SaatDilimi::ayristir("+03:00"),
            ..ayarlar
        };
        let ozet = Ozet::metinden("a.log", metin, &ayarlar).unwrap();
        let filtre = ozet.filtre.as_ref().unwrap();
        assert_eq!(
            filtre.baslangic,
            Some(Zaman::kayittan("2026-10-24 13:00:00"))
        );
        assert_eq!(filtre.kayit_sayisi, 2);
        assert_eq!(
            ozet.zaman_araligi,
            Some((
                Zaman::kayittan("2026-10-24 13:00:00"),
                Zaman::kayittan("2026-10-24 14:00:00")
            ))
        );
    }

    #[test]
    fn her_siralama_adindan_geri_bulunur() {
        for siralama in Siralama::HEPSI {
            assert_eq!(Siralama::ayristir(siralama.ad()), Some(siralama));
        }
        assert_eq!(Siralama::ayristir("rastgele"), None);
    }

    #[test]
    fn zaman_tek_gunde_saat_cok_gunde_tarihle_gosterilir() {
        let zaman = Zaman::kayittan;
        assert_eq!(
            zaman_goster(zaman("2026-10-24 14:03:11"), ZamanGosterimi::Saat),
            "14:03:11"
        );
        assert_eq!(
            zaman_goster(zaman("2026-10-24 14:03:11"), ZamanGosterimi::AyGun),
            "10-24 14:03:11"
        );
        assert_eq!(
            zaman_goster(zaman("2025-07-23 14:03:11"), ZamanGosterimi::Tarih),
            "2025-07-23 14:03:11"
        );
        assert_eq!(
            ZamanGosterimi::Tarih.genislik(),
            "2025-07-23 14:03:11".len()
        );
    }

    #[test]
    fn cok_yillik_logda_yil_da_gosterilir() {
        // Yıl dönümünde de (31 Aralık → 1 Ocak) yıl gösterilir: "12-31" ile "01-01" yanıltırdı
        for (metin, gosterim) in [
            (
                "[2025-07-23 08:37:53] local.ERROR: Eski\n[2026-08-06 10:23:59] local.ERROR: Yeni",
                ZamanGosterimi::Tarih,
            ),
            (
                "[2025-12-31 23:59:59] local.ERROR: a\n[2026-01-01 00:00:01] local.ERROR: b",
                ZamanGosterimi::Tarih,
            ),
            (
                "[2026-01-01 00:00:01] local.ERROR: a\n[2026-12-31 23:59:59] local.ERROR: b",
                ZamanGosterimi::AyGun,
            ),
        ] {
            let ozet = Ozet::metinden("a.log", metin, &RaporAyarlari::default()).unwrap();
            assert_eq!(ozet.zaman_gosterimi(), gosterim, "{metin}");
        }

        let metin = "[2025-07-23 08:37:53] local.ERROR: Göç başarısız\n\
                     [2026-08-06 10:23:59] local.ERROR: Göç başarısız";
        let ozet = Ozet::metinden("a.log", metin, &RaporAyarlari::default()).unwrap();
        let mut cikti = Vec::new();
        metin_yaz(&mut cikti, &ozet).unwrap();
        let rapor = String::from_utf8(cikti).unwrap();
        assert!(
            rapor.ends_with(
                "  adet  seviye     ilk                  son                  kalıp\n     2  ERROR      2025-07-23 08:37:53  2026-08-06 10:23:59  Göç başarısız\n"
            ),
            "{rapor}"
        );
    }

    #[test]
    fn ayni_kalipta_en_yuksek_seviye_ve_zaman_araligi_tutulur() {
        let metin = [
            "[2026-10-24 10:00:05] local.WARNING: Kuyruk dolu",
            "[2026-10-24 10:00:01] local.ERROR: Kuyruk dolu",
            "[2026-10-24 10:00:09] local.INFO: Kuyruk dolu",
        ]
        .join("\n");
        let analiz = kaliplari_cikar(&depo::metinden(&metin), &RaporAyarlari::default());

        assert_eq!(analiz.satirlar.len(), 1);
        let satir = &analiz.satirlar[0];
        assert_eq!(satir.adet, 3);
        assert_eq!(satir.seviye, Seviye::Error);
        assert_eq!(satir.ilk, Zaman::kayittan("2026-10-24 10:00:01"));
        assert_eq!(satir.son, Zaman::kayittan("2026-10-24 10:00:09"));
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

        fn sec<'a>(&mut self, secenekler: &[&'a str]) -> &'a str {
            secenekler[self.sayi(secenekler.len())]
        }
    }

    /// Bir kalıp satırının karşılaştırılan alanları: kalıp, adet, seviye, ilk, son, örnek ve
    /// konum.
    type Satir = (
        String,
        usize,
        Seviye,
        String,
        String,
        String,
        Option<String>,
    );

    /// `rastgele_log`'un yazdığı istisnalar: `app/Services`'teki sınıf (dosyanın adı da odur),
    /// istisnanın sınıfı, uygulama kodundaki satırın fonksiyonu ve satırları. Kod değişmiş gibi
    /// aynı hata iki satırda görünür.
    const ISTISNALAR: [(&str, &str, &str, [u32; 2]); 2] = [
        ("Sepet1", "RuntimeException", "olustur", [12, 15]),
        ("Sepet2", "LogicException", "iptal", [20, 24]),
    ];

    /// Başvuru tablosunda bir grup: küme, parmak izinin satırsız hali (dosya ve gerisi), en
    /// yeni kaydın zamanı ve satırı, tablo satırı.
    type BasvuruGrubu = (usize, Option<(String, String)>, (Zaman, u32), Satir);

    /// v0.1.0'ın kalıp tablosu (`rastgele_log`'un logları için): her kaydın maskeli mesajı
    /// logdaki sırayla, önbelleksiz Drain'den geçer; kayıtlar küme numarasına ve parmak izine
    /// göre gruplanır. Grubun ilk ve son zamanı zamanı en küçük ve en büyük olan ilk kayıttan,
    /// örneği ilk kaydından. Parmak izi satıra bağlı değildir; gösterilen satır en yeni
    /// kaydınkidir, aynı andakilerden logda sonra gelenin.
    fn kayit_kayit_tablo(metin: &str, ayarlar: &Ayarlar) -> Vec<Satir> {
        let (kayitlar, _) = crate::kayit::deneme::topla(metin);
        let maskeliler: Vec<String> = kayitlar
            .iter()
            .map(|kayit| crate::maske::maskele(&kayit.mesaj).into_owned())
            .collect();
        let (kumeler, numaralar) = crate::drain::kumele(ayarlar, &maskeliler);
        let mut gruplar: Vec<BasvuruGrubu> = Vec::new();
        for (kayit, kume) in kayitlar.iter().zip(numaralar) {
            let zaman = Zaman::kayittan(&kayit.zaman);
            // Yığın izindeki uygulama satırı: `#0 /var/www/magaza/app/Services/Sepet1.php(15): ...`
            let istisna = ISTISNALAR.iter().find_map(|&(sinif_adi, sinif, metot, _)| {
                let cerceve = format!("/app/Services/{sinif_adi}.php(");
                kayit.devam.iter().find_map(|satir| {
                    let (_, sonrasi) = satir.split_once(&cerceve)?;
                    let satir_no: u32 = sonrasi.split(')').next()?.parse().ok()?;
                    let dosya = format!("app/Services/{sinif_adi}.php");
                    Some(((dosya, format!(", {metot}() ({sinif})")), satir_no))
                })
            });
            let (kimlik, satir_no) = match istisna {
                Some((kimlik, satir_no)) => (Some(kimlik), satir_no),
                None => (None, 0),
            };
            match gruplar
                .iter_mut()
                .find(|(numara, grup_kimligi, _, _)| *numara == kume && *grup_kimligi == kimlik)
            {
                Some((_, _, en_yeni, satir)) => {
                    satir.1 += 1;
                    satir.2 = satir.2.max(kayit.seviye);
                    if zaman < Zaman::kayittan(&satir.3) {
                        satir.3 = kayit.zaman.clone();
                    }
                    if zaman > Zaman::kayittan(&satir.4) {
                        satir.4 = kayit.zaman.clone();
                    }
                    // Kayıtlar logdaki sırayla: aynı andaki kayıt daha yenidir
                    if zaman >= en_yeni.0 {
                        *en_yeni = (zaman, satir_no);
                    }
                }
                None => gruplar.push((
                    kume,
                    kimlik,
                    (zaman, satir_no),
                    (
                        kumeler[kume].metin(),
                        1,
                        kayit.seviye,
                        kayit.zaman.clone(),
                        kayit.zaman.clone(),
                        kayit.mesaj.clone(),
                        None,
                    ),
                )),
            }
        }
        let mut satirlar: Vec<Satir> = gruplar
            .into_iter()
            .map(|(_, kimlik, (_, satir_no), mut satir)| {
                satir.6 = kimlik.map(|(dosya, gerisi)| format!("{dosya}:{satir_no}{gerisi}"));
                satir
            })
            .collect();
        satirlar.sort();
        satirlar
    }

    /// Rastgele bir log. `bolunmeye_yatkin`: mesajların çoğu üç konumu yüzlerce farklı ad
    /// alan, sonu sayılı uzun mesajlardır; bazı ad üçlüleri sık tekrarlanır. İlk turda bu
    /// konumlar değişken bulunur, ikinci turda aynı mesajın kayıtları sıraya bağlı olarak farklı
    /// kümelere düşebilir: alt gruplar bölünür. Değilse birkaç türden karışık mesajlar.
    /// `istisnali`: uzun mesajların yarısı iki istisnadan birini (`ISTISNALAR`) taşır.
    fn rastgele_log(
        rastgele: &mut Rastgele,
        adet: usize,
        ad_sayisi: usize,
        bolunmeye_yatkin: bool,
        istisnali: bool,
    ) -> String {
        let a = [
            "güncellenemedi",
            "boşaltılamadı",
            "kaydedilemedi",
            "silinemedi",
        ];
        let b = ["web", "mobil", "api"];
        let c = ["tarayıcı", "uygulama"];
        let d = ["kapalı", "açık"];
        let ad = |sira: usize| -> String {
            [sira / 676, sira / 26, sira]
                .iter()
                .map(|basamak| char::from(b'a' + (basamak % 26) as u8))
                .collect()
        };
        let kucuk_havuz = 1 + rastgele.sayi(3);
        let mut satirlar = Vec::with_capacity(adet);
        let mut saniye = 36_000;
        for _ in 0..adet {
            saniye += rastgele.sayi(4) as i64;
            let tur = if bolunmeye_yatkin {
                if rastgele.sayi(5) == 0 { 2 } else { 4 }
            } else {
                rastgele.sayi(5)
            };
            let mesaj = match tur {
                0 => format!(
                    "Sepet {} kullanıcı {} oturumu {} {}",
                    rastgele.sec(&a),
                    rastgele.sayi(999),
                    rastgele.sec(&d),
                    rastgele.sec(&b)
                ),
                1 => format!("Bilinmeyen kullanıcı: {}", ad(rastgele.sayi(ad_sayisi))),
                2 => format!("Kuyruk {} dolu", rastgele.sayi(50)),
                3 => format!(
                    "Ödeme {} {} {} reddedildi",
                    rastgele.sec(&b),
                    rastgele.sec(&d),
                    ad(rastgele.sayi(30))
                ),
                _ => {
                    let havuz = if rastgele.sayi(3) == 0 {
                        kucuk_havuz
                    } else {
                        ad_sayisi
                    };
                    format!(
                        "Sepet {} {} {} {} {} {} {}",
                        ad(rastgele.sayi(havuz)),
                        ad(rastgele.sayi(havuz)),
                        ad(rastgele.sayi(havuz)),
                        rastgele.sec(&d),
                        rastgele.sec(&b),
                        rastgele.sec(&c),
                        rastgele.sayi(999)
                    )
                }
            };
            let seviye = rastgele.sec(&["ERROR", "WARNING", "INFO"]);
            let mut kayit = format!("[{}] local.{seviye}: {mesaj}", Zaman::saniyeden(saniye));
            if istisnali && tur == 4 && rastgele.sayi(2) == 0 {
                let (sinif_adi, sinif, metot, satirlar) = ISTISNALAR[rastgele.sayi(2)];
                // Satır zamandan seçilir: rastgele sayıların sırası değişmesin
                let satir = satirlar[usize::from(saniye % 3 == 0)];
                kayit.push_str(&format!(
                    " {{\"exception\":\"[object] ({sinif}(code: 0): {mesaj} at /var/www/magaza/vendor/firma/paket/src/A.php:3)\n[stacktrace]\n#0 /var/www/magaza/app/Services/{sinif_adi}.php({satir}): A->b()\n#1 /var/www/magaza/vendor/firma/paket/src/C.php(9): App\\\\Services\\\\{sinif_adi}->{metot}()\n#2 {{main}}\n\"}} "
                ));
            }
            satirlar.push(kayit);
        }
        // Biraz karışık sıra: ilk ve son zamanlar sıraya bakılarak seçilir
        for _ in 0..adet / 20 {
            let (x, y) = (rastgele.sayi(adet), rastgele.sayi(adet));
            satirlar.swap(x, y);
        }
        satirlar.join("\n")
    }

    /// Logun kalıp tablosunu her kaydı önbelleksiz kümeleyen başvuru tablosuyla karşılaştırır.
    /// Özet, ikinci turda bölünen alt grupların örneklerini yeniden okuyarak tamamlar. Bölünen
    /// (örneği tamamlanan) alt grup sayısını döndürür.
    fn tablo_kayit_kayit_ayni_mi(metin: &str) -> usize {
        let ayarlar = RaporAyarlari {
            ilk: None,
            ..RaporAyarlari::default()
        };
        let bolunen = depo::metinden_ayarlarla(metin, &ayarlar.drain)
            .alt_gruplar()
            .iter()
            .filter(|alt| !alt.ornek_kesin())
            .count();
        let ozet = Ozet::metinden("a.log", metin, &ayarlar).unwrap();
        assert_eq!(
            tablo(&ozet.analiz),
            kayit_kayit_tablo(metin, &ayarlar.drain)
        );
        bolunen
    }

    /// Kalıp tablosunun karşılaştırılan alanları, sıralanmış olarak.
    fn tablo(analiz: &KalipAnalizi) -> Vec<Satir> {
        let mut satirlar: Vec<Satir> = analiz
            .satirlar
            .iter()
            .map(|satir| {
                (
                    satir.kalip.clone(),
                    satir.adet,
                    satir.seviye,
                    satir.ilk.to_string(),
                    satir.son.to_string(),
                    satir.ornek.clone(),
                    satir.parmak_izi.as_ref().map(ToString::to_string),
                )
            })
            .collect();
        satirlar.sort();
        satirlar
    }

    #[test]
    fn kalip_tablosu_kayit_kayit_kumelemeyle_ayni() {
        // Depo kayıtları okunurken önbellekli Drain'le kümeler; Drain değişken bir konum
        // bulursa okuma bitince ikinci kez. Tablo, her kaydı önbelleksiz kümelemekle birebir
        // aynı olmalı.
        let mut rastgele = Rastgele(0x9E37_79B9_7F4A_7C15);
        for deneme in 0..16 {
            let ad_sayisi = [20, 150, 400][deneme % 3];
            let adet = 200 + rastgele.sayi(500);
            let metin = rastgele_log(
                &mut rastgele,
                adet,
                ad_sayisi,
                deneme % 2 == 0,
                deneme % 4 < 2,
            );
            tablo_kayit_kayit_ayni_mi(&metin);
        }
    }

    #[test]
    fn ikinci_turda_bolunen_alt_grubun_ornegi_yeniden_okunur() {
        // Bu tohumla üretilen logda Drain ikinci tura girer ve beş alt grup bölünür
        let mut rastgele = Rastgele(116u64.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let adet = 150 + rastgele.sayi(250);
        let ad_sayisi = 120 + rastgele.sayi(300);
        let metin = rastgele_log(&mut rastgele, adet, ad_sayisi, true, false);
        let (kayitlar, _) = crate::kayit::deneme::topla(&metin);

        let mut depo = depo::metinden(&metin);
        let tahmini: Vec<(usize, String)> = depo
            .alt_gruplar()
            .iter()
            .filter(|alt| !alt.ornek_kesin())
            .map(|alt| (alt.ilk_kayit(), alt.ornek().to_owned()))
            .collect();
        assert_eq!(tahmini.len(), 5);
        let mut kaynak = YenidenOkuyucu::new(
            Cursor::new(metin.as_bytes()),
            depo.dizin().clone(),
            None,
            None,
        );
        depo.ornekleri_tamamla(&mut kaynak).unwrap();
        // Bölünen parçanın örneği artık ilk kaydının kendi mesajı; tahmini örnek (aynı maskeli
        // mesajlı ilk kaydınki) en az birinde farklıydı
        let mut farkli = 0;
        for (kayit, tahmin) in &tahmini {
            let alt = depo
                .alt_gruplar()
                .iter()
                .find(|alt| alt.ilk_kayit() == *kayit)
                .unwrap();
            assert!(alt.ornek_kesin());
            assert_eq!(alt.ornek(), kayitlar[*kayit].mesaj);
            farkli += usize::from(alt.ornek() != tahmin);
        }
        assert!(farkli > 0);
        assert_eq!(tablo_kayit_kayit_ayni_mi(&metin), 5);
    }

    /// Logun bütün kayıtlarını aynı ana taşır: `[1970-01-01 10:00:00]`.
    fn ayni_anda(metin: &str) -> String {
        metin
            .lines()
            .map(|satir| match satir.get(20..) {
                Some(geri) if satir.as_bytes().get(20) == Some(&b']') => {
                    format!("[1970-01-01 10:00:00{geri}")
                }
                _ => satir.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn bolunen_alt_grubun_en_gec_kaydi_da_bulunur() {
        // Bütün kayıtlar aynı anda: bir satırın konumu logda en sonra gelen kaydınınkidir.
        // Bu tohumlarla üretilen loglarda ikinci turda bölünen alt gruplar var; parçaların en
        // geç kayıtları da yeniden bulunmalı.
        for tohum in [4u64, 17, 37] {
            let mut rastgele = Rastgele(tohum.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
            let adet = 150 + rastgele.sayi(250);
            let ad_sayisi = 120 + rastgele.sayi(300);
            let metin = ayni_anda(&rastgele_log(&mut rastgele, adet, ad_sayisi, true, true));
            assert!(metin.starts_with("[1970-01-01 10:00:00] "), "{tohum}");
            assert!(tablo_kayit_kayit_ayni_mi(&metin) > 0, "{tohum}");
        }
    }

    #[test]
    fn bolunen_parcanin_ornegi_satirin_ornegi_olabilir() {
        // Bu tohumla üretilen logda ikinci turda bölünen bir parça, kendi satırının (küme ve
        // parmak izi) ilk kaydını taşır: satırın örneği yeniden okunan mesajdır. Özet örnekleri
        // tamamlamasaydı tablo başvurudan ayrılırdı.
        let mut rastgele = Rastgele(254u64.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let adet = 150 + rastgele.sayi(250);
        let ad_sayisi = 120 + rastgele.sayi(300);
        let metin = rastgele_log(&mut rastgele, adet, ad_sayisi, true, true);
        let ayarlar = RaporAyarlari {
            ilk: None,
            ..RaporAyarlari::default()
        };
        let tamamlanmadan = tablo(&kaliplari_cikar(&depo::metinden(&metin), &ayarlar));
        let ozet = Ozet::metinden("a.log", &metin, &ayarlar).unwrap();
        assert_ne!(tamamlanmadan, tablo(&ozet.analiz));
        assert!(tablo_kayit_kayit_ayni_mi(&metin) > 0);
    }

    #[test]
    fn farkli_dilimli_kayitlarda_ilk_ve_son_cevrilmis_zamana_gore() {
        // Aynı kalıbın iki seviyesi iki alt gruptur. Raporun saat dilimi JSON kaydındaki
        // +00:00: 10:09+03:00'daki WARNING kaydı 07:09'dur, kalıbın ilk kaydı odur. Metin
        // olarak ya da saat dilimi yok sayılarak karşılaştırılsaydı en son o görünürdü.
        let metin = [
            "[2026-10-24 10:05:00] local.INFO: Kuyruk dolu",
            r#"{"message":"Kuyruk dolu","level_name":"WARNING","datetime":"2026-10-24T10:00:00.5+00:00"}"#,
            "[2026-10-24 10:00:00] local.INFO: Kuyruk dolu",
            "[2026-10-24T10:09:00+03:00] local.WARNING: Kuyruk dolu",
            "[2026-10-24 10:09:00] local.INFO: Kuyruk dolu",
        ]
        .join("\n");
        let analiz = kaliplari_cikar(&depo::metinden(&metin), &RaporAyarlari::default());
        assert_eq!(analiz.satirlar.len(), 1);
        let satir = &analiz.satirlar[0];
        assert_eq!(satir.adet, 5);
        assert_eq!(satir.seviye, Seviye::Warning);
        assert_eq!(satir.ilk, Zaman::kayittan("2026-10-24 07:09:00"));
        assert_eq!(satir.son, Zaman::kayittan("2026-10-24 10:09:00"));
        assert_eq!(analiz.alt_grup_satiri, [0, 0]);
    }

    #[test]
    fn siralama_secenegi_tabloyu_dizer() {
        // Beş kalıp. Eşitlikler bilerek kuruldu: her sıralamanın ikinci ölçütü, satırların
        // ilk görülme sırasından farklı bir sonuç verir; ikinci ölçüt silinirse test bozulur.
        //
        //   kalıp               seviye   adet  ilk
        //   Stok azaldı         WARNING  1     10:00:00
        //   Sepet güncellendi   INFO     3     10:00:01
        //   Kargo hazırlandı    INFO     1     10:00:04
        //   Ödeme reddedildi    ERROR    1     10:00:04
        //   Disk doluyor        WARNING  2     10:00:05
        let metin = [
            "[2026-10-24 10:00:00] local.WARNING: Stok azaldı",
            "[2026-10-24 10:00:01] local.INFO: Sepet güncellendi",
            "[2026-10-24 10:00:02] local.INFO: Sepet güncellendi",
            "[2026-10-24 10:00:03] local.INFO: Sepet güncellendi",
            "[2026-10-24 10:00:04] local.INFO: Kargo hazırlandı",
            "[2026-10-24 10:00:04] local.ERROR: Ödeme reddedildi",
            "[2026-10-24 10:00:05] local.WARNING: Disk doluyor",
            "[2026-10-24 10:00:06] local.WARNING: Disk doluyor",
        ]
        .join("\n");

        for (siralama, beklenen) in [
            // Seviye; aynı seviyede adet (Disk 2 > Stok 1, Sepet 3 > Kargo 1)
            (
                Siralama::Onem,
                [
                    "Ödeme reddedildi",
                    "Disk doluyor",
                    "Stok azaldı",
                    "Sepet güncellendi",
                    "Kargo hazırlandı",
                ],
            ),
            // Adet; aynı adette seviye (Ödeme > Stok > Kargo)
            (
                Siralama::Adet,
                [
                    "Sepet güncellendi",
                    "Disk doluyor",
                    "Ödeme reddedildi",
                    "Stok azaldı",
                    "Kargo hazırlandı",
                ],
            ),
            // İlk görülme, en yeni önce; aynı anda seviye (Ödeme > Kargo)
            (
                Siralama::Yeni,
                [
                    "Disk doluyor",
                    "Ödeme reddedildi",
                    "Kargo hazırlandı",
                    "Sepet güncellendi",
                    "Stok azaldı",
                ],
            ),
        ] {
            // Yapı güncelleme sözdizimi: siralama dışındaki alanlar varsayılandan gelir
            let ayarlar = RaporAyarlari {
                siralama,
                ..RaporAyarlari::default()
            };
            let analiz = kaliplari_cikar(&depo::metinden(&metin), &ayarlar);
            let kaliplar: Vec<&str> = analiz
                .satirlar
                .iter()
                .map(|satir| satir.kalip.as_str())
                .collect();
            assert_eq!(kaliplar, beklenen, "{siralama:?}");
        }
    }

    #[test]
    fn ayni_kalip_farkli_konumda_ayri_satirlara_bolunur() {
        // Aynı mesaj: ikisi aynı denetleyiciden, biri bir servisten, biri istisnasız
        let metin = r#"[2026-10-24 10:00:00] local.ERROR: Undefined array key "id" {"exception":"[object] (ErrorException(code: 0): Undefined array key \"id\" at /var/www/app/Http/Controllers/SepetController.php:30)"}
[2026-10-24 10:05:00] local.ERROR: Undefined array key "id" {"exception":"[object] (ErrorException(code: 0): Undefined array key \"id\" at /var/www/app/Services/FaturaServisi.php:12)"}
[2026-10-24 10:09:00] local.ERROR: Undefined array key "id" {"exception":"[object] (ErrorException(code: 0): Undefined array key \"id\" at /var/www/app/Http/Controllers/SepetController.php:30)"}
[2026-10-24 10:10:00] local.ERROR: Undefined array key "id""#;
        let analiz = kaliplari_cikar(&depo::metinden(metin), &RaporAyarlari::default());

        assert_eq!(analiz.kalip_sayisi, 1);
        let satirlar: Vec<(usize, Option<String>)> = analiz
            .satirlar
            .iter()
            .map(|satir| {
                (
                    satir.adet,
                    satir.parmak_izi.as_ref().map(|iz| iz.to_string()),
                )
            })
            .collect();
        assert_eq!(
            satirlar,
            [
                (
                    2,
                    Some(
                        "/var/www/app/Http/Controllers/SepetController.php:30 (ErrorException)"
                            .to_string()
                    )
                ),
                (
                    1,
                    Some("/var/www/app/Services/FaturaServisi.php:12 (ErrorException)".to_string())
                ),
                (1, None),
            ]
        );
    }

    #[test]
    fn ornek_logda_kalip_bilgileri_dogru() {
        let analiz = kaliplari_cikar(&depo::metinden(ORNEK_LOG), &RaporAyarlari::default());

        assert_eq!(analiz.ham_mesaj_sayisi, 45);
        assert_eq!(analiz.maskeli_mesaj_sayisi, 27);
        assert_eq!(analiz.kalip_sayisi, 23);
        // Bir kalıp iki farklı yerden geldiği için iki satıra bölünür
        assert_eq!(analiz.satirlar.len(), 24);

        // En önemli kalıp başta: 17:05'teki Redis çöküşü
        let ilk = &analiz.satirlar[0];
        assert_eq!(ilk.seviye, Seviye::Critical);
        assert!(ilk.kalip.starts_with("Redis bağlantısı kurulamadı"));
        assert_eq!(
            (ilk.ilk.to_string(), ilk.son.to_string()),
            (
                "2026-10-24 17:05:13".to_string(),
                "2026-10-24 17:06:45".to_string()
            )
        );
        assert!(ilk.ornek.contains("10.0.3.12:6379"));
        assert_eq!(
            ilk.parmak_izi.as_ref().map(|iz| iz.to_string()).as_deref(),
            Some("app/Http/Middleware/HizSiniri.php:22, handle() (RedisException)")
        );

        // Aynı PHP mesajı iki ayrı denetleyiciden geliyor: iki ayrı hata
        let konumlar: Vec<String> = analiz
            .satirlar
            .iter()
            .filter(|satir| satir.kalip == r#"Attempt to read property "ad" on null"#)
            .filter_map(|satir| satir.parmak_izi.as_ref().map(|iz| iz.to_string()))
            .collect();
        assert_eq!(
            konumlar,
            [
                "app/Http/Controllers/KategoriController.php:54, goster() (ErrorException)",
                "app/Http/Controllers/UrunController.php:72, goster() (ErrorException)",
            ]
        );

        // 15:00'teki deploy'dan hemen sonra başlayan hata
        let kargo = analiz
            .satirlar
            .iter()
            .find(|satir| satir.kalip.contains("kargo_ucreti"))
            .unwrap();
        assert_eq!(kargo.adet, 4);
        assert_eq!(kargo.ilk, Zaman::kayittan("2026-10-24 15:02:11"));

        // 16:10–16:16 arasındaki ödeme zaman aşımı patlaması
        let curl = analiz
            .satirlar
            .iter()
            .find(|satir| satir.kalip.starts_with("cURL error"))
            .unwrap();
        assert_eq!(curl.adet, 6);
        assert_eq!(
            (curl.ilk, curl.son),
            (
                Zaman::kayittan("2026-10-24 16:10:02"),
                Zaman::kayittan("2026-10-24 16:16:02")
            )
        );
    }

    #[test]
    fn ozet_ornek_log_icin_dogru_yazilir() {
        let metin = metin_raporu(ORNEK_LOG, &RaporAyarlari::default());

        assert!(metin.starts_with("ornek.log: 205 satır\n"));
        assert!(metin.contains("\nSeviyeler:\n  CRITICAL     2\n  ERROR       16\n"));
        assert!(metin.contains(
            "Kalıplar: 45 farklı mesaj → maskeleyince 27 → Drain ile 23 kalıp (eşik 0,7) → parmak iziyle 24 grup\n"
        ));
        assert!(metin.contains("\n  adet  seviye     ilk       son       kalıp\n"));
        assert!(metin.contains("\n     2  CRITICAL   17:05:13  17:06:45  Redis bağlantısı"));
        assert!(metin.contains(
            "örnek: Redis bağlantısı kurulamadı: Connection refused [tcp://10.0.3.12:6379]"
        ));
        // Konum satırı da kalıp sütununun hizasında başlar
        assert!(metin.contains(
            "\n                                       konum: app/Http/Middleware/HizSiniri.php:22, handle() (RedisException)\n"
        ));
        // 24 satırın 10'u gösterildi; kalanlar söylenir
        assert!(metin.ends_with("\n  … 14 satır daha; tümünü görmek için --hepsi\n"));
    }

    #[test]
    fn ilk_ve_hepsi_tablonun_uzunlugunu_belirler() {
        let ilk_uc = metin_raporu(
            ORNEK_LOG,
            &RaporAyarlari {
                ilk: Some(3),
                ..RaporAyarlari::default()
            },
        );
        assert!(ilk_uc.contains("\n  … 21 satır daha; tümünü görmek için --hepsi\n"));
        // Üçüncü satır gösterilir, dördüncü (SQLSTATE) gösterilmez
        assert!(ilk_uc.contains("kargo_ucreti"));
        assert!(!ilk_uc.contains("SQLSTATE"));

        let hepsi = metin_raporu(
            ORNEK_LOG,
            &RaporAyarlari {
                ilk: None,
                ..RaporAyarlari::default()
            },
        );
        assert!(!hepsi.contains("satır daha"));
        // Önem sırasında en altta kalan DEBUG kalıbı da gösterilir
        assert!(hepsi.contains("Sepet önbellekten okundu"));
        assert!(!ilk_uc.contains("Sepet önbellekten okundu"));
    }

    #[test]
    fn tahmin_olan_farkli_mesaj_sayisi_isaretlenir() {
        let mut ozet = Ozet::metinden("ornek.log", ORNEK_LOG, &RaporAyarlari::default()).unwrap();
        assert!(ozet.kalip_cumlesi().starts_with("45 farklı mesaj → "));
        ozet.analiz.ham_mesaj_sayisi = 348_597;
        ozet.analiz.ham_mesaj_yaklasik = true;
        assert!(
            ozet.kalip_cumlesi()
                .starts_with("≈348597 farklı mesaj → maskeleyince 27 → "),
            "{}",
            ozet.kalip_cumlesi()
        );
    }

    #[test]
    fn filtre_raporda_yazilir() {
        use crate::filtre::Filtre;
        let ayarlar = RaporAyarlari {
            filtre: Filtre {
                seviye: Some(Seviye::Error),
                son: Some(5_400),
                ..Filtre::default()
            },
            ..RaporAyarlari::default()
        };
        let ozet = Ozet::metinden("ornek.log", ORNEK_LOG, &ayarlar).unwrap();
        // Dosyanın sayıları değişmez; filtre satırı onların altında
        let mut cikti = Vec::new();
        metin_yaz(&mut cikti, &ozet).unwrap();
        let metin = String::from_utf8(cikti).unwrap();
        assert!(
            metin.starts_with(
                "ornek.log: 205 satır\n  Kayıt         : 59\n  Devam satırı  : 143\n  Yetim satır   : 3\n  Filtre        : ERROR ve üstü, 2026-10-24 17:01:07 ve sonrası (son 1 sa 30 dk): 3 kayıt\n  Zaman aralığı : 2026-10-24 17:05:13 → 2026-10-24 17:55:03\n"
            ),
            "{metin}"
        );
        assert!(metin.contains("\nSeviyeler:\n  CRITICAL     2\n  ERROR        1\n\n"));

        // Öteki açıklamalar
        let mut filtre = FiltreOzeti {
            seviye: None,
            baslangic: Some(Zaman::kayittan("2026-10-24 16:00:00")),
            bitis: Some(Zaman::kayittan("2026-10-24 17:00:00")),
            son: None,
            kayit_sayisi: 13,
        };
        assert_eq!(
            filtre.aciklama(),
            "2026-10-24 16:00:00 ile 2026-10-24 17:00:00 arası: 13 kayıt"
        );
        filtre.baslangic = None;
        assert_eq!(filtre.aciklama(), "2026-10-24 17:00:00 öncesi: 13 kayıt");
        filtre.bitis = None;
        filtre.seviye = Some(Seviye::Warning);
        assert_eq!(filtre.aciklama(), "WARNING ve üstü: 13 kayıt");
    }

    #[test]
    fn ozet_sayilari_ve_gosterilecek_satirlar() {
        let ozet = Ozet::metinden("ornek.log", ORNEK_LOG, &RaporAyarlari::default()).unwrap();

        assert_eq!(ozet.satir_sayisi, 205);
        assert_eq!(ozet.kayit_sayisi, 59);
        assert_eq!(ozet.zaman_gosterimi(), ZamanGosterimi::Saat);
        assert_eq!(ozet.seviyeler[0], (Seviye::Critical, 2));
        assert_eq!(ozet.seviyeler.last(), Some(&(Seviye::Debug, 3)));
        assert_eq!(ozet.gosterilecek_satirlar().len(), 10);
        assert_eq!(ozet.gosterilmeyen(), 14);
    }

    impl Ozet {
        /// Bellekteki bir log metninin özeti: okur, depoya doldurur, özetler. Ani artışların
        /// örnekleri gerekirse metin yeniden okunur.
        pub fn metinden(
            dosya: &str,
            log_metni: &str,
            ayarlar: &RaporAyarlari,
        ) -> Result<Ozet, OzetHatasi> {
            let filtre = &ayarlar.filtre;
            let en_yeni = if filtre.sonu_gerekir() {
                crate::okuyucu::en_yeni_zaman(
                    log_metni.as_bytes(),
                    &ayarlar.satir_bicimi,
                    ayarlar.saat_dilimi,
                )
                .unwrap()
            } else {
                None
            };
            let depo_ayarlari = DepoAyarlari {
                drain: ayarlar.drain.clone(),
                filtre: (!filtre.bos_mu()).then(|| filtre.coz(en_yeni).expect("geçerli filtre")),
                saat_dilimi: ayarlar.saat_dilimi,
                satir_bicimi: ayarlar.satir_bicimi.clone(),
                konum_turu: ayarlar.konum_turu,
            };
            let mut depo = Depo::doldur(depo_ayarlari, log_metni.as_bytes()).unwrap();
            let mut kaynak = YenidenOkuyucu::new(
                Cursor::new(log_metni.as_bytes()),
                depo.dizin().clone(),
                depo.filtre().copied(),
                depo.saat_dilimi(),
            )
            .bicimle(depo.satir_bicimi().clone());
            Ozet::hesapla(dosya.to_owned(), &mut depo, ayarlar, &mut kaynak)
        }
    }

    /// Metin raporunu ekran yerine bir bayt dizisine yazar ve metin olarak döndürür.
    #[test]
    fn konumun_satiri_gruptaki_en_yeni_kayittan() {
        // Aynı hata üç kayıtta: ikisi metin, biri JSON. JSON kaydının fonksiyonu metin
        // kayıtlarından öğrenilir: üçü tek satırdır. JSON kaydı en yenisi; gösterilen satır
        // onunki.
        let metin_kaydi = |zaman: &str, satir: u32| {
            format!(
                r#"[2026-10-24 {zaman}] production.ERROR: Undefined array key "kargo_ucreti" {{"exception":"[object] (ErrorException(code: 0): Undefined array key \"kargo_ucreti\" at /var/www/magaza/app/Services/SepetHesaplayici.php:{satir})
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Foundation/Bootstrap/HandleExceptions.php(256): Illuminate\\Foundation\\Bootstrap\\HandleExceptions->handleError()
#1 /var/www/magaza/app/Services/SepetHesaplayici.php({satir}): Illuminate\\Foundation\\Bootstrap\\HandleExceptions->Illuminate\\Foundation\\Bootstrap\\{{closure}}()
#2 /var/www/magaza/app/Http/Controllers/SepetController.php(41): App\\Services\\SepetHesaplayici->toplam()
#3 {{main}}
"}}"#
            )
        };
        let json_kaydi = r#"{"message":"Undefined array key \"kargo_ucreti\"","context":{"exception":{"class":"ErrorException","message":"Undefined array key \"kargo_ucreti\"","code":0,"file":"/var/www/magaza/app/Services/SepetHesaplayici.php:88"}},"level":400,"level_name":"ERROR","channel":"production","datetime":"2026-10-24T13:00:00+00:00","extra":{}}"#;
        let log = [
            metin_kaydi("10:00:00", 88),
            metin_kaydi("12:00:00", 91),
            json_kaydi.to_string(),
        ]
        .join("\n");
        let ozet = Ozet::metinden("ornek.log", &log, &RaporAyarlari::default()).unwrap();
        let satirlar = &ozet.analiz.satirlar;
        assert_eq!(satirlar.len(), 1);
        assert_eq!(satirlar[0].adet, 3);
        assert_eq!(
            satirlar[0].parmak_izi.as_ref().map(ToString::to_string),
            Some("app/Services/SepetHesaplayici.php:88, toplam() (ErrorException)".to_string())
        );
        // JSON kaydı olmadan en yeni kayıt 91. satırda; log sıralı olmasa da
        let gosterilen = |kayitlar: &[String]| {
            let ozet = Ozet::metinden("ornek.log", &kayitlar.join("\n"), &RaporAyarlari::default())
                .unwrap();
            assert_eq!(ozet.analiz.satirlar.len(), 1);
            ozet.analiz.satirlar[0]
                .parmak_izi
                .as_ref()
                .and_then(ParmakIzi::gosterilen_satir)
        };
        assert_eq!(
            gosterilen(&[
                metin_kaydi("10:00:00", 88),
                metin_kaydi("12:00:00", 91),
                metin_kaydi("11:00:00", 88),
            ]),
            Some(91)
        );
        // Aynı andaki kayıtlardan logda sonra gelen daha yenidir
        assert_eq!(
            gosterilen(&[metin_kaydi("10:00:00", 91), metin_kaydi("10:00:00", 88)]),
            Some(88)
        );
        assert_eq!(
            gosterilen(&[metin_kaydi("10:00:00", 88), metin_kaydi("10:00:00", 91)]),
            Some(91)
        );
        // Eşitlikte alt grubun ilk kaydı değil, en geç kaydı sayılır: 88'in alt grubu önce
        // açıldı ve aynı andaki kaydı logda 91'inkinden sonra
        assert_eq!(
            gosterilen(&[
                metin_kaydi("09:00:00", 88),
                metin_kaydi("10:00:00", 91),
                metin_kaydi("10:00:00", 88),
            ]),
            Some(88)
        );
    }

    #[test]
    fn ayni_fonksiyondaki_farkli_kaliplar_kendi_satirlarini_gosterir() {
        // Rota dosyasındaki iki closure'da iki farklı hata. Konumları aynı (`routes/web.php`,
        // `{closure}`), kalıpları farklı: iki satır, her biri kendi kaydının satırıyla.
        let rota_kaydi = |zaman: &str, mesaj: &str, satir: u32| {
            format!(
                r#"[2026-10-24 {zaman}] production.ERROR: {mesaj} {{"exception":"[object] (RuntimeException(code: 0): {mesaj} at /var/www/magaza/routes/web.php:{satir})
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/CallableDispatcher.php(39): Illuminate\\Routing\\RouteFileRegistrar->{{closure}}()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/Route.php(254): Illuminate\\Routing\\CallableDispatcher->dispatch()
#2 {{main}}
"}}"#
            )
        };
        let log = [
            rota_kaydi("10:00:00", "Kapanış rotasında hata", 21),
            rota_kaydi("11:00:00", "Dize olarak loglanan istisna", 24),
        ]
        .join("\n");
        let ozet = Ozet::metinden("ornek.log", &log, &RaporAyarlari::default()).unwrap();
        let konumlar: Vec<String> = ozet
            .analiz
            .satirlar
            .iter()
            .map(|satir| satir.parmak_izi.as_ref().unwrap().to_string())
            .collect();
        assert_eq!(
            konumlar,
            [
                "routes/web.php:21, {closure} (RuntimeException)",
                "routes/web.php:24, {closure} (RuntimeException)",
            ]
        );
    }

    fn metin_raporu(log_metni: &str, ayarlar: &RaporAyarlari) -> String {
        let ozet = Ozet::metinden("ornek.log", log_metni, ayarlar).unwrap();
        let mut cikti = Vec::new();
        metin_yaz(&mut cikti, &ozet).unwrap();
        String::from_utf8(cikti).unwrap()
    }

    /// Her mesaj için bir INFO kaydı, saniyede bir.
    fn kayitlar(mesajlar: &[&str]) -> String {
        mesajlar
            .iter()
            .enumerate()
            .map(|(sira, mesaj)| {
                format!(
                    "[2026-10-24 10:{:02}:{:02}] production.INFO: {mesaj}\n",
                    sira / 60,
                    sira % 60
                )
            })
            .collect()
    }

    /// Logun ilk kalıp satırı.
    fn ilk_satir(log_metni: &str) -> KalipSatiri {
        let mut ozet = Ozet::metinden("ornek.log", log_metni, &RaporAyarlari::default()).unwrap();
        ozet.analiz.satirlar.remove(0)
    }

    #[test]
    fn joker_konumunun_degerleri_sayilir() {
        // Kategorik konum: aynı biçimdeki sınıf adları birleşir, değerleri adetleriyle sayılır
        let log = kayitlar(&[
            r"İş tamamlandı: App\Jobs\FaturaGonder",
            r"İş tamamlandı: App\Jobs\EpostaGonder",
            r"İş tamamlandı: App\Jobs\FaturaGonder",
            r"İş tamamlandı: App\Jobs\StokGuncelle",
        ]);
        let satir = ilk_satir(&log);
        assert_eq!(satir.kalip, "İş tamamlandı: <*>");
        assert_eq!(
            satir.degiskenler,
            [DegiskenKonum {
                konum: 3,
                degerler: vec![
                    (r"App\Jobs\FaturaGonder".to_owned(), 2),
                    (r"App\Jobs\EpostaGonder".to_owned(), 1),
                    (r"App\Jobs\StokGuncelle".to_owned(), 1),
                ],
                baska: 0,
            }]
        );
        // Metinde değerler satırı örneğin altında, kalıp sütunuyla hizalı
        let metin = metin_raporu(&log, &RaporAyarlari::default());
        let satirlar: Vec<&str> = metin.lines().collect();
        let kalip = satirlar
            .iter()
            .position(|satir| satir.ends_with("  İş tamamlandı: <*>"))
            .unwrap();
        let girinti = " ".repeat(39);
        assert_eq!(
            satirlar[kalip + 1],
            format!("{girinti}örnek: İş tamamlandı: App\\Jobs\\FaturaGonder")
        );
        // Üçüncü değer 70 karaktere sığmaz: bölünmeden alt satıra taşar
        assert_eq!(
            satirlar[kalip + 2],
            format!("{girinti}değerler: App\\Jobs\\FaturaGonder ×2, App\\Jobs\\EpostaGonder ×1,")
        );
        assert_eq!(
            satirlar[kalip + 3],
            format!("{girinti}          App\\Jobs\\StokGuncelle ×1")
        );
        assert_eq!(satirlar.len(), kalip + 4, "{metin}");
        // Joker yoksa değerler satırı da yok
        let metin = metin_raporu(
            &kayitlar(&["Önbellek temizlendi"; 2]),
            &RaporAyarlari::default(),
        );
        assert!(!metin.contains("değerler:"), "{metin}");
        assert!(
            ilk_satir(&kayitlar(&["Önbellek temizlendi"]))
                .degiskenler
                .is_empty()
        );
    }

    #[test]
    fn en_cok_gorulen_bes_deger_gosterilir() {
        // 7 farklı durum; en sık 5'i adede göre azalan, eşitlikte önce görülen önce
        let mut mesajlar = Vec::new();
        for (durum, adet) in [
            ("ORDER_A", 1),
            ("ORDER_B", 3),
            ("ORDER_C", 2),
            ("ORDER_D", 3),
            ("ORDER_E", 1),
            ("ORDER_F", 4),
            ("ORDER_G", 1),
        ] {
            for _ in 0..adet {
                mesajlar.push(format!("Durum: {durum}"));
            }
        }
        let mesajlar: Vec<&str> = mesajlar.iter().map(String::as_str).collect();
        let log = kayitlar(&mesajlar);
        let satir = ilk_satir(&log);
        assert_eq!(satir.kalip, "Durum: <*>");
        assert_eq!(satir.adet, 15);
        let degisken = &satir.degiskenler[0];
        assert_eq!(degisken.konum, 2);
        let degerler: Vec<(&str, usize)> = degisken
            .degerler
            .iter()
            .map(|(deger, adet)| (deger.as_str(), *adet))
            .collect();
        assert_eq!(
            degerler,
            [
                ("ORDER_F", 4),
                ("ORDER_B", 3),
                ("ORDER_D", 3),
                ("ORDER_C", 2),
                ("ORDER_A", 1)
            ]
        );
        assert_eq!(degisken.baska, 2);
        assert_eq!(
            degerler_metni(&satir.degiskenler, str::to_owned),
            "ORDER_F ×4, ORDER_B ×3, ORDER_D ×3, ORDER_C ×2, ORDER_A ×1, +2 başka"
        );
        // Tam beş değer: "başka" yazılmaz
        let bes = ilk_satir(&kayitlar(&[
            "Durum: A_1",
            "Durum: B_2",
            "Durum: C_3",
            "Durum: D_4",
            "Durum: E_5",
        ]));
        assert_eq!(bes.degiskenler[0].degerler.len(), 5);
        assert_eq!(bes.degiskenler[0].baska, 0);
        assert!(!degerler_metni(&bes.degiskenler, str::to_owned).contains("başka"));
    }

    #[test]
    fn birden_cok_joker_konum_sirasiyla_yazilir() {
        // İki <*>: birinde kullanıcı adları (değişken konum, 101 ad), ötekinde sabitler
        let mut mesajlar: Vec<String> = (0..101)
            .map(|sira| format!("Kullanıcı k{sira:03} için ORDER_CREATED işlendi"))
            .collect();
        mesajlar.push("Kullanıcı k000 için ORDER_FAILED işlendi".to_owned());
        let mesajlar: Vec<&str> = mesajlar.iter().map(String::as_str).collect();
        let satir = ilk_satir(&kayitlar(&mesajlar));
        assert_eq!(satir.kalip, "Kullanıcı <*> için <*> işlendi");
        let konumlar: Vec<usize> = satir.degiskenler.iter().map(|d| d.konum).collect();
        assert_eq!(konumlar, [2, 4]);
        assert_eq!(satir.degiskenler[0].degerler[0], ("k000".to_owned(), 2));
        assert_eq!(satir.degiskenler[0].baska, 96);
        assert_eq!(
            satir.degiskenler[1].degerler,
            [
                ("ORDER_CREATED".to_owned(), 101),
                ("ORDER_FAILED".to_owned(), 1)
            ]
        );
        assert_eq!(
            degerler_metni(&satir.degiskenler, str::to_owned),
            "2: k000 ×2, k001 ×1, k002 ×1, k003 ×1, k004 ×1, +96 başka; \
             4: ORDER_CREATED ×101, ORDER_FAILED ×1"
        );
    }

    #[test]
    fn ayni_degerin_alt_gruplari_toplanir() {
        // Aynı değer farklı alt gruplarda (burada farklı seviyelerde): adetleri toplanır
        let log = "\
[2026-10-24 10:00:00] production.INFO: Durum: ORDER_A
[2026-10-24 10:00:01] production.WARNING: Durum: ORDER_A
[2026-10-24 10:00:02] production.WARNING: Durum: ORDER_A
[2026-10-24 10:00:03] production.WARNING: Durum: ORDER_A
[2026-10-24 10:00:04] production.INFO: Durum: ORDER_B
";
        let satir = ilk_satir(log);
        assert_eq!(satir.kalip, "Durum: <*>");
        assert_eq!(satir.adet, 5);
        assert_eq!(
            satir.degiskenler[0].degerler,
            [("ORDER_A".to_owned(), 4), ("ORDER_B".to_owned(), 1)]
        );
    }

    #[test]
    fn deger_cumlenin_ayraci_olmadan_sayilir() {
        // "(Ayşe," belirtecinin değeri "Ayşe"; ayracı başka olan aynı ad aynı değerdir
        let log = kayitlar(&[
            "kayıt eklendi: (Ayşe, 1) tamam",
            "kayıt eklendi: (Mehmet, 2) tamam",
            "kayıt eklendi: Ayşe, 3) tamam",
        ]);
        let satir = ilk_satir(&log);
        assert_eq!(satir.kalip, "kayıt eklendi: <*> <SAYI>) tamam");
        assert_eq!(
            satir.degiskenler[0].degerler,
            [("Ayşe".to_owned(), 2), ("Mehmet".to_owned(), 1)]
        );
    }

    #[test]
    fn degerler_satiri_kisaltilir_ve_temizlenir() {
        // Tek başına sığmayan bir değer kısaltılır; sığan değer alt satıra geçer; değerlerdeki
        // kontrol karakterleri sembol olur
        let uzun_a = format!("Durum: {}_A", "X".repeat(75));
        let uzun_b = format!("Durum: {}_B", "Y".repeat(60));
        let metin = metin_raporu(&kayitlar(&[&uzun_a, &uzun_b]), &RaporAyarlari::default());
        let satirlar: Vec<&str> = metin.lines().collect();
        let ilk = satirlar
            .iter()
            .position(|satir| satir.contains("değerler: "))
            .unwrap();
        let yazilan = satirlar[ilk]
            .trim_start()
            .strip_prefix("değerler: ")
            .unwrap();
        assert_eq!(yazilan.chars().count(), KALIP_GENISLIGI - 10, "{yazilan}");
        assert!(yazilan.ends_with('…'));
        assert_eq!(
            satirlar[ilk + 1].trim_start(),
            format!("{}_B ×1", "Y".repeat(60))
        );
        assert!(satirlar[ilk + 1].starts_with(&" ".repeat(49)));
        // Her satır genişliğe sığar; parçalar virgülden bölünür
        let satirlar = degerler_satirlari(
            &[DegiskenKonum {
                konum: 1,
                degerler: vec![
                    ("aaaa".to_owned(), 10),
                    ("bbbb".to_owned(), 9),
                    ("cccc".to_owned(), 8),
                ],
                baska: 4,
            }],
            20,
        );
        assert_eq!(satirlar, ["aaaa ×10, bbbb ×9,", "cccc ×8, +4 başka"]);
        assert!(degerler_satirlari(&[], 20).is_empty());
        let metin = metin_raporu(
            &kayitlar(&[
                "Durum kodu şimdi ORDER_A\u{1b}[2J",
                "Durum kodu şimdi ORDER_B",
            ]),
            &RaporAyarlari::default(),
        );
        assert!(
            metin.contains("değerler: ORDER_A␛[2J ×1, ORDER_B ×1\n"),
            "{metin}"
        );
    }
}
