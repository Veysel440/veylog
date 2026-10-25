//! veylog: Laravel log dosyalarını özetleyen komut satırı aracı.

mod drain;
mod kayit;
mod maske;

use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::io::{self, BufWriter, Write};
use std::process;

use drain::{Ayarlar, Drain, Kume};
use kayit::{Log, kayitlari_ayir};
use maske::maskele;

/// Listede gösterilecek en fazla kalıp sayısı.
const GOSTERILECEK_KALIP: usize = 10;
/// Uzun kalıplar bu kadar karakterden sonra kesilir.
const KALIP_GENISLIGI: usize = 90;

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

/// Özeti verilen yere yazar: ekrana (stdout) ya da testlerde bir bayt dizisine.
fn ozet_yaz(cikti: &mut impl Write, yol: &str, icerik: &str, log: &Log<'_>) -> io::Result<()> {
    writeln!(cikti, "{yol}: {} satır", icerik.lines().count())?;
    writeln!(cikti, "  Kayıt         : {}", log.kayitlar.len())?;
    writeln!(cikti, "  Devam satırı  : {}", log.devam_satiri_sayisi())?;
    writeln!(cikti, "  Yetim satır   : {}", log.yetim_satir)?;
    if let Some((en_eski, en_yeni)) = log.zaman_araligi() {
        writeln!(cikti, "  Zaman aralığı : {en_eski} → {en_yeni}")?;
    }

    writeln!(cikti)?;
    writeln!(cikti, "Seviyeler:")?;
    // En önemli seviye en üstte görünsün diye ters sırada yaz
    for (seviye, adet) in log.seviye_sayilari().iter().rev() {
        writeln!(cikti, "  {:<10}{adet:>4}", seviye.ad())?;
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

    writeln!(cikti)?;
    writeln!(
        cikti,
        "Kalıplar: {} farklı mesaj → maskeleyince {} → Drain ile {} kalıp",
        ham_mesajlar.len(),
        maskeli_mesajlar.len(),
        kaliplar.len()
    )?;
    for kume in kaliplar.iter().take(GOSTERILECEK_KALIP) {
        writeln!(
            cikti,
            "  {:>4}  {}",
            kume.adet,
            kisalt(&kume.metin(), KALIP_GENISLIGI)
        )?;
    }

    // Tamponda kalan son satırlar burada yazılır; varsa hatası da burada ortaya çıkar
    cikti.flush()
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

    #[test]
    fn ozet_ornek_log_icin_dogru_yazilir() {
        let icerik = include_str!("../ornekler/laravel.log");
        let log = kayitlari_ayir(icerik);

        // Ekran yerine bir bayt dizisine yaz, sonra metin olarak incele
        let mut cikti = Vec::new();
        ozet_yaz(&mut cikti, "ornek.log", icerik, &log).unwrap();
        let metin = String::from_utf8(cikti).unwrap();

        assert!(metin.starts_with("ornek.log: 203 satır\n"));
        assert!(metin.contains("  Zaman aralığı : 2026-10-24 08:59:58 → 2026-10-24 18:31:07\n"));
        assert!(metin.contains("\nSeviyeler:\n  CRITICAL     2\n  ERROR       16\n"));
        assert!(
            metin.contains("Kalıplar: 45 farklı mesaj → maskeleyince 27 → Drain ile 23 kalıp\n")
        );
        assert!(metin.contains("\n     9  Kullanıcı <SAYI> <*> yaptı\n"));
    }
}
