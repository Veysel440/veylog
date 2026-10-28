# Çıktı biçimleri

Özet bir kez hesaplanır, sonra istenen biçimde yazılır; bu yüzden üç biçimdeki sayılar hep
aynıdır. `--ilk` ve `--siralama` her biçimde geçerlidir. Örneklerdeki çıktılar Türkçe
arayüzledir; varsayılan arayüz İngilizce olduğu için Türkçe çıktı `--dil tr` ya da
`VEYLOG_LANG=tr` ile alınır.

| Biçim | Seçenek | Ne için |
|---|---|---|
| Metin | varsayılan | Terminalde okumak |
| JSON | `--bicim json` | Betikler, `jq`, CI adımları |
| Markdown | `--bicim markdown` | GitHub issue ve PR açıklamaları |

Metin ve Markdown seçili dilde yazılır (varsayılan İngilizce; `--dil tr` ile başlıklar,
etiketler, ondalık ayracı, süre birimleri ve yer tutucular Türkçe: `<NUM>` yerine `<SAYI>`).
JSON her dilde aynıdır ve İngilizcedir: alan adları, sabit değerler ve yer tutucular
([arayüz dili](nasil-calisir.md#arayüz-dili)).

| Yer tutucu (İngilizce, JSON) | Türkçe arayüzde | Ne yerine |
|---|---|---|
| `<NUM>` | `<SAYI>` | sayı |
| `<PATH>` | `<YOL>` | dosya yolu |
| `<EMAIL>` | `<EPOSTA>` | e-posta adresi |
| `<TIME>` | `<ZAMAN>` | tarih ve saat |
| `<URL>`, `<UUID>`, `<IP>` | aynı | adres, UUID, IP adresi |

Canlı izlemede (`--izle`) yalnızca metin biçimi vardır. Her çizim, olağan metin raporunun
başına bir durum satırı eklenmiş halidir:

```text
İzleniyor: storage/logs/laravel.log · her 2 sn · 59 kayıt · son kayıt 2026-10-24 18:31:07
storage/logs/laravel.log: 205 satır
  Kayıt         : 59
...
```

Durum satırında izlenen dosya, aralık, depodaki kayıt sayısı, en yeni kaydın zamanı ve dosya
döndürüldüyse `döndürüldü` yazar. Çıktı bir terminalse her çizimden önce ekran temizlenir
(ANSI `ESC[2J ESC[H`). Bir dosyaya ya da boruya yazılıyorsa çizimler, aralarında 40 `─`
karakterinden oluşan bir ayırıcı satırla art arda yazılır; son çizim son ayırıcıdan sonrakidir.

## JSON

Örnek logun yalnızca ilk satırıyla (`--ilk 1`):

<!-- denetim: veylog --bicim json --ilk 1 ornekler/laravel.log -->
```json
{
  "version": "0.2.0",
  "file": "ornekler/laravel.log",
  "lines": 205,
  "records": 59,
  "continuation_lines": 143,
  "orphan_lines": 3,
  "time_range": {
    "oldest": "2026-10-24 08:59:58",
    "newest": "2026-10-24 18:31:07"
  },
  "time_zone": null,
  "levels": {
    "CRITICAL": 2,
    "ERROR": 16,
    "WARNING": 6,
    "NOTICE": 1,
    "INFO": 31,
    "DEBUG": 3
  },
  "patterns": {
    "distinct_messages": 45,
    "distinct_messages_estimated": false,
    "masked_messages": 27,
    "drain_patterns": 23,
    "groups": 24,
    "threshold": 0.7,
    "location": "method",
    "sort": "severity",
    "omitted": 23,
    "rows": [
      {
        "count": 2,
        "level": "CRITICAL",
        "first": "2026-10-24 17:05:13",
        "last": "2026-10-24 17:06:45",
        "pattern": "Redis bağlantısı kurulamadı: Connection refused [tcp://<IP>:<NUM>]",
        "sample": "Redis bağlantısı kurulamadı: Connection refused [tcp://10.0.3.12:6379]",
        "variables": [],
        "exception": {
          "class": "RedisException",
          "file": "app/Http/Middleware/HizSiniri.php",
          "line": 22,
          "method": "App\\Http\\Middleware\\HizSiniri->handle"
        }
      }
    ]
  }
}
```

- Hiçbir şey kısaltılmaz: kalıplar ve örnekler tam haliyle yazılır. Terminalde 80
  karakterde kesilen SQL kalıbı JSON'da 251 karakterin tamamıdır. Sınıf adı da tam
  haliyledir: `Illuminate\Database\UniqueConstraintViolationException`.
- Bütün zamanlar raporun saat diliminde ve tek biçimdedir: `2026-10-24 17:05:13` (yıl, ay,
  gün, saat, dakika, saniye; kesir ve dilim yazılmaz). Dilim `time_zone` alanındadır.
- `file` logun yoludur; standart girdiden okunduysa `<standard input>`.
- `exception`, kayıtlar bir istisnadan gelmiyorsa `null`'dır. `method` konumun içinde
  bulunduğu fonksiyonun tam adıdır: `App\Http\Middleware\HizSiniri->handle`, closure'larda
  `App\Services\SiparisServisi->{closure:olustur()}`. O zaman `line` o hatanın en yeni
  kaydının satırıdır ([satır değil fonksiyon](nasil-calisir.md#satır-değil-fonksiyon)).
  Hatayı sınıf adı değil dosya ve fonksiyon tanıtır: bir trait'teki metodun hatası onu
  kullanan bütün sınıflarda tek satırdır, `method` kayıtların ilkinin yazdığı sınıfla yazılır.
  Konum satırla belirlendiyse (`--konum satir`, bir dosyanın en üst düzeyindeki kod,
  fonksiyonu bilinmeyen JSON kayıtları) `method` `null`'dır. İstisnanın uygulama kodunda yeri
  bulunamadıysa `file`, `line` ve `method` `null`'dır. View hatalarında `file` şablondur;
  `line` ve `method` `null`'dır.
- `location`: konumun neyle belirlendiği (`--konum`): `method` ya da `line`. `sort`:
  tablonun sırası (`--siralama`): `severity`, `count` ya da `newest`.
- `levels` yalnızca logda geçen seviyeleri, en önemlisi başta olacak şekilde içerir.
- `distinct_messages` farklı ham mesaj sayısıdır; `masked_messages` maskelemeden sonraki,
  `drain_patterns` Drain'in kalıp, `groups` parmak iziyle bölünmüş satır sayısıdır.
- `distinct_messages_estimated`: `distinct_messages` bir tahmin mi? 262 144'ten fazla farklı
  ham mesaj varsa sayı HyperLogLog ile tahmin edilir (±%0,4,
  [ayrıntılar](nasil-calisir.md#akışlı-okuma-ve-bellek)); öteki bütün sayılar tamdır.
- `rows` en fazla `--ilk` kadar satırdır; `omitted` dışarıda kalanların sayısıdır. Bütün
  satırlar için `--hepsi`.
- `variables`: kalıptaki her `<*>` konumu için bir nesne, kalıptaki sırayla; `<*>` yoksa boş
  dizi. `position` konumun kalıptaki belirteç sırasıdır (1'den), `values` en sık beş değer
  (`value`) adetleriyle (`count`; adede göre azalan, eşitlikte logda önce görülen önce),
  `others` listelenmeyen farklı değer sayısı. Değerler belirtecin cümleden gelen ayraç ve
  noktalaması olmadan yazılır ([kategorik konumlar](nasil-calisir.md#kategorik-konumlar)).

Örnek logdaki `Kullanıcı <NUM> <*> yaptı` kalıbının `variables` dizisi:

<!-- denetim: veylog --bicim json --hepsi ornekler/laravel.log | jq '.patterns.rows[] | select(.pattern == "Kullanıcı <NUM> <*> yaptı") | .variables' -->
```json
[
  {
    "position": 3,
    "values": [
      { "value": "giriş", "count": 8 },
      { "value": "çıkış", "count": 1 }
    ],
    "others": 0
  }
]
```

### Saat dilimi

`time_zone` raporun saat dilimidir
([saat dilimleri](nasil-calisir.md#saat-dilimleri)). Logda hiç saat dilimi yazılmamışsa ve
`--saat-dilimi` verilmediyse `null`'dır: zamanlar logda yazıldığı gibidir. Laravel'in metin
kayıtlarıyla Monolog'un JSON satırlarının karıştığı örnek logda:

<!-- denetim: veylog --bicim json ornekler/karisik.log | jq '.time_zone' -->
```json
{
  "zone": "+00:00",
  "source": "log",
  "converted_records": 0,
  "converted_from": [],
  "records_without_zone": 12
}
```

- `source`: `option` (`--saat-dilimi` ile verildi) ya da `log` (logda yazılı ilk dilim).
- `converted_records`: başka bir dilimde yazılmış, raporun dilimine çevrilmiş kayıtlar;
  `converted_from` onların dilimleri, küçükten büyüğe.
- `records_without_zone`: saat dilimi yazılmamış, raporun diliminde sayılan kayıtlar.
  Sayılar filtreye uyan kayıtlarındır.

### Filtre

`filter` alanı yalnızca bir filtre (`--seviye`, `--baslangic`, `--bitis`, `--son`) verilince,
yetim satır sayısından (`orphan_lines`) sonra eklenir. Zaman sınırları (`since`, `until`)
logun sonuna göre çözülmüş halleriyle yazılır; verilmeyenler `null`'dır. `last_seconds`
`--son`'un süresidir, saniye olarak. `records` filtreye uyan kayıtlardır; dosyanın bütün
kayıtları en üstteki `records`tır.

<!-- denetim: veylog --bicim json --seviye warning --baslangic 16:00 ornekler/laravel.log | jq '.filter' -->
```json
{
  "min_level": "WARNING",
  "since": "2026-10-24 16:00:00",
  "until": null,
  "last_seconds": null,
  "records": 13
}
```

### Karşılaştırma

`comparison` alanı yalnızca `--karsilastir` verilince eklenir; normal raporun biçimi
değişmez.

<!-- denetim: veylog --bicim json --karsilastir 16:11 ornekler/laravel.log | jq '.comparison.increased.rows[] | {pattern, before, after, factor, p_value}' -->
```bash
veylog --bicim json --karsilastir 16:11 ornekler/laravel.log | jq '.comparison.increased.rows[] | {pattern, before, after, factor, p_value}'
```

```json
{
  "pattern": "cURL error <NUM>: Operation timed out after <NUM> milliseconds with <NUM> bytes received (see <URL>) for <URL>",
  "before": 1,
  "after": 5,
  "factor": 15.379400570884872,
  "p_value": 0.004243352322045974
}
```

- `point`, `before` ve `after`: bölme anı ile iki yarının başlangıcı (`start`), bitişi
  (`end`), süresi (`seconds`) ve kayıt sayısı (`records`). Zamanlar logdaki biçimden
  bağımsız olarak `2026-10-24 16:11:00` biçimindedir.
- `min_factor`, `significance`, `min_records`: "artan" şartları.
- `new`, `gone`, `increased`: her biri `count`, `omitted` ve `rows` içerir. Satırlar tablo
  satırının alanlarına ek olarak `before`, `after`, `factor` ve `p_value` taşır. Yenilerde
  kat sonsuzdur; JSON'da sonsuz olmadığı için `null` yazılır.

### Ani artışlar

`spikes` alanı yalnızca `--ani-artis` verilince eklenir.

<!-- denetim: veylog --bicim json --ani-artis 10 ornekler/laravel.log | jq '.spikes.rows[] | {pattern, usual, mad, spike}' -->
```bash
veylog --bicim json --ani-artis 10 ornekler/laravel.log | jq '.spikes.rows[] | {pattern, usual, mad, spike}'
```

```json
{
  "pattern": "cURL error <NUM>: Operation timed out after <NUM> milliseconds with <NUM> bytes received (see <URL>) for <URL>",
  "usual": 0,
  "mad": 0,
  "spike": {
    "start": "2026-10-24 16:10:00",
    "end": "2026-10-24 16:20:00",
    "slices": 1,
    "records": 6,
    "level": "ERROR",
    "first": "2026-10-24 16:10:02",
    "last": "2026-10-24 16:16:02",
    "sample": "cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes received (see https://curl.haxx.se/libcurl/c/libcurl-errors.html) for https://api.odeme.test/v1/tahsilat",
    "peak": {
      "start": "2026-10-24 16:10:00",
      "end": "2026-10-24 16:20:00",
      "count": 6,
      "score": 4.047
    }
  }
}
```

- `slice_minutes`, `min_level`, `min_slices`, `threshold`, `min_mad`: ayarlar ve eşikler.
- `start`, `end`, `slices`: izlenen süre, ilk dilimin başından son dilimin sonuna. Logda kayıt
  yoksa `start` ve `end` `null`'dır.
- `count` bütün artışların, `omitted` `--ilk` yüzünden yazılmayanların sayısıdır. `rows`,
  tablo satırının alanlarına ek olarak kalıbın olağan seviyesini (`usual`), `mad`'ını ve
  artışın kendisini (`spike`) taşır: dilim sınırları, dilim ve kayıt sayısı, artıştaki
  kayıtların en yüksek seviyesi, ilk ve son kayıt, ilk kaydın mesajı (`sample`), zirve dilim
  ve puanı (`peak`). Satırın kendi `level`, `first`, `last` ve `sample` alanları bütün
  kalıbındır; artışta yalnızca WARNING ve üstündeki kayıtlar sayıldığı için ikisi farklı
  olabilir.

### jq ile

[jq](https://jqlang.org) ile birkaç kullanım:

```bash
# Tablonun en önemli satırı
veylog --bicim json --hepsi laravel.log | jq '.patterns.rows[0]'

# Hatalar ve uygulama kodundaki yerleri, sekmeyle ayrılmış
veylog --bicim json --hepsi laravel.log | jq -r '.patterns.rows[]
  | select(.level == "CRITICAL" or .level == "ERROR")
  | "\(.count)\t\(.exception.file // "-")\t\(.exception.method // "-")\t\(.pattern)"'

# CI adımı: logda CRITICAL varsa başarısız ol (jq -e, sonuç false ise 1 koduyla çıkar)
veylog --bicim json laravel.log | jq -e '.levels.CRITICAL // 0 | . == 0'

# CI adımı: logda ani artış varsa başarısız ol
veylog --bicim json --ani-artis 5 laravel.log | jq -e '.spikes.count == 0'
```

### Eski alan adları

veylog 0.2'ye kadar JSON'un alan adları, sabit değerleri ve yer tutucuları Türkçeydi. Eski
adlarla yazılmış bir betiği güncellemek için:

| Yer | Eski ad | Yeni ad |
|---|---|---|
| en üst | `surum`, `dosya`, `satir_sayisi`, `kayit_sayisi` | `version`, `file`, `lines`, `records` |
| en üst | `devam_satiri_sayisi`, `yetim_satir_sayisi` | `continuation_lines`, `orphan_lines` |
| en üst | `zaman_araligi` (`en_eski`, `en_yeni`) | `time_range` (`oldest`, `newest`) |
| en üst | `seviyeler`, `kaliplar`, `filtre` | `levels`, `patterns`, `filter` |
| en üst | `saat_dilimi`, `karsilastirma`, `ani_artislar` | `time_zone`, `comparison`, `spikes` |
| `time_zone` | `dilim`, `kaynak` (`secenek`, `log`) | `zone`, `source` (`option`, `log`) |
| `time_zone` | `cevrilen_kayit`, `cevrilen_dilimler`, `dilimsiz_kayit` | `converted_records`, `converted_from`, `records_without_zone` |
| `filter` | `en_dusuk_seviye`, `baslangic`, `bitis`, `son_saniye`, `kayit_sayisi` | `min_level`, `since`, `until`, `last_seconds`, `records` |
| `patterns` | `ham_mesaj_sayisi`, `ham_mesaj_yaklasik`, `maskeli_mesaj_sayisi` | `distinct_messages`, `distinct_messages_estimated`, `masked_messages` |
| `patterns` | `kalip_sayisi`, `grup_sayisi`, `esik` | `drain_patterns`, `groups`, `threshold` |
| `patterns` | `konum` (`metot`, `satir`) | `location` (`method`, `line`) |
| `patterns` | `siralama` (`onem`, `adet`, `yeni`) | `sort` (`severity`, `count`, `newest`) |
| her bölüm | `gosterilmeyen`, `satirlar` | `omitted`, `rows` |
| satır | `adet`, `seviye`, `ilk`, `son`, `kalip`, `ornek` | `count`, `level`, `first`, `last`, `pattern`, `sample` |
| satır | `degiskenler` (`konum`, `degerler`, `deger`, `adet`, `baska`) | `variables` (`position`, `values`, `value`, `count`, `others`) |
| satır | `istisna` (`sinif`, `dosya`, `satir`, `metot`) | `exception` (`class`, `file`, `line`, `method`) |
| `comparison` | `nokta`, `once`, `sonra` | `point`, `before`, `after` |
| `comparison` | `once` ve `sonra` içinde `baslangic`, `bitis`, `saniye`, `kayit_sayisi` | `start`, `end`, `seconds`, `records` |
| `comparison` | `artis_kati`, `anlamlilik`, `en_az_kayit` | `min_factor`, `significance`, `min_records` |
| `comparison` | `yeni`, `kaybolan`, `artan`; içlerinde `kalip_sayisi` | `new`, `gone`, `increased`; içlerinde `count` |
| `comparison` satırı | `once`, `sonra`, `kat`, `p_degeri` | `before`, `after`, `factor`, `p_value` |
| `spikes` | `dilim_dakika`, `en_dusuk_seviye`, `baslangic`, `bitis` | `slice_minutes`, `min_level`, `start`, `end` |
| `spikes` | `dilim_sayisi`, `en_az_dilim`, `esik`, `en_az_mad`, `artis_sayisi` | `slices`, `min_slices`, `threshold`, `min_mad`, `count` |
| `spikes` satırı | `olagan`, `artis` | `usual`, `spike` |
| `spike` | `baslangic`, `bitis`, `dilim_sayisi`, `kayit_sayisi` | `start`, `end`, `slices`, `records` |
| `spike` | `seviye`, `ilk`, `son`, `ornek`, `zirve` | `level`, `first`, `last`, `sample`, `peak` |
| `peak` | `baslangic`, `bitis`, `adet`, `puan` | `start`, `end`, `count`, `score` |
| kalıp | `<SAYI>`, `<YOL>`, `<EPOSTA>`, `<ZAMAN>` | `<NUM>`, `<PATH>`, `<EMAIL>`, `<TIME>` |
| `file` | `<standart girdi>` | `<standard input>` |

### Yazıcı

JSON'u yazmak için kütüphane kullanılmadı. Rust'ta alışılmış yol
[serde](https://serde.rs)'nin `#[derive(Serialize)]` özelliğidir, ama programın
bağımlılıklarını 5 paketten 15'e çıkarırdı; yalnızca serde_json bile 9'a. Kurallar kısa
([RFC 8259](https://www.rfc-editor.org/rfc/rfc8259)):

- JSON değeri bir `enum` ile modellenir (`Yok`, `Sayi`, `Ondalik`, `Metin`, `Dizi`, `Nesne`)
  ve özyinelemeli olarak yazılır. Nesnelerin alanları eklendikleri sırayla yazılır.
- Metinlerde `"` ve `\` kaçılır; kontrol karakterleri `\n`, `\t` gibi kısa biçimleriyle ya da
  `\u001b` olarak yazılır. RFC'nin zorunlu tuttuğu U+0000–U+001F'ye ek olarak DEL ve C1
  karakterleri de kaçılır; böylece JSON doğrudan terminale yazıldığında da güvenlidir. Türkçe
  harfler olduğu gibi kalır.
- NaN ve sonsuz JSON'da olmadığı için `null` yazılır.

Testler çıktıyı [serde_json](https://crates.io/crates/serde_json) ile ayrıştırarak denetler:
tırnak, ters bölü, ESC, emoji gibi zor metinler yazılıp geri okunduğunda aynısı çıkmalı.

## Markdown

`--bicim markdown` GitHub issue ve PR açıklamalarına yapıştırmak içindir:

```text
$ veylog --bicim markdown --ilk 2 ornekler/laravel.log
## veylog: `ornekler/laravel.log`

- **Satır:** 205 · **Kayıt:** 59 · **Devam satırı:** 143 · **Yetim satır:** 3
- **Zaman aralığı:** 2026-10-24 08:59:58 → 2026-10-24 18:31:07
- **Seviyeler:** CRITICAL 2 · ERROR 16 · WARNING 6 · NOTICE 1 · INFO 31 · DEBUG 3
- **Kalıplar:** 45 farklı mesaj → maskeleyince 27 → Drain ile 23 kalıp (eşik 0,7) → parmak iziyle 24 grup

| Adet | Seviye | İlk | Son | Kalıp | Örnek | Değerler | Konum |
|---:|---|---|---|---|---|---|---|
| 2 | CRITICAL | 17:05:13 | 17:06:45 | `Redis bağlantısı kurulamadı: Connection refused [tcp://<IP>:<SAYI>]` | `Redis bağlantısı kurulamadı: Connection refused [tcp://10.0.3.12:6379]` |  | `app/Http/Middleware/HizSiniri.php:22, handle()` RedisException |
| 6 | ERROR | 16:10:02 | 16:16:02 | `cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> by…` | `cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes receiv…` |  | `app/Services/OdemeServisi.php:64, tahsilEt()` ConnectionException |

… 22 satır daha; tümünü görmek için `--hepsi`
```

Karşılaştırmanın her bölümü ayrı bir tablodur ve artanların "Sonra" sütununda kat da yazar:
`5 (15,4 kat)`. Ani artışlar da ayrı bir tablodur; `Zirve` hücresine zirve dilimin aralığı
yazılır: `6 (16:10–16:20)`. `Değerler` sütunu kalıptaki `<*>` konumlarının en sık değerlerini
taşır, her değer kod olarak: `` `giriş` ×8, `çıkış` ×1 ``; birden çok `<*>` varsa konum
sırasıyla (`` 2: `ahmet` ×4; 4: `giriş` ×5 ``). Kalıpta `<*>` yoksa hücre boştur.

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
karakterleri bu yüzden önce sembollere çevrilir
([terminal güvenliği](nasil-calisir.md#terminal-güvenliği)).
