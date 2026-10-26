# veylog

Laravel log dosyalarını okuyup özetleyen, Rust ile yazılmış bir komut satırı aracı.

## Neden?

Bir Laravel uygulamasının `storage/logs/laravel.log` dosyası binlerce satıra ulaşabilir.
Aynı hata yüzlerce kez tekrarlanır, stack trace'ler satır sayısını şişirir ve gerçekten
önemli olan kayıtlar bu kalabalıkta kaybolur. veylog'un amacı bu dosyayı birkaç saniyede
okunabilir bir özete çevirmek: kaç kayıt var, hangi seviyede, hangi hatalar tekrar ediyor,
ne zaman başladı, kodun neresinde oluşuyor ve bir dağıtımdan sonra ne değişti.

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
- Logu bir anda ikiye bölüp iki yarıyı [karşılaştırır](#zaman-aralıklarını-karşılaştırma):
  o andan sonra yeni çıkan, kaybolan ve belirgin şekilde artan kalıplar
  (`--karsilastir 15:00`). Artışın tesadüf olup olmadığını bir istatistik testiyle sınar.
- Benzerlik eşiği, tablonun uzunluğu, sırası ve biçimi [komut satırından](#kullanım)
  değiştirilebilir: `--esik 0,8`, `--ilk 20`, `--hepsi`, `--siralama adet`, `--bicim json`.
  Yazım hatalarında öneri verir.
- Özeti üç biçimde yazar: terminal için metin, başka programlar (jq, CI adımları) için
  [JSON](#json-ve-markdown-çıktısı) ve GitHub issue'larına yapıştırmak için Markdown.
- Log mesajlarındaki kontrol karakterlerini (ESC, CR...) görünür sembollere çevirir: logu
  yazabilen biri raporun ekranda nasıl göründüğünü değiştiremez
  ([terminal güvenliği](#terminal-güvenliği)).
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
ayrı hata. `--karsilastir 15:00` aynı günü deploy öncesi ve sonrası diye ikiye ayırır
([aşağıda](#zaman-aralıklarını-karşılaştırma)).

## Nasıl çalışıyor?

```text
laravel.log
  → kayıtlara ayır    59 kayıt, 45 farklı mesaj
  → maskele           27 farklı mesaj
  → Drain             23 kalıp
  → parmak izi        24 grup
  → karşılaştırma     isteğe bağlı (--karsilastir): yeni çıkan, kaybolan, artan
  → özet              metin, JSON ya da Markdown olarak yazılır
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
satır gösterilir; gösterilmeyen satır sayısı tablonun altında (JSON'da `gosterilmeyen`
alanında) yazar. Üç sıralama vardır (`--siralama`):

| Sıralama | Ölçüt | Ne zaman işe yarar? |
|---|---|---|
| `onem` | Seviye, aynı seviyede adet | Önce neyi düzeltmeli? |
| `adet` | Adet, aynı adette seviye | Logu en çok ne dolduruyor? |
| `yeni` | İlk görülme zamanı (en yeni önce), aynı anda seviye | Son dağıtımdan sonra ne çıktı? |

Bütün sıralamalar kararlıdır: ölçütleri eşit olan satırlar logda ilk görüldükleri sırada
kalır. Örnek logda `--siralama yeni` günü sondan başa anlatır: 17:07'de toparlanan Redis,
17:05'teki çöküşü, 16:10'daki ödeme zaman aşımları...

### Zaman aralıklarını karşılaştırma

"Dağıtımdan sonra ne değişti?" sorusu için log bir anda ikiye bölünür: `--karsilastir 15:00`.
Önce yarısı ilk kayıttan o ana kadar, sonra yarısı o andan son kayda kadar sürer; tam o
andaki kayıt sonraya sayılır. Kalıplar bütün log üzerinde bir kez çıkarılır, sonra her tablo
satırının kayıtları iki yarıya sayılır ve satır üç bölümden birine girer:

| Bölüm | Şart |
|---|---|
| Yeni çıkanlar | Yalnızca sonra görülen kalıplar |
| Kaybolanlar | Yalnızca önce görülen kalıplar |
| Artanlar | İkisinde de görülen ve sonra en az 2 kat sık görülen kalıplar; sonra yarısında en az 2 kaydı olmalı ve artış tesadüf olmamalı |

Örnek logda 15:00'te bölünce dağıtımdan sonra başlayan `kargo_ucreti` hatası "yeni
çıkanlar" arasındadır. Ödeme zaman aşımlarının hepsi 15:00'ten sonra olduğu için onlar da
yenidir; zaman aşımlarının ortasında, 16:11'de bölünce artan olarak görünürler:

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
                                              konum: app/Http/Middleware/HizSiniri.php:22 (RedisException)
  … 2 satır daha; tümünü görmek için --hepsi

Kaybolanlar (yalnızca önce): 11
  önce  sonra  seviye     ilk       son       kalıp
     1      0  ERROR      09:47:12  09:47:12  Attempt to read property "ad" on null
                                              konum: app/Http/Controllers/KategoriController.php:54 (ErrorException)
  … 10 satır daha; tümünü görmek için --hepsi

Artanlar (sonra ≥ 2 kat sık, tesadüf ihtimali < %1): 1
  önce  sonra  seviye     ilk       son       kalıp
     1      5  ERROR      16:10:02  16:16:02  cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> by…
                                              artış: saatte 0,1 → 2,1 (15,4 kat)
                                              örnek: cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes…
                                              konum: app/Services/OdemeServisi.php:64 (ConnectionException)
```

`--ilk` her bölüme ayrı uygulanır; `--siralama` bölümlerin içindeki sırayı belirler.
Markdown çıktısında her bölüm ayrı bir tablodur ve artanların "Sonra" sütununda kat da yazar:
`5 (15,4 kat)`.

**Sıklık saat başına sayılır.** İki yarı farklı uzunlukta olabilir: 6 saatte 3 kez ile 1
saatte 3 kez aynı şey değildir. Bu yüzden kayıt sayıları değil, saatlik sıklıklar
karşılaştırılır:

```text
kat = (b / Ts) / (a / Tö)      a, b: öncedeki ve sonradaki kayıt sayısı
                               Tö, Ts: iki yarının süresi
```

Süreler saniye hassasiyetindedir; son kaydın saniyesi de sonra yarısına sayılır
(Ts = son − nokta + 1 sn).

**Tesadüf mü?** 16:11'de "Ödeme sağlayıcısı yanıt vermedi" kalıbı da 1 kayıttan 2'ye çıkar
ve saatlik sıklığı 6 kat artar. Ama toplam 3 kayıt tesadüfen de böyle dağılabilir. Bunu ayırt
etmek için iki Poisson sıklığını karşılaştıran koşullu binom testi kullanılır (Przyborowski ve
Wilenski, 1940):

1. Sıklık değişmediyse bir kalıbın n = a + b kaydının her biri, sonra yarısına o yarının
   süredeki payı kadar ihtimalle düşer: p₀ = Ts / (Tö + Ts).
2. Sonraya düşen kayıt sayısı Binom(n, p₀) dağılımına uyar. En az gözlenen b kadarının
   tesadüfen sonraya düşme ihtimali, yani p değeri, P(X ≥ b)'dir.
3. p değeri %1'in altındaysa "sıklık değişmedi" varsayımı bu gözlemi açıklayamaz: artış
   tesadüf değildir.

16:11'de p₀ = 8 408 / 34 270 ≈ 0,245:

| Kalıp | Önce → sonra | Kat | p değeri | Sonuç |
|---|---|---|---|---|
| cURL error (ödeme zaman aşımı) | 1 → 5 | 15,4 | 0,0042 | artan |
| Ödeme sağlayıcısı yanıt vermedi | 1 → 2 | 6,2 | 0,151 | tesadüf olabilir |

Bu test kesindir ama temkinlidir: yanlış alarm oranı %1'i geçmez, çoğu zaman altında kalır
([Krishnamoorthy ve Thomson, 2004](https://userweb.ucs.louisiana.edu/~kxk4695/JSPI-04.pdf)).
Python'da statsmodels'in
[`test_poisson_2indep`](https://www.statsmodels.org/dev/generated/statsmodels.stats.rates.test_poisson_2indep.html)
fonksiyonu `method="exact-cond"` ile aynı testi yapar ve aynı sonucu verir.

Üç şartın her biri bir hatayı önler:

- **En az 2 kat:** yalnızca p değerine bakılsa, çok kayıtta küçük farklar da "tesadüf değil"
  çıkar. Eşit yarılarda 1 000 → 1 200 kayıt tesadüf değildir (p ≈ 0,00001), ama %20'lik bir
  artışı trafikteki küçük bir değişim de yapar.
- **p değeri %1'in altında:** yalnızca kata bakılsa az kayıtlı kalıplardaki gürültü
  listelenir (yukarıdaki 1 → 2).
- **Sonra yarısında en az 2 kayıt:** tek kayıttan sıklık çıkarılamaz. Nokta logun sonuna çok
  yakınsa sonra yarısı birkaç saniye sürer ve oraya düşen tek bir kayıt yüzlerce kat artış
  gibi görünür. Son kayıt ise pencerenin sonunu kendisi belirlediği için sonraya her zaman
  düşer.

Yeni çıkan ve kaybolanlar için ek şart yoktur: ilk kez görülen bir hata tek kayıtla bile
önemlidir. Onların p değeri de hesaplanıp JSON'a yazılır (yeni için p₀ᵇ, kaybolan için
(1 − p₀)ᵃ), ama listeyi filtrelemez.

İstatistik için kütüphane kullanılmadı. Binom dağılımının kuyruğu terim terim toplanır; çok
küçük olasılıklar (yaklaşık 10⁻³⁰⁸'in altı) `f64`'te sıfıra yuvarlanacağı için terimlerin
logaritmaları tutulur. Sonuçlar [scipy](https://scipy.org)'nin `binom.sf` fonksiyonuyla
karşılaştırılarak test edilir.

**Zaman.** İki an arasındaki süre için tarih, 1970-01-01'den bu yana geçen güne çevrilir.
Bunun için Howard Hinnant'ın
[`days_from_civil`](https://howardhinnant.github.io/date_algorithms.html) algoritması
kullanılır: yıl Mart'ta başlıyormuş gibi sayılır, böylece Şubat yılın son ayı olur ve artık
gün yılın sonuna düşer. `--karsilastir` şu yazılışları kabul eder:

- `15:00` ya da `15:00:30`: tarih logun son kaydından alınır.
- `2026-10-24`: günün başı.
- `"2026-10-24 15:00"` ya da `2026-10-24T15:00`.

Takvimde olmayan bir an (`2026-02-29`, `24:00`) reddedilir. Nokta ilk kayıtta ya da
öncesindeyse, ya da son kayıttan sonraysa yarılardan biri boş kalacağı için veylog hata
verir (çıkış kodu 2).

**JSON.** Karşılaştırma `karsilastirma` alanındadır; alan yalnızca `--karsilastir` verilince
eklenir:

```bash
veylog --bicim json --karsilastir 16:11 ornekler/laravel.log | jq '.karsilastirma.artan.satirlar[] | {kalip, once, sonra, kat, p_degeri}'
```

```json
{
  "kalip": "cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> bytes received (see <URL>) for <URL>",
  "once": 1,
  "sonra": 5,
  "kat": 15.379400570884872,
  "p_degeri": 0.004243352322045974
}
```

- `nokta`, `once` ve `sonra`: bölme anı ile iki yarının başlangıcı, bitişi, süresi
  (`saniye`) ve kayıt sayısı. Zamanlar logdaki biçimden bağımsız olarak
  `2026-10-24 16:11:00` biçimindedir.
- `artis_kati`, `anlamlilik`, `en_az_kayit`: "artan" şartları.
- `yeni`, `kaybolan`, `artan`: her biri `kalip_sayisi`, `gosterilmeyen` ve `satirlar`
  içerir. Satırlar tablo satırının alanlarına ek olarak `once`, `sonra`, `kat` ve
  `p_degeri` taşır. Yenilerde kat sonsuzdur; JSON'da sonsuz olmadığı için `null` yazılır.

Benzer bir özellik Sumo Logic'in
[LogCompare](https://www.sumologic.com/help/docs/search/behavior-insights/logcompare/)'ıdır:
iki zaman aralığının kalıplarını karşılaştırır, yenileri "New", kaybolanları "Gone" diye
işaretler. Değişimi kalıbın toplam loglar içindeki payıyla ölçer; veylog ise saatlik sıklığı
kullanır, böylece "saatte 0,1 → 2,1" doğrudan okunur.

### Komut satırı

Argümanlar için yaygın bir kütüphane ([clap](https://crates.io/crates/clap)) yerine küçük
bir ayrıştırıcı yazıldı. clap projeye 17 kütüphane ekliyor, mesajları İngilizce ve Türkçedeki
gibi virgüllü ondalık sayıları (`0,8`) tanımıyor. Kendi ayrıştırıcımız şu kuralları izler:

- Seçenekler dosya adından önce ya da sonra gelebilir. Aynı seçenek iki kez yazılırsa
  sonuncusu geçerlidir (`--ilk 5 --hepsi` hepsini gösterir).
- Değer boşlukla (`--esik 0,8`), `=` ile (`--esik=0,8`) ya da kısa seçeneklerde bitişik
  (`-n20`) verilebilir.
- Tek başına `-` ve `--`'dan sonraki her şey dosya adıdır: `veylog -- -garip-ad.log`.
- Türkçe karakterli yazımlar da geçerlidir: `--eşik`, `--sıralama önem`, `--biçim`, `--yardım`.
- Argümanlar işletim sisteminin verdiği ham haliyle (`OsString`) okunur. Linux'ta bir dosya
  adı geçerli UTF-8 olmayabilir; program böyle bir adla da çökmez.
- Tanınmayan bir seçeneğe en çok benzeyen seçenek önerilir. Benzerlik
  [Levenshtein uzaklığıyla](https://en.wikipedia.org/wiki/Levenshtein_distance) ölçülür:
  birini ötekine çevirmek için gereken en az harf ekleme, silme ya da değiştirme sayısı
  (`--esk` → `--esik`: 1). Yanlış öneri vermemek için 5 harften kısa adlarda en fazla 1,
  uzunlarda en fazla 2 harf farka izin verilir.

### JSON ve Markdown çıktısı

Özet bir kez hesaplanır, sonra istenen biçimde yazılır; bu yüzden üç biçimdeki sayılar hep
aynıdır. `--ilk` ve `--siralama` her biçimde geçerlidir.

**JSON** (`--bicim json`) başka programların okuması içindir. Örnek logun yalnızca ilk
satırıyla (`--ilk 1`):

```json
{
  "surum": "0.1.0",
  "dosya": "ornekler/laravel.log",
  "satir_sayisi": 203,
  "kayit_sayisi": 59,
  "devam_satiri_sayisi": 141,
  "yetim_satir_sayisi": 3,
  "zaman_araligi": {
    "en_eski": "2026-10-24 08:59:58",
    "en_yeni": "2026-10-24 18:31:07"
  },
  "seviyeler": {
    "CRITICAL": 2,
    "ERROR": 16,
    "WARNING": 6,
    "NOTICE": 1,
    "INFO": 31,
    "DEBUG": 3
  },
  "kaliplar": {
    "ham_mesaj_sayisi": 45,
    "maskeli_mesaj_sayisi": 27,
    "kalip_sayisi": 23,
    "grup_sayisi": 24,
    "esik": 0.7,
    "siralama": "onem",
    "gosterilmeyen": 23,
    "satirlar": [
      {
        "adet": 2,
        "seviye": "CRITICAL",
        "ilk": "2026-10-24 17:05:13",
        "son": "2026-10-24 17:06:45",
        "kalip": "Redis bağlantısı kurulamadı: Connection refused [tcp://<IP>:<SAYI>]",
        "ornek": "Redis bağlantısı kurulamadı: Connection refused [tcp://10.0.3.12:6379]",
        "istisna": {
          "sinif": "RedisException",
          "dosya": "app/Http/Middleware/HizSiniri.php",
          "satir": 22
        }
      }
    ]
  }
}
```

- Hiçbir şey kısaltılmaz: kalıplar, örnekler ve zamanlar tam haliyle yazılır. Terminalde 80
  karakterde kesilen SQL kalıbı JSON'da 257 karakterin tamamıdır. Sınıf adı da tam
  haliyledir: `Illuminate\Database\UniqueConstraintViolationException`.
- `istisna`, kayıtlar bir istisnadan gelmiyorsa `null`'dır. İstisnanın uygulama kodunda yeri
  bulunamadıysa `dosya` ve `satir` `null`'dır.
- `seviyeler` yalnızca logda geçen seviyeleri, en önemlisi başta olacak şekilde içerir.
- `satirlar` en fazla `--ilk` kadar satırdır; `gosterilmeyen` dışarıda kalanların sayısıdır.
  Bütün satırlar için `--hepsi`.

[jq](https://jqlang.org) ile birkaç kullanım:

```bash
# Tablonun en önemli satırı
veylog --bicim json --hepsi laravel.log | jq '.kaliplar.satirlar[0]'

# Hatalar ve uygulama kodundaki yerleri, sekmeyle ayrılmış
veylog --bicim json --hepsi laravel.log | jq -r '.kaliplar.satirlar[]
  | select(.seviye == "CRITICAL" or .seviye == "ERROR")
  | "\(.adet)\t\(.istisna.dosya // "-")\t\(.kalip)"'

# CI adımı: logda CRITICAL varsa başarısız ol (jq -e, sonuç false ise 1 koduyla çıkar)
veylog --bicim json laravel.log | jq -e '.seviyeler.CRITICAL // 0 | . == 0'
```

JSON'u yazmak için kütüphane kullanılmadı. Rust'ta alışılmış yol
[serde](https://serde.rs)'nin `#[derive(Serialize)]` özelliğidir, ama programın
bağımlılıklarını 5 paketten 15'e çıkarırdı; yalnızca serde_json bile 9'a. veylog JSON'u
yalnızca yazıyor, okumuyor ve kurallar kısa ([RFC 8259](https://www.rfc-editor.org/rfc/rfc8259)):

- JSON değeri bir `enum` ile modellenir (`Yok`, `Sayi`, `Ondalik`, `Metin`, `Dizi`, `Nesne`)
  ve özyinelemeli olarak yazılır. Nesnelerin alanları eklendikleri sırayla yazılır.
- Metinlerde `"` ve `\` kaçılır; kontrol karakterleri `\n`, `\t` gibi kısa biçimleriyle ya da
  `\u001b` olarak yazılır. RFC'nin zorunlu tuttuğu U+0000–U+001F'ye ek olarak DEL ve C1
  karakterleri de kaçılır; böylece JSON doğrudan terminale yazıldığında da güvenlidir. Türkçe
  harfler olduğu gibi kalır.
- NaN ve sonsuz JSON'da olmadığı için `null` yazılır.

Testler çıktıyı [serde_json](https://crates.io/crates/serde_json) ile ayrıştırarak denetler:
tırnak, ters bölü, ESC, emoji gibi zor metinler yazılıp geri okunduğunda aynısı çıkmalı.

**Markdown** (`--bicim markdown`) GitHub issue ve PR açıklamalarına yapıştırmak içindir:

```text
$ veylog --bicim markdown --ilk 2 ornekler/laravel.log
## veylog: `ornekler/laravel.log`

- **Satır:** 203 · **Kayıt:** 59 · **Devam satırı:** 141 · **Yetim satır:** 3
- **Zaman aralığı:** 2026-10-24 08:59:58 → 2026-10-24 18:31:07
- **Seviyeler:** CRITICAL 2 · ERROR 16 · WARNING 6 · NOTICE 1 · INFO 31 · DEBUG 3
- **Kalıplar:** 45 farklı mesaj → maskeleyince 27 → Drain ile 23 kalıp (eşik 0,7) → parmak iziyle 24 grup

| Adet | Seviye | İlk | Son | Kalıp | Örnek | Konum |
|---:|---|---|---|---|---|---|
| 2 | CRITICAL | 17:05:13 | 17:06:45 | `Redis bağlantısı kurulamadı: Connection refused [tcp://<IP>:<SAYI>]` | `Redis bağlantısı kurulamadı: Connection refused [tcp://10.0.3.12:6379]` | `app/Http/Middleware/HizSiniri.php:22` RedisException |
| 6 | ERROR | 16:10:02 | 16:16:02 | `cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> by…` | `cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes receiv…` | `app/Services/OdemeServisi.php:64` ConnectionException |

… 22 satır daha; tümünü görmek için `--hepsi`
```

Markdown'da logdan gelen metin için üç tuzak var; üçü de GitHub'ın ayrıştırıcısıyla
([cmark-gfm](https://github.com/github/cmark-gfm)) denendi:

- `<SAYI>`, `<IP>` gibi yer tutucular düz metin olsa HTML etiketi sanılıp silinir. Bu yüzden
  logdan gelen her metin satır içi kod olarak yazılır; kodun içinde `<`, `*` ve `_` olduğu
  gibi görünür.
- SQL mesajlarındaki ters tırnaklar (`` `siparisler` ``) tek ters tırnaklı kodu ortadan
  böler. Kod, açan çerçeveyle aynı uzunluktaki ilk ters tırnak dizisinde kapandığı için
  çerçeve, metindeki en uzun ters tırnak dizisinden bir uzun seçilir ve iki yanına boşluk
  konur: ``` `` select * from `siparisler` `` ```.
- `|` tabloda sütun ayırıcıdır, kodun içinde bile; hücrelerde `\|` yazılır. Bu kaçış yalnızca
  tabloda geçerlidir: tablo dışında `\|` ters bölüyle birlikte görünür. Bu yüzden başlıktaki
  dosya adında yapılmaz.

Ayrıca CR, Markdown'da da satır sonudur ve tablonun satırını ikiye böler; kontrol
karakterleri bu yüzden önce sembollere çevrilir (aşağıda).

### Terminal güvenliği

Log mesajları güvenilmez metindir: bir form alanı, bir URL ya da bir HTTP başlığı olduğu
gibi loga düşebilir. Laravel mesajı dosyaya ham yazar; bağlam dizisi ise JSON olduğu için
kaçışlıdır. Laravel 13.38'de:

```php
$ad = "Ali\e[1A\e[2K";   // kullanıcının forma yazdığı ad
Log::info("Kullanıcı kaydoldu: $ad", ['ad' => $ad]);
```

```text
[2026-10-26 08:31:30] local.INFO: Kullanıcı kaydoldu: Ali␛[1A␛[2K {"ad":"Ali\u001b[1A\u001b[2K"}
```

(Burada ␛, dosyadaki ham ESC baytını gösteriyor.)

Terminal, ESC ile başlayan dizileri yazı olarak değil komut olarak okur: `\e[1A` imleci bir
satır yukarı alır, `\e[2K` o satırı siler. CR (`\r`) imleci satır başına götürür, ardından
gelen yazı öncekinin üstüne yazılır. Böyle bir mesaj olduğu gibi terminale yazılırsa
kendinden önceki satırları silebilir; bir log özetinde bu, en önemli hatanın ekrandan
kaybolması demektir. Şu üç satırlık log (␛ ve ␍ yine ham baytları gösteriyor), veylog'un
önceki sürümünde CRITICAL satırını ve tablo başlığını ekrandan siliyordu:

```text
[2026-10-24 10:00:00] local.CRITICAL: Veritabani coktu
[2026-10-24 10:00:01] local.INFO: Giris ␛[1A␛[2K␛[1A␛[2Kyapildi
[2026-10-24 10:00:02] local.INFO: Odeme tamam␍HATA: kart reddedildi
```

Metin ve Markdown çıktısı bu yüzden logdan ve dosya adından gelen her metni temizler:

```text
     1  CRITICAL   10:00:00  10:00:00  Veritabani coktu
     1  INFO       10:00:01  10:00:01  Giris ␛[1A␛[2K␛[1A␛[2Kyapildi
     1  INFO       10:00:02  10:00:02  Odeme tamam HATA: kart reddedildi
                                       örnek: Odeme tamam␍HATA: kart reddedildi
```

- U+0000–U+001F arasındaki kontrol karakterleri, Unicode'un "Control Pictures" bloğunda aynı
  sırayla dizilmiş sembollerine çevrilir (U+2400 + kod): ESC → ␛, CR → ␍, sekme → ␉. Bir
  karakter yine tek karakter kalır; tablonun hizası bozulmaz.
- DEL (U+007F) → ␡. C1 karakterlerinin (U+0080–U+009F) sembolü yoktur, `\u{9b}` diye
  yazılır; VTE tabanlı terminaller, kitty ve WezTerm bunları da komut olarak okur.
- Kullanıcının yazdığı argümanları geri yazan hata mesajları da aynı temizlikten geçer.
- JSON'da veri kaybı yoktur: ham mesaj kaçışlarıyla eksiksiz yazılır (`\u001b`), okuyan
  program aslını alır.

Bu bilinen bir açık sınıfıdır ([CWE-150](https://cwe.mitre.org/data/definitions/150.html)).
Terminal emülatörlerinde bu yolla bulunan açıklar için
[ANSI Terminal security in 2023 and finding 10 CVEs](https://dgl.cx/2023/09/ansi-terminal-security),
bir log kütüphanesindeki karşılığı için Rust'ın tracing-subscriber kütüphanesinde düzeltilen
[CVE-2025-58160](https://advisories.gitlab.com/cargo/tracing-subscriber/CVE-2025-58160/)
incelenebilir.

Laravel tarafı için bir ipucu: kullanıcıdan gelen değeri mesaja gömmek yerine bağlama koyun
(`Log::info('Kullanıcı kaydoldu', ['ad' => $ad])`); bağlam JSON olarak kaçışlı yazılır.

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
  `--siralama yeni` yanlış sıralanabilir. `--karsilastir` iki biçimi de doğru okur ama saat
  dilimini yok sayar.
- Karşılaştırma sıklığı saat başına ölçer. Trafik iki katına çıkarsa her kalıp iki kat sık
  görünür; veylog bunu yük artışından ayırt edemez.
- Her kalıp ayrı ayrı sınanır. %1 eşiği ve 2 kat şartı yanlış alarmı az tutar ama
  sıfırlamaz; çok kalıplı bir logda arada bir tesadüfi bir "artan" görülebilir.
- Test, kayıtların zamana rastgele serpildiğini varsayar. Zamanlanmış görevler gibi düzenli
  olaylar ve tek bir anda yığılan patlamalar bu varsayıma uymaz.
- Log tek bir noktada ikiye bölünür; iki ayrı dosya ya da iki keyfi aralık
  karşılaştırılamaz.
- ANSI renk kodları (`␛[31m`) rakam içerdiği için Drain onları değişken sayar; renk kodlu
  kısa mesajlar aynı uzunluktaki başka mesajlarla birleşebilir.
- Yazı yönünü değiştiren Unicode karakterleri (ör. U+202E, "Trojan Source") temizlenmez.
  Kontrol karakteri değiller ve terminali yönetemezler, ama destekleyen terminallerde ve
  GitHub'da yazının görünüşünü karıştırabilirler.
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
| `-b`, `--bicim <ad>` | Çıktının biçimi: `metin`, `json` ya da `markdown` ([ayrıntılar](#json-ve-markdown-çıktısı)). | `metin` |
| `-k`, `--karsilastir <zaman>` | Logu bu anda ikiye böl ve iki yarıyı karşılaştır: yeni çıkan, kaybolan ve artan kalıplar. Zaman: `15:00`, `15:00:30`, `2026-10-24` ya da `"2026-10-24 15:00"` ([ayrıntılar](#zaman-aralıklarını-karşılaştırma)). | |
| `-h`, `--yardim` | Yardımı göster (`--help` da olur). | |
| `-V`, `--surum` | Sürümü göster (`--version` da olur). | |

```bash
# Varsayılan ayarlarla
veylog storage/logs/laravel.log

# Daha sıkı eşik, ilk 20 satır
veylog --esik 0,8 --ilk 20 storage/logs/laravel.log

# Son ortaya çıkan sorunlar önce, tablonun tamamı
veylog --siralama yeni --hepsi storage/logs/laravel.log

# JSON: bütün satırlar, jq ile en önemlisi
veylog --bicim json --hepsi storage/logs/laravel.log | jq '.kaliplar.satirlar[0]'

# Markdown: GitHub issue'suna yapıştırmak için
veylog --bicim markdown storage/logs/laravel.log > rapor.md

# 15:00'teki dağıtımdan önce ve sonra: yeni çıkan, kaybolan ve artan hatalar
veylog --karsilastir 15:00 storage/logs/laravel.log

# Derlemeden çalıştırırken: cargo'nun seçenekleriyle karışmasın diye önce --
cargo run -- --ilk 5 ornekler/laravel.log
```

Çıkış kodları: `0` başarılı, `1` dosya okunamadı ya da çıktı yazılamadı, `2` kullanım hatası
(bilinmeyen seçenek, geçersiz değer, eksik dosya adı, logun dışında kalan bir karşılaştırma
noktası). Böylece veylog'u çağıran bir betik hatanın türünü ayırt edebilir.

```text
$ veylog --esk 0,8 laravel.log
veylog: bilinmeyen seçenek: --esk (bunu mu demek istediniz: --esik)
Kullanım: veylog [SEÇENEKLER] <log-dosyası>
Ayrıntılar için: veylog --yardim
```

## Proje yapısı

```text
src/
├── main.rs          programın girişi: dosyayı okur, özeti istenen biçimde yazar, çıkış kodlarını belirler
├── komut_satiri.rs  komut satırı: seçenekler, yardım metni, hata mesajları
├── kayit.rs         log metnini kayıtlara ayırır: başlık, seviye, mesaj, bağlam, devam satırları
├── maske.rs         mesajlardaki değişkenleri (sayı, IP, e-posta...) yer tutucularla değiştirir
├── drain.rs         Drain algoritması: maskelenmiş mesajlardan kalıp çıkarır
├── parmak_izi.rs    hata parmak izi: istisna sınıfı ve uygulama kodundaki ilk satır
├── rapor.rs         özet: sayılar, seviyeler, kalıp tablosu, sıralamalar; metin çıktısı ve
│                    kontrol karakterlerinin temizlenmesi
├── karsilastirma.rs iki zaman aralığının karşılaştırılması: yeni çıkan, kaybolan, artan kalıplar
├── zaman.rs         tarih ve saat: --karsilastir değerini ayrıştırır, iki an arasındaki süreyi hesaplar
├── istatistik.rs    binom dağılımının kuyruğu: bir artışın tesadüf olup olmadığı
├── json.rs          JSON çıktısı: kütüphanesiz JSON yazıcı
└── markdown.rs      Markdown çıktısı: GitHub'a yapıştırılabilir tablo
tests/
└── cli.rs           programı gerçekten çalıştıran testler: argümanlar, biçimler, hata mesajları, çıkış kodları
```

Program yalnızca [regex](https://crates.io/crates/regex) kütüphanesini kullanır. Testler JSON
çıktısını doğrulamak için [serde_json](https://crates.io/crates/serde_json)'ı da kullanır; bu
bir geliştirme bağımlılığıdır (`[dev-dependencies]`), `cargo build` ve `cargo install` onu
derlemez.

## Testler

```bash
cargo test
```

İki tür test çalışır: `src` içindeki birim testleri her parçayı ayrı ayrı sınar;
`tests/cli.rs` ise derlenmiş programı çalıştırıp kullanıcının gördüğünü kontrol eder. JSON
çıktısı serde_json ile ayrıştırılır, Markdown tablosunun satırları sayılır ve kontrol
karakterleriyle dolu bir logda, üç biçimin hiçbirine tek bir kontrol karakterinin
sızmadığı denetlenir. İstatistik ve tarih hesapları scipy ve Python'un `datetime` modülüyle
hesaplanmış değerlerle sınanır; karşılaştırma bölümünün metin ve Markdown çıktısı beklenen
metinle karakter karakter karşılaştırılır.

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
- [x] JSON ve Markdown çıktısı
- [x] Terminal güvenliği: logdaki kontrol karakterleri terminale komut olarak ulaşmaz
- [x] İki zaman aralığını karşılaştırma: yeni çıkan, kaybolan ve artan hatalar
- [ ] Ani hata artışlarını yakalama (medyan + MAD)