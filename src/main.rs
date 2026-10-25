//! veylog: Laravel log dosyalarını özetleyen komut satırı aracı.

mod kayit;

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::process;

use kayit::{Seviye, seviye_bul};

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

    // Her seviyeden kaç kayıt var? BTreeMap anahtarları sıralı tutar.
    let mut sayaclar: BTreeMap<Seviye, usize> = BTreeMap::new();
    for satir in icerik.lines() {
        if let Some(seviye) = seviye_bul(satir) {
            *sayaclar.entry(seviye).or_insert(0) += 1;
        }
    }

    let satir_sayisi = icerik.lines().count();
    let kayit_sayisi: usize = sayaclar.values().sum();

    println!("{yol}: {satir_sayisi} satır, {kayit_sayisi} kayıt");
    // En önemli seviye en üstte görünsün diye ters sırada yaz
    for (seviye, adet) in sayaclar.iter().rev() {
        println!("  {:<10}{adet:>4}", seviye.ad());
    }
}
