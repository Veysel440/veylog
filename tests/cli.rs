//! Programı gerçekten çalıştıran testler: argümanlar, hata mesajları ve çıkış kodları.
//!
//! Cargo, derlenmiş programın yolunu `CARGO_BIN_EXE_veylog` ortam değişkeniyle verir.

use std::fs;
use std::path::PathBuf;
use std::process::{self, Command, Output};

fn veylog(argumanlar: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_veylog"))
        .args(argumanlar)
        .output()
        .expect("veylog çalıştırılamadı")
}

/// Geçici bir dosya yazar. Testler aynı anda çalıştığı için her testin dosya adı farklıdır.
fn gecici_dosya(ad: &str, icerik: &[u8]) -> PathBuf {
    let yol = std::env::temp_dir().join(format!("veylog-{}-{ad}", process::id()));
    fs::write(&yol, icerik).expect("geçici dosya yazılamadı");
    yol
}

fn metin(baytlar: &[u8]) -> String {
    String::from_utf8_lossy(baytlar).into_owned()
}

#[test]
fn arguman_yoksa_kullanim_gosterilir() {
    let cikti = veylog(&[]);
    // Kullanım hataları 2 koduyla biter; dosya hataları 1 ile
    assert_eq!(cikti.status.code(), Some(2));
    let stderr = metin(&cikti.stderr);
    assert!(stderr.contains("log dosyası verilmedi"), "{stderr}");
    assert!(stderr.contains("Kullanım: veylog"), "{stderr}");
}

#[test]
fn yardim_ve_surum_basariyla_gosterilir() {
    for secenek in ["--yardim", "-h", "--help", "--yardım"] {
        let cikti = veylog(&[secenek]);
        assert!(cikti.status.success(), "{secenek}");
        assert!(cikti.stderr.is_empty(), "{secenek}");
        let stdout = metin(&cikti.stdout);
        assert!(stdout.contains("Kullanım: veylog"), "{secenek}");
        assert!(stdout.contains("--siralama"), "{secenek}");
    }

    let cikti = veylog(&["--surum"]);
    assert!(cikti.status.success());
    assert_eq!(
        metin(&cikti.stdout),
        format!("veylog {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn gecersiz_secenek_kullanim_hatasi_verir() {
    let cikti = veylog(&["--esik", "1,5", "ornekler/laravel.log"]);
    assert_eq!(cikti.status.code(), Some(2));
    assert!(cikti.stdout.is_empty());
    let stderr = metin(&cikti.stderr);
    assert!(
        stderr.contains("veylog: --esik için geçersiz değer \"1,5\""),
        "{stderr}"
    );
    assert!(
        stderr.contains("Ayrıntılar için: veylog --yardim"),
        "{stderr}"
    );
}

#[test]
fn secenekler_raporu_degistirir() {
    // Testler paket kökünde çalıştığı için örnek loga göreli yolla ulaşılır
    let cikti = veylog(&[
        "--esik",
        "0,8",
        "-n",
        "3",
        "--siralama",
        "adet",
        "ornekler/laravel.log",
    ]);
    let stdout = metin(&cikti.stdout);
    assert!(cikti.status.success(), "{stdout}");
    assert!(
        stdout.contains("Drain ile 25 kalıp (eşik 0,8) → parmak iziyle 26 grup"),
        "{stdout}"
    );
    // En sık tekrar eden kalıp en üstte; üç satırdan sonrası söylenir
    assert!(
        stdout.contains(
            "kalıp\n     8  INFO       09:02:13  17:22:08  Kullanıcı <SAYI> giriş yaptı\n"
        ),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("  … 23 satır daha; tümünü görmek için --hepsi\n"),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn utf8_olmayan_dosya_adi_programi_cokertmez() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    // Linux'ta dosya adı herhangi bir bayt dizisi olabilir. `env::args` böyle bir argümanda
    // çöküyordu (çıkış kodu 101); şimdi "okunamadı" hatası verilir.
    let ad = OsStr::from_bytes(b"olmayan-\xff.log");
    let cikti = Command::new(env!("CARGO_BIN_EXE_veylog"))
        .arg(ad)
        .output()
        .expect("veylog çalıştırılamadı");
    assert_eq!(cikti.status.code(), Some(1));
    let stderr = metin(&cikti.stderr);
    assert!(
        stderr.contains("olmayan-\u{FFFD}.log okunamadı"),
        "{stderr}"
    );
}

#[test]
fn olmayan_dosya_hata_verir() {
    let cikti = veylog(&["boyle-bir-dosya-yok.log"]);
    assert_eq!(cikti.status.code(), Some(1));
    assert!(metin(&cikti.stderr).contains("okunamadı"));
}

#[test]
fn gecersiz_utf8_dosyayi_durdurmaz() {
    let yol = gecici_dosya(
        "bozuk.log",
        b"[2026-10-24 10:00:00] production.INFO: Tamam\n\
          [2026-10-24 10:00:01] production.ERROR: Bozuk \xff bayt\n",
    );
    let cikti = veylog(&[yol.to_str().unwrap()]);
    fs::remove_file(&yol).ok();

    assert!(cikti.status.success());
    assert!(metin(&cikti.stderr).contains("geçersiz UTF-8"));
    assert!(metin(&cikti.stdout).contains("Kayıt         : 2"));
}

#[test]
fn iso_8601_zamanli_log_okunur() {
    let yol = gecici_dosya(
        "iso.log",
        "[2026-10-24T14:03:11.123456+03:00] production.ERROR: Bir hata\n\
         [2026-10-24T14:03:12+03:00] production.INFO: Tamam\n"
            .as_bytes(),
    );
    let cikti = veylog(&[yol.to_str().unwrap()]);
    fs::remove_file(&yol).ok();

    let stdout = metin(&cikti.stdout);
    assert!(cikti.status.success());
    assert!(stdout.contains("Kayıt         : 2"), "{stdout}");
    assert!(
        stdout.contains(
            "Zaman aralığı : 2026-10-24T14:03:11.123456+03:00 → 2026-10-24T14:03:12+03:00"
        ),
        "{stdout}"
    );
}
