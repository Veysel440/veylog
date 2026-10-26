//! veylog: Laravel log dosyalarını özetleyen komut satırı aracı.

mod drain;
mod kayit;
mod komut_satiri;
mod maske;
mod parmak_izi;
mod rapor;

use std::borrow::Cow;
use std::env;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::process;

use kayit::kayitlari_ayir;
use komut_satiri::{KULLANIM, Komut};
use rapor::ozet_yaz;

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
            // çağıran bir betik hatanın türünü ayırt edebilir.
            eprintln!("veylog: {hata}");
            eprintln!("{KULLANIM}");
            eprintln!("Ayrıntılar için: veylog --yardim");
            process::exit(2);
        }
    };
    let yol = &secenekler.dosya;

    let baytlar = match fs::read(yol) {
        Ok(baytlar) => baytlar,
        Err(hata) => {
            eprintln!("{} okunamadı: {hata}", yol.display());
            process::exit(1);
        }
    };

    // Geçersiz UTF-8 baytları dosyanın tamamını reddetmek yerine � ile değiştirilir.
    // Dosya zaten geçerliyse hiçbir kopya oluşmaz (Cow::Borrowed).
    let icerik = String::from_utf8_lossy(&baytlar);
    if let Cow::Owned(_) = icerik {
        eprintln!(
            "Uyarı: {} geçersiz UTF-8 baytları içeriyor; bunlar � ile değiştirildi.",
            yol.display()
        );
    }

    let log = kayitlari_ayir(&icerik);

    let mut cikti = BufWriter::new(io::stdout().lock());
    yazma_sonucunu_denetle(ozet_yaz(&mut cikti, yol, &icerik, &log, &secenekler.rapor));
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
