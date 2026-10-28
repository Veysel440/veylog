//! Zaman: kayıt zamanlarını ve `--karsilastir` değerini karşılaştırılabilir hale getirir,
//! iki an arasındaki süreyi hesaplar.
//!
//! Kayıt zamanları logda metin olarak yazılır (`2026-10-24 14:03:11`). İki an arasında kaç
//! saniye olduğunu bulmak için tarih bir sayıya çevrilir: 1970-01-01'den bu yana geçen gün.
//! Saniyenin kesri dikkate alınmaz. Ters yön de vardır: saniyeden ana (ani artış dilimlerinin
//! başı ve sonu böyle bulunur).
//!
//! **Saat dilimi.** Monolog'un ISO 8601 biçiminde zamanın sonunda saat dilimi yazar
//! (`2026-10-24T14:03:11+03:00`); Laravel'in varsayılan biçiminde yazmaz. Bütün zamanlar tek
//! bir dilimde, raporun saat diliminde karşılaştırılır (`DilimCevirici`): `--saat-dilimi` ile
//! verilir, verilmezse logda saat dilimi yazılı ilk kaydınkidir. Saat dilimi başka olan bir
//! kayıt bu dilime çevrilir; saat dilimi yazılmamış bir kayıt bu dilimde yazılmış sayılır.
//! Böylece yaz saatine geçişi kapsayan bir log (`+02:00` ve `+01:00`) ya da saat dilimi
//! değiştirilmiş bir uygulamanın logu doğru sıralanır.

use std::fmt;
use std::ops::RangeInclusive;

use crate::dil;

/// Bir an: tarih ve saniye hassasiyetinde saat.
///
/// `Ord` türetilince alanlar yazıldıkları sırayla karşılaştırılır: önce yıl, eşitse ay,
/// eşitse gün... Bu, iki anın kronolojik karşılaştırmasıdır; alanların sırası bu yüzden
/// önemlidir. Alanlar `u16`: en büyüğü olan yıl bile 9999'u geçmez; bir an 12 bayttır.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Zaman {
    yil: u16,
    ay: u16,
    gun: u16,
    saat: u16,
    dakika: u16,
    saniye: u16,
}

impl Zaman {
    /// Bir kaydın zamanından: `2026-10-24 14:03:11` ya da `2026-10-24T14:03:11.123456+03:00`.
    ///
    /// Kayıt zamanı kayıt ayrıştırılırken denetlendiği için (`kayit` modülü) en az 19 karakter
    /// olduğu, parçaların yerinde rakamlar bulunduğu ve takvimde var olan bir an olduğu bilinir.
    pub fn kayittan(zaman: &str) -> Zaman {
        let baytlar = zaman.as_bytes();
        // Parçalar hep rakamdır; olmasaydı 0 olurdu, program çökmezdi
        let parca = |bas: usize, son: usize| rakamlar(&baytlar[bas..son]).unwrap_or(0);
        Zaman {
            yil: parca(0, 4),
            ay: parca(5, 7),
            gun: parca(8, 10),
            saat: parca(11, 13),
            dakika: parca(14, 16),
            saniye: parca(17, 19),
        }
    }

    /// Parçalarından bir an; takvimde yoksa (`2026-02-30`, `24:00:00`) None.
    pub fn yeni(yil: u16, ay: u16, gun: u16, saat: u16, dakika: u16, saniye: u16) -> Option<Zaman> {
        let zaman = Zaman {
            yil,
            ay,
            gun,
            saat,
            dakika,
            saniye,
        };
        (yil <= 9999 && zaman.takvimde_var_mi()).then_some(zaman)
    }

    /// Takvimde var olan bir an mı? Ay 1–12, gün o ayın gün sayısı kadar (artık yıllar dahil),
    /// saat 0–23, dakika ve saniye 0–59.
    ///
    /// Biçimi doğru ama takvimde olmayan bir zaman (`2026-02-30`, `24:00:00`) iki anı
    /// karşılaştırmayı bozar: `Ord` alanları sırayla karşılaştırır, `saniye` ise gün numarasını
    /// hesaplar. 30 Şubat sıralamada 1 Mart'tan önce gelir ama saniyesi 2 Mart'ınkidir.
    pub fn takvimde_var_mi(self) -> bool {
        (1..=12).contains(&self.ay)
            && (1..=ayin_gun_sayisi(self.yil, self.ay)).contains(&self.gun)
            && self.saat <= 23
            && self.dakika <= 59
            && self.saniye <= 59
    }

    /// 1970-01-01 00:00:00'dan bu yana geçen saniye. İki anın farkı aralarındaki süredir.
    ///
    /// `i64::from`: küçük tamsayı türünü kayıpsız büyüğe çevirir (`as`'ın aksine taşma ya da
    /// kırpma ihtimali olmadığı derleyicice bilinir).
    pub fn saniye(self) -> i64 {
        let gun = gun_numarasi(i64::from(self.yil), i64::from(self.ay), i64::from(self.gun));
        gun * 86_400
            + i64::from(self.saat) * 3_600
            + i64::from(self.dakika) * 60
            + i64::from(self.saniye)
    }

    /// İki an aynı günde mi?
    pub fn ayni_gun(self, diger: Zaman) -> bool {
        (self.yil, self.ay, self.gun) == (diger.yil, diger.ay, diger.gun)
    }

    /// İki an aynı yılda mı?
    pub fn ayni_yil(self, diger: Zaman) -> bool {
        self.yil == diger.yil
    }

    /// `saniye`nin tersi: 1970-01-01 00:00:00'dan bu yana geçen saniyeden an.
    ///
    /// Yıl dört basamaklı kalır: 0000-01-01 00:00:00'dan önceki bir saniye o ana, 9999-12-31
    /// 23:59:59'dan sonraki o ana çekilir.
    pub fn saniyeden(gecen: i64) -> Zaman {
        let gecen = gecen.clamp(EN_ESKI.saniye(), EN_YENI.saniye());
        // Eksi saniyelerde de doğru gün: -1 sn, 1969-12-31'in son saniyesidir
        let (gun, gunun_saniyesi) = (gecen.div_euclid(86_400), gecen.rem_euclid(86_400));
        let (yil, ay, gun) = takvim_gunu(gun);
        Zaman {
            yil: alan(yil),
            ay: alan(ay),
            gun: alan(gun),
            saat: alan(gunun_saniyesi / 3_600),
            dakika: alan(gunun_saniyesi % 3_600 / 60),
            saniye: alan(gunun_saniyesi % 60),
        }
    }
}

/// Dört basamaklı yıllarla yazılabilen ilk ve son an.
const EN_ESKI: Zaman = Zaman {
    yil: 0,
    ay: 1,
    gun: 1,
    saat: 0,
    dakika: 0,
    saniye: 0,
};
const EN_YENI: Zaman = Zaman {
    yil: 9999,
    ay: 12,
    gun: 31,
    saat: 23,
    dakika: 59,
    saniye: 59,
};

/// Sınırları bilinen bir değeri `Zaman` alanına çevirir.
///
/// `i64`'ten `u16`'ya dönüşüm bilgi kaybettirebilir, bu yüzden `From` değil `TryFrom` vardır:
/// `try_from` sığmayan değerde hata döndürür. Buraya gelen değerler `saniyeden`de sınırlandığı
/// için hep sığar.
fn alan(deger: i64) -> u16 {
    u16::try_from(deger).expect("takvim alanları u16'ya sığar")
}

/// `2026-10-24 15:00:00`
impl fmt::Display for Zaman {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.yil, self.ay, self.gun, self.saat, self.dakika, self.saniye
        )
    }
}

/// Bir saat dilimi: UTC'den fark, saniye. `+03:00` → 10 800, `-05:30` → −19 800.
///
/// Fark RFC 3339'daki gibi en fazla 23 saat 59 dakikadır. Gerçek saat dilimleri −12:00 ile
/// +14:00 arasındadır; daha geniş aralık, logda yazılabilecek her farkın okunabilmesi içindir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SaatDilimi(i32);

impl SaatDilimi {
    pub const UTC: SaatDilimi = SaatDilimi(0);

    /// UTC'den fark, saniye.
    pub fn saniye(self) -> i64 {
        i64::from(self.0)
    }

    /// UTC'den farkı saniye olarak yazılmış bir dilim (PHP'nin `Z` biçimi: `10800`, `-18000`).
    /// Fark tam dakika değilse ya da ±23:59'u aşıyorsa None.
    pub fn saniyeden(saniye: i64) -> Option<SaatDilimi> {
        let gecerli = saniye % 60 == 0 && saniye.abs() <= 23 * 3_600 + 59 * 60;
        // Sınır denetlendi: fark i32'ye sığar
        gecerli.then(|| SaatDilimi(i32::try_from(saniye).expect("±23:59 i32'ye sığar")))
    }

    /// Saat ve dakikadan; saat 0–23, dakika 0–59 değilse None.
    fn saat_dakika(eksi: bool, saat: u16, dakika: u16) -> Option<SaatDilimi> {
        let fark = i32::from(saat) * 3_600 + i32::from(dakika) * 60;
        (saat <= 23 && dakika <= 59).then_some(SaatDilimi(if eksi { -fark } else { fark }))
    }

    /// Komut satırındaki yazılışından: `+03:00`, `+0300`, `+03`, `+3`, `+3:30`, `-05:30`, `Z`,
    /// `UTC`.
    pub fn ayristir(metin: &str) -> Option<SaatDilimi> {
        if metin.eq_ignore_ascii_case("z") || metin.eq_ignore_ascii_case("utc") {
            return Some(SaatDilimi::UTC);
        }
        let (eksi, geri) = match metin.as_bytes().first()? {
            b'+' => (false, &metin[1..]),
            b'-' => (true, &metin[1..]),
            _ => return None,
        };
        let (saat, dakika) = match geri.split_once(':') {
            Some(parcalar) => parcalar,
            // "0300": ilk ikisi saat, son ikisi dakika. Bölünen yer bir harfin ortasına
            // düşebilir ("0ş0"): o zaman metin geçersizdir
            None if geri.len() == 4 => geri.split_at_checked(2)?,
            None => (geri, "00"),
        };
        SaatDilimi::saat_dakika(eksi, sayi(saat, 1..=2)?, sayi(dakika, 2..=2)?)
    }

    /// Bir kayıt zamanının sonundaki saat dilimi: `...+03:00`, `...+0300`, `...Z`. Yazılmamışsa
    /// None. Zamanın biçimi kayıt ayrıştırılırken `zaman_eki` ile denetlenmiştir.
    pub fn kayittan(zaman: &str) -> Option<SaatDilimi> {
        zaman_eki(zaman.as_bytes().get(19..)?).flatten()
    }
}

/// `+03:00`, `-05:30`, `+00:00`
impl fmt::Display for SaatDilimi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let isaret = if self.0 < 0 { '-' } else { '+' };
        let mutlak = self.0.unsigned_abs();
        write!(
            f,
            "{isaret}{:02}:{:02}",
            mutlak / 3_600,
            mutlak % 3_600 / 60
        )
    }
}

/// Kayıt zamanı iç biçimde ve geçerli mi? İki biçim kabul edilir:
///
/// - Laravel'in varsayılanı: `2026-10-24 14:03:11`
/// - Monolog'un varsayılanı olan ISO 8601: `2026-10-24T14:03:11+03:00`
///
/// Saniyeden sonra kesir (`.123456`) ve saat dilimi (`Z`, `+03:00`, `+0300`) gelebilir
/// (`zaman_eki`). `--zaman-bicimi` ile okunan zamanlar da bu biçime çevrilir.
///
/// Biçimi doğru ama takvimde olmayan bir zaman (`2026-02-30`, `25:00:00`) ya da saat dilimi
/// (`+25:00`) reddedilir. Laravel böyle bir zaman yazmaz; ama mesajlar satır sonu içerebildiği
/// için, loglanan bir kullanıcı girdisindeki satır bir başlık gibi görünebilir. Kabul edilseydi
/// süre hesapları bozulurdu.
pub fn kayit_zamani_mi(metin: &str) -> bool {
    let baytlar = metin.as_bytes();
    if baytlar.len() < 19 {
        return false;
    }
    let (tarih_saat, ek) = baytlar.split_at(19);
    let tarih_saat_gecerli = tarih_saat
        .iter()
        .enumerate()
        .all(|(sira, &bayt)| match sira {
            4 | 7 => bayt == b'-',
            10 => bayt == b' ' || bayt == b'T',
            13 | 16 => bayt == b':',
            _ => bayt.is_ascii_digit(),
        });
    // `&&` kısa devre yapar: `Zaman::kayittan` yalnızca biçim doğruysa çağrılır
    tarih_saat_gecerli && zaman_eki(ek).is_some() && Zaman::kayittan(metin).takvimde_var_mi()
}

/// Kayıt zamanının saniyeden sonraki kısmı: isteğe bağlı kesir (`.123456`), ardından isteğe
/// bağlı saat dilimi (`Z`, `+03:00`, `+0300`). Geçersizse None; geçerliyse yazılmışsa saat
/// dilimi: `""` → `Some(None)`, `".5+03:00"` → `Some(Some(+03:00))`, `"+25:00"` → None.
///
/// Bir kayıt zamanının geçerliliği bununla denetlenir (`kayit_zamani_mi`); saat dilimini
/// okuyan da (`SaatDilimi::kayittan`) bunu kullanır. Böylece kabul edilen her saat dilimi
/// okunabilir.
pub fn zaman_eki(ek: &[u8]) -> Option<Option<SaatDilimi>> {
    let ek = match ek.strip_prefix(b".") {
        Some(kesir) => {
            let rakam_sayisi = kesir
                .iter()
                .take_while(|bayt| bayt.is_ascii_digit())
                .count();
            if rakam_sayisi == 0 {
                return None;
            }
            &kesir[rakam_sayisi..]
        }
        None => ek,
    };
    let (isaret, saat, dakika) = match ek {
        [] => return Some(None),
        [b'Z'] => return Some(Some(SaatDilimi::UTC)),
        [isaret, s1, s2, b':', d1, d2] | [isaret, s1, s2, d1, d2] => {
            (*isaret, rakamlar(&[*s1, *s2])?, rakamlar(&[*d1, *d2])?)
        }
        _ => return None,
    };
    let eksi = match isaret {
        b'+' => false,
        b'-' => true,
        _ => return None,
    };
    SaatDilimi::saat_dakika(eksi, saat, dakika).map(Some)
}

/// Bir kaydın saat dilimi, raporun saat dilimine göre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KayitDilimi {
    /// Kayıtta saat dilimi yazılmamış: raporun diliminde yazılmış sayıldı.
    Yazilmamis,
    /// Raporun diliminde yazılmış.
    Ayni,
    /// Başka bir dilimde yazılmış: zamanı raporun dilimine çevrildi.
    Baska(SaatDilimi),
}

/// Kayıt zamanlarını raporun saat dilimine çevirir.
///
/// Raporun saat dilimi verilmediyse (`--saat-dilimi`), saat dilimi yazılı ilk kayıtta belirlenir
/// ve bir daha değişmez. O kayıttan önce gelen, saat dilimi yazılmamış kayıtlar da bu dilimde
/// sayılır: çevrilmeleri gerekmez. Böylece her kayıt geldiği anda çevrilebilir; logun tamamını
/// önce okumak gerekmez.
///
/// Logu aynı sırayla okuyan iki çevirici aynı dilimi bulur, her kayda aynı zamanı verir: ön
/// tarama ile asıl okuma bu yüzden tutarlıdır. Logu ortasından okuyan biri (`YenidenOkuyucu`)
/// asıl okumanın bulduğu dilimle başlatılır; dilim belli olunca her kaydın zamanı yalnızca
/// kendisine bağlıdır.
#[derive(Debug, Clone, Copy, Default)]
pub struct DilimCevirici {
    dilim: Option<SaatDilimi>,
    verildi: bool,
}

impl DilimCevirici {
    /// `dilim`: raporun saat dilimi; None ise logdan belirlenir.
    pub fn new(dilim: Option<SaatDilimi>) -> DilimCevirici {
        DilimCevirici {
            dilim,
            verildi: dilim.is_some(),
        }
    }

    /// Kaydın zamanı raporun saat diliminde ve kaydın dilimi. `metin`: kaydın logda yazıldığı
    /// haliyle zamanı.
    pub fn cevir(&mut self, metin: &str) -> (Zaman, KayitDilimi) {
        let yerel = Zaman::kayittan(metin);
        let Some(kendi) = SaatDilimi::kayittan(metin) else {
            return (yerel, KayitDilimi::Yazilmamis);
        };
        let dilim = *self.dilim.get_or_insert(kendi);
        if kendi == dilim {
            (yerel, KayitDilimi::Ayni)
        } else {
            // Önce UTC'ye, sonra raporun dilimine
            let cevrilmis = yerel.saniye() - kendi.saniye() + dilim.saniye();
            (Zaman::saniyeden(cevrilmis), KayitDilimi::Baska(kendi))
        }
    }

    /// Raporun saat dilimi; verilmediyse ve logda henüz saat dilimi yazılı bir kayıt
    /// görülmediyse None.
    pub fn dilim(&self) -> Option<SaatDilimi> {
        self.dilim
    }

    /// Dilim komut satırında mı verildi?
    pub fn verildi(&self) -> bool {
        self.verildi
    }
}

/// `--karsilastir` değeri: logun ikiye bölüneceği an.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZamanNoktasi {
    /// Yalnızca saat (`15:00`): logun son gününe uygulanır.
    Saat { saat: u16, dakika: u16, saniye: u16 },
    /// Tarih ve saat (`2026-10-24 15:00`). Yalnızca tarih verildiyse günün başı.
    Tam(Zaman),
}

impl ZamanNoktasi {
    /// Komut satırındaki yazılışından: `15:00`, `9:30:15`, `2026-10-24`, `2026-10-24 15:00`,
    /// `2026-10-24T15:00:30`. Geçersiz bir tarih ya da saatte (`2026-02-30`, `24:00`) None.
    pub fn ayristir(metin: &str) -> Option<ZamanNoktasi> {
        // Tarih ile saat arasında boşluk ya da (ISO 8601'deki gibi) T olabilir
        if let Some((tarih, saat)) = metin.split_once([' ', 'T']) {
            let (yil, ay, gun) = tarih_ayristir(tarih)?;
            let (saat, dakika, saniye) = saat_ayristir(saat)?;
            Some(ZamanNoktasi::Tam(Zaman {
                yil,
                ay,
                gun,
                saat,
                dakika,
                saniye,
            }))
        } else if metin.contains('-') {
            let (yil, ay, gun) = tarih_ayristir(metin)?;
            Some(ZamanNoktasi::Tam(Zaman {
                yil,
                ay,
                gun,
                saat: 0,
                dakika: 0,
                saniye: 0,
            }))
        } else {
            let (saat, dakika, saniye) = saat_ayristir(metin)?;
            Some(ZamanNoktasi::Saat {
                saat,
                dakika,
                saniye,
            })
        }
    }

    /// Tam bir ana çevirir. Yalnızca saat verildiyse tarih logun son kaydından alınır.
    pub fn coz(self, son_kayit: Zaman) -> Zaman {
        match self {
            // Yapı güncelleme sözdizimi: saat alanları bizden, tarih alanları son kayıttan
            ZamanNoktasi::Saat {
                saat,
                dakika,
                saniye,
            } => Zaman {
                saat,
                dakika,
                saniye,
                ..son_kayit
            },
            ZamanNoktasi::Tam(zaman) => zaman,
        }
    }
}

/// Süreyi okunur yazar: `45 sn`, `12 dk 5 sn`, `6 sa 0 dk`, `2 gün 3 sa`
pub fn sure_yaz(saniye: usize) -> String {
    let (gun, kalan) = (saniye / 86_400, saniye % 86_400);
    let (saat, kalan) = (kalan / 3_600, kalan % 3_600);
    let (dakika, saniye) = (kalan / 60, kalan % 60);
    let [g, sa, dk, sn] = birimler();
    if gun > 0 {
        format!("{gun} {g} {saat} {sa}")
    } else if saat > 0 {
        format!("{saat} {sa} {dakika} {dk}")
    } else if dakika > 0 {
        format!("{dakika} {dk} {saniye} {sn}")
    } else {
        format!("{saniye} {sn}")
    }
}

/// Sürelerin birimleri, seçili dilde: gün, saat, dakika, saniye.
fn birimler() -> [&'static str; 4] {
    dil::sec(["gün", "sa", "dk", "sn"], ["d", "h", "min", "s"])
}

/// Süreyi bütün bileşenleriyle yazar, sıfır olanları atlayarak: `30 dk`, `1 sa 30 dk`,
/// `1 gün 2 sa 3 dk 4 sn`. Kullanıcının verdiği bir süreyi (`--son 1sa30dk`) geri yazmak için.
pub fn sure_tam_yaz(saniye: u64) -> String {
    let [g, sa, dk, sn] = birimler();
    let parcalar = [
        (saniye / 86_400, g),
        (saniye % 86_400 / 3_600, sa),
        (saniye % 3_600 / 60, dk),
        (saniye % 60, sn),
    ];
    let yazilanlar: Vec<String> = parcalar
        .iter()
        .filter(|&&(sayi, _)| sayi > 0)
        .map(|(sayi, birim)| format!("{sayi} {birim}"))
        .collect();
    if yazilanlar.is_empty() {
        format!("0 {sn}")
    } else {
        yazilanlar.join(" ")
    }
}

/// 1970-01-01'den bu yana geçen gün; öncesi için eksi.
///
/// Howard Hinnant'ın `days_from_civil` algoritması. Yıl Mart'ta başlıyormuş gibi sayılır:
/// Şubat yılın son ayı olur ve artık yıldaki 29 Şubat yılın en sonuna düşer. Böylece bir ayın
/// yıl içindeki ilk günü, Şubat'ın uzunluğundan bağımsız tek bir formülle bulunur. Takvim
/// 400 yılda bir kendini tekrarlar (146 097 gün), hesap bu dönemler üzerinden yapılır.
fn gun_numarasi(yil: i64, ay: i64, gun: i64) -> i64 {
    // Ocak ve Şubat bir önceki "Mart yılına" aittir
    let yil = if ay <= 2 { yil - 1 } else { yil };
    // `div_euclid` ve `rem_euclid` eksi sayılarda da aşağı yuvarlar: -1 / 400 = -1, kalan 399
    let donem = yil.div_euclid(400);
    let donem_yili = yil.rem_euclid(400); // 0..=399
    let mart_ayi = (ay + 9) % 12; // Mart 0, Nisan 1, ..., Şubat 11
    // Mart'tan bu ayın başına kadar geçen gün: 153 gün her 5 ayda tekrarlanan 31-30 düzenidir
    let yilin_gunu = (153 * mart_ayi + 2) / 5 + gun - 1; // 0..=365
    // Her 4 yılda bir artık gün; 100'ün katlarında yok, 400'ün katlarında var
    let donemin_gunu = donem_yili * 365 + donem_yili / 4 - donem_yili / 100 + yilin_gunu;
    // 719 468: 0000-03-01 ile 1970-01-01 arasındaki gün sayısı
    donem * 146_097 + donemin_gunu - 719_468
}

/// `gun_numarasi`nın tersi: 1970-01-01'den bu yana geçen günden (yıl, ay, gün).
///
/// Howard Hinnant'ın `civil_from_days` algoritması. Yıl yine Mart'ta başlıyormuş gibi sayılır;
/// önce 400 yıllık dönem, sonra dönemin içindeki yıl, yılın kaçıncı günü olduğu ve ay bulunur.
fn takvim_gunu(gun_numarasi: i64) -> (i64, i64, i64) {
    // 0000-03-01'den bu yana geçen gün
    let gun = gun_numarasi + 719_468;
    let donem = gun.div_euclid(146_097);
    let donemin_gunu = gun.rem_euclid(146_097); // 0..=146 096
    // Dönemin kaçıncı yılı: artık günler çıkarılınca her yıl 365 gün olur. Bölmeler artık
    // günleri sayar: 1 460 günde bir (4 yıl) bir gün çıkar, 36 524 günde bir (100 yıl) geri
    // eklenir, dönemin son gününde (146 096) yine çıkar.
    let donem_yili = (donemin_gunu - donemin_gunu / 1_460 + donemin_gunu / 36_524
        - donemin_gunu / 146_096)
        / 365; // 0..=399
    let yilin_gunu = donemin_gunu - (365 * donem_yili + donem_yili / 4 - donem_yili / 100); // 0..=365
    // `gun_numarasi`ndaki 153/5 formülünün tersi
    let mart_ayi = (5 * yilin_gunu + 2) / 153; // Mart 0, Nisan 1, ..., Şubat 11
    let gun = yilin_gunu - (153 * mart_ayi + 2) / 5 + 1; // 1..=31
    let ay = if mart_ayi < 10 {
        mart_ayi + 3
    } else {
        mart_ayi - 9
    };
    // Ocak ve Şubat "Mart yılının" sonundadır, takvimde bir sonraki yıla aittir.
    // `i64::from(true)` 1, `i64::from(false)` 0'dır.
    let yil = donem * 400 + donem_yili + i64::from(ay <= 2);
    (yil, ay, gun)
}

/// `2026-10-24` → (2026, 10, 24). Ay 1–12, gün o ayın gün sayısıyla sınırlı.
fn tarih_ayristir(metin: &str) -> Option<(u16, u16, u16)> {
    let mut parcalar = metin.split('-');
    let yil = sayi(parcalar.next()?, 4..=4)?;
    let ay = sayi(parcalar.next()?, 2..=2)?;
    let gun = sayi(parcalar.next()?, 2..=2)?;
    let gecerli = parcalar.next().is_none()
        && (1..=12).contains(&ay)
        && (1..=ayin_gun_sayisi(yil, ay)).contains(&gun);
    gecerli.then_some((yil, ay, gun))
}

/// `15:00` ya da `9:30:15` → (saat, dakika, saniye). Saniye verilmezse 0.
fn saat_ayristir(metin: &str) -> Option<(u16, u16, u16)> {
    let mut parcalar = metin.split(':');
    let saat = sayi(parcalar.next()?, 1..=2)?;
    let dakika = sayi(parcalar.next()?, 2..=2)?;
    let saniye = match parcalar.next() {
        Some(parca) => sayi(parca, 2..=2)?,
        None => 0,
    };
    let gecerli = parcalar.next().is_none() && saat <= 23 && dakika <= 59 && saniye <= 59;
    gecerli.then_some((saat, dakika, saniye))
}

/// Basamak sayısı `basamak` aralığında olan bir sayı: `sayi("09", 2..=2)` → 9
fn sayi(metin: &str, basamak: RangeInclusive<usize>) -> Option<u16> {
    if !basamak.contains(&metin.len()) {
        return None;
    }
    rakamlar(metin.as_bytes())
}

/// 1–4 ASCII rakamını sayıya çevirir: `b"2026"` → 2026. Rakam olmayan bir bayt varsa None.
///
/// `fold`: bir başlangıç değeriyle (0) başlar, her rakamda önceki sonucu 10 ile çarpıp rakamı
/// ekler. En çok 4 basamak olduğu için sonuç 9999'u geçmez, `u16` taşmaz.
fn rakamlar(baytlar: &[u8]) -> Option<u16> {
    let gecerli = (1..=4).contains(&baytlar.len()) && baytlar.iter().all(u8::is_ascii_digit);
    gecerli.then(|| {
        baytlar
            .iter()
            .fold(0, |sonuc, bayt| sonuc * 10 + u16::from(bayt - b'0'))
    })
}

fn ayin_gun_sayisi(yil: u16, ay: u16) -> u16 {
    match ay {
        2 if artik_yil(yil) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Her 4 yılda bir artık yıl; 100'ün katları değil, 400'ün katları yine artık yıl.
fn artik_yil(yil: u16) -> bool {
    (yil.is_multiple_of(4) && !yil.is_multiple_of(100)) || yil.is_multiple_of(400)
}

#[cfg(test)]
mod testler {
    use super::*;

    fn tam(metin: &str) -> Zaman {
        match ZamanNoktasi::ayristir(metin) {
            Some(ZamanNoktasi::Tam(zaman)) => zaman,
            diger => panic!("{metin}: tam zaman bekleniyordu, gelen: {diger:?}"),
        }
    }

    #[test]
    fn gun_numarasi_takvimle_uyusur() {
        // Python'un datetime modülüyle hesaplanan değerler
        for (tarih, beklenen) in [
            ((1970, 1, 1), 0),
            ((1969, 12, 31), -1),
            ((2000, 3, 1), 11_017),
            ((2024, 2, 29), 19_782),
            ((2024, 3, 1), 19_783),
            ((2026, 10, 24), 20_750),
            ((2100, 3, 1), 47_541),
            // Hesabın başlangıç noktası: 0000-03-01
            ((0, 3, 1), -719_468),
            // Ocak, bir önceki "Mart yılına", -1. yıla aittir: `/` burada 0 verirdi,
            // `div_euclid` doğru dönemi (-1) verir
            ((0, 1, 1), -719_528),
        ] {
            let (yil, ay, gun) = tarih;
            assert_eq!(gun_numarasi(yil, ay, gun), beklenen, "{tarih:?}");
        }
    }

    #[test]
    fn takvim_gunu_gun_numarasinin_tersi() {
        // Python'un datetime modülüyle hesaplanan değerler
        for (gun, beklenen) in [
            (0, (1970, 1, 1)),
            (-1, (1969, 12, 31)),
            (11_016, (2000, 2, 29)),
            (19_782, (2024, 2, 29)),
            (20_750, (2026, 10, 24)),
            (-25_508, (1900, 3, 1)),
            (-719_528, (0, 1, 1)),
            (2_932_896, (9999, 12, 31)),
        ] {
            assert_eq!(takvim_gunu(gun), beklenen, "{gun}");
        }
        // Dört basamaklı yılların bütün günleri (3,6 milyon gün): takvimde olan bir tarih
        // vermeli ve o tarih aynı güne geri çevrilmeli
        for gun in -719_528..=2_932_896 {
            let (yil, ay, ayin_gunu) = takvim_gunu(gun);
            assert!((1..=12).contains(&ay), "{gun}");
            let uzunluk = i64::from(ayin_gun_sayisi(alan(yil), alan(ay)));
            assert!((1..=uzunluk).contains(&ayin_gunu), "{gun}");
            assert_eq!(gun_numarasi(yil, ay, ayin_gunu), gun);
        }
    }

    #[test]
    fn saniyeden_ana_geri_donulur() {
        for metin in [
            "2026-10-24 16:10:02",
            "1970-01-01 00:00:00",
            "1969-12-31 23:59:59",
            "2024-02-29 12:00:00",
            "0000-01-01 00:00:00",
            "9999-12-31 23:59:59",
        ] {
            let zaman = Zaman::kayittan(metin);
            assert_eq!(Zaman::saniyeden(zaman.saniye()), zaman, "{metin}");
        }
        assert_eq!(Zaman::saniyeden(-1).to_string(), "1969-12-31 23:59:59");
        // Dört basamaklı yılların dışı sınırda kalır
        assert_eq!(Zaman::saniyeden(i64::MIN), EN_ESKI);
        assert_eq!(Zaman::saniyeden(EN_ESKI.saniye() - 1), EN_ESKI);
        assert_eq!(Zaman::saniyeden(EN_YENI.saniye() + 1), EN_YENI);
        assert_eq!(
            Zaman::saniyeden(i64::MAX).to_string(),
            "9999-12-31 23:59:59"
        );
    }

    #[test]
    fn kayit_zamani_okunur() {
        let zaman = Zaman::kayittan("2026-10-24 14:03:11");
        assert_eq!(zaman.to_string(), "2026-10-24 14:03:11");
        // Kesir ve saat dilimi yok sayılır; T ile boşluk aynı anı verir
        assert_eq!(Zaman::kayittan("2026-10-24T14:03:11.123456+03:00"), zaman);
        assert_eq!(zaman.saniye(), 20_750 * 86_400 + 14 * 3_600 + 3 * 60 + 11);
    }

    #[test]
    fn zamanlar_kronolojik_siralanir() {
        let sirali = [
            "2025-12-31 23:59:59",
            "2026-01-01 00:00:00",
            "2026-01-01 00:00:01",
            "2026-01-01 00:01:00",
            "2026-01-01 01:00:00",
            "2026-01-02 00:00:00",
            "2026-02-01 00:00:00",
        ];
        for ikili in sirali.windows(2) {
            let (once, sonra) = (Zaman::kayittan(ikili[0]), Zaman::kayittan(ikili[1]));
            assert!(once < sonra, "{ikili:?}");
            assert!(once.saniye() < sonra.saniye(), "{ikili:?}");
        }
        // Yılın son saniyesi ile yeni yılın ilk saniyesi arasında tam 1 saniye var
        assert_eq!(
            Zaman::kayittan(sirali[1]).saniye() - Zaman::kayittan(sirali[0]).saniye(),
            1
        );
    }

    #[test]
    fn zaman_noktasi_yazilislari() {
        assert_eq!(
            ZamanNoktasi::ayristir("15:00"),
            Some(ZamanNoktasi::Saat {
                saat: 15,
                dakika: 0,
                saniye: 0
            })
        );
        assert_eq!(
            ZamanNoktasi::ayristir("9:30:15"),
            Some(ZamanNoktasi::Saat {
                saat: 9,
                dakika: 30,
                saniye: 15
            })
        );
        assert_eq!(tam("2026-10-24").to_string(), "2026-10-24 00:00:00");
        assert_eq!(tam("2026-10-24 15:00").to_string(), "2026-10-24 15:00:00");
        assert_eq!(
            tam("2026-10-24T15:00:30").to_string(),
            "2026-10-24 15:00:30"
        );
        assert_eq!(
            tam("2024-02-29 23:59:59").to_string(),
            "2024-02-29 23:59:59"
        );
    }

    #[test]
    fn gecersiz_zamanlar_reddedilir() {
        for metin in [
            "",
            "15",
            "15:0",
            "15:000",
            "15:00:0",
            "123:00",
            "015:00",
            "24:00",
            "15:60",
            "15:00:60",
            "15:00:00:00",
            "ab:cd",
            // Sıfır yerine büyük O harfi
            "15:0O",
            "2026-10",
            "2026-13-01",
            "2026-00-10",
            "2026-02-29",
            "2025-04-31",
            "2026-10-00",
            "26-10-24",
            "2026-10-24-1",
            "2026-10-24 15",
            "2026-10-24 25:00",
            "2026-10-24T",
            "15:00 ",
        ] {
            assert_eq!(ZamanNoktasi::ayristir(metin), None, "{metin:?}");
        }
    }

    #[test]
    fn yalnizca_saat_son_kaydin_gunune_uygulanir() {
        let son_kayit = Zaman::kayittan("2026-10-25 08:12:40");
        let nokta = ZamanNoktasi::ayristir("15:00").unwrap();
        assert_eq!(nokta.coz(son_kayit).to_string(), "2026-10-25 15:00:00");
        // Tam zaman verildiyse son kayıt değiştirmez
        let tam_nokta = ZamanNoktasi::ayristir("2026-10-24 15:00").unwrap();
        assert_eq!(tam_nokta.coz(son_kayit).to_string(), "2026-10-24 15:00:00");
    }

    #[test]
    fn saat_dilimi_komut_satirindan_okunur() {
        for (metin, saniye, yazilis) in [
            ("+03:00", 10_800, "+03:00"),
            ("+0300", 10_800, "+03:00"),
            ("+3", 10_800, "+03:00"),
            ("+03", 10_800, "+03:00"),
            ("+3:30", 12_600, "+03:30"),
            ("-05:30", -19_800, "-05:30"),
            ("+14:00", 50_400, "+14:00"),
            ("-12:00", -43_200, "-12:00"),
            ("+23:59", 86_340, "+23:59"),
            ("-23:59", -86_340, "-23:59"),
            ("Z", 0, "+00:00"),
            ("z", 0, "+00:00"),
            ("utc", 0, "+00:00"),
            ("UTC", 0, "+00:00"),
            ("+00:00", 0, "+00:00"),
            ("-00:00", 0, "+00:00"),
        ] {
            let dilim = SaatDilimi::ayristir(metin).unwrap();
            assert_eq!(dilim.saniye(), saniye, "{metin}");
            assert_eq!(dilim.to_string(), yazilis, "{metin}");
        }
        for metin in [
            "",
            "3",
            "03:00",
            "+",
            "-",
            "+24",
            "+24:00",
            "+03:60",
            "+3:0",
            "+003:00",
            "+030",
            "++03",
            "+03:00:00",
            "+aa",
            "+0a:00",
            "+03:0a",
            " +03:00",
            "+03:00 ",
            "GMT",
            "Europe/Istanbul",
            // Dört bayt ama ikinci bayttan sonrası bir harfin ortası: bölünemez, panik olmaz
            "+0ş0",
            "+ş00",
            "-00ş",
            "+٣٠٠",
            "+３:００",
        ] {
            assert_eq!(SaatDilimi::ayristir(metin), None, "{metin:?}");
        }
    }

    #[test]
    fn kayit_zamanindaki_saat_dilimi_okunur() {
        let dilim = |metin| SaatDilimi::kayittan(metin).map(SaatDilimi::saniye);
        assert_eq!(dilim("2026-10-24 14:03:11"), None);
        assert_eq!(dilim("2026-10-24T14:03:11"), None);
        assert_eq!(dilim("2026-10-24T14:03:11.123456"), None);
        assert_eq!(dilim("2026-10-24T14:03:11+03:00"), Some(10_800));
        assert_eq!(dilim("2026-10-24T14:03:11.5+0300"), Some(10_800));
        assert_eq!(dilim("2026-10-24T14:03:11.123456-04:30"), Some(-16_200));
        assert_eq!(dilim("2026-10-24T14:03:11Z"), Some(0));
        assert_eq!(dilim("2026-10-24T14:03:11.123Z"), Some(0));
        assert_eq!(dilim("2026-10-24T14:03:11+23:59"), Some(86_340));
        assert_eq!(dilim("2026-10-24T14:03"), None);
    }

    #[test]
    fn zaman_eki_denetlenir() {
        let ek =
            |metin: &str| zaman_eki(metin.as_bytes()).map(|dilim| dilim.map(SaatDilimi::saniye));
        assert_eq!(ek(""), Some(None));
        assert_eq!(ek(".123456"), Some(None));
        assert_eq!(ek("Z"), Some(Some(0)));
        assert_eq!(ek(".5Z"), Some(Some(0)));
        assert_eq!(ek("+03:00"), Some(Some(10_800)));
        assert_eq!(ek("-0330"), Some(Some(-12_600)));
        assert_eq!(ek(".000001+03:00"), Some(Some(10_800)));
        for gecersiz in [
            ".", ".Z", "z", "+3:00", "+03:0", "+03:000", "+0300Z", "+24:00", "-24:00", "+03:60",
            "+0360", "*03:00", "+0a:00", "+03:0b", " +03:00", "+03:00 ", "Z ", "UTC",
        ] {
            assert_eq!(ek(gecersiz), None, "{gecersiz:?}");
        }
    }

    #[test]
    fn kayitlar_raporun_dilimine_cevrilir() {
        let yerel = Zaman::kayittan;
        // Dilim verilmedi: ilk dilimli kayıttan belirlenir; önceki dilimsiz kayıt çevrilmez
        let mut cevirici = DilimCevirici::new(None);
        assert!(!cevirici.verildi());
        assert_eq!(
            cevirici.cevir("2026-10-24 09:00:00"),
            (yerel("2026-10-24 09:00:00"), KayitDilimi::Yazilmamis)
        );
        assert_eq!(cevirici.dilim(), None);
        assert_eq!(
            cevirici.cevir("2026-10-24T10:00:00+02:00"),
            (yerel("2026-10-24 10:00:00"), KayitDilimi::Ayni)
        );
        let iki = SaatDilimi::ayristir("+02:00").unwrap();
        assert_eq!(cevirici.dilim(), Some(iki));
        // Yaz saatinden kış saatine: 02:30+01:00, 03:30+02:00'dır
        let bir = SaatDilimi::ayristir("+01:00").unwrap();
        assert_eq!(
            cevirici.cevir("2026-10-25T02:30:00+01:00"),
            (yerel("2026-10-25 03:30:00"), KayitDilimi::Baska(bir))
        );
        // Dilim bir kez belirlenince değişmez
        assert_eq!(cevirici.dilim(), Some(iki));
        // Dilimsiz kayıt raporun diliminde sayılır
        assert_eq!(
            cevirici.cevir("2026-10-25 04:00:00"),
            (yerel("2026-10-25 04:00:00"), KayitDilimi::Yazilmamis)
        );

        // Verilen dilime çevrilir; gece yarısını, ayı ve yılı aşar
        let mut cevirici = DilimCevirici::new(SaatDilimi::ayristir("+03:00"));
        assert!(cevirici.verildi());
        assert_eq!(
            cevirici.cevir("2026-12-31T22:30:00Z"),
            (
                yerel("2027-01-01 01:30:00"),
                KayitDilimi::Baska(SaatDilimi::UTC)
            )
        );
        assert_eq!(
            cevirici.cevir("2026-10-31T22:30:00.123+03:00"),
            (yerel("2026-10-31 22:30:00"), KayitDilimi::Ayni)
        );
        // Geri yönde de: 2027-01-01 02:00-05:00 = 2027-01-01 10:00+03:00; 2026-03-01 00:30+05:00 =
        // 2026-02-28 22:30+03:00 (artık olmayan yıl)
        assert_eq!(
            cevirici.cevir("2027-01-01T02:00:00-05:00").0,
            yerel("2027-01-01 10:00:00")
        );
        assert_eq!(
            cevirici.cevir("2026-03-01T00:30:00+05:00").0,
            yerel("2026-02-28 22:30:00")
        );
        // Kendisi verilen dilimde olan kayıt
        assert_eq!(cevirici.dilim(), SaatDilimi::ayristir("+03:00"));
    }

    #[test]
    fn ayni_gun_ve_yil() {
        let zaman = Zaman::kayittan;
        assert!(zaman("2026-10-24 00:00:00").ayni_gun(zaman("2026-10-24 23:59:59")));
        assert!(!zaman("2026-10-24 23:59:59").ayni_gun(zaman("2026-10-25 00:00:00")));
        assert!(!zaman("2026-10-24 10:00:00").ayni_gun(zaman("2026-11-24 10:00:00")));
        assert!(!zaman("2026-10-24 10:00:00").ayni_gun(zaman("2025-10-24 10:00:00")));
        assert!(zaman("2026-01-01 00:00:00").ayni_yil(zaman("2026-12-31 23:59:59")));
        assert!(!zaman("2026-12-31 23:59:59").ayni_yil(zaman("2027-01-01 00:00:00")));
    }

    #[test]
    fn sure_tum_bilesenleriyle_yazilir() {
        assert_eq!(sure_tam_yaz(1_800), "30 dk");
        assert_eq!(sure_tam_yaz(5_400), "1 sa 30 dk");
        assert_eq!(sure_tam_yaz(86_400), "1 gün");
        assert_eq!(sure_tam_yaz(93_784), "1 gün 2 sa 3 dk 4 sn");
        assert_eq!(sure_tam_yaz(45), "45 sn");
        assert_eq!(sure_tam_yaz(0), "0 sn");
    }

    #[test]
    fn sure_okunur_yazilir() {
        assert_eq!(sure_yaz(45), "45 sn");
        assert_eq!(sure_yaz(60), "1 dk 0 sn");
        assert_eq!(sure_yaz(12 * 60 + 5), "12 dk 5 sn");
        assert_eq!(sure_yaz(3_600), "1 sa 0 dk");
        assert_eq!(sure_yaz(21_602), "6 sa 0 dk");
        assert_eq!(sure_yaz(12_668), "3 sa 31 dk");
        assert_eq!(sure_yaz(86_400), "1 gün 0 sa");
        assert_eq!(sure_yaz(2 * 86_400 + 3 * 3_600 + 59), "2 gün 3 sa");
    }

    #[test]
    fn ingilizce_sureler() {
        let _dil = dil::gecici(dil::Dil::En);
        assert_eq!(sure_tam_yaz(5_400), "1 h 30 min");
        assert_eq!(sure_tam_yaz(93_784), "1 d 2 h 3 min 4 s");
        assert_eq!(sure_tam_yaz(0), "0 s");
        assert_eq!(sure_yaz(12 * 60 + 5), "12 min 5 s");
        assert_eq!(sure_yaz(21_602), "6 h 0 min");
        assert_eq!(sure_yaz(2 * 86_400 + 3 * 3_600 + 59), "2 d 3 h");
    }

    #[test]
    fn takvimde_olmayan_anlar_taninir() {
        for metin in [
            "2026-10-24 14:03:11",
            "2024-02-29 00:00:00",
            "2000-02-29 23:59:59",
            "0000-01-01 00:00:00",
            "9999-12-31 23:59:59",
        ] {
            assert!(Zaman::kayittan(metin).takvimde_var_mi(), "{metin}");
        }
        for metin in [
            "2026-02-29 10:00:00",
            "2026-02-30 10:00:00",
            "1900-02-29 10:00:00",
            "2026-04-31 10:00:00",
            "2026-00-10 10:00:00",
            "2026-13-01 10:00:00",
            "2026-10-00 10:00:00",
            "2026-10-24 24:00:00",
            "2026-10-24 23:60:00",
            "2026-10-24 23:59:60",
        ] {
            assert!(!Zaman::kayittan(metin).takvimde_var_mi(), "{metin}");
        }
        // Takvimde olmayan bir an sıralama ile saniyenin çeliştiği yerdir: 30 Şubat 1 Mart'tan
        // önce sıralanır ama saniyesi daha büyüktür
        let (otuz_subat, bir_mart) = (
            Zaman::kayittan("2026-02-30 00:00:00"),
            Zaman::kayittan("2026-03-01 00:00:00"),
        );
        assert!(otuz_subat < bir_mart);
        assert!(otuz_subat.saniye() > bir_mart.saniye());
    }

    #[test]
    fn ay_uzunluklari_ve_artik_yillar() {
        let uzunluklar: Vec<u16> = (1..=12).map(|ay| ayin_gun_sayisi(2026, ay)).collect();
        assert_eq!(uzunluklar, [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]);
        // 4'e bölünen yıllar artık yıldır; 100'e bölünenler değil, 400'e bölünenler yine artık
        for (yil, subat) in [
            (2024, 29),
            (2026, 28),
            (1900, 28),
            (2000, 29),
            (2100, 28),
            (2400, 29),
        ] {
            assert_eq!(ayin_gun_sayisi(yil, 2), subat, "{yil}");
        }
    }
}
