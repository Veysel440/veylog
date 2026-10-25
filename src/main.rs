//! veylog: Laravel log dosyalarını özetleyen komut satırı aracı.

mod drain;
mod kayit;
mod maske;

use std::cmp::Reverse;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::process;

use drain::{Ayarlar, Drain, Kume};
use kayit::kayitlari_ayir;
use maske::maskele;

/// Listede gösterilecek en fazla kalıp sayısı.
const GOSTERILECEK_KALIP: usize = 10;
/// Uzun kalıplar bu kadar karakterden sonra kesilir.
const KALIP_GENISLIGI: usize = 90;

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

    // Her mesajı maskele ve Drain'e ver. Karşılaştırma için farklı mesajları da say.
    let mut drain = Drain::new(Ayarlar::default());
    let mut maskeli_mesajlar = BTreeSet::new();
    for kayit in &log.kayitlar {
        let maskeli = maskele(kayit.mesaj);
        drain.ekle(&maskeli);
        maskeli_mesajlar.insert(maskeli);
    }
    let ham_mesajlar: BTreeSet<&str> = log.kayitlar.iter().map(|kayit| kayit.mesaj).collect();

    // En kalabalık kalıp başa. Sıralama kararlıdır: adedi eşit olanlar ilk görüldükleri
    // sırada kalır.
    let mut kaliplar: Vec<&Kume> = drain.kumeler().iter().collect();
    kaliplar.sort_by_key(|kume| Reverse(kume.adet));

    println!();
    println!(
        "Kalıplar: {} farklı mesaj → maskeleyince {} → Drain ile {} kalıp",
        ham_mesajlar.len(),
        maskeli_mesajlar.len(),
        kaliplar.len()
    );
    for kume in kaliplar.iter().take(GOSTERILECEK_KALIP) {
        println!(
            "  {:>4}  {}",
            kume.adet,
            kisalt(&kume.metin(), KALIP_GENISLIGI)
        );
    }
}

/// Metni en fazla `sinir` karaktere indirir; kısaltırsa sonuna "…" koyar.
///
/// Bayt değil karakter sayılır: "ş" gibi Türkçe harfler UTF-8'de 2 bayttır ve bir harfi
/// ortasından kesmeye çalışmak programı çökertir.
fn kisalt(metin: &str, sinir: usize) -> String {
    if metin.chars().count() <= sinir {
        return metin.to_string();
    }
    let kisa: String = metin.chars().take(sinir - 1).collect();
    format!("{kisa}…")
}

#[cfg(test)]
mod testler {
    use super::*;

    #[test]
    fn kisa_metin_oldugu_gibi_kalir() {
        assert_eq!(kisalt("Şüpheli giriş", 20), "Şüpheli giriş");
    }

    #[test]
    fn uzun_metin_karakterden_kesilir() {
        // 5 karakter: 4 harf + "…". "Ş" ve "ü" 2'şer bayt olsa da harf bölünmez.
        assert_eq!(kisalt("Şüpheli giriş", 5), "Şüph…");
    }
}
