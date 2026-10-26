//! Özet raporu: genel sayılar, seviyeler ve kalıp tablosu.
//!
//! Drain yalnızca metinle ilgilenir; seviyeyi, zamanı ve yığın izini bilmez. Bu modül her
//! kaydı maskeleyip Drain'e verir, sonra kaydı iki şeye göre bir gruba ekler: Drain'in
//! döndürdüğü küme numarası ve kaydın parmak izi. Aynı kalıptaki kayıtlar kodun farklı
//! yerlerinden geliyorsa böylece ayrı satırlarda görünür.

use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap};
use std::io::{self, Write};
use std::path::Path;

use crate::drain::{Ayarlar, Drain};
use crate::kayit::{Kayit, Log, Seviye};
use crate::maske::maskele;
use crate::parmak_izi::ParmakIzi;

/// Tabloda varsayılan olarak gösterilecek satır sayısı.
pub const VARSAYILAN_ILK: usize = 10;
/// Uzun kalıplar ve örnekler bu kadar karakterden sonra kesilir.
const KALIP_GENISLIGI: usize = 80;

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
}

impl Default for RaporAyarlari {
    fn default() -> Self {
        RaporAyarlari {
            drain: Ayarlar::default(),
            siralama: Siralama::Onem,
            ilk: Some(VARSAYILAN_ILK),
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
        }
    }

    /// Kaydı gruba katar: adedi artırır, en yüksek seviyeyi ve zaman aralığını günceller.
    fn ekle(&mut self, kayit: &Kayit<'a>) {
        self.adet += 1;
        self.seviye = self.seviye.max(kayit.seviye());
        self.ilk = self.ilk.min(kayit.zaman());
        self.son = self.son.max(kayit.zaman());
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

    for kayit in &log.kayitlar {
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
        gruplar[sira].ekle(kayit);
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

/// Özeti verilen yere yazar: ekrana (stdout) ya da testlerde bir bayt dizisine.
pub fn ozet_yaz(
    cikti: &mut impl Write,
    yol: &Path,
    icerik: &str,
    log: &Log<'_>,
    ayarlar: &RaporAyarlari,
) -> io::Result<()> {
    let zaman_araligi = log.zaman_araligi();

    // `display`: dosya adı geçerli UTF-8 değilse bozuk baytlar � olarak yazılır
    writeln!(cikti, "{}: {} satır", yol.display(), icerik.lines().count())?;
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

    let analiz = kaliplari_cikar(log, ayarlar);
    writeln!(cikti)?;
    writeln!(
        cikti,
        "Kalıplar: {} farklı mesaj → maskeleyince {} → Drain ile {} kalıp (eşik {}) → parmak iziyle {} grup",
        analiz.ham_mesaj_sayisi,
        analiz.maskeli_mesaj_sayisi,
        analiz.kalip_sayisi,
        ondalik(ayarlar.drain.benzerlik_esigi),
        analiz.satirlar.len()
    )?;

    if !analiz.satirlar.is_empty() {
        // Bütün kayıtlar aynı gündeyse saat yeter, değilse tarih de gösterilir
        let tek_gun =
            zaman_araligi.is_some_and(|(en_eski, en_yeni)| en_eski[..10] == en_yeni[..10]);
        let zaman_genisligi = if tek_gun { 8 } else { 14 };
        // Örnek ve konum satırları, kalıp sütununun hizasından başlasın
        let girinti = " ".repeat(2 + 4 + 2 + 9 + 2 + zaman_genisligi + 2 + zaman_genisligi + 2);

        writeln!(
            cikti,
            "  {:>4}  {:<9}  {:<zaman_genisligi$}  {:<zaman_genisligi$}  kalıp",
            "adet", "seviye", "ilk", "son"
        )?;
        // None (--hepsi): sınır yok
        let gosterilecek = ayarlar.ilk.unwrap_or(usize::MAX);
        for satir in analiz.satirlar.iter().take(gosterilecek) {
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
            // Kayıtlar bir istisnadan geliyorsa nerede oluştuğunu göster. Dosya yolu
            // kısaltılmaz: terminalde tıklanıp doğrudan açılabilsin.
            if let Some(parmak_izi) = satir.parmak_izi {
                writeln!(cikti, "{girinti}konum: {parmak_izi}")?;
            }
        }
        // Gösterilmeyen satırlar varsa söyle; yoksa kullanıcı tablonun bittiğini sanır
        let gizlenen = analiz.satirlar.len().saturating_sub(gosterilecek);
        if gizlenen > 0 {
            writeln!(
                cikti,
                "  … {gizlenen} satır daha; tümünü görmek için --hepsi"
            )?;
        }
    }

    // Tamponda kalan son satırlar burada yazılır; varsa hatası da burada ortaya çıkar
    cikti.flush()
}

/// Ondalık sayıyı Türkçedeki gibi virgülle yazar: 0.7 → "0,7"
pub fn ondalik(sayi: f64) -> String {
    sayi.to_string().replace('.', ",")
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
    fn ondalik_virgulle_yazilir() {
        assert_eq!(ondalik(0.7), "0,7");
        assert_eq!(ondalik(0.85), "0,85");
        assert_eq!(ondalik(1.0), "1");
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

        let metin = ozet(&log, &RaporAyarlari::default());

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

        let ilk_uc = ozet(
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

        let hepsi = ozet(
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

    /// Özeti ekran yerine bir bayt dizisine yazar ve metin olarak döndürür.
    fn ozet(log: &Log<'_>, ayarlar: &RaporAyarlari) -> String {
        let mut cikti = Vec::new();
        ozet_yaz(&mut cikti, Path::new("ornek.log"), ORNEK_LOG, log, ayarlar).unwrap();
        String::from_utf8(cikti).unwrap()
    }
}
