# Karşılaştırma ve ani artışlar

veylog bir logda iki soruya istatistikle cevap verir: "Dağıtımdan sonra ne değişti?"
(`--karsilastir`) ve "Hangi hata bir anda patladı?" (`--ani-artis`). Bu belge iki yöntemi,
eşiklerin neden öyle seçildiğini ve nasıl sınandıklarını anlatır. Kalıpların nasıl
çıkarıldığı [nasıl çalışır](nasil-calisir.md) belgesinde.

## Zaman aralıklarını karşılaştırma

Log bir anda ikiye bölünür: `--karsilastir 15:00`. Önce yarısı ilk kayıttan o ana kadar, sonra
yarısı o andan son kayda kadar sürer; tam o andaki kayıt sonraya sayılır. Kalıplar bütün log
üzerinde bir kez çıkarılır, sonra her tablo satırının kayıtları iki yarıya sayılır ve satır
üç bölümden birine girer:

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
Artanlar (sonra ≥ 2 kat sık, tesadüf ihtimali < %1): 1
  önce  sonra  seviye     ilk       son       kalıp
     1      5  ERROR      16:10:02  16:16:02  cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> by…
                                              artış: saatte 0,1 → 2,1 (15,4 kat)
                                              örnek: cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes…
                                              konum: app/Services/OdemeServisi.php:64 (ConnectionException)
```

`--ilk` her bölüme ayrı uygulanır; `--siralama` bölümlerin içindeki sırayı belirler.

### Sıklık saat başına sayılır

İki yarı farklı uzunlukta olabilir: 6 saatte 3 kez ile 1 saatte 3 kez aynı şey değildir. Bu
yüzden kayıt sayıları değil, saatlik sıklıklar karşılaştırılır:

```text
kat = (b / Ts) / (a / Tö)      a, b: öncedeki ve sonradaki kayıt sayısı
                               Tö, Ts: iki yarının süresi
```

Süreler saniye hassasiyetindedir; son kaydın saniyesi de sonra yarısına sayılır
(Ts = son − nokta + 1 sn). Seyrek bir logda saatlik sıklık 0,05'in altına düşebilir; o zaman
iki anlamlı basamakla yazılır (`saatte 0,048`).

### Tesadüf mü?

16:11'de "Ödeme sağlayıcısı yanıt vermedi" kalıbı da 1 kayıttan 2'ye çıkar ve saatlik sıklığı
6 kat artar. Ama toplam 3 kayıt tesadüfen de böyle dağılabilir. Bunu ayırt etmek için iki
Poisson sıklığını karşılaştıran koşullu binom testi kullanılır (Przyborowski ve Wilenski,
1940):

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
karşılaştırılarak test edilir; yaklaşık 5 800 rastgele durumda (n bir milyona kadar) en büyük
göreli fark 3,7 × 10⁻⁸'dir ve %1 eşiğinde hiçbir karar değişmez.

### Zaman

İki an arasındaki süre için tarih, 1970-01-01'den bu yana geçen güne çevrilir. Bunun için
Howard Hinnant'ın [`days_from_civil`](https://howardhinnant.github.io/date_algorithms.html)
algoritması kullanılır: yıl Mart'ta başlıyormuş gibi sayılır, böylece Şubat yılın son ayı
olur ve artık gün yılın sonuna düşer. `--karsilastir` şu yazılışları kabul eder:

- `15:00` ya da `15:00:30`: tarih logun son kaydından alınır.
- `2026-10-24`: günün başı.
- `"2026-10-24 15:00"` ya da `2026-10-24T15:00`.

Takvimde olmayan bir an (`2026-02-29`, `24:00`) reddedilir. Nokta ilk kayıtta ya da
öncesindeyse, ya da son kayıttan sonraysa yarılardan biri boş kalacağı için veylog hata
verir (çıkış kodu 2).

Benzer bir özellik Sumo Logic'in
[LogCompare](https://www.sumologic.com/help/docs/search/behavior-insights/logcompare/)'ıdır:
iki zaman aralığının kalıplarını karşılaştırır, yenileri "New", kaybolanları "Gone" diye
işaretler. Değişimi kalıbın toplam loglar içindeki payıyla ölçer; veylog ise saatlik sıklığı
kullanır, böylece "saatte 0,1 → 2,1" doğrudan okunur.

## Ani artışlar

Karşılaştırma için bakılacak anı bilmek gerekir. Ani artışlarda log tek bir anda bölünmez:
her kalıbın sıklığı dilim dilim izlenir (`--ani-artis 1` ile dakika dakika) ve olağan
seviyesinden sıçradığı anlar bulunur.

```text
$ veylog --ani-artis 10 --ilk 3 ornekler/laravel.log
...
Ani artışlar (WARNING ve üstü, 10 dakikalık 59 dilim, puan > 3,5): 1
  zirve  olağan  puan  seviye     ilk       son       kalıp
      6       0   4,0  ERROR      16:10:02  16:16:02  cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> by…
                                                      artış: 16:10–16:20 arası 6 kayıt
                                                      örnek: cURL error 28: Operation timed out after 10001 milliseconds with 0 bytes…
                                                      konum: app/Services/OdemeServisi.php:64 (ConnectionException)
```

Ödeme zaman aşımı günün geri kalanında hiç görülmüyor, 16:10–16:20 diliminde ise 6 kez
görülüyor. `zirve` en çok kaydın düştüğü dilimdeki kayıt sayısı, `olağan` kalıbın dilim
başına olağan kayıt sayısı, `puan` da sıçramanın büyüklüğüdür. `seviye`, `ilk`, `son` ve
`örnek` burada kalıbın değil artışın kayıtlarındandır: artışın en yüksek seviyesi, ilk ve son
kaydı ve ilk kaydının mesajı.

### Dilimler

Logun kapsadığı süre eşit uzunlukta dilimlere bölünür. Bir kaydın dilimi, 1970-01-01
00:00'dan bu yana geçen saniyenin dilim uzunluğuna bölümüdür (aşağı yuvarlanır). Bu yüzden
dilimler ilk kayda göre değil saate göre hizalanır: 60'ı bölen uzunluklarda (1, 5, 10, 15,
30...) her saat başında bir dilim başlar; 10 dakikalık dilimler 16:00, 16:10, 16:20... diye
gider. Dilimler ilk kaydın diliminden son kaydın dilimine kadar uzanır ve kaydı olmayan
dilimler de 0 olarak sayılır. Süreyi bütün kayıtlar (INFO dahil) belirler, çünkü bir hatanın
saatlerce görülmemesi de onun olağanının parçasıdır.

### Olağan seviye: medyan ve MAD

Her kalıp için her dilimdeki kayıt sayısı bulunur. Kalıbın olağan seviyesi bu sayıların
medyanı, olağan dalgalanması ise
[MAD](https://en.wikipedia.org/wiki/Median_absolute_deviation)'ıdır: sayıların medyandan
mutlak sapmalarının medyanı. Ortalama ve standart sapma tek bir patlamayla kayardı; medyan ve
MAD ise dilimlerin yarısına kadarı patlama olsa bile yerinde kalır. Böylece aranan patlama
kendi ölçüsünü bozmaz.

Ayda birkaç kez görülen bir hatanın 43 200 dakikalık dizisi neredeyse tamamen sıfırdır. Bu
yüzden sıfırlar tek tek tutulmaz, yalnızca sayılır. Sıralı dizide sıfırlar baştadır: k'inci
eleman, k sıfır sayısından küçükse 0'dır; değilse kaydı olan dilimlerin sıralı sayılarından
`sayilar[k − sıfır sayısı]` elemanıdır. MAD'da her sıfırın sapması medyanın kendisidir; bu
değerin sıralı sapmalar arasındaki yeri ikili aramayla bulunur. Bellek, kaydı olan dilim
sayısı kadardır.

### Puan

Her dilim, Iglewicz ve Hoaglin'in (1993) değiştirilmiş z puanını alır:

```text
puan = 0,6745 × (sayı − medyan) / MAD        puan > 3,5 → sıçrama
```

0,6745 standart normal dağılımın üçüncü çeyreğidir. Normal dağılımda MAD ≈ 0,6745 σ olduğu
için puan, sayının olağandan kaç standart sapma uzakta olduğunun dayanıklı bir tahminidir.
3,5 eşiği de Iglewicz ve Hoaglin'in önerisidir;
[NIST'in istatistik el kitabı](https://itl.nist.gov/div898/handbook/eda/section3/eda35h.htm)
da bu eşiği verir.

Seyrek bir hatanın dilimlerinin yarısından fazlası boştur; MAD'ı 0 çıkar ve tek bir kayıt
bile sonsuz puan alırdı. Sayılar tam sayı olduğu için bir kayıttan küçük bir dalgalanma
zaten ölçülemez, bu yüzden MAD 1'den küçükse 1 sayılır. Sonuç olarak normalde görülmeyen bir
hata bir dilimde **en az 6 kayıtla** sıçrar: 0,6745 × 6 = 4,05, oysa 5 kayıt 3,37'de kalır.

Bunun yerine puanın paydasını (MAD / 0,6745) en az 1 yapmak da düşünülebilirdi; o zaman 4
kayıt yeterdi. Poisson dağılımıyla hesaplanınca, dakikada ortalama 0,69 kayıt yazan
(medyanı hâlâ 0 olan) bir kalıpta bu yol 1 dakikalık dilimlerle günde yaklaşık 8 yanlış
sıçrama verir, seçilen yol 0,12. Bedeli, küçük patlamaların kaçabilmesidir: dakikada 0,01
sıklıkta görülen bir hataya 5 dakika boyunca dakikada ortalama k kayıt eklenen benzetimde
patlamaların k = 4 iken %72'si, k = 5 iken %91'i, k = 6 iken %97'si, k ≥ 8 iken hepsi
yakalandı.

### Çift eşik

Bir ani artış, sayısı olağanın (medyanın) üstünde kalan art arda dilimlerin tamamıdır; yeter
ki en az biri sıçramış olsun. Artışı sıçrama başlatır, olağana dönüş bitirir. Görüntü
işlemedeki Canny kenar bulucusunun iki eşiği (histerezis) de bu fikre dayanır. Aşağıdaki
gerçekçi logda ödeme sağlayıcısı 15 dakika yanıt vermedi; zaman aşımlarının dakikalık
sayıları şöyleydi (sıçrayanlar, yani 6 ve üstü, işaretli):

```text
14:xx      20  21  22  23  24  25  26  27  28  29  30  31  32  33  34
kayıt       4   5   2   3   6   4   3   6   8   4   4   1   2   2   5
sıçrama                     ●           ●   ●
```

Yalnızca sıçrayan dakikalar birleştirilseydi arıza "14:24" ve "14:27–14:29" diye iki parçaya
bölünürdü. Çift eşikle tek satır çıkar: `14:20–14:35 arası 15 dilimde 59 kayıt, zirve
14:28–14:29`. Artışın puanı, zirve dilimin puanıdır.

### Yalnızca WARNING ve üstü

INFO ve DEBUG kayıtları çoğu zaman trafikle birlikte gün içinde inip çıkar ve tek bir olağan
seviye bunu açıklayamaz. Gerçekçi bir logda bu sınır kaldırılınca 14 yanlış artış daha çıktı.
Hepsi yoğun saatlerdeki "Kullanıcı giriş yaptı" ve "Sipariş oluşturuldu" dakikalarıydı; biri
gerçek ödeme arızasının bile önüne geçti.

Seviyeye kalıp satırının değil her kaydın kendisine bakılır. Drain tek kelimesi farklı iki
mesajı birleştirebilir: `Kullanıcı 7 giriş yaptı` (INFO) ile `Kullanıcı 7 giriş yapamadı`
(WARNING) `Kullanıcı <SAYI> giriş <*>` kalıbında buluşur ve satırın seviyesi WARNING olur.
Satırın kayıtlarının hepsi sayılsaydı INFO trafiğinin her yoğun anı bir WARNING artışı gibi
görünür, gerçek bir WARNING patlaması da INFO'ların yükselttiği olağan seviyenin altında
kalabilirdi.

### Gerçekçi bir günde

Yöntem, Laravel 13.38 ile üretilen bir günlük log üzerinde denendi. Log uydurma bir
e-ticaret gününü anlatır; kayıtları Laravel'in kendisi yazar ve istisnalar gerçekten
fırlatılır (HTTP zaman aşımı, PDO bağlantı hatası, phpredis, PHP uyarısı). Trafik gece düşük,
gündüz yüksektir; toplam 3 821 kayıt.

| Olay | `--ani-artis 1` | `--ani-artis 5` |
|---|---|---|
| 03:15 veritabanı yük devri: 40 saniyede 40 hata | ✓ puan 27,0 | ✓ |
| 14:20–14:35 ödeme zaman aşımları, dakikada ~5 hata | ✓ tek parça, 15 dilimde 59 kayıt | ✓ 3 dilimde 59 kayıt |
| Aynı arızanın uyarıları (WARNING), dakikada ~4 | kısmen: 14:30–14:35, 55 kaydın 25'i | ✓ 55 kaydın tamamı |
| 19:42 Redis çöküşü: 2 dakikada 39 CRITICAL | ✓ 2 dilimde 39 kayıt | ✓ |
| 16:00 dağıtımından sonra başlayan kargo hatası, dakikada ~0,3 | yok | yok |
| Saat başı stok senkronizasyonu uyarıları (2–4) ve seyrek hatalar | yok | yok |

İki dilim uzunluğunda da yanlış alarm çıkmadı. Uyarılar dakikalık dilimlerde kısmen bulundu:
ilk dokuz dakikada hiçbir dakika 6 kayda ulaşmadı ve 14:29'da hiç kayıt yoktu, çift eşik
orada koptu. Kargo hatası bir sıçrama değil, kalıcı bir seviye değişimidir; onu
`--karsilastir 16:00` yeni çıkanlar arasında gösterir.

Geliştirme sırasında yöntem numpy ve scipy ile yazılmış bağımsız bir hesapla karşılaştırıldı:
40 rastgele logda beş dilim uzunluğuyla bulunan 1 635 artışın ve gerçekçi loglardaki
artışların hepsi aynı çıktı. Hesap hızlıdır: 30 günlük, 92 MB'lık bir logda (43 195 dakikalık
dilim) `--ani-artis 1` ile çalıştırma yaklaşık bir saniyede biter.

### Dilim uzunluğu

Dilim, aranan olayın süresine yakın seçilmelidir. Örnek logdaki altı zaman aşımı altı ayrı
dakikaya düşer; dakikalık dilimlerde (`--ani-artis 1`) hiçbir dakika olağanın dışına çıkmaz.
10 dakikalık dilimde ise altısı bir aradadır. Gerçekçi logdaki dört olayın hepsini 5 ve 10
dakikalık dilimler eksiksiz bulur. Dilim uzadıkça yavaş değişimler de artış gibi görünür: 15
dakikalık dilimlerde kargo hatasının seviye değişimi, 60 dakikalık dilimlerde ayrıca gündüz
saatlerinde sıklaşan bir hata listeye girer. Kısa arızalar için 1–10 dakika iyi bir
başlangıçtır.

### Sıra ve sınırlar

- Puanı en yüksek artış başta gelir; puanlar eşitse önce başlayan. `--ilk` bu bölüme de
  uygulanır.
- Olağan seviyeyi ölçmek için log en az 10 dilim sürmelidir. Daha kısaysa artış aranmaz ve
  bölüm bunu söyler; bu bir hata değildir, çıkış kodu 0'dır. Örnek logda `--ani-artis 120`:
  `Ani artışlar (WARNING ve üstü, 120 dakikalık 6 dilim): olağan seviyeyi ölçmek için en az 10 dilim gerekir`
- Dilim 1 ile 1440 dakika (bir gün) arasında olabilir; bunun dışındaki bir değer kullanım
  hatasıdır (çıkış kodu 2).

Dilimlerin başı ve sonu yazılırken saniyeden tarihe geri dönülür. Bunun için yine Hinnant'ın
algoritmalarından, bu kez ters yöndeki
[`civil_from_days`](https://howardhinnant.github.io/date_algorithms.html) kullanılır. Test,
dört basamaklı yılların 3,6 milyon gününün her birini gidip geri dönerek denetler.

Twitter, bulut altyapısındaki ölçümlerde anormallik bulmak için benzer bir yöntem kullandı
([Vallis ve ark., 2014](https://usenix.org/system/files/conference/hotcloud14/hotcloud14-vallis.pdf);
açık kaynak [AnomalyDetection](https://github.com/twitter/AnomalyDetection) paketi). O da
ortalama ve standart sapma yerine medyan ve MAD kullanır, ama önce günlük ve haftalık döngüyü
seriden çıkarır. veylog döngüyü çıkarmaz; bu yüzden yalnızca WARNING ve üstüne bakar ve uzun
dilimlerde dikkat ister ([sınırlamalar](sinirlamalar.md)).

İki bölümün JSON ve Markdown çıktısı: [çıktı biçimleri](cikti-bicimleri.md).
