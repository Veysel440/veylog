//! veylog: Laravel log dosyalarını özetleyen komut satırı aracı.

mod kayit;
mod maske;

use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::process;

use kayit::kayitlari_ayir;
use maske::maskele;

/// Listede gösterilecek en fazla mesaj sayısı.
const GOSTERILECEK_MESAJ: usize = 10;
/// Uzun mesajlar bu kadar karakterden sonra kesilir.
const MESAJ_GENISLIGI: usize = 90;

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

    // Aynı maskelenmiş mesaja sahip kayıtları say. BTreeMap mesajları alfabetik tutar.
    let mut mesaj_sayilari: BTreeMap<Cow<str>, usize> = BTreeMap::new();
    for kayit in &log.kayitlar {
        *mesaj_sayilari.entry(maskele(kayit.mesaj)).or_insert(0) += 1;
    }
    let ham_mesajlar: BTreeSet<&str> = log.kayitlar.iter().map(|kayit| kayit.mesaj).collect();

    // En sık olan başa. Reverse sırayı ters çevirir (çoktan aza). Sıralama kararlıdır:
    // adedi eşit olanlar BTreeMap'ten geldikleri alfabetik sırada kalır.
    let mut en_sik: Vec<(Cow<str>, usize)> = mesaj_sayilari.into_iter().collect();
    en_sik.sort_by_key(|(_, adet)| Reverse(*adet));

    println!();
    println!(
        "Mesajlar: {} farklı mesaj, maskeleyince {}",
        ham_mesajlar.len(),
        en_sik.len()
    );
    for (mesaj, adet) in en_sik.iter().take(GOSTERILECEK_MESAJ) {
        println!("  {adet:>4}  {}", kisalt(mesaj, MESAJ_GENISLIGI));
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
