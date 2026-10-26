# veylog

Laravel log dosyalarını okuyup özetleyen, Rust ile yazılmış bir komut satırı aracı.

## Neden?

Bir Laravel uygulamasının `storage/logs/laravel.log` dosyası binlerce satıra ulaşabilir.
Aynı hata yüzlerce kez tekrarlanır, stack trace'ler satır sayısını şişirir ve gerçekten
önemli olan kayıtlar bu kalabalıkta kaybolur. veylog'un amacı bu dosyayı birkaç saniyede
okunabilir bir özete çevirmek: kaç kayıt var, hangi seviyede, hangi hatalar tekrar ediyor,
ne zaman başladı ve kodun neresinde oluşuyor.

## Şu an ne yapıyor?

- Log dosyasını kayıtlara ayırır: `[2026-10-24 14:03:11] production.ERROR: ...` biçimindeki
  başlık satırı yeni bir kayıt başlatır; stack trace gibi devam satırları bir sonraki başlığa
  kadar o kayda bağlanır.
- Laravel'in tarih biçimini (`2026-10-24 14:03:11`) ve Monolog'un varsayılanı olan ISO 8601
  biçimini (`2026-10-24T14:03:11.123456+03:00`) tanır.
- Dosya bir kaydın ortasından başlıyorsa, hiçbir kayda ait olmayan bu ilk satırları
  "yetim satır" olarak ayrıca sayar.
- Logun kapsadığı zaman aralığını gösterir.
- Kayıtları seviyelerine göre (DEBUG → EMERGENCY) sayar; en önemlisi en üstte.
- Mesajlardaki değişken kısımları (URL, e-posta, UUID, zaman, IP, sayı) yer tutucularla
  maskeler: `Kullanıcı 381 giriş yaptı` → `Kullanıcı <SAYI> giriş yaptı`.
- Maskelenmiş mesajları [Drain](#drain) algoritmasıyla kalıplara ayırır:
  `Zamanlanmış görev başladı: kurlar:guncelle` ile `Zamanlanmış görev başladı: yedek:al`
  → `Zamanlanmış görev başladı: <*>`.
- Kısa mesajlardaki rakamlı değişkenleri de birleştirir: `Yanıt süresi 120ms` ile
  `Yanıt süresi 95ms` → `Yanıt süresi <*>`.
- Hata kayıtlarının stack trace'inden bir [parmak izi](#hata-parmak-izi) çıkarır: istisna
  sınıfı ve uygulama kodundaki ilk satır (`app/Services/SepetHesaplayici.php:88`). Aynı mesaj
  kodun farklı yerlerinden geliyorsa ayrı satırlarda gösterir.
- Her satır için en yüksek seviyeyi, ilk ve son görülme zamanını, maskelenmemiş bir örnek
  mesajı ve hatanın konumunu gösterir. Tablo önem sırasına göre dizilir: en ciddi hatalar
  en üstte.
- Benzerlik eşiği, tablonun uzunluğu ve sırası [komut satırından](#kullanım) değiştirilebilir:
  `--esik 0,8`, `--ilk 20`, `--hepsi`, `--siralama adet`. Yazım hatalarında öneri verir.
- Geçersiz UTF-8 baytları içeren dosyalarda durmaz; bu baytları `�` ile değiştirip uyarır.
- Çıktısı başka bir programa yönlendirilebilir (`veylog laravel.log | head`); okuyan program
  erken kapansa da çökmez.

```text
$ veylog ornekler/laravel.log
ornekler/laravel.log: 203 satır
  Kayıt         : 59
  Devam satırı  : 141
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
                                       konum: app/Http/Middleware/HizSiniri.php:22 (RedisException)
     6  ERROR      16:10:02  16:16:02  cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> by…
                                       örnek: cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes…
                                       konum: app/Services/OdemeServisi.php:64 (ConnectionException)
     4  ERROR      15:02:11  16:58:12  Undefined array key "kargo_ucreti"
                                       konum: app/Services/SepetHesaplayici.php:88 (ErrorException)
     3  ERROR      09:14:20  17:55:03  SQLSTATE[<SAYI>]: Integrity constraint violation: <SAYI> Duplicate entry '<EPOS…
                                       örnek: SQLSTATE[23000]: Integrity constraint violation: 1062 Duplicate entry 'a…
                                       konum: app/Http/Controllers/KayitController.php:38 (UniqueConstraintViolationException)
     1  ERROR      09:47:12  09:47:12  Attempt to read property "ad" on null
                                       konum: app/Http/Controllers/KategoriController.php:54 (ErrorException)
     1  ERROR      11:30:44  11:30:44  App\Jobs\FaturaGonder has been attempted too many times.
                                       konum: uygulama kodunda bulunamadı (MaxAttemptsExceededException)
     1  ERROR      13:18:20  13:18:20  Attempt to read property "ad" on null
                                       konum: app/Http/Controllers/UrunController.php:72 (ErrorException)
     3  WARNING    16:10:03  16:12:41  Ödeme sağlayıcısı yanıt vermedi, sipariş <SAYI> için yeniden denenecek (deneme …
                                       örnek: Ödeme sağlayıcısı yanıt vermedi, sipariş 1047 için yeniden denenecek (de…
     1  WARNING    09:33:05  09:33:05  Yavaş sorgu (<SAYI> ms): select * from `siparisler` where `kullanici_id` = <SAY…
                                       örnek: Yavaş sorgu (1532 ms): select * from `siparisler` where `kullanici_id` =…
     1  WARNING    10:58:51  10:58:51  Yavaş sorgu (<SAYI> ms): select * from `urunler` where `kategori_id` = <SAYI> a…
                                       örnek: Yavaş sorgu (2210 ms): select * from `urunler` where `kategori_id` = 12 …
  … 14 satır daha; tümünü görmek için --hepsi
```

Örnek logdaki tablo bir günün hikâyesini anlatıyor: saat 15:00'teki deploy'dan iki dakika
sonra başlayan `kargo_ucreti` hatası, 16:10–16:16 arasındaki ödeme zaman aşımları ve
17:05'teki Redis çöküşü. `Attempt to read property "ad" on null` ise iki ayrı satırda
görünüyor: mesajları aynı ama biri kategori sayfasında, diğeri ürün sayfasında oluşan iki
ayrı hata.

## Nasıl çalışıyor?

```text
laravel.log
  → kayıtlara ayır    59 kayıt, 45 farklı mesaj
  → maskele           27 farklı mesaj
  → Drain             23 kalıp
  → parmak izi        24 grup
  → kalıp raporu
```

### Maskeleme

Değişken kısımlar düzenli ifadelerle şu sırayla yer tutuculara çevrilir: URL, e-posta, UUID,
zaman, IP, sayı. Sıra önemlidir: geniş yapılar önce, tek tek sayılar en son yakalanır. Yoksa
`10.0.3.12` bir IP olarak değil, sayılara bölünmüş olarak maskelenirdi.

### Drain

Kalıp çıkarma için He ve arkadaşlarının
[Drain](http://jiemingzhu.github.io/pub/pjhe_icws2017.pdf) algoritması (ICWS 2017) kullanılır.
Mesajlar teker teker işlenir:

1. Mesaj boşluklardan belirteçlere bölünür.
2. Sabit derinlikli bir ağaçta önce belirteç sayısına, sonra ilk belirtece göre dal seçilir.
   İçinde rakam olan belirteçler ortak `<*>` dalına gider. Böylece her mesaj bütün kalıplarla
   değil, yalnızca birkaç adayla karşılaştırılır.
3. Varılan yapraktaki her kalıp için benzerlik hesaplanır: aynı konumda aynı olan
   belirteçlerin oranı.
4. En iyi benzerlik eşiği geçerse mesaj o kümeye katılır ve kalıpta farklı olan konumlar
   `<*>` olur; geçmezse yeni bir küme açılır.

```text
Zamanlanmış görev başladı: kurlar:guncelle
Zamanlanmış görev başladı: yedek:al
→ 4 belirtecin 3'ü aynı: 3/4 = 0,75 ≥ 0,7
→ Zamanlanmış görev başladı: <*>
```

| Ayar | Varsayılan | Açıklama |
|---|---|---|
| Önek derinliği | 1 | Kaç baş belirtece göre dallanılacağı. Laravel hata mesajlarında değişken çoğu zaman ikinci sıradadır (`View [x] not found.`), bu yüzden yalnızca ilk belirteç kullanılır. |
| Benzerlik eşiği | 0,7 | Bir mesajın kümeye katılması için gereken en düşük benzerlik. `--esik` ile değiştirilebilir. |
| En fazla çocuk | 100 | Bir düğümün en fazla dal sayısı; dolunca yeni belirteçler `<*>` dalına gider. |
| Rakama duyarlı eşik | açık | Eşik, rakam içeren farklılıkların oranında gevşer (aşağıda). |

Eşiğin örnek logdaki etkisi:

| Eşik | Kalıp | Etkisi |
|---|---|---|
| 0,4 | 22 | İki farklı yavaş SQL sorgusu anlamsız tek bir kalıpta birleşir. |
| **0,7** | 23 | Sorgular ayrı kalır; zamanlanmış görevler birleşir. |
| 0,8 | 25 | `giriş` ve `çıkış` ayrılır, ama zamanlanmış görevler de ayrılır. |

Yanlışlıkla birleştirilen yeni bir hata eski bir kalıbın içinde kaybolacağı için varsayılan
eşik temkinli seçildi.

### Rakama duyarlı eşik

Sabit bir eşik kısa mesajlarda sorun çıkarır: `Yanıt süresi 120ms` ile `Yanıt süresi 95ms`
mesajlarında 3 belirtecin 2'si aynıdır (0,67 < 0,7) ve her değer ayrı bir kalıp olur. Eşiği
düşürmek ise uzun mesajlarda farklı SQL sorgularını birleştirir. Bunun yerine, Drain'in
[genişletilmiş sürümündeki](https://arxiv.org/pdf/1806.04356) fikir uygulanır: içinde rakam
olan belirteçler büyük ihtimalle değişkendir, eşik onların oranında gevşer.

```text
eşik = 0,7 × (n − rakamlı fark) / n
```

Burada `rakamlı fark`, farklı olan (ya da kalıpta `<*>` olan) ve iki taraftan en az birinde
rakam bulunan konumların sayısıdır.

| Mesajlar | Rakamlı fark | Eşik | Benzerlik | Sonuç |
|---|---|---|---|---|
| `Yanıt süresi 120ms` / `95ms` | 1 | 0,47 | 0,67 | Birleşir: `Yanıt süresi <*>` |
| `Ödeme alındı` / `Ödeme reddedildi` | 0 | 0,70 | 0,50 | Ayrı kalır |
| `S3 yüklemesi tamamlandı` / `başarısız` | 0 | 0,70 | 0,67 | Ayrı kalır |

Makaledeki formülden farkı, bütün rakamlı belirteçleri değil yalnızca rakamlı
**farklılıkları** saymasıdır. `S3` gibi rakam içeren ama sabit olan bir kelime bu yüzden
eşiği gevşetmez.

Geliştirme sırasında [Drain3](https://github.com/IBM/Drain3) (IBM, Python) ile
karşılaştırıldı: rakama duyarlı eşik kapalıyken aynı kümeleri, açıkken de rakam içermeyen
mesajlarda aynı kümeleri veriyor.

### Hata parmak izi

Aynı PHP mesajı kodun birçok yerinden gelebilir: `Attempt to read property "ad" on null`
hem kategori sayfasında hem ürün sayfasında oluşabilir. Drain yalnızca mesaja baktığı için
bunları tek kalıpta birleştirir, oysa bunlar iki ayrı hatadır. [Sentry](https://docs.sentry.io/concepts/data-management/event-grouping/)
gibi hata takip araçları bu yüzden hataları mesajla değil, stack trace'in uygulamaya ait
kısmıyla tanır. veylog bunun sade bir halini kullanır: istisna sınıfı ve uygulama kodundaki
ilk satır.

Laravel bir istisnayı loglarken bağlama sınıfını, fırlatıldığı yeri ve stack trace'i yazar
(satırlar kısaltıldı):

```text
[2026-10-24 09:14:20] production.ERROR: SQLSTATE[23000]: ... {"exception":"[object] (Illuminate\\Database\\UniqueConstraintViolationException(code: 23000): ... at /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php:819)
[stacktrace]
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Connection.php(779): ...
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Query/Builder.php(3780): ...
#2 /var/www/magaza/app/Http/Controllers/KayitController.php(38): Illuminate\\Database\\Eloquent\\Model->save()
#3 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/ControllerDispatcher.php(46): ...
#4 /var/www/magaza/public/index.php(17): ...
#5 {main}
"}
```

Parmak izi şöyle çıkarılır:

1. Bağlamda `[object] (Sınıf(code: ...` başlığı aranır; buradan istisna sınıfı alınır.
   Mesaj çok satırlıysa bağlam bir sonraki satırlardan birinde başlar.
2. Başlık `[stacktrace]` satırına kadar sürer, çünkü istisna mesajı da çok satırlı olabilir.
   Son satırının sonundaki `at dosya:satır` istisnanın fırlatıldığı yerdir.
3. Stack trace'in çerçeveleri (`#2 dosya(satır): ...`) okunur. `[previous exception]` ile
   başlayan kısım bu istisnaya yol açan önceki istisnadır; okunmaz.
4. Proje kökü, `/vendor/` içeren ilk yoldan bulunur (`/var/www/magaza/`) ve yollardan silinir.
   Böylece her dağıtımda klasör adı değişse de (`releases/20261024/`) konum aynı kalır.
5. Adaylar içten dışa taranır, önce fırlatıldığı yer, sonra onu çağıranlar. `vendor/`
   altında olmayan ve giriş noktası (`artisan`, `public/index.php`) olmayan ilk dosya seçilir.

Örnekteki istisna vendor'da fırlatıldı; uygulama kodundaki ilk satır `#2`'dir:
`app/Http/Controllers/KayitController.php:38 (UniqueConstraintViolationException)`. Hata
tamamen vendor içinde oluştuysa, örneğin bir kuyruk işinin deneme hakkı bittiyse, konum
"uygulama kodunda bulunamadı" olarak gösterilir.

Parmak izi Drain'in kalıplarını yalnızca **böler**: aynı kalıptaki kayıtlar farklı yerlerden
geliyorsa ayrı satırlarda görünür. Aynı yerden gelen farklı kalıplar ise birleştirilmez,
çünkü bir yardımcı fonksiyondaki tek bir `throw` birbirinden bağımsız birçok hatanın ortak
yeri olabilir.

Algoritma, Laravel 13.38 ile üretilen gerçek bir log üzerinde doğrulandı: Eloquent, Blade,
HTTP istemcisi ve Artisan hataları, önceki istisnası olan ve mesajı çok satırlı olan
istisnalar.

### Kalıp raporu

Drain yalnızca metinle ilgilenir; seviyeyi, zamanı ve stack trace'i bilmez. Rapor, Drain'in
her mesaj için döndürdüğü küme numarasını kaydın parmak iziyle birlikte kullanır: ikisi de
aynı olan kayıtlar tablonun bir satırını oluşturur. Her satıra şu bilgiler eklenir:

- **seviye:** satırdaki kayıtların en önemlisi.
- **ilk / son:** ilk ve son görülme zamanı. Tek günlük logda yalnızca saat, birden çok günü
  kapsayan logda tarih de gösterilir (`10-23 23:59:58`).
- **örnek:** satırdaki ilk kaydın maskelenmemiş mesajı; `<SAYI>` ve `<*>`'ın arkasındaki
  gerçek değerleri gösterir. Kalıp ham mesajla aynıysa gösterilmez.
- **konum:** kayıtlar bir istisnadan geliyorsa uygulama kodundaki ilk satır ve istisna
  sınıfı. Dosya yolu kısaltılmaz; birçok editörün terminalinde `dosya:satır` tıklanarak
  açılabilir.

Tablo varsayılan olarak önem sırasına göre dizilir: önce seviye, aynı seviyede adet. İlk 10
satır gösterilir; gösterilmeyen satır sayısı tablonun altında yazar. Üç sıralama vardır
(`--siralama`):

| Sıralama | Ölçüt | Ne zaman işe yarar? |
|---|---|---|
| `onem` | Seviye, aynı seviyede adet | Önce neyi düzeltmeli? |
| `adet` | Adet, aynı adette seviye | Logu en çok ne dolduruyor? |
| `yeni` | İlk görülme zamanı (en yeni önce), aynı anda seviye | Son dağıtımdan sonra ne çıktı? |

Bütün sıralamalar kararlıdır: ölçütleri eşit olan satırlar logda ilk görüldükleri sırada
kalır. Örnek logda `--siralama yeni` günü sondan başa anlatır: 17:07'de toparlanan Redis,
17:05'teki çöküşü, 16:10'daki ödeme zaman aşımları...

### Komut satırı

Argümanlar için yaygın bir kütüphane ([clap](https://crates.io/crates/clap)) yerine küçük
bir ayrıştırıcı yazıldı. clap projeye 17 kütüphane ekliyor, mesajları İngilizce ve Türkçedeki
gibi virgüllü ondalık sayıları (`0,8`) tanımıyor. Kendi ayrıştırıcımız şu kuralları izler:

- Seçenekler dosya adından önce ya da sonra gelebilir. Aynı seçenek iki kez yazılırsa
  sonuncusu geçerlidir (`--ilk 5 --hepsi` hepsini gösterir).
- Değer boşlukla (`--esik 0,8`), `=` ile (`--esik=0,8`) ya da kısa seçeneklerde bitişik
  (`-n20`) verilebilir.
- Tek başına `-` ve `--`'dan sonraki her şey dosya adıdır: `veylog -- -garip-ad.log`.
- Türkçe karakterli yazımlar da geçerlidir: `--eşik`, `--sıralama önem`, `--yardım`.
- Argümanlar işletim sisteminin verdiği ham haliyle (`OsString`) okunur. Linux'ta bir dosya
  adı geçerli UTF-8 olmayabilir; program böyle bir adla da çökmez.
- Tanınmayan bir seçeneğe en çok benzeyen seçenek önerilir. Benzerlik
  [Levenshtein uzaklığıyla](https://en.wikipedia.org/wiki/Levenshtein_distance) ölçülür:
  birini ötekine çevirmek için gereken en az harf ekleme, silme ya da değiştirme sayısı
  (`--esk` → `--esik`: 1). Yanlış öneri vermemek için 5 harften kısa adlarda en fazla 1,
  uzunlarda en fazla 2 harf farka izin verilir.

### Bilinen sınırlamalar

- Drain sözdizimine bakar, anlama bakmaz: `Kullanıcı <SAYI> giriş yaptı` ile
  `Kullanıcı <SAYI> çıkış yaptı` aynı yapıda olduğu için `Kullanıcı <SAYI> <*> yaptı`
  kalıbında birleşir.
- Kısa mesajlarda rakam içermeyen bir değişken hâlâ her değer için ayrı kalıp oluşturur
  (ör. `İşlendi: App\Jobs\FaturaGonder` ile `İşlendi: App\Jobs\EpostaGonder`).
- Boşluk içeren bir değer (ör. `Ali Veli`) belirteç sayısını değiştirdiği için aynı kalıptan
  gelen mesajlar farklı kümelere düşebilir. Parmak izi kalıpları birleştirmediği için bu
  kayıtlar aynı yerden gelseler de ayrı satırlarda kalır.
- Parmak izi satır numarasını içerir: bir dağıtımda dosyanın üst kısmı değişirse aynı hata
  iki ayrı satırda görünebilir. Sentry gibi araçlar bu yüzden satır numarası kullanmaz.
- Yalnızca Laravel'in istisna biçimi tanınır: yakalanmayan istisnalar, `report($e)` ve
  `Log::error('...', ['exception' => $e])`. İstisnayı metne çeviren `Log::error($e)`
  kullanımı (`Stack trace:` biçimi) tanınmaz.
- Blade hatalarında konum, derlenmiş görünüm dosyasıdır (`storage/framework/views/…`);
  asıl Blade dosyası mesajın sonunda `(View: …)` olarak yazar.
- Windows yolları (`C:\laragon\www\...`) henüz tanınmıyor: vendor klasörü ayırt edilemediği
  için konum, istisnanın fırlatıldığı yer olur.
- Aynı dosyada farklı zaman biçimleri ya da saat dilimleri karışırsa zaman aralığı ve
  `--siralama yeni` yanlış sıralanabilir.
- Tek seferde yalnızca bir log dosyası okunur.
- Dosyanın tamamı belleğe okunuyor; çok büyük dosyalar için satır satır okuma ileride eklenecek.

## Kurulum

[Rust](https://rustup.rs) 1.88 veya daha yeni bir sürüm gerekir.

```bash
git clone https://github.com/Veysel440/veylog.git
cd veylog
cargo install --path .
```

`cargo install` programı derleyip `~/.cargo/bin` klasörüne koyar; rustup bu klasörü PATH'e
ekler, böylece `veylog` her klasörden çalışır. Kodu değiştirdikten sonra aynı komut programı
günceller. Yalnızca derlemek için `cargo build --release` yeterlidir; program
`target/release/veylog` konumunda oluşur.

## Kullanım

```text
veylog [SEÇENEKLER] <log-dosyası>
```

| Seçenek | Açıklama | Varsayılan |
|---|---|---|
| `-e`, `--esik <sayı>` | Drain benzerlik eşiği, 0 ile 1 arası. Büyüdükçe kalıplar daha az birleşir. | 0,7 |
| `-n`, `--ilk <sayı>` | Tabloda gösterilecek satır sayısı. | 10 |
| `--hepsi` | Tablonun tamamını göster. | |
| `-s`, `--siralama <ölçüt>` | Tablonun sırası: `onem`, `adet` ya da `yeni` ([ayrıntılar](#kalıp-raporu)). | `onem` |
| `-h`, `--yardim` | Yardımı göster (`--help` da olur). | |
| `-V`, `--surum` | Sürümü göster (`--version` da olur). | |

```bash
# Varsayılan ayarlarla
veylog storage/logs/laravel.log

# Daha sıkı eşik, ilk 20 satır
veylog --esik 0,8 --ilk 20 storage/logs/laravel.log

# Son ortaya çıkan sorunlar önce, tablonun tamamı
veylog --siralama yeni --hepsi storage/logs/laravel.log

# Derlemeden çalıştırırken: cargo'nun seçenekleriyle karışmasın diye önce --
cargo run -- --ilk 5 ornekler/laravel.log
```

Çıkış kodları: `0` başarılı, `1` dosya okunamadı ya da çıktı yazılamadı, `2` kullanım hatası
(bilinmeyen seçenek, geçersiz değer, eksik dosya adı). Böylece veylog'u çağıran bir betik
hatanın türünü ayırt edebilir.

```text
$ veylog --esk 0,8 laravel.log
veylog: bilinmeyen seçenek: --esk (bunu mu demek istediniz: --esik)
Kullanım: veylog [SEÇENEKLER] <log-dosyası>
Ayrıntılar için: veylog --yardim
```

## Proje yapısı

```text
src/
├── main.rs         programın girişi: dosyayı okur, raporu yazar, çıkış kodlarını belirler
├── komut_satiri.rs komut satırı: seçenekler, yardım metni, hata mesajları
├── kayit.rs        log metnini kayıtlara ayırır: başlık, seviye, mesaj, bağlam, devam satırları
├── maske.rs        mesajlardaki değişkenleri (sayı, IP, e-posta...) yer tutucularla değiştirir
├── drain.rs        Drain algoritması: maskelenmiş mesajlardan kalıp çıkarır
├── parmak_izi.rs   hata parmak izi: istisna sınıfı ve uygulama kodundaki ilk satır
└── rapor.rs        özet raporu: sayılar, seviyeler, kalıp tablosu ve sıralamalar
tests/
└── cli.rs          programı gerçekten çalıştıran testler: argümanlar, hata mesajları, çıkış kodları
```

Dış kütüphane olarak yalnızca [regex](https://crates.io/crates/regex) kullanılır.

## Testler

```bash
cargo test
```

İki tür test çalışır: `src` içindeki birim testleri her parçayı ayrı ayrı sınar;
`tests/cli.rs` ise derlenmiş programı çalıştırıp kullanıcının gördüğünü kontrol eder.

## Örnek log hakkında

`ornekler/laravel.log` tamamen **kurgusaldır**: bir e-ticaret sitesinin uydurma bir gününü
anlatır. İçindeki IP adresleri dokümantasyon için ayrılmış aralıklardan, e-posta adresleri
`.test` uzantılıdır; gerçek bir sisteme ya da kişiye ait veri içermez.

## Yol haritası

- [x] Kayıt başlıklarını tanıma ve sayma
- [x] Seviyelere göre sayım
- [x] Kodu modüllere ayırma
- [x] Çok satırlı kayıtları (stack trace) kayda bağlama, zaman aralığını gösterme
- [x] Değişken kısımları maskeleme (sayı, IP, e-posta, UUID → `<SAYI>`, `<IP>`...)
- [x] Drain algoritmasıyla tekrar eden kalıpları bulma
- [x] Dayanıklı okuma: ISO 8601 zaman, geçersiz UTF-8, yönlendirilen çıktı
- [x] Rakama duyarlı benzerlik eşiği: kısa mesajlardaki sayısal değişkenler
- [x] Kalıp raporu: en yüksek seviye, ilk/son görülme zamanı, örnek mesaj
- [x] Hata parmak izi: istisna sınıfı ve uygulamadaki ilk satır
- [x] Komut satırı seçenekleri: benzerlik eşiği, gösterilecek kalıp sayısı, sıralama
- [ ] JSON ve Markdown çıktısı
- [ ] İki zaman aralığını karşılaştırma: yeni çıkan, kaybolan ve artan hatalar
- [ ] Ani hata artışlarını yakalama (medyan + MAD)