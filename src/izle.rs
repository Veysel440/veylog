//! Canlı izleme (`--izle`): log sonuna kadar okunur ve rapor yazılır; sonra eklenen kayıtlar
//! okunmaya devam eder, yeni kayıt geldikçe rapor yeniden çizilir.
//!
//! **Artımlı depo.** Girdi yeniden okunmaz: her çizimde yalnızca o çizimden beri eklenen baytlar
//! işlenir. Depo okuma durumunu çizimler arasında korur (`Depo::ara_durum`); Drain değişken
//! bir konum bulduysa kayıtlar o çizimde yeniden kümelenir. Rapor (`Ozet`) her çizimde depodan
//! yeniden üretilir; maliyeti alt grupların sayısıyla orantılıdır, dosyanın boyuyla değil.
//! Zaman penceresi ve ani artışlar kayıt sütunlarını da dolaşır: onlarla kayıt sayısıyla.
//!
//! **Kayıt tamamlanma kuralı.** Bir kaydın devam satırları (yığın izi) başlığından sonra gelir
//! ve yazan süreç onları birkaç yazma işlemiyle yazabilir. Okunan son kayıt bu yüzden hemen
//! depoya alınmaz: ondan sonra bir kayıt başlığı gelince ya da girdi bir aralık boyunca sessiz
//! kalınca (hiç bayt gelmeyince) tamamlanmış sayılır. `\n` ile bitmemiş son satır da bekletilir
//! (`SatirTamponu`): bir yazma işlemi satırın ortasında bitebilir. Sessizlikte o da son satır
//! sayılır. Böylece girdi bir aralık sessiz kaldıktan sonraki rapor, o ana kadar gelen baytların
//! bir kerede özetlenmesiyle aynıdır. İzleme başlarken dosya bir aralıktan uzun süredir
//! değişmemişse sessizlik o zamandan sayılır: durgun bir logun son kaydı ilk çizimde görünür.
//!
//! **Döndürme.** Her aralıkta önce eldeki dosyanın sonuna kadar okunur, sonra yolundaki dosyayla
//! karşılaştırılır: yolda artık başka bir dosya varsa (Unix'te aygıt ve düğüm numarası,
//! Windows'ta oluşturulma zamanı değişmişse) ya da dosya kısalmışsa (copytruncate) eskisi
//! bitmiş sayılır, yenisi baştan okunur. Eski dosyaya döndürülmeden hemen önce yazılanlar
//! böylece kaybolmaz. Depo korunur: eski dosyanın kayıtları raporda kalır, yalnızca örnekleri
//! artık yeniden okunamaz; yeni dosyanın kayıtları dizine yeni dosyadaki konumlarıyla yazılır
//! (`Depo::yeni_girdi`). Yol geçici olarak yoksa (döndürmenin ortasında) eldeki dosya okunmaya
//! devam eder, sonraki aralıkta yeniden bakılır.
//!
//! **Kayan pencere.** Zaman filtreleri (`--son 1sa`, `--baslangic 15:00`) okunurken değil her
//! çizimde, o ana kadar okunan en yeni kayda göre uygulanır (`Depo::pencere`): rapor hep son bir
//! saati gösterir. En yeni kayıt seviye filtresine uymayanlarla birlikte bulunur, toplu
//! okumadaki gibi. Seviye filtresi okunurken uygulanır; filtreye uymayan kayıt depoya girmez.
//! Log ilerledikçe aralık boşalabilir: o zaman pencerede kayıt kalmaz, izleme sürer.
//!
//! **Standart girdi.** Ayrı bir iş parçacığı okur; ana döngü parçaları bir kanaldan alır ve
//! bir aralık boyunca parça gelmezse sessizlik sayar. Girdi bitince son rapor yazılır ve
//! çıkılır (`tail -f laravel.log | veylog --izle -`). Dosya Ctrl+C'ye kadar izlenir.
//!
//! **Çizim.** Çıktı bir terminalse her çizimde ekran temizlenir, durum satırı ve uyarılar en
//! alttadır; değilse (bir dosyaya ya da boruya yazılıyorsa) çizimler bir ayırıcı satırla art arda
//! yazılır, durum satırı başlarındadır. Geri kalanı olağan metin raporudur.

use std::collections::HashMap;
#[cfg(any(unix, test))]
use std::fs;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, IsTerminal, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use crate::depo::{Depo, DepoAyarlari};
use crate::dil::{self, Dil};
use crate::filtre::Filtre;
use crate::kayit::{KayitToplayici, SatirSayilari};
use crate::komut_satiri::{Izleme, Secenekler};
use crate::okuyucu::{OkumaSonucu, OrnekIstegi, OrnekKaynagi, SATIR_SINIRI, YenidenOkuyucu};
use crate::rapor::{self, Ozet, OzetHatasi};
use crate::zaman::sure_tam_yaz;
use crate::{GeciciDosya, Hata, OkunamazKaynak, TAMPON, dosyayi_ac};

/// `--aralik`ın en küçük değeri: daha sık bakmak işlemciyi boşuna yorar.
pub const EN_KISA_ARALIK: Duration = Duration::from_millis(100);

/// `--aralik`ın en büyük değeri: bir saatten seyrek bakan bir izleme izleme sayılmaz.
pub const EN_UZUN_ARALIK: Duration = Duration::from_secs(3_600);

/// `--aralik` verilmezse.
pub const VARSAYILAN_ARALIK: Duration = Duration::from_secs(2);

/// Çıktı terminal değilse art arda yazılan çizimlerin arasına konan satır.
pub const AYIRICI: &str = "────────────────────────────────────────";

/// Aralığı okunur yazar: `2 sn`, `500 ms`, `1 sn 500 ms`, `1 dk 30 sn`.
pub fn aralik_yaz(aralik: Duration) -> String {
    let saniye = aralik.as_secs();
    let milisaniye = aralik.subsec_millis();
    match (saniye, milisaniye) {
        (0, milisaniye) => format!("{milisaniye} ms"),
        (saniye, 0) => sure_tam_yaz(saniye),
        (saniye, milisaniye) => format!("{} {milisaniye} ms", sure_tam_yaz(saniye)),
    }
}

/// Parçalar halinde gelen baytları satırlara ayırır. `\n` ile bitmemiş son satır bekletilir;
/// kalanı gelince verilir. Satır sonları `str::lines` gibidir: `\n` ya da `\r\n`. Bir satırın
/// en fazla `sinir` baytı tutulur (`SATIR_SINIRI`); ötesi satır sonuna kadar atılır.
pub struct SatirTamponu {
    /// Bekleyen (henüz bitmemiş) satırın baytları; sınırı aşanlar yok.
    bekleyen: Vec<u8>,
    /// Bekleyen satır için şimdiye kadar gelen bayt sayısı, atılanlarla birlikte.
    bekleyen_bayt: u64,
    /// Bekleyen satırın başının girdideki konumu.
    konum: u64,
    sinir: usize,
    /// Bekleyen satır sınırı aştı: satır sonuna kadar gelenler atılıyor.
    kesildi: bool,
    gecersiz_utf8: bool,
}

impl Default for SatirTamponu {
    fn default() -> Self {
        SatirTamponu::sinirli(SATIR_SINIRI)
    }
}

impl SatirTamponu {
    /// Satırların en fazla `sinir` baytını tutan bir tampon.
    pub fn sinirli(sinir: usize) -> Self {
        SatirTamponu {
            bekleyen: Vec::new(),
            bekleyen_bayt: 0,
            konum: 0,
            sinir,
            kesildi: false,
            gecersiz_utf8: false,
        }
    }

    /// Bir parçayı ekler; tamamlanan her satırı (satır sonu olmadan) başının girdideki
    /// konumuyla `satir`a verir.
    pub fn ekle(&mut self, mut parca: &[u8], mut satir: impl FnMut(&str, u64)) {
        while let Some(sonu) = parca.iter().position(|&bayt| bayt == b'\n') {
            let (govde, kalan) = (&parca[..sonu], &parca[sonu + 1..]);
            let bas = self.konum;
            // Satırın önceki parçalardan gelen baytları da (atılanlar dahil) geçildi
            self.konum += self.bekleyen_bayt + sonu as u64 + 1;
            if self.bekleyen.is_empty() && !self.kesildi {
                // Satırın tamamı bu parçada: kopyalanmadan verilir
                if govde.len() > self.sinir {
                    self.ver(&govde[..self.sinir], true, false, bas, &mut satir);
                } else {
                    self.ver(govde, false, true, bas, &mut satir);
                }
            } else {
                self.biriktir(govde);
                let bekleyen = std::mem::take(&mut self.bekleyen);
                self.ver(&bekleyen, self.kesildi, !self.kesildi, bas, &mut satir);
                // Tampon satırdan satıra yeniden kullanılır
                self.bekleyen = bekleyen;
                self.bekleyen.clear();
            }
            self.bekleyen_bayt = 0;
            self.kesildi = false;
            parca = kalan;
        }
        self.biriktir(parca);
    }

    /// Parçanın satır sonu içermeyen kalanını bekletir; sınırı aşan kısım atılır.
    fn biriktir(&mut self, baytlar: &[u8]) {
        self.bekleyen_bayt += baytlar.len() as u64;
        if self.kesildi || baytlar.is_empty() {
            return;
        }
        let yer = self.sinir - self.bekleyen.len();
        if baytlar.len() <= yer {
            self.bekleyen.extend_from_slice(baytlar);
        } else {
            self.bekleyen.extend_from_slice(&baytlar[..yer]);
            self.kesildi = true;
        }
    }

    /// Girdi bitti ya da bir süre sessiz kaldı: `\n` ile bitmemiş bekleyen satır son satır
    /// sayılır ve verilir (`SatirOkuyucu` girdinin son satırını da satır sonu olmadan verir).
    pub fn bitir(&mut self, mut satir: impl FnMut(&str, u64)) {
        if self.bekleyen_bayt == 0 {
            return;
        }
        let bas = self.konum;
        self.konum += self.bekleyen_bayt;
        let bekleyen = std::mem::take(&mut self.bekleyen);
        self.ver(&bekleyen, self.kesildi, false, bas, &mut satir);
        self.bekleyen = bekleyen;
        self.bekleyen.clear();
        self.bekleyen_bayt = 0;
        self.kesildi = false;
    }

    /// Tamamlanan satırı verir. `kesildi`: satır sınırı aştı; kesilen yerde yarım kalan bir
    /// UTF-8 karakteri atılır, geçersiz bayt sayılmaz. `satir_sonu`: satır `\n` ile bitti;
    /// öyleyse sondaki `\r` de atılır (`\r\n`).
    fn ver(
        &mut self,
        mut baytlar: &[u8],
        kesildi: bool,
        satir_sonu: bool,
        bas: u64,
        satir: &mut impl FnMut(&str, u64),
    ) {
        if kesildi {
            if let Err(hata) = std::str::from_utf8(baytlar)
                && hata.error_len().is_none()
            {
                baytlar = &baytlar[..hata.valid_up_to()];
            }
        } else if satir_sonu && baytlar.last() == Some(&b'\r') {
            baytlar = &baytlar[..baytlar.len() - 1];
        }
        let metin = crate::okuyucu::utf8_metin(baytlar, &mut self.gecersiz_utf8);
        satir(&metin, bas);
    }

    /// Satır sonu gelmemiş baytlar bekliyor mu?
    pub fn bekleyen_var(&self) -> bool {
        self.bekleyen_bayt > 0
    }

    /// Bir sonraki satırın (ya da bekleyen satırın) başının girdideki konumu.
    #[cfg(test)]
    fn konum(&self) -> u64 {
        self.konum
    }

    /// Şimdiye kadar geçersiz UTF-8 baytı görüldü mü?
    pub fn gecersiz_utf8(&self) -> bool {
        self.gecersiz_utf8
    }

    /// Yeni bir girdiye (döndürülen dosyaya) geçer: konumlar sıfırdan sayılır. Bekleyen satır
    /// önce `bitir` ile verilmiş olmalı.
    pub fn sifirla(&mut self) {
        debug_assert!(!self.bekleyen_var(), "bekleyen satır önce verilmeli");
        self.konum = 0;
    }
}

/// Bir dosyanın kimliği (Unix): aygıt ve düğüm (inode) numarası. Yolda hâlâ aynı dosya var
/// mı diye buna bakılır.
#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Kimlik {
    aygit: u64,
    dugum: u64,
}

#[cfg(unix)]
impl Kimlik {
    fn new(bilgi: &fs::Metadata) -> Kimlik {
        use std::os::unix::fs::MetadataExt;
        Kimlik {
            aygit: bilgi.dev(),
            dugum: bilgi.ino(),
        }
    }
}

/// İzlenen dosya: açık tutulur, her aralıkta kaldığı yerden sonuna kadar okunur.
pub struct DosyaKaynagi {
    yol: PathBuf,
    dosya: File,
    #[cfg(unix)]
    kimlik: Option<Kimlik>,
    /// Okunan bayt sayısı: dosyadaki konum.
    konum: u64,
    parca: Vec<u8>,
}

/// Döndürme denetiminin sonucu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dondurme {
    Yok,
    /// Yolda başka bir dosya var ya da dosya kısaldı: baştan okunacak.
    Donduruldu,
}

impl DosyaKaynagi {
    #[cfg(test)]
    pub fn ac(yol: &Path) -> io::Result<DosyaKaynagi> {
        Ok(DosyaKaynagi::new(yol, dosyayi_ac(yol)?))
    }

    /// `yol`daki, zaten açılmış dosya. Dosya bir kez açılır: ikinci bir tutamaç döndürülen
    /// dosyanın diskteki yerini program kapanana kadar tutardı.
    pub fn new(yol: &Path, dosya: File) -> DosyaKaynagi {
        DosyaKaynagi {
            yol: yol.to_path_buf(),
            #[cfg(unix)]
            kimlik: dosya.metadata().ok().map(|bilgi| Kimlik::new(&bilgi)),
            dosya,
            konum: 0,
            parca: vec![0; TAMPON],
        }
    }

    /// Yoldaki dosya değişmiş (döndürülmüş) ya da kısalmış mı? Öyleyse baştan okumaya hazırlar.
    ///
    /// Unix'te yolun düğüm numarasına bakılır: değiştiyse yeni dosya açılır. Kısalma eldeki
    /// dosyanın boyundan anlaşılır (copytruncate aynı düğümü kısaltır).
    #[cfg(unix)]
    pub fn dondurme_denetle(&mut self) -> Dondurme {
        // Yol geçici olarak yoksa (döndürmenin ortası) eldeki dosya okunmaya devam eder
        let Ok(bilgi) = fs::metadata(&self.yol) else {
            return Dondurme::Yok;
        };
        let kimlik = Kimlik::new(&bilgi);
        if self.kimlik.is_some_and(|eski| eski != kimlik) {
            // Yeni dosya açılamazsa (yarım kalmış bir döndürme) eskisi okunmaya devam eder
            let Ok(dosya) = File::open(&self.yol) else {
                return Dondurme::Yok;
            };
            self.dosya = dosya;
            self.kimlik = Some(kimlik);
            self.konum = 0;
            return Dondurme::Donduruldu;
        }
        self.kisalmis_mi()
    }

    /// Windows'ta düğüm numarası yoktur; oluşturulma zamanı da güvenilir değildir (yeniden
    /// adlandırılan bir dosyanın yerine hemen açılan yeni dosya eskisinin zamanını alabilir,
    /// Wine'da aynı dosyanınki değişebilir). Yoldaki dosya her seferinde yeniden açılır ve
    /// eldekiyle karşılaştırılır: oluşturulma zamanı başkaysa ya da yoldaki dosya eldekinden
    /// kısaysa yolda başka bir dosya vardır. İki tutamacın boyları da güvenilirdir; yolun
    /// boyu ise dosyayı yazan süreç tutamacını kapatana kadar güncellenmeyebilir.
    #[cfg(not(unix))]
    pub fn dondurme_denetle(&mut self) -> Dondurme {
        let Ok(yeni) = File::open(&self.yol) else {
            return Dondurme::Yok;
        };
        let (Ok(yeni_bilgi), Ok(eski_bilgi)) = (yeni.metadata(), self.dosya.metadata()) else {
            return Dondurme::Yok;
        };
        let baska = yeni_bilgi.created().ok() != eski_bilgi.created().ok()
            || yeni_bilgi.len() < eski_bilgi.len();
        if baska {
            self.dosya = yeni;
            self.konum = 0;
            return Dondurme::Donduruldu;
        }
        self.kisalmis_mi()
    }

    /// Eldeki dosya okunandan kısaysa (copytruncate) başa sarılır.
    fn kisalmis_mi(&mut self) -> Dondurme {
        let Ok(bilgi) = self.dosya.metadata() else {
            return Dondurme::Yok;
        };
        if bilgi.len() < self.konum && self.dosya.seek(SeekFrom::Start(0)).is_ok() {
            self.konum = 0;
            return Dondurme::Donduruldu;
        }
        Dondurme::Yok
    }

    /// Dosyayı kaldığı yerden sonuna kadar okur; her parçayı `isle`ye verir. Hiç bayt
    /// gelmediyse false.
    pub fn oku(&mut self, mut isle: impl FnMut(&[u8])) -> io::Result<bool> {
        let mut geldi = false;
        loop {
            match self.dosya.read(&mut self.parca) {
                Ok(0) => return Ok(geldi),
                Ok(okunan) => {
                    self.konum += okunan as u64;
                    geldi = true;
                    isle(&self.parca[..okunan]);
                }
                Err(hata) if hata.kind() == io::ErrorKind::Interrupted => {}
                Err(hata) => return Err(hata),
            }
        }
    }

    /// Eldeki dosyanın son değiştirilmesinden bu yana geçen süre. Bilinmiyorsa (ya da
    /// değiştirilme zamanı gelecekteyse: saatler uyuşmuyor) None.
    pub fn degismeden_gecen(&self) -> Option<Duration> {
        let degisti = self
            .dosya
            .metadata()
            .and_then(|bilgi| bilgi.modified())
            .ok()?;
        SystemTime::now().duration_since(degisti).ok()
    }

    /// Dosyadaki konum: okunan bayt sayısı.
    #[cfg(test)]
    fn konum(&self) -> u64 {
        self.konum
    }
}

/// Bir akışı (standart girdi, boru) ayrı bir iş parçacığında okur; parçaları kanaldan verir.
/// Akış bitince kanal kapanır. Kanal sınırlıdır: ana döngü yavaş kalırsa okuma bekler, bellek
/// parçalarla dolmaz.
pub fn akis_baslat(mut okuyucu: impl Read + Send + 'static) -> Receiver<io::Result<Vec<u8>>> {
    let (gonderen, alan) = mpsc::sync_channel(8);
    thread::spawn(move || {
        let mut parca = vec![0u8; TAMPON];
        loop {
            match okuyucu.read(&mut parca) {
                Ok(0) => break,
                Ok(okunan) => {
                    if gonderen.send(Ok(parca[..okunan].to_vec())).is_err() {
                        break;
                    }
                }
                Err(hata) if hata.kind() == io::ErrorKind::Interrupted => {}
                Err(hata) => {
                    let _ = gonderen.send(Err(hata));
                    break;
                }
            }
        }
    });
    alan
}

/// Deponun penceresi için örnek kaynağı: pencerenin sıra numaralarını deponun sıra numaralarına
/// çevirir (`Depo::pencere`), bulunanları geri çevirir.
struct CevrilmisKaynak<'a> {
    ic: &'a mut dyn OrnekKaynagi,
    /// Pencerenin k. kaydının depodaki sıra numarası.
    tam_sira: &'a [usize],
}

impl OrnekKaynagi for CevrilmisKaynak<'_> {
    fn mesajlar(&mut self, istekler: &[OrnekIstegi<'_>]) -> io::Result<HashMap<usize, String>> {
        let cevrilen: Vec<OrnekIstegi<'_>> = istekler
            .iter()
            .map(|istek| OrnekIstegi {
                sira: self.tam_sira[istek.sira],
                ..*istek
            })
            .collect();
        let bulunanlar = self.ic.mesajlar(&cevrilen)?;
        Ok(istekler
            .iter()
            .filter_map(|istek| {
                let mesaj = bulunanlar.get(&self.tam_sira[istek.sira])?;
                Some((istek.sira, mesaj.clone()))
            })
            .collect())
    }

    fn degisti(&self) -> bool {
        self.ic.degisti()
    }
}

/// Örneklerin çizimler arasında saklandığı kaynak. Kayan bir pencerede (`--son 1sa`) ilk kaydı
/// pencerenin dışında kalan her alt grubun örneği, pencerenin içindeki ilk kaydının mesajıdır
/// ve girdiden yeniden okunur. Pencere her çizimde biraz kaydığı için istenenlerin çoğu bir
/// önceki çizimde de istenmiştir: onlar yeniden okunmaz. Bir çizimde istenmeyenler unutulur;
/// saklananlar en fazla bir çizimin istediği kadardır. Sıra numarası bir kaydı hep gösterir:
/// döndürmeden önce okunanlar, eski dosya artık okunamasa da doğrudur.
struct OnbellekliKaynak<'a> {
    ic: &'a mut dyn OrnekKaynagi,
    /// Önceki çizimde okunanlar.
    onceki: &'a HashMap<usize, String>,
    /// Bu çizimde istenenler.
    simdiki: &'a mut HashMap<usize, String>,
}

impl OrnekKaynagi for OnbellekliKaynak<'_> {
    fn mesajlar(&mut self, istekler: &[OrnekIstegi<'_>]) -> io::Result<HashMap<usize, String>> {
        let mut bulunanlar = HashMap::with_capacity(istekler.len());
        let mut eksikler = Vec::new();
        for istek in istekler {
            match self
                .simdiki
                .get(&istek.sira)
                .or_else(|| self.onceki.get(&istek.sira))
            {
                Some(mesaj) => {
                    bulunanlar.insert(istek.sira, mesaj.clone());
                }
                None => eksikler.push(*istek),
            }
        }
        if !eksikler.is_empty() {
            bulunanlar.extend(self.ic.mesajlar(&eksikler)?);
        }
        for (sira, mesaj) in &bulunanlar {
            self.simdiki.insert(*sira, mesaj.clone());
        }
        Ok(bulunanlar)
    }

    fn degisti(&self) -> bool {
        self.ic.degisti()
    }
}

/// Döndürmeden sonraki örnek kaynağı: eski dosyanın kayıtları (sırası `ilk`ten küçük olanlar)
/// yeni dosyadan okunamaz; onlar istenmez.
struct SinirliKaynak<'a> {
    ic: &'a mut dyn OrnekKaynagi,
    ilk: usize,
}

impl OrnekKaynagi for SinirliKaynak<'_> {
    fn mesajlar(&mut self, istekler: &[OrnekIstegi<'_>]) -> io::Result<HashMap<usize, String>> {
        let okunabilenler: Vec<OrnekIstegi<'_>> = istekler
            .iter()
            .copied()
            .filter(|istek| istek.sira >= self.ilk)
            .collect();
        if okunabilenler.is_empty() {
            return Ok(HashMap::new());
        }
        self.ic.mesajlar(&okunabilenler)
    }

    fn degisti(&self) -> bool {
        self.ic.degisti()
    }
}

/// Girdinin yeniden okunabilen hali: örnekler için.
enum Yeniden {
    /// İzlenen dosyanın kendisi; her çizimde yeniden açılır.
    Dosya(PathBuf),
    /// Standart girdinin geçici kopyası; parçalar geldikçe ona da yazılır.
    Kopya(GeciciDosya),
    /// Yeniden okunamaz: örnekler depodakilerden seçilir.
    Yok,
}

/// Raporu çizen: terminalde ekranı temizleyerek, başka bir çıktıda ayırıcıyla.
pub struct Cizici {
    cikti: Box<dyn Write>,
    terminal: bool,
    cizildi: bool,
}

impl Cizici {
    /// Standart çıktıya çizer. Windows'un eski konsolunda ekranı temizleyen kaçış dizileri
    /// ancak açılırsa çalışır (`sanal_terminali_ac`); açılamazsa çıktı terminal değilmiş gibi
    /// ayırıcıyla çizilir.
    pub fn standart_cikti() -> Cizici {
        let terminal = io::stdout().is_terminal() && sanal_terminali_ac();
        Cizici::new(Box::new(BufWriter::new(io::stdout())), terminal)
    }

    pub fn new(cikti: Box<dyn Write>, terminal: bool) -> Cizici {
        Cizici {
            cikti,
            terminal,
            cizildi: false,
        }
    }

    /// Durum satırını ve raporu yazar.
    ///
    /// Terminalde ekran ve kaydırma geçmişi temizlenir, durum satırı en alta yazılır: rapor
    /// ekrandan uzunsa başı yukarı kayar, durum satırı hep görünür. Uyarılar da (standart
    /// hataya yazılır yazılmaz ekran temizlendiği için) durum satırının üstünde tekrarlanır.
    /// Başka bir çıktıda (dosya, boru) çizimler bir ayırıcıyla art arda yazılır, durum satırı
    /// başlarındadır; uyarılar yalnızca standart hatadadır.
    pub fn ciz(&mut self, durum: &str, uyarilar: &[String], ozet: &Ozet) -> io::Result<()> {
        let cikti = &mut self.cikti;
        if self.terminal {
            // İmleci sol üste al, ekranı ve kaydırma geçmişini temizle
            write!(cikti, "\x1b[H\x1b[2J\x1b[3J")?;
            rapor::metin_yaz(cikti, ozet)?;
            writeln!(cikti)?;
            for uyari in uyarilar {
                writeln!(cikti, "{uyari}")?;
            }
            writeln!(cikti, "{durum}")?;
        } else {
            if self.cizildi {
                writeln!(cikti, "{AYIRICI}")?;
            }
            writeln!(cikti, "{durum}")?;
            rapor::metin_yaz(cikti, ozet)?;
        }
        cikti.flush()?;
        self.cizildi = true;
        Ok(())
    }
}

/// Windows'un konsolunda ekranı temizleyen kaçış dizilerini (sanal terminal işlemeyi) açar.
/// Windows Terminal'de zaten açıktır; eski konsolda (conhost) program açmalıdır, yoksa
/// `\x1b[2J` ekrana olduğu gibi yazılır. Açılamazsa false.
#[cfg(windows)]
fn sanal_terminali_ac() -> bool {
    use std::os::windows::io::AsRawHandle;
    type Tutamac = *mut std::ffi::c_void;
    const SANAL_TERMINAL_ISLEME: u32 = 0x0004; // ENABLE_VIRTUAL_TERMINAL_PROCESSING
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetConsoleMode(konsol: Tutamac, kip: *mut u32) -> i32;
        fn SetConsoleMode(konsol: Tutamac, kip: u32) -> i32;
    }
    let konsol = io::stdout().as_raw_handle();
    let mut kip = 0u32;
    // SAFETY: tutamaç standart çıktınındır ve program boyunca açıktır; `kip` yazılabilir bir
    // u32'dir. İki işlev de başarısızlıkta 0 döndürür, belleğe başka bir şey yazmaz.
    unsafe {
        GetConsoleMode(konsol, &mut kip) != 0
            && (kip & SANAL_TERMINAL_ISLEME != 0
                || SetConsoleMode(konsol, kip | SANAL_TERMINAL_ISLEME) != 0)
    }
}

/// Unix terminalleri kaçış dizilerini her zaman işler.
#[cfg(not(windows))]
fn sanal_terminali_ac() -> bool {
    true
}

/// İzlemenin durumu: depo, toplanmakta olan kayıt, bekleyen satır ve çizim.
pub struct Izleyici<'a> {
    secenekler: &'a Secenekler,
    aralik: Duration,
    yol_metni: &'a str,
    gosterilen_yol: &'a str,
    depo: Depo,
    toplayici: KayitToplayici,
    tampon: SatirTamponu,
    yeniden: Yeniden,
    /// Döndürmeden sonra: bu sıradan önceki kayıtlar artık yeniden okunamaz.
    ilk_okunabilir: usize,
    donduruldu: bool,
    cizici: Cizici,
    /// Son çizimdeki satır sayıları ve depodaki kayıt sayısı: değişmediyse çizilmez.
    son_cizilen: Option<(SatirSayilari, usize)>,
    /// Girdiden en son bayt gelen an.
    son_veri: Instant,
    uyarildi: Uyarilar,
    /// Yazılan uyarılar: terminalde her çizimde durum satırının üstünde tekrarlanır.
    uyarilar: Vec<String>,
    /// Önceki çizimde okunan örnekler (`OnbellekliKaynak`).
    ornekler: HashMap<usize, String>,
    /// Standart girdinin geçici kopyasına yazılan bayt sayısı ve en fazla yazılabilecek
    /// (`KOPYA_SINIRI`; testlerde daha küçük).
    kopyalanan: u64,
    kopya_siniri: u64,
}

/// Standart girdi izlenirken geçici kopyanın en büyük boyu (256 MB). Kopya yalnızca ani
/// artışların örnekleri için tutulur ve izleme sürdükçe büyür; `/tmp` birçok dağıtımda
/// bellektedir. Sınır aşılınca kopya bırakılır: daha önce okunan örnekler saklanır, yenileri
/// aynı kalıpta aynı seviyedeki ilk kaydın mesajı olur.
const KOPYA_SINIRI: u64 = 256 << 20;

/// Bir kez yazılan uyarılar.
#[derive(Default)]
struct Uyarilar {
    gecersiz_utf8: bool,
    kayit_yok: bool,
    taninmayan_seviye: bool,
    kopya: bool,
}

impl<'a> Izleyici<'a> {
    /// `yeniden_okunabilir`: dosya izleniyorsa yolu (örnekler ondan okunur); standart girdide
    /// None (ani artış istendiyse geçici bir kopya tutulmaya çalışılır).
    pub fn new(
        secenekler: &'a Secenekler,
        izleme: Izleme,
        yol_metni: &'a str,
        gosterilen_yol: &'a str,
        yeniden_okunabilir: Option<&Path>,
        cizici: Cizici,
    ) -> Izleyici<'a> {
        let rapor = &secenekler.rapor;
        // Zaman sınırları her çizimde uygulanır; okunurken yalnızca seviye süzülür
        let seviye = Filtre {
            seviye: rapor.filtre.seviye,
            ..Filtre::default()
        };
        let filtre =
            (!seviye.bos_mu()).then(|| seviye.coz(None).expect("yalnızca seviye: aralık yok"));
        let depo = Depo::new(DepoAyarlari {
            drain: rapor.drain.clone(),
            filtre,
            saat_dilimi: rapor.saat_dilimi,
            satir_bicimi: rapor.satir_bicimi.clone(),
            konum_turu: rapor.konum_turu,
        });
        let mut uyarildi = Uyarilar::default();
        let mut uyarilar = Vec::new();
        let yeniden = match yeniden_okunabilir {
            Some(yol) => Yeniden::Dosya(yol.to_path_buf()),
            None if rapor.ani_artis.is_some() => match GeciciDosya::olustur() {
                Ok(gecici) => Yeniden::Kopya(gecici),
                Err(hata) => {
                    let uyari = crate::kopya_uyarisi(&hata);
                    eprintln!("{uyari}");
                    uyarilar.push(uyari);
                    uyarildi.kopya = true;
                    Yeniden::Yok
                }
            },
            None => Yeniden::Yok,
        };
        Izleyici {
            secenekler,
            aralik: izleme.aralik,
            yol_metni,
            gosterilen_yol,
            depo,
            toplayici: KayitToplayici::new(rapor.satir_bicimi.clone()),
            tampon: SatirTamponu::default(),
            yeniden,
            ilk_okunabilir: 0,
            donduruldu: false,
            cizici,
            son_cizilen: None,
            son_veri: Instant::now(),
            uyarildi,
            uyarilar,
            ornekler: HashMap::new(),
            kopyalanan: 0,
            kopya_siniri: KOPYA_SINIRI,
        }
    }

    /// Bir uyarı: standart hataya yazılır; terminalde ekran her çizimde temizlendiği için
    /// çizimlerde de gösterilir.
    fn uyari(&mut self, uyari: String) {
        eprintln!("{uyari}");
        self.uyarilar.push(uyari);
    }

    /// Girdiden gelen bir parçayı işler: tamamlanan satırlar kayıt toplayıcıya, tamamlanan
    /// kayıtlar depoya gider.
    pub fn isle(&mut self, parca: &[u8]) {
        self.son_veri = Instant::now();
        // Kopyanın konumu örnekler okunurken (`YenidenOkuyucu`, aynı dosya tutamacı) değişir:
        // her parça dosyanın sonuna yazılır
        if let Yeniden::Kopya(gecici) = &self.yeniden {
            let sonuc = if self.kopyalanan + parca.len() as u64 > self.kopya_siniri {
                Err(None)
            } else {
                gecici
                    .dosya()
                    .seek(SeekFrom::End(0))
                    .and_then(|_| gecici.dosya().write_all(parca))
                    .map_err(Some)
            };
            match sonuc {
                Ok(()) => self.kopyalanan += parca.len() as u64,
                // Kopya yazılamıyor (disk dolu) ya da sınırı aştı: bırakılır, bundan sonra
                // örnekler depodakilerden seçilir
                Err(hata) => {
                    self.yeniden = Yeniden::Yok;
                    if !self.uyarildi.kopya {
                        self.uyarildi.kopya = true;
                        let uyari = match hata {
                            Some(hata) => crate::kopya_uyarisi(&hata),
                            None => kopya_siniri_uyarisi(self.kopya_siniri),
                        };
                        self.uyari(uyari);
                    }
                }
            }
        }
        self.tampon.ekle(parca, |satir, konum| {
            self.toplayici.satir(satir, konum, &mut self.depo);
        });
    }

    /// Girdi bir aralık boyunca sessiz kaldıysa bekleyen satır ve toplanmakta olan kayıt
    /// tamamlanmış sayılır.
    pub fn sessizligi_denetle(&mut self) {
        if (self.toplayici.bekliyor() || self.tampon.bekleyen_var())
            && self.son_veri.elapsed() >= self.aralik
        {
            self.bitir();
        }
    }

    /// Girdi bitti (standart girdi kapandı) ya da sessiz kaldı: bekleyen her şey depoya
    /// alınır.
    pub fn bitir(&mut self) {
        self.tampon.bitir(|satir, konum| {
            self.toplayici.satir(satir, konum, &mut self.depo);
        });
        self.toplayici.bitir(&mut self.depo);
    }

    /// Dosya döndürüldü: eski dosya bitmiştir, bekleyen satırı ve kaydı depoya alınır; yeni
    /// dosyanın konumları sıfırdan sayılır.
    pub fn donduruldu(&mut self) {
        self.bitir();
        self.tampon.sifirla();
        self.depo.yeni_girdi();
        self.ilk_okunabilir = self.depo.kayit_sayisi();
        self.donduruldu = true;
    }

    /// Son çizimden beri satır ya da kayıt sayısı değişti mi?
    pub fn degisti(&self) -> bool {
        self.son_cizilen != Some(self.durum())
    }

    fn durum(&self) -> (SatirSayilari, usize) {
        (self.toplayici.sayilar(), self.depo.kayit_sayisi())
    }

    /// Depodaki kayıt sayısı.
    #[cfg(test)]
    pub fn kayit_sayisi(&self) -> usize {
        self.depo.kayit_sayisi()
    }

    /// Raporu depodan üretip çizer.
    pub fn ciz(&mut self) -> Result<(), Hata> {
        self.depo.ara_durum(OkumaSonucu {
            sayilar: self.toplayici.sayilar(),
            gecersiz_utf8: self.tampon.gecersiz_utf8(),
        });
        self.uyar();
        let rapor = &self.secenekler.rapor;
        let dizin = self.depo.dizin().clone();
        let (filtre, dilim) = (self.depo.filtre().copied(), self.depo.saat_dilimi());
        let bicim = self.depo.satir_bicimi().clone();
        let yeniden_okuyucu = |dosya: File| {
            YenidenOkuyucu::new(
                BufReader::with_capacity(TAMPON, dosya),
                dizin,
                filtre,
                dilim,
            )
            .bicimle(bicim)
        };
        let mut kaynak: Box<dyn OrnekKaynagi> = match &self.yeniden {
            Yeniden::Dosya(yol) => match File::open(yol) {
                Ok(dosya) => Box::new(yeniden_okuyucu(dosya)),
                Err(_) => Box::new(OkunamazKaynak),
            },
            Yeniden::Kopya(gecici) => match gecici.dosya().try_clone() {
                Ok(dosya) => Box::new(yeniden_okuyucu(dosya)),
                Err(_) => Box::new(OkunamazKaynak),
            },
            Yeniden::Yok => Box::new(OkunamazKaynak),
        };
        let mut sinirli = SinirliKaynak {
            ic: kaynak.as_mut(),
            ilk: self.ilk_okunabilir,
        };
        let onceki = std::mem::take(&mut self.ornekler);
        let mut sinirli = OnbellekliKaynak {
            ic: &mut sinirli,
            onceki: &onceki,
            simdiki: &mut self.ornekler,
        };
        let ozet = if rapor.filtre.zaman_sinirli() {
            // Pencere okunan en yeni kayda göre çözülür, seviye filtresine uymasa da: toplu
            // okumadaki ön tarama da bütün kayıtlara bakar. Log ilerledikçe aralık boşalabilir
            // (`--bitis 2026-10-25`): o zaman pencerede kayıt yoktur, izleme sürer.
            let cozulmus = rapor.filtre.coz_bos_olabilir(self.depo.okunan_en_yeni());
            // Deponun örnekleri önce tamamlanır: pencere onları paylaşır, bir kez okunur
            self.depo
                .ornekleri_tamamla(&mut sinirli)
                .map_err(|hata| Hata::Ozet(OzetHatasi::Okuma(hata)))?;
            let (mut pencere, tam_sira) = self.depo.pencere(cozulmus);
            let mut cevrilmis = CevrilmisKaynak {
                ic: &mut sinirli,
                tam_sira: &tam_sira,
            };
            Ozet::hesapla(
                self.yol_metni.to_owned(),
                &mut pencere,
                rapor,
                &mut cevrilmis,
            )
        } else {
            Ozet::hesapla(
                self.yol_metni.to_owned(),
                &mut self.depo,
                rapor,
                &mut sinirli,
            )
        }
        .map_err(Hata::Ozet)?;
        let durum = self.durum_satiri();
        self.cizici
            .ciz(&durum, &self.uyarilar, &ozet)
            .map_err(Hata::Yazma)?;
        self.son_cizilen = Some(self.durum());
        Ok(())
    }

    /// Raporun başındaki satır:
    /// "İzleniyor: laravel.log · her 2 sn · 1234 kayıt · son kayıt 2026-10-24 14:03:11";
    /// İngilizcede "Watching: laravel.log · every 2 s · 1234 records · last record ..."
    fn durum_satiri(&self) -> String {
        let (yol, aralik, sayi) = (
            self.gosterilen_yol,
            aralik_yaz(self.aralik),
            self.depo.kayit_sayisi(),
        );
        let mut parcalar = match dil::simdiki() {
            Dil::Tr => vec![format!("İzleniyor: {yol}"), format!("her {aralik}")],
            Dil::En => vec![format!("Watching: {yol}"), format!("every {aralik}")],
        };
        parcalar.push(dil::sayili(sayi, "kayıt", "record", "records"));
        parcalar.push(match self.depo.okunan_en_yeni() {
            Some(en_yeni) => format!("{} {en_yeni}", dil::sec("son kayıt", "last record")),
            None => dil::sec("henüz kayıt yok", "no records yet").to_owned(),
        });
        if self.donduruldu {
            parcalar.push(dil::sec("döndürüldü", "rotated").to_owned());
        }
        parcalar.join(" · ")
    }

    /// Bir kez yazılan uyarılar: geçersiz UTF-8, biçimi uymayan log, seviyesi tanınmayan
    /// satırlar.
    fn uyar(&mut self) {
        if self.depo.gecersiz_utf8() && !self.uyarildi.gecersiz_utf8 {
            self.uyarildi.gecersiz_utf8 = true;
            self.uyari(crate::gecersiz_utf8_uyarisi(self.gosterilen_yol));
        }
        let sayilar = self.depo.sayilar();
        if sayilar.kayit == 0 && sayilar.satir > 0 && !self.uyarildi.kayit_yok {
            self.uyarildi.kayit_yok = true;
            let uyari =
                crate::kayit_yok_uyarisi(self.gosterilen_yol, &self.secenekler.rapor.satir_bicimi);
            self.uyari(uyari);
        }
        if sayilar.taninmayan_seviye > 0 && !self.uyarildi.taninmayan_seviye {
            self.uyarildi.taninmayan_seviye = true;
            self.uyari(crate::taninmayan_seviye_uyarisi(
                self.gosterilen_yol,
                sayilar.taninmayan_seviye,
            ));
        }
    }

    /// Dosyayı Ctrl+C'ye kadar izler.
    pub fn dosya_izle(mut self, mut kaynak: DosyaKaynagi) -> Result<(), Hata> {
        self.dosya_adimi(&mut kaynak, true)?;
        loop {
            thread::sleep(self.aralik);
            self.dosya_adimi(&mut kaynak, false)?;
        }
    }

    /// Dosya izlemenin bir adımı: eldeki dosyada yeni ne varsa okunur; yolda artık başka bir
    /// dosya varsa (döndürme) eskisi bitmiş sayılır ve yenisi baştan okunur; sessizlik
    /// denetlenir; bir şey değiştiyse (ilk adımda her zaman) rapor çizilir.
    fn dosya_adimi(&mut self, kaynak: &mut DosyaKaynagi, ilk: bool) -> Result<(), Hata> {
        kaynak.oku(|parca| self.isle(parca)).map_err(Hata::Okuma)?;
        if kaynak.dondurme_denetle() == Dondurme::Donduruldu {
            self.donduruldu();
            kaynak.oku(|parca| self.isle(parca)).map_err(Hata::Okuma)?;
        }
        if ilk && let Some(gecen) = kaynak.degismeden_gecen() {
            match Instant::now().checked_sub(gecen) {
                // Dosya bir süredir değişmiyorsa sessizlik o zamandan sayılır
                Some(degisti) => self.son_veri = self.son_veri.min(degisti),
                // Makine açılmadan önce değişmiş: `Instant` o kadar geriye gidemez, sessizlik
                // çoktan doldu
                None => self.bitir(),
            }
        }
        self.sessizligi_denetle();
        if ilk || self.degisti() {
            self.ciz()?;
        }
        Ok(())
    }

    /// Bir akışı (standart girdi) bitene kadar izler; bitince son raporu çizer.
    pub fn akis_izle(mut self, alan: Receiver<io::Result<Vec<u8>>>) -> Result<(), Hata> {
        let mut son_cizim: Option<Instant> = None;
        loop {
            match alan.recv_timeout(self.aralik) {
                Ok(Ok(parca)) => self.isle(&parca),
                Ok(Err(hata)) => return Err(Hata::Okuma(hata)),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    self.bitir();
                    return self.ciz();
                }
            }
            self.sessizligi_denetle();
            if (son_cizim.is_none() || self.degisti())
                && son_cizim.is_none_or(|an| an.elapsed() >= self.aralik)
            {
                self.ciz()?;
                son_cizim = Some(Instant::now());
            }
        }
    }
}

/// Standart girdinin geçici kopyası sınırını (`KOPYA_SINIRI`) aştı.
fn kopya_siniri_uyarisi(sinir: u64) -> String {
    let mb = sinir >> 20;
    match dil::simdiki() {
        Dil::Tr => format!(
            "Uyarı: girdinin geçici kopyası {mb} MB'ı aştı ve bırakıldı; bundan sonraki bazı örnekler aynı kalıpta aynı seviyedeki ilk kaydın mesajı olacak."
        ),
        Dil::En => format!(
            "Warning: the temporary copy of the input exceeded {mb} MB and was dropped; some later samples will be the message of the first record with the same pattern and level."
        ),
    }
}

/// `--izle`: girdiyi canlı izler. Dosyaysa Ctrl+C'ye kadar; standart girdiyse (ya da boru,
/// aygıt gibi normal olmayan bir dosyaysa) girdi bitene kadar.
pub fn calistir(
    secenekler: &Secenekler,
    izleme: Izleme,
    standart_girdi: bool,
    yol_metni: &str,
    gosterilen_yol: &str,
) -> Result<(), Hata> {
    if standart_girdi {
        let izleyici = Izleyici::new(
            secenekler,
            izleme,
            yol_metni,
            gosterilen_yol,
            None,
            Cizici::standart_cikti(),
        );
        return izleyici.akis_izle(akis_baslat(io::stdin()));
    }
    let dosya = dosyayi_ac(&secenekler.dosya).map_err(Hata::Okuma)?;
    if dosya.metadata().is_ok_and(|bilgi| bilgi.is_file()) {
        let kaynak = DosyaKaynagi::new(&secenekler.dosya, dosya);
        let izleyici = Izleyici::new(
            secenekler,
            izleme,
            yol_metni,
            gosterilen_yol,
            Some(&secenekler.dosya),
            Cizici::standart_cikti(),
        );
        izleyici.dosya_izle(kaynak)
    } else {
        // Boru ya da aygıt: standart girdi gibi bir kez, bitene kadar okunur
        let izleyici = Izleyici::new(
            secenekler,
            izleme,
            yol_metni,
            gosterilen_yol,
            None,
            Cizici::standart_cikti(),
        );
        izleyici.akis_izle(akis_baslat(dosya))
    }
}

#[cfg(test)]
mod testler {
    use super::*;
    use crate::komut_satiri::{self, Komut};
    use std::ffi::OsString;
    use std::sync::{Arc, Mutex};

    /// Parçaları ekler; satırları ve konumlarını toplar.
    fn satirlar(tampon: &mut SatirTamponu, parcalar: &[&[u8]]) -> Vec<(String, u64)> {
        let mut toplanan = Vec::new();
        for parca in parcalar {
            tampon.ekle(parca, |satir, konum| {
                toplanan.push((satir.to_owned(), konum))
            });
        }
        toplanan
    }

    #[test]
    fn satirlar_parca_sinirlarindan_bagimsiz() {
        let metin = b"ilk satir\nikinci\r\n\nson";
        let butun = {
            let mut tampon = SatirTamponu::default();
            let bulunan = satirlar(&mut tampon, &[metin]);
            assert!(tampon.bekleyen_var());
            // Bekleyen "son"un başı
            assert_eq!(tampon.konum(), 19);
            bulunan
        };
        assert_eq!(
            butun,
            vec![
                ("ilk satir".to_owned(), 0),
                ("ikinci".to_owned(), 10),
                (String::new(), 18)
            ]
        );
        // Her bayt sınırından bölünmüş parçalar aynı satırları verir
        for bolme in 1..metin.len() {
            let mut tampon = SatirTamponu::default();
            let parcalar: Vec<&[u8]> = vec![&metin[..bolme], &metin[bolme..]];
            assert_eq!(satirlar(&mut tampon, &parcalar), butun, "bölme: {bolme}");
            assert_eq!(tampon.konum(), 19);
        }
        // Bekleyen son satır, satır sonu gelince verilir
        let mut tampon = SatirTamponu::default();
        satirlar(&mut tampon, &[metin]);
        assert_eq!(
            satirlar(&mut tampon, &[b" satir\n"]),
            vec![("son satir".to_owned(), 19)]
        );
        assert!(!tampon.bekleyen_var());
        assert_eq!(tampon.konum(), 29);
        // Girdi biterse (ya da sessiz kalırsa) satır sonu olmadan verilir; sondaki \r kalır
        let mut tampon = SatirTamponu::default();
        assert_eq!(
            butun_satirlar(&mut tampon, &[b"bir\nson\r"]),
            vec![("bir".to_owned(), 0), ("son\r".to_owned(), 4)]
        );
        assert_eq!(tampon.konum(), 8);
        // Bekleyen yoksa bitir bir şey vermez
        tampon.bitir(|_, _| panic!("satır beklenmiyordu"));
    }

    /// Parçalar ve son `bitir`: tampon girdinin bütün satırlarını verir.
    fn butun_satirlar(tampon: &mut SatirTamponu, parcalar: &[&[u8]]) -> Vec<(String, u64)> {
        let mut toplanan = satirlar(tampon, parcalar);
        tampon.bitir(|satir, konum| toplanan.push((satir.to_owned(), konum)));
        assert!(!tampon.bekleyen_var());
        toplanan
    }

    #[test]
    fn tampon_parcalarla_satir_okuyucuyla_ayni_satirlari_verir() {
        // Rastgele baytlar (satır sonları, CR, iki baytlı Ö'nün yarıları, geçersiz bayt) rastgele
        // parçalara bölünür; satırlar, konumları ve geçersiz UTF-8 bilgisi girdinin bir kerede
        // okunmasıyla (`SatirOkuyucu`) aynı olmalı. Küçük satır sınırlarıyla da.
        let harfler: [&[u8]; 7] = [
            b"a",
            b"\n",
            b"\r",
            b"\r\n",
            "Ö".as_bytes(),
            b"\xff",
            b"\xc3",
        ];
        let mut tohum: u64 = 0x5eed;
        let mut rastgele = |ust: usize| {
            tohum = tohum
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (tohum >> 33) as usize % ust
        };
        for deneme in 0..3_000 {
            let mut girdi = Vec::new();
            for _ in 0..rastgele(24) {
                girdi.extend_from_slice(harfler[rastgele(harfler.len())]);
            }
            let sinir = [1, 2, 3, 4, 7, SATIR_SINIRI][rastgele(6)];
            let mut okuyucu = crate::okuyucu::SatirOkuyucu::sinirli(io::Cursor::new(&girdi), sinir);
            let mut beklenen = Vec::new();
            while let Some((satir, konum)) = okuyucu.sonraki().unwrap() {
                beklenen.push((satir.into_owned(), konum));
            }
            let mut parcalar: Vec<&[u8]> = Vec::new();
            let mut kalan = &girdi[..];
            while !kalan.is_empty() {
                let (parca, geri) = kalan.split_at(1 + rastgele(kalan.len()));
                parcalar.push(parca);
                kalan = geri;
            }
            let mut tampon = SatirTamponu::sinirli(sinir);
            assert_eq!(
                butun_satirlar(&mut tampon, &parcalar),
                beklenen,
                "deneme {deneme}: {girdi:?}, sınır {sinir}, parçalar {parcalar:?}"
            );
            assert_eq!(
                tampon.gecersiz_utf8(),
                okuyucu.gecersiz_utf8(),
                "deneme {deneme}"
            );
        }
    }

    #[test]
    fn cok_baytli_karakter_parca_sinirinda_bolunebilir() {
        let metin = "Ödeme alındı: ürün\n".as_bytes();
        let mut tampon = SatirTamponu::default();
        // "Ö" iki bayttır: ilk baytı birinci parçada
        let bulunan = satirlar(&mut tampon, &[&metin[..1], &metin[1..]]);
        assert_eq!(bulunan, vec![("Ödeme alındı: ürün".to_owned(), 0)]);
        assert!(!tampon.gecersiz_utf8());
        // Gerçekten geçersiz bayt � olur ve işaretlenir
        let mut tampon = SatirTamponu::default();
        let bulunan = satirlar(&mut tampon, &[b"a\xffb\n"]);
        assert_eq!(bulunan, vec![("a\u{FFFD}b".to_owned(), 0)]);
        assert!(tampon.gecersiz_utf8());
    }

    #[test]
    fn siniri_asan_satir_kesilir_sonrasi_atilir() {
        let mut tampon = SatirTamponu::sinirli(4);
        let bulunan = satirlar(&mut tampon, &[b"abc", b"defgh\nxy\n"]);
        assert_eq!(bulunan, vec![("abcd".to_owned(), 0), ("xy".to_owned(), 9)]);
        assert!(!tampon.bekleyen_var());
        // Tam sınıra sığan satır kesilmiş sayılmaz: sondaki \r atılır
        let mut tampon = SatirTamponu::sinirli(4);
        assert_eq!(
            satirlar(&mut tampon, &[b"abc\r\n"]),
            vec![("abc".to_owned(), 0)]
        );
        // Kesilen yerde yarım kalan UTF-8 karakteri atılır, geçersiz sayılmaz
        let mut tampon = SatirTamponu::sinirli(2);
        assert_eq!(
            satirlar(&mut tampon, &["aÖb\n".as_bytes()]),
            vec![("a".to_owned(), 0)]
        );
        assert!(!tampon.gecersiz_utf8());
        // Kesilmiş bir satır beklerken girdi biterse kesilmiş haliyle verilir; yeni girdinin
        // (döndürülen dosyanın) konumları sıfırdan sayılır
        let mut tampon = SatirTamponu::sinirli(4);
        satirlar(&mut tampon, &[b"abcdefg"]);
        assert!(tampon.bekleyen_var());
        assert_eq!(
            butun_satirlar(&mut tampon, &[]),
            vec![("abcd".to_owned(), 0)]
        );
        assert_eq!(tampon.konum(), 7);
        tampon.sifirla();
        assert_eq!(tampon.konum(), 0);
        assert_eq!(
            satirlar(&mut tampon, &[b"hi\n"]),
            vec![("hi".to_owned(), 0)]
        );
    }

    #[test]
    fn aralik_okunur_yazilir() {
        assert_eq!(aralik_yaz(Duration::from_millis(500)), "500 ms");
        assert_eq!(aralik_yaz(Duration::from_secs(2)), "2 sn");
        assert_eq!(aralik_yaz(Duration::from_millis(1_500)), "1 sn 500 ms");
        assert_eq!(aralik_yaz(Duration::from_secs(90)), "1 dk 30 sn");
    }

    /// Testin geçici dosyası: her test kendi adını verir.
    fn gecici_yol(ad: &str) -> PathBuf {
        std::env::temp_dir().join(format!("veylog-izle-{}-{ad}", std::process::id()))
    }

    #[test]
    fn dosya_kaldigi_yerden_okunur_kisalirsa_bastan() {
        let yol = gecici_yol("kisalan.log");
        fs::write(&yol, b"bir\n").unwrap();
        let mut kaynak = DosyaKaynagi::ac(&yol).unwrap();
        let mut okunan = Vec::new();
        assert!(kaynak.oku(|parca| okunan.extend_from_slice(parca)).unwrap());
        assert_eq!(okunan, b"bir\n");
        // Yeni bir şey yoksa hiçbir parça gelmez
        assert!(!kaynak.oku(|_| panic!("parça beklenmiyordu")).unwrap());
        assert_eq!(kaynak.dondurme_denetle(), Dondurme::Yok);
        // Eklenen kısım kaldığı yerden okunur
        fs::OpenOptions::new()
            .append(true)
            .open(&yol)
            .unwrap()
            .write_all(b"iki\n")
            .unwrap();
        okunan.clear();
        assert!(kaynak.oku(|parca| okunan.extend_from_slice(parca)).unwrap());
        assert_eq!(okunan, b"iki\n");
        assert_eq!(kaynak.konum(), 8);
        // Dosya kısaldı (copytruncate): baştan okunur
        fs::write(&yol, b"uc\n").unwrap();
        assert_eq!(kaynak.dondurme_denetle(), Dondurme::Donduruldu);
        assert_eq!(kaynak.konum(), 0);
        okunan.clear();
        kaynak.oku(|parca| okunan.extend_from_slice(parca)).unwrap();
        assert_eq!(okunan, b"uc\n");
        fs::remove_file(&yol).ok();
    }

    #[test]
    fn yolda_baska_dosya_varsa_donduruldu() {
        let yol = gecici_yol("donen.log");
        fs::write(&yol, b"eski\n").unwrap();
        let mut kaynak = DosyaKaynagi::ac(&yol).unwrap();
        kaynak.oku(|_| {}).unwrap();
        // Yol geçici olarak yok: döndürme sayılmaz, eldeki dosya okunur
        let yedek = gecici_yol("donen.log.1");
        fs::rename(&yol, &yedek).unwrap();
        assert_eq!(kaynak.dondurme_denetle(), Dondurme::Yok);
        // Yeni bir dosya: baştan okunur
        fs::write(&yol, b"yeni\n").unwrap();
        // Windows'ta kimlik oluşturulma zamanıdır; aynı anda oluşturulmuş sayılmasın
        if !cfg!(unix) {
            thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(kaynak.dondurme_denetle(), Dondurme::Donduruldu);
        let mut okunan = Vec::new();
        kaynak.oku(|parca| okunan.extend_from_slice(parca)).unwrap();
        assert_eq!(okunan, b"yeni\n");
        // Konumlar yeni dosyada sıfırdan sayılır
        assert_eq!(kaynak.konum(), 5);
        fs::remove_file(&yol).ok();
        fs::remove_file(&yedek).ok();
    }

    #[test]
    fn akis_parcalar_halinde_gelir_bitince_kanal_kapanir() {
        let alan = akis_baslat(io::Cursor::new(b"abc\ndef".to_vec()));
        let mut toplanan = Vec::new();
        for parca in alan.iter() {
            toplanan.extend_from_slice(&parca.unwrap());
        }
        assert_eq!(toplanan, b"abc\ndef");
    }

    /// Testlerde çizimin yazıldığı yer.
    #[derive(Clone, Default)]
    struct Yazilan(Arc<Mutex<Vec<u8>>>);

    impl Write for Yazilan {
        fn write(&mut self, tampon: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(tampon);
            Ok(tampon.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Yazilan {
        fn metin(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    fn secenekler(argumanlar: &[&str]) -> Secenekler {
        match komut_satiri::ayristir(argumanlar.iter().map(OsString::from)) {
            Ok(Komut::Ozet(secenekler)) => *secenekler,
            diger => panic!("özet bekleniyordu: {diger:?}"),
        }
    }

    fn izleyici<'a>(secenekler: &'a Secenekler, yazilan: &Yazilan) -> Izleyici<'a> {
        Izleyici::new(
            secenekler,
            secenekler.izleme.expect("--izle"),
            "deneme.log",
            "deneme.log",
            None,
            Cizici::new(Box::new(yazilan.clone()), false),
        )
    }

    const KAYIT: &str = "[2026-10-24 10:00:00] production.ERROR: Ödeme alınamadı\n";
    const DEVAM: &str = "#0 /var/www/magaza/app/Odeme.php(12): odeme()\n";

    #[test]
    fn son_kayit_baslik_gelince_ya_da_sessizlikte_tamamlanir() {
        let secenekler = secenekler(&["--izle", "--aralik", "100ms", "deneme.log"]);
        let yazilan = Yazilan::default();
        let mut izleyici = izleyici(&secenekler, &yazilan);
        izleyici.isle(KAYIT.as_bytes());
        // Kayıt henüz depoda değil: devam satırları gelebilir
        assert_eq!(izleyici.kayit_sayisi(), 0);
        // Sessizlik son baytın gelişinden sayılır: eski bir an, veri gelince unutulur
        izleyici.son_veri = Instant::now() - Duration::from_millis(150);
        izleyici.isle(DEVAM.as_bytes());
        izleyici.sessizligi_denetle();
        assert_eq!(izleyici.kayit_sayisi(), 0, "sessizlik süresi dolmadı");
        // Yeni bir başlık öncekini tamamlar
        izleyici.isle(KAYIT.as_bytes());
        assert_eq!(izleyici.kayit_sayisi(), 1);
        // Bir aralık boyunca sessizlik: ikinci kayıt da tamamlanır
        izleyici.son_veri = Instant::now() - Duration::from_millis(150);
        izleyici.sessizligi_denetle();
        assert_eq!(izleyici.kayit_sayisi(), 2);
        // Satır sonu gelmemiş bir başlık: sessizlik dolmadan beklenir
        izleyici.isle(b"[2026-10-24 10:00:02] production.ERROR: yarim");
        izleyici.sessizligi_denetle();
        assert_eq!(izleyici.kayit_sayisi(), 2);
        // Başlığın satırı bitti, kaydı toplanıyor; arkasındaki başlığın satırı bitmedi
        izleyici.isle(b" kayit\n[2026-10-24 10:00:03] production.ERROR: son");
        assert_eq!(izleyici.kayit_sayisi(), 2);
        // Sessizlikte satır sonu gelmemiş satır da son satır sayılır: rapor, gelen baytların
        // bir kerede özetlenmesiyle aynı olur
        izleyici.son_veri = Instant::now() - Duration::from_millis(150);
        izleyici.sessizligi_denetle();
        assert_eq!(izleyici.kayit_sayisi(), 4);
        assert!(!izleyici.tampon.bekleyen_var());
        // Toplanan kayıt yokken de satır sonu gelmemiş satır sessizlikte okunur
        izleyici.isle(b"[2026-10-24 10:00:04] production.ERROR: tek");
        assert_eq!(izleyici.kayit_sayisi(), 4);
        izleyici.son_veri = Instant::now() - Duration::from_millis(150);
        izleyici.sessizligi_denetle();
        assert_eq!(izleyici.kayit_sayisi(), 5);
    }

    #[test]
    fn cizim_durum_satiriyla_baslar_ve_degismedikce_cizilmez() {
        let secenekler = secenekler(&["--izle", "--aralik", "500ms", "deneme.log"]);
        let yazilan = Yazilan::default();
        let mut izleyici = izleyici(&secenekler, &yazilan);
        assert!(izleyici.degisti(), "hiç çizilmedi");
        izleyici.ciz().ok().unwrap();
        let ilk = yazilan.metin();
        assert!(
            ilk.starts_with("İzleniyor: deneme.log · her 500 ms · 0 kayıt · henüz kayıt yok\n"),
            "{ilk}"
        );
        assert!(ilk.contains("deneme.log: 0 satır\n"), "{ilk}");
        assert!(!ilk.contains(AYIRICI));
        assert!(!izleyici.degisti());
        izleyici.isle(KAYIT.as_bytes());
        assert!(izleyici.degisti(), "satır sayısı değişti");
        izleyici.bitir();
        izleyici.ciz().ok().unwrap();
        let ikinci = &yazilan.metin()[ilk.len()..];
        assert!(ikinci.starts_with(&format!("{AYIRICI}\n")), "{ikinci}");
        assert!(
            ikinci.contains("· 1 kayıt · son kayıt 2026-10-24 10:00:00\n"),
            "{ikinci}"
        );
        assert!(ikinci.contains("Ödeme alınamadı"), "{ikinci}");
        // Bekleyen kaydın tamamlanması satır sayısını değil depodaki kayıt sayısını değiştirir;
        // o da yeniden çizilmeyi gerektirir
        izleyici.isle(KAYIT.as_bytes());
        izleyici.ciz().ok().unwrap();
        assert!(!izleyici.degisti());
        izleyici.bitir();
        assert!(izleyici.degisti(), "depoya giren kayıt çizimi değiştirir");
    }

    #[test]
    fn ingilizce_durum_satiri() {
        let _dil = dil::gecici(Dil::En);
        let secenekler = secenekler(&["--watch", "--interval", "1500ms", "deneme.log"]);
        let yazilan = Yazilan::default();
        let mut izleyici = izleyici(&secenekler, &yazilan);
        izleyici.ciz().ok().unwrap();
        assert!(
            yazilan.metin().starts_with(
                "Watching: deneme.log · every 1 s 500 ms · 0 records · no records yet\n"
            ),
            "{}",
            yazilan.metin()
        );
        izleyici.isle(KAYIT.as_bytes());
        izleyici.donduruldu();
        izleyici.ciz().ok().unwrap();
        let metin = yazilan.metin();
        let son = metin.rsplit_once(AYIRICI).unwrap().1;
        assert!(
            son.starts_with(
                "\nWatching: deneme.log · every 1 s 500 ms · 1 record · last record 2026-10-24 10:00:00 · rotated\n"
            ),
            "{son}"
        );
        assert_eq!(aralik_yaz(Duration::from_millis(500)), "500 ms");
        assert_eq!(aralik_yaz(Duration::from_secs(90)), "1 min 30 s");
    }

    #[test]
    fn seviye_filtresi_okunurken_uygulanir() {
        let secenekler = secenekler(&["--izle", "--seviye", "error", "deneme.log"]);
        let yazilan = Yazilan::default();
        let mut izleyici = izleyici(&secenekler, &yazilan);
        izleyici.isle(
            "[2026-10-24 10:00:00] production.INFO: Kullanıcı 7 giriş yaptı\n\
             [2026-10-24 10:00:01] production.ERROR: Ödeme alınamadı\n"
                .as_bytes(),
        );
        izleyici.bitir();
        assert_eq!(izleyici.kayit_sayisi(), 1);
        izleyici.ciz().ok().unwrap();
        let metin = yazilan.metin();
        assert!(metin.contains("ERROR ve üstü: 1 kayıt"), "{metin}");
        assert!(!metin.contains("giriş yaptı"), "{metin}");
    }

    #[test]
    fn terminalde_ekran_temizlenir() {
        let yazilan = Yazilan::default();
        let secenekler = secenekler(&["--izle", "deneme.log"]);
        let mut cizici = Cizici::new(Box::new(yazilan.clone()), true);
        let mut depo = crate::depo::metinden("");
        let ozet = Ozet::hesapla(
            "deneme.log".to_owned(),
            &mut depo,
            &secenekler.rapor,
            &mut OkunamazKaynak,
        )
        .unwrap();
        let uyarilar = ["Uyarı: bir şey".to_owned()];
        cizici.ciz("durum", &[], &ozet).unwrap();
        cizici.ciz("durum", &uyarilar, &ozet).unwrap();
        let metin = yazilan.metin();
        // Ekran ve kaydırma geçmişi temizlenir; durum satırı en altta, uyarılar onun üstünde
        let temizle = "\x1b[H\x1b[2J\x1b[3J";
        assert!(metin.starts_with(temizle), "{metin:?}");
        assert_eq!(metin.matches(temizle).count(), 2);
        let (ilk, ikinci) = metin[temizle.len()..].split_once(temizle).unwrap();
        assert!(ilk.starts_with("deneme.log: 0 satır\n"), "{ilk:?}");
        assert!(ilk.ends_with("\n\ndurum\n"), "{ilk:?}");
        assert!(
            ikinci.ends_with("\n\nUyarı: bir şey\ndurum\n"),
            "{ikinci:?}"
        );
        assert!(!metin.contains(AYIRICI));
    }

    #[test]
    fn dondurmede_eski_dosyanin_bekleyenleri_tamamlanir() {
        let secenekler = secenekler(&["--izle", "deneme.log"]);
        let yazilan = Yazilan::default();
        let mut izleyici = izleyici(&secenekler, &yazilan);
        izleyici.isle(KAYIT.as_bytes());
        izleyici.isle(b"[2026-10-24 10:00:01] production.ERROR: yarim");
        izleyici.donduruldu();
        // Eski dosya bitti: satır sonu olmayan son satırı da bir kayıt
        assert_eq!(izleyici.kayit_sayisi(), 2);
        assert_eq!(izleyici.ilk_okunabilir, 2);
        assert!(izleyici.donduruldu);
        assert!(!izleyici.tampon.bekleyen_var());
        // Yeni dosyanın ilk satırı sıfırıncı konumdan sayılır ve dizine yazılır
        izleyici.isle(KAYIT.as_bytes());
        izleyici.bitir();
        assert_eq!(izleyici.kayit_sayisi(), 3);
        assert_eq!(izleyici.tampon.konum(), KAYIT.len() as u64);
        assert_eq!(izleyici.depo.dizin().kayitlar(), [(0, 0), (2, 0)]);
        izleyici.ciz().ok().unwrap();
        assert!(yazilan.metin().contains("· döndürüldü\n"));
    }

    /// Bir dosyayı izleyen izleyici; örnekler dosyanın kendisinden okunur.
    fn dosya_izleyicisi<'a>(
        secenekler: &'a Secenekler,
        yol: &'a Path,
        yazilan: &Yazilan,
    ) -> Izleyici<'a> {
        Izleyici::new(
            secenekler,
            secenekler.izleme.expect("--izle"),
            "deneme.log",
            "deneme.log",
            Some(yol),
            Cizici::new(Box::new(yazilan.clone()), false),
        )
    }

    #[test]
    fn durgun_dosyanin_son_kaydi_ilk_cizimde_gorunur() {
        let secenekler = secenekler(&["--izle", "deneme.log"]);
        // Az önce yazılmış dosya: son kayıt bir aralık bekletilir
        let yol = gecici_yol("taze.log");
        fs::write(&yol, KAYIT).unwrap();
        let yazilan = Yazilan::default();
        let mut izleyici = dosya_izleyicisi(&secenekler, &yol, &yazilan);
        let mut kaynak = DosyaKaynagi::ac(&yol).unwrap();
        izleyici.dosya_adimi(&mut kaynak, true).ok().unwrap();
        assert_eq!(izleyici.kayit_sayisi(), 0);
        assert!(
            yazilan.metin().contains(" · 0 kayıt · "),
            "{}",
            yazilan.metin()
        );
        fs::remove_file(&yol).ok();
        // Bir saattir değişmeyen dosya: son kaydı tamamdır
        let yol = gecici_yol("durgun.log");
        fs::write(&yol, KAYIT).unwrap();
        File::options()
            .write(true)
            .open(&yol)
            .unwrap()
            .set_modified(SystemTime::now() - Duration::from_secs(3_600))
            .unwrap();
        let yazilan = Yazilan::default();
        let mut izleyici = dosya_izleyicisi(&secenekler, &yol, &yazilan);
        let mut kaynak = DosyaKaynagi::ac(&yol).unwrap();
        izleyici.dosya_adimi(&mut kaynak, true).ok().unwrap();
        assert_eq!(izleyici.kayit_sayisi(), 1);
        let metin = yazilan.metin();
        assert!(
            metin.contains(" · 1 kayıt · son kayıt 2026-10-24 10:00:00\n"),
            "{metin}"
        );
        assert!(metin.contains("Ödeme alınamadı"), "{metin}");
        fs::remove_file(&yol).ok();
    }

    #[cfg(unix)]
    #[test]
    fn dondurmeden_once_eski_dosyaya_yazilanlar_kaybolmaz() {
        let secenekler = secenekler(&["--izle", "deneme.log"]);
        let yol = gecici_yol("dondurulen.log");
        fs::write(&yol, KAYIT).unwrap();
        let yazilan = Yazilan::default();
        let mut izleyici = dosya_izleyicisi(&secenekler, &yol, &yazilan);
        let mut kaynak = DosyaKaynagi::ac(&yol).unwrap();
        izleyici.dosya_adimi(&mut kaynak, true).ok().unwrap();
        // Son okumadan sonra eski dosyaya bir kayıt daha yazılır, sonra dosya döndürülür
        fs::OpenOptions::new()
            .append(true)
            .open(&yol)
            .unwrap()
            .write_all(b"[2026-10-24 10:00:01] production.ERROR: Son eski hata\n")
            .unwrap();
        let yedek = gecici_yol("dondurulen.log.1");
        fs::rename(&yol, &yedek).unwrap();
        fs::write(&yol, "[2026-10-24 10:00:02] production.ERROR: Yeni hata\n").unwrap();
        izleyici.dosya_adimi(&mut kaynak, false).ok().unwrap();
        // Eski dosyanın iki kaydı alındı; yeni dosyanınki bekliyor
        assert_eq!(izleyici.kayit_sayisi(), 2);
        assert!(izleyici.donduruldu);
        izleyici.son_veri = Instant::now() - Duration::from_secs(3);
        izleyici.sessizligi_denetle();
        assert_eq!(izleyici.kayit_sayisi(), 3);
        assert_eq!(izleyici.depo.dizin().kayitlar(), [(0, 0), (2, 0)]);
        izleyici.ciz().ok().unwrap();
        let son = yazilan.metin();
        let son = son.rsplit_once(AYIRICI).map_or(&son[..], |(_, son)| son);
        assert!(
            son.contains(" · 3 kayıt · son kayıt 2026-10-24 10:00:02 · döndürüldü\n"),
            "{son}"
        );
        assert!(
            son.contains("Son eski hata") && son.contains("Yeni hata"),
            "{son}"
        );
        fs::remove_file(&yol).ok();
        fs::remove_file(&yedek).ok();
    }

    #[test]
    fn kayan_pencere_her_cizimde_en_yeni_kayda_gore() {
        let secenekler = secenekler(&["--izle", "--son", "1sa", "deneme.log"]);
        let yazilan = Yazilan::default();
        let mut izleyici = izleyici(&secenekler, &yazilan);
        izleyici.isle(
            "[2026-10-24 10:00:00] production.ERROR: eski hata\n\
             [2026-10-24 10:30:00] production.ERROR: orta hata\n"
                .as_bytes(),
        );
        izleyici.bitir();
        izleyici.ciz().ok().unwrap();
        let ilk = yazilan.metin();
        assert!(
            ilk.contains("2026-10-24 09:30:00 ve sonrası (son 1 sa): 2 kayıt"),
            "{ilk}"
        );
        assert!(ilk.contains("eski hata"), "{ilk}");
        izleyici.isle("[2026-10-24 11:15:00] production.ERROR: yeni hata\n".as_bytes());
        izleyici.bitir();
        izleyici.ciz().ok().unwrap();
        let ikinci = &yazilan.metin()[ilk.len()..];
        // 10:15'ten eski olan ilk kayıt pencerenin dışında; depo onu hâlâ tutar
        assert!(
            ikinci.contains("2026-10-24 10:15:00 ve sonrası (son 1 sa): 2 kayıt"),
            "{ikinci}"
        );
        assert!(!ikinci.contains("eski hata"), "{ikinci}");
        assert!(
            ikinci.contains("orta hata") && ikinci.contains("yeni hata"),
            "{ikinci}"
        );
        assert!(ikinci.contains("· 3 kayıt ·"), "{ikinci}");
    }

    #[test]
    fn pencere_seviyeye_uymayan_kayitlarla_da_kayar() {
        // Toplu okumadaki gibi: `--son` logun en yeni kaydına göredir, seviyeye uymasa da
        let secenekler = secenekler(&["--izle", "--seviye", "error", "--son", "1sa", "deneme.log"]);
        let yazilan = Yazilan::default();
        let mut izleyici = izleyici(&secenekler, &yazilan);
        izleyici.isle(
            "[2026-10-24 10:00:00] local.ERROR: Odeme alinamadi\n\
             [2026-10-24 10:30:00] local.ERROR: Odeme alinamadi\n\
             [2026-10-24 12:00:00] local.INFO: Kullanici girdi\n\
             [2026-10-24 15:00:00] local.INFO: Kullanici girdi\n"
                .as_bytes(),
        );
        izleyici.bitir();
        izleyici.ciz().ok().unwrap();
        let metin = yazilan.metin();
        assert!(
            metin.contains("ERROR ve üstü, 2026-10-24 14:00:00 ve sonrası (son 1 sa): 0 kayıt"),
            "{metin}"
        );
        // Durum satırındaki son kayıt da logun son kaydı
        assert!(metin.contains("· son kayıt 2026-10-24 15:00:00"), "{metin}");
    }

    #[test]
    fn izlemede_bosalan_aralik_hata_degildir() {
        let secenekler = secenekler(&[
            "--izle",
            "--baslangic",
            "09:00",
            "--bitis",
            "2026-10-25",
            "deneme.log",
        ]);
        let yazilan = Yazilan::default();
        let mut izleyici = izleyici(&secenekler, &yazilan);
        // Kayıt yokken sona göre sınır çözülmez: 1970'e göre bir tarih de yazılmaz
        izleyici.ciz().ok().unwrap();
        let ilk = yazilan.metin();
        assert!(ilk.contains("2026-10-25 00:00:00 öncesi: 0 kayıt"), "{ilk}");
        assert!(!ilk.contains("1970") && !ilk.contains("1969"), "{ilk}");
        izleyici.isle("[2026-10-24 10:00:00] local.ERROR: dun\n".as_bytes());
        izleyici.bitir();
        izleyici.ciz().ok().unwrap();
        // Log 25 Ekim'e geçti: başlangıç (25 Ekim 09:00) bitişten sonra, pencere boş
        izleyici.isle("[2026-10-25 10:00:00] local.ERROR: bugun\n".as_bytes());
        izleyici.bitir();
        assert!(izleyici.ciz().is_ok());
        let son = &yazilan.metin()[ilk.len()..];
        assert!(
            son.contains("2026-10-25 09:00:00 ile 2026-10-25 00:00:00 arası: 0 kayıt"),
            "{son}"
        );
    }

    #[test]
    fn penceredeki_saat_dilimi_cumlesi_okunan_kayitlara_gore() {
        // Kayıtların hiçbirinde dilim yazılmamış: pencerede de "yazılı değil" denir, okunan
        // bütün kayıtların sayısı pencerenin kayıtlarınınkiyle karışmaz
        let secenekler = secenekler(&[
            "--izle",
            "--son",
            "1sa",
            "--saat-dilimi",
            "+03:00",
            "deneme.log",
        ]);
        let yazilan = Yazilan::default();
        let mut izleyici = izleyici(&secenekler, &yazilan);
        let log: String = (0..60)
            .map(|dakika| {
                format!(
                    "[2026-10-24 {:02}:{:02}:00] local.ERROR: Hata\n",
                    8 + dakika / 6,
                    dakika % 6 * 10
                )
            })
            .collect();
        izleyici.isle(log.as_bytes());
        izleyici.bitir();
        izleyici.ciz().ok().unwrap();
        let metin = yazilan.metin();
        assert!(
            metin.contains("kayıtlarda saat dilimi yazılı değil, zamanlar logdaki gibi"),
            "{metin}"
        );
    }

    #[test]
    fn izlemede_seviyesi_taninmayan_satirlar_uyarilir() {
        let secenekler = secenekler(&[
            "--izle",
            "--desen",
            r"(?P<zaman>\S+ \S+) \[(?P<seviye>\w+)\] (?P<mesaj>.*)",
            "deneme.log",
        ]);
        let yazilan = Yazilan::default();
        let mut izleyici = izleyici(&secenekler, &yazilan);
        izleyici.isle(b"2026-10-24 10:00:00 [ERROR] Bir\n2026-10-24 10:00:01 [XYZ] Iki\n");
        izleyici.bitir();
        izleyici.ciz().ok().unwrap();
        assert_eq!(izleyici.kayit_sayisi(), 1);
        assert_eq!(izleyici.uyarilar.len(), 1, "{:?}", izleyici.uyarilar);
        assert!(
            izleyici.uyarilar[0].starts_with(
                "Uyarı: deneme.log içinde --desen'e uyan 1 satırın seviyesi tanınmadı"
            ),
            "{:?}",
            izleyici.uyarilar
        );
        // Uyarı bir kez yazılır
        izleyici.isle(b"2026-10-24 10:00:02 [XYZ] Uc\n");
        izleyici.bitir();
        izleyici.ciz().ok().unwrap();
        assert_eq!(izleyici.uyarilar.len(), 1);
    }

    #[test]
    fn standart_girdinin_kopyasi_sinirda_birakilir() {
        // Standart girdi ve ani artışlar: örnekler için girdinin geçici bir kopyası tutulur
        let secenekler = secenekler(&["--izle", "--ani-artis", "1", "-"]);
        let yazilan = Yazilan::default();
        let mut izleyici = izleyici(&secenekler, &yazilan);
        assert!(matches!(izleyici.yeniden, Yeniden::Kopya(_)));
        let parca = KAYIT.len() as u64;
        izleyici.kopya_siniri = 2 * parca;
        izleyici.isle(KAYIT.as_bytes());
        izleyici.isle(KAYIT.as_bytes());
        // Sınıra kadar yazılır
        assert!(matches!(izleyici.yeniden, Yeniden::Kopya(_)));
        assert_eq!(izleyici.kopyalanan, 2 * parca);
        assert!(izleyici.uyarilar.is_empty());
        // Sınırı aşacak parça kopyayı bıraktırır; uyarı bir kez yazılır ve çizimde de görünür
        izleyici.isle(KAYIT.as_bytes());
        assert!(matches!(izleyici.yeniden, Yeniden::Yok));
        izleyici.isle(KAYIT.as_bytes());
        assert_eq!(izleyici.uyarilar.len(), 1);
        assert!(
            izleyici.uyarilar[0].starts_with("Uyarı: girdinin geçici kopyası 0 MB'ı aştı"),
            "{:?}",
            izleyici.uyarilar
        );
        // Kayıtlar okunmaya devam eder
        izleyici.bitir();
        assert_eq!(izleyici.kayit_sayisi(), 4);
    }

    #[test]
    fn ornekler_cizimler_arasinda_saklanir() {
        struct Sayan(usize);
        impl OrnekKaynagi for Sayan {
            fn mesajlar(
                &mut self,
                istekler: &[OrnekIstegi<'_>],
            ) -> io::Result<HashMap<usize, String>> {
                self.0 += istekler.len();
                Ok(istekler
                    .iter()
                    .map(|istek| (istek.sira, format!("kayıt {}", istek.sira)))
                    .collect())
            }
        }
        let istek = |sira| OrnekIstegi {
            sira,
            maskeli: "x",
            seviye: crate::kayit::Seviye::Error,
        };
        let mut ic = Sayan(0);
        let mut onceki = HashMap::new();
        for (cizim, siralar) in [[1, 2], [2, 3], [3, 4]].into_iter().enumerate() {
            let mut simdiki = HashMap::new();
            let mut kaynak = OnbellekliKaynak {
                ic: &mut ic,
                onceki: &onceki,
                simdiki: &mut simdiki,
            };
            let istekler: Vec<OrnekIstegi<'_>> = siralar.iter().map(|&sira| istek(sira)).collect();
            let bulunanlar = kaynak.mesajlar(&istekler).unwrap();
            assert_eq!(bulunanlar.len(), 2);
            assert_eq!(bulunanlar[&siralar[1]], format!("kayıt {}", siralar[1]));
            // İlk çizimde ikisi de okunur, sonrakilerde yalnızca yenisi
            assert_eq!(ic.0, 2 + cizim);
            // Bu çizimde istenmeyenler unutulur
            assert_eq!(simdiki.len(), 2);
            onceki = simdiki;
        }
    }

    #[test]
    fn pencere_kaynagi_siralari_cevirir() {
        struct Sayan(Vec<usize>);
        impl OrnekKaynagi for Sayan {
            fn mesajlar(
                &mut self,
                istekler: &[OrnekIstegi<'_>],
            ) -> io::Result<HashMap<usize, String>> {
                self.0 = istekler.iter().map(|istek| istek.sira).collect();
                Ok(istekler
                    .iter()
                    .map(|istek| (istek.sira, format!("kayıt {}", istek.sira)))
                    .collect())
            }
        }
        let mut ic = Sayan(Vec::new());
        let tam_sira = [5, 7, 9];
        let istek = |sira| OrnekIstegi {
            sira,
            maskeli: "m",
            seviye: crate::kayit::Seviye::Error,
        };
        let mut sinirli = SinirliKaynak {
            ic: &mut ic,
            ilk: 7,
        };
        let mut kaynak = CevrilmisKaynak {
            ic: &mut sinirli,
            tam_sira: &tam_sira,
        };
        let bulunan = kaynak.mesajlar(&[istek(0), istek(2)]).unwrap();
        // Pencerenin 0. kaydı deponun 5. kaydıdır: döndürmeden önce, istenmez
        assert_eq!(ic.0, vec![9]);
        assert_eq!(bulunan, HashMap::from([(2, "kayıt 9".to_owned())]));
    }
}
