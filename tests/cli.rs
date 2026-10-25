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
    assert_eq!(cikti.status.code(), Some(1));
    assert!(metin(&cikti.stderr).contains("Kullanım: veylog"));
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
