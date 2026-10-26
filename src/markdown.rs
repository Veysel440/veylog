//! Markdown çıktısı: özeti GitHub issue'larına ve PR açıklamalarına yapıştırılabilecek bir
//! tablo olarak yazar.
//!
//! Kalıplar, örnekler ve yollar satır içi kod (`...`) olarak yazılır. Böylece `<SAYI>` gibi
//! yer tutucular HTML etiketi sanılıp silinmez, `*` ve `_` vurguya dönüşmez. Logdan gelen
//! metinlerdeki kontrol karakterleri önce görünür sembollere çevrilir: Markdown'da CR de satır
//! sonu sayılır ve tablonun satırını ikiye böler.

use std::io::{self, Write};

use crate::rapor::{KALIP_GENISLIGI, KalipSatiri, Ozet, gorunur, kisalt, zaman_goster};

/// Özeti Markdown olarak yazar. `--ilk` ve `--siralama` burada da geçerlidir.
pub fn yaz(cikti: &mut impl Write, ozet: &Ozet<'_>) -> io::Result<()> {
    writeln!(cikti, "## veylog: {}", kod(&gorunur(&ozet.dosya)))?;
    writeln!(cikti)?;
    writeln!(
        cikti,
        "- **Satır:** {} · **Kayıt:** {} · **Devam satırı:** {} · **Yetim satır:** {}",
        ozet.satir_sayisi, ozet.kayit_sayisi, ozet.devam_satiri_sayisi, ozet.yetim_satir_sayisi
    )?;
    if let Some((en_eski, en_yeni)) = ozet.zaman_araligi {
        writeln!(cikti, "- **Zaman aralığı:** {en_eski} → {en_yeni}")?;
    }
    let seviyeler: Vec<String> = ozet
        .seviyeler
        .iter()
        .map(|(seviye, adet)| format!("{} {adet}", seviye.ad()))
        .collect();
    writeln!(cikti, "- **Seviyeler:** {}", seviyeler.join(" · "))?;
    writeln!(cikti, "- **Kalıplar:** {}", ozet.kalip_cumlesi())?;

    let satirlar = ozet.gosterilecek_satirlar();
    if !satirlar.is_empty() {
        let tek_gun = ozet.tek_gun();
        writeln!(cikti)?;
        writeln!(
            cikti,
            "| Adet | Seviye | İlk | Son | Kalıp | Örnek | Konum |"
        )?;
        // ---: sütunu sağa yaslar
        writeln!(cikti, "|---:|---|---|---|---|---|---|")?;
        for satir in satirlar {
            let hucreler = [
                satir.adet.to_string(),
                satir.seviye.ad().to_owned(),
                zaman_goster(satir.ilk, tek_gun),
                zaman_goster(satir.son, tek_gun),
                kod(&kisalt(&gorunur(&satir.kalip), KALIP_GENISLIGI)),
                ornek(satir),
                konum(satir),
            ];
            // Her hücre aynı kuraldan geçer; sonradan eklenen bir sütun da unutulmaz
            writeln!(
                cikti,
                "| {} |",
                hucreler.map(|metin| hucre(&metin)).join(" | ")
            )?;
        }
        let gizlenen = ozet.gosterilmeyen();
        if gizlenen > 0 {
            writeln!(cikti)?;
            writeln!(
                cikti,
                "… {gizlenen} satır daha; tümünü görmek için `--hepsi`"
            )?;
        }
    }
    Ok(())
}

/// Örnek hücresi: kalıp ham mesajla aynıysa boş kalır.
fn ornek(satir: &KalipSatiri<'_>) -> String {
    if satir.ornek == satir.kalip {
        String::new()
    } else {
        kod(&kisalt(&gorunur(satir.ornek), KALIP_GENISLIGI))
    }
}

/// Konum hücresi: `app/Services/X.php:88` ErrorException
///
/// Kısa sınıf adı kod dışında yazılabilir: parmak izi yalnızca harf, rakam, `_`, `\` ve `@`
/// içeren sınıf adlarını kabul eder.
fn konum(satir: &KalipSatiri<'_>) -> String {
    match satir.parmak_izi {
        Some(parmak_izi) => match parmak_izi.konum() {
            Some((dosya, satir_no)) => {
                format!(
                    "{} {}",
                    kod(&gorunur(&format!("{dosya}:{satir_no}"))),
                    parmak_izi.kisa_sinif()
                )
            }
            None => format!("uygulama kodunda bulunamadı ({})", parmak_izi.kisa_sinif()),
        },
        None => String::new(),
    }
}

/// Metni Markdown satır içi kodu olarak yazar: `metin`
///
/// Metnin içinde ters tırnak varsa (SQL'deki `siparisler` gibi) tek tırnaklı çerçeve kodu
/// ortadan böler. Çerçeve, metindeki en uzun ters tırnak dizisinden bir uzun olur ve iki yana
/// birer boşluk konur; Markdown bu boşlukları göstermez.
fn kod(metin: &str) -> String {
    if metin.is_empty() {
        return String::new();
    }
    // Ters tırnak olmayan her karakterden böl: geriye yalnızca ters tırnak dizileri kalır
    let en_uzun_dizi = metin
        .split(|harf| harf != '`')
        .map(str::len)
        .max()
        .unwrap_or(0);
    if en_uzun_dizi == 0 {
        format!("`{metin}`")
    } else {
        let cerceve = "`".repeat(en_uzun_dizi + 1);
        format!("{cerceve} {metin} {cerceve}")
    }
}

/// Tablo hücresinin içeriği: `|` sütunları ayırdığı için `\|` yazılır, kodun içinde bile.
/// GitHub bunu `|` olarak gösterir.
///
/// Bu kaçış yalnızca tabloda geçerlidir. Tablo dışındaki kodda (başlıktaki dosya adı) `\|`
/// ters bölüyle birlikte görünürdü; bu yüzden `kod`un değil, hücrenin işidir.
fn hucre(metin: &str) -> String {
    metin.replace('|', "\\|")
}

#[cfg(test)]
mod testler {
    use super::*;
    use crate::kayit::kayitlari_ayir;
    use crate::rapor::RaporAyarlari;
    use std::path::Path;

    const ORNEK_LOG: &str = include_str!("../ornekler/laravel.log");

    /// Logun Markdown raporu.
    fn markdown(log_metni: &str, yol: &str) -> String {
        let log = kayitlari_ayir(log_metni);
        let ozet = Ozet::hesapla(Path::new(yol), log_metni, &log, &RaporAyarlari::default());
        let mut cikti = Vec::new();
        yaz(&mut cikti, &ozet).unwrap();
        String::from_utf8(cikti).unwrap()
    }

    #[test]
    fn kod_cercevesi_ters_tirnaklara_gore_secilir() {
        assert_eq!(
            kod("Kullanıcı <SAYI> giriş yaptı"),
            "`Kullanıcı <SAYI> giriş yaptı`"
        );
        assert_eq!(
            kod("select * from `siparisler`"),
            "`` select * from `siparisler` ``"
        );
        assert_eq!(kod("a``b"), "``` a``b ```");
        assert_eq!(kod(""), "");
    }

    #[test]
    fn dikey_cizgi_yalnizca_hucrede_kacilir() {
        // Tablo dışında (başlıkta) kaçış gerekmez; kaçılsaydı ters bölü görünürdü
        assert_eq!(kod("a|b"), "`a|b`");
        assert_eq!(hucre(&kod("a|b")), r"`a\|b`");
        // Metindeki ters bölü olduğu gibi kalır; GitHub `a\\|b` hücresini a\|b gösterir
        assert_eq!(hucre(&kod(r"a\|b")), r"`a\\|b`");
    }

    #[test]
    fn ornek_log_tablosu() {
        let metin = markdown(ORNEK_LOG, "ornekler/laravel.log");
        assert!(metin.starts_with("## veylog: `ornekler/laravel.log`\n\n"));
        assert!(metin.contains(
            "- **Seviyeler:** CRITICAL 2 · ERROR 16 · WARNING 6 · NOTICE 1 · INFO 31 · DEBUG 3\n"
        ));
        assert!(metin.contains(
            "\n| Adet | Seviye | İlk | Son | Kalıp | Örnek | Konum |\n|---:|---|---|---|---|---|---|\n"
        ));
        // Yer tutucular kod içinde; konum dosya:satır ve kısa sınıf adıyla
        assert!(metin.contains(
            "| 2 | CRITICAL | 17:05:13 | 17:06:45 | `Redis bağlantısı kurulamadı: Connection refused [tcp://<IP>:<SAYI>]` | `Redis bağlantısı kurulamadı: Connection refused [tcp://10.0.3.12:6379]` | `app/Http/Middleware/HizSiniri.php:22` RedisException |\n"
        ));
        // Kalıp ham mesajla aynıysa örnek hücresi boş
        assert!(metin.contains(
            "| 4 | ERROR | 15:02:11 | 16:58:12 | `Undefined array key \"kargo_ucreti\"` |  | `app/Services/SepetHesaplayici.php:88` ErrorException |\n"
        ));
        // SQL'deki ters tırnaklar yüzünden çift çerçeve
        assert!(metin.contains("| `` Yavaş sorgu (<SAYI> ms): select * from `siparisler`"));
        assert!(metin.ends_with("\n… 14 satır daha; tümünü görmek için `--hepsi`\n"));
    }

    #[test]
    fn kontrol_karakterleri_tabloyu_bozmaz() {
        let log_metni = "\
[2026-10-24 10:00:00] local.ERROR: Odeme tamam\rHATA: kart|reddedildi
[2026-10-24 10:00:01] local.INFO: \u{1b}[31mKırmızı\u{1b}[0m yazı
[2026-10-24 10:00:02] local.WARNING: Bozuk yol bulundu {\"exception\":\"[object] (RuntimeException(code: 0): Bozuk yol bulundu at /var/www/app/K\u{1b}]0;x\u{7}otu.php:5)\"}
";
        let metin = markdown(log_metni, "a|b\u{1b}[2J.log");

        let kalanlar: Vec<char> = metin
            .chars()
            .filter(|&harf| harf.is_control() && harf != '\n')
            .collect();
        assert!(kalanlar.is_empty(), "{kalanlar:?}");

        // Başlıkta | kaçılmaz, ESC sembol olur
        assert!(metin.starts_with("## veylog: `a|b␛[2J.log`\n"), "{metin}");
        // CR satırı bölmez: kayıt tek tablo satırında, | kaçılmış
        assert!(
            metin.contains(
                "| 1 | ERROR | 10:00:00 | 10:00:00 | `Odeme tamam HATA: kart\\|reddedildi` | `Odeme tamam␍HATA: kart\\|reddedildi` |  |\n"
            ),
            "{metin}"
        );
        assert!(
            metin.contains("| `␛[31mKırmızı␛[0m yazı` |  |  |\n"),
            "{metin}"
        );
        // Yığın izinden gelen yol da temizlenir
        assert!(
            metin.contains("| `/var/www/app/K␛]0;x␇otu.php:5` RuntimeException |\n"),
            "{metin}"
        );
        // Başlık, ayraç ve üç kayıt: tablo tam beş satır
        let tablo_satirlari = metin.lines().filter(|satir| satir.starts_with('|')).count();
        assert_eq!(tablo_satirlari, 5, "{metin}");
    }
}
