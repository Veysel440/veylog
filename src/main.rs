//! veylog: Laravel log dosyalarını özetleyen komut satırı aracı.

mod drain;
mod istatistik;
mod json;
mod karsilastirma;
mod kayit;
mod komut_satiri;
mod markdown;
mod maske;
mod parmak_izi;
mod rapor;
mod zaman;

use std::borrow::Cow;
use std::env;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::process;

use kayit::kayitlari_ayir;
use komut_satiri::{Bicim, KULLANIM, Komut};
use rapor::{Ozet, gorunur};

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
    let yol = &secenekler.dosya;
    // Yolun mesajlarda gösterilecek hali: geçersiz UTF-8 baytları �, kontrol karakterleri
    // sembol olur (Linux'ta bir dosya adı ESC de içerebilir)
    let yol_metni = yol.display().to_string();
    let gosterilen_yol = gorunur(&yol_metni);

    let baytlar = match fs::read(yol) {
        Ok(baytlar) => baytlar,
        Err(hata) => {
            eprintln!("{gosterilen_yol} okunamadı: {hata}");
            process::exit(1);
        }
    };

    // Geçersiz UTF-8 baytları dosyanın tamamını reddetmek yerine � ile değiştirilir.
    // Dosya zaten geçerliyse hiçbir kopya oluşmaz (Cow::Borrowed).
    let icerik = String::from_utf8_lossy(&baytlar);
    if let Cow::Owned(_) = icerik {
        eprintln!(
            "Uyarı: {gosterilen_yol} geçersiz UTF-8 baytları içeriyor; bunlar � ile değiştirildi."
        );
    }

    let log = kayitlari_ayir(&icerik);
    // Özet bir kez hesaplanır; biçim yalnızca nasıl yazılacağını belirler. Karşılaştırma
    // noktası logun dışındaysa bu, seçeneğin bu dosyaya uymadığı bir kullanım hatasıdır.
    let ozet = match Ozet::hesapla(yol, &icerik, &log, &secenekler.rapor) {
        Ok(ozet) => ozet,
        Err(hata) => {
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

/// Çıktı yazılamadıysa nedenini söyler ve 1 koduyla çıkar.
///
/// Çıktıyı okuyan program erken kapanırsa (ör. `veylog laravel.log | head`) yazma BrokenPipe
/// hatası verir. Bu bir hata değil, okuyanın işi bitmiştir: sessizce çıkılır.
fn yazma_sonucunu_denetle(sonuc: io::Result<()>) {
    if let Err(hata) = sonuc
        && hata.kind() != io::ErrorKind::BrokenPipe
    {
        eprintln!("Çıktı yazılamadı: {hata}");
        process::exit(1);
    }
}
