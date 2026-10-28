//! veylog: Laravel log dosyalarını özetleyen komut satırı aracı.

mod ani_artis;
mod depo;
mod drain;
mod farkli_sayac;
mod filtre;
mod istatistik;
mod istisna_metni;
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
mod satir_bicimi;
mod sutunlar;
mod zaman;
mod zaman_bicimi;

use std::borrow::Cow;
use std::collections::HashMap;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use depo::{Depo, DepoAyarlari};
use filtre::FiltreHatasi;
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
        // Karşılaştırma noktası logun dışındaysa ya da filtrenin aralığı boşsa bu, seçeneğin
        // bu dosyaya uymadığı bir kullanım hatasıdır
        Err(Hata::Ozet(hata @ OzetHatasi::Karsilastirma(_))) => {
            eprintln!("veylog: {hata}");
            process::exit(2);
        }
        Err(Hata::Filtre(hata)) => {
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
    /// Filtrenin aralığı boş.
    Filtre(FiltreHatasi),
    Ozet(OzetHatasi),
}

/// Girdiyi okur ve özetler. Okunan satırlar bırakılır; bellekte depo kalır.
///
/// Girdi en fazla iki kez okunur. Logun sonuna göre çözülen bir filtre (`--son 2sa`,
/// `--baslangic 15:00`) önce en yeni kaydı bilmeyi gerektirir: log bir kez taranır, sonra
/// filtreyle okunur. Ani artışların ve bazı kalıpların örnekleri için de girdinin bir kısmı
/// yeniden okunabilir. Dosya ise aynı dosya yeniden okunur. Standart girdi (ya da
/// `<(zcat ...)` gibi bir boru) yalnızca bir kez okunabildiği için o zaman ilk okunuşunda
/// geçici bir dosyaya kopyalanır. Ön tarama için kopya şarttır. Yalnızca ani artışlar için
/// gerekiyorsa ve geçici dosya oluşturulamazsa girdi kopyalanmadan okunur: yeniden okunamayan
/// kayıtların yerine aynı alt gruptaki ilk kaydın mesajı örnek olur.
fn ozet_cikar(
    secenekler: &Secenekler,
    standart_girdi: bool,
    yol_metni: &str,
    gosterilen_yol: &str,
) -> Result<Ozet, Hata> {
    let on_tarama = secenekler.rapor.filtre.sonu_gerekir();
    let dosya = if standart_girdi {
        None
    } else {
        Some(dosyayi_ac(&secenekler.dosya).map_err(Hata::Okuma)?)
    };
    let mut kopya = None;
    let mut depo;
    // Normal bir dosya değilse (boru, aygıt) standart girdi gibi bir kez okunur
    let girdi = match dosya {
        Some(dosya) if dosya.metadata().is_ok_and(|bilgi| bilgi.is_file()) => {
            let mut girdi = BufReader::with_capacity(TAMPON, dosya);
            let en_yeni = if on_tarama {
                let en_yeni = okuyucu::en_yeni_zaman(
                    &mut girdi,
                    &secenekler.rapor.satir_bicimi,
                    secenekler.rapor.saat_dilimi,
                )
                .map_err(Hata::Okuma)?;
                girdi.seek(SeekFrom::Start(0)).map_err(Hata::Okuma)?;
                en_yeni
            } else {
                None
            };
            depo = depoyu_doldur(secenekler, en_yeni, &mut girdi)?;
            Some(girdi)
        }
        akis => {
            let ic: Box<dyn io::BufRead + '_> = match &akis {
                Some(akis) => Box::new(BufReader::with_capacity(TAMPON, akis)),
                None => Box::new(io::stdin().lock()),
            };
            let gecici = if on_tarama {
                Some(GeciciDosya::olustur().map_err(Hata::Kopya)?)
            } else if secenekler.rapor.ani_artis.is_some() {
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
            match gecici {
                Some(gecici) => {
                    let mut kopyalayan =
                        Kopyalayan::new(ic, BufWriter::with_capacity(TAMPON, gecici.dosya()));
                    if on_tarama {
                        // Ön tarama girdiyi kopyalar; asıl okuma kopyadan yapılır
                        let en_yeni = okuyucu::en_yeni_zaman(
                            &mut kopyalayan,
                            &secenekler.rapor.satir_bicimi,
                            secenekler.rapor.saat_dilimi,
                        )
                        .map_err(Hata::Okuma)?;
                        kopyayi_bitir(kopyalayan)?;
                        let mut kopyasi = gecici.dosya();
                        kopyasi.seek(SeekFrom::Start(0)).map_err(Hata::Kopya)?;
                        depo = depoyu_doldur(
                            secenekler,
                            en_yeni,
                            BufReader::with_capacity(TAMPON, kopyasi),
                        )?;
                    } else {
                        depo = depoyu_doldur(secenekler, None, &mut kopyalayan)?;
                        kopyayi_bitir(kopyalayan)?;
                    }
                    kopya = Some(gecici);
                }
                None => depo = depoyu_doldur(secenekler, None, ic)?,
            }
            None
        }
    };
    if depo.gecersiz_utf8() {
        eprintln!(
            "Uyarı: {gosterilen_yol} geçersiz UTF-8 baytları içeriyor; bunlar � ile değiştirildi."
        );
    }
    // Satır var ama hiçbiri bir kayıt başlatmıyor: büyük olasılıkla biçim uymuyor
    let sayilar = depo.sayilar();
    if sayilar.kayit == 0 && sayilar.satir > 0 {
        let bicim = &secenekler.rapor.satir_bicimi;
        if bicim.desenli() {
            eprintln!(
                "Uyarı: {gosterilen_yol} içinde --desen'e uyan satır yok; hiçbir kayıt okunmadı."
            );
        } else if bicim.zaman_bicimli() {
            eprintln!(
                "Uyarı: {gosterilen_yol} içinde zamanı --zaman-bicimi'ne uyan kayıt yok; hiçbir kayıt okunmadı."
            );
        } else {
            eprintln!(
                "Uyarı: {gosterilen_yol} içinde Laravel'in biçiminde kayıt yok; log başka bir biçimdeyse --desen ile tanıtılabilir."
            );
        }
    }

    let dizin = depo.dizin().to_vec();
    let (filtre, dilim) = (depo.filtre().copied(), depo.saat_dilimi());
    let bicim = depo.satir_bicimi().clone();
    let mut kaynak: Box<dyn OrnekKaynagi + '_> = match (girdi, &kopya) {
        (Some(girdi), _) => {
            Box::new(YenidenOkuyucu::new(girdi, dizin, filtre, dilim).bicimle(bicim))
        }
        (None, Some(gecici)) => {
            let mut dosya = gecici.dosya();
            dosya.seek(SeekFrom::Start(0)).map_err(Hata::Kopya)?;
            Box::new(
                YenidenOkuyucu::new(
                    BufReader::with_capacity(TAMPON, dosya),
                    dizin,
                    filtre,
                    dilim,
                )
                .bicimle(bicim),
            )
        }
        (None, None) => Box::new(OkunamazKaynak),
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

/// Filtreyi logun sonuna göre çözer, girdiyi okuyup depoyu doldurur. `en_yeni`: ön taramada
/// bulunan en yeni kaydın zamanı; filtre logun sonunu gerektirmiyorsa None.
fn depoyu_doldur(
    secenekler: &Secenekler,
    en_yeni: Option<zaman::Zaman>,
    girdi: impl io::BufRead,
) -> Result<Depo, Hata> {
    let rapor = &secenekler.rapor;
    let filtre = if rapor.filtre.bos_mu() {
        None
    } else {
        Some(rapor.filtre.coz(en_yeni).map_err(Hata::Filtre)?)
    };
    let ayarlar = DepoAyarlari {
        drain: rapor.drain.clone(),
        filtre,
        saat_dilimi: rapor.saat_dilimi,
        satir_bicimi: rapor.satir_bicimi.clone(),
        konum_turu: rapor.konum_turu,
    };
    Depo::doldur(ayarlar, girdi).map_err(Hata::Okuma)
}

/// Kopyalanan girdinin kopyasını diske yazar; kopyalanırken bir hata olduysa onu döndürür.
fn kopyayi_bitir<R: io::BufRead, W: Write>(kopyalayan: Kopyalayan<R, W>) -> Result<(), Hata> {
    kopyalayan
        .bitir()
        .and_then(|mut yazici| yazici.flush())
        .map_err(Hata::Kopya)
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
