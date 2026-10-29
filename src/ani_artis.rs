//! Ani artışlar: her kalıbın sıklığı dilim dilim izlenir, olağan seviyesinden sıçrayan anlar
//! bulunur.
//!
//! Karşılaştırmanın aksine log tek bir anda bölünmez. Logun kapsadığı süre eşit uzunlukta
//! dilimlere ayrılır (`--ani-artis 1`: dakika dakika) ve her kalıbın her dilimde kaç kaydı
//! olduğu sayılır. Kalıbın olağan seviyesi bu sayıların medyanı, olağan dalgalanması MAD'ıdır
//! (sayıların medyandan mutlak sapmalarının medyanı). Her dilim bir puan alır: olağan seviyeden
//! ne kadar uzakta olduğu. Bu, Iglewicz ve Hoaglin'in (1993) değiştirilmiş z puanıdır:
//!
//! ```text
//! puan = 0,6745 × (sayı − medyan) / MAD
//! ```
//!
//! Puanı 3,5'i geçen dilim olağanın dışındadır: bir sıçramadır. Bir ani artış, sıçrayan dilimin
//! çevresinde sayının olağanın (medyanın) üstünde kaldığı art arda dilimlerin hepsidir. İki
//! eşik kullanılır: artışı sıçrama başlatır, olağana dönüş bitirir. Böylece dakikada 5–8 kayıt
//! yazan bir arıza, arada eşiğin altında kalan dakikalar yüzünden parçalara bölünmez.
//!
//! Yalnızca WARNING ve üstündeki kayıtlar sayılır. INFO ve DEBUG kayıtları çoğu zaman trafikle
//! birlikte gün içinde inip çıkar; tek bir olağan seviye bunu açıklayamaz ve yoğun saatlerin
//! her dakikası sıçrama gibi görünür. Seviyeye kalıp satırının değil her kaydın kendisine
//! bakılır: Drain tek kelimesi farklı iki mesajı birleştirebilir (`Kullanıcı 7 giriş yaptı`
//! INFO, `Kullanıcı 7 giriş yapamadı` WARNING). Satırın seviyesi en yükseğidir; ona bakılsaydı
//! INFO trafiğinin her yoğun anı bir WARNING artışı sayılırdı. Artışın seviyesi, örneği, ilk ve
//! son zamanı da yalnızca sayılan kayıtlardandır.
//!
//! Kayıtlar depodaki sütunlardan bir kez okunur; her satırın her dolu dilimi için kayıt sayısı,
//! ilk ve son kayıt ve en yüksek seviye toplanır. Artışın örneği, ilk kaydının mesajıdır: o kayıt
//! bir alt grubun ilk kaydıysa mesajı depodadır, değilse girdiden yeniden okunur. Yeniden
//! okunamazsa (girdi bu arada değiştiyse) kaydın alt grubunun örneği yazılır: maskeli mesajı,
//! seviyesi ve istisnası aynı olan ilk kaydın mesajı.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::io;

use crate::depo::Depo;
use crate::kayit::Seviye;
use crate::okuyucu::{OrnekIstegi, OrnekKaynagi};
use crate::rapor::KalipAnalizi;
use crate::stats::median_and_mad;
use crate::zaman::Zaman;

/// Sıçrama sayılmak için puanın geçmesi gereken eşik (Iglewicz ve Hoaglin, 1993).
pub const ESIK: f64 = 3.5;
/// MAD'ı standart sapma ölçeğine çeviren çarpan: standart normal dağılımın üçüncü çeyreği,
/// Φ⁻¹(0,75). Normal dağılımda MAD ≈ 0,6745 σ olduğundan puan, sayının olağandan kaç standart
/// sapma uzakta olduğunun dayanıklı bir tahminidir.
pub const CARPAN: f64 = 0.6745;
/// MAD bundan küçükse bu kabul edilir: bir kayıt. Seyrek bir kalıbın dilimlerinin yarısından
/// fazlası boş olduğunda MAD 0 çıkar ve kaydı olan her dilim sonsuz puan alırdı. Sayılar tam
/// sayı olduğu için bir kayıttan küçük bir dalgalanma zaten ölçülemez.
pub const EN_AZ_MAD: f64 = 1.0;
/// Olağan seviyeyi ölçmek için log en az bu kadar dilim sürmeli.
pub const EN_AZ_DILIM: usize = 10;
/// Bir dilim en fazla bir gün olabilir.
pub const EN_UZUN_DILIM: u16 = 1_440;
/// Ani artış aranan en düşük seviye.
pub const EN_DUSUK_SEVIYE: Seviye = Seviye::Warning;

/// Bir kalıbın ani artışı: sayısı olağanın üstünde olan art arda dilimler; en az biri sıçramış.
#[derive(Debug, Clone, PartialEq)]
pub struct AniArtis {
    /// Satırın `KalipAnalizi::satirlar` içindeki sırası.
    pub satir: usize,
    /// Artıştaki kayıtların en yüksek seviyesi.
    pub seviye: Seviye,
    /// Artıştaki ilk kaydın maskelenmemiş mesajı. Satırın örneği bir INFO kaydı olabilir; bu
    /// her zaman sayılan bir kayıttır.
    pub ornek: String,
    /// Kalıbın olağan seviyesi: dilim başına kayıt sayısının medyanı.
    pub olagan: f64,
    /// Kalıbın olağan dalgalanması: sayıların medyandan mutlak sapmalarının medyanı.
    pub mad: f64,
    /// Artışın ilk diliminin başı ve son diliminin sonu.
    pub baslangic: Zaman,
    pub bitis: Zaman,
    pub dilim_sayisi: usize,
    pub kayit_sayisi: usize,
    /// Artıştaki ilk ve son kaydın zamanı.
    pub ilk: Zaman,
    pub son: Zaman,
    /// En çok kaydın düştüğü dilim (eşitlikte ilki): başı, sonu ve kayıt sayısı.
    pub zirve_baslangic: Zaman,
    pub zirve_bitis: Zaman,
    pub zirve_adet: usize,
    /// Zirve dilimin puanı; artışın en yüksek puanı.
    pub puan: f64,
}

/// Bütün logun ani artışları.
#[derive(Debug)]
pub struct AniArtislar {
    /// Bir dilimin uzunluğu, dakika.
    pub dilim_dakika: u16,
    /// İzlenen süre: ilk dilimin başı ve son dilimin sonu. Logda kayıt yoksa None.
    pub aralik: Option<(Zaman, Zaman)>,
    /// Kayıtlar filtrelendi mi? Kayıt yoksa nedeni böyle söylenir.
    pub filtreli: bool,
    pub dilim_sayisi: usize,
    /// Puanı en yüksek olan başta. Log `EN_AZ_DILIM`'den kısaysa boştur.
    pub artislar: Vec<AniArtis>,
}

/// Bir kalıbın kaydı olan bir dilimi.
#[derive(Debug, Clone)]
struct DoluDilim {
    /// 1970-01-01'den bu yana kaçıncı dilim olduğu.
    numara: i64,
    adet: usize,
    /// Dilimdeki ilk ve son kaydın zamanı, 1970'ten bu yana saniye. Aynı anda birden çok kayıt
    /// varsa ilk, logda önce gelenidir.
    ilk: i64,
    son: i64,
    /// Dilimdeki ilk kaydın sıra numarası ve alt grubu.
    ilk_kayit: usize,
    ilk_alt_grup: u32,
    /// Dilimdeki kayıtların en yüksek seviyesi.
    seviye: Seviye,
}

impl DoluDilim {
    fn new(numara: i64, saniye: i64, kayit: usize, alt_grup: u32, seviye: Seviye) -> DoluDilim {
        DoluDilim {
            numara,
            adet: 1,
            ilk: saniye,
            son: saniye,
            ilk_kayit: kayit,
            ilk_alt_grup: alt_grup,
            seviye,
        }
    }

    /// Kaydı dilime katar. Kayıtlar logdaki sırayla geldiği için zamanı ilkininkiyle aynı olan
    /// bir kayıt ilk olamaz.
    fn ekle(&mut self, saniye: i64, kayit: usize, alt_grup: u32, seviye: Seviye) {
        self.adet += 1;
        if saniye < self.ilk {
            self.ilk = saniye;
            self.ilk_kayit = kayit;
            self.ilk_alt_grup = alt_grup;
        }
        self.son = self.son.max(saniye);
        self.seviye = self.seviye.max(seviye);
    }
}

impl AniArtislar {
    /// Logun süresini `dilim_dakika` uzunluğunda dilimlere bölüp her kalıbın ani artışlarını
    /// bulur. `ornekler`: artışın ilk kaydının mesajı depoda yoksa girdiyi yeniden okuyan kaynak.
    pub fn hesapla(
        depo: &Depo,
        analiz: &KalipAnalizi,
        dilim_dakika: u16,
        ornekler: &mut dyn OrnekKaynagi,
    ) -> io::Result<AniArtislar> {
        let dilim_saniye = i64::from(dilim_dakika) * 60;
        // Her kaydın dilimi: 1970-01-01 00:00'dan bu yana kaçıncı dilimde olduğu. Böylece
        // dilimler saat başlarına hizalanır: 10 dakikalık dilimler 16:00, 16:10, 16:20... diye
        // başlar. `div_euclid` 1970'ten önceki (eksi) saniyelerde de aşağı yuvarlar.
        let dilim = |saniye: i64| saniye.div_euclid(dilim_saniye);
        let dilim_basi = |numara: i64| Zaman::saniyeden(numara * dilim_saniye);

        let mut sonuc = AniArtislar {
            dilim_dakika,
            aralik: None,
            filtreli: depo.filtre().is_some(),
            dilim_sayisi: 0,
            artislar: Vec::new(),
        };
        let (Some(ilk), Some(son)) = (depo.en_eski(), depo.en_yeni()) else {
            return Ok(sonuc);
        };
        let (ilk_dilim, son_dilim) = (dilim(ilk.saniye()), dilim(son.saniye()));
        sonuc.aralik = Some((dilim_basi(ilk_dilim), dilim_basi(son_dilim + 1)));
        // 64 bitlik sistemlerde her zaman sığar
        sonuc.dilim_sayisi = usize::try_from(son_dilim - ilk_dilim + 1).unwrap_or(usize::MAX);
        if sonuc.dilim_sayisi < EN_AZ_DILIM {
            return Ok(sonuc);
        }

        // Her satırın WARNING ve üstündeki kayıtlarının dolu dilimleri. Kayıt sütunlardan logdaki
        // sırayla okunur. Seviyeye kalıp satırının değil kaydın kendisine (alt grubuna) bakılır.
        let mut dilimler: HashMap<(usize, i64), DoluDilim> = HashMap::new();
        for (kayit, (saniye, alt_grup)) in depo.kayitlar().enumerate() {
            let seviye = depo.alt_grup_seviyesi(alt_grup);
            if seviye < EN_DUSUK_SEVIYE {
                continue;
            }
            let satir = analiz.alt_grup_satiri[alt_grup as usize];
            let numara = dilim(saniye);
            dilimler
                .entry((satir, numara))
                .and_modify(|dolu| dolu.ekle(saniye, kayit, alt_grup, seviye))
                .or_insert_with(|| DoluDilim::new(numara, saniye, kayit, alt_grup, seviye));
        }
        // Satır satır, dilimlerin sırasıyla. Satırda böyle bir kayıt yoksa (bütün kayıtları
        // INFO) satırın dilimi de yoktur.
        let mut satir_dilimleri: Vec<Vec<DoluDilim>> = vec![Vec::new(); analiz.satirlar.len()];
        for ((satir, _), dolu) in dilimler {
            satir_dilimleri[satir].push(dolu);
        }

        // Artışların ilk kayıtları: örnekleri en sonda topluca bulunur
        let mut ornek_kayitlari = Vec::new();
        for (sira, dolu) in satir_dilimleri.iter_mut().enumerate() {
            if dolu.is_empty() {
                continue;
            }
            dolu.sort_unstable_by_key(|dilim| dilim.numara);
            let sayilar: Vec<usize> = dolu.iter().map(|dilim| dilim.adet).collect();
            // Kaydı olmayan dilimler 0 sayılır. Dizi boş olamaz: log en az `EN_AZ_DILIM` dilim.
            let olagan = median_and_mad(sonuc.dilim_sayisi - dolu.len(), &sayilar)
                .expect("dilim dizisi boş değil");
            let puan =
                |adet: usize| CARPAN * (adet as f64 - olagan.median) / olagan.mad.max(EN_AZ_MAD);
            // Olağanın üstündeki dilimler: sayısı medyanı geçenler. Boş bir dilim olağanın
            // üstünde olamaz (0, medyanı geçemez); bu yüzden yalnızca dolulara bakılır.
            let ustundekiler: Vec<&DoluDilim> = dolu
                .iter()
                .filter(|dilim| dilim.adet as f64 > olagan.median)
                .collect();
            // Olağanın üstündeki art arda dilimler bir gruptur: bir sonraki, bir öncekinin hemen
            // arkasındaki dilimse aynı gruba girer
            for grup in ustundekiler.chunk_by(|onceki, sonraki| sonraki.numara == onceki.numara + 1)
            {
                // `max_by_key` eşitlikte sonuncuyu verir; ters çevrilmiş anahtarla `min_by_key`
                // ilkini
                let zirve = grup
                    .iter()
                    .min_by_key(|dilim| Reverse(dilim.adet))
                    .expect("`chunk_by` boş grup vermez");
                // En yüksek puan zirvenindir: zirve sıçramadıysa gruptaki hiçbir dilim sıçramadı
                if puan(zirve.adet) <= ESIK {
                    continue;
                }
                let (ilk, son) = (grup[0], grup[grup.len() - 1]);
                ornek_kayitlari.push((ilk.ilk_kayit, ilk.ilk_alt_grup));
                sonuc.artislar.push(AniArtis {
                    satir: sira,
                    seviye: grup
                        .iter()
                        .map(|dilim| dilim.seviye)
                        .max()
                        .expect("`chunk_by` boş grup vermez"),
                    // Örnekler aşağıda bulunur
                    ornek: String::new(),
                    olagan: olagan.median,
                    mad: olagan.mad,
                    baslangic: dilim_basi(ilk.numara),
                    bitis: dilim_basi(son.numara + 1),
                    dilim_sayisi: grup.len(),
                    kayit_sayisi: grup.iter().map(|dilim| dilim.adet).sum(),
                    ilk: Zaman::saniyeden(ilk.ilk),
                    son: Zaman::saniyeden(son.son),
                    zirve_baslangic: dilim_basi(zirve.numara),
                    zirve_bitis: dilim_basi(zirve.numara + 1),
                    zirve_adet: zirve.adet,
                    puan: puan(zirve.adet),
                });
            }
        }
        let mesajlar = mesajlari_bul(&ornek_kayitlari, depo, ornekler)?;
        for (artis, (kayit, alt_grup)) in sonuc.artislar.iter_mut().zip(&ornek_kayitlari) {
            // Kayıt yeniden okunamadıysa (girdi bu arada değiştiyse) alt grubunun örneği
            artis.ornek = match mesajlar.get(kayit) {
                Some(mesaj) => mesaj.clone(),
                None => depo.alt_gruplar()[*alt_grup as usize].ornek().to_owned(),
            };
        }
        // Puanı en yüksek olan başta; eşitse önce başlayan. Sıralama kararlıdır: ikisi de
        // eşitse kalıp satırlarının sırası (--siralama) korunur.
        sonuc.artislar.sort_by(|a, b| {
            b.puan
                .total_cmp(&a.puan)
                .then(a.baslangic.cmp(&b.baslangic))
        });
        Ok(sonuc)
    }

    /// Log, olağan seviyeyi ölçmeye yetecek kadar dilim sürüyor mu?
    pub fn yeterli(&self) -> bool {
        self.dilim_sayisi >= EN_AZ_DILIM
    }
}

/// Kayıtların maskelenmemiş mesajları; kayıtlar (sıra numarası, alt grup) olarak verilir. Kayıt
/// bir alt grubun ilk kaydıysa mesajı depodadır; değilse girdiden yeniden okunur. Bulunamayan
/// kayıt sonuçta yer almaz.
fn mesajlari_bul(
    kayitlar: &[(usize, u32)],
    depo: &Depo,
    ornekler: &mut dyn OrnekKaynagi,
) -> io::Result<HashMap<usize, String>> {
    let depodakiler: HashMap<usize, &str> = depo
        .alt_gruplar()
        .iter()
        .map(|alt_grup| (alt_grup.ilk_kayit(), alt_grup.ornek()))
        .collect();
    let okunacaklar: Vec<OrnekIstegi<'_>> = kayitlar
        .iter()
        .filter(|(kayit, _)| !depodakiler.contains_key(kayit))
        .map(|&(sira, alt_grup)| {
            let alt_grup = &depo.alt_gruplar()[alt_grup as usize];
            OrnekIstegi {
                sira,
                maskeli: depo.maskeli_mesaj(alt_grup.maskeli()),
                seviye: alt_grup.seviye(),
            }
        })
        .collect();
    let mut mesajlar = if okunacaklar.is_empty() {
        HashMap::new()
    } else {
        ornekler.mesajlar(&okunacaklar)?
    };
    for (kayit, _) in kayitlar {
        if let Some(mesaj) = depodakiler.get(kayit) {
            mesajlar.insert(*kayit, (*mesaj).to_owned());
        }
    }
    Ok(mesajlar)
}

/// Rapor ve Markdown testlerinin seviyesi karışık logu. "Kuyruk <SAYI> dolu" kalıbında on
/// dakikada bir WARNING (10:00:30'dan başlayarak), 10:05'te bir CRITICAL ve iki dilimlik bir
/// artış: 11:00'de 8 WARNING ve bir ERROR, 11:01'de 4 WARNING. Satırın seviyesi CRITICAL,
/// örneği ilk kaydı ("Kuyruk 2 dolu"); artışın seviyesi ERROR, örneği "Kuyruk 3 dolu".
#[cfg(test)]
pub fn karisik_seviyeli_log() -> String {
    let mut satirlar = vec!["[2026-10-24 10:05:00] local.CRITICAL: Kuyruk 1 dolu".to_string()];
    let ilk = Zaman::kayittan("2026-10-24 10:00:30").saniye();
    for dakika in (0..120).step_by(10) {
        let an = Zaman::saniyeden(ilk + dakika * 60);
        satirlar.push(format!("[{an}] local.WARNING: Kuyruk 2 dolu"));
    }
    for saniye in (0..40).step_by(5) {
        satirlar.push(format!(
            "[2026-10-24 11:00:{saniye:02}] local.WARNING: Kuyruk 3 dolu"
        ));
    }
    satirlar.push("[2026-10-24 11:00:50] local.ERROR: Kuyruk 4 dolu".to_string());
    for saniye in (0..20).step_by(5) {
        satirlar.push(format!(
            "[2026-10-24 11:01:{saniye:02}] local.WARNING: Kuyruk 5 dolu"
        ));
    }
    // Zamana göre sırala; aynı saniyedekiler mesajlarına göre
    satirlar.sort();
    satirlar.join("\n")
}

/// Rapor, JSON ve Markdown testlerinin ortak logu: iki saat (10:00–11:59) ve üç ani artış.
///
/// - "Kuyruk dolu" (ERROR): on dakikada bir kayıt (olağan 0); 10:30'da 6, 10:31'de 9, 10:32'de 7
///   kayıt: üç dilimlik bir artış. 11:30'da 6 kayıt: ikinci artış.
/// - "Yavaş istek" (WARNING): her dakika 2 kayıt (olağan 2); 10:45'te 9.
///
/// `#[cfg(test)]`: yalnızca testler derlenirken var olur, programın içine girmez.
#[cfg(test)]
pub fn deneme_logu() -> String {
    let saat_10 = Zaman::kayittan("2026-10-24 10:00:00").saniye();
    let mut satirlar = Vec::new();
    for dakika in 0..120 {
        let yavas = if dakika == 45 { 9 } else { 2 };
        let kuyruk = match dakika {
            30 | 90 => 6,
            31 => 9,
            32 => 7,
            _ if dakika % 10 == 5 => 1,
            _ => 0,
        };
        for (adet, ilk_saniye, seviye, mesaj) in [
            (yavas, 10, "WARNING", "Yavaş istek"),
            (kuyruk, 30, "ERROR", "Kuyruk dolu"),
        ] {
            for saniye in ilk_saniye..ilk_saniye + adet {
                let an = Zaman::saniyeden(saat_10 + dakika * 60 + saniye);
                satirlar.push(format!("[{an}] local.{seviye}: {mesaj}"));
            }
        }
    }
    satirlar.join("\n")
}

#[cfg(test)]
mod testler {
    use super::*;
    use crate::depo;
    use crate::okuyucu::YenidenOkuyucu;
    use crate::rapor::{RaporAyarlari, kaliplari_cikar};
    use std::io::Cursor;

    const ORNEK_LOG: &str = include_str!("../ornekler/laravel.log");

    /// Saat ve mesajdan bir log metni: her eleman (zaman, seviye, mesaj, kaç kez, aralık sn).
    /// Aynı mesaj `kac` kez, `aralik` saniye arayla yazılır. Tarih hep 2026-10-24'tür ama saat
    /// 24'ü geçerse ertesi güne taşar.
    fn log_metni(kayitlar: &[(&str, &str, &str, u32, u32)]) -> String {
        let mut satirlar = Vec::new();
        for &(baslangic, seviye, mesaj, kac, aralik) in kayitlar {
            let zaman = Zaman::kayittan(&format!("2026-10-24 {baslangic}"));
            for tekrar in 0..kac {
                let an = Zaman::saniyeden(zaman.saniye() + i64::from(tekrar * aralik));
                satirlar.push(format!("[{an}] local.{seviye}: {mesaj}"));
            }
        }
        satirlar.join("\n")
    }

    /// Logun ani artışları ve kalıp satırlarının metinleri (artışlardaki `satir` bunlara
    /// bakar).
    fn hesapla(metin: &str, dakika: u16) -> (AniArtislar, Vec<String>) {
        let depo = depo::metinden(metin);
        let analiz = kaliplari_cikar(&depo, &RaporAyarlari::default());
        let kaliplar = analiz
            .satirlar
            .iter()
            .map(|satir| satir.kalip.clone())
            .collect();
        (artislar(metin, &depo, &analiz, dakika), kaliplar)
    }

    /// Ani artışlar; örnekler gerekirse metinden yeniden okunur.
    fn artislar(metin: &str, depo: &Depo, analiz: &KalipAnalizi, dakika: u16) -> AniArtislar {
        let mut kaynak = YenidenOkuyucu::new(
            Cursor::new(metin.as_bytes()),
            depo.dizin().clone(),
            depo.filtre().copied(),
            depo.saat_dilimi(),
        );
        AniArtislar::hesapla(depo, analiz, dakika, &mut kaynak).unwrap()
    }

    /// Bir artışın kalıbı, zirvesi ve dilim aralığı: ("Kuyruk dolu", 12, "11:00", "11:02")
    fn ozetle(sonuc: &(AniArtislar, Vec<String>)) -> Vec<(String, usize, String, String)> {
        let (ani, kaliplar) = sonuc;
        ani.artislar
            .iter()
            .map(|artis| {
                (
                    kaliplar[artis.satir].clone(),
                    artis.zirve_adet,
                    artis.baslangic.to_string()[11..16].to_string(),
                    artis.bitis.to_string()[11..16].to_string(),
                )
            })
            .collect()
    }

    /// İki saatlik bir logun iki ucu: dilimleri INFO kayıtları belirler, ani artışa katılmaz.
    const IKI_SAAT: [(&str, &str, &str, u32, u32); 2] = [
        ("10:00:00", "INFO", "Gece görevi başladı", 1, 0),
        ("11:59:59", "INFO", "Gece görevi başladı", 1, 0),
    ];

    #[test]
    fn patlama_bulunur_ve_bilgileri_dogru() {
        let mut kayitlar = IKI_SAAT.to_vec();
        kayitlar.extend([
            // Arka plan: on dakikada bir kayıt. Dilimlerin çoğu boş: medyan 0, MAD 0.
            ("10:05:00", "ERROR", "Ödeme servisi yanıt vermedi", 12, 600),
            // Patlama: 11:00'de 8, 11:01'de 12, 11:02'de 5 kayıt
            ("11:00:10", "ERROR", "Ödeme servisi yanıt vermedi", 8, 6),
            ("11:01:00", "ERROR", "Ödeme servisi yanıt vermedi", 12, 5),
            ("11:02:00", "ERROR", "Ödeme servisi yanıt vermedi", 5, 10),
        ]);
        let (ani, _) = hesapla(&log_metni(&kayitlar), 1);

        assert_eq!(ani.dilim_sayisi, 120);
        let (bas, son) = ani.aralik.unwrap();
        assert_eq!(bas.to_string(), "2026-10-24 10:00:00");
        assert_eq!(son.to_string(), "2026-10-24 12:00:00");

        // MAD tabanı 1 kayıt: puan = 0,6745 × sayı. 8 ve 12 eşiği geçer (5,4 ve 8,1); 5 geçmez
        // (3,4) ama olağanın (0) üstündedir, artışın parçasıdır. 11:03 boş: artış biter.
        assert_eq!(ani.artislar.len(), 1, "{:?}", ani.artislar);
        let artis = &ani.artislar[0];
        assert_eq!((artis.olagan, artis.mad), (0.0, 0.0));
        assert_eq!(artis.baslangic.to_string(), "2026-10-24 11:00:00");
        assert_eq!(artis.bitis.to_string(), "2026-10-24 11:03:00");
        assert_eq!((artis.dilim_sayisi, artis.kayit_sayisi), (3, 25));
        assert_eq!(artis.ilk.to_string(), "2026-10-24 11:00:10");
        assert_eq!(artis.son.to_string(), "2026-10-24 11:02:40");
        assert_eq!(artis.zirve_baslangic.to_string(), "2026-10-24 11:01:00");
        assert_eq!(artis.zirve_bitis.to_string(), "2026-10-24 11:02:00");
        assert_eq!(artis.zirve_adet, 12);
        assert_eq!(artis.puan, CARPAN * 12.0);
    }

    #[test]
    fn seyrek_kalip_bir_dilimde_en_az_alti_kayitla_sicrar() {
        let mut kayitlar = IKI_SAAT.to_vec();
        kayitlar.extend([
            // 0,6745 × 5 = 3,37: eşiğin altında
            ("10:30:00", "ERROR", "Kuyruk dolu", 5, 1),
            // 0,6745 × 6 = 4,05: eşiğin üstünde
            ("10:40:00", "WARNING", "Disk dolmak üzere", 6, 1),
        ]);
        let sonuc = hesapla(&log_metni(&kayitlar), 1);
        assert_eq!(
            ozetle(&sonuc),
            [(
                "Disk dolmak üzere".to_string(),
                6,
                "10:40".to_string(),
                "10:41".to_string()
            )]
        );
        assert_eq!(sonuc.0.artislar[0].puan, CARPAN * 6.0);
    }

    #[test]
    fn olagan_seviye_sifir_olmayan_kalip() {
        let mut kayitlar = IKI_SAAT.to_vec();
        kayitlar.extend([
            // İki saat boyunca her dakika 2 kayıt: medyan 2, MAD 0 → tabanı 1
            ("10:00:10", "WARNING", "Yavaş istek", 120, 60),
            ("10:00:40", "WARNING", "Yavaş istek", 120, 60),
            // 10:29'da 3, 10:30'da 9, 10:31'de 4 kayıt. 10:30'un puanı 0,6745 × 7 = 4,7: sıçrar.
            // 3 ve 4 olağanın (2) üstünde: artışa katılır. 10:32'de yine 2: artış biter.
            ("10:29:01", "WARNING", "Yavaş istek", 1, 1),
            ("10:30:01", "WARNING", "Yavaş istek", 7, 1),
            ("10:31:01", "WARNING", "Yavaş istek", 2, 1),
            // 10:50'de 7 kayıt: olağanın üstünde ama puanı 3,4; sıçramadığı için artış değil
            ("10:50:01", "WARNING", "Yavaş istek", 5, 1),
        ]);
        let (ani, _) = hesapla(&log_metni(&kayitlar), 1);
        assert_eq!(ani.artislar.len(), 1, "{:?}", ani.artislar);
        let artis = &ani.artislar[0];
        assert_eq!((artis.olagan, artis.mad), (2.0, 0.0));
        assert_eq!(artis.baslangic.to_string(), "2026-10-24 10:29:00");
        assert_eq!(artis.bitis.to_string(), "2026-10-24 10:32:00");
        assert_eq!((artis.dilim_sayisi, artis.kayit_sayisi), (3, 16));
        assert_eq!(artis.zirve_baslangic.to_string(), "2026-10-24 10:30:00");
        assert_eq!(artis.zirve_adet, 9);
        assert_eq!(artis.puan, CARPAN * 7.0);
    }

    #[test]
    fn artis_olagana_donene_kadar_surer() {
        let mut kayitlar = IKI_SAAT.to_vec();
        kayitlar.extend([
            // Dakikada 7, 4, 5, 3, 8 kayıt: 4, 5 ve 3 eşiğin altında ama sıfırın üstünde. Arıza
            // beş dakika sürdü; parçalara bölünmez. 10:15 boş: artış biter.
            ("10:10:00", "ERROR", "Bağlantı zaman aşımı", 7, 1),
            ("10:11:00", "ERROR", "Bağlantı zaman aşımı", 4, 1),
            ("10:12:00", "ERROR", "Bağlantı zaman aşımı", 5, 1),
            ("10:13:00", "ERROR", "Bağlantı zaman aşımı", 3, 1),
            ("10:14:00", "ERROR", "Bağlantı zaman aşımı", 8, 1),
            // 10:16'da 2 kayıt: olağanın üstünde ama boş bir dakikayla ayrılmış
            ("10:16:00", "ERROR", "Bağlantı zaman aşımı", 2, 1),
            // Hiç sıçramayan bir grup: 10:40–10:44 arası her dakika 5
            ("10:40:00", "ERROR", "Bağlantı zaman aşımı", 25, 12),
        ]);
        let (ani, _) = hesapla(&log_metni(&kayitlar), 1);
        assert_eq!(ani.artislar.len(), 1, "{:?}", ani.artislar);
        let artis = &ani.artislar[0];
        assert_eq!(artis.baslangic.to_string(), "2026-10-24 10:10:00");
        assert_eq!(artis.bitis.to_string(), "2026-10-24 10:15:00");
        assert_eq!((artis.dilim_sayisi, artis.kayit_sayisi), (5, 27));
        // Zirve sonda: 8 kayıtlı 10:14
        assert_eq!(artis.zirve_baslangic.to_string(), "2026-10-24 10:14:00");
        assert_eq!(artis.puan, CARPAN * 8.0);
    }

    #[test]
    fn dalgalanma_buyukse_esik_de_buyur() {
        // Yarım saatlerde dakikada 1, 3, 5 ve 7 kayıt; yalnızca 10:45'te 14, 11:15'te 17. Medyan
        // 5, MAD 2: eşik 5 + 3,5 × 2 / 0,6745 ≈ 15,4. Seyrek bir kalıpta sıçrama olacak 14 burada
        // olağan dalgalanmanın içinde kalır (puan 3,0); 17 sıçrar (puan 4,0).
        let saat_10 = Zaman::kayittan("2026-10-24 10:00:00").saniye();
        let mut satirlar = vec![log_metni(&IKI_SAAT)];
        for dakika in 0..120 {
            let adet = match dakika {
                45 => 14,
                75 => 17,
                _ => 1 + 2 * (dakika / 30),
            };
            for kayit in 0..adet {
                let an = Zaman::saniyeden(saat_10 + dakika * 60 + kayit + 1);
                satirlar.push(format!("[{an}] local.ERROR: Sepet hesaplanamadı"));
            }
        }
        let (ani, _) = hesapla(&satirlar.join("\n"), 1);
        assert_eq!(ani.artislar.len(), 1, "{:?}", ani.artislar);
        let artis = &ani.artislar[0];
        assert_eq!((artis.olagan, artis.mad), (5.0, 2.0));
        assert_eq!(artis.zirve_adet, 17);
        assert_eq!(artis.puan, CARPAN * 12.0 / 2.0);
        assert_eq!(artis.baslangic.to_string(), "2026-10-24 11:15:00");
    }

    #[test]
    fn ara_veren_artislar_ayri_sayilir() {
        let mut kayitlar = IKI_SAAT.to_vec();
        kayitlar.extend([
            // 10:10 ve 10:11 art arda: tek artış. 10:12 boş, 10:13 yeniden: ikinci artış.
            ("10:10:00", "ERROR", "Bağlantı zaman aşımı", 7, 1),
            ("10:11:00", "ERROR", "Bağlantı zaman aşımı", 9, 1),
            ("10:13:00", "ERROR", "Bağlantı zaman aşımı", 6, 1),
            // Zirvede eşitlik: 11:20'de 7, 11:21'de 9, 11:22'de 9. Zirve ilk 9.
            ("11:20:00", "ERROR", "Bağlantı zaman aşımı", 7, 1),
            ("11:21:00", "ERROR", "Bağlantı zaman aşımı", 9, 1),
            ("11:22:00", "ERROR", "Bağlantı zaman aşımı", 9, 1),
        ]);
        let (ani, _) = hesapla(&log_metni(&kayitlar), 1);
        let araliklar: Vec<(String, String, usize, String)> = ani
            .artislar
            .iter()
            .map(|artis| {
                (
                    artis.baslangic.to_string()[11..16].to_string(),
                    artis.bitis.to_string()[11..16].to_string(),
                    artis.kayit_sayisi,
                    artis.zirve_baslangic.to_string()[11..16].to_string(),
                )
            })
            .collect();
        // Puan sırası: 9'lu iki artış önce (eşit puan, önce başlayan önce), sonra 6'lı
        assert_eq!(
            araliklar,
            [
                ("10:10".into(), "10:12".into(), 16, "10:11".into()),
                ("11:20".into(), "11:23".into(), 25, "11:21".into()),
                ("10:13".into(), "10:14".into(), 6, "10:13".into()),
            ]
        );
    }

    #[test]
    fn puani_yuksek_olan_once_gelir() {
        let mut kayitlar = IKI_SAAT.to_vec();
        kayitlar.extend([
            // Önce görülen ama küçük artış ve sonra görülen büyük artış
            ("10:20:00", "WARNING", "Disk dolmak üzere", 7, 1),
            (
                "11:40:00",
                "CRITICAL",
                "Veritabanı bağlantısı koptu tamamen",
                30,
                1,
            ),
            // Aynı puan: önce başlayan önce
            ("10:50:00", "ERROR", "Kuyruk dolu", 7, 1),
        ]);
        let sonuc = hesapla(&log_metni(&kayitlar), 1);
        let kaliplar: Vec<String> = ozetle(&sonuc).into_iter().map(|satir| satir.0).collect();
        assert_eq!(
            kaliplar,
            [
                "Veritabanı bağlantısı koptu tamamen",
                "Disk dolmak üzere",
                "Kuyruk dolu",
            ]
        );
    }

    #[test]
    fn info_ve_debug_kaliplarina_bakilmaz() {
        let mut kayitlar = IKI_SAAT.to_vec();
        kayitlar.extend([
            ("10:30:00", "INFO", "Kullanıcı giriş yaptı", 50, 1),
            ("10:40:00", "DEBUG", "Önbellekten okundu", 50, 1),
            ("10:50:00", "NOTICE", "Yapılandırma yenilendi şimdi", 50, 1),
            ("11:00:00", "WARNING", "Disk dolmak üzere", 50, 1),
        ]);
        let sonuc = hesapla(&log_metni(&kayitlar), 1);
        let kaliplar: Vec<String> = ozetle(&sonuc).into_iter().map(|satir| satir.0).collect();
        assert_eq!(kaliplar, ["Disk dolmak üzere"]);
    }

    /// 10:00'dan başlayan, `dakika_sayisi` dakikalık bir log. Her dakika `adetler(dakika)` kaç
    /// kayıt yazılacağını söyler: (INFO giriş, WARNING başarısız giriş). Bir dakikanın kayıtları
    /// birer saniye arayla yazılır.
    fn giris_logu(dakika_sayisi: i64, adetler: impl Fn(i64) -> (i64, i64)) -> String {
        let saat_10 = Zaman::kayittan("2026-10-24 10:00:00").saniye();
        let mut satirlar = Vec::new();
        for dakika in 0..dakika_sayisi {
            let (info, warning) = adetler(dakika);
            for (adet, seviye, mesaj) in [
                (info, "INFO", "Kullanıcı 8 giriş yaptı"),
                (warning, "WARNING", "Kullanıcı 7 giriş yapamadı"),
            ] {
                for saniye in 0..adet {
                    let an = Zaman::saniyeden(saat_10 + dakika * 60 + saniye);
                    satirlar.push(format!("[{an}] local.{seviye}: {mesaj}"));
                }
            }
        }
        satirlar.join("\n")
    }

    #[test]
    fn ayni_kaliptaki_info_kayitlari_sayilmaz() {
        // "giriş yaptı" (INFO) ve "giriş yapamadı" (WARNING) tek kelimeyle ayrılır; Drain ikisini
        // tek kalıpta birleştirir ve satırın seviyesi WARNING olur. 10:30'daki 30 kaydın hepsi
        // INFO: bir WARNING artışı değil. Tek WARNING 11:00'de.
        let metin = giris_logu(120, |dakika| match dakika {
            30 => (30, 0),
            60 => (2, 1),
            _ => (2, 0),
        });
        let (ani, kaliplar) = hesapla(&metin, 1);
        assert_eq!(kaliplar, ["Kullanıcı <SAYI> giriş <*>"]);
        assert!(ani.artislar.is_empty(), "{:?}", ani.artislar);
    }

    #[test]
    fn info_trafigi_warning_artisini_gizlemez() {
        // Aynı kalıpta dakikada 5 ile 25 arasında inip çıkan INFO trafiği ve on dakikada bir
        // WARNING. 11:00'de 8 WARNING: WARNING'lerin olağanı 0, sıçrama açık (puan 5,4).
        // INFO'lar da sayılsaydı olağan 19, dalgalanma 7 olurdu; 11:00'deki 13 kayıt olağanın
        // altında kalırdı.
        let metin = giris_logu(120, |dakika| {
            let info = 5 + 20 * (dakika % 2);
            let warning = match dakika {
                60 => 8,
                _ if dakika % 10 == 3 => 1,
                _ => 0,
            };
            (info, warning)
        });
        let (ani, kaliplar) = hesapla(&metin, 1);
        assert_eq!(kaliplar, ["Kullanıcı <SAYI> giriş <*>"]);
        assert_eq!(ani.artislar.len(), 1, "{:?}", ani.artislar);
        let artis = &ani.artislar[0];
        assert_eq!((artis.olagan, artis.mad), (0.0, 0.0));
        assert_eq!((artis.zirve_adet, artis.kayit_sayisi), (8, 8));
        assert_eq!(artis.baslangic.to_string(), "2026-10-24 11:00:00");
        assert_eq!(artis.puan, CARPAN * 8.0);
        // Örnek sayılan bir kayıttır; satırın ilk kaydı (bir INFO) değil
        assert_eq!(artis.ornek, "Kullanıcı 7 giriş yapamadı");
        assert_eq!(artis.seviye, Seviye::Warning);
    }

    #[test]
    fn artisin_seviyesi_ve_ornegi_kendi_kayitlarindan() {
        // Satırın seviyesi CRITICAL (10:05'teki tek kayıt), örneği ilk kaydı. Artışın ilk
        // diliminde 8 WARNING, 11:00:30'daki on dakikalık WARNING ve bir ERROR, ikincisinde 4
        // WARNING var: seviyesi iki dilimin en yükseği (ERROR), örneği ilk kaydı.
        let metin = karisik_seviyeli_log();
        let depo = depo::metinden(&metin);
        let analiz = kaliplari_cikar(&depo, &RaporAyarlari::default());
        assert_eq!(analiz.satirlar.len(), 1);
        assert_eq!(analiz.satirlar[0].seviye, Seviye::Critical);
        assert_eq!(analiz.satirlar[0].ornek, "Kuyruk 2 dolu");

        let ani = artislar(&metin, &depo, &analiz, 1);
        assert_eq!(ani.artislar.len(), 1, "{:?}", ani.artislar);
        let artis = &ani.artislar[0];
        assert_eq!(artis.seviye, Seviye::Error);
        assert_eq!(artis.ornek, "Kuyruk 3 dolu");
        assert_eq!(
            (artis.ilk.to_string(), artis.son.to_string()),
            (
                "2026-10-24 11:00:00".to_string(),
                "2026-10-24 11:01:15".to_string()
            )
        );
        assert_eq!(
            (artis.dilim_sayisi, artis.kayit_sayisi, artis.zirve_adet),
            (2, 14, 10)
        );
    }

    /// Hiçbir kaydı bulamayan örnek kaynağı: girdi değişmiş ya da yeniden okunamıyor.
    struct BulamayanKaynak;

    impl OrnekKaynagi for BulamayanKaynak {
        fn mesajlar(&mut self, _: &[OrnekIstegi<'_>]) -> io::Result<HashMap<usize, String>> {
            Ok(HashMap::new())
        }
    }

    #[test]
    fn yeniden_okunamayan_ornegin_yerine_alt_grubunki_yazilir() {
        // Kalıbın ilk kaydı INFO, artış ERROR kayıtlarından. Artışın ilk kaydı (Kuyruk 7) alt
        // grubunun ilk kaydı değil: yeniden okunur. Okunamazsa yerine ERROR alt grubunun örneği
        // (Kuyruk 2) yazılır; kalıbın örneği (INFO kaydı) değil.
        let mut satirlar = vec![
            "[2026-10-24 10:00:00] local.INFO: Kuyruk 1 dolu".to_string(),
            "[2026-10-24 10:05:00] local.ERROR: Kuyruk 2 dolu".to_string(),
        ];
        for numara in [7, 3, 9, 4, 8, 6, 5] {
            satirlar.push(format!(
                "[2026-10-24 11:00:00] local.ERROR: Kuyruk {numara} dolu"
            ));
        }
        satirlar.push("[2026-10-24 11:59:59] local.INFO: Başladı".to_string());
        let metin = satirlar.join("\n");
        let depo = depo::metinden(&metin);
        let analiz = kaliplari_cikar(&depo, &RaporAyarlari::default());
        assert_eq!(analiz.satirlar[0].ornek, "Kuyruk 1 dolu");

        let okunan = artislar(&metin, &depo, &analiz, 1);
        assert_eq!(okunan.artislar[0].ornek, "Kuyruk 7 dolu");
        let okunamayan = AniArtislar::hesapla(&depo, &analiz, 1, &mut BulamayanKaynak).unwrap();
        assert_eq!(okunamayan.artislar.len(), 1);
        assert_eq!(okunamayan.artislar[0].ornek, "Kuyruk 2 dolu");
    }

    #[test]
    fn sirasiz_dilimin_ornegi_en_erken_kaydin_alt_grubundan() {
        // Artışın dilimine önce bir ERROR kaydı yazılmış, sonra zamanı daha erken bir WARNING
        // kaydı (aynı kalıp, başka alt grup). Örnek en erken kayıttır; yeniden okunurken onun
        // alt grubunun maskeli mesajı ve seviyesiyle denetlenir.
        let mut satirlar = vec![
            "[2026-10-24 10:00:00] local.INFO: Başladı".to_string(),
            "[2026-10-24 10:05:00] local.ERROR: Kuyruk 1 dolu".to_string(),
            "[2026-10-24 10:06:00] local.WARNING: Kuyruk 2 dolu".to_string(),
            "[2026-10-24 11:00:30] local.ERROR: Kuyruk 7 dolu".to_string(),
            "[2026-10-24 11:00:05] local.WARNING: Kuyruk 8 dolu".to_string(),
        ];
        for numara in 10..16 {
            satirlar.push(format!(
                "[2026-10-24 11:00:40] local.ERROR: Kuyruk {numara} dolu"
            ));
        }
        satirlar.push("[2026-10-24 11:59:59] local.INFO: Başladı".to_string());
        let (ani, _) = hesapla(&satirlar.join("\n"), 1);
        assert_eq!(ani.artislar.len(), 1, "{:?}", ani.artislar);
        let artis = &ani.artislar[0];
        assert_eq!(artis.ornek, "Kuyruk 8 dolu");
        assert_eq!(artis.ilk.to_string(), "2026-10-24 11:00:05");
    }

    #[test]
    fn ayni_saniyedeki_kayitlardan_logda_once_gelen_ornektir() {
        // Artışın sekiz kaydı aynı saniyede; örnek, logda ilk yazılanı. O kayıt kalıbın ilk
        // kaydı değil (10:05'teki): mesajı girdiden yeniden okunur.
        let mut satirlar = vec![
            "[2026-10-24 10:00:00] local.INFO: Başladı".to_string(),
            "[2026-10-24 10:05:00] local.ERROR: Kuyruk 1 dolu".to_string(),
        ];
        for numara in [7, 3, 9, 4, 8, 2, 6, 5] {
            satirlar.push(format!(
                "[2026-10-24 11:00:00] local.ERROR: Kuyruk {numara} dolu"
            ));
        }
        satirlar.push("[2026-10-24 11:59:59] local.INFO: Başladı".to_string());
        let (ani, kaliplar) = hesapla(&satirlar.join("\n"), 1);
        assert_eq!(ani.artislar.len(), 1, "{:?}", ani.artislar);
        let artis = &ani.artislar[0];
        assert_eq!(kaliplar[artis.satir], "Kuyruk <SAYI> dolu");
        assert_eq!(artis.ornek, "Kuyruk 7 dolu");
        assert_eq!(
            (artis.ilk.to_string(), artis.kayit_sayisi),
            ("2026-10-24 11:00:00".to_string(), 8)
        );
    }

    #[test]
    fn olagan_seviye_icin_en_az_on_dilim_gerekir() {
        // 10:00:00–10:08:59: 9 dilim. Kocaman bir patlama bile ani artış sayılmaz.
        let kisa = log_metni(&[
            ("10:00:00", "INFO", "Başladı", 1, 0),
            ("10:04:00", "ERROR", "Kuyruk dolu", 40, 1),
            ("10:08:59", "INFO", "Başladı", 1, 0),
        ]);
        let (ani, _) = hesapla(&kisa, 1);
        assert_eq!(ani.dilim_sayisi, 9);
        assert!(!ani.yeterli());
        assert!(ani.artislar.is_empty());

        // Bir saniye sonra onuncu dilim başlar
        let on_dilim = format!("{kisa}\n[2026-10-24 10:09:00] local.INFO: Başladı");
        let (ani, _) = hesapla(&on_dilim, 1);
        assert_eq!(ani.dilim_sayisi, 10);
        assert!(ani.yeterli());
        assert_eq!(ani.artislar.len(), 1);
    }

    #[test]
    fn bos_logda_dilim_yok() {
        let (ani, _) = hesapla("", 1);
        assert_eq!(ani.aralik, None);
        assert_eq!(ani.dilim_sayisi, 0);
        assert!(!ani.yeterli());
        assert!(ani.artislar.is_empty());
    }

    #[test]
    fn dilimler_saat_basina_hizalanir() {
        // İlk kayıt 10:03'te, son kayıt 11:57'de: 10 dakikalık dilimler 10:00'da başlar,
        // 12:00'de biter
        let metin = log_metni(&[
            ("10:03:00", "INFO", "Başladı", 1, 0),
            ("10:47:30", "ERROR", "Kuyruk dolu", 6, 50),
            ("11:57:00", "INFO", "Başladı", 1, 0),
        ]);
        let (ani, _) = hesapla(&metin, 10);
        let (bas, son) = ani.aralik.unwrap();
        assert_eq!(
            (bas.to_string(), son.to_string()),
            (
                "2026-10-24 10:00:00".to_string(),
                "2026-10-24 12:00:00".to_string()
            )
        );
        assert_eq!(ani.dilim_sayisi, 12);
        // 10:47:30–10:51:40 arasındaki 6 kayıt iki dilime bölünür (10:40'ta 3, 10:50'de 3)
        assert!(ani.artislar.is_empty(), "{:?}", ani.artislar);

        // 60'ı bölmeyen bir uzunlukta dilimler saat başına denk gelmez. Hizalama gün başına
        // değil 1970-01-01 00:00'a göredir: 10:03 kaydının 7 dakikalık dilimi 09:59'da başlar.
        let (ani, _) = hesapla(&metin, 7);
        assert_eq!(ani.aralik.unwrap().0.to_string(), "2026-10-24 09:59:00");
    }

    #[test]
    fn gece_yarisini_asan_artis_tek_parca() {
        // 22:00'den ertesi gün 02:00'ye; 23:59 ve 00:00 dilimleri art arda
        let metin = log_metni(&[
            ("22:00:00", "INFO", "Başladı", 1, 0),
            ("23:59:00", "ERROR", "Kuyruk dolu", 8, 5),
            ("24:00:00", "ERROR", "Kuyruk dolu", 8, 5),
            ("26:00:00", "INFO", "Başladı", 1, 0),
        ]);
        let (ani, _) = hesapla(&metin, 1);
        assert_eq!(ani.artislar.len(), 1, "{:?}", ani.artislar);
        let artis = &ani.artislar[0];
        assert_eq!(artis.baslangic.to_string(), "2026-10-24 23:59:00");
        assert_eq!(artis.bitis.to_string(), "2026-10-25 00:01:00");
        assert_eq!(artis.kayit_sayisi, 16);
    }

    #[test]
    fn deneme_logunda_uc_artis() {
        let sonuc = hesapla(&deneme_logu(), 1);
        assert_eq!(sonuc.0.dilim_sayisi, 120);
        let ozet: Vec<(String, usize, String, String)> = ozetle(&sonuc);
        assert_eq!(
            ozet,
            [
                ("Kuyruk dolu".into(), 9, "10:30".into(), "10:33".into()),
                ("Yavaş istek".into(), 9, "10:45".into(), "10:46".into()),
                ("Kuyruk dolu".into(), 6, "11:30".into(), "11:31".into()),
            ]
        );
        let yavas = &sonuc.0.artislar[1];
        assert_eq!((yavas.olagan, yavas.mad), (2.0, 0.0));
    }

    #[test]
    fn bin_dokuz_yuz_yetmisten_onceki_dilimler() {
        // 1970-01-01 00:00'dan önceki anlar eksi saniyedir. 23:59:30, -30 sn: `div_euclid` onu
        // -1. dilime (23:59) koyar; `/` sıfıra doğru yuvarlayıp 0. dilime (00:00) koyardı.
        let mut satirlar = vec!["[1969-12-31 23:50:00] local.INFO: Başladı".to_string()];
        for saniye in 30..38 {
            satirlar.push(format!(
                "[1969-12-31 23:59:{saniye}] local.ERROR: Kuyruk dolu"
            ));
        }
        satirlar.push("[1970-01-01 00:10:00] local.INFO: Başladı".to_string());
        let (ani, _) = hesapla(&satirlar.join("\n"), 1);
        assert_eq!(ani.dilim_sayisi, 21);
        assert_eq!(ani.artislar.len(), 1, "{:?}", ani.artislar);
        let artis = &ani.artislar[0];
        assert_eq!(artis.baslangic.to_string(), "1969-12-31 23:59:00");
        assert_eq!(artis.bitis.to_string(), "1970-01-01 00:00:00");
    }

    #[test]
    fn sirasiz_kayitlar_ayni_sonucu_verir() {
        let mut kayitlar = IKI_SAAT.to_vec();
        kayitlar.extend([
            ("11:00:10", "ERROR", "Ödeme servisi yanıt vermedi", 8, 6),
            ("11:01:00", "ERROR", "Ödeme servisi yanıt vermedi", 12, 5),
        ]);
        let metin = log_metni(&kayitlar);
        // Aynı satırlar ters sırada: zaman sırası bozuk bir dosya
        let ters: Vec<&str> = metin.lines().rev().collect();
        let (sirali, _) = hesapla(&metin, 1);
        let (sirasiz, _) = hesapla(&ters.join("\n"), 1);
        assert_eq!(sirali.artislar, sirasiz.artislar);
        assert_eq!(sirasiz.artislar[0].ilk.to_string(), "2026-10-24 11:00:10");
    }

    #[test]
    fn ornek_logda_odeme_zaman_asimlari() {
        // Dakika dakika bakınca hiçbir kalıp bir dakikada 6 kayda ulaşmıyor
        let (ani, _) = hesapla(ORNEK_LOG, 1);
        assert_eq!(ani.dilim_sayisi, 573);
        assert!(ani.artislar.is_empty(), "{:?}", ani.artislar);
        // 5 dakikalık dilimlerde de: zaman aşımları 16:10 ve 16:15 dilimlerine bölünüyor (4 + 2)
        let (ani, _) = hesapla(ORNEK_LOG, 5);
        assert!(ani.artislar.is_empty(), "{:?}", ani.artislar);

        // 10 dakikalık dilimlerde 16:10–16:20 dilimine 6 zaman aşımı düşüyor
        let sonuc = hesapla(ORNEK_LOG, 10);
        assert_eq!(sonuc.0.dilim_sayisi, 59);
        let artislar = ozetle(&sonuc);
        assert_eq!(artislar.len(), 1, "{artislar:?}");
        assert!(artislar[0].0.starts_with("cURL error <SAYI>"));
        let artis = &sonuc.0.artislar[0];
        assert_eq!((artis.zirve_adet, artis.kayit_sayisi), (6, 6));
        assert_eq!(
            (artis.ilk.to_string(), artis.son.to_string()),
            (
                "2026-10-24 16:10:02".to_string(),
                "2026-10-24 16:16:02".to_string()
            )
        );

        // 2 saatlik dilimlerle log yalnızca 6 dilim: 08:00–20:00
        let (ani, _) = hesapla(ORNEK_LOG, 120);
        assert_eq!(ani.dilim_sayisi, 6);
        assert!(!ani.yeterli());
    }
}
