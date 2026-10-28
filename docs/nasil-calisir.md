# Nasıl çalışır

Bu belge veylog'un bir log dosyasını adım adım nasıl özetlediğini, her adımın gerekçesini
ve gerçek loglarda nasıl sınandığını anlatır. Örnekler depodaki kurgusal loglardandır
(`ornekler/`). Karşılaştırma ve ani artışların istatistiği ayrı bir belgede:
[karşılaştırma ve ani artışlar](karsilastirma-ve-ani-artis.md).

```text
laravel.log
  → kayıtlara ayır    59 kayıt, 45 farklı mesaj
  → saat dilimi       zamanlar tek bir dilime çevrilir (--saat-dilimi)
  → filtre            isteğe bağlı (--seviye, --baslangic, --bitis, --son)
  → maskele           27 farklı mesaj
  → Drain             23 kalıp
  → parmak izi        24 grup
  → karşılaştırma     isteğe bağlı (--karsilastir): yeni çıkan, kaybolan, artan
  → ani artışlar      isteğe bağlı (--ani-artis): olağan seviyesinden sıçrayan anlar
  → özet              metin, JSON ya da Markdown olarak yazılır
  → canlı izleme      isteğe bağlı (--izle): eklenen kayıtlar okunur, özet yeniden çizilir
```

## Kayıtlara ayırma

Bir Laravel kaydı bir başlık satırıyla başlar:

```text
[2026-10-24 14:03:11] production.ERROR: Mesaj {"bağlam":"..."}
[2026-10-24T14:03:11.123456+03:00] production.ERROR: Mesaj
```

Başlıktan sonra gelen satırlar (stack trace, çok satırlı mesajlar) bir sonraki başlığa kadar o
kayda aittir. Dosya bir kaydın ortasından başlıyorsa (log döndürüldüğünde olur), hiçbir kayda
ait olmayan ilk satırlar "yetim satır" olarak ayrıca sayılır. Her satır tam olarak bir yere
gider: başlık, devam ya da yetim.

- Zaman Laravel'in biçiminde (`2026-10-24 14:03:11`) ya da Monolog'un varsayılanı ISO 8601
  biçiminde olabilir; saniyeden sonra kesir ve saat dilimi (`Z`, `+03:00`, `+0300`) gelebilir
  ([saat dilimleri](#saat-dilimleri)).
- Biçimi doğru ama takvimde olmayan bir zaman (`2026-02-30`, `24:00:00`) ya da saat dilimi
  (`+25:00`) başlık sayılmaz.
  Laravel böyle bir zaman yazmaz; ama mesajlar satır sonu içerebildiği için loglanan bir
  kullanıcı girdisindeki bir satır başlık gibi görünebilir. Kabul edilseydi süre hesapları
  bozulurdu: 30 Şubat sıralamada 1 Mart'tan önce gelir ama saniyesi 2 Mart'ınkidir.
- Satırın başındaki UTF-8 BOM (bazı Windows araçları yazar) atılır; atılmasa satır `[` ile
  başlamaz ve kayıt kaybolurdu. Her satıra bakılır: birleştirilmiş dosyalarda
  (`cat a.log b.log | veylog -`) her dosyanın BOM'u kendi ilk satırındadır.
- Geçersiz UTF-8 baytları dosyanın tamamını reddetmek yerine `�` ile değiştirilir ve bir
  uyarı yazılır.
- Dosya adı yerine `-` verilirse log standart girdiden okunur:
  `zcat laravel-2026-10-24.log.gz | veylog -`.
- Uygulama Monolog'a başka bir satır ya da tarih biçimi verdiyse biçim `--desen` ve
  `--zaman-bicimi` ile tanıtılır ([log biçimleri](#log-biçimleri)). Satırlardan hiçbiri bir
  kayıt başlatmıyorsa bir uyarı yazılır.
- Mesaj PHP'nin istisna metniyse (`Log::error($e)`) kaydın mesajı istisnanın mesajıdır
  ([PHP istisna metni](#php-istisna-metni)).

## JSON satırları

Laravel'de bir kanalın biçimlendiricisi Monolog'un `JsonFormatter`'ı yapılırsa her kayıt tek
satırlık bir JSON nesnesi olur. Logları toplayan servisler (Loki, Elasticsearch, CloudWatch)
bu biçimi kolay okur:

```php
// config/logging.php
'single' => [
    'driver' => 'single',
    'path' => storage_path('logs/laravel.log'),
    'formatter' => Monolog\Formatter\JsonFormatter::class,
    // Yığın izi de yazılsın: parmak izinin konumu için gerekir
    'formatter_with' => ['includeStacktraces' => true],
],
```

```text
{"message":"Kullanıcı 57 giriş yaptı","context":{"ip":"203.0.113.99"},"level":200,"level_name":"INFO","channel":"local","datetime":"2026-10-25T10:02:05.635543+00:00","extra":{}}
```

veylog bir satırı şu şartlarla kayıt sayar: `{` ile başlar, baştan sona geçerli bir JSON
nesnesidir ve `datetime` (geçerli bir zaman), `level_name` (bilinen bir seviye) ve `message`
alanları vardır. Biri eksikse satır, metin biçimindeki gibi bir önceki kaydın devam satırıdır.
İki biçim aynı dosyada karışık olabilir: kanalın biçimlendiricisi değiştirilince eski kayıtlar
metin, yenileri JSON olarak kalır.

- Mesajın kaçışları çözülür (`\"` → `"`, `\u00e7` → `ç`) ve metin biçimindeki gibi yalnızca
  ilk satırı alınır. Böylece aynı hata iki biçimde de aynı kalıba düşer.
- İstisna bağlamda bir nesnedir: sınıfı `class`, fırlatıldığı yer `file` (`/yol/Dosya.php:26`),
  çağrıların yerleri `trace` alanındadır ve parmak izi metin biçimindeki kuralla bulunur.
  `trace` yalnızca `includeStacktraces` açıksa yazılır; kapalıysa yalnızca fırlatıldığı yer
  bilinir. Çağrılan fonksiyonlar hiç yazılmaz; satırın fonksiyonu logun başka kayıtlarından
  öğrenilir ([satır değil fonksiyon](#satır-değil-fonksiyon)).
- Kanal (`channel`) ve `extra` kullanılmaz.

`ornekler/karisik.log` bu durumu anlatır: Windows'ta Herd ile çalışan uydurma bir mağazanın
kanalı 10:00'da JSON'a geçirilmiş.

```text
$ veylog --ilk 4 ornekler/karisik.log
ornekler/karisik.log: 683 satır
  Kayıt         : 21
...
     2  ERROR      09:05:40  09:06:02  SQLSTATE[HY000] [<SAYI>] Connection refused (Connection: mysql_yok, Host: <IP>,…
                                       örnek: SQLSTATE[HY000] [2002] Connection refused (Connection: mysql_yok, Host: …
                                       konum: database/migrations/2026_10_25_090000_create_kuponlar_table.php:11, up() (QueryException)
     2  ERROR      09:20:11  10:15:30  Undefined array key "kargo_ucreti"
                                       konum: app/Services/SepetHesaplayici.php:10, kargoUcreti() (ErrorException)
  … 9 satır daha; tümünü görmek için --hepsi
```

`kargo_ucreti` hatasının 09:20'deki kaydı metin, 10:15'teki kaydı JSON biçimindedir; ikisi aynı
satırda birleşir. JSON kaydının yığın izinde çağrılar olmadığı için `kargoUcreti()` metin
kaydının yığın izinden öğrenilir. Migration hatasının konumu ise Windows yolundan bulunmuştur
([aşağıda](#windows-yolları)).

**Okuyucu.** JSON'u yazarken olduğu gibi okurken de kütüphane kullanılmadı. Okuyucu
[RFC 8259](https://www.rfc-editor.org/rfc/rfc8259)'u izler ve özyinelemeli inişle çalışır: her
değer türü için bir fonksiyon vardır; bir nesne okunurken içindeki değerler için aynı
fonksiyonlar çağrılır.

- Okunan değerler satırdan ödünç alınır. Metinler tırnakların arasındaki ham halleriyle
  tutulur, kaçışları yalnızca gerektiğinde çözülür; kaçış yoksa kopya da yoktur (`Cow`).
- `\u` kaçışları UTF-16 birimidir: U+FFFF'in üstündeki karakterler (emoji gibi) iki birimle, bir
  vekil (surrogate) çiftiyle yazılır: `\ud83d\ude80` → 🚀.
- İç içe en fazla 127 dizi ya da nesne okunur. Sınır olmasa `[[[[...` diye on binlerce düzey
  açan tek bir log satırı yığını taşırıp programı çökertebilirdi.

Doğruluğu [serde_json](https://crates.io/crates/serde_json) ile karşılaştırılarak denetlenir:
2 000 rastgele değer (sıkışık ve girintili yazılışlarıyla), elle seçilmiş geçerli ve geçersiz
metinler ve Laravel'in yazdığı loglardaki 17 944 satır iki okuyucuda aynı sonucu verir. İki
bilinen fark var, ikisi de RFC'ye göre geçerli metinlerde: `f64`'e sığmayan bir sayıyı
(`1E400`) serde_json çeviremediği için reddeder, veylog sayıları çevirmediği için kabul eder.
Eşi olmayan bir vekil kaçışını (`"\ud800"`) serde_json reddeder, veylog `�` olarak okur.
Monolog ikisini de yazmaz.

## Log biçimleri

Laravel'in satır biçimi ve Monolog'un JSON satırları kendiliğinden tanınır. Uygulama
Monolog'a başka bir satır biçimi verdiyse veylog'a da aynısı verilir:

```php
// config/logging.php
'single' => [
    'driver' => 'single',
    'path' => storage_path('logs/laravel.log'),
    'formatter' => Monolog\Formatter\LineFormatter::class,
    'formatter_with' => [
        'format' => "%datetime% [%level_name%] %channel%: %message% %context% %extra%\n",
        'dateFormat' => 'd.m.Y H:i:s',
        'allowInlineLineBreaks' => true,
        'includeStacktraces' => true,
    ],
],
```

```bash
veylog --desen '%datetime% [%level_name%] %channel%: %message% %context% %extra%' \
       --zaman-bicimi 'd.m.Y H:i:s' storage/logs/laravel.log
```

**Monolog'un biçim dizgesi** düzenli ifadeye çevrilir. Harfler olduğu gibi aranır (düzenli
ifadede özel olan `[`, `.`, `(` kaçışlanır); `%datetime%` zamanı, `%level_name%` (ya da sayı
olarak `%level%`) seviyeyi, `%channel%` kanalı yakalar; mesajdan önceki öteki yer tutucular
(`%extra.istek_no%`) herhangi bir metne uyar. `%message%`'dan sonrası mesaja katılır: Monolog
bağlamı ve ek alanları mesajın arkasına yazar, bağlam Laravel'in biçimindeki gibi ` {"` ile
ayrılır. Yukarıdaki dizge şu ifadeye dönüşür:

```text
^(?:(?P<zaman>[0-9]{2}\.[0-9]{2}\.[0-9]{4} [0-9]{2}:[0-9]{2}:[0-9]{2}) \[(?P<seviye>[A-Za-z]+)\] (?P<kanal>.*?): (?P<mesaj>.*))
```

**Düzenli ifade** doğrudan da verilebilir; Monolog'la yazılmamış loglar böyle okunur. `zaman`
ve `mesaj` grupları gerekir; `seviye`, `baglam` ve `kanal` isteğe bağlıdır (İngilizce adları da
olur: `time`, `message`, `level`, `context`, `channel`):

```bash
veylog --desen '(?P<zaman>\S+ \S+) (?P<seviye>\w+) +(?P<mesaj>.*)' app.log
```

- İfade satırın başına bağlanır. Uymayan satırlar bir önceki kaydın devamıdır (stack trace, çok
  satırlı mesajlar), Laravel'in biçimindeki gibi. JSON satırları her biçimde tanınır.
- Seviye büyük küçük harf ayrımı olmadan okunur; `WARN`, `ERR`, `CRIT`, `EMERG`, `FATAL`
  (CRITICAL sayılır) ve `TRACE` (DEBUG) kısaltmaları ve Monolog'un sayıları (100 DEBUG …
  600 EMERGENCY) da olur. Bilinmeyen bir seviye yakalayan satır kayıt sayılmaz. Seviye grubu
  yoksa her kayıt INFO'dur.
- Bağlam grubu yoksa bağlam mesajın sonundan ` {"` ile ayrılır.
- Derlenemeyen bir ifade, eksik ya da bilinmeyen bir grup adı (`(?P<mesage>...)`) kullanım
  hatasıdır.

**Zaman biçimi** PHP'nin `date()` biçim dizgesidir, yani Monolog'un `dateFormat`'ı. Dizge bir
kez derlenir: her harf bir parçaya dönüşür (`d` iki basamaklı gün, `j` bir ya da iki basamaklı
gün, `M` ayın kısa adı, `P` saat dilimi...). Harf olmayan karakterler ve PHP'nin tanımadığı
harfler olduğu gibi aranır, `\` kendinden sonrakini harf olmaktan çıkarır: PHP de böyle yazar.
Kaydın zamanı bu parçalarla soldan sağa okunur ve zamanın tamamı okunmalıdır. Sonuç veylog'un
iç biçimidir (`2026-10-27 14:03:11+03:00`): kayıt bundan sonra Laravel'in biçiminde yazılmış
gibi işlenir. Okunan tarih takvimde var olmalıdır.

| Harfler | Okunan |
|---|---|
| `Y`, `y` | Dört ya da iki basamaklı yıl (`y`: 00–69 2000'li, 70–99 1900'lü yıllar) |
| `m`, `n`, `M`, `F` | Ay: iki ya da bir-iki basamaklı, kısa (`Oct`) ya da uzun (`October`) adıyla |
| `d`, `j` | Gün |
| `H`, `G`; `h`, `g` ve `A`, `a` | Saat: 24 saatlik ya da 12 saatlik (`AM`/`PM` ile) |
| `i`, `s` | Dakika ve saniye; saniye yoksa 0 |
| `u`, `v` | Saniyenin kesri: mikrosaniye, milisaniye |
| `P`, `p`, `O`, `Z`, `T`, `e` | Saat dilimi: `+03:00`, `Z`, `+0300`, saniye (`10800`), kısaltma, ad |
| `U` | Unix zamanı (UTC) |
| `c`, `r` | ISO 8601 ve RFC 2822 biçimlerinin tamamı |
| `D`, `l`, `S`, `N`, `w`, `z`, `W`, `t`, `L`, `o`, `B`, `I` | Okunur, kullanılmaz |

Yıl, ay, gün, saat ve dakikadan biri eksikse (ya da `U` yoksa) ve 12 saatlik saatin yanında
`A` yoksa dizge kullanım hatasıdır. PHP ay ve gün adlarını her zaman İngilizce yazar; adlar
İngilizce okunur. Saat dilimi kısaltmalarından yalnızca `UTC`, `GMT` ve sayıyla yazılanlar
(`+03`) çevrilir: `CET`, `EST` gibi kısaltmalar birden çok dilime karşılık gelebilir. Dilimin
adı (`Europe/Istanbul`) yaz saatini bilmeden farka çevrilemez. İkisi de dilimi yazılmamış
sayılır ([saat dilimleri](#saat-dilimleri)).

Laravel'in satır biçimindeki ama tarih biçimi değiştirilmiş bir log için `--zaman-bicimi` yeter:
`[27.10.2026 14:03:11] production.ERROR: ...`. Zaman biçimi verilince zamanlar yalnızca o
biçimde okunur. JSON satırlarının zamanı ise hep ISO 8601'dir.

Bir desenle okumak daha yavaştır: her satır düzenli ifadeyle sınanır. 500 MB'lık sentetik log
Laravel'in biçimiyle 7,7, aynı biçimi tarif eden bir desenle 10,3 saniyede okunur. Başlığın
parçalarının satırdaki yerleri saklanır; kayıt tamamlanınca başlık bunlardan kurulur, ifade
satıra ikinci kez uygulanmaz.

**Doğrulama.** Laravel 13 ile kurulan bir uygulamada aynı istisnalar üç kanala yazdırıldı:
Laravel'in biçimi, `JsonFormatter` ve yukarıdaki özel biçim. Laravel'in biçimiyle özel biçimin
raporu baştan sona aynıdır; JSON'un raporunda yalnızca devam satırı sayısı (JSON kaydı tek
satırdır) ve saat dilimi satırı farklıdır, kalıp tablosu aynıdır. 500 MB'lık sentetik log,
Laravel'in biçimini tarif eden bir desenle okununca da varsayılanla aynı raporu verir.

## Maskeleme

Değişken kısımlar düzenli ifadelerle şu sırayla yer tutuculara çevrilir: URL, dosya yolu,
e-posta, UUID, zaman, IP, sayı. Sıra önemlidir: geniş yapılar önce, tek tek sayılar en son
yakalanır. Yoksa `10.0.3.12` bir IP olarak değil, sayılara bölünmüş olarak maskelenirdi. URL
boşlukta, kapanan parantezde, tırnakta ve açılı ayraçta biter: `value "https://..." is
invalid` içindeki kapanış tırnağı maskeye katılmaz.

Dosya yolları `<YOL>` olur. Laravel bir Blade hatasının mesajının sonuna şablonun tam yolunu
ekler:

```text
Route [panel.kampanya.olustur] not defined. (View: C:\Users\İrem\Herd\magaza\resources\views\panel\kampanyalar.blade.php)
→ Route [panel.kampanya.olustur] not defined. (View: <YOL>)
```

Yol maskelenmese aynı hata her şablon için, hatta projeyi açan her bilgisayar için ayrı bir
kalıp olurdu; tablodaki kalıp da 80 karakterde, şablonun adına gelmeden kesilirdi. Hangi
şablon olduğu konum sütununda yazar ([aşağıda](#view-hataları)). Bu ekin yolu `.php)` ile
bittiği için boşluk içerse de (`C:\Users\Ali Veli\Herd\...`) tamamı maskelenir.

Öteki yollar: Windows'ta bir yol sürücü harfiyle başlar (`C:\`, `C:/`). Linux ve macOS'ta `/`
ile başlayan bir yol ancak bir dosya uzantısıyla bitiyorsa maskelenir
(`/var/www/magaza/app/X.php`): böylece `GET /api/urunler/15` gibi URL yolları kalıpta kalır.

## Drain

Kalıp çıkarma için He ve arkadaşlarının
[Drain](https://jiemingzhu.github.io/pub/pjhe_icws2017.pdf) algoritması (ICWS 2017) kullanılır.
Mesajlar teker teker işlenir:

1. Mesaj boşluklardan belirteçlere bölünür.
2. Sabit derinlikli bir ağaçta önce belirteç sayısına, sonra ilk belirtece göre dal seçilir.
   İçinde rakam olan belirteçler ve dolmuş bir düğüme gelen yeni belirteçler ortak `<*>`
   dalına gider. Böylece her mesaj bütün kalıplarla değil, yalnızca birkaç adayla
   karşılaştırılır.
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
| En fazla çocuk | 100 | Bir düğümün en fazla dal sayısı; dolunca yeni belirteçler `<*>` dalına gider. Bir yaprakta bundan fazla değer alan konum [değişkendir](#değişken-konumlar). |
| Esnek eşik | açık | Eşik, büyük ihtimalle değişken olan farklılıkların oranında gevşer (aşağıda). |

Eşiğin örnek logdaki etkisi:

| Eşik | Kalıp | Etkisi |
|---|---|---|
| 0,4 | 22 | İki farklı yavaş SQL sorgusu anlamsız tek bir kalıpta birleşir. |
| **0,7** | 23 | Sorgular ayrı kalır; zamanlanmış görevler birleşir. |
| 0,8 | 24 | `giriş` ve `çıkış` ayrılır; zamanlanmış görevler, adları [kodsal biçimde](#kategorik-konumlar) olduğu için birleşmeye devam eder. |

Yanlışlıkla birleştirilen yeni bir hata eski bir kalıbın içinde kaybolacağı için varsayılan
eşik temkinli seçildi.

### Esnek eşik

Sabit bir eşik kısa mesajlarda sorun çıkarır: `Yanıt süresi 120ms` ile `Yanıt süresi 95ms`
mesajlarında 3 belirtecin 2'si aynıdır (0,67 < 0,7) ve her değer ayrı bir kalıp olur. Eşiği
düşürmek ise uzun mesajlarda farklı SQL sorgularını birleştirir. Bunun yerine eşik, büyük
ihtimalle değişken olan farklılıkların oranında gevşer:

```text
eşik = 0,7 × (n − değişken fark) / n
```

Burada `değişken fark`, farklı olan (ya da kalıpta `<*>` olan) ve iki şarttan birini sağlayan
konumların sayısıdır: iki tarafında da rakam var (kalıpta `<*>` varsa mesajın tarafında) ya
da konum yaprakta [değişken](#değişken-konumlar). Rakam fikri Drain'in
[genişletilmiş sürümünden](https://arxiv.org/pdf/1806.04356) gelir: içinde rakam olan
belirteçler büyük ihtimalle değişkendir.

| Mesajlar | Değişken fark | Eşik | Benzerlik | Sonuç |
|---|---|---|---|---|
| `Yanıt süresi 120ms` / `95ms` | 1 | 0,47 | 0,67 | Birleşir: `Yanıt süresi <*>` |
| `Yanıt süresi 120ms` / `uzun` | 0 | 0,70 | 0,67 | Ayrı kalır: rakam yalnızca bir tarafta |
| `Ödeme alındı` / `Ödeme reddedildi` | 0 | 0,70 | 0,50 | Ayrı kalır |
| `S3 yüklemesi tamamlandı` / `başarısız` | 0 | 0,70 | 0,67 | Ayrı kalır |

Makaledeki formülden farkı, bütün rakamlı belirteçleri değil yalnızca iki tarafında da rakam
olan **farklılıkları** saymasıdır. `S3` gibi rakam içeren ama sabit olan bir kelime eşiği
gevşetmez; `Unauthenticated.` ile `kullanici42` gibi yalnızca birinde rakam olan iki mesaj da
birleşmez.

Kümelerin eşikleri böylece farklı olabilir. Mesaj, eşiği geçen kümelerin en benzerine katılır
(eşitlikte Drain'deki gibi daha çok `<*>` içeren, yani daha genel kalıba): en benzer küme
eşiği geçmese de bir başkası geçebilir. Esnek eşik kapalıyken eşik herkes için aynıdır ve bu
kural klasik Drain'in kuralıyla aynı sonucu verir.

Gevşemenin iki sınırı vardır:

- **Tutunacak yer.** Hiçbir belirteci tutmayan bir birleşme, ancak bütün farklılıkların iki
  tarafında da rakam varsa olur (`a1b2 c3d4` ile `e5f6 g7h8`). Tek ortak yanı bir değişken
  konum olan iki mesaj birleşmez.
- **Tek kelimelik mesajlar.** Bütün tek belirteçli mesajlar (`Unauthenticated.`,
  `odeme.basarisiz`) aynı yapraktadır; orada değişken konum aranmaz. Rakamsız olanlar yalnızca
  birebir aynıysa birleşir, rakamlı olanlar (kimlikler, özetler) tek bir `<*>` kümesinde
  toplanır. Böylece binlerce farklı olay adı olsa da her mesaj en fazla iki kümeyle
  karşılaştırılır.

Kalıptaki `<*>` içeride boşluk içeren bir işaretle tutulur; belirteçler boşluklardan
bölündüğü için mesajdaki hiçbir kelimeyle karışmaz. Mesajda yazılı bir `<*>` bu yüzden
sıradan bir kelimedir ve birebir aynı mesajlar birleşir. Karışsaydı her biri ayrı bir küme
açar, 20 bin satırlık bir log yaklaşık 10 saniye sürerdi.

### Değişken konumlar

Rakam içermeyen bir değer kısa mesajlarda birleşmez. Kaba kuvvetle giriş denenen bir
uygulamanın logu:

```text
[2026-10-24 03:12:07] production.WARNING: Bilinmeyen kullanıcı: ahmet
[2026-10-24 03:12:07] production.WARNING: Bilinmeyen kullanıcı: admin
[2026-10-24 03:12:08] production.WARNING: Bilinmeyen kullanıcı: root
```

Üç belirtecin ikisi aynıdır (0,67 < 0,7) ve rakam yoktur: her ad ayrı bir küme açar. 40 bin
denemelik bir log 40 bin satırlık bir tabloya dönüşür; üstelik her yeni mesaj yapraktaki
bütün kümelerle karşılaştırıldığı için süre mesaj sayısının karesiyle artar (bu kural
eklenmeden önce yaklaşık bir dakika).

Drain'in 2. adımdaki dolu düğüm kuralı (en fazla 100 dal) yaprakta da uygulanır: bir
yapraktaki kümelerin kalıplarında bir konum **100'den fazla farklı belirteç** aldıysa o konum
değişkendir ve oradaki farklılıklar rakamlı farklılıklar gibi eşiği gevşetir. Kalıp
`Bilinmeyen kullanıcı: <*>` olur ve yaprak büyümeyi bırakır.

Konumun değişken olduğu ancak 101. değer görülünce anlaşılır; o zamana kadar açılmış 100
küme ayrı kalırdı. Bu yüzden böyle bir konum bulunursa mesajlar bu bilgiyle bir kez daha,
baştan kümelenir. Değişken konum yoksa (logların çoğunda) tek tur yeter. Sonuç: 40 bin
deneme tek bir satırda, saniyenin dörtte birinde.

Sınırın 100 olması temkinlidir: `Ödeme alındı` ile `Ödeme reddedildi` gibi farklı olaylar
aynı konumda yüz farklı kelime almaz. Sayım da ancak bir yaprakta 100'den fazla küme olunca
başlar; yaprakların çoğunda hiç yapılmaz. Değerlerin kendisi değil 64 bitlik özetleri
saklanır: yalnızca kaç farklı değer görüldüğü önemlidir.

İlk belirteci rakamlı mesajların ortak `<*>` dalında birbiriyle ilgisiz mesajlar yan yana
durur. Orada bir mesaj ailesinden öğrenilen değişken konum, başka bir mesajı ancak öteki
konumları da tutuyorsa birleştirir: 101 farklı adla `Kod7 dosyası <*>` kalıbı oluşsa da
`Rapor dosyası hazır` ona katılmaz, çünkü `Rapor` ile `Kod7`'nin yalnızca birinde rakam var.

Geliştirme sırasında [Drain3](https://github.com/logpai/Drain3) (Python, 0.9.11) ile
karşılaştırıldı. Depodaki örnek loglarda, iki gerçek geliştirme logunda ve 20 bin ile 124 bin
mesaj arasındaki üç sentetik logda birebir aynı kümeleri ve kalıpları veriyor. Esnek eşik
açıkken de: eşiğin gevşemesi bu loglarda hiçbir birleşmeyi değiştirmiyor. Esnek eşik
Drain3'ten yalnızca eşiği gevşeten farklılıklarda (iki tarafında da rakam olan ya da 100'den
fazla değer alan bir konumda) ayrılır; 40 bin farklı kullanıcı adlı logda olduğu gibi.

Esnek eşik kapalıyken bilinen farklar, gerçek loglarda görülmeyen girdilerdedir: Drain3
rakamı Unicode'a göre tanır (`²`, `٣`), belirteçleri Python'un `split()`'iyle böler (`\x1c`
gibi ayırıcılar da boşluk sayılır) ve mesajda yazılı bir `<*>`'ı joker sayar.

### Kategorik konumlar

Yüz farklı değer sınırı sıradan kelimeler içindir. Oysa kısa mesajlardaki değişkenlerin
çoğu koddan gelir ve kelimeye benzemez:

```text
[2026-10-25 09:00:03] production.INFO: İş tamamlandı: App\Jobs\FaturaGonder
[2026-10-25 09:00:05] production.INFO: İş tamamlandı: App\Jobs\EpostaGonder
[2026-10-25 09:02:40] production.WARNING: Sipariş 1051 durumu ORDER_PENDING olarak kaldı
[2026-10-25 09:05:40] production.WARNING: Sipariş 1052 durumu ORDER_FAILED olarak kaldı
```

Bir uygulamada yüz farklı iş sınıfı olmaz; her sınıf ayrı bir kalıp olurdu. Bir belirtecin
**biçimi** ise ne olduğunu söyler. Belirtecin cümleden gelen kısmı (baştaki açan ayraç,
sondaki kapatan ayraç ve noktalama: `[App\Jobs\X]`, `ORDER_CREATED:`) soyulduktan sonra
biçimi şunlardan biridir:

| Biçim | Örnek | Ne? |
|---|---|---|
| sınıf | `App\Jobs\FaturaGonder`, `\App\Models\Urun`, JSON'daki `App\\Jobs\\X` | Ters bölüyle ayrılmış ad alanlı PHP sınıf adı |
| sabit | `ORDER_CREATED`, `HTTP_2` | Büyük harfli, en az bir alt çizgili |
| tanımlayıcı | `FaturaGonder`, `hizSiniri`, `fatura_gonder`, `$urun` | deveAdı, PascalAdı, yilan_adi, PHP değişkeni |
| noktalı | `odeme.basarisiz`, `kurlar:guncelle`, `ornek.test` | Noktayla ya da iki noktayla parçalanmış ad: olay, komut, yapılandırma anahtarı, sunucu adı |
| tırnaklı | `'fiyat'`, `"dsn"` | Tek ya da çift tırnakla sarılı tek kelime |
| sözcük | `Ödeme`, `alındı`, `GET`, `SQLSTATE` | Yalnızca harfler; kısa büyük harfli kelimeler de |
| diğer | `<SAYI>`, `oauth2`, `->`, `` `urunler` `` | Geri kalan her şey |

İlk beşi **kodsal** biçimlerdir. İki belirteç farklı ama ikisi de aynı kodsal biçimdeyse fark,
iki tarafında da rakam olan bir fark gibi [eşiği gevşetir](#esnek-eşik):
`İş tamamlandı: App\Jobs\FaturaGonder` ile `İş tamamlandı: App\Jobs\EpostaGonder` 2 belirtecin
1'inde aynıdır (0,5), eşik 0,7 × 1/2 = 0,35'e iner ve ikisi `İş tamamlandı: <*>` olur. Konum
**kategoriktir**: değerleri bir kümeden gelir. `Ödeme alındı` ile `Ödeme reddedildi` iki
sıradan kelimede ayrılır, eşik gevşemez ve iki olay ayrı kalır. Kodsal bir değerle sıradan bir
kelime (`Durum: ORDER_FAILED`, `Durum: hazır`) ya da iki ayrı kodsal biçim (`ORDER_FAILED`,
`odeme.bekliyor`) de birleşmez. Karar anlıktır, [ikinci tur](#değişken-konumlar) gerekmez.

Ağacın [ilk belirteç dalı](#drain) da bunu bilir: kodsal biçimli bir baş belirteç, rakamlı
belirteçler gibi ortak `<*>` dalına gider. Yoksa `ORDER_CREATED olayı işlendi` ile
`ORDER_FAILED olayı işlendi` hiç karşılaştırılmazdı.

Temkinli kalan yerler:

- Kısa büyük harfli kelimeler sözcüktür: `GET /api/urunler` ile `POST /api/urunler` farklı
  olaylardır; `ORDER_CREATED` ancak alt çizgisiyle sabit olur.
- SQL'in ters tırnaklı tablo ve sütun adları (`` `urunler` ``) kodsal sayılmaz: tablosu farklı
  iki yavaş sorgu, iki ayrı sorgudur ve [eşik tablosundaki](#drain) gibi ayrı kalır.
- Tek kelimelik mesajlar değişmez: `odeme.basarisiz` tek başına bir olay adıdır,
  `odeme.basarili` ile birleşmez.
- Kuyruk adları, durum kelimeleri (`emails`, `default`, `pending`) sıradan kelimelerdir; yüz
  farklı değer sınırı onlar için geçerlidir.

Örnek logdaki `Zamanlanmış görev başladı: kurlar:guncelle` ve `yedek:al` kayıtları zaten
0,7'de birleşiyordu; eşik 0,8'e çıkarılınca da birleşmeye devam eder, çünkü komut adları
noktalı biçimdedir.

Rapor her kalıbın her `<*>` konumunun değerlerini sayar ([kalıp raporu](#kalıp-raporu)):
`değerler: App\Jobs\FaturaGonder ×2, App\Jobs\EpostaGonder ×1`. Bu sayım rapor zamanında,
depodaki maskeli mesajlardan yapılır; her farklı maskeli mesaj zaten bir
[alt gruptur](#akışlı-okuma-ve-bellek), ek bellek gerekmez. Rakamlı farklılıklarla ya da
yüz değer sınırıyla açılan konumlar da sayılır: `Kullanıcı <SAYI> <*> yaptı` kalıbının
`giriş ×8, çıkış ×1` değerleri, Drain'in sözdizimine bakıp birleştirdiği iki olayı görünür
kılar.

## Hata parmak izi

Aynı PHP mesajı kodun birçok yerinden gelebilir: `Attempt to read property "ad" on null`
hem kategori sayfasında hem ürün sayfasında oluşabilir. Drain yalnızca mesaja baktığı için
bunları tek kalıpta birleştirir, oysa bunlar iki ayrı hatadır.
[Sentry](https://docs.sentry.io/concepts/data-management/event-grouping/) gibi hata takip
araçları bu yüzden hataları mesajla değil, stack trace'in uygulamaya ait kısmıyla tanır.
veylog bunun sade bir halini kullanır: istisna sınıfı, uygulama kodundaki ilk satır ve o
satırın içinde bulunduğu fonksiyon.

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
3. Stack trace'in çerçeveleri (`#2 dosya(satır): çağrı`) okunur. `[previous exception]` ile
   başlayan kısım bu istisnaya yol açan önceki istisnadır; okunmaz.
4. Proje kökü, `/vendor/` içeren ilk yoldan bulunur (`/var/www/magaza/`) ve yollardan
   silinir; hiçbir yolda vendor yoksa `/storage/framework/views/` içeren ilk yoldan. Böylece
   her dağıtımda klasör adı değişse de (`releases/20261024/`) konum aynı kalır. Sıra
   önemlidir: sıfır kesintili dağıtımlarda (Forge, Envoyer, Deployer) `storage` bütün
   dağıtımların paylaştığı bir klasördür ve derlenmiş şablonun yolu dağıtımın klasörünün
   dışındadır; yığın izinde vendor'dan önce gelse de kök vendor'dan bulunur. Kaydın kendi
   yollarında ikisi de yoksa, örneğin yığın izi olmayan ölümcül bir hatada, logun başka
   kayıtlarında bulunan kökler denenir.
5. Adaylar içten dışa taranır: önce fırlatıldığı yer, sonra onu çağıranlar. `vendor/` ve
   `storage/framework/views/` altında olmayan, giriş noktası da (`artisan`,
   `public/index.php`, eski Laravel'lerde `server.php`) olmayan ilk dosya seçilir.
   Middleware'in isteği bir sonraki adıma ilettiği satır atlanır.
6. Seçilen satırın içinde bulunduğu fonksiyon, yığın izinde bir sonraki satırda çağrılan
   fonksiyondur ([satır değil fonksiyon](#satır-değil-fonksiyon)).

Örnekteki istisna vendor'da fırlatıldı; uygulama kodundaki ilk satır `#2`'dir, `#3` onun
`store()`'un içinde olduğunu söyler:
`app/Http/Controllers/KayitController.php:38, store() (UniqueConstraintViolationException)`. Hata
tamamen vendor içinde oluştuysa, örneğin bir kuyruk işinin deneme hakkı bittiyse, konum
"uygulama kodunda bulunamadı" olarak gösterilir.

Parmak izi Drain'in kalıplarını yalnızca **böler**: aynı kalıptaki kayıtlar farklı yerlerden
geliyorsa ayrı satırlarda görünür. Aynı yerden gelen farklı kalıplar ise birleştirilmez,
çünkü bir yardımcı fonksiyondaki tek bir `throw` birbirinden bağımsız birçok hatanın ortak
yeri olabilir.

### Satır değil fonksiyon

Satır numarası kodla birlikte değişir. Bir dağıtım `SiparisServisi.php`'nin başına dört satır
eklerse dosyadaki her hata dört satır aşağı kayar: satırla tanınan bir hata dağıtımdan sonra
başka bir hata gibi görünür ve `--karsilastir` onu hem "kaybolan" hem "yeni çıkan" sayar.
Konum bu yüzden dosya ve satırın içinde bulunduğu fonksiyondur:
`app/Services/SiparisServisi.php:38, kargoUcreti()`. Satır gösterilir ama kimliğe katılmaz;
gösterilen, o hatanın en yeni kaydınınkidir: kod değiştiyse güncel satır.

Yığın izinin her satırı bir yeri ve o yerde çağrılan fonksiyonu yazar. Bir yerin içinde
bulunduğu fonksiyon, bir sonraki satırda çağrılan fonksiyondur (gerçek bir Laravel 13
logundan, kısaltıldı):

```text
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Foundation/Bootstrap/HandleExceptions.php(265): Illuminate\\Foundation\\Bootstrap\\HandleExceptions->handleError()
#1 /var/www/magaza/app/Services/SiparisServisi.php(34): Illuminate\\Foundation\\Bootstrap\\HandleExceptions->Illuminate\\Foundation\\Bootstrap\\{closure}()
#2 /var/www/magaza/app/Http/Controllers/SiparisController.php(23): App\\Services\\SiparisServisi->kargoUcreti()
```

`SiparisServisi.php:34`, `kargoUcreti()`'nin içindedir. İstisna çoğu zaman yığın izinde
bulunmayan bir yerde fırlatılır; o yerin fonksiyonu ilk satırda çağrılan fonksiyondur. PHP'nin
bir uyarısından doğan `ErrorException`'da (`Undefined array key`) ise fırlatıldığı yer yığın
izinde de görünür: Laravel'in hata işleyicisi oradan çağrılmış gibidir (`#1`). Fonksiyonu da o
satırınkidir.

Gösterilen ad kısadır: sınıfın adı dosyanın adıyla aynıysa (PSR-4) yalnızca metot
(`kargoUcreti()`), değilse sınıfıyla (`Urun::bul()`). Migration'ların anonim sınıfının adı bir
şey söylemez, yazılmaz: `up()`. Tam ad JSON çıktısındadır:
`App\Services\SiparisServisi->kargoUcreti`.

**Closure'lar.** PHP 8.4'ten önce bir sınıftaki bütün closure'lar aynı adla yazılır:
`App\Services\SiparisServisi->App\Services\{closure}`. İki metottaki iki closure'ı ayırmak için
closure'ın tanımlandığı fonksiyon da bulunur. Closure çoğu zaman tanımlandığı yerden bir
fonksiyona verilir, o fonksiyon da onu çağırır:

```text
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/Concerns/ManagesTransactions.php(35): App\\Services\\SiparisServisi->App\\Services\\{closure}()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Database/DatabaseManager.php(502): Illuminate\\Database\\Connection->transaction()
#2 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Support/Facades/Facade.php(364): Illuminate\\Database\\DatabaseManager->__call()
#3 /var/www/magaza/app/Services/SiparisServisi.php(12): Illuminate\\Support\\Facades\\Facade::__callStatic()
#4 /var/www/magaza/app/Http/Controllers/SiparisController.php(11): App\\Services\\SiparisServisi->olustur()
```

Closure `DB::transaction()`'a 12. satırda verildi. Yığın izinde closure'dan dışa doğru aynı
dosyadaki ilk satır (`#3`) onu tanımlayan fonksiyonun içindedir: `olustur()`. Konum
`{closure:olustur()}` olur. PHP 8.4 bunu closure'ın adına kendisi yazar
(`{closure:App\Services\SiparisServisi::olustur():12}`); o zaman addan okunur, sonuç aynıdır.
Satır numarası addan atılır. Tanımlandığı satır yığın izinde yoksa (closure bir değişkende
saklanıp başka bir yerden çağrıldıysa) yalnızca `{closure}` yazılır. Rota dosyalarındaki
closure'ların sahibi yoktur: `routes/web.php` Laravel'in kendi sınıfının içinde yüklenir ve
closure'ları onun adıyla yazılır; bu ad gösterilmez.

**En üst düzey.** Rota dosyası yüklenirken oluşan bir hatada kod bir fonksiyonun içinde
değildir: yığın izinde bir sonraki satır `require(...)`'dır. Çalıştırılan bir betiğin en üst
düzeyinde de `{main}`'dir. Bu konumlar satırla belirlenir.

**Çağrısız yığın izi.** Monolog'un JSON biçiminde yığın izinde yalnızca yerler yazılır
(`"trace":["/var/www/magaza/app/Http/Controllers/KayitController.php:38", ...]`); yığın izi
kapalıysa yalnızca fırlatıldığı yer bilinir. Böyle bir kaydın fonksiyonu logun başka
kayıtlarından öğrenilir: okunan her yığın izinin uygulama kodundaki satırları bir haritaya
yazılır (`dosya:satır` → fonksiyon). Okuma bitince satırla kalmış konumlar bu haritaya bakılarak
fonksiyonla belirlenir; bir kanal JSON yazıyorsa ya da log biçimi değiştirildiyse aynı hatanın
JSON ve metin kayıtları böyle birleşir. Harita yalnızca kesin bilgiyle çalışır: aynı satır iki
farklı fonksiyonda görüldüyse (log bir dağıtımı aşıyor ve satır başka bir fonksiyona kaymış)
o satır kullanılmaz. Logun hiçbir yığın izinde geçmeyen bir satır da satırla kalır.

`--konum satir` 0.1'in davranışıdır: konum dosya ve satırdır. Aynı fonksiyonun iki satırında
aynı istisna sınıfı ve aynı kalıpla oluşan iki hata fonksiyonla tek satır olur; satırla
ayrılır.

Gerçek Laravel 13 çıktısıyla sınandı. Dört hata (bir `DB::transaction()` closure'ında, bir
`Collection::map()` closure'ında, bir PHP uyarısında ve bir rota closure'ında) hem Laravel'in
metin biçiminde hem yığın izi kapalı JSON'da loglandı; sonra servisin başına dört satır eklenip
aynı istekler tekrarlandı. Varsayılan konumla 16 kayıt 4 satırda kaldı, JSON kayıtları metin
kayıtlarıyla birleşti ve `--karsilastir` dağıtımda hiçbir hatayı yeni saymadı. `--konum satir`
ile 7 satır oldu; üç hata hem kaybolan hem yeni çıkan sayıldı. Kişisel bir projenin 13 MB'lık
geliştirme logunda 169 satır 158'e indi: farklı sürümlerde başka satırlarda görünen aynı
hatalar birleşti. Satır numarası ters yönde de yanıltıyordu: bir denetleyicinin 19. satırı bir
sürümde `create()`'in, sonraki bir sürümde `show()`'un içindeydi. Satırla iki ayrı metodun
hataları tek satırdı; fonksiyonla ayrıldılar.

### PHP istisna metni

`report($e)` ve yakalanmayan istisnalar bağlama yazılır. `Log::error($e)` ise istisnayı
bağlama koymaz: Laravel onu metne çevirip mesaj olarak yazar. Metin PHP'nin
`Throwable::__toString()`'idir (gerçek bir Laravel 13 logundan, yollar kısaltıldı):

```text
[2026-10-25 17:38:17] local.ERROR: RuntimeException: Dize olarak loglanan istisna in /var/www/magaza/routes/web.php:24
Stack trace:
#0 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/CallableDispatcher.php(39): Illuminate\Routing\RouteFileRegistrar->{closure}()
#1 /var/www/magaza/vendor/laravel/framework/src/Illuminate/Routing/Route.php(254): Illuminate\Routing\CallableDispatcher->dispatch()
...
#48 {main}
```

veylog bu metni şöyle okur:

1. Kaydın satırlarında `Stack trace:` aranır; yoksa kayıt bir istisna metni değildir. Başka
   hiçbir şey ayrıştırılmaz.
2. `Stack trace:`'ten önceki satırın sonu ` in dosya:satır`'dır: istisnanın fırlatıldığı yer.
   Mesaj ` in ` içerebilir, yer sondan aranır. Mesajı satır sonu içeren istisnalarda
   (Laravel'in HTTP istemcisinin hataları) başlık birkaç satır sürer.
3. Sınıf başlığın ilk satırındadır: `Sınıf: mesaj`. Mesaj boşsa başlık yalnızca `Sınıf in yer`
   olur.
4. İstisna başka bir istisnadan doğduysa PHP önce en içtekini yazar, sonra her birini boş bir
   satır ve `Next` ile. Logu yazdıran istisna en sondadır; parmak izi ve mesaj ondan alınır.
   Bağlamdaki istisnada da önceki istisnanın yığın izi okunmaz.
5. Yığın izinin satırları bağlamdakiyle aynı biçimdedir (`#2 dosya(satır): çağrı`) ve aynı
   kurallarla taranır.

Kaydın mesajı istisnanın mesajıdır, `report($e)`'nin yazdığı gibi; aynı hata iki yolla
loglansa da tek satırda birleşir:

```text
     2  ERROR      14:03:11  14:05:40  Ödeme alınamadı: sipariş <SAYI>
                                       örnek: Ödeme alınamadı: sipariş 1047
                                       konum: app/Services/OdemeServisi.php:64, tahsilEt() (RuntimeException)
```

İstisnanın önüne bir açıklama da yazılabilir: `Log::error('Ödeme başarısız: ' . $e)`. PHP sınıf
adlarına bir kısıt koymaz, `Ödeme` de bir sınıf adı olabilir; bu yüzden yalnızca istisna adı
gibi görünen bir ad sınıf sayılır: ad alanı içeren (`App\Exceptions\OdemeHatasi`) ya da
`Exception` veya `Error` ile biten (`RuntimeException`, `TypeError`). Satır böyle bir adla
başlıyorsa açıklama yoktur; başlamıyorsa bir boşluktan sonra gelen ilk böyle ad sınıftır,
öncesi açıklamadır. Kaydın mesajı açıklamayla istisnanın mesajıdır:
`Ödeme başarısız: Bağlantı koptu`.

Bağlamda `\` işareti JSON kaçışıyla `\\` diye yazılır, istisna metninde ise tektir
(`App\Services\X`, `C:\Users\...`). Okunan her sınıf adı ve yol bu yüzden önce çözülür;
parmak izleri çözülmüş halleriyle karşılaştırılır. Aynı hata hangi yolla loglanmış olursa olsun
aynı parmak izini taşır. Kök de çözülmüş yollardan bulunur: istisna metnindeki bir kaydın
vendor yolu, bağlamla loglanmış ve kendi yollarında vendor geçmeyen bir kaydın kökünü verir.

Gerçek Laravel 13 çıktısıyla sınandı: aynı istisnalar (çok satırlı mesajı olan bir HTTP
istemcisi hatası, önceki istisnası olan bir ödeme hatası, mesajsız bir istisna) bağlamla,
`Log::error($e)` ile ve açıklamalı `Log::error('...' . $e)` ile loglandı. Bağlamla ve
`Log::error($e)` ile loglananlar aynı satırda birleşti; açıklamalılar aynı yeri gösteren ayrı
satırlar oldu.

### Middleware

Bir route middleware'inden geçen her istekte, sonraki adımlarda oluşan her hatanın yığın
izinde middleware'in `return $next($request);` satırı da bulunur. Denetleyicide tanımsız bir
metot çağrıldıysa (`Call to undefined method ...::duzenle()`) uygulama kodundaki tek satır
budur; ama hatanın yeri orası değildir, istek yalnızca oradan geçmiştir. Yığın izinin her
satırı o satırda yapılan çağrıyı da yazar ve bu satırdaki çağrı, Laravel'in Pipeline'ındaki
bir closure'dır:

```text
#3 /var/www/magaza/app/Http/Middleware/YoneticiMi.php(13): Illuminate\\Pipeline\\Pipeline->Illuminate\\Pipeline\\{closure}()
```

PHP 8.4'ten beri closure'ın adında nerede tanımlandığı da yazar:
`Illuminate\\Pipeline\\Pipeline->{closure:Illuminate\\Pipeline\\Pipeline::carry():184}`. İki
yazılış da tanınır ve bu satırlar aday sayılmaz. Middleware'in başka bir çağrısı (ör.
`abort_if()`) hatanın yeri olabileceği için atlanmaz.

### View hataları

Blade şablonundaki bir hata `ViewException` olarak loglanır ve Laravel mesajın sonuna
şablonun yolunu ekler: `Undefined variable $urun (View: /var/www/magaza/resources/views/vitrin.blade.php)`.
Hata, şablonun PHP'ye derlenmiş halinde (`storage/framework/views/8748fa....php`) oluşur; o
dosyanın adı bir özettir ve satırı Blade dosyasındaki satıra karşılık gelmez. Şablondaki bir
`route()` hatası da yanıt hazırlanırken, denetleyici döndükten sonra oluştuğu için yığın
izinde denetleyici bulunmaz. Bu yüzden view hatalarının konumu şablonun kendisidir, satır
numarası olmadan. Şablon başka bir şablonun içindeyse Laravel her düzey için bir `(View: ...)`
ekler; ilki hatanın oluştuğu şablondur. Şablon bir paketin vendor klasöründeyse uygulama kodu
sayılmaz ve yığın izine bakılır.

```text
$ veylog --ilk 8 ornekler/karisik.log
...
     2  ERROR      09:48:21  09:49:05  Route [panel.kampanya.olustur] not defined. (View: <YOL>)
                                       örnek: Route [panel.kampanya.olustur] not defined. (View: C:\Users\İrem\Herd\ma…
                                       konum: resources/views/panel/kampanyalar.blade.php (ViewException)
     1  ERROR      09:52:37  09:52:37  Call to undefined method App\Http\Controllers\KampanyaController::duzenle()
                                       konum: uygulama kodunda bulunamadı (Error)
     1  ERROR      09:55:10  09:55:10  Undefined variable $urun (View: <YOL>) (View: <YOL>)
                                       örnek: Undefined variable $urun (View: C:\Users\İrem\Herd\magaza\resources\view…
                                       konum: resources/views/partials/urun-karti.blade.php (ViewException)
     1  ERROR      09:57:44  09:57:44  Trait "App\Traits\Disariaktarilabilir" not found
                                       konum: app/Http/Controllers/RaporController.php:5 (FatalError)
  … 5 satır daha; tümünü görmek için --hepsi
```

Son satırdaki ölümcül hatanın yığın izi yoktur (`#0 {main}`); yalnızca fırlatıldığı yer
bilinir ve orada vendor geçmez. Kök, logun diğer kayıtlarından bulunur.

**Derlenmiş şablonlar** uygulama kodu sayılmaz. Şablondaki sıradan bir hata `ViewException`
olarak loglandığı için derlenmiş şablonda kalan hatalar yalnızca ölümcül hatalardır
(`Maximum execution time of 30 seconds exceeded`); onların yeri de rastgeledir: süre o an
nerede dolduysa orasıdır. Böylece aynı zaman aşımı, bir kez şablonda bir kez vendor'da
yakalansa da tek satırda kalır.

Kişisel bir Laravel projesinin 13 MB'lık geliştirme logunda (699 kayıt) 282 view hatasının
hepsi kendi şablonunu gösteriyor; middleware'e bağlanmıyor. Tanımsız bir denetleyici metodunu
çağıran 53 kayıt "uygulama kodunda bulunamadı" diyor (eksik metot mesajda yazıyor) ve süre
sınırına takılan 9 ölümcül hata tek satırda toplanıyor.

### Windows yolları

Herd ya da Laragon'da yolların ayracı `\`'dir ve bağlam JSON olduğu için logda `\\` diye
yazılır: `C:\\Users\\İrem\\Herd\\magaza\\vendor\\...`. Yol okunurken kaçış çözülür, proje kökü
`\vendor\`'dan da bulunur ve konum gösterilirken ayraç `/` olur:
`database/migrations/2026_10_25_090000_create_kuponlar_table.php:11, up()`. Giriş noktası da
(logda `public\\index.php`) aynı yolla tanınır. Gerçek bir geliştirme logunda migration hatalarının
konumu böylece migration dosyası olarak bulundu; önceden vendor'daki `Connection.php:822`
görünüyordu.

## Saat dilimleri

Laravel'in varsayılan biçimi saat dilimi yazmaz (`2026-10-24 14:03:11`); zaman uygulamanın
saat dilimindedir (`config/app.php`'deki `timezone`). Monolog'un ISO 8601 biçimi ve JSON
satırları yazar (`2026-10-24T14:03:11.123456+03:00`). Bir logda saat dilimleri karışabilir:

- Yaz saatine geçişi kapsayan bir log: `+03:00` ile yazılmış kayıtların ardından `+02:00`
  ile yazılmışlar gelir. Saat dilimi yok sayılsa 02:30'da yazılan bir kayıt 03:10'dakinden
  sonra sıralanırdı; gerçekte ondan 40 dakika öncedir.
- Saat dilimi değiştirilmiş bir uygulama ya da farklı dilimlerdeki sunucuların birleştirilmiş
  logları.

Bütün zamanlar bu yüzden tek bir dilimde, raporun saat diliminde karşılaştırılır ve
gösterilir. Raporun dilimi `--saat-dilimi` ile verilir (`+03:00`, `-05:30`, `Z`); verilmezse
logda saat dilimi yazılı ilk kaydınkidir. Her kaydın zamanı okunurken bu dilime çevrilir:

```text
zaman (raporun diliminde) = yazılan zaman − kaydın dilimi + raporun dilimi
2026-10-25T02:10:00+02:00 → 02:10 − 2 sa + 3 sa = 03:10 (+03:00'da)
```

Saat dilimi yazılmamış bir kayıt raporun diliminde yazılmış sayılır: gerçek dilimini bilmenin
yolu yoktur. Raporun dilimi verilmediyse ilk dilimli kayıtta belirlenir ve bir daha değişmez;
ondan önce gelen dilimsiz kayıtlar zaten bu dilimde sayılacakları için çevrilmeleri gerekmez.
Böylece her kayıt geldiği anda çevrilir; logun tamamını önce okumak gerekmez.

Raporda saat dilimi ayrı bir satırdadır; kaç kaydın çevrildiği de yazar:

```text
  Zaman aralığı : 2026-10-24 12:00:00 → 2026-10-24 14:45:00
  Saat dilimi   : +03:00 (--saat-dilimi); 2 kayıt +00:00 diliminden çevrildi
```

Logda hiç saat dilimi yazılmamışsa ve verilmediyse satır çıkmaz; zamanlar logdaki gibidir.
`--saat-dilimi` saat dilimi yazılmamış bir loga verilirse zamanlar değişmez, rapor bunu söyler.
Zamanlar her yerde tek biçimde yazılır: `2026-10-24 14:03:11`; kesir ve dilim yazılmaz.

Logun sonunu bilmeyi gerektiren filtreler (`--son`) için yapılan ön tarama, ani artışların
örneklerini bulmak için logun ortasından yapılan yeniden okuma da aynı çeviriyi yapar. Ön tarama
logu aynı sırayla okuduğu için aynı dilimi bulur. Yeniden okuma deponun bulduğu dilimle başlar:
dilim belliyken bir kaydın zamanı yalnızca kendisine bağlıdır, okumaya nereden başlandığına
bağlı değildir. Filtrelerin, `--karsilastir`'ın ve ani artış dilimlerinin zamanları da raporun
dilimindedir: `--saat-dilimi +03:00 --baslangic 14:00` UTC'de yazılmış bir logun 11:00'den
sonrasıdır.

Saat dilimleri adla (`Europe/Istanbul`) değil farkla (`+03:00`) verilir. Adla verilen bir
dilimin farkı yaz saatine göre değişir; bunu bilmek için IANA saat dilimi veritabanı gerekir.
Logda zaten her kaydın kendi farkı yazılıdır.

## Filtreler

Filtre kayıtlar okunurken uygulanır: filtreye uymayan bir kayıt maskelenmez, kümelenmez,
depoya girmez. Rapor böylece log yalnızca uyan kayıtlardan oluşuyormuş gibi çıkar; kalıplar,
seviyeler, zaman aralığı, karşılaştırma ve ani artışlar onlardandır. Dosyanın satır ve kayıt
sayıları ise dosyanın tamamınındır; uyan kayıt sayısı `Filtre` satırında yazar:

```text
$ veylog --seviye error --son 1sa30dk ornekler/laravel.log
ornekler/laravel.log: 205 satır
  Kayıt         : 59
  Devam satırı  : 143
  Yetim satır   : 3
  Filtre        : ERROR ve üstü, 2026-10-24 17:01:07 ve sonrası (son 1 sa 30 dk): 3 kayıt
  Zaman aralığı : 2026-10-24 17:05:13 → 2026-10-24 17:55:03
```

| Filtre | Kayıt | Sınır |
|---|---|---|
| `--seviye warning` | WARNING ve üstü | Seviyeler büyük ya da küçük harfle yazılabilir. |
| `--baslangic 15:00` | Bu andan sonraki | Dahil. |
| `--bitis 2026-10-25` | Bu andan önceki | Hariç: `--bitis 2026-10-25` 24 Ekim'in son saniyesine kadardır. |
| `--son 2sa` | En yeni kayıttan iki saat öncesinden bu yana | Dahil. Birimler `sn`, `dk`, `sa`, `g` (ya da `s`, `m`, `h`, `d`); birleştirilebilir: `1sa30dk`. |

Zamanlar `--karsilastir` gibi yazılır; yalnızca saat verilirse logun son gününe uygulanır.
`--son` ve yalnızca saat verilmiş sınırlar logun sonunu, yani en yeni kaydı bilmeyi
gerektirir. Kayıtlar her zaman zaman sırasıyla yazılmadığı için en yeni kayıt dosyanın
sonundaki kayıt olmayabilir: log önce baştan sona taranır. Taramada kayıtlar toplanmaz,
yalnızca bir kaydın başladığı satırlara bakılır. Standart girdi yalnızca bir kez okunabildiği
için tarama sırasında geçici bir dosyaya kopyalanır ve asıl okuma kopyadan yapılır.

Filtreye uymayan kayıtlar en pahalı adımları (maskeleme, Drain, parmak izi) atladığı için
filtreli bir rapor daha hızlı çıkar. 500 MB'lık sentetik bir logda:

| Komut | Süre | Bellek |
|---|---|---|
| filtresiz | 8 sn | 17 MB |
| `--seviye error` | 1,9 sn | 7 MB |
| `--son 2sa` (tarama dahil) | 2,5 sn | 8 MB |

## Kalıp raporu

Drain yalnızca metinle ilgilenir; seviyeyi, zamanı ve stack trace'i bilmez. Rapor, Drain'in
her mesaj için bulduğu kümeyi kaydın parmak iziyle birlikte kullanır: ikisi de aynı olan
kayıtlar tablonun bir satırını oluşturur ([bellekte nasıl toplandıkları](#akışlı-okuma-ve-bellek)).
Her satıra şu bilgiler eklenir:

- **seviye:** satırdaki kayıtların en önemlisi.
- **ilk / son:** ilk ve son görülme zamanı, raporun saat diliminde. Tek günlük logda yalnızca
  saat, birden çok günü kapsayan logda ay ve gün de (`10-23 23:59:58`), birden çok yıla
  yayılan logda yıl da (`2025-07-23 08:37:53`) gösterilir. Zamanlar metin olarak değil tarih
  olarak karşılaştırılır; aynı logdaki iki zaman biçimi (`2026-10-24 10:00:00` ve
  `2026-10-24T09:00:00+00:00`) doğru sıralanır.
- **örnek:** satırdaki ilk kaydın maskelenmemiş mesajı; `<SAYI>` ve `<*>`'ın arkasındaki
  gerçek değerleri gösterir. Kalıp ham mesajla aynıysa gösterilmez.
- **değerler:** kalıptaki her `<*>` konumunun satırdaki kayıtlarda aldığı en sık beş değer,
  adetleriyle; kalanı `+N başka` (N: listelenmeyen farklı değer sayısı). Adetler alt
  grupların adetlerinin toplamıdır; eşit adette logda önce görülen önce. Birden çok `<*>`
  varsa her konum kalıptaki sırasıyla, 1'den sayılarak (`2: ahmet ×4; 4: giriş ×5`).
  Belirtecin cümleden gelen ayraç ve noktalaması değere katılmaz: `(Ayşe,` değeri `Ayşe`dir.
  Liste kalıp genişliğine sığmıyorsa değerler bölünmeden alt satırlara taşar. Kalıpta `<*>`
  yoksa gösterilmez.
- **konum:** kayıtlar bir istisnadan geliyorsa uygulama kodundaki ilk satır, içinde
  bulunduğu fonksiyon ve istisna sınıfı. Satır, satırdaki kayıtların en yenisininkidir. Dosya
  yolu kısaltılmaz; birçok editörün terminalinde `dosya:satır` tıklanarak açılabilir.

Tablo varsayılan olarak önem sırasına göre dizilir: önce seviye, aynı seviyede adet. İlk 10
satır gösterilir; gösterilmeyen satır sayısı tablonun altında (JSON'da `gosterilmeyen`
alanında) yazar. Sayı sütunları en uzun sayıya göre genişler; 12 345 kayıtlı bir kalıp tabloyu
kaydırmaz. Üç sıralama vardır (`--siralama`):

| Sıralama | Ölçüt | Ne zaman işe yarar? |
|---|---|---|
| `onem` | Seviye, aynı seviyede adet | Önce neyi düzeltmeli? |
| `adet` | Adet, aynı adette seviye | Logu en çok ne dolduruyor? |
| `yeni` | İlk görülme zamanı (en yeni önce), aynı anda seviye | Son dağıtımdan sonra ne çıktı? |

Bütün sıralamalar kararlıdır: ölçütleri eşit olan satırlar logda ilk görüldükleri sırada
kalır. Örnek logda `--siralama yeni` günü sondan başa anlatır: 17:07'de toparlanan Redis,
17:05'teki çöküşü, 16:10'daki ödeme zaman aşımları...

## Akışlı okuma ve bellek

Log dosyası belleğe alınmaz, satır satır okunur. Bellekte o an okunan satır, toplanmakta olan
kayıt ve özet için biriktirilenler durur. Bir kaydın bittiği ancak bir sonraki kayıt
başladığında anlaşılır; o zaman kayıt işlenir ve bırakılır. İstisnası bağlamında olan bir
JSON kaydı beklemez: gereken her şey kendi satırındadır.

Her kayıttan özet için gerekenler bir depoya alınır:

| Depoda | Ne kadar yer tutar | Ne için |
|---|---|---|
| Alt gruplar: maskeli mesajı, Drain kümesi, istisnası (konumun gösterilen satırıyla) ve seviyesi aynı olan kayıtlar; her birinin kayıt sayısı, ilk kaydının mesajı, ilk ve son zamanı, en geç kaydının sırası | farklı alt grup başına | Kalıp tablosu |
| Her farklı maskeli mesaj, bir kez; Drain'in o mesaj için son bulduğu küme | farklı mesaj başına | Drain |
| Kayıt sütunları: her kaydın zamanı ve alt grubu | kayıt başına 2–3 bayt | Karşılaştırma, ani artışlar |
| Seyrek dizin: her 1024 kayıtta bir kaydın dosyadaki yeri | 1024 kayıtta 8 bayt | Bir kaydın mesajını yeniden okumak |
| Parmak izleri, proje kökleri, kökü bekleyen istisnalar | farklı istisna başına | Konum sütunu |
| Farklı ham mesaj sayısı | en fazla 4–8 MB, sonra 64 KB | "45 farklı mesaj" |

Bellek böylece dosyanın boyuyla değil, farklı mesajların sayısıyla büyür:

| Log | Boyut | 0.1.0 | Şimdi |
|---|---|---|---|
| Gerçek bir geliştirme logu | 13 MB | 21 MB | 9 MB |
| 30 günlük log, `--ani-artis 1` | 92 MB | 138 MB | 9 MB |
| Sentetik üretim logu, 1,5 milyon kayıt | 500 MB | 976 MB | 17 MB |
| Aynı, 6,3 milyon kayıt | 2 GB | 3 975 MB | 30 MB |

Süre değişmedi; 2 GB'lık log iki çekirdekli bir makinede 33 saniyede okunuyor (0.1.0'da 35
saniye).

**Drain sıraya bağlıdır.** Her kaydın maskeli mesajı, okunurken, logdaki sırasıyla Drain'e
verilir. Aynı mesaj logun farklı yerlerinde farklı kümelere katılabilir. Dört kayıtlık bir
örnek:

```text
Sepet güncellenemedi kullanıcı oturumu kapalı mobil uygulama   → 1. küme
Sepet güncellenemedi kullanıcı oturumu kapalı web tarayıcı     → 1. küme: … kapalı <*> <*>
Sepet boşaltılamadı kullanıcı oturumu kapalı web tarayıcı      → 2. küme (1.'ye 7'de 4)
Sepet güncellenemedi kullanıcı oturumu kapalı web tarayıcı     → 2. küme (2.'ye 7'de 6, 1.'ye 5)
```

İkinci ve dördüncü kayıt aynı mesajdır ama farklı kümelerdedir: dördüncü geldiğinde ona daha
çok benzeyen bir küme açılmıştır. Bu yüzden alt grubun anahtarında kümesi de vardır ve her
kayıt kendi sırasında kümelenir; her farklı mesajı bir kez kümelemek tabloyu değiştirirdi
(bu örnekte 2 + 2 yerine 3 + 1).

**Önbellek.** Logların çoğunda kayıtların büyük kısmı daha önce görülmüş bir mesajın
tekrarıdır; her tekrarda ağaçta arama yapmak gereksizdir. Ağacın her uzunluktaki kısmının
(belirteç sayısı aynı mesajların dalının) bir sürümü vardır: o uzunlukta bir küme açılınca ya
da bir kalıp değişince artar. Her mesajın son kümelenişi, o andaki sürümle birlikte saklanır.
Mesaj yeniden geldiğinde sürüm değişmemişse arama aynı kümeyi bulurdu ve kalıp (mesaja zaten
uyduğu için) değişmezdi: sonuç aramadan verilir. Bir mesajın aramasını etkileyen her şey
(dallar, yapraktaki kümelerin kalıpları ve değişken konumları, tek belirteçli kümeler) yalnızca
o uzunlukta değişir; sonuç her kaydı aramayla kümelemekle birebir aynıdır. Bu, rastgele
mesaj dizileriyle ve her ayarla, önbelleksiz kümelemeyle karşılaştırılarak sınanır.

**İkinci tur.** Drain bir yaprakta değişken bir konum bulursa
([değişken konumlar](#değişken-konumlar)) kayıtlar bu bilgiyle baştan, aynı sırayla bir kez
daha kümelenir. Girdi yeniden okunmaz: her kaydın maskeli mesajı kayıt sütunlarındaki alt
grubundan bilinir. Alt gruplar yeni kümeleriyle yeniden kurulur. Bir alt grubun kayıtları
ikinci turda farklı kümelere düşebilir; bölünen parçanın ilk kaydı alt grubun ilk kaydı
değilse mesajı depoda yoktur, girdiden yeniden okunur (aşağıda). Standart girdi
`--ani-artis` olmadan kopyalanmadığı için yeniden okunamaz; o zaman, nadiren, parçanın örneği
aynı maskeli mesajlı ilk kaydın mesajı olur.

0.1.0'la karşılaştırmada (60 log ve sıraya bağlılığı, ikinci turu ve bölünmeyi sınayan 228
rastgele log; dosyadan, standart girdiden ve süreç ikamesiyle) bütün tablolar aynı çıktı.

**Kayıt sütunları.** Karşılaştırma ve ani artışlar her kaydın zamanını ve hangi kalıp
satırına düştüğünü bilmelidir: kayıt başına iki sayı. Düz tutulsalar 12 bayt ederdi; sütunlar
onları değişken uzunluklu tamsayılar (LEB128) olarak yazar. Zaman bir önceki kaydın
zamanından farkı olarak yazılır (delta kodlaması): kayıtlar çoğunlukla zaman sırasıyla
yazıldığı için fark birkaç saniyedir ve tek bayta sığar. Fark eksi de olabilir (aynı anda
yazan iki işlem); zigzag kodlaması küçük eksi sayıları küçük artı sayılara çevirir (0 → 0,
−1 → 1, 1 → 2, −2 → 3). Alt grubun numarası ilk 128 alt grupta tek bayttır.

**Örneği yeniden okumak.** Bir ani artışın örneği, artışın ilk kaydının mesajıdır. O kayıt
bir alt grubun ilk kaydıysa mesajı depodadır; değilse dosyadan yeniden okunur: seyrek
dizindeki en yakın konuma atlanır, oradan en fazla 1024 kayıt okunur. Bulunan kaydın maskeli
mesajı ve seviyesi depodakiyle karşılaştırılır. Dosya okunduktan sonra değiştiyse (ör. log
döndürülürken `copytruncate` ile kesilip yeniden yazıldıysa) o sırada başka bir kayıt vardır;
o kayıt örnek olmaz, yerine alt grubunun örneği (maskeli mesajı, seviyesi ve istisnası aynı
ilk kaydın mesajı) yazılır ve bir uyarı çıkar.

Standart girdi yalnızca bir kez okunabildiği için `--ani-artis` verildiyse okunurken geçici
bir dosyaya kopyalanır. Dosya oluşturulur oluşturulmaz silinir: açık kaldıkça kullanılır,
program nasıl biterse bitsin (Ctrl+C dahil) diskte kalmaz. Unix'te yalnızca sahibi okuyup
yazabilir; silinmeden önceki kısa sürede başka bir kullanıcı açamaz. Kopya girdi kadar yer
tutar. Geçici dosya oluşturulamazsa (geçici klasör yok, disk dolu) girdi kopyalanmadan okunur:
rapor yine çıkar, yeniden okunamayan örneklerin yerine alt gruplarının örneği yazılır.

**Farklı mesajları saymak.** "45 farklı mesaj" için her ham mesajın 64 bitlik özeti saklanır
ve tam sayılır. Her mesajı başka olan bir logda (bir kimlik, bir zaman damgası) bu, kayıt
sayısıyla büyüyen bir bellek demektir. Farklı mesaj 262 144'ü geçince sayaç
[HyperLogLog](https://algo.inria.fr/flajolet/Publications/FlFuGaMe07.pdf)'a geçer: 64 KB'lık
sabit bir bellekle tahmin eder. Fikir şu: iyi bir özet rastgele bitler gibidir; başı k sıfırla
başlayan bir özete ortalama 2^k farklı değerde bir rastlanır. Tek bir ölçüm çok oynak olduğu
için özetler ilk 16 bitlerine göre 65 536 kovaya dağıtılır, her kova kendi en uzun sıfır
dizisini tutar. Tahmin Ertl'in sapmasız tahmincisiyle hesaplanır
([2017](https://arxiv.org/abs/1702.01284)); asıl formül küçük sayılarda saptığı için başka
araçlar deneysel düzeltme tabloları kullanır, bu tahminci kullanmaz. Standart hata %0,41;
1'den 18 milyona kadar denenen sayılarda en büyük hata %0,6 çıktı. Tahmin olan sayının önünde
`≈` yazar; JSON'da `ham_mesaj_yaklasik` alanı `true` olur.

İki koruma da var: bir satırın en fazla 256 MB'ı okunur; bir kaydın devam satırlarının en
fazla 256 MB'ı (ve 16 milyon satırı) saklanır, sığmayan ilk satırdan sonrakiler sayılır ama
saklanmaz. Satır sonu olmayan dev bir dosya ya da sonu gelmeyen bir döküm belleği
dolduramaz. Gerçek loglardaki en uzun satırlar bağlamında dev bir SQL sorgusu taşıyan
istisnalardır (MySQL'in bir sorguya izin verdiği en büyük boy 64 MB'tır); onlar da sınırın
çok altındadır. Saklanan kısım kaydın başıdır: istisnanın sınıfı ve yeri, uygulama kodunun
çerçeveleri.

### Canlı izleme

`--izle` logu `tail -f` gibi izler: sonuna kadar okur, raporu yazar, sonra eklenen kayıtları
okumaya devam eder ve yeni kayıt geldikçe raporu yeniden çizer. Dört parçadan oluşur.

**Artımlı depo.** Girdi hiçbir çizimde yeniden okunmaz. Depo okuma durumunu (alt grupların
anahtar sözlüğünü, toplanmakta olan kaydı) çizimler arasında korur; eklenen baytlar satırlara
ayrılır, satırlar kayıt toplayıcıya, tamamlanan kayıtlar depoya gider. Rapor her çizimde
depodan yeniden üretilir: maliyeti alt grupların sayısıyla orantılıdır, dosyanın boyuyla değil.
Drain bir çizimde değişken bir konum bulduysa kayıtlar o çizimde yeniden kümelenir (ikinci
tur); sonraki çizimlerde yeni bir değişken konum bulunursa bir tur daha yapılır ve önceki
turlarda öğrenilenler unutulmaz. Log bir kerede okunurken en fazla iki tur vardır; izlemede
konumlar parça parça öğrenildiği için kümeleme nadiren bir kerede okumadakinden farklı
olabilir.

**Kayıt tamamlanma kuralı.** Bir kaydın devam satırları (yığın izi) başlığından sonra gelir
ve yazan süreç onları birkaç yazma işlemiyle yazabilir. Okunan son kayıt bu yüzden hemen
depoya alınmaz: ondan sonra bir kayıt başlığı gelince ya da girdi bir aralık (`--aralik`,
varsayılan 2 saniye) boyunca sessiz kalınca (hiç bayt gelmeyince) tamamlanmış sayılır.
Laravel bir kaydı tek yazma işlemiyle yazar; kural, parça parça yazan süreçler ve satır
ortasında kesilen okumalar içindir. `\n` ile bitmemiş son satır da bekletilir: bir okuma
satırın ortasında bitebilir, satır o halde ne bir başlık ne de bir devam satırıdır. Sessizlikte
o da son satır sayılır, tıpkı bir dosyanın satır sonu olmayan son satırı gibi. Böylece girdi bir
aralık sessiz kaldıktan sonra çizilen rapor, o ana kadar gelen baytların bir kerede
özetlenmesiyle aynıdır. Bedeli: bir kayıt en geç bir aralık sonra raporda görünür. Yazan süreç
bir kaydın ya da satırın ortasında bir aralıktan uzun duraklarsa kalanı ayrı satırlar olur
(devam satırları yetim sayılır).

İzleme başlarken dosya bir aralıktan uzun süredir değişmemişse (değiştirilme zamanına
bakılır) sessizlik o zamandan sayılır: durgun bir logun son kaydı ilk çizimde görünür, bir
aralık sonra değil.

**Döndürme.** Her aralıkta önce eldeki dosya sonuna kadar okunur, sonra yolundaki dosyayla
karşılaştırılır; eski dosyaya döndürülmeden hemen önce yazılan kayıtlar böylece kaybolmaz.
Unix'te yolun düğüm (inode) numarası değiştiyse (logrotate dosyayı yeniden adlandırıp yenisini
açmıştır) eski dosya bitmiş sayılır (satır sonu olmayan son satırı da okunur), yeni dosya
açılıp baştan okunur; eldeki dosya okunandan kısaysa (`copytruncate`) başa sarılır.
Windows'ta düğüm numarası yoktur ve oluşturulma zamanı güvenilir değildir (yeniden
adlandırılan bir dosyanın yerine hemen açılan yeni dosya eskisinin zamanını alabilir); yoldaki
dosya her seferinde yeniden açılır, oluşturulma zamanı başkaysa ya da eldekinden kısaysa
başka bir dosya sayılır. Depo korunur: eski dosyanın kayıtları raporda kalır, yalnızca
örnekleri artık yeniden okunamaz. Yeni dosyanın ilk kaydı seyrek dizine yeni dosyadaki
konumuyla yazılır; dizinin öteki kayıtları her 1024 kayıtta birdir ve döndürmenin ortasına
düşen bloktaki yeni kayıtlar, o olmasa eski dosyanın konumlarıyla aranırdı. Yol geçici olarak yoksa (döndürmenin ortası) eldeki dosya
okunmaya devam eder, sonraki aralıkta yeniden bakılır. Durum satırına `döndürüldü` yazılır.

**Kayan pencere.** Zaman filtreleri okunurken değil her çizimde, o ana kadar okunan en yeni
kayda göre uygulanır: `--son 1sa` hep son bir saati, `--baslangic 15:00` hep logun son
gününün 15:00'inden sonrasını gösterir. Deponun pencere görünümü (`Depo::pencere`) kayıt
sütunlarını bir kez dolaşıp alt grupların sayılarını ve zamanlarını yalnızca pencereye giren
kayıtlardan toplar; kalıplar, maskeli mesajlar, parmak izleri ve proje kökleri okunan bütün
kayıtlardan gelir (kümeleme yinelenmez; pencerenin dışına çıkan kayıtlar da öğretir). Farklı
ham mesaj sayısı ve saat dilimi sayıları kayıt başına saklanmadığı için okunan bütün kayıtlar
içindir. Seviye filtresi ise okunurken uygulanır: uymayan kayıt depoya hiç girmez.

Standart girdi ayrı bir iş parçacığında okunur; ana döngü parçaları bir kanaldan alır ve bir
aralık boyunca parça gelmezse sessizlik sayar. Girdi bitince son rapor yazılır ve çıkılır.
`--ani-artis` istendiyse girdi okunurken geçici bir dosyaya da kopyalanır; artışların
örnekleri ondan okunur. Dosya izlenirken örnekler dosyanın kendisinden okunur.

Doğrulama: örnek log satır ortalarından üç parçaya bölünüp aralıklarla bir dosyaya eklenirken
(ve standart girdiye yazılırken) izlenir; son çizim, durum satırı dışında, dosyanın bir kerede
özetlenmesiyle birebir aynı çıkar (`tests/cli.rs`). Gerçek bir geliştirme logu parçalar
halinde eklenerek de aynı denklik görüldü.

## Komut satırı

Argümanlar için yaygın bir kütüphane ([clap](https://crates.io/crates/clap)) yerine küçük
bir ayrıştırıcı yazıldı. clap projeye 17 kütüphane ekliyor, mesajları İngilizce ve Türkçedeki
gibi virgüllü ondalık sayıları (`0,8`) tanımıyor. Kendi ayrıştırıcımız şu kuralları izler:

- Seçenekler dosya adından önce ya da sonra gelebilir. Aynı seçenek iki kez yazılırsa
  sonuncusu geçerlidir (`--ilk 5 --hepsi` hepsini gösterir).
- Değer boşlukla (`--esik 0,8`), `=` ile (`--esik=0,8`) ya da kısa seçeneklerde bitişik
  (`-n20`) verilebilir.
- `--`'dan sonraki her şey dosya adıdır: `veylog -- -garip-ad.log`. Tek başına `-` standart
  girdidir; `-` adlı bir dosya `./-` diye verilir.
- Türkçe karakterli yazımlar da geçerlidir: `--eşik`, `--sıralama önem`, `--biçim`, `--yardım`.
- Argümanlar işletim sisteminin verdiği ham haliyle (`OsString`) okunur. Linux'ta bir dosya
  adı geçerli UTF-8 olmayabilir; program böyle bir adla da çökmez. Boş bir dosya adı
  (`veylog "$LOG"`, LOG tanımsız) kullanım hatasıdır.
- Tanınmayan bir seçeneğe en çok benzeyen seçenek önerilir. Benzerlik
  [Levenshtein uzaklığıyla](https://en.wikipedia.org/wiki/Levenshtein_distance) ölçülür:
  birini ötekine çevirmek için gereken en az harf ekleme, silme ya da değiştirme sayısı
  (`--esk` → `--esik`: 1). Yanlış öneri vermemek için 5 harften kısa adlarda en fazla 1,
  uzunlarda en fazla 2 harf farka izin verilir.
- Okuma hataları Türkçe anlatılır: `dosya bulunamadı`, `okuma izni yok`, bir klasör
  verildiyse `bu bir klasör; log dosyasını verin`.
- Her uzun seçeneğin İngilizce adı da her dilde geçerlidir (`--threshold`, `--last`,
  `--watch`); değer adlarının da (`--sort count`, `--format text`, `--location line`).
  Öneriler iki dilin adları arasından seçilir; eşit yakınlıkta seçili dildeki ad önerilir:
  `--sor` Türkçede `--son`, İngilizcede `--sort` olur.

### Arayüz dili

`--dil en` (`--lang en`) ya da `VEYLOG_DIL=en` ortam değişkeni arayüzü İngilizceye çevirir:
yardım, rapor, Markdown, uyarılar ve hata mesajları. İkisi birden verilirse `--dil` geçerlidir.

Dil, argümanlar ayrıştırılmadan önce seçilir: `veylog --esk 1 --lang en x.log` gibi
`--lang`'dan önce gelen bir yanlışlığın mesajı da İngilizcedir. Seçilen dil iş parçacığına
yazılır ve metin üreten her yer onu oradan okur; böylece hata türlerinin `Display`'i ve
sayıları yazan küçük işlevler bir dil parametresi taşımaz. Çıktı yalnızca ana iş
parçacığında üretilir; testler kendi iş parçacıklarında çalıştığı için bir testin seçtiği dil
ötekileri etkilemez.

İki dil arasındaki farklar yalnızca metinde değildir:

- Ondalık ayracı: `eşik 0,7` ↔ `threshold 0.7`; yüzde işareti: `%1` ↔ `1%`; kat: `15,4 kat`
  ↔ `15.4×`. Komut satırı iki ayracı da her dilde kabul eder.
- Süre birimleri: `2 sa 30 dk` ↔ `2 h 30 min`; `--son` ve `--aralik` iki dilin birimlerini
  de okur (`2sa`, `2h`).
- Tekil ve çoğul: Türkçede sayıdan sonra ad çekimsizdir (`1 kayıt`, `3 kayıt`), İngilizcede
  değildir (`1 record`, `3 records`).
- Yer tutucular: kalıplar depoda Türkçe adlarla (`<SAYI>`) saklanır ve kümelenir; metin ve
  Markdown çıktısı onları İngilizcede `<NUM>`, `<PATH>`, `<EMAIL>`, `<TIME>` diye gösterir.

JSON çıktısı her dilde aynıdır: alan adları, değerler (`"konum": "metot"`) ve yer tutucular
değişmez. JSON'u okuyan bir betik, onu çalıştıranın dilinden bağımsız çalışmalıdır. Seviye
adları (`ERROR`) ve zamanlar zaten iki dilde aynıdır. Logun kendi içeriği (mesajlar, örnekler)
çevrilmez.

Doğrulama: İngilizce çıktıların her bölümü birim testlerinde karakter karakter denetlenir.
Bir uçtan uca test de İngilizce içerikli bir logu bütün bölümlerle (karşılaştırma, ani
artışlar, filtreler, saat dilimi, izleme, hatalar, yardım) İngilizce özetler ve çıktıda tek bir
Türkçe harf ya da arayüz kelimesi kalmadığını denetler; JSON'un iki dilde bayt bayt aynı
olduğu da sınanır.

## Terminal güvenliği

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
önceki bir sürümünde CRITICAL satırını ve tablo başlığını ekrandan siliyordu:

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
