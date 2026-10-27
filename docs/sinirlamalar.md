# Sınırlamalar

veylog'un bilinen sınırları. Her biri bir tasarım kararının bedelidir ya da henüz yapılmamış
bir iştir ([yol haritası](../README.md#yol-haritası)).

## Kalıplar

- Drain sözdizimine bakar, anlama bakmaz: `Kullanıcı <SAYI> giriş yaptı` ile
  `Kullanıcı <SAYI> çıkış yaptı` aynı yapıda olduğu için `Kullanıcı <SAYI> <*> yaptı`
  kalıbında birleşir.
- Kısa mesajlarda rakam içermeyen bir değişken, aynı konumda 100'den fazla farklı değer
  görülene kadar her değer için ayrı kalıp oluşturur (ör. `İşlendi: App\Jobs\FaturaGonder`
  ile `İşlendi: App\Jobs\EpostaGonder`). Bu sınırı aşan konumlar
  [değişken](nasil-calisir.md#değişken-konumlar) sayılır.
- Tek kelimelik mesajlarda değişken konum aranmaz: rakamsız olanlar yalnızca birebir aynıysa
  birleşir. Tek başına loglanan binlerce farklı kullanıcı adı (`Log::info($ad)`) binlerce
  satır olur.
- Boşluk içeren bir değer (ör. `Ali Veli`) belirteç sayısını değiştirdiği için aynı kalıptan
  gelen mesajlar farklı kümelere düşebilir. Parmak izi kalıpları birleştirmediği için bu
  kayıtlar aynı yerden gelseler de ayrı satırlarda kalır.
- Linux ve macOS'ta `/` ile başlayan bir yol ancak bir dosya uzantısıyla bitiyorsa maskelenir;
  `/var/www/magaza/storage` gibi uzantısız yollar maskelenmez. View hatalarının ekindeki yol
  dışında, boşluk içeren bir yolun (`C:\Program Files\...`) yalnızca ilk parçası maskelenir.
- ANSI renk kodları (`␛[31m`) rakam içerdiği için Drain onları değişken sayar; renk kodlu
  kısa mesajlar aynı uzunluktaki başka renk kodlu mesajlarla birleşebilir.

## Parmak izi

- Parmak izi satır numarasını içerir: bir dağıtımda dosyanın üst kısmı değişirse aynı hata
  iki ayrı satırda görünebilir. Sentry gibi araçlar bu yüzden satır numarası kullanmaz.
- Yalnızca Laravel'in istisna biçimi tanınır: yakalanmayan istisnalar, `report($e)` ve
  `Log::error('...', ['exception' => $e])`; JSON satırlarında da bağlamın `exception` alanı.
  İstisnayı metne çeviren `Log::error($e)` kullanımı (`Stack trace:` biçimi) tanınmaz.
- Blade hatalarında şablondaki satır bilinmez: hata derlenmiş şablonda oluşur ve onun satırı
  Blade dosyasındaki satıra karşılık gelmez. Konum yalnızca şablon dosyasıdır.
- Derlenmiş şablonlar Laravel'in varsayılan klasöründe (`storage/framework/views`) aranır;
  `VIEW_COMPILED_PATH` başka bir klasörü gösteriyorsa o klasördeki dosyalar uygulama kodu
  sayılabilir.
- JSON satırlarının yığın izinde çağrılar yazılmaz; middleware'in `$next($request)` satırı
  orada ayırt edilemez ve konum olarak seçilebilir.
- `JsonFormatter` yığın izini varsayılan olarak yazmaz. O zaman vendor'da fırlatılan
  istisnaların konumu bulunamaz. Uygulamada fırlatılanların kökü de ancak logun başka bir
  kaydında vendor yolu geçiyorsa bulunur; geçmiyorsa yol tam yazılır
  (`C:/Users/.../app/Models/Urun.php:12`) ve aynı hatanın metin biçimindeki kayıtlarıyla
  birleşmez. `formatter_with` ile `includeStacktraces` açılmalıdır.
- JSON satırlarında bağlamın yalnızca `exception` alanına bakılır. Uygulamanın kendi yazdığı
  istisna bilgisi (ör. bağlamdaki `class`, `file`, `line`) parmak izine katılmaz. Mesajı bir
  olay adı olan kayıtlar (`odeme.basarisiz`) olay adına göre gruplanır; ayrıntı bağlamdadır
  ve gösterilmez.

## Zaman ve okuma

- Saat dilimi yok sayılır: farklı saat dilimleriyle yazılmış kayıtlar (`+03:00` ve `+00:00`)
  aynı dosyada karışırsa zaman aralığı, sıralama, karşılaştırma ve ani artışlar kayabilir.
- JSON satırlarında yalnızca Monolog'un alan adları tanınır (`message`, `level_name`,
  `datetime`); başka bir JSON şeması (ör. Elastic'in ECS'i, `@timestamp`) kayıt sayılmaz.
- Bellek dosyanın boyuyla değil, farklı mesajların sayısıyla büyür
  ([akışlı okuma](nasil-calisir.md#akışlı-okuma-ve-bellek)). En kötü durum, maskelemenin hiçbir
  şeyini değiştiremediği, her mesajı başka olan bir logdur: her mesaj bir kez saklanır. Her
  satırında başka bir kullanıcı adı olan 40 bin satırlık bir log 24 MB tutar.
- 262 144'ten fazla farklı ham mesaj varsa "farklı mesaj" sayısı tahmindir (±%0,4); önünde
  `≈` yazar. Öteki bütün sayılar tamdır.
- `--ani-artis` ile standart girdiden okurken girdi geçici bir dosyaya kopyalanır: girdi kadar
  disk yeri gerekir. Dosya oluşturulur oluşturulmaz silinir; diskte kalmaz.
- Bir satırın 256 MB'tan uzun kısmı okunmaz; bir kaydın devam satırlarının 256 MB'tan sonrası
  saklanmaz ([akışlı okuma](nasil-calisir.md#akışlı-okuma-ve-bellek)). Gerçek loglarda
  satırlar bunun çok altındadır.
- Tek seferde bir log okunur. Birden çok dosya standart girdiden birleştirilebilir:
  `cat storage/logs/laravel-*.log | veylog -`.

## Karşılaştırma ve ani artışlar

- İkisi de kayıtları sayar. Trafik iki katına çıkarsa her kalıp iki kat sık görünür; veylog
  bunu yük artışından ayırt edemez.
- Her kalıp ayrı ayrı sınanır. %1 eşiği ve 2 kat şartı yanlış alarmı az tutar ama
  sıfırlamaz; çok kalıplı bir logda arada bir tesadüfi bir "artan" görülebilir.
- Karşılaştırmadaki test, kayıtların zamana rastgele serpildiğini varsayar. Zamanlanmış
  görevler gibi düzenli olaylar ve tek bir anda yığılan patlamalar bu varsayıma uymaz.
- Log tek bir noktada ikiye bölünür; iki ayrı dosya ya da iki keyfi aralık
  karşılaştırılamaz.
- Ani artışlarda olağan seviye bütün log için tektir; günlük döngü hesaba katılmaz. Gündüz
  sıklaşan bir kalıbın yoğun saatleri, özellikle uzun dilimlerde, artış gibi görünebilir. Bu
  yüzden yalnızca WARNING ve üstündeki kayıtlara bakılır; INFO ve DEBUG'daki artışlar
  aranmaz.
- Kalıcı bir seviye değişimi (bir dağıtımdan sonra sürekli görülmeye başlayan bir hata) kısa
  dilimlerde artış sayılmaz, uzun dilimlerde sayılır. Bunun için `--karsilastir` daha
  uygundur.
- Kayıtlarını toplu yazan kalıplar her seferinde artış sayılabilir: her çalıştığında 6–8
  uyarı yazan saatlik bir iş, dakikalık dilimlerde günde 24 artış verir. Medyan ve MAD
  düzenli tekrarı tanımaz.
- Normalde görülmeyen bir hata, bir dilimde en az 6 kaydı olunca artış sayılır; daha küçük
  patlamalar kaçabilir.
- Sayılar tam sayı olduğu için MAD da kaba adımlarla değişir ve dakikada birkaç kayıt yazan
  kalıplarda gerçek dalgalanmayı küçük gösterebilir. Benzetimde, dakikada ortalama 3–5 kayıt
  yazan düzenli bir kalıp 1 dakikalık dilimlerle günde 5–8 tesadüfi artış verdi; dakikada 1
  kayda kadar olan kalıplarda bu sayı 0,1 civarında kaldı.
- Dilimler saate göre hizalandığı için logun ilk ve son dilimi kısmi olabilir: log 16:07'de
  başlıyorsa 16:00–16:10 dilimi yalnızca 3 dakikayı kapsar ve oradaki bir artış daha az
  kayıtla görünür.
- Arka arkaya iki olay, aradaki dilimlerde sayı olağana inmezse tek bir artış olarak
  görünür.

## Terminal

- Yazı yönünü değiştiren Unicode karakterleri (ör. U+202E, "Trojan Source") temizlenmez.
  Kontrol karakteri değiller ve terminali yönetemezler, ama destekleyen terminallerde ve
  GitHub'da yazının görünüşünü karıştırabilirler.
