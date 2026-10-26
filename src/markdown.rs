//! Markdown çıktısı: özeti GitHub issue'larına ve PR açıklamalarına yapıştırılabilecek bir
//! tablo olarak yazar.
//!
//! Kalıplar, örnekler ve yollar satır içi kod (`...`) olarak yazılır. Böylece `<SAYI>` gibi
//! yer tutucular HTML etiketi sanılıp silinmez, `*` ve `_` vurguya dönüşmez. Logdan gelen
//! metinlerdeki kontrol karakterleri önce görünür sembollere çevrilir: Markdown'da CR de satır
//! sonu sayılır ve tablonun satırını ikiye böler.

use std::io::{self, Write};

use crate::ani_artis::AniArtislar;
use crate::karsilastirma::Karsilastirma;
use crate::rapor::{
    Hucreler, KALIP_GENISLIGI, KalipSatiri, Ozet, ZamanGosterimi, ani_artis_basligi, dilim_araligi,
    gorunur, karsilastirma_bolumleri, kisalt, ondalik, ondalik_kisa, zaman_goster,
};
use crate::zaman::sure_yaz;

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
    let gosterim = ozet.zaman_gosterimi();
    if !satirlar.is_empty() {
        writeln!(cikti)?;
        writeln!(
            cikti,
            "| Adet | Seviye | İlk | Son | Kalıp | Örnek | Konum |"
        )?;
        // ---: sütunu sağa yaslar
        writeln!(cikti, "|---:|---|---|---|---|---|---|")?;
        for satir in satirlar {
            satir_yaz(
                cikti,
                &[satir.adet.to_string()],
                satir,
                &Hucreler::satirdan(satir),
                gosterim,
            )?;
        }
        gizlenen_yaz(cikti, ozet.gosterilmeyen())?;
    }

    if let Some(karsilastirma) = &ozet.karsilastirma {
        karsilastirma_yaz(cikti, ozet, karsilastirma, gosterim)?;
    }
    if let Some(ani_artislar) = &ozet.ani_artislar {
        ani_artis_yaz(cikti, ozet, ani_artislar, gosterim)?;
    }
    Ok(())
}

/// Karşılaştırma bölümü: iki yarının özeti ve yeni çıkan, kaybolan, artan kalıpların tabloları.
fn karsilastirma_yaz(
    cikti: &mut impl Write,
    ozet: &Ozet<'_>,
    karsilastirma: &Karsilastirma,
    gosterim: ZamanGosterimi,
) -> io::Result<()> {
    writeln!(cikti)?;
    writeln!(
        cikti,
        "### Karşılaştırma: {} öncesi ve sonrası",
        karsilastirma.nokta
    )?;
    writeln!(cikti)?;
    for (ad, pencere) in [
        ("Önce", &karsilastirma.once),
        ("Sonra", &karsilastirma.sonra),
    ] {
        writeln!(
            cikti,
            "- **{ad}:** {} → {} · {} · {} kayıt, saatte {}",
            pencere.baslangic,
            pencere.bitis,
            sure_yaz(pencere.saniye),
            pencere.kayit_sayisi,
            ondalik_kisa(pencere.saatlik(pencere.kayit_sayisi))
        )?;
    }

    for (baslik, degisimler) in karsilastirma_bolumleri(karsilastirma) {
        writeln!(cikti)?;
        if degisimler.is_empty() {
            writeln!(cikti, "#### {baslik}: yok")?;
            continue;
        }
        writeln!(cikti, "#### {baslik}: {}", degisimler.len())?;
        writeln!(cikti)?;
        writeln!(
            cikti,
            "| Önce | Sonra | Seviye | İlk | Son | Kalıp | Örnek | Konum |"
        )?;
        writeln!(cikti, "|---:|---:|---|---|---|---|---|---|")?;
        for degisim in ozet.sinirla(degisimler) {
            // Artanlarda sonraki sayının yanına kaç kat sık görüldüğü yazılır
            let sonra = if degisim.once > 0 && degisim.sonra > 0 {
                format!("{} ({} kat)", degisim.sonra, ondalik_kisa(degisim.kat))
            } else {
                degisim.sonra.to_string()
            };
            let satir = &ozet.analiz.satirlar[degisim.satir];
            satir_yaz(
                cikti,
                &[degisim.once.to_string(), sonra],
                satir,
                &Hucreler::satirdan(satir),
                gosterim,
            )?;
        }
        gizlenen_yaz(cikti, ozet.disarida_kalan(degisimler))?;
    }
    Ok(())
}

/// Ani artış bölümü: başlık ve puanı en yüksek olan artışların tablosu. Zirve hücresinde zirve
/// dilimin aralığı da yazar: `6 (16:10–16:20)`. Seviye, ilk, son ve örnek artışın
/// kayıtlarındandır.
fn ani_artis_yaz(
    cikti: &mut impl Write,
    ozet: &Ozet<'_>,
    ani_artislar: &AniArtislar,
    gosterim: ZamanGosterimi,
) -> io::Result<()> {
    writeln!(cikti)?;
    writeln!(cikti, "### {}", ani_artis_basligi(ani_artislar))?;
    if ani_artislar.artislar.is_empty() {
        return Ok(());
    }
    writeln!(cikti)?;
    writeln!(
        cikti,
        "| Zirve | Olağan | Puan | Seviye | İlk | Son | Kalıp | Örnek | Konum |"
    )?;
    writeln!(cikti, "|---:|---:|---:|---|---|---|---|---|---|")?;
    for artis in ozet.sinirla(&ani_artislar.artislar) {
        let zirve = format!(
            "{} ({})",
            artis.zirve_adet,
            dilim_araligi(artis.zirve_baslangic, artis.zirve_bitis, gosterim)
        );
        let (ilk, son) = (artis.ilk.to_string(), artis.son.to_string());
        satir_yaz(
            cikti,
            &[zirve, ondalik(artis.olagan), ondalik_kisa(artis.puan)],
            &ozet.analiz.satirlar[artis.satir],
            &Hucreler::artistan(artis, &ilk, &son),
            gosterim,
        )?;
    }
    gizlenen_yaz(cikti, ozet.disarida_kalan(&ani_artislar.artislar))
}

/// Tablonun bir satırı. `sayilar`: satır başındaki sayı hücreleri (adet ya da önce ve sonra).
/// Kalıp ve konum `satir`dan, seviye, zamanlar ve örnek `hucreler`den gelir.
fn satir_yaz(
    cikti: &mut impl Write,
    sayilar: &[String],
    satir: &KalipSatiri<'_>,
    hucreler: &Hucreler<'_>,
    gosterim: ZamanGosterimi,
) -> io::Result<()> {
    let kalip_hucreleri = [
        hucreler.seviye.ad().to_owned(),
        zaman_goster(hucreler.ilk, gosterim),
        zaman_goster(hucreler.son, gosterim),
        kod(&kisalt(&gorunur(&satir.kalip), KALIP_GENISLIGI)),
        ornek(hucreler.ornek, &satir.kalip),
        konum(satir),
    ];
    // Her hücre aynı kuraldan geçer; sonradan eklenen bir sütun da unutulmaz
    let tum_hucreler: Vec<String> = sayilar
        .iter()
        .chain(&kalip_hucreleri)
        .map(|metin| hucre(metin))
        .collect();
    writeln!(cikti, "| {} |", tum_hucreler.join(" | "))
}

/// Gösterilmeyen satırlar varsa tablonun altına yazar.
fn gizlenen_yaz(cikti: &mut impl Write, gizlenen: usize) -> io::Result<()> {
    if gizlenen > 0 {
        writeln!(cikti)?;
        writeln!(
            cikti,
            "… {gizlenen} satır daha; tümünü görmek için `--hepsi`"
        )?;
    }
    Ok(())
}

/// Örnek hücresi: kalıp ham mesajla aynıysa boş kalır.
fn ornek(ornek: &str, kalip: &str) -> String {
    if ornek == kalip {
        String::new()
    } else {
        kod(&kisalt(&gorunur(ornek), KALIP_GENISLIGI))
    }
}

/// Konum hücresi: `app/Services/X.php:88` ErrorException. View hatalarında satırsız bir şablon:
/// `resources/views/vitrin.blade.php` ViewException
///
/// Kısa sınıf adı kod dışında yazılabilir: parmak izi yalnızca harf, rakam, `_`, `\` ve `@`
/// içeren sınıf adlarını kabul eder.
fn konum(satir: &KalipSatiri<'_>) -> String {
    match satir.parmak_izi {
        Some(parmak_izi) => match parmak_izi.konum() {
            Some((dosya, satir_no)) => {
                let yer = match satir_no {
                    Some(satir_no) => format!("{dosya}:{satir_no}"),
                    None => dosya.into_owned(),
                };
                format!("{} {}", kod(&gorunur(&yer)), parmak_izi.kisa_sinif())
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
    use crate::ani_artis::{deneme_logu, karisik_seviyeli_log};
    use crate::kayit::kayitlari_ayir;
    use crate::rapor::RaporAyarlari;
    use crate::zaman::ZamanNoktasi;
    use std::path::Path;

    const ORNEK_LOG: &str = include_str!("../ornekler/laravel.log");

    /// Logun varsayılan ayarlarla Markdown raporu.
    fn markdown(log_metni: &str, yol: &str) -> String {
        markdown_ayarlarla(log_metni, yol, &RaporAyarlari::default())
    }

    fn markdown_ayarlarla(log_metni: &str, yol: &str, ayarlar: &RaporAyarlari) -> String {
        let log = kayitlari_ayir(log_metni);
        let ozet = Ozet::hesapla(Path::new(yol), log_metni, &log, ayarlar).unwrap();
        let mut cikti = Vec::new();
        yaz(&mut cikti, &ozet).unwrap();
        String::from_utf8(cikti).unwrap()
    }

    /// Örnek logun, verilen anda bölünmüş karşılaştırmalı raporu. Kısa kalsın diye `--ilk 2`.
    fn karsilastirmali(nokta: &str) -> String {
        let ayarlar = RaporAyarlari {
            karsilastir: ZamanNoktasi::ayristir(nokta),
            ilk: Some(2),
            ..RaporAyarlari::default()
        };
        markdown_ayarlarla(ORNEK_LOG, "ornekler/laravel.log", &ayarlar)
    }

    #[test]
    fn karsilastirma_tablolari() {
        // Her bölüm ayrı bir tablo; yalnızca artanda sonraki sayının yanında kat yazılır
        let beklenen = r#"
### Karşılaştırma: 2026-10-24 16:11:00 öncesi ve sonrası

- **Önce:** 2026-10-24 08:59:58 → 2026-10-24 16:11:00 · 7 sa 11 dk · 39 kayıt, saatte 5,4
- **Sonra:** 2026-10-24 16:11:00 → 2026-10-24 18:31:07 · 2 sa 20 dk · 20 kayıt, saatte 8,6

#### Yeni çıkanlar (yalnızca sonra): 3

| Önce | Sonra | Seviye | İlk | Son | Kalıp | Örnek | Konum |
|---:|---:|---|---|---|---|---|---|
| 0 | 2 | CRITICAL | 17:05:13 | 17:06:45 | `Redis bağlantısı kurulamadı: Connection refused [tcp://<IP>:<SAYI>]` | `Redis bağlantısı kurulamadı: Connection refused [tcp://10.0.3.12:6379]` | `app/Http/Middleware/HizSiniri.php:22` RedisException |
| 0 | 1 | INFO | 16:24:10 | 16:24:10 | `Ödeme sağlayıcısı yeniden yanıt veriyor` |  |  |

… 1 satır daha; tümünü görmek için `--hepsi`

#### Kaybolanlar (yalnızca önce): 11

| Önce | Sonra | Seviye | İlk | Son | Kalıp | Örnek | Konum |
|---:|---:|---|---|---|---|---|---|
| 1 | 0 | ERROR | 09:47:12 | 09:47:12 | `Attempt to read property "ad" on null` |  | `app/Http/Controllers/KategoriController.php:54` ErrorException |
| 1 | 0 | ERROR | 11:30:44 | 11:30:44 | `App\Jobs\FaturaGonder has been attempted too many times.` |  | uygulama kodunda bulunamadı (MaxAttemptsExceededException) |

… 9 satır daha; tümünü görmek için `--hepsi`

#### Artanlar (sonra ≥ 2 kat sık, tesadüf ihtimali < %1): 1

| Önce | Sonra | Seviye | İlk | Son | Kalıp | Örnek | Konum |
|---:|---:|---|---|---|---|---|---|
| 1 | 5 (15,4 kat) | ERROR | 16:10:02 | 16:16:02 | `cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> by…` | `cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes receiv…` | `app/Services/OdemeServisi.php:64` ConnectionException |
"#;
        let metin = karsilastirmali("16:11");
        assert!(metin.ends_with(beklenen), "{metin}");

        // Boş bölüm tablo yerine "yok" yazar
        assert!(
            karsilastirmali("15:00")
                .ends_with("\n#### Artanlar (sonra ≥ 2 kat sık, tesadüf ihtimali < %1): yok\n")
        );
    }

    #[test]
    fn ani_artis_tablosu() {
        // Zirve hücresinde zirve dilim de yazar; İlk ve Son artışın kayıtlarıdır
        let beklenen = r#"
### Ani artışlar (WARNING ve üstü, 10 dakikalık 59 dilim, puan > 3,5): 1

| Zirve | Olağan | Puan | Seviye | İlk | Son | Kalıp | Örnek | Konum |
|---:|---:|---:|---|---|---|---|---|---|
| 6 (16:10–16:20) | 0 | 4,0 | ERROR | 16:10:02 | 16:16:02 | `cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> by…` | `cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes receiv…` | `app/Services/OdemeServisi.php:64` ConnectionException |
"#;
        let ani_artisli = |dakika| {
            let ayarlar = RaporAyarlari {
                ani_artis: Some(dakika),
                ilk: Some(2),
                ..RaporAyarlari::default()
            };
            markdown_ayarlarla(ORNEK_LOG, "ornekler/laravel.log", &ayarlar)
        };
        let metin = ani_artisli(10);
        assert!(metin.ends_with(beklenen), "{metin}");

        // Artış yoksa tablo yerine "yok"
        assert!(ani_artisli(1).ends_with(
            "\n### Ani artışlar (WARNING ve üstü, 1 dakikalık 573 dilim, puan > 3,5): yok\n"
        ));

        // Üç artış, ikisi gösterilir. Zirve hücresinde zirve dilim, İlk ve Son'da artışın
        // kayıtları; olağan seviyesi 2 olan kalıp
        let ayarlar = RaporAyarlari {
            ani_artis: Some(1),
            ilk: Some(2),
            ..RaporAyarlari::default()
        };
        let metin = markdown_ayarlarla(&deneme_logu(), "a.log", &ayarlar);
        let beklenen = r#"
| Zirve | Olağan | Puan | Seviye | İlk | Son | Kalıp | Örnek | Konum |
|---:|---:|---:|---|---|---|---|---|---|
| 9 (10:31–10:32) | 0 | 6,1 | ERROR | 10:30:30 | 10:32:36 | `Kuyruk dolu` |  |  |
| 9 (10:45–10:46) | 2 | 4,7 | WARNING | 10:45:10 | 10:45:18 | `Yavaş istek` |  |  |

… 1 satır daha; tümünü görmek için `--hepsi`
"#;
        assert!(metin.ends_with(beklenen), "{metin}");

        // Seviye ve örnek de artışın kayıtlarından: satırın seviyesi CRITICAL, örneği
        // "Kuyruk 2 dolu"
        let ayarlar = RaporAyarlari {
            ani_artis: Some(1),
            ..RaporAyarlari::default()
        };
        let metin = markdown_ayarlarla(&karisik_seviyeli_log(), "a.log", &ayarlar);
        assert!(
            metin.contains(
                "| CRITICAL | 10:00:30 | 11:50:30 | `Kuyruk <SAYI> dolu` | `Kuyruk 2 dolu` |  |\n"
            ),
            "{metin}"
        );
        assert!(
            metin.ends_with(
                "| ERROR | 11:00:00 | 11:01:15 | `Kuyruk <SAYI> dolu` | `Kuyruk 3 dolu` |  |\n"
            ),
            "{metin}"
        );
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
