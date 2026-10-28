//! Arayüzün dili: İngilizce (varsayılan) ya da Türkçe (`--lang tr`, `VEYLOG_LANG=tr`).
//!
//! Dil programın başında bir kez seçilir ve iş parçacığına yazılır (`ayarla`); metin üreten her
//! yer onu buradan okur (`simdiki`, `sec`). Böylece hata türlerinin `Display`'i de, sayıları
//! yazan küçük işlevler de bir dil parametresi taşımadan iki dilde yazabilir. Çıktı yalnızca ana
//! iş parçacığında üretilir. Testler kendi iş parçacıklarında çalışır: bir testin seçtiği dil
//! ötekileri etkilemez.
//!
//! Değişmeyenler: JSON çıktısı (alan adları, `method` gibi değerler ve `<NUM>` gibi yer
//! tutucular) dilden bağımsızdır ve İngilizcedir; betikler dil değişince bozulmasın. Seviye
//! adları (`ERROR`) ve zamanlar her dilde aynıdır.

use std::borrow::Cow;
use std::cell::Cell;

/// Arayüzün dili.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dil {
    /// Türkçe: virgüllü ondalık (`0,7`), yüzde işareti önde (`%1`).
    Tr,
    /// İngilizce, varsayılan dil: noktalı ondalık (`0.7`), yüzde işareti sonda (`1%`).
    #[default]
    En,
}

impl Dil {
    /// Bütün diller; yardım metni ve hata mesajları bu listeden üretilir.
    pub const HEPSI: [Dil; 2] = [Dil::Tr, Dil::En];

    /// Komut satırındaki adı: `tr`, `en`.
    pub fn ad(self) -> &'static str {
        match self {
            Dil::Tr => "tr",
            Dil::En => "en",
        }
    }

    /// Adından dile çevirir; büyük-küçük harf fark etmez: `en`, `EN`, `tr`.
    pub fn ayristir(ad: &str) -> Option<Dil> {
        Dil::HEPSI
            .into_iter()
            .find(|dil| dil.ad().eq_ignore_ascii_case(ad.trim()))
    }
}

thread_local! {
    /// Bu iş parçacığının dili. Program onu başında seçer (`ayarla`); seçilmeden önce
    /// Türkçedir, çünkü birim testlerinin çoğu Türkçe çıktıyı denetler. İngilizceyi denetleyen
    /// testler onu `gecici` ile seçer.
    static DIL: Cell<Dil> = const { Cell::new(Dil::Tr) };
}

/// Bu iş parçacığında üretilecek metinlerin dilini seçer.
pub fn ayarla(dil: Dil) {
    DIL.with(|secili| secili.set(dil));
}

/// Seçili dil.
pub fn simdiki() -> Dil {
    DIL.with(Cell::get)
}

/// Testlerde: dili seçer; dönen koruyucu düşünce (test başarısız olsa da) önceki dile dönülür.
#[cfg(test)]
#[must_use = "koruyucu düşünce dil eskisine döner"]
pub fn gecici(dil: Dil) -> Koruyucu {
    let onceki = simdiki();
    ayarla(dil);
    Koruyucu(onceki)
}

/// `gecici`nin döndürdüğü koruyucu.
#[cfg(test)]
pub struct Koruyucu(Dil);

#[cfg(test)]
impl Drop for Koruyucu {
    fn drop(&mut self) {
        ayarla(self.0);
    }
}

/// Seçili dildeki metin: Türkçesi ya da İngilizcesi.
pub fn sec<T>(tr: T, en: T) -> T {
    match simdiki() {
        Dil::Tr => tr,
        Dil::En => en,
    }
}

/// Bir seçeneğin seçili dildeki adı: mesajlar kullanıcının dilindeki adı göstersin
/// (`--son` → `--last`). Tanınmayan ad olduğu gibi kalır.
pub fn secenek(ad: &'static str) -> &'static str {
    if simdiki() == Dil::Tr {
        return ad;
    }
    SECENEK_ADLARI
        .iter()
        .find(|&&(tr, _)| tr == ad)
        .map_or(ad, |&(_, en)| en)
}

/// Uzun seçeneklerin Türkçe ve İngilizce adları. İkisi de her dilde geçerlidir; yardım metni
/// ve mesajlar seçili dildekini gösterir.
pub const SECENEK_ADLARI: [(&str, &str); 20] = [
    ("--esik", "--threshold"),
    ("--konum", "--location"),
    ("--ilk", "--top"),
    ("--hepsi", "--all"),
    ("--siralama", "--sort"),
    ("--bicim", "--format"),
    ("--karsilastir", "--compare"),
    ("--ani-artis", "--spikes"),
    ("--seviye", "--level"),
    ("--baslangic", "--since"),
    ("--bitis", "--until"),
    ("--son", "--last"),
    ("--saat-dilimi", "--timezone"),
    ("--desen", "--pattern"),
    ("--zaman-bicimi", "--time-format"),
    ("--izle", "--watch"),
    ("--aralik", "--interval"),
    ("--dil", "--lang"),
    ("--yardim", "--help"),
    ("--surum", "--version"),
];

/// Ondalık sayıyı seçili dilde yazar: Türkçede virgülle (`0,7`), İngilizcede noktayla (`0.7`).
/// `metin`, sayının noktalı yazılışıdır.
pub fn ondalik_ayraci(metin: String) -> String {
    match simdiki() {
        Dil::Tr => metin.replace('.', ","),
        Dil::En => metin,
    }
}

/// Yüzde: Türkçede işaret önde (`%1`), İngilizcede sonda (`1%`).
pub fn yuzde(sayi: &str) -> String {
    match simdiki() {
        Dil::Tr => format!("%{sayi}"),
        Dil::En => format!("{sayi}%"),
    }
}

/// Sayı ve adı: Türkçede ad sayıdan sonra çekimsizdir (`3 kayıt`), İngilizcede sayıya göre
/// tekil ya da çoğuldur (`1 record`, `3 records`).
pub fn sayili(sayi: usize, tr: &str, tekil: &str, cogul: &str) -> String {
    match simdiki() {
        Dil::Tr => format!("{sayi} {tr}"),
        Dil::En if sayi == 1 => format!("{sayi} {tekil}"),
        Dil::En => format!("{sayi} {cogul}"),
    }
}

/// "a", "a ve b", "a, b ve c"; İngilizcede "a and b", "a, b and c".
pub fn liste<S: AsRef<str>>(ogeler: &[S]) -> String {
    match ogeler {
        [] => String::new(),
        [tek] => tek.as_ref().to_owned(),
        [onceki @ .., son] => {
            let onceki: Vec<&str> = onceki.iter().map(AsRef::as_ref).collect();
            format!(
                "{} {} {}",
                onceki.join(", "),
                sec("ve", "and"),
                son.as_ref()
            )
        }
    }
}

/// Maskelemenin yer tutucuları: Türkçe adları (depoda ve Türkçe arayüzde) ve İngilizce adları
/// (İngilizce arayüzde ve JSON'da). `<URL>`, `<UUID>` ve `<IP>` iki dilde aynıdır.
const YER_TUTUCULAR: [(&str, &str); 4] = [
    ("<SAYI>", "<NUM>"),
    ("<YOL>", "<PATH>"),
    ("<EPOSTA>", "<EMAIL>"),
    ("<ZAMAN>", "<TIME>"),
];

/// Kalıptaki yer tutucuları seçili dilde gösterir: İngilizcede `<SAYI>` → `<NUM>`. Kalıp
/// Türkçe adlarla saklanır ve kümelenir; çeviri yalnızca çıktıdadır.
pub fn yer_tutucular(kalip: &str) -> Cow<'_, str> {
    if simdiki() == Dil::Tr {
        return Cow::Borrowed(kalip);
    }
    ingilizce_yer_tutucular(kalip)
}

/// Kalıptaki yer tutucuların İngilizce adları, seçili dilden bağımsız: JSON her dilde bunları
/// yazar.
pub fn ingilizce_yer_tutucular(kalip: &str) -> Cow<'_, str> {
    if !kalip.contains('<') {
        return Cow::Borrowed(kalip);
    }
    let mut sonuc = Cow::Borrowed(kalip);
    for (tr, en) in YER_TUTUCULAR {
        if sonuc.contains(tr) {
            sonuc = Cow::Owned(sonuc.replace(tr, en));
        }
    }
    sonuc
}

#[cfg(test)]
mod testler {
    use super::*;

    #[test]
    fn dil_adlari() {
        assert_eq!(Dil::ayristir("en"), Some(Dil::En));
        assert_eq!(Dil::ayristir(" EN "), Some(Dil::En));
        assert_eq!(Dil::ayristir("Tr"), Some(Dil::Tr));
        assert_eq!(Dil::ayristir("de"), None);
        assert_eq!(Dil::ayristir(""), None);
        for dil in Dil::HEPSI {
            assert_eq!(Dil::ayristir(dil.ad()), Some(dil));
        }
    }

    #[test]
    fn dil_is_parcacigina_ozeldir() {
        assert_eq!(simdiki(), Dil::Tr);
        {
            let _dil = gecici(Dil::En);
            assert_eq!(sec("Kayıt", "Records"), "Records");
            // Başka bir iş parçacığı varsayılan dilde başlar
            let oteki = std::thread::spawn(|| (simdiki(), sec(1, 2)))
                .join()
                .unwrap();
            assert_eq!(oteki, (Dil::Tr, 1));
        }
        // Koruyucu düşünce önceki dile dönüldü
        assert_eq!(sec("Kayıt", "Records"), "Kayıt");
    }

    #[test]
    fn sayilar_ve_listeler_dilde_yazilir() {
        assert_eq!(ondalik_ayraci("0.7".to_owned()), "0,7");
        assert_eq!(yuzde("1"), "%1");
        assert_eq!(liste(&["a", "b", "c"]), "a, b ve c");
        assert_eq!(sayili(1, "kayıt", "record", "records"), "1 kayıt");
        assert_eq!(sayili(2, "kayıt", "record", "records"), "2 kayıt");
        assert_eq!(liste(&["a"]), "a");
        assert_eq!(liste::<&str>(&[]), "");
        let _dil = gecici(Dil::En);
        assert_eq!(ondalik_ayraci("0.7".to_owned()), "0.7");
        assert_eq!(yuzde("1"), "1%");
        assert_eq!(liste(&["a", "b"]), "a and b");
        assert_eq!(liste(&["a", "b", "c"]), "a, b and c");
        assert_eq!(sayili(1, "kayıt", "record", "records"), "1 record");
        assert_eq!(sayili(0, "kayıt", "record", "records"), "0 records");
        assert_eq!(sayili(2, "kayıt", "record", "records"), "2 records");
    }

    #[test]
    fn secenek_adlari() {
        assert_eq!(secenek("--son"), "--son");
        {
            let _dil = gecici(Dil::En);
            assert_eq!(secenek("--son"), "--last");
            assert_eq!(secenek("--izle"), "--watch");
            // Tanınmayan ad olduğu gibi kalır
            assert_eq!(secenek("--x"), "--x");
        }
        // Her Türkçe ad ve her İngilizce ad bir kez geçer
        let mut adlar: Vec<&str> = SECENEK_ADLARI
            .iter()
            .flat_map(|&(tr, en)| [tr, en])
            .collect();
        let sayi = adlar.len();
        adlar.sort_unstable();
        adlar.dedup();
        assert_eq!(adlar.len(), sayi);
    }

    #[test]
    fn yer_tutucular_ingilizcede_cevrilir() {
        let kalip = "Kullanıcı <SAYI> giriş yaptı: <YOL> <EPOSTA> <ZAMAN> <IP> <URL> <*>";
        assert!(matches!(yer_tutucular(kalip), Cow::Borrowed(_)));
        let _dil = gecici(Dil::En);
        assert_eq!(
            yer_tutucular(kalip),
            "Kullanıcı <NUM> giriş yaptı: <PATH> <EMAIL> <TIME> <IP> <URL> <*>"
        );
        assert!(matches!(yer_tutucular("<IP>:<*>"), Cow::Borrowed(_)));
        assert!(matches!(yer_tutucular("düz metin"), Cow::Borrowed(_)));
    }
}
