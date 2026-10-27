# Çıktı biçimleri

Özet bir kez hesaplanır, sonra istenen biçimde yazılır; bu yüzden üç biçimdeki sayılar hep
aynıdır. `--ilk` ve `--siralama` her biçimde geçerlidir.

| Biçim | Seçenek | Ne için |
|---|---|---|
| Metin | varsayılan | Terminalde okumak |
| JSON | `--bicim json` | Betikler, `jq`, CI adımları |
| Markdown | `--bicim markdown` | GitHub issue ve PR açıklamaları |

## JSON

Örnek logun yalnızca ilk satırıyla (`--ilk 1`):

<!-- denetim: veylog --bicim json --ilk 1 ornekler/laravel.log -->
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
    "ham_mesaj_yaklasik": false,
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
- `dosya` logun yoludur; standart girdiden okunduysa `<standart girdi>`.
- `istisna`, kayıtlar bir istisnadan gelmiyorsa `null`'dır. İstisnanın uygulama kodunda yeri
  bulunamadıysa `dosya` ve `satir` `null`'dır. View hatalarında `dosya` şablondur ve `satir`
  `null`'dır.
- `seviyeler` yalnızca logda geçen seviyeleri, en önemlisi başta olacak şekilde içerir.
- `ham_mesaj_yaklasik`: `ham_mesaj_sayisi` bir tahmin mi? 262 144'ten fazla farklı ham mesaj
  varsa sayı HyperLogLog ile tahmin edilir (±%0,4,
  [ayrıntılar](nasil-calisir.md#akışlı-okuma-ve-bellek)); öteki bütün sayılar tamdır.
- `satirlar` en fazla `--ilk` kadar satırdır; `gosterilmeyen` dışarıda kalanların sayısıdır.
  Bütün satırlar için `--hepsi`.

### Karşılaştırma

`karsilastirma` alanı yalnızca `--karsilastir` verilince eklenir; normal raporun biçimi
değişmez.

<!-- denetim: veylog --bicim json --karsilastir 16:11 ornekler/laravel.log | jq '.karsilastirma.artan.satirlar[] | {kalip, once, sonra, kat, p_degeri}' -->
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

### Ani artışlar

`ani_artislar` alanı yalnızca `--ani-artis` verilince eklenir.

<!-- denetim: veylog --bicim json --ani-artis 10 ornekler/laravel.log | jq '.ani_artislar.satirlar[] | {kalip, olagan, mad, artis}' -->
```bash
veylog --bicim json --ani-artis 10 ornekler/laravel.log | jq '.ani_artislar.satirlar[] | {kalip, olagan, mad, artis}'
```

```json
{
  "kalip": "cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> bytes received (see <URL>) for <URL>",
  "olagan": 0,
  "mad": 0,
  "artis": {
    "baslangic": "2026-10-24 16:10:00",
    "bitis": "2026-10-24 16:20:00",
    "dilim_sayisi": 1,
    "kayit_sayisi": 6,
    "seviye": "ERROR",
    "ilk": "2026-10-24 16:10:02",
    "son": "2026-10-24 16:16:02",
    "ornek": "cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes received (see https://curl.haxx.se/libcurl/c/libcurl-errors.html) for https://api.odeme.test/v1/tahsilat",
    "zirve": {
      "baslangic": "2026-10-24 16:10:00",
      "bitis": "2026-10-24 16:20:00",
      "adet": 6,
      "puan": 4.047
    }
  }
}
```

- `dilim_dakika`, `en_dusuk_seviye`, `en_az_dilim`, `esik`, `en_az_mad`: ayarlar ve eşikler.
- `baslangic`, `bitis`, `dilim_sayisi`: izlenen süre, ilk dilimin başından son dilimin sonuna.
  Logda kayıt yoksa `baslangic` ve `bitis` `null`'dır.
- `artis_sayisi` bütün artışların, `gosterilmeyen` `--ilk` yüzünden yazılmayanların sayısıdır.
  `satirlar`, tablo satırının alanlarına ek olarak kalıbın olağan seviyesini (`olagan`),
  `mad`'ını ve artışın kendisini (`artis`) taşır: dilim sınırları, dilim ve kayıt sayısı,
  artıştaki kayıtların en yüksek seviyesi, ilk ve son kayıt, ilk kaydın mesajı (`ornek`), zirve
  dilim ve puanı. Satırın kendi `seviye`, `ilk`, `son` ve `ornek` alanları bütün kalıbındır;
  artışta yalnızca WARNING ve üstündeki kayıtlar sayıldığı için ikisi farklı olabilir.

### jq ile

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

# CI adımı: logda ani artış varsa başarısız ol
veylog --bicim json --ani-artis 5 laravel.log | jq -e '.ani_artislar.artis_sayisi == 0'
```

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

Karşılaştırmanın her bölümü ayrı bir tablodur ve artanların "Sonra" sütununda kat da yazar:
`5 (15,4 kat)`. Ani artışlar da ayrı bir tablodur; `Zirve` hücresine zirve dilimin aralığı
yazılır: `6 (16:10–16:20)`.

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
