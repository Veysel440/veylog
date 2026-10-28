//! Programı gerçekten çalıştıran testler: argümanlar, hata mesajları ve çıkış kodları.
//!
//! Cargo, derlenmiş programın yolunu `CARGO_BIN_EXE_veylog` ortam değişkeniyle verir.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{self, Command, Output, Stdio};

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
    assert_eq!(
        metin(&cikti.stderr),
        "veylog: boyle-bir-dosya-yok.log okunamadı: dosya bulunamadı\n"
    );
}

#[test]
fn klasor_verilirse_ne_yapilacagi_soylenir() {
    let cikti = veylog(&["ornekler"]);
    assert_eq!(cikti.status.code(), Some(1));
    assert_eq!(
        metin(&cikti.stderr),
        "veylog: ornekler okunamadı: bu bir klasör; log dosyasını verin (ör. storage/logs/laravel.log)\n"
    );
}

#[test]
fn bos_dosya_adi_kullanim_hatasidir() {
    // `veylog "$LOG"` ve LOG tanımsız: adsız bir "okunamadı" hatası yerine
    let cikti = veylog(&[""]);
    assert_eq!(cikti.status.code(), Some(2));
    assert!(metin(&cikti.stderr).starts_with("veylog: dosya adı boş olamaz\n"));
}

#[test]
fn standart_girdiden_okunur() {
    // `zcat laravel-2026-10-24.log.gz | veylog -`
    let mut surec = Command::new(env!("CARGO_BIN_EXE_veylog"))
        .args(["-b", "json", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("veylog çalıştırılamadı");
    surec
        .stdin
        .take()
        .unwrap()
        .write_all(&fs::read("ornekler/laravel.log").unwrap())
        .unwrap();
    let cikti = surec.wait_with_output().unwrap();
    assert!(cikti.status.success());
    let rapor: serde_json::Value = serde_json::from_slice(&cikti.stdout).unwrap();
    assert_eq!(rapor["dosya"], "<standart girdi>");
    assert_eq!(rapor["kayit_sayisi"], 59);
}

#[test]
fn bom_ilk_kaydi_yutmaz() {
    // Bazı Windows araçları dosyanın başına UTF-8 BOM'u yazar
    let yol = gecici_dosya(
        "bom.log",
        "\u{feff}[2026-10-24 10:00:00] local.CRITICAL: Veritabanı çöktü\n\
         [2026-10-24 10:00:05] local.INFO: Tamam\n"
            .as_bytes(),
    );
    let cikti = veylog(&[yol.to_str().unwrap()]);
    fs::remove_file(&yol).ok();

    assert!(cikti.status.success());
    let stdout = metin(&cikti.stdout);
    assert!(stdout.contains("  Kayıt         : 2\n"), "{stdout}");
    assert!(stdout.contains("  Yetim satır   : 0\n"), "{stdout}");
    assert!(stdout.contains("\n  CRITICAL     1\n"), "{stdout}");

    // `cat gun1.log gun2.log | veylog -`: ikinci dosyanın BOM'u akışın ortasında
    let mut surec = Command::new(env!("CARGO_BIN_EXE_veylog"))
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("veylog çalıştırılamadı");
    surec
        .stdin
        .take()
        .unwrap()
        .write_all(
            "\u{feff}[2026-10-24 10:00:00] local.INFO: Birinci gün\n\
             \u{feff}[2026-10-25 10:00:00] local.CRITICAL: Veritabanı çöktü\n"
                .as_bytes(),
        )
        .unwrap();
    let cikti = surec.wait_with_output().unwrap();
    let stdout = metin(&cikti.stdout);
    assert!(stdout.contains("  Kayıt         : 2\n"), "{stdout}");
    assert!(stdout.contains("\n  CRITICAL     1\n"), "{stdout}");
}

#[test]
fn eksi_sifir_esik_sifir_yazilir() {
    let cikti = veylog(&["--esik", "-0", "-b", "json", "ornekler/laravel.log"]);
    assert!(cikti.status.success());
    let stdout = metin(&cikti.stdout);
    assert!(stdout.contains("\"esik\": 0,\n"), "{stdout}");
}

#[test]
fn rakamsiz_degisken_tek_kalipta_birlesir() {
    // Kaba kuvvet denemesi: her satırda başka bir kullanıcı adı. Kısa mesajda rakamsız bir
    // değer Drain'de birleşmez; 100'ü aşınca konum değişken sayılır ve hepsi tek kalıp olur.
    let mut satirlar = String::new();
    for sira in 0..150 {
        let ad: String = [sira / 26, sira]
            .iter()
            .map(|basamak| char::from(b'a' + (basamak % 26) as u8))
            .collect();
        satirlar.push_str(&format!(
            "[2026-10-24 10:{:02}:{:02}] local.WARNING: Bilinmeyen kullanıcı: {ad}\n",
            sira / 60,
            sira % 60
        ));
    }
    let yol = gecici_dosya("kaba-kuvvet.log", satirlar.as_bytes());
    let cikti = veylog(&[yol.to_str().unwrap()]);
    fs::remove_file(&yol).ok();

    assert!(cikti.status.success());
    let stdout = metin(&cikti.stdout);
    assert!(
        stdout.contains("150 farklı mesaj → maskeleyince 150 → Drain ile 1 kalıp"),
        "{stdout}"
    );
    assert!(
        stdout.contains(" 150  WARNING    10:00:00  10:02:29  Bilinmeyen kullanıcı: <*>\n"),
        "{stdout}"
    );
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

#[cfg(unix)]
#[test]
fn kontrol_karakterli_dosya_adi_temizlenir() {
    // Linux'ta dosya adı ESC de içerebilir: hem uyarıda hem raporun ilk satırında sembol olmalı
    let yol = gecici_dosya(
        "kötü\u{1b}[2J.log",
        b"[2026-10-24 10:00:00] local.INFO: Bozuk \xff bayt\n",
    );
    let cikti = veylog(&[yol.to_str().unwrap()]);
    fs::remove_file(&yol).ok();

    assert!(cikti.status.success());
    let stderr = metin(&cikti.stderr);
    assert!(stderr.contains("kötü␛[2J.log geçersiz UTF-8"), "{stderr}");
    assert!(!stderr.contains('\u{1b}'), "{stderr:?}");
    let stdout = metin(&cikti.stdout);
    assert!(stdout.contains("kötü␛[2J.log: 1 satır\n"), "{stdout}");
    assert!(!stdout.contains('\u{1b}'), "{stdout:?}");
}

#[test]
fn json_bicimi_gecerli_ve_tam() {
    let cikti = veylog(&["--bicim", "json", "--hepsi", "ornekler/laravel.log"]);
    assert!(cikti.status.success());
    assert!(cikti.stderr.is_empty(), "{}", metin(&cikti.stderr));

    // Başka bir programın yapacağı gibi: çıktıyı bir JSON ayrıştırıcısına ver
    let rapor: serde_json::Value =
        serde_json::from_slice(&cikti.stdout).expect("geçerli JSON değil");
    assert_eq!(rapor["surum"], env!("CARGO_PKG_VERSION"));
    assert_eq!(rapor["kayit_sayisi"], 59);
    assert_eq!(rapor["kaliplar"]["grup_sayisi"], 24);
    // --hepsi: bütün satırlar, gösterilmeyen yok
    assert_eq!(rapor["kaliplar"]["satirlar"].as_array().unwrap().len(), 24);
    assert_eq!(rapor["kaliplar"]["gosterilmeyen"], 0);
}

#[test]
fn markdown_bicimi_tablo_yazar() {
    let cikti = veylog(&["-b", "markdown", "-n", "2", "ornekler/laravel.log"]);
    assert!(cikti.status.success());
    let stdout = metin(&cikti.stdout);
    assert!(
        stdout.starts_with("## veylog: `ornekler/laravel.log`\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("| Adet | Seviye | İlk | Son | Kalıp | Örnek | Konum |\n"),
        "{stdout}"
    );
    // Başlık, ayraç ve --ilk kadar satır
    let tablo_satirlari = stdout
        .lines()
        .filter(|satir| satir.starts_with('|'))
        .count();
    assert_eq!(tablo_satirlari, 4, "{stdout}");
    assert!(
        stdout.ends_with("… 22 satır daha; tümünü görmek için `--hepsi`\n"),
        "{stdout}"
    );
}

#[test]
fn bilinmeyen_bicim_kullanim_hatasi_verir() {
    let cikti = veylog(&["--bicim", "xml", "ornekler/laravel.log"]);
    assert_eq!(cikti.status.code(), Some(2));
    assert!(cikti.stdout.is_empty());
    let stderr = metin(&cikti.stderr);
    assert!(
        stderr.contains("şunlardan biri olmalı: metin, json, markdown"),
        "{stderr}"
    );
}

#[test]
fn logdaki_kontrol_karakterleri_terminale_ulasmaz() {
    // İkinci mesajdaki ESC dizileri imleci iki satır yukarı alıp o satırları siler: eski
    // sürümde CRITICAL satırı ekrandan kayboluyordu. Üçüncüsü pencere başlığını değiştirir
    // ve CR ile satır başına dönüp yazdığının üstüne yazar.
    let yol = gecici_dosya(
        "kontrol.log",
        "[2026-10-24 10:00:00] local.CRITICAL: Veritabanı çöktü\n\
         [2026-10-24 10:00:01] local.INFO: Giriş \u{1b}[1A\u{1b}[2K\u{1b}[1A\u{1b}[2Kyapıldı\n\
         [2026-10-24 10:00:02] local.INFO: \u{1b}]0;başlık\u{7}Ödeme tamam\rHATA\n"
            .as_bytes(),
    );
    let yol_metni = yol.to_str().unwrap();
    let ciktilar: Vec<(&str, Output)> = ["metin", "json", "markdown"]
        .into_iter()
        .map(|bicim| (bicim, veylog(&["--bicim", bicim, yol_metni])))
        .collect();
    fs::remove_file(&yol).ok();

    for (bicim, cikti) in &ciktilar {
        assert!(cikti.status.success(), "{bicim}");
        let stdout = metin(&cikti.stdout);
        let kalanlar: Vec<char> = stdout
            .chars()
            .filter(|&harf| harf.is_control() && harf != '\n')
            .collect();
        assert!(kalanlar.is_empty(), "{bicim}: {kalanlar:?}");
        assert!(stdout.contains("Veritabanı çöktü"), "{bicim}: {stdout}");
    }

    // JSON ham mesajı kaybetmez: kaçışları çözülünce logdaki metnin aynısı çıkar
    let rapor: serde_json::Value = serde_json::from_slice(&ciktilar[1].1.stdout).unwrap();
    let ornekler: Vec<&str> = rapor["kaliplar"]["satirlar"]
        .as_array()
        .unwrap()
        .iter()
        .map(|satir| satir["ornek"].as_str().unwrap())
        .collect();
    assert!(
        ornekler.contains(&"\u{1b}]0;başlık\u{7}Ödeme tamam\rHATA"),
        "{ornekler:?}"
    );
}

#[test]
fn hata_mesajlarindaki_argumanlar_temizlenir() {
    // Dosya adı da fazladan argüman da kullanıcıdan gelir ve hata mesajında geri yazılır
    for (argumanlar, beklenen, cikis_kodu) in [
        (&["yok\u{1b}[2J.log"][..], "yok␛[2J.log okunamadı", 1),
        (
            &["a.log", "b\u{1b}[31m.log"],
            "fazladan argüman: b␛[31m.log",
            2,
        ),
    ] {
        let cikti = veylog(argumanlar);
        assert_eq!(cikti.status.code(), Some(cikis_kodu), "{argumanlar:?}");
        let stderr = metin(&cikti.stderr);
        assert!(stderr.contains(beklenen), "{stderr}");
        assert!(!stderr.contains('\u{1b}'), "{stderr:?}");
    }
}

#[test]
fn karsilastirma_raporu() {
    // Örnek logda 15:00'te yeni sürüm yayına alındı: öncesi ve sonrası
    let cikti = veylog(&["--karsilastir", "15:00", "ornekler/laravel.log"]);
    assert!(cikti.status.success());
    let stdout = metin(&cikti.stdout);
    assert!(
        stdout.contains("\nKarşılaştırma: 2026-10-24 15:00:00 öncesi ve sonrası\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("\nYeni çıkanlar (yalnızca sonra): 7\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("\nKaybolanlar (yalnızca önce): 10\n"),
        "{stdout}"
    );
    // Yayından iki dakika sonra başlayan hata yeni çıkanlar arasında
    assert!(
        stdout.contains(
            "\n     0      4  ERROR      15:02:11  16:58:12  Undefined array key \"kargo_ucreti\"\n"
        ),
        "{stdout}"
    );
    assert!(
        stdout.ends_with("\nArtanlar (sonra ≥ 2 kat sık, tesadüf ihtimali < %1): yok\n"),
        "{stdout}"
    );

    // JSON: 16:11'de bölünce ödeme zaman aşımı 1'den 5'e çıkıyor
    let cikti = veylog(&["-b", "json", "-k", "16:11", "ornekler/laravel.log"]);
    assert!(cikti.status.success());
    let rapor: serde_json::Value = serde_json::from_slice(&cikti.stdout).unwrap();
    let artan = &rapor["karsilastirma"]["artan"];
    assert_eq!(artan["kalip_sayisi"], 1);
    assert_eq!(
        (
            &artan["satirlar"][0]["once"],
            &artan["satirlar"][0]["sonra"]
        ),
        (&1.into(), &5.into())
    );
}

#[test]
fn karsilastirma_noktasi_logun_disindaysa_kullanim_hatasi_verir() {
    let cikti = veylog(&["--karsilastir", "07:00", "ornekler/laravel.log"]);
    assert_eq!(cikti.status.code(), Some(2));
    assert!(cikti.stdout.is_empty());
    let stderr = metin(&cikti.stderr);
    assert!(
        stderr.contains(
            "veylog: karşılaştırma noktası 2026-10-24 07:00:00 logun içinde değil: kayıtlar 2026-10-24 08:59:58 ile 2026-10-24 18:31:07 arasında"
        ),
        "{stderr}"
    );

    // Zaman geçersizse dosya okunmadan reddedilir
    let cikti = veylog(&["-k", "25:00", "ornekler/laravel.log"]);
    assert_eq!(cikti.status.code(), Some(2));
    let stderr = metin(&cikti.stderr);
    assert!(
        stderr.contains("veylog: -k için geçersiz değer \"25:00\""),
        "{stderr}"
    );
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
    // Zamanlar tek biçimde; saat dilimi ayrı bir satırda
    assert!(
        stdout.contains(
            "  Zaman aralığı : 2026-10-24 14:03:11 → 2026-10-24 14:03:12\n  Saat dilimi   : +03:00\n"
        ),
        "{stdout}"
    );
}

/// UTC'de yazılmış bir log: üç kayıt, biri yaz saatinden önce (+03:00) yazılmış.
const UTC_LOG: &str = "\
[2026-10-24T09:00:00.000000+00:00] production.ERROR: Kuyruk 1 dolu
[2026-10-24T11:30:00.000000+00:00] production.ERROR: Kuyruk 2 dolu
[2026-10-24T14:45:00+03:00] production.WARNING: Disk doluyor
";

#[test]
fn saat_dilimi_verilirse_zamanlar_ona_cevrilir() {
    let yol = gecici_dosya("utc.log", UTC_LOG.as_bytes());
    let yol = yol.to_str().unwrap();

    // Raporun dilimi logdaki ilk dilim (+00:00); +03:00'daki kayıt ona çevrilir
    let cikti = veylog(&[yol]);
    let stdout = metin(&cikti.stdout);
    assert!(
        stdout.contains(
            "  Zaman aralığı : 2026-10-24 09:00:00 → 2026-10-24 11:45:00\n  Saat dilimi   : +00:00; 1 kayıt +03:00 diliminden çevrildi\n"
        ),
        "{stdout}"
    );

    // --saat-dilimi +03:00: bütün zamanlar bu dilimde; filtre de
    let cikti = veylog(&["--saat-dilimi", "+03:00", "--baslangic", "14:00", yol]);
    let stdout = metin(&cikti.stdout);
    assert!(cikti.status.success(), "{}", metin(&cikti.stderr));
    assert!(
        stdout.contains(
            "  Zaman aralığı : 2026-10-24 14:30:00 → 2026-10-24 14:45:00\n  Saat dilimi   : +03:00 (--saat-dilimi); 1 kayıt +00:00 diliminden çevrildi\n"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains("Filtre        : 2026-10-24 14:00:00 ve sonrası: 2 kayıt"),
        "{stdout}"
    );
    assert!(
        stdout.contains("14:30:00  14:30:00  Kuyruk <SAYI> dolu"),
        "{stdout}"
    );

    // JSON'da zamanlar raporun diliminde, dilim ayrı bir nesnede
    let cikti = veylog(&["--bicim", "json", "--saat-dilimi", "+03:00", yol]);
    let json = metin(&cikti.stdout);
    assert!(
        json.contains(
            "\"zaman_araligi\": {\n    \"en_eski\": \"2026-10-24 12:00:00\",\n    \"en_yeni\": \"2026-10-24 14:45:00\"\n  },\n  \"saat_dilimi\": {\n    \"dilim\": \"+03:00\",\n    \"kaynak\": \"secenek\",\n    \"cevrilen_kayit\": 2,\n    \"cevrilen_dilimler\": [\n      \"+00:00\"\n    ],\n    \"dilimsiz_kayit\": 0\n  },"
        ),
        "{json}"
    );

    // Geçersiz dilim bir kullanım hatasıdır
    let cikti = veylog(&["--saat-dilimi", "+3:0", yol]);
    fs::remove_file(yol).ok();
    assert_eq!(cikti.status.code(), Some(2));
    assert!(
        metin(&cikti.stderr).contains("veylog: --saat-dilimi için geçersiz değer \"+3:0\""),
        "{}",
        metin(&cikti.stderr)
    );
}

#[test]
fn son_ve_ani_artis_raporun_saat_diliminde() {
    // UTC'de yazılmış bir log, Türkiye saatiyle (+03:00): en yeni kayıt 22:30Z = 01:30
    let mut satirlar = vec![
        "[2026-10-24T07:00:00+00:00] local.ERROR: Kuyruk 999 dolu".to_string(),
        "[2026-10-24T08:30:00+00:00] local.INFO: Başladı".to_string(),
        "[2026-10-24T08:35:00+00:00] local.ERROR: Kuyruk 1 dolu".to_string(),
    ];
    for numara in 50..56 {
        satirlar.push(format!(
            "[2026-10-24T09:00:{:02}+00:00] local.ERROR: Kuyruk {numara} dolu",
            numara - 50
        ));
    }
    satirlar.push("[2026-10-24T22:30:00+00:00] local.INFO: Bitti".to_string());
    let yol = gecici_dosya("utc-son.log", satirlar.join("\n").as_bytes());
    let yol = yol.to_str().unwrap();

    // Son bir saat: 00:30'dan (+03:00) bu yana; 22:30Z'den değil
    let cikti = veylog(&["--saat-dilimi", "+03:00", "--son", "1sa", yol]);
    let stdout = metin(&cikti.stdout);
    assert!(
        stdout.contains("Filtre        : 2026-10-25 00:30:00 ve sonrası (son 1 sa): 1 kayıt"),
        "{stdout}"
    );

    // Ani artışın ilk kaydı (Kuyruk 50) alt grubunun ilk kaydı değil: yeniden okunur. Yeniden
    // okuma da 11:00'dan (+03:00) sonrasını, yani 08:00Z'den sonrasını sayar.
    let cikti = veylog(&[
        "--saat-dilimi",
        "+03:00",
        "--baslangic",
        "2026-10-24 11:00",
        "--bitis",
        "2026-10-24 13:00",
        "-a",
        "1",
        yol,
    ]);
    fs::remove_file(yol).ok();
    let (stdout, stderr) = (metin(&cikti.stdout), metin(&cikti.stderr));
    assert!(stderr.is_empty(), "{stderr}");
    let (_, artislar) = stdout.split_once("\nAni artışlar").unwrap();
    assert!(artislar.contains("örnek: Kuyruk 50 dolu\n"), "{stdout}");
}

#[test]
fn saat_dilimi_yazilmamis_logda_dilim_satiri_yok() {
    // Laravel'in varsayılan biçimi: saat dilimi yazılmaz, satır da çıkmaz
    let cikti = veylog(&["ornekler/laravel.log"]);
    assert!(!metin(&cikti.stdout).contains("Saat dilimi"));
    let cikti = veylog(&["--bicim", "json", "ornekler/laravel.log"]);
    assert!(metin(&cikti.stdout).contains("\n  \"saat_dilimi\": null,\n"));
    // Verilirse zamanlar olduğu gibi kalır; rapor bunu söyler
    let cikti = veylog(&["--saat-dilimi", "+03:00", "ornekler/laravel.log"]);
    let stdout = metin(&cikti.stdout);
    assert!(
        stdout.contains(
            "  Zaman aralığı : 2026-10-24 08:59:58 → 2026-10-24 18:31:07\n  Saat dilimi   : +03:00 (--saat-dilimi); kayıtlarda saat dilimi yazılı değil, zamanlar logdaki gibi\n"
        ),
        "{stdout}"
    );
}

#[test]
fn ani_artis_raporu() {
    // Örnek logda 10 dakikalık dilimlerle 16:10'daki ödeme zaman aşımları görünür
    let cikti = veylog(&["--ani-artis", "10", "ornekler/laravel.log"]);
    assert!(cikti.status.success());
    let stdout = metin(&cikti.stdout);
    assert!(
        stdout.contains("\nAni artışlar (WARNING ve üstü, 10 dakikalık 59 dilim, puan > 3,5): 1\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("artış: 16:10–16:20 arası 6 kayıt\n"),
        "{stdout}"
    );

    // Dakika dakika bakınca hiçbir kalıp bir dakikada 6 kayda ulaşmıyor
    let cikti = veylog(&["-a", "1", "ornekler/laravel.log"]);
    assert!(cikti.status.success());
    let stdout = metin(&cikti.stdout);
    assert!(
        stdout.ends_with(
            "\nAni artışlar (WARNING ve üstü, 1 dakikalık 573 dilim, puan > 3,5): yok\n"
        ),
        "{stdout}"
    );

    let cikti = veylog(&["-b", "json", "-a", "10", "ornekler/laravel.log"]);
    assert!(cikti.status.success());
    let rapor: serde_json::Value = serde_json::from_slice(&cikti.stdout).unwrap();
    let ani_artislar = &rapor["ani_artislar"];
    assert_eq!(ani_artislar["artis_sayisi"], 1);
    assert_eq!(ani_artislar["satirlar"][0]["artis"]["zirve"]["adet"], 6);
}

/// İki saatlik bir log: "Kuyruk <SAYI> dolu" on dakikada bir WARNING, 11:00'de 8 WARNING'lik bir
/// artış. Artışın ilk kaydı ("Kuyruk 50 dolu") kalıbın ilk kaydı değil: örneği bulmak için log
/// yeniden okunmalı.
fn kuyruk_artisi() -> String {
    let mut satirlar = Vec::new();
    for dakika in (0..120).step_by(10) {
        let (saat, dakika) = (10 + dakika / 60, dakika % 60);
        satirlar.push(format!(
            "[2026-10-24 {saat}:{dakika:02}:30] local.WARNING: Kuyruk {} dolu",
            saat * 100 + dakika
        ));
    }
    for saniye in 0..8 {
        satirlar.push(format!(
            "[2026-10-24 11:00:{:02}] local.WARNING: Kuyruk {} dolu",
            saniye * 5 + 1,
            50 + saniye
        ));
    }
    satirlar.sort();
    satirlar.join("\n")
}

#[test]
fn ani_artis_ornegi_yeniden_okunarak_bulunur() {
    let log = kuyruk_artisi();
    let beklenen = "örnek: Kuyruk 50 dolu\n";

    // Dosyadan: aynı dosya yeniden okunur
    let yol = gecici_dosya("ani-ornek.log", log.as_bytes());
    let cikti = veylog(&["-a", "1", yol.to_str().unwrap()]);
    fs::remove_file(&yol).ok();
    let stdout = metin(&cikti.stdout);
    assert!(cikti.status.success());
    let (kaliplar, artislar) = stdout.split_once("\nAni artışlar").unwrap();
    assert!(kaliplar.contains("örnek: Kuyruk 1000 dolu\n"), "{stdout}");
    assert!(artislar.contains(beklenen), "{stdout}");

    // Standart girdiden: geçici bir dosyaya kopyalanır, iş bitince geçici dosya kalmaz.
    // Geçici klasör yalnızca bu test için: başka testlerin dosyaları karışmasın.
    let gecici_klasor = std::env::temp_dir().join(format!("veylog-{}-gecici", process::id()));
    fs::create_dir_all(&gecici_klasor).unwrap();
    for (secenekler, ornekli) in [(&["-a", "1", "-"][..], true), (&["-"][..], false)] {
        let mut surec = Command::new(env!("CARGO_BIN_EXE_veylog"))
            .args(secenekler)
            .env("TMPDIR", &gecici_klasor)
            .env("TMP", &gecici_klasor)
            .env("TEMP", &gecici_klasor)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("veylog çalıştırılamadı");
        surec
            .stdin
            .take()
            .unwrap()
            .write_all(log.as_bytes())
            .unwrap();
        let cikti = surec.wait_with_output().unwrap();
        assert!(cikti.status.success(), "{}", metin(&cikti.stderr));
        assert_eq!(metin(&cikti.stdout).contains(beklenen), ornekli);
        let kalanlar: Vec<_> = fs::read_dir(&gecici_klasor).unwrap().collect();
        assert!(kalanlar.is_empty(), "{kalanlar:?}");
    }
    fs::remove_dir(&gecici_klasor).ok();
}

/// Geçici dosya oluşturulamazsa (geçici klasör yok, disk dolu) standart girdi kopyalanmadan
/// okunur: rapor yine çıkar, yeniden okunamayan örneğin yerine alt grubun örneği yazılır.
#[cfg(unix)]
#[test]
fn gecici_dosya_olusturulamazsa_rapor_yine_cikar() {
    // Geçici klasör bir dosyanın altında: orada hiçbir kullanıcı (root da) dosya oluşturamaz.
    // Var olmadığı varsayılan sabit bir yol (`/olmayan/bir/klasor`) her makinede yok değildir.
    let dosya = gecici_dosya("klasor-degil", b"");
    let gecici_klasor = dosya.join("alt");
    let mut surec = Command::new(env!("CARGO_BIN_EXE_veylog"))
        .args(["-a", "1", "-"])
        .env("TMPDIR", &gecici_klasor)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("veylog çalıştırılamadı");
    surec
        .stdin
        .take()
        .unwrap()
        .write_all(kuyruk_artisi().as_bytes())
        .unwrap();
    let cikti = surec.wait_with_output().unwrap();
    fs::remove_file(&dosya).ok();
    let (stdout, stderr) = (metin(&cikti.stdout), metin(&cikti.stderr));
    assert!(cikti.status.success(), "{stderr}");
    assert!(
        stderr.starts_with("Uyarı: girdinin geçici kopyası oluşturulamadı ("),
        "geçici klasör: {}\nstderr: {stderr}\nstdout: {stdout}",
        gecici_klasor.display()
    );
    let (_, artislar) = stdout.split_once("\nAni artışlar").unwrap();
    assert!(artislar.contains("örnek: Kuyruk 1000 dolu\n"), "{stdout}");
}

#[test]
fn filtreler_raporu_daraltir() {
    let cikti = veylog(&["--seviye", "error", "ornekler/laravel.log"]);
    assert!(cikti.status.success());
    let stdout = metin(&cikti.stdout);
    // Dosyanın sayıları değişmez; seviyeler ve kalıplar yalnızca filtreye uyanlardan
    assert!(
        stdout.starts_with(
            "ornekler/laravel.log: 203 satır\n  Kayıt         : 59\n  Devam satırı  : 141\n  Yetim satır   : 3\n  Filtre        : ERROR ve üstü: 18 kayıt\n"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains("\nSeviyeler:\n  CRITICAL     2\n  ERROR       16\n\n"),
        "{stdout}"
    );

    // Zaman aralığı: başlangıç dahil, bitiş hariç; JSON'da çözülmüş sınırlar
    let cikti = veylog(&[
        "-b",
        "json",
        "--baslangic",
        "16:00",
        "--bitis",
        "17:00",
        "ornekler/laravel.log",
    ]);
    assert!(cikti.status.success());
    let rapor: serde_json::Value = serde_json::from_slice(&cikti.stdout).unwrap();
    assert_eq!(rapor["filtre"]["baslangic"], "2026-10-24 16:00:00");
    assert_eq!(rapor["filtre"]["bitis"], "2026-10-24 17:00:00");
    assert_eq!(rapor["kayit_sayisi"], 59);
    assert_eq!(rapor["zaman_araligi"]["en_eski"], "2026-10-24 16:02:44");
    assert_eq!(rapor["zaman_araligi"]["en_yeni"], "2026-10-24 16:58:12");
}

#[test]
fn son_standart_girdiden_de_okunur() {
    // `--son` için en yeni kayıt önce bulunur: standart girdi geçici bir dosyaya kopyalanır,
    // iki kez okunur, iş bitince kopya kalmaz
    let gecici_klasor = std::env::temp_dir().join(format!("veylog-{}-son", process::id()));
    fs::create_dir_all(&gecici_klasor).unwrap();
    let mut surec = Command::new(env!("CARGO_BIN_EXE_veylog"))
        .args(["--son", "30dk", "-a", "1", "-"])
        .env("TMPDIR", &gecici_klasor)
        .env("TMP", &gecici_klasor)
        .env("TEMP", &gecici_klasor)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("veylog çalıştırılamadı");
    surec
        .stdin
        .take()
        .unwrap()
        .write_all(&fs::read("ornekler/laravel.log").unwrap())
        .unwrap();
    let cikti = surec.wait_with_output().unwrap();
    assert!(cikti.status.success(), "{}", metin(&cikti.stderr));
    let stdout = metin(&cikti.stdout);
    assert!(
        stdout.contains(
            "  Filtre        : 2026-10-24 18:01:07 ve sonrası (son 30 dk): 3 kayıt\n  Zaman aralığı : 2026-10-24 18:12:19 → 2026-10-24 18:31:07\n"
        ),
        "{stdout}"
    );
    assert!(
        stdout
            .contains("\nAni artışlar (WARNING ve üstü, 1 dakikalık 20 dilim, puan > 3,5): yok\n"),
        "{stdout}"
    );
    let kalanlar: Vec<_> = fs::read_dir(&gecici_klasor).unwrap().collect();
    assert!(kalanlar.is_empty(), "{kalanlar:?}");
    fs::remove_dir(&gecici_klasor).ok();
}

#[test]
fn filtre_hatalari_kullanim_hatasidir() {
    for (argumanlar, beklenen) in [
        (
            &[
                "--baslangic",
                "17:00",
                "--bitis",
                "16:00",
                "ornekler/laravel.log",
            ][..],
            "veylog: filtre aralığı boş: başlangıç (2026-10-24 17:00:00) bitişten (2026-10-24 16:00:00) önce olmalı\n",
        ),
        (
            &["--son", "1sa", "--bitis", "16:00", "ornekler/laravel.log"],
            "veylog: --son ile --bitis birlikte kullanılamaz\n",
        ),
        (
            &[
                "--seviye",
                "emergency",
                "-k",
                "15:00",
                "ornekler/laravel.log",
            ],
            "veylog: karşılaştırma için filtreye uyan kayıt yok\n",
        ),
    ] {
        let cikti = veylog(argumanlar);
        assert_eq!(cikti.status.code(), Some(2), "{argumanlar:?}");
        assert!(cikti.stdout.is_empty());
        assert!(
            metin(&cikti.stderr).starts_with(beklenen),
            "{}",
            metin(&cikti.stderr)
        );
    }
    // Filtreye uyan kayıt yoksa rapor yine yazılır
    let cikti = veylog(&["--seviye", "emergency", "-a", "1", "ornekler/laravel.log"]);
    assert!(cikti.status.success());
    let stdout = metin(&cikti.stdout);
    assert!(
        stdout.contains("  Filtre        : EMERGENCY ve üstü: 0 kayıt\n"),
        "{stdout}"
    );
    assert!(
        stdout.ends_with(
            "\nAni artışlar (WARNING ve üstü, 1 dakikalık dilimler): filtreye uyan kayıt yok\n"
        ),
        "{stdout}"
    );
}

#[test]
fn ani_artis_dilimi_denetlenir() {
    let cikti = veylog(&["--ani-artis", "0", "ornekler/laravel.log"]);
    assert_eq!(cikti.status.code(), Some(2));
    assert!(cikti.stdout.is_empty());
    let stderr = metin(&cikti.stderr);
    assert!(
        stderr.contains(
            "veylog: --ani-artis için geçersiz değer \"0\": 1 ile 1440 arasında bir dakika sayısı olmalı"
        ),
        "{stderr}"
    );

    // Log dilim sayısına göre kısaysa hata değil: bölüm bunu söyler, çıkış kodu 0
    let cikti = veylog(&["-a", "120", "ornekler/laravel.log"]);
    assert!(cikti.status.success());
    let stdout = metin(&cikti.stdout);
    assert!(
        stdout.ends_with("olağan seviyeyi ölçmek için en az 10 dilim gerekir\n"),
        "{stdout}"
    );
}

#[test]
fn json_satirlari_ve_windows_yollari() {
    // ornekler/karisik.log: Windows'ta (Herd) çalışan bir uygulamanın logu. 10:00'a kadar
    // Laravel'in metin biçiminde, sonra Monolog'un JSON biçiminde yazılmış.
    let cikti = veylog(&["--hepsi", "ornekler/karisik.log"]);
    assert!(cikti.status.success());
    let stdout = metin(&cikti.stdout);
    assert!(stdout.contains("  Kayıt         : 21\n"), "{stdout}");
    // Windows yolu: köke göre ve ayracı / olarak
    assert!(
        stdout.contains(
            "konum: database/migrations/2026_10_25_090000_create_kuponlar_table.php:11 (QueryException)\n"
        ),
        "{stdout}"
    );
    // Aynı hata biri metin, biri JSON biçiminde iki kez yazıldı: tek satırda birleşir
    assert!(
        stdout.contains(
            "     2  ERROR      09:20:11  10:15:30  Undefined array key \"kargo_ucreti\"\n"
        ),
        "{stdout}"
    );
    // JSON kaydının çok satırlı mesajının ilk satırı
    assert!(
        stdout.contains("     1  NOTICE     11:30:00  11:30:00  Kupon \"YAZ25\" süresi doldu:\n"),
        "{stdout}"
    );

    let cikti = veylog(&["-b", "json", "--hepsi", "ornekler/karisik.log"]);
    let rapor: serde_json::Value = serde_json::from_slice(&cikti.stdout).unwrap();
    let curl = rapor["kaliplar"]["satirlar"]
        .as_array()
        .unwrap()
        .iter()
        .find(|satir| satir["kalip"].as_str().unwrap().starts_with("cURL"))
        .unwrap();
    assert_eq!(curl["adet"], 3);
    assert_eq!(
        curl["istisna"],
        serde_json::json!({
            "sinif": r"Illuminate\Http\Client\ConnectionException",
            "dosya": "app/Services/OdemeServisi.php",
            "satir": 22
        })
    );
}

#[test]
fn view_hatalari_middleware_ve_kok() {
    // ornekler/karisik.log'un 09:48–09:58 arası: bir yönetim paneline gelen istekler. Hepsi
    // `YoneticiMi` middleware'inden geçiyor.
    let cikti = veylog(&["--hepsi", "ornekler/karisik.log"]);
    let stdout = metin(&cikti.stdout);
    // View hatasının konumu şablon; şablonun yolu kalıpta maskelenir
    assert!(
        stdout.contains(
            "     2  ERROR      09:48:21  09:49:05  Route [panel.kampanya.olustur] not defined. (View: <YOL>)\n"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains("konum: resources/views/panel/kampanyalar.blade.php (ViewException)\n"),
        "{stdout}"
    );
    // İç içe şablonda hatanın oluştuğu şablon
    assert!(
        stdout.contains("konum: resources/views/partials/urun-karti.blade.php (ViewException)\n"),
        "{stdout}"
    );
    // Tanımsız denetleyici metodu: yığın izindeki tek uygulama satırı middleware'in
    // `return $next($request);` satırı, o hatanın yeri değil
    assert!(
        stdout.contains(
            "  Call to undefined method App\\Http\\Controllers\\KampanyaController::duzenle()\n                                       konum: uygulama kodunda bulunamadı (Error)\n"
        ),
        "{stdout}"
    );
    // Yığın izi olmayan ölümcül hata: kök logun başka kayıtlarından bulunur
    assert!(
        stdout.contains("konum: app/Http/Controllers/RaporController.php:5 (FatalError)\n"),
        "{stdout}"
    );

    // JSON'da şablonun satırı yok: null
    let cikti = veylog(&["-b", "json", "--hepsi", "ornekler/karisik.log"]);
    let rapor: serde_json::Value = serde_json::from_slice(&cikti.stdout).unwrap();
    let route = rapor["kaliplar"]["satirlar"]
        .as_array()
        .unwrap()
        .iter()
        .find(|satir| satir["kalip"].as_str().unwrap().starts_with("Route"))
        .unwrap();
    assert_eq!(
        route["istisna"],
        serde_json::json!({
            "sinif": r"Illuminate\View\ViewException",
            "dosya": "resources/views/panel/kampanyalar.blade.php",
            "satir": null
        })
    );

    // Markdown'da da satırsız
    let cikti = veylog(&["-b", "markdown", "--hepsi", "ornekler/karisik.log"]);
    let stdout = metin(&cikti.stdout);
    assert!(
        stdout.contains("| `resources/views/panel/kampanyalar.blade.php` ViewException |\n"),
        "{stdout}"
    );
}

/// Aynı hata iki yolla loglanmış: `Log::error($e)` (PHP'nin istisna metni) ve `report($e)`
/// (bağlamdaki istisna). İkisi tek satırda birleşmeli. İkincinin yollarında vendor yok: proje
/// kökü birincinin yığın izinden bulunur.
const IKI_YOLLA_LOGLANAN: &str = r#"[2026-10-27 14:03:11] production.ERROR: RuntimeException: Ödeme alınamadı: sipariş 1047 in /var/www/magaza/app/Services/OdemeServisi.php:64
Stack trace:
#0 /var/www/magaza/app/Http/Controllers/OdemeController.php(31): App\Services\OdemeServisi->tahsilEt()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/Controller.php(54): App\Http\Controllers\OdemeController->odeme()
#2 /var/www/magaza/public/index.php(17): Illuminate\Foundation\Application->handleRequest()
#3 {main}  
[2026-10-27 14:05:40] production.ERROR: Ödeme alınamadı: sipariş 1052 {"exception":"[object] (RuntimeException(code: 0): Ödeme alınamadı: sipariş 1052 at /var/www/magaza/app/Services/OdemeServisi.php:64)
[stacktrace]
#0 /var/www/magaza/app/Http/Controllers/OdemeController.php(31): App\\Services\\OdemeServisi->tahsilEt()
#1 {main}
"} 
"#;

#[test]
fn istisna_metni_baglamdaki_istisnayla_birlesir() {
    let yol = gecici_dosya("istisna-metni.log", IKI_YOLLA_LOGLANAN.as_bytes());
    let cikti = veylog(&["--hepsi", yol.to_str().unwrap()]);
    let json = veylog(&["-b", "json", yol.to_str().unwrap()]);
    fs::remove_file(&yol).ok();
    let stdout = metin(&cikti.stdout);
    assert!(cikti.status.success());
    assert!(cikti.stderr.is_empty(), "{}", metin(&cikti.stderr));
    assert!(
        stdout.ends_with(
            "  adet  seviye     ilk       son       kalıp\n     2  ERROR      14:03:11  14:05:40  Ödeme alınamadı: sipariş <SAYI>\n                                       örnek: Ödeme alınamadı: sipariş 1047\n                                       konum: app/Services/OdemeServisi.php:64 (RuntimeException)\n"
        ),
        "{stdout}"
    );
    let rapor: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    let satir = &rapor["kaliplar"]["satirlar"][0];
    assert_eq!(satir["adet"], 2);
    assert_eq!(satir["istisna"]["sinif"], "RuntimeException");
    assert_eq!(satir["istisna"]["dosya"], "app/Services/OdemeServisi.php");
    assert_eq!(satir["istisna"]["satir"], 64);
}

/// `kuyruk_artisi`nin başka bir satır ve zaman biçiminde yazılmış hali:
/// `24.10.2026 10:00:30 [WARNING] Kuyruk 1000 dolu`
fn ozel_bicimde(log: &str) -> String {
    log.lines()
        .map(|satir| {
            let (zaman, kalan) = satir[1..].split_once("] local.").unwrap();
            let (seviye, mesaj) = kalan.split_once(": ").unwrap();
            let (tarih, saat) = zaman.split_once(' ').unwrap();
            let parcalar: Vec<&str> = tarih.split('-').collect();
            format!(
                "{}.{}.{} {saat} [{seviye}] {mesaj}",
                parcalar[2], parcalar[1], parcalar[0]
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Raporun dosya adı dışındaki kısmı.
fn adsiz(cikti: &Output) -> String {
    let stdout = metin(&cikti.stdout);
    stdout.split_once(": ").unwrap().1.to_string()
}

#[test]
fn desen_ve_zaman_bicimiyle_okunur() {
    let laravel = gecici_dosya("laravel-bicimi.log", kuyruk_artisi().as_bytes());
    let ozel_log = ozel_bicimde(&kuyruk_artisi());
    assert!(ozel_log.starts_with("24.10.2026 10:00:30 [WARNING] Kuyruk 1000 dolu\n"));
    let ozel = gecici_dosya("ozel-bicim.log", ozel_log.as_bytes());
    // Ön tarama (`--son`) ve yeniden okuma (ani artışın örneği) da aynı biçimle okumalı
    let ortak = ["-a", "1", "--son", "1sa"];
    let beklenen = veylog(&[&ortak[..], &[laravel.to_str().unwrap()]].concat());
    assert!(adsiz(&beklenen).contains("örnek: Kuyruk 50 dolu\n"));
    for desen in [
        "%datetime% [%level_name%] %message%",
        r"(?P<zaman>\S+ \S+) \[(?P<seviye>\w+)\] (?P<mesaj>.*)",
    ] {
        let cikti = veylog(
            &[
                &["--desen", desen, "--zaman-bicimi", "d.m.Y H:i:s"][..],
                &ortak,
                &[ozel.to_str().unwrap()],
            ]
            .concat(),
        );
        assert!(cikti.status.success(), "{desen}");
        assert!(cikti.stderr.is_empty(), "{desen}: {}", metin(&cikti.stderr));
        assert_eq!(adsiz(&cikti), adsiz(&beklenen), "{desen}");
    }

    // Standart girdiden: girdi geçici bir kopyadan iki kez okunur, ikisi de desenle
    let gecici_klasor = std::env::temp_dir().join(format!("veylog-{}-desen", process::id()));
    fs::create_dir_all(&gecici_klasor).unwrap();
    let mut surec = Command::new(env!("CARGO_BIN_EXE_veylog"))
        .args(["--desen", "%datetime% [%level_name%] %message%"])
        .args(["--zaman-biçimi", "d.m.Y H:i:s"])
        .args(ortak)
        .arg("-")
        .env("TMPDIR", &gecici_klasor)
        .env("TMP", &gecici_klasor)
        .env("TEMP", &gecici_klasor)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("veylog çalıştırılamadı");
    surec
        .stdin
        .take()
        .unwrap()
        .write_all(ozel_log.as_bytes())
        .unwrap();
    let cikti = surec.wait_with_output().unwrap();
    assert!(cikti.stderr.is_empty(), "{}", metin(&cikti.stderr));
    assert_eq!(adsiz(&cikti), adsiz(&beklenen));
    fs::remove_dir_all(&gecici_klasor).ok();
    fs::remove_file(&laravel).ok();
    fs::remove_file(&ozel).ok();
}

#[test]
fn desen_ve_zaman_bicimi_hatalari_kullanim_hatasidir() {
    for (secenekler, beklenen) in [
        (
            &["--desen", "(?P<zaman>x"][..],
            "veylog: --desen için geçersiz değer \"(?P<zaman>x\": düzenli ifade derlenemedi: unclosed group\n",
        ),
        (
            &["--desen", r"(?P<zaman>\S+) (?P<msj>.*)"][..],
            "veylog: --desen için geçersiz değer \"(?P<zaman>\\S+) (?P<msj>.*)\": \"msj\" grubu bilinmiyor; kullanılabilenler: zaman, seviye, mesaj, baglam, kanal\n",
        ),
        (
            &["--desen", r"(?P<zaman>\S+ \S+)"][..],
            "veylog: --desen için geçersiz değer \"(?P<zaman>\\S+ \\S+)\": (?P<mesaj>...) grubu yok; zaman ve mesaj grupları gerekli\n",
        ),
        (
            &["--desen", "[%level_name%] %message%"][..],
            "veylog: --desen için geçersiz değer \"[%level_name%] %message%\": %datetime% yok; zaman gerekli\n",
        ),
        (
            &["--zaman-bicimi", "H:i"][..],
            "veylog: --zaman-bicimi için geçersiz değer \"H:i\": zaman biçiminde yıl, ay ve gün yok; yıl (Y), ay (m), gün (d), saat (H) ve dakika (i) gerekir, ya da Unix zamanı (U)\n",
        ),
        (
            &["--zaman-biçimi", "Y-m-d h:i"][..],
            "veylog: --zaman-biçimi için geçersiz değer \"Y-m-d h:i\": 12 saatlik saat (h, g) öğleden önce/sonra işareti (A ya da a) olmadan okunamaz\n",
        ),
        (
            &["--desn", "x"][..],
            "veylog: bilinmeyen seçenek: --desn (bunu mu demek istediniz: --desen)\n",
        ),
    ] {
        let cikti = veylog(&[secenekler, &["ornekler/laravel.log"]].concat());
        assert_eq!(cikti.status.code(), Some(2), "{secenekler:?}");
        assert!(cikti.stdout.is_empty());
        let stderr = metin(&cikti.stderr);
        assert!(stderr.starts_with(beklenen), "{secenekler:?}: {stderr}");
    }
}

#[test]
fn kayit_bulunamazsa_uyarilir() {
    let yol = gecici_dosya("bicimsiz.log", ozel_bicimde(&kuyruk_artisi()).as_bytes());
    let yol_metni = yol.to_str().unwrap();
    for (secenekler, uyari) in [
        (
            &[][..],
            "içinde Laravel'in biçiminde kayıt yok; log başka bir biçimdeyse --desen ile tanıtılabilir.\n",
        ),
        (
            &["--desen", "%datetime% <%level_name%> %message%"][..],
            "içinde --desen'e uyan satır yok; hiçbir kayıt okunmadı.\n",
        ),
        (
            &["--zaman-bicimi", "Y.m.d H:i:s"][..],
            "içinde zamanı --zaman-bicimi'ne uyan kayıt yok; hiçbir kayıt okunmadı.\n",
        ),
    ] {
        let cikti = veylog(&[secenekler, &[yol_metni]].concat());
        assert!(cikti.status.success(), "{secenekler:?}");
        assert_eq!(
            metin(&cikti.stderr),
            format!("Uyarı: {yol_metni} {uyari}"),
            "{secenekler:?}"
        );
        assert!(metin(&cikti.stdout).contains("  Kayıt         : 0\n"));
    }
    fs::remove_file(&yol).ok();
    // Boş dosyada uyarı yok
    let bos = gecici_dosya("bos-uyarisiz.log", b"");
    let cikti = veylog(&[bos.to_str().unwrap()]);
    fs::remove_file(&bos).ok();
    assert!(cikti.stderr.is_empty(), "{}", metin(&cikti.stderr));
}
