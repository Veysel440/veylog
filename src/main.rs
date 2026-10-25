//! veylog: Laravel log dosyalarını özetleyen komut satırı aracı.

mod kayit;

use std::env;
use std::fs;
use std::process;

use kayit::kayitlari_ayir;

fn main() {
    let argumanlar: Vec<String> = env::args().collect();

    if argumanlar.len() < 2 {
        eprintln!("Kullanım: veylog <log-dosyası>");
        process::exit(1);
    }
    let yol = &argumanlar[1];

    let icerik = match fs::read_to_string(yol) {
        Ok(metin) => metin,
        Err(hata) => {
            eprintln!("{yol} okunamadı: {hata}");
            process::exit(1);
        }
    };

    let log = kayitlari_ayir(&icerik);

    println!("{yol}: {} satır", icerik.lines().count());
    println!("  Kayıt         : {}", log.kayitlar.len());
    println!("  Devam satırı  : {}", log.devam_satiri_sayisi());
    println!("  Yetim satır   : {}", log.yetim_satir);
    if let Some((en_eski, en_yeni)) = log.zaman_araligi() {
        println!("  Zaman aralığı : {en_eski} → {en_yeni}");
    }

    println!();
    println!("Seviyeler:");
    // En önemli seviye en üstte görünsün diye ters sırada yaz
    for (seviye, adet) in log.seviye_sayilari().iter().rev() {
        println!("  {:<10}{adet:>4}", seviye.ad());
    }
}
