//! Özet raporu: genel sayılar, seviyeler ve kalıp tablosu.
//!
//! Drain yalnızca metinle ilgilenir; seviyeyi, zamanı ve yığın izini bilmez. Bu modül her
//! kaydı maskeleyip Drain'e verir, sonra kaydı iki şeye göre bir gruba ekler: Drain'in
//! döndürdüğü küme numarası ve kaydın parmak izi. Aynı kalıptaki kayıtlar kodun farklı
//! yerlerinden geliyorsa böylece ayrı satırlarda görünür.
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
use std::collections::{BTreeSet, HashMap};
use std::io::{self, Write};
use std::path::Path;

use crate::ani_artis::{AniArtis, AniArtislar, EN_AZ_DILIM, EN_DUSUK_SEVIYE, ESIK};
use crate::drain::{Ayarlar, Drain};
use crate::karsilastirma::{
    ANLAMLILIK, ARTIS_KATI, Degisim, Karsilastirma, KarsilastirmaHatasi, Pencere,
};
use crate::kayit::{Kayit, Log, Seviye};
use crate::maske::maskele;
use crate::parmak_izi::ParmakIzi;
use crate::zaman::{Zaman, ZamanNoktasi, sure_yaz};

/// Tabloda varsayılan olarak gösterilecek satır sayısı.
pub const VARSAYILAN_ILK: usize = 10;
/// Uzun kalıplar ve örnekler bu kadar karakterden sonra kesilir (metin ve Markdown).
pub const KALIP_GENISLIGI: usize = 80;

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

    /// Satırları bu sıralamaya göre dizer. Sıralama kararlıdır: eşit olanlar ilk
    /// görüldükleri sırada kalır.
    fn uygula(self, satirlar: &mut [KalipSatiri<'_>]) {
        match self {
            Siralama::Onem => satirlar.sort_by_key(|satir| Reverse((satir.seviye, satir.adet))),
            Siralama::Adet => satirlar.sort_by_key(|satir| Reverse((satir.adet, satir.seviye))),
            Siralama::Yeni => satirlar.sort_by_key(|satir| Reverse((satir.ilk, satir.seviye))),
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
}

impl Default for RaporAyarlari {
    fn default() -> Self {
        RaporAyarlari {
            drain: Ayarlar::default(),
            siralama: Siralama::Onem,
            ilk: Some(VARSAYILAN_ILK),
            karsilastir: None,
            ani_artis: None,
        }
    }
}

/// Tablodaki bir satır: bir kalıp, bir parmak izi ve onlara uyan kayıtlar hakkındaki bilgiler.
#[derive(Debug)]
pub struct KalipSatiri<'a> {
    /// Kalıbın son hali: "Zamanlanmış görev başladı: <*>"
    pub kalip: String,
    pub adet: usize,
    /// Satırdaki kayıtların en önemli seviyesi.
    pub seviye: Seviye,
    /// İlk görülme zamanı.
    pub ilk: &'a str,
    /// Son görülme zamanı.
    pub son: &'a str,
    /// Satırdaki ilk kaydın maskelenmemiş mesajı: "Kullanıcı 381 giriş yaptı"
    pub ornek: &'a str,
    /// Kayıtlar bir istisnadan geliyorsa: istisnanın sınıfı ve uygulama kodundaki yeri.
    pub parmak_izi: Option<ParmakIzi<'a>>,
    /// Satırdaki kayıtların `Log::kayitlar` içindeki sıra numaraları, logdaki sırayla.
    /// Karşılaştırma kayıtları bunlarla iki yarıya, ani artış aramak da dilimlere sayar.
    pub kayitlar: Vec<usize>,
}

/// Kalıp çıkarmanın sonucu.
#[derive(Debug)]
pub struct KalipAnalizi<'a> {
    /// Maskelemeden önce kaç farklı mesaj vardı?
    pub ham_mesaj_sayisi: usize,
    /// Maskelemeden sonra kaç farklı mesaj kaldı?
    pub maskeli_mesaj_sayisi: usize,
    /// Drain kaç kalıp buldu? Parmak izi bir kalıbı birden çok satıra bölebilir.
    pub kalip_sayisi: usize,
    /// Ayarlardaki sıralamaya göre dizilmiş satırlar.
    pub satirlar: Vec<KalipSatiri<'a>>,
}

/// Kalıp çıkarılırken her grup için biriktirilen bilgiler. Bir grup, aynı Drain kümesine
/// düşen ve aynı parmak izini taşıyan kayıtlardır.
struct Grup<'a> {
    /// Drain'deki küme numarası; kalıp metni en sonda bununla alınır.
    kume: usize,
    parmak_izi: Option<ParmakIzi<'a>>,
    adet: usize,
    seviye: Seviye,
    ilk: &'a str,
    son: &'a str,
    ornek: &'a str,
    kayitlar: Vec<usize>,
}

impl<'a> Grup<'a> {
    /// Boş bir grup açar; seviye, zaman ve örnek ilk kayıttan alınır. Kaydın kendisini
    /// saymak `ekle`nin işidir.
    fn yeni(kume: usize, parmak_izi: Option<ParmakIzi<'a>>, ilk_kayit: &Kayit<'a>) -> Grup<'a> {
        Grup {
            kume,
            parmak_izi,
            adet: 0,
            seviye: ilk_kayit.seviye(),
            ilk: ilk_kayit.zaman(),
            son: ilk_kayit.zaman(),
            ornek: ilk_kayit.mesaj(),
            kayitlar: Vec::new(),
        }
    }

    /// Kaydı gruba katar: adedi artırır, en yüksek seviyeyi ve zaman aralığını günceller,
    /// kaydın sıra numarasını saklar.
    fn ekle(&mut self, sira: usize, kayit: &Kayit<'a>) {
        self.adet += 1;
        self.seviye = self.seviye.max(kayit.seviye());
        self.ilk = self.ilk.min(kayit.zaman());
        self.son = self.son.max(kayit.zaman());
        self.kayitlar.push(sira);
    }
}

/// Kayıtları maskeleyip Drain'den geçirir, parmak izine göre böler, her grup için bir satır
/// üretir ve satırları ayarlardaki sıraya dizer.
pub fn kaliplari_cikar<'a>(log: &Log<'a>, ayarlar: &RaporAyarlari) -> KalipAnalizi<'a> {
    let mut drain = Drain::new(ayarlar.drain.clone());
    let mut gruplar: Vec<Grup<'a>> = Vec::new();
    // (küme numarası, parmak izi) → grubun `gruplar` listesindeki sırası
    let mut grup_sirasi: HashMap<(usize, Option<ParmakIzi<'a>>), usize> = HashMap::new();
    let mut ham_mesajlar = BTreeSet::new();
    let mut maskeli_mesajlar = BTreeSet::new();

    for (kayit_sirasi, kayit) in log.kayitlar.iter().enumerate() {
        let maskeli = maskele(kayit.mesaj());
        let kume = drain.ekle(&maskeli);
        let parmak_izi = ParmakIzi::kayittan(kayit);
        ham_mesajlar.insert(kayit.mesaj());
        maskeli_mesajlar.insert(maskeli);

        // Bu küme ve parmak izi ilk kez görülüyorsa yeni bir grup aç, sonra kaydı ona kat
        let sira = *grup_sirasi.entry((kume, parmak_izi)).or_insert_with(|| {
            gruplar.push(Grup::yeni(kume, parmak_izi, kayit));
            gruplar.len() - 1
        });
        gruplar[sira].ekle(kayit_sirasi, kayit);
    }

    // Kalıp metni yeni mesajlar geldikçe değişebildiği için en sonda Drain'den alınır
    let kumeler = drain.kumeler();
    let mut satirlar: Vec<KalipSatiri<'a>> = gruplar
        .into_iter()
        .map(|grup| KalipSatiri {
            kalip: kumeler[grup.kume].metin(),
            adet: grup.adet,
            seviye: grup.seviye,
            ilk: grup.ilk,
            son: grup.son,
            ornek: grup.ornek,
            parmak_izi: grup.parmak_izi,
            kayitlar: grup.kayitlar,
        })
        .collect();
    ayarlar.siralama.uygula(&mut satirlar);

    KalipAnalizi {
        ham_mesaj_sayisi: ham_mesajlar.len(),
        maskeli_mesaj_sayisi: maskeli_mesajlar.len(),
        kalip_sayisi: kumeler.len(),
        satirlar,
    }
}

/// Raporun bütün bilgileri. Bir kez hesaplanır; metin, JSON ve Markdown çıktıları aynı özeti
/// farklı biçimlerde yazar. Böylece üç biçimdeki sayılar hep aynıdır.
pub struct Ozet<'a> {
    /// Dosyanın yolu, ekranda gösterileceği haliyle.
    pub dosya: String,
    pub satir_sayisi: usize,
    pub kayit_sayisi: usize,
    pub devam_satiri_sayisi: usize,
    pub yetim_satir_sayisi: usize,
    /// En eski ve en yeni kaydın zamanı; hiç kayıt yoksa None.
    pub zaman_araligi: Option<(&'a str, &'a str)>,
    /// Her seviyeden kaç kayıt var; en önemli seviye başta.
    pub seviyeler: Vec<(Seviye, usize)>,
    pub analiz: KalipAnalizi<'a>,
    /// `--karsilastir` verildiyse logun iki yarısının karşılaştırması.
    pub karsilastirma: Option<Karsilastirma>,
    /// `--ani-artis` verildiyse kalıpların ani artışları.
    pub ani_artislar: Option<AniArtislar>,
    pub ayarlar: RaporAyarlari,
}

impl<'a> Ozet<'a> {
    /// Logu özetler. Özet, logdan ödünç aldığı zamanlar ve mesajlar yüzünden en fazla logu
    /// ödünç aldığı süre kadar (`'a`) yaşayabilir.
    ///
    /// Karşılaştırma noktası logun dışındaysa (ör. bütün kayıtlardan önce) özet çıkarılamaz.
    pub fn hesapla(
        yol: &Path,
        icerik: &str,
        log: &'a Log<'_>,
        ayarlar: &RaporAyarlari,
    ) -> Result<Ozet<'a>, KarsilastirmaHatasi> {
        let analiz = kaliplari_cikar(log, ayarlar);
        // Option<Result<K, H>> → Result<Option<K>, H>: `transpose`. Böylece `?` yalnızca
        // karşılaştırma istendiyse ve başarısız olduysa erken döner.
        let karsilastirma = ayarlar
            .karsilastir
            .map(|nokta| Karsilastirma::hesapla(log, &analiz, nokta))
            .transpose()?;
        // Ani artış aramak başarısız olmaz: log kısaysa bölüm bunu söyler
        let ani_artislar = ayarlar
            .ani_artis
            .map(|dakika| AniArtislar::hesapla(log, &analiz, dakika));
        Ok(Ozet {
            // `display`: dosya adı geçerli UTF-8 değilse bozuk baytlar � olarak yazılır
            dosya: yol.display().to_string(),
            satir_sayisi: icerik.lines().count(),
            kayit_sayisi: log.kayitlar.len(),
            devam_satiri_sayisi: log.devam_satiri_sayisi(),
            yetim_satir_sayisi: log.yetim_satir,
            zaman_araligi: log.zaman_araligi(),
            // BTreeMap seviyeleri küçükten büyüğe tutar; en önemlisi başta olsun diye ters çevir
            seviyeler: log.seviye_sayilari().into_iter().rev().collect(),
            analiz,
            karsilastirma,
            ani_artislar,
            ayarlar: ayarlar.clone(),
        })
    }

    /// Bütün kayıtlar aynı gündeyse tabloda saat yeter, değilse tarih de gösterilir.
    pub fn tek_gun(&self) -> bool {
        self.zaman_araligi
            .is_some_and(|(en_eski, en_yeni)| en_eski[..10] == en_yeni[..10])
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
    pub fn gosterilecek_satirlar(&self) -> &[KalipSatiri<'a>] {
        self.sinirla(&self.analiz.satirlar)
    }

    /// `--ilk` sınırı yüzünden gösterilmeyen satır sayısı.
    pub fn gosterilmeyen(&self) -> usize {
        self.disarida_kalan(&self.analiz.satirlar)
    }

    /// Kalıp çıkarmanın adımları tek cümlede: "45 farklı mesaj → maskeleyince 27 → ..."
    pub fn kalip_cumlesi(&self) -> String {
        let analiz = &self.analiz;
        format!(
            "{} farklı mesaj → maskeleyince {} → Drain ile {} kalıp (eşik {}) → parmak iziyle {} grup",
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
pub fn metin_yaz(cikti: &mut impl Write, ozet: &Ozet<'_>) -> io::Result<()> {
    writeln!(
        cikti,
        "{}: {} satır",
        gorunur(&ozet.dosya),
        ozet.satir_sayisi
    )?;
    writeln!(cikti, "  Kayıt         : {}", ozet.kayit_sayisi)?;
    writeln!(cikti, "  Devam satırı  : {}", ozet.devam_satiri_sayisi)?;
    writeln!(cikti, "  Yetim satır   : {}", ozet.yetim_satir_sayisi)?;
    if let Some((en_eski, en_yeni)) = ozet.zaman_araligi {
        writeln!(cikti, "  Zaman aralığı : {en_eski} → {en_yeni}")?;
    }

    writeln!(cikti)?;
    writeln!(cikti, "Seviyeler:")?;
    for (seviye, adet) in &ozet.seviyeler {
        writeln!(cikti, "  {:<10}{adet:>4}", seviye.ad())?;
    }

    writeln!(cikti)?;
    writeln!(cikti, "Kalıplar: {}", ozet.kalip_cumlesi())?;

    let satirlar = ozet.gosterilecek_satirlar();
    let tek_gun = ozet.tek_gun();
    if !satirlar.is_empty() {
        baslik_yaz(cikti, &format!("{:>4}", "adet"), tek_gun)?;
        for satir in satirlar {
            satir_yaz(
                cikti,
                &format!("{:>4}", satir.adet),
                satir,
                (satir.ilk, satir.son),
                None,
                tek_gun,
            )?;
        }
        gizlenen_yaz(cikti, ozet.gosterilmeyen())?;
    }

    if let Some(karsilastirma) = &ozet.karsilastirma {
        karsilastirma_yaz(cikti, ozet, karsilastirma, tek_gun)?;
    }
    if let Some(ani_artislar) = &ozet.ani_artislar {
        ani_artis_yaz(cikti, ozet, ani_artislar, tek_gun)?;
    }
    Ok(())
}

/// Karşılaştırma bölümü: iki yarının özeti, sonra yeni çıkan, kaybolan ve artan kalıplar.
fn karsilastirma_yaz(
    cikti: &mut impl Write,
    ozet: &Ozet<'_>,
    karsilastirma: &Karsilastirma,
    tek_gun: bool,
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
            zaman_goster(&pencere.baslangic.to_string(), tek_gun),
            zaman_goster(&pencere.bitis.to_string(), tek_gun),
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
        baslik_yaz(cikti, &format!("{:>4}  {:>5}", "önce", "sonra"), tek_gun)?;
        for degisim in ozet.sinirla(degisimler) {
            let satir = &ozet.analiz.satirlar[degisim.satir];
            // Artanlarda sıklığın nasıl değiştiği de yazılır
            let artis = (degisim.once > 0 && degisim.sonra > 0)
                .then(|| artis_yaz(degisim, &karsilastirma.once, &karsilastirma.sonra));
            satir_yaz(
                cikti,
                &format!("{:>4}  {:>5}", degisim.once, degisim.sonra),
                satir,
                (satir.ilk, satir.son),
                artis.as_deref(),
                tek_gun,
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

/// Ani artış bölümü: puanı en yüksek olan artışlar. Satırın ilk ve son zamanı, kalıbın değil
/// artışın ilk ve son kaydıdır.
fn ani_artis_yaz(
    cikti: &mut impl Write,
    ozet: &Ozet<'_>,
    ani_artislar: &AniArtislar,
    tek_gun: bool,
) -> io::Result<()> {
    writeln!(cikti)?;
    writeln!(cikti, "{}", ani_artis_basligi(ani_artislar))?;
    if ani_artislar.artislar.is_empty() {
        return Ok(());
    }
    let sutunlar =
        |zirve: &str, olagan: &str, puan: &str| format!("{zirve:>5}  {olagan:>6}  {puan:>5}");
    baslik_yaz(cikti, &sutunlar("zirve", "olağan", "puan"), tek_gun)?;
    for artis in ozet.sinirla(&ani_artislar.artislar) {
        let satir = &ozet.analiz.satirlar[artis.satir];
        satir_yaz(
            cikti,
            &sutunlar(
                &artis.zirve_adet.to_string(),
                &ondalik(artis.olagan),
                &ondalik_kisa(artis.puan),
            ),
            satir,
            (&artis.ilk.to_string(), &artis.son.to_string()),
            Some(&ani_artis_aciklamasi(artis, tek_gun)),
            tek_gun,
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
        format!("Ani artışlar ({kapsam} dilimler): logda kayıt yok")
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
fn ani_artis_aciklamasi(artis: &AniArtis, tek_gun: bool) -> String {
    let aralik = dilim_araligi(artis.baslangic, artis.bitis, tek_gun);
    if artis.dilim_sayisi == 1 {
        format!("{aralik} arası {} kayıt", artis.kayit_sayisi)
    } else {
        format!(
            "{aralik} arası {} dilimde {} kayıt, zirve {}",
            artis.dilim_sayisi,
            artis.kayit_sayisi,
            dilim_araligi(artis.zirve_baslangic, artis.zirve_bitis, tek_gun)
        )
    }
}

/// Dilim sınırlarından bir aralık. Sınırlar hep dakika başı olduğu için saniye yazılmaz. Tek
/// günlük logda "16:10–16:20"; çok günlükte "10-24 16:10–16:20", gün değişiyorsa
/// "10-24 23:50–10-25 00:10".
///
/// `Zaman`'ın metni hep 19 ASCII karakterdir (`2026-10-24 16:10:00`); bayt konumlarıyla
/// dilimlemek güvenlidir.
pub fn dilim_araligi(baslangic: Zaman, bitis: Zaman, tek_gun: bool) -> String {
    let (bas, son) = (baslangic.to_string(), bitis.to_string());
    if tek_gun {
        format!("{}–{}", &bas[11..16], &son[11..16])
    } else if bas[..10] == son[..10] {
        format!("{} {}–{}", &bas[5..10], &bas[11..16], &son[11..16])
    } else {
        format!(
            "{} {}–{} {}",
            &bas[5..10],
            &bas[11..16],
            &son[5..10],
            &son[11..16]
        )
    }
}

/// Tablonun başlık satırı. `sayilar`: satır başındaki sayı sütunlarının başlıkları.
fn baslik_yaz(cikti: &mut impl Write, sayilar: &str, tek_gun: bool) -> io::Result<()> {
    let zaman_genisligi = if tek_gun { 8 } else { 14 };
    writeln!(
        cikti,
        "  {sayilar}  {:<9}  {:<zaman_genisligi$}  {:<zaman_genisligi$}  kalıp",
        "seviye", "ilk", "son"
    )
}

/// Tablonun bir satırı ve altındaki açıklama, örnek ve konum satırları.
///
/// `sayilar`: satır başındaki sayı sütunları (`"   4"` ya da `"   0      2"`). Alttaki satırlar
/// kalıp sütununun hizasından başlar; hiza, sayı sütunlarının genişliğinden hesaplanır.
/// `zamanlar`: ilk ve son sütunlarına yazılacak zamanlar; kalıp tablosunda kalıbın, ani artış
/// tablosunda artışın ilk ve son kaydı.
fn satir_yaz(
    cikti: &mut impl Write,
    sayilar: &str,
    satir: &KalipSatiri<'_>,
    zamanlar: (&str, &str),
    aciklama: Option<&str>,
    tek_gun: bool,
) -> io::Result<()> {
    let zaman_genisligi = if tek_gun { 8 } else { 14 };
    let girinti = " ".repeat(
        2 + sayilar.chars().count() + 2 + 9 + 2 + zaman_genisligi + 2 + zaman_genisligi + 2,
    );
    let (ilk, son) = zamanlar;
    writeln!(
        cikti,
        "  {sayilar}  {:<9}  {:<zaman_genisligi$}  {:<zaman_genisligi$}  {}",
        satir.seviye.ad(),
        zaman_goster(ilk, tek_gun),
        zaman_goster(son, tek_gun),
        // Önce temizle, sonra kısalt: genişlik ekranda görünen karakterlerle sayılsın
        kisalt(&gorunur(&satir.kalip), KALIP_GENISLIGI)
    )?;
    if let Some(aciklama) = aciklama {
        writeln!(cikti, "{girinti}artış: {aciklama}")?;
    }
    // Kalıp maskelenmiş ya da <*> içeriyorsa arkasındaki gerçek değerleri göster
    if satir.ornek != satir.kalip {
        writeln!(
            cikti,
            "{girinti}örnek: {}",
            kisalt(&gorunur(satir.ornek), KALIP_GENISLIGI - 7)
        )?;
    }
    // Kayıtlar bir istisnadan geliyorsa nerede oluştuğunu göster. Dosya yolu kısaltılmaz:
    // terminalde tıklanıp doğrudan açılabilsin.
    if let Some(parmak_izi) = satir.parmak_izi {
        writeln!(
            cikti,
            "{girinti}konum: {}",
            gorunur(&parmak_izi.to_string())
        )?;
    }
    Ok(())
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
pub fn ondalik_kisa(sayi: f64) -> String {
    format!("{sayi:.1}").replace('.', ",")
}

/// Zamanı tabloda gösterilecek hale getirir: tek günlük logda "14:03:11", birden çok günü
/// kapsayan logda "10-24 14:03:11".
///
/// Kayıt zamanlarının ilk 19 karakteri her zaman geçerli bir tarih-saat olduğu ve bu
/// karakterler ASCII olduğu için bayt konumlarıyla dilimlemek güvenlidir.
pub fn zaman_goster(zaman: &str, tek_gun: bool) -> String {
    if tek_gun {
        zaman[11..19].to_string()
    } else {
        format!("{} {}", &zaman[5..10], &zaman[11..19])
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
    use crate::ani_artis::deneme_logu;
    use crate::kayit::kayitlari_ayir;

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
        let log = kayitlari_ayir(KONTROLLU_LOG);
        let ozet = Ozet::hesapla(
            Path::new("kötü\u{1b}[2J.log"),
            KONTROLLU_LOG,
            &log,
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
        let log = kayitlari_ayir(&metin);
        let rapor = metin_raporu(&log, &RaporAyarlari::default());
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
        let log = kayitlari_ayir(metin);
        let ozet =
            Ozet::hesapla(Path::new("a.log"), metin, &log, &RaporAyarlari::default()).unwrap();
        assert!(!ozet.tek_gun());

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
        let log = kayitlari_ayir(ORNEK_LOG);
        let ayarlar = RaporAyarlari {
            karsilastir: ZamanNoktasi::ayristir("16:11"),
            ilk: Some(2),
            ..RaporAyarlari::default()
        };
        let metin = metin_raporu(&log, &ayarlar);

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
                                              konum: app/Http/Middleware/HizSiniri.php:22 (RedisException)
     0      1  INFO       16:24:10  16:24:10  Ödeme sağlayıcısı yeniden yanıt veriyor
  … 1 satır daha; tümünü görmek için --hepsi

Kaybolanlar (yalnızca önce): 11
  önce  sonra  seviye     ilk       son       kalıp
     1      0  ERROR      09:47:12  09:47:12  Attempt to read property "ad" on null
                                              konum: app/Http/Controllers/KategoriController.php:54 (ErrorException)
     1      0  ERROR      11:30:44  11:30:44  App\Jobs\FaturaGonder has been attempted too many times.
                                              konum: uygulama kodunda bulunamadı (MaxAttemptsExceededException)
  … 9 satır daha; tümünü görmek için --hepsi

Artanlar (sonra ≥ 2 kat sık, tesadüf ihtimali < %1): 1
  önce  sonra  seviye     ilk       son       kalıp
     1      5  ERROR      16:10:02  16:16:02  cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> by…
                                              artış: saatte 0,1 → 2,1 (15,4 kat)
                                              örnek: cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes…
                                              konum: app/Services/OdemeServisi.php:64 (ConnectionException)
"#;
        assert!(metin.ends_with(beklenen), "{metin}");
    }

    #[test]
    fn bos_bolum_ve_cok_gunlu_karsilastirma() {
        let log = kayitlari_ayir(ORNEK_LOG);
        let ayarlar = RaporAyarlari {
            karsilastir: ZamanNoktasi::ayristir("15:00"),
            ..RaporAyarlari::default()
        };
        assert!(
            metin_raporu(&log, &ayarlar)
                .ends_with("\nArtanlar (sonra ≥ 2 kat sık, tesadüf ihtimali < %1): yok\n")
        );

        // İki güne yayılan logda zamanlar tarihle yazılır
        let metin = "[2026-10-24 23:00:00] local.ERROR: Gece hatası\n\
                     [2026-10-25 01:00:00] local.ERROR: Sabah hatası";
        let log = kayitlari_ayir(metin);
        let ayarlar = RaporAyarlari {
            karsilastir: ZamanNoktasi::ayristir("00:30"),
            ..RaporAyarlari::default()
        };
        let ozet = Ozet::hesapla(Path::new("a.log"), metin, &log, &ayarlar).unwrap();
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
    fn ani_artis_bolumu_yazilir() {
        let log = kayitlari_ayir(ORNEK_LOG);
        let ayarlar = RaporAyarlari {
            ani_artis: Some(10),
            ilk: Some(2),
            ..RaporAyarlari::default()
        };
        let metin = metin_raporu(&log, &ayarlar);

        // Normal rapor değişmeden başta, ani artışlar sonda. İlk ve son sütunları artışın ilk
        // ve son kaydıdır; açıklama satırı dilimleri söyler.
        let beklenen = r#"
  … 22 satır daha; tümünü görmek için --hepsi

Ani artışlar (WARNING ve üstü, 10 dakikalık 59 dilim, puan > 3,5): 1
  zirve  olağan   puan  seviye     ilk       son       kalıp
      6       0    4,0  ERROR      16:10:02  16:16:02  cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> by…
                                                       artış: 16:10–16:20 arası 6 kayıt
                                                       örnek: cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes…
                                                       konum: app/Services/OdemeServisi.php:64 (ConnectionException)
"#;
        assert!(metin.ends_with(beklenen), "{metin}");
    }

    #[test]
    fn ani_artislar_puana_gore_ve_ilk_kadar() {
        let metin = deneme_logu();
        let log = kayitlari_ayir(&metin);
        let ayarlar = RaporAyarlari {
            ani_artis: Some(1),
            ilk: Some(2),
            ..RaporAyarlari::default()
        };
        let ozet = Ozet::hesapla(Path::new("a.log"), &metin, &log, &ayarlar).unwrap();
        let mut cikti = Vec::new();
        metin_yaz(&mut cikti, &ozet).unwrap();
        let rapor = String::from_utf8(cikti).unwrap();

        // Üç artışın puanı en yüksek ikisi. İlk ve son, kalıbın değil artışın kayıtları
        // (kuyruk kalıbının ilk kaydı 10:05:30); olağan, dilim başına medyan.
        let beklenen = r#"
Ani artışlar (WARNING ve üstü, 1 dakikalık 120 dilim, puan > 3,5): 3
  zirve  olağan   puan  seviye     ilk       son       kalıp
      9       0    6,1  ERROR      10:30:30  10:32:36  Kuyruk dolu
                                                       artış: 10:30–10:33 arası 3 dilimde 22 kayıt, zirve 10:31–10:32
      9       2    4,7  WARNING    10:45:10  10:45:18  Yavaş istek
                                                       artış: 10:45–10:46 arası 9 kayıt
  … 1 satır daha; tümünü görmek için --hepsi
"#;
        assert!(rapor.ends_with(beklenen), "{rapor}");
    }

    #[test]
    fn ani_artis_basligi_durumu_soyler() {
        let log = kayitlari_ayir(ORNEK_LOG);
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
            let metin = metin_raporu(&log, &ayarlar);
            assert!(metin.ends_with(beklenen), "{metin}");
        }

        let log = kayitlari_ayir("");
        let ayarlar = RaporAyarlari {
            ani_artis: Some(1),
            ..RaporAyarlari::default()
        };
        let ozet = Ozet::hesapla(Path::new("bos.log"), "", &log, &ayarlar).unwrap();
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
        let log = kayitlari_ayir(ORNEK_LOG);
        let ayarlar = RaporAyarlari {
            karsilastir: ZamanNoktasi::ayristir("16:11"),
            ani_artis: Some(10),
            ilk: Some(1),
            ..RaporAyarlari::default()
        };
        let metin = metin_raporu(&log, &ayarlar);
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
        let log = kayitlari_ayir(&metin);
        let ayarlar = RaporAyarlari {
            ani_artis: Some(1),
            ..RaporAyarlari::default()
        };
        let ozet = Ozet::hesapla(Path::new("a.log"), &metin, &log, &ayarlar).unwrap();
        let mut cikti = Vec::new();
        metin_yaz(&mut cikti, &ozet).unwrap();
        let rapor = String::from_utf8(cikti).unwrap();
        let beklenen = r#"
Ani artışlar (WARNING ve üstü, 1 dakikalık 241 dilim, puan > 3,5): 1
  zirve  olağan   puan  seviye     ilk             son             kalıp
      8       0    5,4  ERROR      10-24 23:59:00  10-25 00:00:07  Kuyruk dolu
                                                                   artış: 10-24 23:59–10-25 00:01 arası 2 dilimde 16 kayıt, zirve 10-24 23:59–10-25 00:00
"#;
        assert!(rapor.ends_with(beklenen), "{rapor}");
    }

    #[test]
    fn dilim_araligi_ve_ani_artis_aciklamasi() {
        let zaman = Zaman::kayittan;
        let (bas, son) = (zaman("2026-10-24 16:10:00"), zaman("2026-10-24 16:20:00"));
        assert_eq!(dilim_araligi(bas, son, true), "16:10–16:20");
        assert_eq!(dilim_araligi(bas, son, false), "10-24 16:10–16:20");
        assert_eq!(
            dilim_araligi(
                zaman("2026-10-24 23:50:00"),
                zaman("2026-10-25 00:10:00"),
                false
            ),
            "10-24 23:50–10-25 00:10"
        );

        let mut artis = AniArtis {
            satir: 0,
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
            ani_artis_aciklamasi(&artis, true),
            "16:10–16:20 arası 6 kayıt"
        );
        // Birden çok dilim: zirve de yazılır
        artis.bitis = zaman("2026-10-24 16:40:00");
        artis.dilim_sayisi = 3;
        artis.kayit_sayisi = 25;
        artis.zirve_baslangic = zaman("2026-10-24 16:20:00");
        artis.zirve_bitis = zaman("2026-10-24 16:30:00");
        assert_eq!(
            ani_artis_aciklamasi(&artis, true),
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
        assert_eq!(zaman_goster("2026-10-24 14:03:11", true), "14:03:11");
        assert_eq!(
            zaman_goster("2026-10-24T14:03:11.123456+03:00", false),
            "10-24 14:03:11"
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
        let log = kayitlari_ayir(&metin);
        let analiz = kaliplari_cikar(&log, &RaporAyarlari::default());

        assert_eq!(analiz.satirlar.len(), 1);
        let satir = &analiz.satirlar[0];
        assert_eq!(satir.adet, 3);
        assert_eq!(satir.seviye, Seviye::Error);
        assert_eq!(satir.ilk, "2026-10-24 10:00:01");
        assert_eq!(satir.son, "2026-10-24 10:00:09");
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
        let log = kayitlari_ayir(&metin);

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
            let analiz = kaliplari_cikar(&log, &ayarlar);
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
        let log = kayitlari_ayir(metin);
        let analiz = kaliplari_cikar(&log, &RaporAyarlari::default());

        assert_eq!(analiz.kalip_sayisi, 1);
        let satirlar: Vec<(usize, Option<String>)> = analiz
            .satirlar
            .iter()
            .map(|satir| (satir.adet, satir.parmak_izi.map(|iz| iz.to_string())))
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
        let log = kayitlari_ayir(ORNEK_LOG);
        let analiz = kaliplari_cikar(&log, &RaporAyarlari::default());

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
            (ilk.ilk, ilk.son),
            ("2026-10-24 17:05:13", "2026-10-24 17:06:45")
        );
        assert!(ilk.ornek.contains("10.0.3.12:6379"));
        assert_eq!(
            ilk.parmak_izi.map(|iz| iz.to_string()).as_deref(),
            Some("app/Http/Middleware/HizSiniri.php:22 (RedisException)")
        );

        // Aynı PHP mesajı iki ayrı denetleyiciden geliyor: iki ayrı hata
        let konumlar: Vec<String> = analiz
            .satirlar
            .iter()
            .filter(|satir| satir.kalip == r#"Attempt to read property "ad" on null"#)
            .filter_map(|satir| satir.parmak_izi.map(|iz| iz.to_string()))
            .collect();
        assert_eq!(
            konumlar,
            [
                "app/Http/Controllers/KategoriController.php:54 (ErrorException)",
                "app/Http/Controllers/UrunController.php:72 (ErrorException)",
            ]
        );

        // 15:00'teki deploy'dan hemen sonra başlayan hata
        let kargo = analiz
            .satirlar
            .iter()
            .find(|satir| satir.kalip.contains("kargo_ucreti"))
            .unwrap();
        assert_eq!(kargo.adet, 4);
        assert_eq!(kargo.ilk, "2026-10-24 15:02:11");

        // 16:10–16:16 arasındaki ödeme zaman aşımı patlaması
        let curl = analiz
            .satirlar
            .iter()
            .find(|satir| satir.kalip.starts_with("cURL error"))
            .unwrap();
        assert_eq!(curl.adet, 6);
        assert_eq!(
            (curl.ilk, curl.son),
            ("2026-10-24 16:10:02", "2026-10-24 16:16:02")
        );
    }

    #[test]
    fn ozet_ornek_log_icin_dogru_yazilir() {
        let log = kayitlari_ayir(ORNEK_LOG);

        let metin = metin_raporu(&log, &RaporAyarlari::default());

        assert!(metin.starts_with("ornek.log: 203 satır\n"));
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
            "\n                                       konum: app/Http/Middleware/HizSiniri.php:22 (RedisException)\n"
        ));
        // 24 satırın 10'u gösterildi; kalanlar söylenir
        assert!(metin.ends_with("\n  … 14 satır daha; tümünü görmek için --hepsi\n"));
    }

    #[test]
    fn ilk_ve_hepsi_tablonun_uzunlugunu_belirler() {
        let log = kayitlari_ayir(ORNEK_LOG);

        let ilk_uc = metin_raporu(
            &log,
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
            &log,
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
    fn ozet_sayilari_ve_gosterilecek_satirlar() {
        let log = kayitlari_ayir(ORNEK_LOG);
        let ozet = Ozet::hesapla(
            Path::new("ornek.log"),
            ORNEK_LOG,
            &log,
            &RaporAyarlari::default(),
        )
        .unwrap();

        assert_eq!(ozet.satir_sayisi, 203);
        assert_eq!(ozet.kayit_sayisi, 59);
        assert!(ozet.tek_gun());
        assert_eq!(ozet.seviyeler[0], (Seviye::Critical, 2));
        assert_eq!(ozet.seviyeler.last(), Some(&(Seviye::Debug, 3)));
        assert_eq!(ozet.gosterilecek_satirlar().len(), 10);
        assert_eq!(ozet.gosterilmeyen(), 14);
    }

    /// Metin raporunu ekran yerine bir bayt dizisine yazar ve metin olarak döndürür.
    fn metin_raporu(log: &Log<'_>, ayarlar: &RaporAyarlari) -> String {
        let ozet = Ozet::hesapla(Path::new("ornek.log"), ORNEK_LOG, log, ayarlar).unwrap();
        let mut cikti = Vec::new();
        metin_yaz(&mut cikti, &ozet).unwrap();
        String::from_utf8(cikti).unwrap()
    }
}
