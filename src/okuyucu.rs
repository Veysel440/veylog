//! Girdiyi satır satır okur: dosya ya da standart girdi, baştan sona tek geçişte.
//!
//! Dosyanın tamamı belleğe alınmaz. Her satır okunur, kayıt toplayıcıya verilir ve bırakılır;
//! bellekte o an okunan satır ve toplanmakta olan kayıt durur. Satır sonları `str::lines` gibi
//! ele alınır: `\n` ya da `\r\n`.
//!
//! Geçersiz UTF-8 baytları dosyanın tamamını reddetmek yerine � ile değiştirilir. `\n` hiçbir
//! çok baytlı UTF-8 karakterinin parçası olamayacağı için bu, satır satır yapılınca da dosyanın
//! tamamına yapılmış gibi sonuç verir.
//!
//! Bir kaydın mesajı sonradan gerekirse (ani artışın örneği) kayıt yeniden okunur: her
//! `DIZIN_ARALIGI` kayıtta bir, kaydın girdideki bayt konumu saklanır (seyrek dizin). Aranan
//! kaydın bloğunun başına atlanır, oradan en fazla o kadar kayıt okunur. Bulunan kaydın maskeli
//! mesajı ve seviyesi depodakiyle karşılaştırılır: girdi okunduktan sonra değiştiyse (ör. log
//! döndürülürken `copytruncate` ile kesilip yeniden yazıldıysa) yanlış bir kayıt örnek olmaz.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io::{self, BufRead, Read, Seek, SeekFrom, Write};

use crate::filtre::CozulmusFiltre;
use crate::kayit::{Kayit, KayitAlici, KayitToplayici, SatirSayilari, Seviye, kayit_baslangici};
use crate::maske::maskele;
use crate::zaman::Zaman;

/// Bir satırın en fazla bu kadar baytı (256 MB) okunur; ötesi atlanır. Gerçek bir logda en
/// uzun satırlar, bağlamında dev bir SQL sorgusu taşıyan istisnalardır; onlar da bunun çok
/// altındadır. Sınır, satır sonu olmayan dev bir dosyanın belleği doldurmasını önler.
pub const SATIR_SINIRI: usize = 256 << 20;

/// Seyrek dizinin aralığı: her bu kadar kayıtta bir kaydın konumu saklanır.
pub const DIZIN_ARALIGI: usize = 1024;

/// Satır okuyucu: satırları ve başlarının girdideki bayt konumunu verir.
pub struct SatirOkuyucu<R> {
    girdi: R,
    /// Son okunan satırın baytları; satırdan satıra yeniden kullanılır.
    tampon: Vec<u8>,
    /// Bir sonraki satırın başının konumu.
    konum: u64,
    /// Satırın en fazla okunacak baytı: `SATIR_SINIRI` (testlerde daha küçük).
    sinir: usize,
    gecersiz_utf8: bool,
}

impl<R: BufRead> SatirOkuyucu<R> {
    /// Girdinin başından okur.
    pub fn new(girdi: R) -> Self {
        SatirOkuyucu::konumdan(girdi, 0)
    }

    /// Girdinin içinde bir yerden okur; konumlar `konum`dan başlayarak sayılır.
    fn konumdan(girdi: R, konum: u64) -> Self {
        SatirOkuyucu {
            girdi,
            tampon: Vec::new(),
            konum,
            sinir: SATIR_SINIRI,
            gecersiz_utf8: false,
        }
    }

    /// Satırların en fazla `sinir` baytını okuyan bir okuyucu (testler için).
    #[cfg(test)]
    fn sinirli(girdi: R, sinir: usize) -> Self {
        SatirOkuyucu {
            sinir,
            ..SatirOkuyucu::new(girdi)
        }
    }

    /// Bir sonraki satır (satır sonu olmadan) ve başının konumu; girdi bittiyse None.
    pub fn sonraki(&mut self) -> io::Result<Option<(Cow<'_, str>, u64)>> {
        self.tampon.clear();
        let bas = self.konum;
        // `take`: en fazla sınır kadar oku. `&mut` ile: okuyucu tüketilmez, ödünç alınır.
        let okunan = (&mut self.girdi)
            .take(self.sinir as u64)
            .read_until(b'\n', &mut self.tampon)?;
        if okunan == 0 {
            return Ok(None);
        }
        self.konum += okunan as u64;
        let mut satir_sonu = self.tampon.last() == Some(&b'\n');
        if !satir_sonu && okunan == self.sinir {
            if self.girdi.fill_buf()?.first() == Some(&b'\n') {
                // Satır sınıra tam sığdı; satır sonu hemen arkasında
                self.girdi.consume(1);
                self.konum += 1;
                satir_sonu = true;
            } else {
                // Satır sınırı aştı: kalanı satır sonuna kadar atlanır
                let atlanan = self.girdi.skip_until(b'\n')?;
                self.konum += atlanan as u64;
                // Kesilen yerde yarım kalan bir UTF-8 karakteri atılır; geçersiz bayt
                // sayılmasın. Girdi tam sınırda bittiyse satır kesilmemiştir.
                if atlanan > 0
                    && let Err(hata) = std::str::from_utf8(&self.tampon)
                    && hata.error_len().is_none()
                {
                    self.tampon.truncate(hata.valid_up_to());
                }
            }
        }
        // `str::lines` gibi: satır sonu `\n` ya da `\r\n`
        if satir_sonu {
            if self.tampon.last() == Some(&b'\n') {
                self.tampon.pop();
            }
            if self.tampon.last() == Some(&b'\r') {
                self.tampon.pop();
            }
        }
        let metin = String::from_utf8_lossy(&self.tampon);
        if let Cow::Owned(_) = metin {
            self.gecersiz_utf8 = true;
        }
        Ok(Some((metin, bas)))
    }
}

/// Okumanın sonucu: satır sayıları ve geçersiz UTF-8 baytı görülüp görülmediği.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OkumaSonucu {
    pub sayilar: SatirSayilari,
    pub gecersiz_utf8: bool,
}

/// Girdiyi baştan sona okur; her kaydı tamamlanınca `alici`ya verir.
pub fn oku<R: BufRead, A: KayitAlici + ?Sized>(girdi: R, alici: &mut A) -> io::Result<OkumaSonucu> {
    let mut okuyucu = SatirOkuyucu::new(girdi);
    let mut toplayici = KayitToplayici::default();
    while let Some((satir, konum)) = okuyucu.sonraki()? {
        toplayici.satir(&satir, konum, alici);
    }
    toplayici.bitir(alici);
    Ok(OkumaSonucu {
        sayilar: toplayici.sayilar(),
        gecersiz_utf8: okuyucu.gecersiz_utf8,
    })
}

/// Girdiyi baştan sona tarar ve en yeni kaydın zamanını bulur; hiç kayıt yoksa None.
///
/// Logun sonuna göre çözülen filtreler (`--son 2sa`, `--baslangic 15:00`) için bir ön taramadır.
/// Kayıtlar toplanmaz: yalnızca bir kaydın başladığı satırlara bakılır.
pub fn en_yeni_zaman<R: BufRead>(girdi: R) -> io::Result<Option<Zaman>> {
    let mut okuyucu = SatirOkuyucu::new(girdi);
    let mut en_yeni = None;
    while let Some((satir, _)) = okuyucu.sonraki()? {
        if let Some(kayit) = kayit_baslangici(&satir) {
            let zaman = Zaman::kayittan(kayit.zaman());
            en_yeni = Some(en_yeni.map_or(zaman, |en_yeni: Zaman| en_yeni.max(zaman)));
        }
    }
    Ok(en_yeni)
}

/// Okunan her baytı bir kopyaya da yazan okuyucu. Standart girdi yalnızca bir kez
/// okunabildiği için, sonradan yeniden okunması gerekecekse geçici bir dosyaya kopyalanır.
///
/// Kopyaya yazmak başarısız olursa (disk dolu) okuma sürer, hata `bitir`de döner.
pub struct Kopyalayan<R, W> {
    ic: R,
    kopya: W,
    hata: Option<io::Error>,
}

impl<R: BufRead, W: Write> Kopyalayan<R, W> {
    pub fn new(ic: R, kopya: W) -> Self {
        Kopyalayan {
            ic,
            kopya,
            hata: None,
        }
    }

    /// Kopyayı döndürür; yazılırken bir hata olduysa o hatayı.
    pub fn bitir(self) -> io::Result<W> {
        match self.hata {
            Some(hata) => Err(hata),
            None => Ok(self.kopya),
        }
    }
}

impl<R: BufRead, W: Write> Read for Kopyalayan<R, W> {
    fn read(&mut self, tampon: &mut [u8]) -> io::Result<usize> {
        let okunan = self.ic.read(tampon)?;
        if self.hata.is_none()
            && let Err(hata) = self.kopya.write_all(&tampon[..okunan])
        {
            self.hata = Some(hata);
        }
        Ok(okunan)
    }
}

impl<R: BufRead, W: Write> BufRead for Kopyalayan<R, W> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.ic.fill_buf()
    }

    /// Tüketilen baytlar tüketilmeden önce kopyaya yazılır. `fill_buf` burada yeni bir şey
    /// okumaz: tamponda daha önce verilen baytlar durmaktadır. Hiçbir şey tüketilmiyorsa
    /// tampona hiç bakılmaz: tampon boşken `fill_buf` girdiden okurdu (terminalde bir satır
    /// daha beklerdi).
    fn consume(&mut self, miktar: usize) {
        if miktar == 0 {
            return;
        }
        if self.hata.is_none() {
            // Alanlar ayrı ayrı ödünç alınır: tampon `ic`ten, yazılan `kopya`ya
            match self.ic.fill_buf() {
                Ok(tampon) => {
                    if let Err(hata) = self.kopya.write_all(&tampon[..miktar]) {
                        self.hata = Some(hata);
                    }
                }
                Err(hata) => self.hata = Some(hata),
            }
        }
        self.ic.consume(miktar);
    }
}

/// Girdiden yeniden okunacak bir kayıt: sıra numarası ve depodaki halinden bilinenler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrnekIstegi<'a> {
    pub sira: usize,
    /// Kaydın maskeli mesajı.
    pub maskeli: &'a str,
    pub seviye: Seviye,
}

/// Bir kaydın mesajını sonradan bulmak için girdiyi yeniden okuyan kaynak.
pub trait OrnekKaynagi {
    /// İstenen kayıtların maskelenmemiş mesajları, sıra numaralarıyla. Bulunamayan kayıt
    /// sonuçta yer almaz: girdi bu arada kısaldıysa ya da o sıradaki kaydın maskeli mesajı ve
    /// seviyesi istenene uymuyorsa (girdi bu arada değiştiyse).
    fn mesajlar(&mut self, istekler: &[OrnekIstegi<'_>]) -> io::Result<HashMap<usize, String>>;

    /// Yeniden okunurken girdinin okunduktan sonra değiştiği anlaşıldı mı?
    fn degisti(&self) -> bool {
        false
    }
}

/// Yeniden okunabilen bir girdi (dosya ya da kopyası) ve onun seyrek dizini.
pub struct YenidenOkuyucu<R> {
    girdi: R,
    /// `dizin[i]`: `i × DIZIN_ARALIGI`. kaydın başlık satırının konumu.
    dizin: Vec<u64>,
    /// Depo filtreliyse aynı filtre: sıra numaraları yalnızca filtreye uyan kayıtlar
    /// arasındadır, yeniden okurken de öyle sayılır.
    filtre: Option<CozulmusFiltre>,
    degisti: bool,
}

impl<R: BufRead + Seek> YenidenOkuyucu<R> {
    pub fn new(girdi: R, dizin: Vec<u64>, filtre: Option<CozulmusFiltre>) -> Self {
        YenidenOkuyucu {
            girdi,
            dizin,
            filtre,
            degisti: false,
        }
    }
}

/// Kayıt depoya girmiş miydi? Filtre yoksa her kayıt girer.
fn depoda_mi(filtre: Option<&CozulmusFiltre>, kayit: &Kayit<'_>) -> bool {
    filtre
        .is_none_or(|filtre| filtre.gecer(Zaman::kayittan(kayit.zaman()).saniye(), kayit.seviye()))
}

impl<R: BufRead + Seek> OrnekKaynagi for YenidenOkuyucu<R> {
    fn mesajlar(&mut self, istekler: &[OrnekIstegi<'_>]) -> io::Result<HashMap<usize, String>> {
        let mut istenenler: Vec<&OrnekIstegi<'_>> = istekler.iter().collect();
        istenenler.sort_unstable_by_key(|istek| istek.sira);
        istenenler.dedup_by_key(|istek| istek.sira);
        let mut bulunanlar = HashMap::new();
        // Aynı bloktaki kayıtlar tek okumada bulunur
        for blok in istenenler.chunk_by(|a, b| a.sira / DIZIN_ARALIGI == b.sira / DIZIN_ARALIGI) {
            let numara = blok[0].sira / DIZIN_ARALIGI;
            let Some(&konum) = self.dizin.get(numara) else {
                continue;
            };
            self.girdi.seek(SeekFrom::Start(konum))?;
            let mut okuyucu = SatirOkuyucu::konumdan(&mut self.girdi, konum);
            let mut siradaki = numara * DIZIN_ARALIGI;
            let mut aranan = blok.iter().peekable();
            while let Some(&&istek) = aranan.peek() {
                let Some((satir, _)) = okuyucu.sonraki()? else {
                    // Girdi kısalmış
                    self.degisti = true;
                    break;
                };
                let Some(kayit) = kayit_baslangici(&satir) else {
                    continue;
                };
                if !depoda_mi(self.filtre.as_ref(), &kayit) {
                    continue;
                }
                if siradaki == istek.sira {
                    if kayit.seviye() == istek.seviye && maskele(kayit.mesaj()) == istek.maskeli {
                        bulunanlar.insert(istek.sira, kayit.mesaj().to_owned());
                    } else {
                        self.degisti = true;
                    }
                    aranan.next();
                }
                siradaki += 1;
            }
        }
        Ok(bulunanlar)
    }

    fn degisti(&self) -> bool {
        self.degisti
    }
}

#[cfg(test)]
mod testler {
    use super::*;
    use crate::kayit::deneme::ToplananKayit;
    use std::io::Cursor;

    /// Girdiyi satırlarına ayırır: (satır, konum)
    fn satirlar(girdi: &[u8]) -> (Vec<(String, u64)>, bool) {
        let mut okuyucu = SatirOkuyucu::new(girdi);
        let mut satirlar = Vec::new();
        while let Some((satir, konum)) = okuyucu.sonraki().unwrap() {
            satirlar.push((satir.into_owned(), konum));
        }
        (satirlar, okuyucu.gecersiz_utf8)
    }

    #[test]
    fn satir_sonlari_lines_gibi() {
        for metin in [
            "",
            "a",
            "a\n",
            "a\r\n",
            "a\r",
            "a\r\r\n",
            "\n",
            "\r\n",
            "a\nb",
            "a\n\nb\n",
            "a\rb\n",
            "a\n\r",
            "ş\r\nğ\nü",
        ] {
            let (okunan, gecersiz) = satirlar(metin.as_bytes());
            let beklenen: Vec<&str> = metin.lines().collect();
            let okunan_metinler: Vec<&str> =
                okunan.iter().map(|(satir, _)| satir.as_str()).collect();
            assert_eq!(okunan_metinler, beklenen, "{metin:?}");
            assert!(!gecersiz);
        }
        // Konumlar satırların ilk baytıdır
        let (okunan, _) = satirlar("ab\r\nç\n\nd".as_bytes());
        let konumlar: Vec<u64> = okunan.iter().map(|&(_, konum)| konum).collect();
        assert_eq!(konumlar, [0, 4, 7, 8]);
    }

    #[test]
    fn gecersiz_utf8_satir_satir_degistirilir() {
        // Bozuk baytlar: tek başına devam baytı, yarım kalmış üç baytlık karakter (satır
        // sonundan önce), geçersiz başlangıç baytı
        let girdi: &[u8] = b"a\x80b\n\xe2\x82\nok\n\xff";
        let (okunan, gecersiz) = satirlar(girdi);
        assert!(gecersiz);
        let tamami = String::from_utf8_lossy(girdi);
        let beklenen: Vec<&str> = tamami.lines().collect();
        let okunan_metinler: Vec<&str> = okunan.iter().map(|(satir, _)| satir.as_str()).collect();
        assert_eq!(okunan_metinler, beklenen);
    }

    /// Girdiyi satırların en fazla `sinir` baytını okuyarak satırlarına ayırır.
    fn sinirli_satirlar(girdi: &[u8], sinir: usize) -> (Vec<(String, u64)>, bool) {
        let mut okuyucu = SatirOkuyucu::sinirli(girdi, sinir);
        let mut satirlar = Vec::new();
        while let Some((satir, konum)) = okuyucu.sonraki().unwrap() {
            satirlar.push((satir.into_owned(), konum));
        }
        (satirlar, okuyucu.gecersiz_utf8)
    }

    #[test]
    fn uzun_satir_kesilir_konumlar_bozulmaz() {
        const SINIR: usize = 64;
        // Sınırı aşan satır: ilk SINIR baytı kalır, kalanı atlanır; sonraki satırın konumu
        // yine doğrudur. Kesilen yerde yarım kalan "ş" (2 bayt) atılır.
        let mut girdi = vec![b'x'; SINIR - 1];
        girdi.extend_from_slice("şyyy\nsonraki\n".as_bytes());
        let (okunan, gecersiz) = sinirli_satirlar(&girdi, SINIR);
        assert!(!gecersiz);
        assert_eq!(okunan.len(), 2);
        assert_eq!(okunan[0].0.len(), SINIR - 1);
        assert_eq!(okunan[1], ("sonraki".to_string(), (SINIR + 5) as u64));

        // Sınır tam satır sonuna denk gelirse satır kesilmez
        let mut girdi = vec![b'x'; SINIR - 1];
        girdi.extend_from_slice(b"\nsonraki");
        let (okunan, _) = sinirli_satirlar(&girdi, SINIR);
        assert_eq!(okunan[0].0.len(), SINIR - 1);
        assert_eq!(okunan[1], ("sonraki".to_string(), SINIR as u64));

        // Satır sınıra tam sığıyor, satır sonu hemen arkasında: kesilmiş sayılmaz. CRLF'deki
        // \r de `str::lines`'taki gibi atılır.
        for son in ["\n", "\r\n"] {
            let mut girdi = vec![b'x'; SINIR + 1 - son.len()];
            girdi.extend_from_slice(son.as_bytes());
            girdi.extend_from_slice(b"sonraki");
            let (okunan, _) = sinirli_satirlar(&girdi, SINIR);
            let beklenen: Vec<&str> = std::str::from_utf8(&girdi).unwrap().lines().collect();
            let metinler: Vec<&str> = okunan.iter().map(|(satir, _)| satir.as_str()).collect();
            assert_eq!(metinler, beklenen, "{son:?}");
            assert_eq!(okunan[1].1, girdi.len() as u64 - 7);
        }

        // Girdi tam sınırda bitiyor: satır kesilmemiştir, sondaki yarım karakter de geçersiz
        // UTF-8 olarak kalır (dosyanın tamamı okunsaydı da öyle olurdu)
        let mut girdi = vec![b'x'; SINIR - 1];
        girdi.push("ş".as_bytes()[0]);
        let (okunan, gecersiz) = sinirli_satirlar(&girdi, SINIR);
        assert!(gecersiz);
        assert_eq!(okunan.len(), 1);
        assert!(okunan[0].0.ends_with('\u{fffd}'));
    }

    #[test]
    fn oku_kayitlari_ve_sayilari_verir() {
        let metin = "#1 yetim\n[2026-10-24 10:00:00] local.ERROR: Bir\nayrıntı\n\
                     [2026-10-24 10:00:01] local.INFO: İki";
        let mut kayitlar: Vec<ToplananKayit> = Vec::new();
        let sonuc = oku(metin.as_bytes(), &mut kayitlar).unwrap();
        assert_eq!(
            sonuc.sayilar,
            SatirSayilari {
                satir: 4,
                kayit: 2,
                devam: 1,
                yetim: 1
            }
        );
        assert!(!sonuc.gecersiz_utf8);
        assert_eq!(kayitlar[0].devam, ["ayrıntı"]);
        assert_eq!(
            kayitlar[1].bilgi.konum,
            metin.find("[2026-10-24 10:00:01]").unwrap() as u64
        );
    }

    #[test]
    fn kopyalayan_her_bayti_kopyalar() {
        // Kopya girdinin aynısıdır: konumlar kopyada da geçerli
        let mut girdi = b"[2026-10-24 10:00:00] local.INFO: a\r\n".to_vec();
        girdi.extend(vec![b'z'; 300_000]);
        girdi.extend_from_slice(b"\n[2026-10-24 10:00:01] local.INFO: b");
        let mut kopyalayan = Kopyalayan::new(io::BufReader::new(&girdi[..]), Vec::new());
        let mut kayitlar: Vec<ToplananKayit> = Vec::new();
        oku(&mut kopyalayan, &mut kayitlar).unwrap();
        assert_eq!(kayitlar.len(), 2);
        assert_eq!(kopyalayan.bitir().unwrap(), girdi);

        // Sınırı aşan satırın atlanan kalanı da kopyalanır
        let mut kopyalayan = Kopyalayan::new(io::BufReader::new(&girdi[..]), Vec::new());
        let mut okuyucu = SatirOkuyucu::sinirli(&mut kopyalayan, 1_000);
        let mut satir_sayisi = 0;
        while okuyucu.sonraki().unwrap().is_some() {
            satir_sayisi += 1;
        }
        assert_eq!(satir_sayisi, 3);
        assert_eq!(kopyalayan.bitir().unwrap(), girdi);
    }

    /// `fill_buf` çağrılarını sayan bir okuyucu.
    struct SayanOkuyucu<'a> {
        ic: &'a [u8],
        doldurma: usize,
    }

    impl Read for SayanOkuyucu<'_> {
        fn read(&mut self, tampon: &mut [u8]) -> io::Result<usize> {
            self.ic.read(tampon)
        }
    }

    impl BufRead for SayanOkuyucu<'_> {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            self.doldurma += 1;
            Ok(self.ic)
        }

        fn consume(&mut self, miktar: usize) {
            self.ic = &self.ic[miktar..];
        }
    }

    #[test]
    fn kopyalayan_bos_tuketimde_girdiye_bakmaz() {
        // Girdi bittikten sonra `consume(0)` girdiden yeniden okumamalı: terminalden okunurken
        // bu, Ctrl+D'ye ikinci kez basmayı gerektirirdi
        let mut kopyalayan = Kopyalayan::new(
            SayanOkuyucu {
                ic: b"",
                doldurma: 0,
            },
            Vec::new(),
        );
        kopyalayan.consume(0);
        assert_eq!(kopyalayan.ic.doldurma, 0);
        let mut kayitlar: Vec<ToplananKayit> = Vec::new();
        oku(&mut kopyalayan, &mut kayitlar).unwrap();
        // `read_until` girdinin sonunu bir kez görür
        assert_eq!(kopyalayan.ic.doldurma, 1);
    }

    /// `adet` kayıtlık bir log; arada devam satırları, JSON kayıtları ve BOM'lu satırlar.
    fn uzun_log(adet: usize) -> String {
        let mut satirlar = vec!["öncesinden kalma bir satır".to_string()];
        for sira in 0..adet {
            match sira % 4 {
                0 => satirlar.push(format!(
                    "[2026-10-24 10:00:00] local.ERROR: Hata {sira} {{\"exception\":\"x\"}}\n[stacktrace]\n#0 {{main}}"
                )),
                1 => satirlar.push(format!(
                    r#"{{"message":"JSON \"{sira}\"\nikinci satır","level_name":"INFO","datetime":"2026-10-24T10:00:00+00:00"}}"#
                )),
                2 => satirlar.push(format!("\u{feff}[2026-10-24 10:00:00] local.INFO: BOM {sira}")),
                _ => satirlar.push(format!("[2026-10-24 10:00:00] local.DEBUG: Düz {sira}\r")),
            }
        }
        satirlar.join("\n")
    }

    /// Bir logun kayıtlarını okur; seyrek dizini de kurar.
    fn dizinle_oku(metin: &str) -> (Vec<ToplananKayit>, Vec<u64>) {
        let mut kayitlar: Vec<ToplananKayit> = Vec::new();
        oku(metin.as_bytes(), &mut kayitlar).unwrap();
        let dizin = kayitlar
            .iter()
            .filter(|kayit| kayit.bilgi.sira.is_multiple_of(DIZIN_ARALIGI))
            .map(|kayit| kayit.bilgi.konum)
            .collect();
        (kayitlar, dizin)
    }

    /// Kayıtların maskeli mesajları; istekler bunları ödünç alır.
    fn maskeliler(kayitlar: &[ToplananKayit]) -> Vec<String> {
        kayitlar
            .iter()
            .map(|kayit| maskele(&kayit.mesaj).into_owned())
            .collect()
    }

    /// Sıra numaraları verilen kayıtların istekleri.
    fn istekler<'a>(
        siralar: &[usize],
        kayitlar: &[ToplananKayit],
        maskeliler: &'a [String],
    ) -> Vec<OrnekIstegi<'a>> {
        siralar
            .iter()
            .map(|&sira| OrnekIstegi {
                sira,
                maskeli: &maskeliler[sira.min(maskeliler.len() - 1)],
                seviye: kayitlar[sira.min(kayitlar.len() - 1)].seviye,
            })
            .collect()
    }

    #[test]
    fn kayitlar_seyrek_dizinle_yeniden_bulunur() {
        let metin = uzun_log(3 * DIZIN_ARALIGI + 7);
        let (kayitlar, dizin) = dizinle_oku(&metin);
        assert_eq!(dizin.len(), 4);
        let maskeliler = maskeliler(&kayitlar);
        let mut kaynak = YenidenOkuyucu::new(Cursor::new(metin.as_bytes()), dizin, None);
        // Blok sınırları, ilk ve son kayıt, aynı bloktan birkaç kayıt, tekrar ve sırasızlık
        let aranan = [
            0,
            1,
            DIZIN_ARALIGI - 1,
            DIZIN_ARALIGI,
            DIZIN_ARALIGI + 1,
            2 * DIZIN_ARALIGI + 500,
            2 * DIZIN_ARALIGI + 3,
            3 * DIZIN_ARALIGI + 6,
            DIZIN_ARALIGI,
        ];
        let bulunan = kaynak
            .mesajlar(&istekler(&aranan, &kayitlar, &maskeliler))
            .unwrap();
        assert_eq!(bulunan.len(), 8);
        for sira in aranan {
            assert_eq!(bulunan[&sira], kayitlar[sira].mesaj, "{sira}");
        }
        // Bütün kayıtlar tek tek de bulunur
        let hepsi: Vec<usize> = (0..kayitlar.len()).collect();
        let bulunan = kaynak
            .mesajlar(&istekler(&hepsi, &kayitlar, &maskeliler))
            .unwrap();
        assert!(
            hepsi
                .iter()
                .all(|sira| bulunan[sira] == kayitlar[*sira].mesaj)
        );
        assert!(!kaynak.degisti());
    }

    #[test]
    fn en_yeni_zaman_kayit_baslangiclarindan_bulunur() {
        assert_eq!(en_yeni_zaman("".as_bytes()).unwrap(), None);
        // Sırasız kayıtlar, JSON, BOM; bilinmeyen seviyeli satır ve devam satırları kayıt değil
        let metin = "#0 yetim\n\
                     [2026-10-24 10:00:05] local.INFO: a\n\
                     {\"message\":\"b\",\"level_name\":\"INFO\",\"datetime\":\"2026-10-24T11:00:00+03:00\"}\n\
                     \u{feff}[2026-10-24 10:30:00] local.INFO: c\n\
                     [2026-10-24 12:00:00] local.HATA: kayıt değil\n\
                     [2026-10-24 09:00:00] local.INFO: d";
        assert_eq!(
            en_yeni_zaman(metin.as_bytes()).unwrap(),
            Some(Zaman::kayittan("2026-10-24 11:00:00"))
        );
    }

    #[test]
    fn filtreli_depodaki_kayitlar_yeniden_bulunur() {
        use crate::depo::{Depo, DepoAyarlari};
        use crate::filtre::Filtre;
        // Kayıtların yarısı WARNING: depodaki sıra numaraları dosyadakilerin yarısı
        let satirlar: Vec<String> = (0..3 * DIZIN_ARALIGI)
            .map(|sira| {
                let seviye = if sira % 2 == 0 { "INFO" } else { "WARNING" };
                format!("[2026-10-24 10:00:00] local.{seviye}: Kayıt {sira}")
            })
            .collect();
        let metin = satirlar.join("\n");
        let filtre = Filtre {
            seviye: Some(Seviye::Warning),
            ..Filtre::default()
        }
        .coz(None)
        .unwrap();
        let ayarlar = DepoAyarlari {
            filtre: Some(filtre),
            ..DepoAyarlari::default()
        };
        let depo = Depo::doldur(ayarlar, metin.as_bytes()).unwrap();
        assert_eq!(depo.kayit_sayisi(), 3 * DIZIN_ARALIGI / 2);
        let mut kaynak = YenidenOkuyucu::new(
            Cursor::new(metin.as_bytes()),
            depo.dizin().to_vec(),
            Some(filtre),
        );
        let aranan = [0, 1, DIZIN_ARALIGI - 1, DIZIN_ARALIGI, DIZIN_ARALIGI + 100];
        let istekler: Vec<OrnekIstegi<'_>> = aranan
            .iter()
            .map(|&sira| OrnekIstegi {
                sira,
                maskeli: "Kayıt <SAYI>",
                seviye: Seviye::Warning,
            })
            .collect();
        let bulunan = kaynak.mesajlar(&istekler).unwrap();
        for sira in aranan {
            assert_eq!(bulunan[&sira], format!("Kayıt {}", 2 * sira + 1), "{sira}");
        }
        assert!(!kaynak.degisti());
    }

    #[test]
    fn olmayan_kayit_sonucta_yer_almaz() {
        let metin = uzun_log(10);
        let (kayitlar, dizin) = dizinle_oku(&metin);
        let maskeliler = maskeliler(&kayitlar);
        let mut kaynak = YenidenOkuyucu::new(Cursor::new(metin.as_bytes()), dizin.clone(), None);
        let bulunan = kaynak
            .mesajlar(&istekler(
                &[9, 10, DIZIN_ARALIGI + 3],
                &kayitlar,
                &maskeliler,
            ))
            .unwrap();
        assert_eq!(bulunan.keys().copied().collect::<Vec<_>>(), [9]);
        // Girdi sonradan kısaldıysa (döndürülen log) bulunamayan kayıt yine yoktur
        let kisa = &metin.as_bytes()[..metin.len() / 2];
        let mut kaynak = YenidenOkuyucu::new(Cursor::new(kisa), dizin, None);
        let bulunan = kaynak
            .mesajlar(&istekler(&[0, 9], &kayitlar, &maskeliler))
            .unwrap();
        assert!(bulunan.contains_key(&0));
        assert!(!bulunan.contains_key(&9));
        assert!(kaynak.degisti());
    }

    #[test]
    fn degisen_girdideki_baska_kayit_ornek_olmaz() {
        // Log okunduktan sonra kesilip yeniden yazıldı (`copytruncate`): aynı sırada başka bir
        // kalıbın ya da seviyenin kaydı var. Örnek olarak verilmez; değiştiği bildirilir.
        let metin = "[2026-10-24 10:00:00] local.ERROR: Kuyruk 1 dolu\n\
                     [2026-10-24 10:00:01] local.ERROR: Kuyruk 2 dolu\n\
                     [2026-10-24 10:00:02] local.WARNING: Kuyruk 3 dolu";
        let (kayitlar, dizin) = dizinle_oku(metin);
        let maskeliler = maskeliler(&kayitlar);
        let istekler = istekler(&[1, 2], &kayitlar, &maskeliler);
        let yeni = "[2026-10-25 00:00:00] local.ERROR: Döndürüldü\n\
                    [2026-10-25 00:00:01] local.ERROR: Kuyruk 9 dolu\n\
                    [2026-10-25 00:00:02] local.ERROR: Kuyruk 7 dolu";
        let mut kaynak = YenidenOkuyucu::new(Cursor::new(yeni.as_bytes()), dizin.clone(), None);
        let bulunan = kaynak.mesajlar(&istekler).unwrap();
        // 1. sıradaki kayıt aynı kalıp ve seviyede: o da bu kalıbın bir örneğidir. 2.'nin seviyesi
        // farklı.
        assert_eq!(bulunan.len(), 1);
        assert_eq!(bulunan[&1], "Kuyruk 9 dolu");
        assert!(kaynak.degisti());
        // Değişmemiş girdide değişiklik bildirilmez
        let mut kaynak = YenidenOkuyucu::new(Cursor::new(metin.as_bytes()), dizin, None);
        assert_eq!(kaynak.mesajlar(&istekler).unwrap().len(), 2);
        assert!(!kaynak.degisti());
    }
}
