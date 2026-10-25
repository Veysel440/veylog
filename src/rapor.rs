//! Özet raporu: genel sayılar, seviyeler ve kalıp tablosu.
//!
//! Drain yalnızca metinle ilgilenir; seviye ve zaman gibi bilgileri bilmez. Bu modül her
//! kaydı maskeleyip Drain'e verir, Drain'in döndürdüğü küme numarasını kullanarak da kaydın
//! seviyesini, zamanını ve ham mesajını o kümenin bilgilerine ekler.

use std::cmp::Reverse;
use std::collections::BTreeSet;
use std::io::{self, Write};

use crate::drain::{Ayarlar, Drain};
use crate::kayit::{Log, Seviye};
use crate::maske::maskele;

/// Tabloda gösterilecek en fazla kalıp sayısı.
const GOSTERILECEK_KALIP: usize = 10;
/// Uzun kalıplar ve örnekler bu kadar karakterden sonra kesilir.
const KALIP_GENISLIGI: usize = 80;

/// Bir kalıp ve ona uyan kayıtlar hakkında toplanan bilgiler.
#[derive(Debug)]
pub struct KalipSatiri<'a> {
    /// Kalıbın son hali: "Zamanlanmış görev başladı: <*>"
    pub kalip: String,
    pub adet: usize,
    /// Kalıba uyan kayıtların en önemli seviyesi.
    pub seviye: Seviye,
    /// İlk görülme zamanı.
    pub ilk: &'a str,
    /// Son görülme zamanı.
    pub son: &'a str,
    /// Kalıba uyan ilk kaydın maskelenmemiş mesajı: "Kullanıcı 381 giriş yaptı"
    pub ornek: &'a str,
}

/// Kalıp çıkarmanın sonucu.
#[derive(Debug)]
pub struct KalipAnalizi<'a> {
    /// Maskelemeden önce kaç farklı mesaj vardı?
    pub ham_mesaj_sayisi: usize,
    /// Maskelemeden sonra kaç farklı mesaj kaldı?
    pub maskeli_mesaj_sayisi: usize,
    /// En önemli seviye başta; aynı seviyede en kalabalık kalıp başta.
    pub satirlar: Vec<KalipSatiri<'a>>,
}

/// Kalıp çıkarılırken her küme için biriktirilen bilgiler.
struct KumeBilgisi<'a> {
    seviye: Seviye,
    ilk: &'a str,
    son: &'a str,
    ornek: &'a str,
}

/// Kayıtları maskeleyip Drain'den geçirir ve her kalıp için bir satır üretir.
pub fn kaliplari_cikar<'a>(log: &Log<'a>, ayarlar: Ayarlar) -> KalipAnalizi<'a> {
    let mut drain = Drain::new(ayarlar);
    // Küme numarası → o kümeye uyan kayıtlardan toplanan bilgiler. Drain her yeni kümeye
    // sıradaki numarayı (0, 1, 2...) verdiği için bu liste kümeler listesiyle aynı sırada büyür.
    let mut bilgiler: Vec<KumeBilgisi<'a>> = Vec::new();
    let mut ham_mesajlar = BTreeSet::new();
    let mut maskeli_mesajlar = BTreeSet::new();

    for kayit in &log.kayitlar {
        let maskeli = maskele(kayit.mesaj());
        let numara = drain.ekle(&maskeli);
        ham_mesajlar.insert(kayit.mesaj());
        maskeli_mesajlar.insert(maskeli);

        if numara == bilgiler.len() {
            // Yeni küme: bilgilerini bu kayıtla başlat
            bilgiler.push(KumeBilgisi {
                seviye: kayit.seviye(),
                ilk: kayit.zaman(),
                son: kayit.zaman(),
                ornek: kayit.mesaj(),
            });
        } else {
            // Bilinen küme: en yüksek seviyeyi ve zaman aralığını güncelle
            let bilgi = &mut bilgiler[numara];
            bilgi.seviye = bilgi.seviye.max(kayit.seviye());
            bilgi.ilk = bilgi.ilk.min(kayit.zaman());
            bilgi.son = bilgi.son.max(kayit.zaman());
        }
    }

    // Kalıp metni yeni mesajlar geldikçe değişebildiği için en sonda Drain'den alınır
    let mut satirlar: Vec<KalipSatiri<'a>> = drain
        .kumeler()
        .iter()
        .zip(bilgiler)
        .map(|(kume, bilgi)| KalipSatiri {
            kalip: kume.metin(),
            adet: kume.adet,
            seviye: bilgi.seviye,
            ilk: bilgi.ilk,
            son: bilgi.son,
            ornek: bilgi.ornek,
        })
        .collect();
    // En önemli seviye başa, aynı seviyede en kalabalık başa. Sıralama kararlıdır: eşit
    // olanlar ilk görüldükleri sırada kalır.
    satirlar.sort_by_key(|satir| Reverse((satir.seviye, satir.adet)));

    KalipAnalizi {
        ham_mesaj_sayisi: ham_mesajlar.len(),
        maskeli_mesaj_sayisi: maskeli_mesajlar.len(),
        satirlar,
    }
}

/// Özeti verilen yere yazar: ekrana (stdout) ya da testlerde bir bayt dizisine.
pub fn ozet_yaz(cikti: &mut impl Write, yol: &str, icerik: &str, log: &Log<'_>) -> io::Result<()> {
    let zaman_araligi = log.zaman_araligi();

    writeln!(cikti, "{yol}: {} satır", icerik.lines().count())?;
    writeln!(cikti, "  Kayıt         : {}", log.kayitlar.len())?;
    writeln!(cikti, "  Devam satırı  : {}", log.devam_satiri_sayisi())?;
    writeln!(cikti, "  Yetim satır   : {}", log.yetim_satir)?;
    if let Some((en_eski, en_yeni)) = zaman_araligi {
        writeln!(cikti, "  Zaman aralığı : {en_eski} → {en_yeni}")?;
    }

    writeln!(cikti)?;
    writeln!(cikti, "Seviyeler:")?;
    // En önemli seviye en üstte görünsün diye ters sırada yaz
    for (seviye, adet) in log.seviye_sayilari().iter().rev() {
        writeln!(cikti, "  {:<10}{adet:>4}", seviye.ad())?;
    }

    let analiz = kaliplari_cikar(log, Ayarlar::default());
    writeln!(cikti)?;
    writeln!(
        cikti,
        "Kalıplar: {} farklı mesaj → maskeleyince {} → Drain ile {} kalıp",
        analiz.ham_mesaj_sayisi,
        analiz.maskeli_mesaj_sayisi,
        analiz.satirlar.len()
    )?;

    if !analiz.satirlar.is_empty() {
        // Bütün kayıtlar aynı gündeyse saat yeter, değilse tarih de gösterilir
        let tek_gun =
            zaman_araligi.is_some_and(|(en_eski, en_yeni)| en_eski[..10] == en_yeni[..10]);
        let zaman_genisligi = if tek_gun { 8 } else { 14 };
        // Örnek satırı, kalıp sütununun hizasından başlasın
        let girinti = " ".repeat(2 + 4 + 2 + 9 + 2 + zaman_genisligi + 2 + zaman_genisligi + 2);

        writeln!(
            cikti,
            "  {:>4}  {:<9}  {:<zaman_genisligi$}  {:<zaman_genisligi$}  kalıp",
            "adet", "seviye", "ilk", "son"
        )?;
        for satir in analiz.satirlar.iter().take(GOSTERILECEK_KALIP) {
            writeln!(
                cikti,
                "  {:>4}  {:<9}  {:<zaman_genisligi$}  {:<zaman_genisligi$}  {}",
                satir.adet,
                satir.seviye.ad(),
                zaman_goster(satir.ilk, tek_gun),
                zaman_goster(satir.son, tek_gun),
                kisalt(&satir.kalip, KALIP_GENISLIGI)
            )?;
            // Kalıp maskelenmiş ya da <*> içeriyorsa arkasındaki gerçek değerleri göster
            if satir.ornek != satir.kalip {
                writeln!(
                    cikti,
                    "{girinti}örnek: {}",
                    kisalt(satir.ornek, KALIP_GENISLIGI - 7)
                )?;
            }
        }
    }

    // Tamponda kalan son satırlar burada yazılır; varsa hatası da burada ortaya çıkar
    cikti.flush()
}

/// Zamanı tabloda gösterilecek hale getirir: tek günlük logda "14:03:11", birden çok günü
/// kapsayan logda "10-24 14:03:11".
///
/// Kayıt zamanlarının ilk 19 karakteri her zaman geçerli bir tarih-saat olduğu ve bu
/// karakterler ASCII olduğu için bayt konumlarıyla dilimlemek güvenlidir.
fn zaman_goster(zaman: &str, tek_gun: bool) -> String {
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
fn kisalt(metin: &str, sinir: usize) -> String {
    if metin.chars().count() <= sinir {
        return metin.to_string();
    }
    let kisa: String = metin.chars().take(sinir - 1).collect();
    format!("{kisa}…")
}

#[cfg(test)]
mod testler {
    use super::*;
    use crate::kayit::kayitlari_ayir;

    const ORNEK_LOG: &str = include_str!("../ornekler/laravel.log");

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
        let analiz = kaliplari_cikar(&log, Ayarlar::default());

        assert_eq!(analiz.satirlar.len(), 1);
        let satir = &analiz.satirlar[0];
        assert_eq!(satir.adet, 3);
        assert_eq!(satir.seviye, Seviye::Error);
        assert_eq!(satir.ilk, "2026-10-24 10:00:01");
        assert_eq!(satir.son, "2026-10-24 10:00:09");
    }

    #[test]
    fn ornek_logda_kalip_bilgileri_dogru() {
        let log = kayitlari_ayir(ORNEK_LOG);
        let analiz = kaliplari_cikar(&log, Ayarlar::default());

        assert_eq!(analiz.ham_mesaj_sayisi, 45);
        assert_eq!(analiz.maskeli_mesaj_sayisi, 27);
        assert_eq!(analiz.satirlar.len(), 23);

        // En önemli kalıp başta: 17:05'teki Redis çöküşü
        let ilk = &analiz.satirlar[0];
        assert_eq!(ilk.seviye, Seviye::Critical);
        assert!(ilk.kalip.starts_with("Redis bağlantısı kurulamadı"));
        assert_eq!(
            (ilk.ilk, ilk.son),
            ("2026-10-24 17:05:13", "2026-10-24 17:06:45")
        );
        assert!(ilk.ornek.contains("10.0.3.12:6379"));

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

        // Ekran yerine bir bayt dizisine yaz, sonra metin olarak incele
        let mut cikti = Vec::new();
        ozet_yaz(&mut cikti, "ornek.log", ORNEK_LOG, &log).unwrap();
        let metin = String::from_utf8(cikti).unwrap();

        assert!(metin.starts_with("ornek.log: 203 satır\n"));
        assert!(metin.contains("\nSeviyeler:\n  CRITICAL     2\n  ERROR       16\n"));
        assert!(
            metin.contains("Kalıplar: 45 farklı mesaj → maskeleyince 27 → Drain ile 23 kalıp\n")
        );
        assert!(metin.contains("\n  adet  seviye     ilk       son       kalıp\n"));
        assert!(metin.contains("\n     2  CRITICAL   17:05:13  17:06:45  Redis bağlantısı"));
        assert!(metin.contains(
            "örnek: Redis bağlantısı kurulamadı: Connection refused [tcp://10.0.3.12:6379]"
        ));
    }
}
