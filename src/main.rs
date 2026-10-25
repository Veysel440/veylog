use std::env;
use std::fs;
use std::process;

fn main() {
    // Komut satırı argümanları: [0] programın adı, [1] bizim verdiğimiz dosya yolu
    let argumanlar: Vec<String> = env::args().collect();

    if argumanlar.len() < 2 {
        eprintln!("Kullanım: elek <log-dosyası>");
        process::exit(1);
    }
    let yol = &argumanlar[1];

    // Dosyayı oku; okunamazsa hatayı yaz ve çık
    let icerik = match fs::read_to_string(yol) {
        Ok(metin) => metin,
        Err(hata) => {
            eprintln!("{yol} okunamadı: {hata}");
            process::exit(1);
        }
    };

    let satir_sayisi = icerik.lines().count();
    let kayit_sayisi = icerik.lines().filter(|satir| baslik_mi(satir)).count();

    println!("{yol}: {satir_sayisi} satır, {kayit_sayisi} kayıt");
}

/// Satır bir log kaydının başlangıcı mı? Örnek: "[2026-10-24 14:03:11] production.ERROR: ..."
fn baslik_mi(satir: &str) -> bool {
    let Some(geri) = satir.strip_prefix('[') else {
        return false;
    };
    let Some((zaman, _)) = geri.split_once("] ") else {
        return false;
    };
    zaman_gecerli_mi(zaman)
}

/// "2026-10-24 14:03:11" biçiminde mi? (19 karakter: rakamlar, -, boşluk ve :)
fn zaman_gecerli_mi(metin: &str) -> bool {
    let baytlar = metin.as_bytes();
    if baytlar.len() != 19 {
        return false;
    }
    baytlar.iter().enumerate().all(|(sira, &bayt)| match sira {
        4 | 7 => bayt == b'-',
        10 => bayt == b' ',
        13 | 16 => bayt == b':',
        _ => bayt.is_ascii_digit(),
    })
}

#[cfg(test)]
mod testler {
    use super::*;

    #[test]
    fn gercek_baslik_taninir() {
        assert!(baslik_mi("[2026-10-24 14:03:11] production.ERROR: x"));
    }

    #[test]
    fn koseli_parantezli_diger_satirlar_baslik_degildir() {
        assert!(!baslik_mi("[stacktrace]"));
        assert!(!baslik_mi("[previous exception] [object] (x)"));
        assert!(!baslik_mi("#0 /var/www/app/Kod.php(41)"));
        assert!(!baslik_mi(""));
    }

    #[test]
    fn bozuk_zaman_reddedilir() {
        assert!(!baslik_mi("[2026-10-24] local.INFO: x"));
        assert!(!baslik_mi("[2026/10/24 14:03:11] local.INFO: x"));
        assert!(!baslik_mi("[2026-10-24 14:03:1x] local.INFO: x"));
    }
}