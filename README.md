# veylog

[![CI](https://github.com/Veysel440/veylog/actions/workflows/ci.yml/badge.svg)](https://github.com/Veysel440/veylog/actions/workflows/ci.yml)
[![Sürüm](https://img.shields.io/github/v/release/Veysel440/veylog?label=s%C3%BCr%C3%BCm)](https://github.com/Veysel440/veylog/releases/latest)
[![crates.io](https://img.shields.io/crates/v/veylog.svg)](https://crates.io/crates/veylog)
[![Rust 1.88+](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)
[![Lisans: MIT](https://img.shields.io/badge/lisans-MIT-blue.svg)](LICENSE)

**Laravel log dosyalarını saniyenin altında okunabilir bir özete çeviren komut satırı aracı.**
Rust ile yazıldı; tek bir çalıştırılabilir dosyadır, projeye paket eklemek ya da PHP kurmak
gerekmez.

[Kurulum](#kurulum) · [Kullanım](#kullanım) · [Örnekler](#örnekler) ·
[Nasıl çalışır](#nasıl-çalışır) · [Tasarım kararları](#tasarım-kararları) ·
[Performans](#performans) · [Sınırlamalar](#sınırlamalar) · [English](#english)

`storage/logs/laravel.log` kısa sürede binlerce satıra ulaşır: aynı hata yüzlerce kez
tekrarlanır, stack trace'ler satır sayısını şişirir, gerçekten önemli olan kayıt kalabalıkta
kaybolur. veylog dosyayı okur ve şu sorulara cevap verir:

- Hangi hatalar tekrar ediyor, kaç kez, hangi seviyede?
- Kodun neresinde oluşuyorlar?
- Ne zaman başladılar, son dağıtımdan sonra ne değişti?
- Hangi hata bir anda patladı?

```text
$ veylog ornekler/laravel.log
ornekler/laravel.log: 205 satır
  Kayıt         : 59
  Devam satırı  : 143
  Yetim satır   : 3
  Zaman aralığı : 2026-10-24 08:59:58 → 2026-10-24 18:31:07

Seviyeler:
  CRITICAL     2
  ERROR       16
  WARNING      6
  NOTICE       1
  INFO        31
  DEBUG        3

Kalıplar: 45 farklı mesaj → maskeleyince 27 → Drain ile 23 kalıp (eşik 0,7) → parmak iziyle 24 grup
  adet  seviye     ilk       son       kalıp
     2  CRITICAL   17:05:13  17:06:45  Redis bağlantısı kurulamadı: Connection refused [tcp://<IP>:<SAYI>]
                                       örnek: Redis bağlantısı kurulamadı: Connection refused [tcp://10.0.3.12:6379]
                                       konum: app/Http/Middleware/HizSiniri.php:22, handle() (RedisException)
     6  ERROR      16:10:02  16:16:02  cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> by…
                                       örnek: cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes…
                                       konum: app/Services/OdemeServisi.php:64, tahsilEt() (ConnectionException)
     4  ERROR      15:02:11  16:58:12  Undefined array key "kargo_ucreti"
                                       konum: app/Services/SepetHesaplayici.php:88, toplam() (ErrorException)
     3  ERROR      09:14:20  17:55:03  SQLSTATE[<SAYI>]: Integrity constraint violation: <SAYI> Duplicate entry '<EPOS…
                                       örnek: SQLSTATE[23000]: Integrity constraint violation: 1062 Duplicate entry 'a…
                                       değerler: Ayşe ×1, Mehmet ×1, Zeynep ×1
                                       konum: app/Http/Controllers/KayitController.php:38, store() (UniqueConstraintViolationException)
     1  ERROR      09:47:12  09:47:12  Attempt to read property "ad" on null
                                       konum: app/Http/Controllers/KategoriController.php:54, goster() (ErrorException)
     1  ERROR      11:30:44  11:30:44  App\Jobs\FaturaGonder has been attempted too many times.
                                       konum: uygulama kodunda bulunamadı (MaxAttemptsExceededException)
     1  ERROR      13:18:20  13:18:20  Attempt to read property "ad" on null
                                       konum: app/Http/Controllers/UrunController.php:72, goster() (ErrorException)
     3  WARNING    16:10:03  16:12:41  Ödeme sağlayıcısı yanıt vermedi, sipariş <SAYI> için yeniden denenecek (deneme …
                                       örnek: Ödeme sağlayıcısı yanıt vermedi, sipariş 1047 için yeniden denenecek (de…
     1  WARNING    09:33:05  09:33:05  Yavaş sorgu (<SAYI> ms): select * from `siparisler` where `kullanici_id` = <SAY…
                                       örnek: Yavaş sorgu (1532 ms): select * from `siparisler` where `kullanici_id` =…
     1  WARNING    10:58:51  10:58:51  Yavaş sorgu (<SAYI> ms): select * from `urunler` where `kategori_id` = <SAYI> a…
                                       örnek: Yavaş sorgu (2210 ms): select * from `urunler` where `kategori_id` = 12 …
  … 14 satır daha; tümünü görmek için --hepsi
```

Tablo, örnek logdaki bir günün hikâyesini anlatıyor: 15:00'teki dağıtımdan iki dakika sonra
başlayan `kargo_ucreti` hatası, 16:10–16:16 arasındaki ödeme zaman aşımları ve 17:05'teki
Redis çöküşü. `Attempt to read property "ad" on null` iki ayrı satırda: mesajları aynı, ama
biri kategori sayfasında, öteki ürün sayfasında oluşan iki ayrı hata.

## Öne çıkanlar

- **Satırlar değil, kalıplar.** Mesajlardaki değişken kısımlar maskelenir (`<SAYI>`, `<IP>`,
  `<YOL>`...) ve mesajlar [Drain](https://jiemingzhu.github.io/pub/pjhe_icws2017.pdf)
  algoritmasıyla kalıplara ayrılır: 59 kayıt 24 satıra iner.
- **Kategorik konumlar.** Koddan gelen değerler (sınıf adları, sabitler, olay adları,
  tırnaklı anahtarlar) sıradan kelimelerden biçimleriyle ayrılır: `İş tamamlandı:
  App\Jobs\FaturaGonder` ile `İş tamamlandı: App\Jobs\EpostaGonder` yüz farklı iş beklenmeden
  tek kalıpta birleşir, `Ödeme alındı` ile `Ödeme reddedildi` birleşmez. Her `<*>` konumunun
  en sık değerleri adetleriyle raporda yazılır: hangi işler, hangi olaylar, kaçar kez.
- **Hatanın yeri.** Stack trace'ten istisna sınıfı, uygulama kodundaki ilk satır ve o satırın
  içinde bulunduğu fonksiyon çıkarılır. Vendor, giriş noktaları ve middleware'in
  `$next($request)` satırı atlanır; Blade hatalarında şablonun kendisi gösterilir. Aynı mesaj
  kodun farklı yerlerinden geliyorsa ayrı satırlara bölünür. İstisna `report($e)` ile
  bağlamda da, `Log::error($e)` ile metin olarak da loglansa aynı hatadır.
- **Dağıtımlara dayanıklı.** Hata satırla değil fonksiyonla tanınır: bir dağıtım dosyanın
  üstüne satır eklese de aynı hata aynı kalır, yeni çıkmış sayılmaz. Closure'lar
  tanımlandıkları fonksiyonla ayrılır (`{closure:olustur()}`); çağrıları yazılmayan JSON
  kayıtlarının fonksiyonu logun başka kayıtlarından öğrenilir. Proje kökü yollardan silinir:
  sıfır kesintili dağıtımlarda (Forge, Envoyer) klasör her seferinde değişse de aynı hata tek
  satırda kalır.
- **Dağıtımdan önce ve sonra.** `--karsilastir 15:00` logu ikiye böler ve o andan sonra yeni
  çıkan, kaybolan ve belirgin şekilde artan hataları listeler. Artışın tesadüf olmadığı
  kesin bir binom testiyle sınanır.
- **Ani artışlar.** `--ani-artis 5` her kalıbın WARNING ve üstündeki kayıtlarını beşer
  dakikalık dilimlerde sayar ve olağan seviyesinden sıçradığı anları bulur. Olağan seviye
  medyanla, dalgalanma MAD ile ölçülür; aranan patlama kendi ölçüsünü bozmaz.
- **Canlı izleme.** `--izle` logu sonuna kadar okur, raporu yazar, sonra eklenen kayıtları
  okumaya devam eder ve yeni kayıt geldikçe raporu yeniden çizer; `--son 1sa` ile hep son bir
  saat görünür. Döndürülen log baştan okunur, `tail -f` ile gelen bir akış da izlenebilir.
  Girdi yeniden okunmaz: her çizim yalnızca yeni baytları işler.
- **Üç çıktı biçimi.** Terminal için tablo, betikler ve CI adımları için JSON, GitHub
  issue'larına yapıştırmak için Markdown.
- **İki dil.** Arayüz Türkçe; `--dil en` (ya da `VEYLOG_DIL=en`) yardımı, raporu ve bütün
  mesajları İngilizce yazar. Her seçeneğin İngilizce adı da geçerlidir (`--last 2h`,
  `--format json`). JSON çıktısı her dilde aynıdır: betikler dil değişince bozulmaz.
- **Gerçek loglara dayanıklı.** Laravel'in metin biçimi ve Monolog'un JSON satırları (aynı
  dosyada karışık olsalar bile), ISO 8601 zamanlar, Windows yolları (Herd, Laragon),
  standart girdi, UTF-8 BOM ve geçersiz UTF-8. Uygulama Monolog'a başka bir satır ya da
  tarih biçimi verdiyse aynı dizgeler `--desen` ve `--zaman-bicimi` ile verilir.
- **Güvenli çıktı.** Log mesajlarındaki kontrol karakterleri (ESC, CR) terminale komut olarak
  ulaşmaz: logu yazabilen biri raporun ekranda nasıl göründüğünü değiştiremez.
- **Tek bağımlılık.** Yalnızca [regex](https://crates.io/crates/regex). JSON okuyucu ve
  yazıcı, Drain, istatistik hesapları ve komut satırı ayrıştırıcı elle yazıldı; doğrulukları
  referans uygulamalarla (serde_json, scipy, numpy, Drain3) karşılaştırılarak sınanır.

## Kurulum

### Hazır program

[Sürümler](https://github.com/Veysel440/veylog/releases/latest) sayfasında Linux, macOS ve
Windows için derlenmiş program bulunur; Rust kurmak gerekmez.

| Sistem | Dosya |
|---|---|
| Linux (x86_64, statik; her dağıtımda çalışır) | `veylog-x86_64-unknown-linux-musl.tar.gz` |
| macOS (Apple Silicon) | `veylog-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `veylog-x86_64-apple-darwin.tar.gz` |
| Windows (x86_64) | `veylog-x86_64-pc-windows-msvc.zip` |

Her arşivin yanında SHA-256 özeti (`.sha256`) de yayımlanır. Linux'ta (WSL dahil) indirme,
doğrulama ve kurulum:

```bash
curl -LO https://github.com/Veysel440/veylog/releases/latest/download/veylog-x86_64-unknown-linux-musl.tar.gz
curl -LO https://github.com/Veysel440/veylog/releases/latest/download/veylog-x86_64-unknown-linux-musl.sha256
sha256sum -c veylog-x86_64-unknown-linux-musl.sha256
tar -xzf veylog-x86_64-unknown-linux-musl.tar.gz
sudo mv veylog /usr/local/bin/
veylog --surum
```

macOS'ta dosya adlarındaki hedefi değiştirmek yeterlidir; özet `shasum -a 256 -c` ile
doğrulanır. Arşiv tarayıcıyla indirildiyse macOS ilk çalıştırmada uyarabilir:
`xattr -d com.apple.quarantine veylog`.

Windows'ta (PowerShell):

```powershell
Invoke-WebRequest https://github.com/Veysel440/veylog/releases/latest/download/veylog-x86_64-pc-windows-msvc.zip -OutFile veylog.zip
Expand-Archive veylog.zip -DestinationPath veylog
.\veylog\veylog.exe --surum
```

`veylog.exe` tek başına çalışır; Visual C++ çalışma zamanı gibi ek bir kurulum gerekmez. Her
klasörden çalıştırmak için `veylog` klasörünü `PATH`'e ekleyin.

Ekranda Türkçe harfler doğru görünür. Windows'la gelen Windows PowerShell 5.1 ise bir dosyaya
ya da başka bir programa yönlendirilen çıktıyı (`> rapor.md`, `| jq`) yeniden kodlar ve
Türkçe harfleri bozar; PowerShell 7.4 ve sonrası (`winget install Microsoft.PowerShell`)
baytları olduğu gibi aktarır. 5.1'de yönlendirme `cmd` ile yapılabilir:

```powershell
cmd /c "veylog --bicim markdown storage\logs\laravel.log > rapor.md"
```

### Cargo ile

[Rust](https://rustup.rs) 1.88 ya da daha yenisi kuruluysa
[crates.io](https://crates.io/crates/veylog)'dan:

```bash
cargo install veylog --locked
```

Program `~/.cargo/bin` klasörüne kurulur; rustup bu klasörü `PATH`'e ekler. Henüz sürüme
girmemiş son değişiklikler için: `cargo install --git https://github.com/Veysel440/veylog --locked`

### Kaynaktan

```bash
git clone https://github.com/Veysel440/veylog.git
cd veylog
cargo build --release
./target/release/veylog ornekler/laravel.log
```

## Kullanım

```text
veylog [SEÇENEKLER] <log-dosyası>
```

| Seçenek | Açıklama | Varsayılan |
|---|---|---|
| `-e`, `--esik <sayı>` | Drain benzerlik eşiği, 0 ile 1 arası. Büyüdükçe kalıplar daha az birleşir. | 0,7 |
| `--konum <tür>` | Bir hatanın yeri neyle belirlensin: `metot` (dosya ve fonksiyon; satırlar kaysa da hata aynı kalır) ya da `satir` (dosya ve satır, 0.1'deki gibi). | `metot` |
| `-n`, `--ilk <sayı>` | Tabloda gösterilecek satır sayısı. | 10 |
| `--hepsi` | Tablonun tamamını göster. | |
| `-s`, `--siralama <ölçüt>` | Tablonun sırası: `onem`, `adet` ya da `yeni`. | `onem` |
| `-b`, `--bicim <ad>` | Çıktının biçimi: `metin`, `json` ya da `markdown`. | `metin` |
| `-k`, `--karsilastir <zaman>` | Logu bu anda ikiye böl ve iki yarıyı karşılaştır. Zaman: `15:00`, `15:00:30`, `2026-10-24` ya da `"2026-10-24 15:00"`. | |
| `-a`, `--ani-artis <dakika>` | Logu bu uzunlukta dilimlere böl; her kalıbın WARNING ve üstündeki kayıtlarının olağan seviyesinden sıçradığı anları bul (1–1440). | |
| `--seviye <seviye>` | Yalnızca bu seviye ve üstündeki kayıtlar: `debug`, `info`, `notice`, `warning`, `error`, `critical`, `alert`, `emergency`. | |
| `--baslangic <zaman>` | Yalnızca bu andan (dahil) sonraki kayıtlar. Zaman `--karsilastir` gibi yazılır. | |
| `--bitis <zaman>` | Yalnızca bu andan (hariç) önceki kayıtlar. | |
| `--son <süre>` | Yalnızca logun son kısmı: en yeni kayıttan bu kadar öncesinden bu yana. Süre: `30dk`, `2sa`, `1g`, `1sa30dk`. | |
| `--saat-dilimi <dilim>` | Zamanları bu saat diliminde göster: `+03:00`, `-05:30`, `Z` (UTC). Saat dilimi yazılı kayıtlar bu dilime çevrilir. | logda yazılı ilk dilim |
| `--desen <desen>` | Bir kaydı başlatan satırın biçimi: Monolog'un biçim dizgesi (`"%datetime% [%level_name%] %message% %context%"`) ya da adlandırılmış gruplu bir düzenli ifade. | Laravel'in biçimi |
| `--zaman-bicimi <biçim>` | Zamanın biçimi, PHP'nin `date()` dizgesiyle: `"d.m.Y H:i:s"`, `"d/M/Y:H:i:s O"`, `"U"`. | `2026-10-24 14:03:11` ya da ISO 8601 |
| `--izle` | Logu canlı izle: sonuna kadar oku, raporu yaz, eklenen kayıtlar geldikçe yeniden çiz. Yalnızca metin biçiminde; `--karsilastir` ile kullanılamaz. | |
| `--aralik <süre>` | İzlemede girdiye bu sıklıkla bak; raporu en sık bu kadarda bir çiz. Süre `--son` gibi, `ms` de olur (`500ms`); 100 ms ile 1 sa arası. | `2sn` |
| `--dil <dil>` | Arayüzün dili: `tr` ya da `en`. `VEYLOG_DIL` ortam değişkeniyle de seçilir. | `tr` |
| `-h`, `--yardim` | Yardımı göster (`--help` da olur). | |
| `-V`, `--surum` | Sürümü göster (`--version` da olur). | |

```bash
# Varsayılan ayarlarla
veylog storage/logs/laravel.log

# Son ortaya çıkan sorunlar önce, tablonun tamamı
veylog --siralama yeni --hepsi storage/logs/laravel.log

# 15:00'teki dağıtımdan önce ve sonra: yeni çıkan, kaybolan ve artan hatalar
veylog --karsilastir 15:00 storage/logs/laravel.log

# Aynı fonksiyonun farklı satırlarındaki hatalar ayrı ayrı
veylog --konum satir storage/logs/laravel.log

# Beşer dakikalık dilimlerde olağan seviyesinden sıçrayan hatalar
veylog --ani-artis 5 storage/logs/laravel.log

# Logun son iki saatindeki hatalar; ya da yalnızca dünkü 14:00–16:00
veylog --seviye error --son 2sa storage/logs/laravel.log
veylog --baslangic "2026-10-23 14:00" --bitis "2026-10-23 16:00" storage/logs/laravel.log

# UTC'de yazılmış bir log, Türkiye saatiyle
veylog --saat-dilimi +03:00 storage/logs/laravel.log

# Biçimi config/logging.php'de değiştirilmiş bir log: Monolog'un biçim ve tarih dizgeleri
veylog --desen '%datetime% [%level_name%] %message% %context%' --zaman-bicimi 'd.m.Y H:i:s' storage/logs/laravel.log

# Sıkıştırılmış bir günlük log ya da sunucudaki bir log: dosya yerine - standart girdidir
zcat laravel-2026-10-24.log.gz | veylog -
ssh sunucu 'cat /var/www/magaza/storage/logs/laravel.log' | veylog -

# Logu canlı izle: her 2 saniyede yeni kayıtlara bak, son bir saati göster; Ctrl+C ile çık
veylog --izle --son 1sa storage/logs/laravel.log
ssh sunucu 'tail -f /var/www/magaza/storage/logs/laravel.log' | veylog --izle --ani-artis 1 -

# WSL'den, Windows'taki bir Herd projesinin logu
veylog /mnt/c/Users/Ad/Herd/proje/storage/logs/laravel.log
```

Seçenekler dosya adından önce ya da sonra yazılabilir; değer boşlukla, `=` ile ya da kısa
seçeneklerde bitişik verilir (`--esik 0,8`, `--esik=0,8`, `-n20`, `-n=20`). Ondalık ayracı
virgül de olabilir, nokta da; değerlerde büyük ve küçük harf fark etmez (`--bicim JSON`).
Türkçe karakterli yazımlar da geçerlidir (`--eşik`, `--sıralama önem`).
Her uzun seçeneğin ve her değerin İngilizce adı da her dilde geçerlidir; yardım ve mesajlar
seçili dildeki adları gösterir:

| Türkçe | İngilizce | Türkçe | İngilizce |
|---|---|---|---|
| `--esik` | `--threshold` | `--baslangic` | `--since` |
| `--konum metot`, `satir` | `--location method`, `line` | `--bitis` | `--until` |
| `--ilk` | `--top` | `--son 2sa` | `--last 2h` |
| `--hepsi` | `--all` | `--saat-dilimi` | `--timezone` |
| `--siralama onem`, `adet`, `yeni` | `--sort severity`, `count`, `newest` | `--desen` | `--pattern` |
| `--bicim metin` | `--format text` | `--zaman-bicimi` | `--time-format` |
| `--karsilastir` | `--compare` | `--izle` | `--watch` |
| `--ani-artis` | `--spikes` | `--aralik` | `--interval` |
| `--seviye` | `--level` | `--dil` | `--lang` |

Yazım hatalarında en yakın seçenek önerilir:

```text
$ veylog --esk 0,8 laravel.log
veylog: bilinmeyen seçenek: --esk (bunu mu demek istediniz: --esik)
Kullanım: veylog [SEÇENEKLER] <log-dosyası>
Ayrıntılar için: veylog --yardim
```

Filtreler (`--seviye`, `--baslangic`, `--bitis`, `--son`) raporu yalnızca uyan kayıtlarla
çıkarır: kalıplar, seviyeler, zaman aralığı, karşılaştırma ve ani artışlar onlardandır.
Dosyanın satır ve kayıt sayıları dosyanın tamamınındır; uyan kayıt sayısı `Filtre` satırında
yazar. Yalnızca saat verilmiş bir sınır (`--baslangic 15:00`) `--karsilastir` gibi logun son
gününe uygulanır. `--son` ve yalnızca saat verilmiş sınırlar logun sonunu bilmeyi gerektirir:
log önce bir kez taranır (standart girdi bir borudan geliyorsa o sırada geçici bir dosyaya
kopyalanır). Filtreye
uymayan kayıtlar maskelenmediği ve kümelenmediği için filtreli bir rapor daha hızlı çıkar:
500 MB'lık bir logun son iki saati 1,8 saniyede özetlenir.

`--izle` logu canlı izler: sonuna kadar okur, raporu yazar, sonra eklenen kayıtları okumaya
devam eder ve yeni kayıt geldikçe raporu yeniden çizer (terminalde ekranı temizleyerek; bir
dosyaya ya da boruya yazılıyorsa çizimler bir ayırıcı satırla art arda). Girdi yeniden
okunmaz: her çizim yalnızca o çizimden beri eklenen baytları işler, rapor depodan yeniden
üretilir. Raporun başındaki satır kaç kaydın okunduğunu ve en yeni kaydın zamanını söyler.
Zaman filtreleri izlemede kayan penceredir: `--son 1sa` her çizimde o ana kadarki en yeni
kayda göre uygulanır, rapor hep son bir saati gösterir. Döndürülen log (logrotate,
`copytruncate`) baştan okunur, önceki kayıtlar raporda kalır. Standart girdi (`tail -f ... |
veylog --izle -`) bitince son rapor yazılır ve çıkılır; dosya Ctrl+C'ye kadar izlenir. Bir
kaydın yığın izi birkaç yazma işlemiyle gelebileceği için okunan son kayıt, ondan sonra bir
kayıt başlığı gelince ya da girdi bir aralık (`--aralik`, varsayılan 2 saniye) boyunca sessiz
kalınca depoya alınır ([ayrıntılar](docs/nasil-calisir.md#canlı-izleme)).

Kayıtlarda saat dilimi yazılıysa (Monolog'un ISO 8601 biçimi ve JSON satırları:
`2026-10-24T14:03:11+03:00`) bütün zamanlar tek bir dilime, raporun saat dilimine çevrilir:
yaz saatine geçişi kapsayan ya da saat dilimi değiştirilmiş bir uygulamanın logu doğru sıralanır.
Raporun dilimi `--saat-dilimi` ile verilir; verilmezse logda yazılı ilk dilimdir. Raporda
`Saat dilimi` satırı kaç kaydın çevrildiğini söyler. Filtrelerin ve `--karsilastir`'ın zamanları
da bu dilimdedir ([ayrıntılar](docs/nasil-calisir.md#saat-dilimleri)).

Laravel'in satır biçimi ve Monolog'un JSON satırları kendiliğinden tanınır. `Log::error($e)`
istisnayı bağlama koymaz, PHP'nin metni olarak yazar (`RuntimeException: ... in /yol:64` ve
`Stack trace:`); veylog sınıfı, yeri ve mesajı bu metinden de okur: aynı hata `report($e)` ile
loglanmış olsa da tek satırdır ([ayrıntılar](docs/nasil-calisir.md#php-istisna-metni)).
Uygulama Monolog'a başka bir satır biçimi verdiyse (`formatter_with` ile `format` ve
`dateFormat`) aynı dizgeler `--desen` ve `--zaman-bicimi` ile verilir. Monolog'la yazılmamış
bir log adlandırılmış gruplu bir düzenli ifadeyle tanıtılır:
`--desen '(?P<zaman>\S+ \S+) (?P<seviye>\w+): (?P<mesaj>.*)'`
([ayrıntılar](docs/nasil-calisir.md#log-biçimleri)).

Bir hatanın yeri dosya ve satırın içinde bulunduğu fonksiyondur:
`app/Services/SepetHesaplayici.php:88, toplam()`. Satır numarası gösterilir ama kimliğe
katılmaz: bir dağıtım dosyanın üstüne satır eklerse aynı hata 91. satırda görünür ve yine aynı
hatadır; gösterilen satır en yeni kaydınkidir. Closure'larda tanımlandıkları fonksiyon yazılır
(`{closure:olustur()}`); PHP 8.4 onu closure'ın adına kendisi yazar, öncesinde yığın izinden
bulunur. Aynı fonksiyonun iki satırındaki hataları ayırmak için `--konum satir`
([ayrıntılar](docs/nasil-calisir.md#satır-değil-fonksiyon)).

Çıkış kodları betiklerin hatanın türünü ayırt edebilmesi içindir: `0` başarılı, `1` dosya
okunamadı ya da çıktı yazılamadı, `2` kullanım hatası (bilinmeyen seçenek, geçersiz değer,
logun dışında kalan bir karşılaştırma noktası, boş bir filtre aralığı).

## Örnekler

### Dağıtımdan önce ve sonra

`--karsilastir` logu verilen anda ikiye böler. Kalıplar bütün log üzerinde bir kez çıkarılır,
sonra her satırın kayıtları iki yarıya sayılır. Yarılar farklı uzunlukta olabileceği için
kayıt sayıları değil saatlik sıklıklar karşılaştırılır.

```text
$ veylog --karsilastir 16:11 --ilk 1 ornekler/laravel.log
...
Karşılaştırma: 2026-10-24 16:11:00 öncesi ve sonrası
  önce  : 08:59:58 → 16:11:00   7 sa 11 dk    39 kayıt, saatte 5,4
  sonra : 16:11:00 → 18:31:07   2 sa 20 dk    20 kayıt, saatte 8,6

Yeni çıkanlar (yalnızca sonra): 3
  önce  sonra  seviye     ilk       son       kalıp
     0      2  CRITICAL   17:05:13  17:06:45  Redis bağlantısı kurulamadı: Connection refused [tcp://<IP>:<SAYI>]
                                              örnek: Redis bağlantısı kurulamadı: Connection refused [tcp://10.0.3.12:6379]
                                              konum: app/Http/Middleware/HizSiniri.php:22, handle() (RedisException)
  … 2 satır daha; tümünü görmek için --hepsi

Kaybolanlar (yalnızca önce): 11
  önce  sonra  seviye     ilk       son       kalıp
     1      0  ERROR      09:47:12  09:47:12  Attempt to read property "ad" on null
                                              konum: app/Http/Controllers/KategoriController.php:54, goster() (ErrorException)
  … 10 satır daha; tümünü görmek için --hepsi

Artanlar (sonra ≥ 2 kat sık, tesadüf ihtimali < %1): 1
  önce  sonra  seviye     ilk       son       kalıp
     1      5  ERROR      16:10:02  16:16:02  cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> by…
                                              artış: saatte 0,1 → 2,1 (15,4 kat)
                                              örnek: cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes…
                                              konum: app/Services/OdemeServisi.php:64, tahsilEt() (ConnectionException)
```

Hatalar satırla değil fonksiyonla tanındığı için dağıtım dosyaların üstüne satır eklediyse
de aynı hata tablonun aynı satırında kalır; `--konum satir` ile hem "kaybolan" hem "yeni çıkan"
sayılırdı.

Bir kalıbın "artan" sayılması için üç şart birlikte aranır: sonra en az 2 kat sık görülmeli,
sonra yarısında en az 2 kaydı olmalı ve artış tesadüf olmamalı (p < 0,01). Aynı anda
"Ödeme sağlayıcısı yanıt vermedi" kalıbı da 1 kayıttan 2'ye çıkar ve sıklığı 6 kat artar;
ama toplam 3 kaydın tesadüfen böyle dağılma ihtimali %15'tir, listeye girmez.

### Ani artışlar

Karşılaştırma için bakılacak anı bilmek gerekir. `--ani-artis` ise logu dilimlere böler ve her
kalıbın olağan seviyesinden sıçradığı anları kendisi bulur:

```text
$ veylog --ani-artis 10 --ilk 3 ornekler/laravel.log
...
Ani artışlar (WARNING ve üstü, 10 dakikalık 59 dilim, puan > 3,5): 1
  zirve  olağan  puan  seviye     ilk       son       kalıp
      6       0   4,0  ERROR      16:10:02  16:16:02  cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> by…
                                                      artış: 16:10–16:20 arası 6 kayıt
                                                      örnek: cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes…
                                                      konum: app/Services/OdemeServisi.php:64, tahsilEt() (ConnectionException)
```

Ödeme zaman aşımı günün geri kalanında hiç görülmüyor; 16:10–16:20 diliminde 6 kez. Puan,
Iglewicz ve Hoaglin'in değiştirilmiş z puanıdır: `0,6745 × (sayı − medyan) / MAD`.

### Kategorik konumlar

Bir kuyruk işçisinin logu ([ornekler/kuyruk.log](ornekler/kuyruk.log)): iş sınıfları, olay
adları ve durum sabitleri birer değişkendir, kuyruk adları (`emails`, `default`) sıradan
kelimelerdir.

```text
$ veylog ornekler/kuyruk.log
...
Kalıplar: 11 farklı mesaj → maskeleyince 11 → Drain ile 6 kalıp (eşik 0,7) → parmak iziyle 6 grup
  adet  seviye     ilk       son       kalıp
     2  WARNING    09:02:40  09:05:40  Sipariş <SAYI> durumu <*> olarak kaldı
                                       örnek: Sipariş 1051 durumu ORDER_PENDING olarak kaldı
                                       değerler: ORDER_PENDING ×1, ORDER_FAILED ×1
     2  WARNING    09:06:18  09:06:19  Undefined array key <*>
                                       örnek: Undefined array key "kargo_ucreti"
                                       değerler: "kargo_ucreti" ×1, "indirim" ×1
     4  INFO       09:00:03  09:01:12  İş tamamlandı: <*>
                                       örnek: İş tamamlandı: App\Jobs\FaturaGonder
                                       değerler: App\Jobs\FaturaGonder ×2, App\Jobs\EpostaGonder ×1,
                                                 App\Jobs\StokGuncelle ×1
     3  INFO       09:01:30  09:02:02  Olay işlendi: <*>
                                       örnek: Olay işlendi: siparis.olusturuldu
                                       değerler: siparis.olusturuldu ×2, odeme.alindi ×1
     1  INFO       09:07:11  09:07:11  Kuyruk durdu: emails
     1  INFO       09:07:11  09:07:11  Kuyruk durdu: default
```

Bir konumun en sık beş değeri yazılır, kalanı `+N başka` olur. JSON'da her satırın
`degiskenler` dizisi aynı bilgiyi taşır ([çıktı biçimleri](docs/cikti-bicimleri.md)).
Hangi belirteçlerin kodsal sayıldığı ve neden kuyruk adlarının birleşmediği
[kategorik konumlar](docs/nasil-calisir.md#kategorik-konumlar) bölümünde.

### Betikler ve CI için JSON

```bash
# Hatalar ve uygulama kodundaki yerleri, sekmeyle ayrılmış
veylog --bicim json --hepsi laravel.log | jq -r '.kaliplar.satirlar[]
  | select(.seviye == "CRITICAL" or .seviye == "ERROR")
  | "\(.adet)\t\(.istisna.dosya // "-")\t\(.istisna.metot // "-")\t\(.kalip)"'

# CI adımı: logda CRITICAL varsa başarısız ol
veylog --bicim json laravel.log | jq -e '.seviyeler.CRITICAL // 0 | . == 0'

# CI adımı: bir ani artış varsa başarısız ol
veylog --bicim json --ani-artis 5 laravel.log | jq -e '.ani_artislar.artis_sayisi == 0'
```

`--bicim markdown` aynı özeti GitHub issue ve PR açıklamalarına yapıştırılabilecek tablolar
olarak yazar. Alanların hepsi [çıktı biçimleri](docs/cikti-bicimleri.md) belgesinde.

## Nasıl çalışır

```text
laravel.log
  → kayıtlara ayır    59 kayıt, 45 farklı mesaj
  → maskele           27 farklı mesaj
  → Drain             23 kalıp
  → parmak izi        24 grup
  → karşılaştırma     isteğe bağlı (--karsilastir): yeni çıkan, kaybolan, artan
  → ani artışlar      isteğe bağlı (--ani-artis): olağan seviyesinden sıçrayan anlar
  → özet              metin, JSON ya da Markdown
```

1. **Kayıtlara ayırma.** `[2026-10-24 14:03:11] production.ERROR: ...` başlığı (ya da
   `--desen`'e uyan bir satır) veya Monolog'un bir JSON satırı yeni bir kayıt başlatır; stack
   trace gibi satırlar bir sonraki başlığa kadar o kayda bağlanır. Mesaj PHP'nin istisna
   metniyse (`Log::error($e)`) kaydın mesajı istisnanın mesajıdır.
2. **Maskeleme.** URL, dosya yolu, e-posta, UUID, zaman, IP ve sayılar yer tutucu olur:
   `Kullanıcı 381 giriş yaptı` → `Kullanıcı <SAYI> giriş yaptı`.
3. **Drain.** Maskelenmiş mesajlar sabit derinlikli bir ağaçta kümelenir; değişen konumlar
   `<*>` olur. Eşik üç durumda gevşer: iki tarafında da rakam olan farklılıklarda (`120ms`,
   `95ms`), aynı biçimdeki iki kodsal değerde (`ORDER_CREATED`, `ORDER_FAILED`) ve bir
   konumun 100'den fazla farklı sıradan kelime aldığı yaprakta (kullanıcı adları gibi). Her
   `<*>` konumunun en sık değerleri raporda sayılır.
4. **Parmak izi.** Stack trace'ten istisna sınıfı, uygulama kodundaki ilk satır ve o satırın
   içinde bulunduğu fonksiyon çıkarılır
   ([Sentry](https://docs.sentry.io/concepts/data-management/event-grouping/) gibi hata
   takip araçlarının yaptığının sade bir hali). İstisna bağlamdan, JSON'dan ya da PHP'nin
   istisna metninden okunur. Bir yerin fonksiyonu yığın izinde bir sonraki satırda çağrılan
   fonksiyondur; closure'ların sahibi dışa doğru aynı dosyadaki ilk satırdan, çağrıları
   yazılmayan JSON kayıtlarınınki logun öteki yığın izlerinden öğrenilir. Drain'in kalıplarını
   yalnızca böler: aynı mesaj iki yerden geliyorsa iki ayrı hatadır.
5. **İstatistik.** Karşılaştırmada iki Poisson sıklığını karşılaştıran koşullu binom testi;
   ani artışlarda dilim başına kayıt sayılarının medyanı ve MAD'ı.
6. **Canlı izleme.** `--izle` ile depo okuma durumunu korur: eklenen baytlar satırlara,
   satırlar kayıtlara, kayıtlar depoya akar; rapor her çizimde depodan yeniden üretilir.
   Okunan son kayıt, devam satırları gelebileceği için, bir sonraki kayıt başlayınca ya da
   girdi bir aralık boyunca sessiz kalınca tamamlanmış sayılır.

Her adımın ayrıntısı, gerekçesi ve gerçek loglarda nasıl sınandığı
[nasıl çalışır](docs/nasil-calisir.md) ve
[karşılaştırma ve ani artışlar](docs/karsilastirma-ve-ani-artis.md) belgelerinde.

## Tasarım kararları

| Karar | Neden |
|---|---|
| Akışlı okuma | Dosya satır satır okunur, kayıtlar saklanmaz; özet için gerekenler sıkıştırılmış olarak birikir. 2 GB'lık bir log 27 MB bellekle özetlenir ([ayrıntılar](docs/nasil-calisir.md#akışlı-okuma-ve-bellek)). |
| Tek bağımlılık: `regex` | serde ile JSON yazmak bağımlılıkları 5 paketten 15'e, clap ile argüman ayrıştırmak 22'ye çıkarırdı. JSON yalnızca okunuyor ve yazılıyor, kuralları kısa ([RFC 8259](https://www.rfc-editor.org/rfc/rfc8259)); clap'in mesajları İngilizce ve virgüllü ondalığı tanımıyor. |
| Elle yazılan parçalar referansla sınanır | JSON okuyucu serde_json ile (2 000 rastgele değer, gerçek loglardan 17 944 satır), binom kuyruğu scipy ile, medyan ve MAD numpy ile, tarih hesabı Python'un `datetime`'ı ile, kümeleme Drain3 ile karşılaştırılır. |
| Ortalama yerine medyan ve MAD | Ortalama ve standart sapma tek bir patlamayla kayar; medyan ve MAD dilimlerin yarısına kadarı patlama olsa bile yerinde kalır. |
| Kesin binom testi | Az kayıtta da geçerlidir; yanlış alarm oranı %1'i geçmez. Yaklaşık testler (z testi) küçük sayılarda yanılır. |
| Parmak izi fonksiyona bağlı, satıra değil | `Attempt to read property "ad" on null` iki denetleyicide iki ayrı hatadır, ama bir dağıtım satırları kaydırsa da aynı hata aynı kalır; Sentry de satır numarası kullanmaz. Bedeli: aynı fonksiyonda aynı sınıf ve kalıpla oluşan iki hata birleşir; `--konum satir` ayırır. |
| Kontrol karakterleri sembole çevrilir | Log güvenilmez metindir; ESC dizileri ekranı silebilir, satırları gizleyebilir ([CWE-150](https://cwe.mitre.org/data/definitions/150.html)). JSON'da veri kaybı yoktur: ham mesaj kaçışlarıyla yazılır. |
| Türkçe arayüz, İngilizcesi de | Hedef kitle Türkçe konuşan Laravel geliştiricileri: mesajlar, seçenekler (`--esik`, `--eşik`) ve virgüllü ondalık sayılar Türkçeye göre. İngilizce arayüz (`--dil en`) her metnin elle yazılmış karşılığıdır; çeviri kütüphanesi yok. JSON dilden bağımsızdır. |

## Performans

İki çekirdekli bir bulut sanal makinesinde, `--release` derlemesiyle; süre birkaç
çalıştırmanın en kısası, bellek en yüksek yerleşik bellektir (GNU `time`). Dosya satır satır
okunur; bellek dosyanın boyuyla değil, farklı mesajların sayısı ve uzunluğuyla büyür:

| Log | Boyut | Süre | Bellek |
|---|---|---|---|
| Örnek log | 205 satır | < 0,01 sn | 5 MB |
| Gerçek bir geliştirme logu | 13 MB, 64 931 satır | 0,04 sn | 5 MB |
| Sentetik 426 bin satır | 55 MB | 0,9 sn | 12 MB |
| 30 günlük log, `--ani-artis 1` (43 195 dilim) | 92 MB, 524 bin satır | 0,4 sn | 7 MB |
| Sentetik üretim logu | 500 MB, 1,5 milyon kayıt | 3,8 sn | 16 MB |
| Aynısı, biçimi `--desen` ile tarif edilerek | 500 MB | 6,4 sn | 16 MB |
| Aynısı, dört kat büyük | 2 GB, 6,3 milyon kayıt | 15 sn | 27 MB |
| Her satırda başka bir kullanıcı adı | 40 bin satır | 0,2 sn | 23 MB |

0.1.0 dosyanın tamamını belleğe okuyordu: 2 GB'lık log 4 GB bellek istiyor ve 36 saniye
sürüyordu.

Son satır klasik Drain için en kötü durumdur: kısa bir mesajdaki rakamsız bir değer
birleşmez, her değer ayrı bir küme açar. [Değişken konumlar](docs/nasil-calisir.md#değişken-konumlar)
eklenmeden önce aynı log yaklaşık bir dakika sürüyor ve 40 bin satırlık bir tablo
çıkarıyordu; şimdi tek bir kalıp çıkıyor: `Bilinmeyen kullanıcı: <*>`.

## Kalite

- **Testler:** 385 birim testi her modülü ayrı ayrı, 53 uçtan uca test derlenmiş programı
  çalıştırıp kullanıcının gördüğünü sınar: çıktı metinleri karakter karakter, JSON
  serde_json ile ayrıştırılarak, Markdown tablosu satır satır.
- **Referans karşılaştırmaları:** yukarıdaki tabloda. Kümeleme gerçek ve sentetik loglarda
  [Drain3](https://github.com/logpai/Drain3) ile birebir aynı kümeleri veriyor; esnek eşik
  yalnızca eşiği gevşeten farklılıklarda ayrılır
  ([ayrıntılar](docs/nasil-calisir.md#değişken-konumlar)).
- **Mutasyon testleri:** koda bilerek küçük hatalar yerleştirilip (bir karşılaştırmayı
  çevirmek, bir sınırı kaydırmak, bir kuralı silmek) testlerin yakaladığı denetlendi;
  geliştirme adımlarında ve son incelemede denenen 464 mutasyonun 463'ü yakalandı. Kalan
  biri sonucu hiç değiştirmeyen (eşdeğer) bir mutasyondu.
- **CI/CD:** `main` dalına her gönderimde ve her PR'da Linux, macOS ve Windows'ta testler;
  `rustfmt`, `clippy -D warnings` ve en düşük Rust sürümü (1.88). Her sürüm etiketinde dört
  sistem için programlar derlenir ve SHA-256 özetleriyle otomatik yayımlanır.
- **Gerçek loglar:** iki kişisel Laravel projesinin geliştirme logları (13 MB'a kadar) ve
  Laravel 13 ile gerçekten fırlatılan istisnalarla üretilmiş loglar: bağlamda ve
  `Log::error($e)` ile metin olarak; Laravel'in biçiminde, JSON'da ve özel bir satır ve tarih
  biçiminde. Üç biçimde de kalıp tablosu aynıdır. Bir dağıtım da taklit edildi: servisin
  başına satırlar eklenip aynı istekler tekrarlandı; her hata tablonun aynı satırında kaldı.
  Örnek loglar (`ornekler/`) kurgusaldır; IP adresleri dokümantasyon aralıklarından
  ([RFC 5737](https://www.rfc-editor.org/rfc/rfc5737)) ya da yerel ve özel ağdandır
  (`127.0.0.1`, `10.0.3.12`), e-postalar `.test` uzantılıdır.

```bash
cargo test
```

## Sınırlamalar

- Drain sözdizimine bakar, anlama bakmaz: `Kullanıcı <SAYI> giriş yaptı` ile
  `Kullanıcı <SAYI> çıkış yaptı` aynı kalıpta birleşir; raporun `değerler` satırı birleşenleri
  gösterir.
- Kısa mesajlarda sıradan bir kelime olan değişkenler (`Kuyruk durdu: emails`) ancak 100'den
  fazla farklı değer görülünce birleşir; kodsal biçimli değerler hemen birleşir.
- Aynı fonksiyonda aynı istisna sınıfı ve aynı kalıpla oluşan iki hata tek satırdır;
  `--konum satir` ayırır.
- Saat dilimi yazılmamış kayıtlar (Laravel'in varsayılan biçimi) raporun diliminde yazılmış
  sayılır; `--saat-dilimi` onların zamanını değiştiremez.
- Ani artışlarda günlük döngü hesaba katılmaz; bu yüzden yalnızca WARNING ve üstüne bakılır.
- Tek seferde bir dosya okunur; birden çok dosya için `cat laravel-*.log | veylog -`.

Tam liste: [sınırlamalar](docs/sinirlamalar.md).

## Proje yapısı

```text
src/
├── main.rs          girişi açar (dosya ya da standart girdi), özeti istenen biçimde yazar
├── komut_satiri.rs  seçenekler, yardım metni, hata mesajları, yazım hatasına öneri
├── dil.rs           arayüzün dili (--dil): seçili dildeki metin, sayıların ve listelerin yazılışı
├── okuyucu.rs       girdiyi satır satır okur; bir kaydı yeniden okumak için seyrek dizin
├── izle.rs          --izle: eklenen baytları satırlara ayırır, döndürmeyi algılar, raporu yeniden çizer
├── filtre.rs        --seviye, --baslangic, --bitis, --son: sınırların logun sonuna göre çözülmesi
├── kayit.rs         satırları kayıtlara toplar: başlık ya da JSON satırı, seviye, mesaj
├── satir_bicimi.rs  başlık satırının biçimi: Laravel'inki ya da --desen (Monolog dizgesi, düzenli ifade)
├── zaman_bicimi.rs  --zaman-bicimi: PHP'nin date() dizgesiyle yazılmış zamanları okur
├── depo.rs          okunurken her kayıttan özet için gerekenler: alt gruplar, sütunlar, kökler
├── sutunlar.rs      kayıtların zamanları ve alt grupları, delta ve LEB128 ile sıkıştırılmış
├── farkli_sayac.rs  farklı mesaj sayacı: tam, çok sayıda farklı değerde HyperLogLog
├── json_okuyucu.rs  Monolog'un JSON satırları için kütüphanesiz JSON okuyucu
├── maske.rs         değişkenleri yer tutucularla değiştirir: <SAYI>, <IP>, <YOL>...
├── drain.rs         Drain: maskelenmiş mesajlardan kalıp çıkarır; esnek eşik, kodsal biçimler, değişken konumlar
├── parmak_izi.rs    istisna sınıfı, uygulama kodundaki ilk satır ve fonksiyonu
├── istisna_metni.rs PHP'nin istisna metni (Log::error($e)): sınıf, yer, yığın izi, Next
├── rapor.rs         özet ve kalıp tablosu; metin çıktısı, kontrol karakterlerinin temizlenmesi
├── karsilastirma.rs iki zaman aralığının karşılaştırılması
├── ani_artis.rs     dilimler, medyan ve MAD, değiştirilmiş z puanı, çift eşik
├── istatistik.rs    binom dağılımının kuyruğu; seyrek dizilerde medyan ve MAD
├── zaman.rs         tarih ve saat hesapları, saat dilimleri
├── json.rs          JSON çıktısı
└── markdown.rs      Markdown çıktısı
tests/cli.rs         derlenmiş programı çalıştıran uçtan uca testler
docs/                ayrıntılı belgeler
ornekler/            kurgusal örnek loglar
```

## Yol haritası

- [x] Dosyayı satır satır okumak: bellek kullanımı dosyanın boyutundan bağımsız
- [ ] Birden çok dosyayı ve `.gz` arşivlerini doğrudan okumak
- [x] Saat dilimlerini hesaba katmak
- [x] `Log::error($e)` ile metne çevrilmiş istisnalar; kendi satır ve tarih biçimleri
- [x] Satır numarasından bağımsız parmak izi: hata, satırın içinde bulunduğu fonksiyonla tanınır
- [x] Kategorik konumlar: koddan gelen değerler biçimleriyle tanınır, en sık değerleri raporda
- [x] Canlı izleme: `--izle` ile log eklendikçe rapor yeniden çizilir
- [x] İngilizce arayüz: `--dil en`

Sürümlerde nelerin değiştiği: [CHANGELOG.md](CHANGELOG.md).

## Katkı

Hata bildirimleri ve öneriler için bir [issue](https://github.com/Veysel440/veylog/issues)
açabilirsiniz. Bir log biçimi tanınmıyorsa, kişisel bilgileri çıkarılmış kısa bir örnek
çok yardımcı olur. Pull request'lerde şu komutların temiz geçmesi beklenir:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Lisans

[MIT](LICENSE) © 2026 Veysel Tulu

## English

veylog is a fast command-line tool, written in Rust, that turns Laravel log files into a
readable summary. It masks variable parts of messages, groups them into patterns with the
Drain algorithm, fingerprints exceptions by class, first application frame and the function
enclosing it (skipping vendor code, entry points and middleware pass-through frames, and
pointing Blade errors at the template), so an error keeps its identity when a deploy shifts
line numbers; closures are told apart by the function that defines them, and JSON records,
whose traces carry no calls, learn their functions from the other traces in the log.
Code-like values (class names, constants, event names, quoted keys) are told apart from
ordinary words by their shape, so categorical positions merge without waiting for a hundred
distinct values, and the report lists the most frequent values of every `<*>` position. It
compares two time ranges with an exact conditional binomial test, and detects sudden spikes
with median/MAD-based modified z-scores. It reads Laravel's text format and Monolog's JSON
lines, handles Windows paths, and writes text, JSON or Markdown. Timestamps with UTC offsets
are converted to a single report time zone, so logs that span a daylight saving change sort
correctly. Exceptions logged as strings (`Log::error($e)`) are parsed from PHP's own text
and grouped with the same exception logged through the context; custom line formats are
given as a Monolog format string or a regex with named groups (`--pattern`), and custom date
formats as a PHP `date()` format (`--time-format`). The file is streamed line by line into a
compact store, so a 2 GB log is summarized in 15 seconds with 27 MB of memory. `--watch`
follows a log live, like `tail -f`: new records are fed into the same store, the report is
redrawn as they arrive, time filters become a sliding window and rotated files are picked
up. Its only dependency is `regex`; the JSON reader and writer, statistics and argument
parser are hand-written and tested against serde_json, scipy and Drain3. Prebuilt binaries
for Linux (static), macOS and Windows are on the
[Releases](https://github.com/Veysel440/veylog/releases/latest) page; `cargo install veylog`
also works.

The interface is in Turkish by default. `--lang en`, or `VEYLOG_DIL=en` in the environment,
switches the help, the report and every message to English:

```text
$ veylog --lang en ornekler/laravel.log
ornekler/laravel.log: 205 lines
  Records       : 59
  Continuations : 143
  Orphan lines  : 3
  Time range    : 2026-10-24 08:59:58 → 2026-10-24 18:31:07
...
Patterns: 45 distinct messages → 27 after masking → 23 Drain patterns (threshold 0.7) → 24 groups by fingerprint
  count  level      first     last      pattern
      2  CRITICAL   17:05:13  17:06:45  Redis bağlantısı kurulamadı: Connection refused [tcp://<IP>:<NUM>]
                                        sample: Redis bağlantısı kurulamadı: Connection refused [tcp://10.0.3.12:6379]
                                        location: app/Http/Middleware/HizSiniri.php:22, handle() (RedisException)
...
```

Every option has an English name, accepted in either language: `--threshold`, `--location`
(`method`, `line`), `--top`, `--all`, `--sort` (`severity`, `count`, `newest`), `--format`
(`text`, `json`, `markdown`), `--compare`, `--spikes`, `--level`, `--since`, `--until`,
`--last`, `--timezone`, `--pattern`, `--time-format`, `--watch`, `--interval` and `--lang`.
Durations take `s`, `m`, `h`, `d` (and `ms` for `--interval`): `--last 1h30m`. Placeholders
are shown in English (`<NUM>`, `<PATH>`, `<EMAIL>`, `<TIME>`). The JSON output is the same in
both languages, so scripts do not break when the language changes; its keys and placeholders
are Turkish: `<SAYI>` is a number, `<YOL>` a path, `<EPOSTA>` an e-mail address and
`<ZAMAN>` a timestamp. `veylog --lang en --help` lists everything.
