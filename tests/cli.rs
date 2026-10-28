//! Programı gerçekten çalıştıran testler: argümanlar, hata mesajları ve çıkış kodları.
//!
//! Cargo, derlenmiş programın yolunu `CARGO_BIN_EXE_veylog` ortam değişkeniyle verir.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{self, Command, Output, Stdio};

/// Derlenmiş program. Arayüzün dilini seçen ortam değişkeni silinir: testleri çalıştıranın
/// ortamında `VEYLOG_DIL=en` olsa da Türkçe çıktı beklenen testler bozulmasın.
fn komut() -> Command {
    let mut komut = Command::new(env!("CARGO_BIN_EXE_veylog"));
    komut.env_remove("VEYLOG_DIL");
    komut
}

fn veylog(argumanlar: &[&str]) -> Output {
    komut()
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
        stdout.contains("Drain ile 24 kalıp (eşik 0,8) → parmak iziyle 25 grup"),
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
        stdout.ends_with("  … 22 satır daha; tümünü görmek için --hepsi\n"),
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
    let cikti = komut().arg(ad).output().expect("veylog çalıştırılamadı");
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
    let mut surec = komut()
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
    let mut surec = komut()
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
        stdout.contains("| Adet | Seviye | İlk | Son | Kalıp | Örnek | Değerler | Konum |\n"),
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
        let mut surec = komut()
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
    let mut surec = komut()
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
            "ornekler/laravel.log: 205 satır\n  Kayıt         : 59\n  Devam satırı  : 143\n  Yetim satır   : 3\n  Filtre        : ERROR ve üstü: 18 kayıt\n"
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
    let mut surec = komut()
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
            "konum: database/migrations/2026_10_25_090000_create_kuponlar_table.php:11, up() (QueryException)\n"
        ),
        "{stdout}"
    );
    // Aynı hata biri metin, biri JSON biçiminde iki kez yazıldı: tek satırda birleşir. JSON'un
    // yığın izinde çağrılar yok; satırın fonksiyonu metin kaydının yığın izinden öğrenilir.
    assert!(
        stdout.contains(
            "     2  ERROR      09:20:11  10:15:30  Undefined array key \"kargo_ucreti\"\n                                       konum: app/Services/SepetHesaplayici.php:10, kargoUcreti() (ErrorException)\n"
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
            "satir": 22,
            "metot": null
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
            "satir": null,
            "metot": null
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
            "  adet  seviye     ilk       son       kalıp\n     2  ERROR      14:03:11  14:05:40  Ödeme alınamadı: sipariş <SAYI>\n                                       örnek: Ödeme alınamadı: sipariş 1047\n                                       konum: app/Services/OdemeServisi.php:64, tahsilEt() (RuntimeException)\n"
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

/// Bir dağıtımı aşan log. Dağıtımdan sonra `SiparisServisi.php`'nin başına dört satır
/// eklenmiş: aynı iki hata başka satırlarda. Biri `DB::transaction()`'a verilen bir closure'da.
/// Bir JSON kanalı da hatalardan birini yığın izi olmadan yazıyor.
fn dagitim_logu() -> String {
    let kayitlar = |saat: &str, kayma: u32| {
        format!(
            r#"[2026-10-27 {saat}:00] production.ERROR: Sipariş işlenemedi {{"exception":"[object] (RuntimeException(code: 0): Sipariş işlenemedi at /var/www/magaza/app/Services/SiparisServisi.php:{closure_ici})
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Concerns/ManagesTransactions.php(35): App\\Services\\SiparisServisi->App\\Services\\{{closure}}()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Support/Facades/Facade.php(364): Illuminate\\Database\\DatabaseManager->__call()
#2 /var/www/magaza/app/Services/SiparisServisi.php({closure_tanimi}): Illuminate\\Support\\Facades\\Facade::__callStatic()
#3 /var/www/magaza/app/Http/Controllers/SiparisController.php(11): App\\Services\\SiparisServisi->olustur()
#4 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/ControllerDispatcher.php(46): App\\Http\\Controllers\\SiparisController->olustur()
#5 {{main}}
"}}
[2026-10-27 {saat}:05] production.ERROR: Undefined array key "kargo_ucreti" {{"exception":"[object] (ErrorException(code: 0): Undefined array key \"kargo_ucreti\" at /var/www/magaza/app/Services/SiparisServisi.php:{kargo})
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Foundation/Bootstrap/HandleExceptions.php(265): Illuminate\\Foundation\\Bootstrap\\HandleExceptions->handleError()
#1 /var/www/magaza/app/Services/SiparisServisi.php({kargo}): Illuminate\\Foundation\\Bootstrap\\HandleExceptions->Illuminate\\Foundation\\Bootstrap\\{{closure}}()
#2 /var/www/magaza/app/Http/Controllers/SiparisController.php(23): App\\Services\\SiparisServisi->kargoUcreti()
#3 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/ControllerDispatcher.php(46): App\\Http\\Controllers\\SiparisController->kargo()
#4 {{main}}
"}}
"#,
            closure_ici = 15 + kayma,
            closure_tanimi = 12 + kayma,
            kargo = 34 + kayma,
        )
    };
    let json = r#"{"message":"Undefined array key \"kargo_ucreti\"","context":{"exception":{"class":"ErrorException","message":"Undefined array key \"kargo_ucreti\"","code":0,"file":"/var/www/magaza/app/Services/SiparisServisi.php:38"}},"level":400,"level_name":"ERROR","channel":"production","datetime":"2026-10-27T11:00:10.000000+00:00","extra":{}}"#;
    format!("{}{}{json}\n", kayitlar("10:00", 0), kayitlar("11:00", 4))
}

#[test]
fn satirlar_kaysa_da_hata_ayni_kalir() {
    let yol = gecici_dosya("dagitim.log", dagitim_logu().as_bytes());
    let dosya = yol.to_str().unwrap();
    let metot = veylog(&["--hepsi", "--karsilastir", "10:30", dosya]);
    let satir = veylog(&[
        "--hepsi",
        "--karsilastir",
        "10:30",
        "--konum",
        "satir",
        dosya,
    ]);
    let json = veylog(&["-b", "json", dosya]);
    let gecersiz = veylog(&["--konum", "dosya", dosya]);
    fs::remove_file(&yol).ok();

    // Varsayılan: konum fonksiyonla belirlenir. Dağıtımdan önceki ve sonraki kayıtlar aynı
    // hatadır; gösterilen satır en yeni kaydınki. JSON kaydının fonksiyonu metin kayıtlarının
    // yığın izinden bulunur.
    assert!(metot.status.success());
    let stdout = metin(&metot.stdout);
    assert!(stdout.contains("→ parmak iziyle 2 grup\n"), "{stdout}");
    assert!(
        stdout.contains(
            "     3  ERROR      10:00:05  11:00:10  Undefined array key \"kargo_ucreti\"\n                                       konum: app/Services/SiparisServisi.php:38, kargoUcreti() (ErrorException)\n"
        ),
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "     2  ERROR      10:00:00  11:00:00  Sipariş işlenemedi\n                                       konum: app/Services/SiparisServisi.php:19, {closure:olustur()} (RuntimeException)\n"
        ),
        "{stdout}"
    );
    assert!(
        stdout
            .contains("Yeni çıkanlar (yalnızca sonra): yok\n\nKaybolanlar (yalnızca önce): yok\n"),
        "{stdout}"
    );

    // `--konum satir`: dosya ve satır. Her hata dağıtımdan sonra yeniymiş gibi görünür.
    assert!(satir.status.success());
    let stdout = metin(&satir.stdout);
    assert!(stdout.contains("→ parmak iziyle 4 grup\n"), "{stdout}");
    assert!(
        stdout.contains("Yeni çıkanlar (yalnızca sonra): 2\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Kaybolanlar (yalnızca önce): 2\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("konum: app/Services/SiparisServisi.php:15 (RuntimeException)\n"),
        "{stdout}"
    );

    let rapor: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(rapor["kaliplar"]["konum"], "metot");
    let kargo = &rapor["kaliplar"]["satirlar"][0];
    assert_eq!(kargo["adet"], 3);
    assert_eq!(
        kargo["istisna"],
        serde_json::json!({
            "sinif": "ErrorException",
            "dosya": "app/Services/SiparisServisi.php",
            "satir": 38,
            "metot": r"App\Services\SiparisServisi->kargoUcreti"
        })
    );
    assert_eq!(
        rapor["kaliplar"]["satirlar"][1]["istisna"]["metot"],
        r"App\Services\SiparisServisi->{closure:olustur()}"
    );

    assert_eq!(gecersiz.status.code(), Some(2));
    assert!(gecersiz.stdout.is_empty());
    assert!(
        metin(&gecersiz.stderr).contains(
            "veylog: --konum için geçersiz değer \"dosya\": şunlardan biri olmalı: metot, satir"
        ),
        "{}",
        metin(&gecersiz.stderr)
    );
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
    let mut surec = komut()
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

#[test]
fn kategorik_konumlar_raporda_degerleriyle_gorunur() {
    // Aynı biçimdeki kodsal değerler (sınıf adları, sabitler, olay adları, tırnaklı anahtarlar)
    // 100 farklı değer beklenmeden birleşir; raporda değerleri adetleriyle yazılır
    let cikti = veylog(&["ornekler/kuyruk.log"]);
    let stdout = metin(&cikti.stdout);
    assert!(cikti.status.success(), "{stdout}");
    assert!(
        stdout.contains("Drain ile 6 kalıp (eşik 0,7) → parmak iziyle 6 grup\n"),
        "{stdout}"
    );
    let beklenen = "\
     4  INFO       09:00:03  09:01:12  İş tamamlandı: <*>
                                       örnek: İş tamamlandı: App\\Jobs\\FaturaGonder
                                       değerler: App\\Jobs\\FaturaGonder ×2, App\\Jobs\\EpostaGonder ×1,
                                                 App\\Jobs\\StokGuncelle ×1
     3  INFO       09:01:30  09:02:02  Olay işlendi: <*>
                                       örnek: Olay işlendi: siparis.olusturuldu
                                       değerler: siparis.olusturuldu ×2, odeme.alindi ×1
";
    assert!(stdout.contains(beklenen), "{stdout}");
    assert!(
        stdout.contains("Sipariş <SAYI> durumu <*> olarak kaldı\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("değerler: ORDER_PENDING ×1, ORDER_FAILED ×1\n"),
        "{stdout}"
    );
    assert!(
        stdout.contains("değerler: \"kargo_ucreti\" ×1, \"indirim\" ×1\n"),
        "{stdout}"
    );
    // Sıradan kelimeler (kuyruk adları) birleşmez
    assert!(stdout.contains("  Kuyruk durdu: emails\n"), "{stdout}");
    assert!(stdout.contains("  Kuyruk durdu: default\n"), "{stdout}");

    // JSON: her <*> konumu için değerler; joker olmayan kalıpta boş dizi
    let cikti = veylog(&["-b", "json", "ornekler/kuyruk.log"]);
    assert!(cikti.status.success());
    let rapor: serde_json::Value = serde_json::from_slice(&cikti.stdout).unwrap();
    let satirlar = rapor["kaliplar"]["satirlar"].as_array().unwrap();
    let isler = satirlar
        .iter()
        .find(|satir| satir["kalip"] == "İş tamamlandı: <*>")
        .unwrap();
    assert_eq!(
        isler["degiskenler"],
        serde_json::json!([{
            "konum": 3,
            "degerler": [
                {"deger": "App\\Jobs\\FaturaGonder", "adet": 2},
                {"deger": "App\\Jobs\\EpostaGonder", "adet": 1},
                {"deger": "App\\Jobs\\StokGuncelle", "adet": 1}
            ],
            "baska": 0
        }])
    );
    let kuyruk = satirlar
        .iter()
        .find(|satir| satir["kalip"] == "Kuyruk durdu: emails")
        .unwrap();
    assert_eq!(kuyruk["degiskenler"], serde_json::json!([]));

    // Markdown: değerler sütunu, her değer kod olarak
    let cikti = veylog(&["-b", "markdown", "ornekler/kuyruk.log"]);
    let stdout = metin(&cikti.stdout);
    assert!(
        stdout.contains(
            "| `Olay işlendi: <*>` | `Olay işlendi: siparis.olusturuldu` | `siparis.olusturuldu` ×2, `odeme.alindi` ×1 |  |\n"
        ),
        "{stdout}"
    );
}

/// Canlı izlemede (`--izle`) çizimlerin arasındaki ayırıcı: `izle` modülündeki `AYIRICI`.
const AYIRICI: &str = "────────────────────────────────────────";

/// İzleme modunda çalışan veylog: çıktısı ayrı bir iş parçacığında sürekli okunur, testler
/// beklenen çizimi bekleyerek yoklar (yavaş makinede sabit bir bekleme yetmeyebilir).
struct Izlenen {
    surec: process::Child,
    stdout: Arc<Mutex<Vec<u8>>>,
    stderr: Arc<Mutex<Vec<u8>>>,
}

use std::io::Read;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// Çıktıyı bitene kadar okuyup biriktiren iş parçacığı.
fn biriktir(mut okunan: impl Read + Send + 'static) -> Arc<Mutex<Vec<u8>>> {
    let biriken = Arc::new(Mutex::new(Vec::new()));
    let yazilan = Arc::clone(&biriken);
    thread::spawn(move || {
        let mut parca = [0u8; 4096];
        while let Ok(okunan) = okunan.read(&mut parca) {
            if okunan == 0 {
                break;
            }
            yazilan.lock().unwrap().extend_from_slice(&parca[..okunan]);
        }
    });
    biriken
}

impl Izlenen {
    fn baslat(argumanlar: &[&str]) -> Izlenen {
        let mut surec = komut()
            .args(argumanlar)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("veylog çalıştırılamadı");
        let stdout = biriktir(surec.stdout.take().unwrap());
        let stderr = biriktir(surec.stderr.take().unwrap());
        Izlenen {
            surec,
            stdout,
            stderr,
        }
    }

    fn stdout(&self) -> String {
        metin(&self.stdout.lock().unwrap())
    }

    fn stderr(&self) -> String {
        metin(&self.stderr.lock().unwrap())
    }

    /// Çıktı koşulu sağlayana kadar bekler; 30 saniyede sağlamazsa çıktıyla birlikte durur.
    fn bekle(&self, aciklama: &str, kosul: impl Fn(&str) -> bool) {
        let baslangic = Instant::now();
        while !kosul(&self.stdout()) {
            assert!(
                baslangic.elapsed() < Duration::from_secs(30),
                "{aciklama} beklenirken zaman aşımı\nstdout:\n{}\nstderr:\n{}",
                self.stdout(),
                self.stderr()
            );
            thread::sleep(Duration::from_millis(50));
        }
    }

    /// Standart girdiye yazar.
    fn yaz(&mut self, baytlar: &[u8]) {
        self.surec
            .stdin
            .as_mut()
            .unwrap()
            .write_all(baytlar)
            .unwrap();
    }

    /// Standart girdiyi kapatır ve sürecin bitmesini bekler; çıkış kodunu ve çıktıları verir.
    fn girdiyi_kapat(mut self) -> (Option<i32>, String, String) {
        drop(self.surec.stdin.take());
        let durum = self.surec.wait().unwrap();
        // Çıktı okuyucuları boru kapanınca biter; birikeni almadan önce kısa bir an verilir
        thread::sleep(Duration::from_millis(100));
        (durum.code(), self.stdout(), self.stderr())
    }

    /// Süreci sonlandırır; çıktıları verir.
    fn bitir(mut self) -> (String, String) {
        self.surec.kill().ok();
        self.surec.wait().ok();
        thread::sleep(Duration::from_millis(100));
        (self.stdout(), self.stderr())
    }
}

/// Son çizim: ayırıcıdan sonraki kısım.
fn son_cizim(cikti: &str) -> &str {
    cikti
        .rsplit_once(&format!("{AYIRICI}\n"))
        .map_or(cikti, |(_, son)| son)
}

/// Çizimin durum satırından sonrası: olağan rapor.
fn rapor_kismi(cizim: &str) -> &str {
    cizim.split_once('\n').map_or("", |(_, rapor)| rapor)
}

/// Örnek logu üç parçaya böler: sınırlar satır ortasına düşer.
fn ornek_parcalari() -> (Vec<u8>, [std::ops::Range<usize>; 3]) {
    let log = fs::read("ornekler/laravel.log").unwrap();
    let (bir, iki) = (log.len() / 3 + 7, 2 * log.len() / 3 + 11);
    assert_ne!(log[bir - 1], b'\n');
    assert_ne!(log[iki - 1], b'\n');
    let uzunluk = log.len();
    (log, [0..bir, bir..iki, iki..uzunluk])
}

#[test]
fn izleme_dosyaya_eklenenleri_okur_ve_normal_raporla_ayni_sonucu_verir() {
    let (log, parcalar) = ornek_parcalari();
    let yol = gecici_dosya("izlenen.log", &log[parcalar[0].clone()]);
    let yol_metni = yol.to_str().unwrap();
    let izlenen = Izlenen::baslat(&["--izle", "--aralik", "500ms", yol_metni]);
    izlenen.bekle("ilk çizim", |cikti| cikti.starts_with("İzleniyor: "));
    let mut dosya = fs::OpenOptions::new().append(true).open(&yol).unwrap();
    for parca in &parcalar[1..] {
        dosya.write_all(&log[parca.clone()]).unwrap();
        dosya.flush().unwrap();
        thread::sleep(Duration::from_millis(50));
    }
    drop(dosya);
    let normal = metin(&veylog(&[yol_metni]).stdout);
    assert!(normal.contains("  Kayıt         : 59\n"), "{normal}");
    izlenen.bekle("tam rapor", |cikti| rapor_kismi(son_cizim(cikti)) == normal);
    let (stdout, stderr) = izlenen.bitir();
    fs::remove_file(&yol).ok();
    assert!(stderr.is_empty(), "{stderr}");
    // Terminal değil: çizimler ayırıcıyla art arda, ilk çizimden önce ayırıcı yok
    assert!(stdout.starts_with("İzleniyor: "), "{stdout}");
    assert!(
        stdout.contains(&format!("\n{AYIRICI}\nİzleniyor: ")),
        "{stdout}"
    );
    let son = son_cizim(&stdout);
    assert!(
        son.starts_with(&format!(
            "İzleniyor: {yol_metni} · her 500 ms · 59 kayıt · son kayıt 2026-10-24 18:31:07\n"
        )),
        "{son}"
    );
    assert_eq!(rapor_kismi(son), normal);
}

#[test]
fn izleme_standart_girdide_girdi_bitince_son_raporu_yazip_cikar() {
    let (log, parcalar) = ornek_parcalari();
    // Son satırın sonu yazılmaz: girdi bitince bekleyen satır da okunmalı
    assert_eq!(log.last(), Some(&b'\n'));
    let log = &log[..log.len() - 1];
    let mut izlenen = Izlenen::baslat(&["--izle", "--aralik", "200ms", "-"]);
    for parca in &parcalar {
        izlenen.yaz(&log[parca.start..parca.end.min(log.len())]);
        thread::sleep(Duration::from_millis(50));
    }
    let (kod, stdout, stderr) = izlenen.girdiyi_kapat();
    assert_eq!(kod, Some(0), "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    let normal = komut()
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .and_then(|mut surec| {
            surec.stdin.take().unwrap().write_all(log)?;
            surec.wait_with_output()
        })
        .unwrap();
    let normal = metin(&normal.stdout);
    assert!(
        normal.starts_with("<standart girdi>: 205 satır\n"),
        "{normal}"
    );
    let son = son_cizim(&stdout);
    assert!(
        son.starts_with("İzleniyor: <standart girdi> · her 200 ms · "),
        "{son}"
    );
    assert_eq!(rapor_kismi(son), normal);
}

#[test]
fn izleme_standart_girdide_ani_artisin_ornegi_kopyadan_okunur() {
    // Ani artışların örnekleri girdinin geçici kopyasından okunur. İlk parçadaki artışın
    // örneği parçaların arasındaki çizimde okunur; okuma kopyanın konumunu değiştirir (aynı
    // dosya tutamacı) ama ikinci parça yine kopyanın sonuna yazılmalı ki oradaki artışın
    // örneği doğru okunsun. Kopya okuma tamponundan (64 KB) büyük olsun: okuma dosyanın
    // ortasında kalsın.
    let zaman = |dakika: u32| {
        let (gun, kalan) = (dakika / 1_440, dakika % 1_440);
        format!("2026-10-{:02} {:02}:{:02}", 1 + gun, kalan / 60, kalan % 60)
    };
    let mut satirlar = Vec::new();
    for dakika in (0..20 * 1_440).step_by(10) {
        satirlar.push(format!(
            "[{}:30] local.WARNING: Kuyruk {} dolu",
            zaman(dakika),
            1_000 + dakika
        ));
    }
    for (dakika, ilk) in [(6 * 60, 50), (19 * 1_440 + 6 * 60, 70)] {
        for saniye in 0..8 {
            satirlar.push(format!(
                "[{}:{:02}] local.WARNING: Kuyruk {} dolu",
                zaman(dakika),
                saniye * 5 + 1,
                ilk + saniye
            ));
        }
    }
    satirlar.sort();
    let log = satirlar.join("\n");
    assert!(log.len() > 128 * 1024, "{}", log.len());
    let bolme = log.find("[2026-10-13 00:00:30]").unwrap() + 7;
    assert!(bolme > 64 * 1024, "{bolme}");
    let mut izlenen = Izlenen::baslat(&["--izle", "--aralik", "200ms", "-a", "1", "-"]);
    izlenen.yaz(&log.as_bytes()[..bolme]);
    // İlk parçanın tamamı okunup çizilsin: çizim ilk artışın örneğini kopyadan okur
    let ilk_parca = log[..bolme].matches('\n').count();
    izlenen.bekle("ilk parçanın raporu", |cikti| {
        let son = son_cizim(cikti);
        son.contains(&format!(" · {ilk_parca} kayıt · ")) && son.contains("örnek: Kuyruk 50 dolu\n")
    });
    izlenen.yaz(&log.as_bytes()[bolme..]);
    let (kod, stdout, stderr) = izlenen.girdiyi_kapat();
    assert_eq!(kod, Some(0), "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    let son = son_cizim(&stdout);
    let (kaliplar, artislar) = son.split_once("\nAni artışlar").unwrap();
    assert!(kaliplar.contains("örnek: Kuyruk 1000 dolu\n"), "{son}");
    assert!(artislar.contains("örnek: Kuyruk 50 dolu\n"), "{son}");
    assert!(artislar.contains("örnek: Kuyruk 70 dolu\n"), "{son}");
}

#[test]
fn izleme_kisalan_dosyayi_bastan_okur_depoyu_korur() {
    let (log, parcalar) = ornek_parcalari();
    // İlk parça bir kayıt başlığıyla bitsin: kayıt sayısı kesin olsun
    let ilk = &log[..log[parcalar[0].clone()]
        .iter()
        .rposition(|&bayt| bayt == b'\n')
        .unwrap()
        + 1];
    let yol = gecici_dosya("kisalan-izlenen.log", ilk);
    let yol_metni = yol.to_str().unwrap();
    let ilk_kayit = metin(&veylog(&[yol_metni]).stdout);
    let ilk_kayit = ilk_kayit
        .split_once("  Kayıt         : ")
        .unwrap()
        .1
        .split_once('\n')
        .unwrap()
        .0
        .parse::<usize>()
        .unwrap();
    let izlenen = Izlenen::baslat(&["--izle", "--aralik", "200ms", yol_metni]);
    izlenen.bekle("ilk dosyanın raporu", |cikti| {
        son_cizim(cikti).contains(&format!(" · {ilk_kayit} kayıt · "))
    });
    // copytruncate: dosya sıfırlanıp yeni kayıtlar yazılır
    let yeni = "[2026-10-24 19:00:00] production.ERROR: Döndürmeden sonraki ilk hata\n";
    fs::write(&yol, yeni).unwrap();
    izlenen.bekle("döndürme", |cikti| {
        son_cizim(cikti).contains(&format!(
            " · {} kayıt · son kayıt 2026-10-24 19:00:00 · döndürüldü\n",
            ilk_kayit + 1
        ))
    });
    let (stdout, stderr) = izlenen.bitir();
    fs::remove_file(&yol).ok();
    assert!(stderr.is_empty(), "{stderr}");
    let son = son_cizim(&stdout);
    assert!(son.contains("Döndürmeden sonraki ilk hata"), "{son}");
    // Eski dosyanın kayıtları duruyor
    assert!(
        son.contains("Zaman aralığı : 2026-10-24 08:59:58 → 2026-10-24 19:00:00"),
        "{son}"
    );
}

#[cfg(unix)]
#[test]
fn izleme_yeniden_adlandirilan_dosyanin_yerine_aciliani_okur() {
    let yol = gecici_dosya(
        "donen-izlenen.log",
        b"[2026-10-24 10:00:00] production.ERROR: Eski dosyadaki hata\n",
    );
    let yol_metni = yol.to_str().unwrap();
    let izlenen = Izlenen::baslat(&["--izle", "--aralik", "200ms", yol_metni]);
    izlenen.bekle("ilk dosyanın raporu", |cikti| {
        son_cizim(cikti).contains(" · 1 kayıt · ")
    });
    let yedek = yol.with_extension("log.1");
    fs::rename(&yol, &yedek).unwrap();
    // Döndürmenin ortası: yol yok, izleme sürer
    thread::sleep(Duration::from_millis(400));
    fs::write(
        &yol,
        b"[2026-10-24 10:05:00] production.ERROR: Yeni dosyadaki hata\n",
    )
    .unwrap();
    izlenen.bekle("yeni dosya", |cikti| {
        son_cizim(cikti).contains(" · 2 kayıt · son kayıt 2026-10-24 10:05:00 · döndürüldü\n")
    });
    let (stdout, stderr) = izlenen.bitir();
    fs::remove_file(&yol).ok();
    fs::remove_file(&yedek).ok();
    assert!(stderr.is_empty(), "{stderr}");
    let son = son_cizim(&stdout);
    assert!(
        son.contains("Eski dosyadaki hata") && son.contains("Yeni dosyadaki hata"),
        "{son}"
    );
}

#[test]
fn izleme_secenekleri_denetlenir() {
    for (argumanlar, hata) in [
        (
            &["--izle", "--bicim", "json", "ornekler/laravel.log"][..],
            "--bicim json ile --izle birlikte kullanılamaz: izleme yalnızca metin biçimindedir",
        ),
        (
            &["--izle", "--karsilastir", "15:00", "ornekler/laravel.log"][..],
            "--karsilastir ile --izle birlikte kullanılamaz: izlenen log sürekli uzar",
        ),
        (
            &["--aralik", "1sn", "ornekler/laravel.log"][..],
            "--aralik yalnızca --izle ile kullanılabilir",
        ),
        (
            &["--izle", "--aralik", "50ms", "ornekler/laravel.log"][..],
            "--aralik için geçersiz değer \"50ms\": en az 100 ms olan bir süre olmalı",
        ),
        (
            &["--izle=evet", "ornekler/laravel.log"][..],
            "--izle değer almaz",
        ),
    ] {
        let cikti = veylog(argumanlar);
        assert_eq!(cikti.status.code(), Some(2), "{argumanlar:?}");
        assert!(cikti.stdout.is_empty(), "{argumanlar:?}");
        let stderr = metin(&cikti.stderr);
        assert!(stderr.contains(hata), "{argumanlar:?}\n{stderr}");
    }
    // Olmayan dosya izlenemez: okuma hatası
    let cikti = veylog(&["--izle", "olmayan-izlenecek.log"]);
    assert_eq!(cikti.status.code(), Some(1));
    assert!(metin(&cikti.stderr).contains("olmayan-izlenecek.log okunamadı: dosya bulunamadı"));
    // Yardımda var
    let yardim = metin(&veylog(&["--yardim"]).stdout);
    assert!(yardim.contains("      --izle "), "{yardim}");
    assert!(yardim.contains("      --aralik <süre>"), "{yardim}");
}

/// İngilizce içerikli kurgusal bir log: dil testlerinde raporda Türkçe bir şey kalmadığı
/// görülebilsin diye mesajlarında Türkçe harf ve kelime yok. Yetim satır, istisna, JSON
/// satırı (başka bir saat diliminde), kategorik değerler ve bir ani artış içerir.
fn ingilizce_log() -> String {
    let mut satirlar = vec!["stray line before the first record".to_owned()];
    for saat in 8..20 {
        satirlar.push(format!(
            "[2026-10-24T{saat:02}:00:00+03:00] production.INFO: User {} logged in {{\"ip\":\"198.51.100.7\"}}",
            100 + saat
        ));
        satirlar.push(format!(
            "[2026-10-24T{saat:02}:30:00+03:00] production.WARNING: Slow query ({} ms): select * from orders where user_id = {saat}",
            1_000 + saat
        ));
    }
    for (dakika, is) in [
        (5, "SendInvoice"),
        (6, "SendEmail"),
        (7, "SendInvoice"),
        (8, "SyncStock"),
    ] {
        satirlar.push(format!(
            "[2026-10-24T09:{dakika:02}:00+03:00] production.INFO: Job processed: App\\Jobs\\{is}"
        ));
    }
    satirlar.push(
        "[2026-10-24T11:15:00+03:00] production.ERROR: Undefined array key \"shipping\" {\"exception\":\"[object] (ErrorException(code: 0): Undefined array key \\\"shipping\\\" at /var/www/shop/app/Services/Cart.php:88)\n[stacktrace]\n#0 /var/www/shop/vendor/laravel/framework/src/Illuminate/Foundation/Bootstrap/HandleExceptions.php(256): Illuminate\\\\Foundation\\\\Bootstrap\\\\HandleExceptions->handleError(2, 'Undefined array...', '/var/www/shop/a...', 88)\n#1 /var/www/shop/app/Services/Cart.php(88): Illuminate\\\\Foundation\\\\Bootstrap\\\\HandleExceptions->Illuminate\\\\Foundation\\\\Bootstrap\\\\{closure}(2, 'Undefined array...', '/var/www/shop/a...', 88)\n#2 /var/www/shop/app/Http/Controllers/CartController.php(21): App\\\\Services\\\\Cart->total()\n#3 {main}\n\"}".to_owned(),
    );
    satirlar.push(
        r#"{"message":"Payment provider timed out","context":{},"level":400,"level_name":"ERROR","channel":"production","datetime":"2026-10-24T13:20:00+00:00","extra":{}}"#.to_owned(),
    );
    for saniye in 0..8 {
        satirlar.push(format!(
            "[2026-10-24T16:10:{:02}+03:00] production.ERROR: Queue full: {} jobs waiting",
            saniye * 5,
            50 + saniye
        ));
    }
    satirlar.join("\n") + "\n"
}

/// Logu standart girdiden okuyan veylog: dosya adı (geçici klasörün yolu Türkçe harf
/// içerebilir) rapora girmesin.
fn girdiyle(argumanlar: &[&str], girdi: &str, ortam: &[(&str, &str)]) -> Output {
    let mut surec = komut()
        .args(argumanlar)
        .envs(ortam.iter().copied())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("veylog çalıştırılamadı");
    surec
        .stdin
        .take()
        .unwrap()
        .write_all(girdi.as_bytes())
        .unwrap();
    surec.wait_with_output().unwrap()
}

/// Türkçe arayüzün kelimeleri ve harfleri: İngilizce çıktıda hiçbiri olmamalı.
fn turkce_kalintilar(metin: &str) -> Vec<String> {
    const KELIMELER: [&str; 34] = [
        "kayit",
        "satir",
        "kalip",
        "kaliplar",
        "ornek",
        "konum",
        "seviye",
        "seviyeler",
        "filtre",
        "zaman",
        "araligi",
        "dilim",
        "dilimi",
        "grup",
        "esik",
        "sonra",
        "artis",
        "zirve",
        "olagan",
        "puan",
        "yok",
        "baska",
        "daha",
        "icin",
        "ve",
        "ile",
        "sa",
        "dk",
        "sn",
        "kat",
        "saatte",
        "uyari",
        "izleniyor",
        "hepsi",
    ];
    let mut bulunan: Vec<String> = metin
        .chars()
        .filter(|harf| "çğıöşüÇĞİÖŞÜ".contains(*harf))
        .map(|harf| format!("harf {harf}"))
        .collect();
    for kelime in metin.split(|harf: char| !harf.is_alphanumeric()) {
        if KELIMELER.contains(&kelime.to_lowercase().as_str()) {
            bulunan.push(format!("kelime {kelime}"));
        }
    }
    bulunan
}

#[test]
fn ingilizce_arayuzde_turkce_kalmaz() {
    let log = ingilizce_log();
    for argumanlar in [
        &["--lang", "en", "-"][..],
        &[
            "--lang",
            "en",
            "--spikes",
            "1",
            "--compare",
            "12:00",
            "--top",
            "2",
            "-",
        ],
        &["--lang", "en", "--level", "warning", "--last", "3h", "-"],
        &[
            "--lang",
            "en",
            "--since",
            "09:00",
            "--until",
            "18:00",
            "--timezone",
            "+00:00",
            "-",
        ],
        &[
            "--lang",
            "en",
            "--location",
            "line",
            "--sort",
            "newest",
            "--all",
            "-",
        ],
        &[
            "--lang",
            "en",
            "--format",
            "markdown",
            "--spikes",
            "1",
            "--compare",
            "12:00",
            "--top",
            "1",
            "-",
        ],
        &["--lang", "en", "--spikes", "120", "-"],
        &[
            "--lang",
            "en",
            "--level",
            "emergency",
            "--spikes",
            "1",
            "--compare",
            "12:00",
            "-",
        ],
        &["--lang", "en", "--threshold", "2", "-"],
        &["--lang", "en", "--watch", "--format", "json", "-"],
        &["--lang", "en", "--watch", "--interval", "100ms", "-"],
        &["--lang", "en", "--compare", "2027-01-01", "-"],
        &["--lang", "en", "--help"],
    ] {
        let cikti = girdiyle(argumanlar, &log, &[]);
        let butun = metin(&cikti.stdout) + &metin(&cikti.stderr);
        assert!(!butun.is_empty(), "{argumanlar:?}");
        let mut kalintilar = turkce_kalintilar(&butun);
        // Yardım seçeneklerin Türkçe adlarını ve JSON'un alan adlarını da anar
        if argumanlar.contains(&"--help") {
            kalintilar.retain(|kalinti| kalinti.starts_with("harf"));
        }
        assert_eq!(kalintilar, Vec::<String>::new(), "{argumanlar:?}\n{butun}");
    }
    // Ortam değişkeniyle de; yazılmamış saat dilimi ve kayıt bulunamayan log uyarısı da
    let cikti = girdiyle(&["-"], "no record here\n", &[("VEYLOG_DIL", "en")]);
    let stderr = metin(&cikti.stderr);
    assert_eq!(
        stderr,
        "Warning: <standard input> has no records in Laravel's format; if the log has another format, describe it with --pattern.\n"
    );
    assert!(turkce_kalintilar(&metin(&cikti.stdout)).is_empty());
}

#[test]
fn ingilizce_arayuz() {
    let log = ingilizce_log();
    let cikti = girdiyle(&["--lang", "en", "--spikes", "1", "-"], &log, &[]);
    assert!(cikti.status.success());
    let stdout = metin(&cikti.stdout);
    let beklenen = "\
<standard input>: 45 lines
  Records       : 38
  Continuations : 6
  Orphan lines  : 1
  Time range    : 2026-10-24 08:00:00 → 2026-10-24 19:30:00
  Time zone     : +03:00; 1 record converted from +00:00
";
    assert!(stdout.starts_with(beklenen), "{stdout}");
    for parca in [
        "\nPatterns: 37 distinct messages → 8 after masking → 6 Drain patterns (threshold 0.7) → 6 groups by fingerprint\n",
        "  count  level      first     last      pattern\n",
        "  Queue full: <NUM> jobs waiting\n",
        "values: App\\Jobs\\SendInvoice ×2, App\\Jobs\\SendEmail ×1, App\\Jobs\\SyncStock ×1\n",
        "location: app/Services/Cart.php:88, total() (ErrorException)\n",
        "\nSpikes (WARNING and above, 691 slices of 1 min, score > 3.5): 1\n",
        // JSON satırının +00:00'lık zamanı raporun dilimine çevrildi
        "  1  ERROR      16:20:00  16:20:00  Payment provider timed out\n",
        "spike: 8 records in 16:10–16:11\n",
    ] {
        assert!(stdout.contains(parca), "{parca}\n{stdout}");
    }
    // Ortam değişkeni aynı sonucu verir; --dil ondan önce gelir
    let ortamla = girdiyle(&["--spikes", "1", "-"], &log, &[("VEYLOG_DIL", "en")]);
    assert_eq!(metin(&ortamla.stdout), stdout);
    let turkce = girdiyle(&["--dil", "tr", "-"], &log, &[("VEYLOG_DIL", "en")]);
    assert!(metin(&turkce.stdout).starts_with("<standart girdi>: 45 satır\n"));
    // Tanınmayan değer iki dilde uyarılır, Türkçe sürer
    let tanimsiz = girdiyle(&["-"], &log, &[("VEYLOG_DIL", "english")]);
    assert_eq!(
        metin(&tanimsiz.stderr),
        "Uyarı / Warning: VEYLOG_DIL=\"english\" tanınmadı / not recognized; tr, en\n"
    );
    assert!(metin(&tanimsiz.stdout).starts_with("<standart girdi>: 45 satır\n"));
    // --dil verildiyse ortam değişkenine bakılmaz; boş değişken verilmemiş sayılır
    for (argumanlar, deger) in [(&["--dil", "en", "-"][..], "english"), (&["-"][..], " ")] {
        let cikti = girdiyle(argumanlar, &log, &[("VEYLOG_DIL", deger)]);
        assert!(cikti.stderr.is_empty(), "{}", metin(&cikti.stderr));
    }
    // Hatalar ve çıkış kodları
    let cikti = veylog(&["--lang", "en", "--esik", "2", "ornekler/laravel.log"]);
    assert_eq!(cikti.status.code(), Some(2));
    assert_eq!(
        metin(&cikti.stderr),
        "veylog: invalid value \"2\" for --esik: must be a number between 0 and 1 (e.g. 0.7)\nUsage: veylog [OPTIONS] <log-file>\nFor details: veylog --lang en --help\n"
    );
    let cikti = veylog(&["--lang", "en", "missing.log"]);
    assert_eq!(cikti.status.code(), Some(1));
    assert_eq!(
        metin(&cikti.stderr),
        "veylog: cannot read missing.log: file not found\n"
    );
    let cikti = veylog(&["--lang", "en", "-k", "2027-01-01", "ornekler/laravel.log"]);
    assert_eq!(cikti.status.code(), Some(2));
    assert_eq!(
        metin(&cikti.stderr),
        "veylog: the comparison point 2027-01-01 00:00:00 is not inside the log: records are between 2026-10-24 08:59:58 and 2026-10-24 18:31:07\n"
    );
    // Yardım
    let cikti = veylog(&["--lang", "en", "--help"]);
    assert!(cikti.status.success());
    assert!(metin(&cikti.stdout).contains("\nUsage: veylog [OPTIONS] <log-file>\n"));
}

#[test]
fn json_her_dilde_ayni() {
    // JSON betikler içindir: alan adları, değerler ve yer tutucular dil değişince değişmez
    for argumanlar in [
        &["-b", "json", "ornekler/laravel.log"][..],
        &[
            "-b",
            "json",
            "-a",
            "10",
            "-k",
            "16:11",
            "--seviye",
            "warning",
            "ornekler/laravel.log",
        ],
        &[
            "-b",
            "json",
            "--son",
            "2sa",
            "--konum",
            "satir",
            "--siralama",
            "yeni",
            "ornekler/laravel.log",
        ],
        &["-b", "json", "--hepsi", "ornekler/kuyruk.log"],
    ] {
        let turkce = veylog(argumanlar);
        assert!(turkce.status.success(), "{argumanlar:?}");
        let mut ingilizce_argumanlar = vec!["--lang", "en"];
        ingilizce_argumanlar.extend_from_slice(argumanlar);
        let ingilizce = veylog(&ingilizce_argumanlar);
        assert_eq!(
            metin(&ingilizce.stdout),
            metin(&turkce.stdout),
            "{argumanlar:?}"
        );
    }
}
