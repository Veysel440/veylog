//! veylog: Laravel log dosyalarını özetleyen komut satırı aracı.

mod drain;
mod kayit;
mod maske;
mod rapor;

use std::borrow::Cow;
use std::env;
use std::fs;
use std::io::{self, BufWriter};
use std::process;

use kayit::kayitlari_ayir;
use rapor::ozet_yaz;

fn main() {
    let argumanlar: Vec<String> = env::args().collect();
    let Some(yol) = argumanlar.get(1) else {
        eprintln!("Kullanım: veylog <log-dosyası>");
        process::exit(1);
    };

    let baytlar = match fs::read(yol) {
        Ok(baytlar) => baytlar,
        Err(hata) => {
            eprintln!("{yol} okunamadı: {hata}");
            process::exit(1);
        }
    };

    // Geçersiz UTF-8 baytları dosyanın tamamını reddetmek yerine � ile değiştirilir.
    // Dosya zaten geçerliyse hiçbir kopya oluşmaz (Cow::Borrowed).
    let icerik = String::from_utf8_lossy(&baytlar);
    if let Cow::Owned(_) = icerik {
        eprintln!("Uyarı: {yol} geçersiz UTF-8 baytları içeriyor; bunlar � ile değiştirildi.");
    }

    let log = kayitlari_ayir(&icerik);

    // Çıktıyı okuyan program erken kapanırsa (ör. `veylog laravel.log | head`) yazma
    // BrokenPipe hatası verir. Bu bir hata değil, okuyanın işi bitmiştir: sessizce çıkılır.
    let mut cikti = BufWriter::new(io::stdout().lock());
    if let Err(hata) = ozet_yaz(&mut cikti, yol, &icerik, &log)
        && hata.kind() != io::ErrorKind::BrokenPipe
    {
        eprintln!("Çıktı yazılamadı: {hata}");
        process::exit(1);
    }
}
