//! veylog: Laravel log dosyalarını özetleyen komut satırı aracı.

mod ani_artis;
mod depo;
mod drain;
mod farkli_sayac;
mod istatistik;
mod json;
mod json_okuyucu;
mod karsilastirma;
mod kayit;
mod komut_satiri;
mod markdown;
mod maske;
mod okuyucu;
mod parmak_izi;
mod rapor;
mod sutunlar;
mod zaman;

use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use depo::Depo;
use komut_satiri::{Bicim, KULLANIM, Komut, Secenekler};
use okuyucu::{Kopyalayan, OrnekIstegi, OrnekKaynagi, YenidenOkuyucu};
use rapor::{Ozet, OzetHatasi, gorunur};

/// Dosya adı yerine `-` verilince raporda gösterilen ad.
const STANDART_GIRDI: &str = "<standart girdi>";

/// Okuma tamponunun boyu. Satırlar tampondan kopyalanmadan bulunur; büyük bir tampon daha az
/// sistem çağrısı demektir.
const TAMPON: usize = 1 << 16;

fn main() {
    // `args_os`: argümanları işletim sisteminin verdiği ham haliyle alır. `args` ise geçerli
    // UTF-8 olmayan bir argümanda (Linux'ta dosya adları herhangi bir bayt dizisi olabilir)
    // programı çökertir. `skip(1)`: ilk argüman programın kendi adıdır.
    let secenekler = match komut_satiri::ayristir(env::args_os().skip(1)) {
        Ok(Komut::Ozet(secenekler)) => secenekler,
        Ok(Komut::Yardim) => {
            yazma_sonucunu_denetle(io::stdout().write_all(komut_satiri::yardim_metni().as_bytes()));
            return;
        }
        Ok(Komut::Surum) => {
            yazma_sonucunu_denetle(writeln!(
                io::stdout(),
                "veylog {}",
                env!("CARGO_PKG_VERSION")
            ));
            return;
        }
        Err(hata) => {
            // Kullanım hataları 2 koduyla biter; dosya ve çıktı hataları 1 ile. Böylece onu
            // çağıran bir betik hatanın türünü ayırt edebilir. Mesaj kullanıcının yazdığı
            // argümanları içerebilir; içlerindeki kontrol karakterleri terminale ham gitmesin.
            eprintln!("veylog: {}", gorunur(&hata.to_string()));
            eprintln!("{KULLANIM}");
            eprintln!("Ayrıntılar için: veylog --yardim");
            process::exit(2);
        }
    };
    // `-` standart girdidir: `zcat laravel-2026-10-24.log.gz | veylog -`
    let standart_girdi = secenekler.dosya.as_os_str() == "-";
    let yol = if standart_girdi {
        Path::new(STANDART_GIRDI)
    } else {
        secenekler.dosya.as_path()
    };
    // Yolun mesajlarda gösterilecek hali: geçersiz UTF-8 baytları �, kontrol karakterleri
    // sembol olur (Linux'ta bir dosya adı ESC de içerebilir)
    let yol_metni = yol.display().to_string();
    let gosterilen_yol = gorunur(&yol_metni);

    let ozet = match ozet_cikar(&secenekler, standart_girdi, &yol_metni, &gosterilen_yol) {
        Ok(ozet) => ozet,
        Err(Hata::Okuma(hata)) => {
            eprintln!(
                "veylog: {gosterilen_yol} okunamadı: {}",
                okuma_hatasi(&hata)
            );
            process::exit(1);
        }
        Err(Hata::Kopya(hata)) => {
            eprintln!("veylog: girdi geçici bir dosyaya kopyalanamadı: {hata}");
            process::exit(1);
        }
        Err(Hata::Ozet(OzetHatasi::Okuma(hata))) => {
            eprintln!(
                "veylog: {gosterilen_yol} yeniden okunamadı: {}",
                okuma_hatasi(&hata)
            );
            process::exit(1);
        }
        // Karşılaştırma noktası logun dışındaysa bu, seçeneğin bu dosyaya uymadığı bir
        // kullanım hatasıdır
        Err(Hata::Ozet(hata @ OzetHatasi::Karsilastirma(_))) => {
            eprintln!("veylog: {hata}");
            process::exit(2);
        }
    };

    let mut cikti = BufWriter::new(io::stdout().lock());
    let yazildi = match secenekler.bicim {
        Bicim::Metin => rapor::metin_yaz(&mut cikti, &ozet),
        Bicim::Json => json::yaz(&mut cikti, &ozet),
        Bicim::Markdown => markdown::yaz(&mut cikti, &ozet),
    };
    // Tamponda kalan son satırlar `flush` ile yazılır; varsa hatası da burada ortaya çıkar.
    // `and_then`: yazma başarılıysa flush'ı da dene, ilk hatayı döndür.
    yazma_sonucunu_denetle(yazildi.and_then(|()| cikti.flush()));
}

/// Özet çıkarılırken olabilecek hatalar.
enum Hata {
    /// Girdi okunamadı.
    Okuma(io::Error),
    /// Standart girdinin geçici kopyası yazılamadı.
    Kopya(io::Error),
    Ozet(OzetHatasi),
}

/// Girdiyi okur ve özetler. Okunan satırlar bırakılır; bellekte depo kalır.
///
/// Ani artışların ve bazı kalıpların örnekleri için girdinin bir kısmı yeniden okunabilir.
/// Dosya ise aynı dosya yeniden okunur. Standart girdi (ya da `<(zcat ...)` gibi bir boru)
/// yalnızca bir kez okunabildiği için `--ani-artis` verildiyse okunurken geçici bir dosyaya
/// kopyalanır. Geçici dosya oluşturulamazsa girdi kopyalanmadan okunur: yeniden okunamayan
/// kayıtların yerine aynı alt gruptaki ilk kaydın mesajı örnek olur.
fn ozet_cikar(
    secenekler: &Secenekler,
    standart_girdi: bool,
    yol_metni: &str,
    gosterilen_yol: &str,
) -> Result<Ozet, Hata> {
    let mut depo = Depo::new(secenekler.rapor.drain.clone());
    let yeniden_okunacak = secenekler.rapor.ani_artis.is_some();
    let dosya = if standart_girdi {
        None
    } else {
        Some(dosyayi_ac(&secenekler.dosya).map_err(Hata::Okuma)?)
    };
    // Normal bir dosya değilse (boru, aygıt) standart girdi gibi bir kez okunur
    let dosya = match dosya {
        Some(dosya) if dosya.metadata().is_ok_and(|bilgi| bilgi.is_file()) => {
            let mut girdi = BufReader::with_capacity(TAMPON, dosya);
            let sonuc = okuyucu::oku(&mut girdi, &mut depo).map_err(Hata::Okuma)?;
            depo.okuma_bitti(sonuc);
            Ok(girdi)
        }
        akis => Err(akis),
    };
    let mut kopya = None;
    if let Err(akis) = &dosya {
        let ic: Box<dyn io::BufRead + '_> = match akis {
            Some(akis) => Box::new(BufReader::with_capacity(TAMPON, akis)),
            None => Box::new(io::stdin().lock()),
        };
        let gecici = if yeniden_okunacak {
            GeciciDosya::olustur()
                .inspect_err(|hata| {
                    eprintln!(
                        "Uyarı: girdinin geçici kopyası oluşturulamadı ({hata}); girdi yeniden okunamayacak, bazı örnekler aynı kalıpta aynı seviyedeki ilk kaydın mesajı olacak."
                    );
                })
                .ok()
        } else {
            None
        };
        let sonuc = if let Some(gecici) = gecici {
            let mut kopyalayan =
                Kopyalayan::new(ic, BufWriter::with_capacity(TAMPON, gecici.dosya()));
            let sonuc = okuyucu::oku(&mut kopyalayan, &mut depo).map_err(Hata::Okuma)?;
            kopyalayan
                .bitir()
                .and_then(|mut yazici| yazici.flush())
                .map_err(Hata::Kopya)?;
            kopya = Some(gecici);
            sonuc
        } else {
            okuyucu::oku(ic, &mut depo).map_err(Hata::Okuma)?
        };
        depo.okuma_bitti(sonuc);
    }
    if depo.gecersiz_utf8() {
        eprintln!(
            "Uyarı: {gosterilen_yol} geçersiz UTF-8 baytları içeriyor; bunlar � ile değiştirildi."
        );
    }

    let dizin = depo.dizin().to_vec();
    let mut kaynak: Box<dyn OrnekKaynagi + '_> = match (dosya, &kopya) {
        (Ok(girdi), _) => Box::new(YenidenOkuyucu::new(girdi, dizin)),
        (Err(_), Some(gecici)) => {
            let mut dosya = gecici.dosya();
            dosya.seek(SeekFrom::Start(0)).map_err(Hata::Kopya)?;
            Box::new(YenidenOkuyucu::new(
                BufReader::with_capacity(TAMPON, dosya),
                dizin,
            ))
        }
        (Err(_), None) => Box::new(OkunamazKaynak),
    };
    let ozet = Ozet::hesapla(
        yol_metni.to_owned(),
        &mut depo,
        &secenekler.rapor,
        kaynak.as_mut(),
    )
    .map_err(Hata::Ozet)?;
    if kaynak.degisti() {
        eprintln!(
            "Uyarı: {gosterilen_yol} özetlenirken değişti (ör. log döndürüldü); yeniden okunamayan örneklerin yerine aynı kalıpta aynı seviyedeki ilk kaydın mesajı yazıldı."
        );
    }
    Ok(ozet)
}

/// Log dosyasını açar. Bir klasör verildiyse açmaya çalışmaz: Linux'ta bunun hatası "Is a
/// directory", Windows'ta "Access is denied" olurdu; ikincisi yanıltıcıdır.
fn dosyayi_ac(yol: &Path) -> io::Result<File> {
    if yol.is_dir() {
        return Err(io::Error::from(io::ErrorKind::IsADirectory));
    }
    File::open(yol)
}

/// Yeniden okunamayan bir girdinin örnek kaynağı: hiçbir kaydı bulamaz. Standart girdi ani
/// artış istenmediyse (ya da geçici dosya oluşturulamadıysa) böyle kalır; o zaman örnekler
/// depodakilerden seçilir.
struct OkunamazKaynak;

impl OrnekKaynagi for OkunamazKaynak {
    fn mesajlar(&mut self, _: &[OrnekIstegi<'_>]) -> io::Result<HashMap<usize, String>> {
        Ok(HashMap::new())
    }
}

/// Standart girdinin geçici kopyası. Dosya oluşturulur oluşturulmaz silinir: açık kaldıkça
/// okunup yazılabilir, program nasıl biterse bitsin (Ctrl+C dahil) diskte kalmaz. Silinemezse
/// (bazı Windows kurulumları açık dosyanın silinmesine izin vermez) program bitince silinir.
struct GeciciDosya {
    yol: PathBuf,
    dosya: Option<File>,
    silindi: bool,
}

impl GeciciDosya {
    fn olustur() -> io::Result<GeciciDosya> {
        let klasor = env::temp_dir();
        // Ad, başka bir veylog'unkiyle çakışmasın: süreç numarası ve an
        let an = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |sure| sure.as_nanos());
        for deneme in 0..100 {
            let yol = klasor.join(format!("veylog-{}-{an}-{deneme}.tmp", process::id()));
            let mut secenekler = OpenOptions::new();
            secenekler.read(true).write(true).create_new(true);
            // Unix'te yalnızca sahibi okuyup yazabilsin: dosya silinene kadar geçen kısa sürede
            // bile başka bir kullanıcı açıp girdiyi okuyamasın. Windows'ta geçici klasör zaten
            // kullanıcıya özeldir.
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut secenekler, 0o600);
            match secenekler.open(&yol) {
                Ok(dosya) => {
                    let silindi = fs::remove_file(&yol).is_ok();
                    return Ok(GeciciDosya {
                        yol,
                        dosya: Some(dosya),
                        silindi,
                    });
                }
                Err(hata) if hata.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(hata) => return Err(hata),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "geçici dosya için boş bir ad bulunamadı",
        ))
    }

    /// Dosyanın kendisi. `&File` de okunup yazılabilir ve konumlanabilir: hepsi aynı dosyadır.
    fn dosya(&self) -> &File {
        self.dosya
            .as_ref()
            .expect("dosya ancak düşürülürken kapanır")
    }
}

impl Drop for GeciciDosya {
    fn drop(&mut self) {
        // Önce kapat: Windows açık bir dosyayı silmeyebilir
        drop(self.dosya.take());
        if !self.silindi {
            let _ = fs::remove_file(&self.yol);
        }
    }
}

/// Okuma hatasının açıklaması: sık görülenler Türkçe, ötekiler işletim sisteminin mesajıyla.
fn okuma_hatasi(hata: &io::Error) -> Cow<'static, str> {
    match hata.kind() {
        io::ErrorKind::NotFound => "dosya bulunamadı".into(),
        io::ErrorKind::PermissionDenied => "okuma izni yok".into(),
        io::ErrorKind::IsADirectory => {
            "bu bir klasör; log dosyasını verin (ör. storage/logs/laravel.log)".into()
        }
        _ => hata.to_string().into(),
    }
}

/// Çıktı yazılamadıysa nedenini söyler ve 1 koduyla çıkar.
///
/// Çıktıyı okuyan program erken kapanırsa (ör. `veylog laravel.log | head`) yazma BrokenPipe
/// hatası verir. Bu bir hata değil, okuyanın işi bitmiştir: sessizce çıkılır.
fn yazma_sonucunu_denetle(sonuc: io::Result<()>) {
    if let Err(hata) = sonuc
        && hata.kind() != io::ErrorKind::BrokenPipe
    {
        eprintln!("veylog: çıktı yazılamadı: {hata}");
        process::exit(1);
    }
}

#[cfg(all(test, unix))]
mod testler {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn gecici_dosyayi_yalnizca_sahibi_okuyup_yazar() {
        let gecici = GeciciDosya::olustur().unwrap();
        // Dosya silindi ama açık; izinleri açık dosyadan okunur
        let izin = gecici.dosya().metadata().unwrap().permissions().mode();
        assert_eq!(izin & 0o777, 0o600);
        assert!(gecici.silindi);
        assert!(!gecici.yol.exists());
    }
}
