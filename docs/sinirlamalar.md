# Sınırlamalar

veylog'un bilinen sınırları. Her biri bir tasarım kararının bedelidir ya da henüz yapılmamış
bir iştir.

## Kalıplar

- Drain sözdizimine bakar, anlama bakmaz: `Kullanıcı <SAYI> giriş yaptı` ile
  `Kullanıcı <SAYI> çıkış yaptı` aynı yapıda olduğu için `Kullanıcı <SAYI> <*> yaptı`
  kalıbında birleşir. Raporun `değerler` satırı birleşenleri gösterir (`giriş ×8, çıkış ×1`).
- Kısa mesajlarda sıradan bir kelime olan değişkenler (kuyruk adı `emails`, durum kelimesi
  `pending`, alt çizgisiz büyük harfli `FAILED`) aynı konumda 100'den fazla farklı değer
  görülene kadar her değer için ayrı kalıp oluşturur (`Kuyruk durdu: emails` ile
  `Kuyruk durdu: default`). Bu sınırı aşan konumlar
  [değişken](nasil-calisir.md#değişken-konumlar) sayılır. Koddan geldiği biçiminden belli olan
  değerler (sınıf adları, sabitler, olay adları, tırnaklı anahtarlar) bu sınırı beklemez
  ([kategorik konumlar](nasil-calisir.md#kategorik-konumlar)).
- Aynı kodsal biçimdeki iki değer başka bir fark olsa da birleştirebilir: `syntax error,
  unexpected identifier "x", expecting "function"` ile `syntax error, unexpected token "}",
  expecting ";"` iki tırnaklı değerde ayrıldığı için eşik gevşer ve `identifier` ile `token`
  farkına rağmen `syntax error, unexpected <*> <*> expecting <*>` olur. Kayıtlar bir istisnadan
  geliyorsa parmak izi onları yine ayırır.
- SQL'in ters tırnaklı adları (`` `urunler` ``) kodsal sayılmaz; `'...'` ile tırnaklı değerler
  sayılır. Bir mesajda tırnak içinde yazılan sabit bir kelime (`durum 'hazır' oldu`) başka bir
  tırnaklı kelimeyle birleşebilir.
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
- Kelime sınırları ASCII'dir: ASCII olmayan bir harfe yapışık rakam (`Sipariş5`, `4üncü`)
  maskelenir, ASCII bir harfe yapışık olan (`oauth2`) maskelenmez.
- Mesajın kendisi ` {"` ile başlayan ve ardından kelime gelmeyen bir JSON içeriyorsa
  (`Yanıt: {"durum":"ok"}`) o JSON bağlam sayılır ve kalıba girmez.
- Bir Drain yaprağında 1 000 küme olunca ([değişken konumlar](nasil-calisir.md#değişken-konumlar))
  hiçbir kümeye benzemeyen yeni mesajlar tek bir taşma kümesinde toplanır; kalıbı çoğu konumda
  `<*>` olur.

## Parmak izi

- Konum, satırın içinde bulunduğu fonksiyondur
  ([satır değil fonksiyon](nasil-calisir.md#satır-değil-fonksiyon)): aynı fonksiyonda aynı
  istisna sınıfı ve aynı kalıpla oluşan iki hata tek satırda birleşir; gösterilen satır en yeni
  kaydınkidir. Satırları ayırmak için `--konum satir`.
- PHP 8.4'ten önce closure'ın tanımlandığı fonksiyon yığın izinden tahmin edilir: closure'dan
  dışa doğru aynı dosyadaki ilk satırın fonksiyonu. Closure bir değişkende saklanıp aynı
  dosyanın başka bir fonksiyonundan çağrıldıysa tahmin yanlış olabilir. Tanımlandığı satır
  yığın izinde hiç yoksa (ör. bir service provider'ın `boot()`'unda kaydedilip sonra çağrılan
  bir dinleyici) sahibi bilinmez. PHP 8.4 sahibi closure'ın adına yazdığı için bu ikisinde
  sürüm yükseltilince sahip değişir; hata bir kez yeni bir hata gibi görünür.
- Sahibi bilinmeyen closure'lar ve rota dosyalarındaki closure'lar
  (`Route::get('/x', function () {...})`) sınıf ve dosya başına tek bir `{closure}` sayılır:
  aynı istisna sınıfı ve aynı kalıpla oluşan hataları birleşir.
- Bir dosyanın en üst düzeyindeki kod (rota dosyası yüklenirken, çalıştırılan bir betikte)
  bir fonksiyonun içinde değildir; konumu satırla belirlenir ve dağıtımda satırı kayarsa
  başka bir hata gibi görünür.
- İstisna üç yazılışla tanınır: bağlamda (yakalanmayan istisnalar, `report($e)`,
  `Log::error('...', ['exception' => $e])`), JSON satırlarında bağlamın `exception` alanında
  ve PHP'nin istisna metninde (`Log::error($e)`;
  [ayrıntılar](nasil-calisir.md#php-istisna-metni)). Başka yazılışlar
  (`Log::error($e->getMessage() . "\n" . $e->getTraceAsString())`) tanınmaz.
- İstisna metni kaydın ilk satırında başlamalıdır: `Log::error("Hata:\n" . $e)` tanınmaz.
- İstisna metninde sınıf, ad alanı içeren ya da `Exception` veya `Error` ile biten addır. Ad
  alanı olmayan ve böyle bitmeyen bir sınıf (PHP'nin `SoapFault`'u) tanınmaz. Açıklamalı
  metinde açıklamanın kendisi böyle bir adla başlıyorsa (`Log::error('Error: ' . $e)`,
  `Log::error(static::class . ': ' . $e)`) sınıf o sayılır, asıl sınıf mesaja kalır.
- Blade hatalarında şablondaki satır bilinmez: hata derlenmiş şablonda oluşur ve onun satırı
  Blade dosyasındaki satıra karşılık gelmez. Konum yalnızca şablon dosyasıdır.
- Derlenmiş şablonlar Laravel'in varsayılan klasöründe (`storage/framework/views`) aranır;
  `VIEW_COMPILED_PATH` başka bir klasörü gösteriyorsa o klasördeki dosyalar uygulama kodu
  sayılabilir.
- JSON satırlarının yığın izinde çağrılar yazılmaz: middleware'in `$next($request)` satırı
  ayırt edilemez ve konum olarak seçilebilir. Satırın fonksiyonu da ancak logun başka bir
  kaydının yığın izinde geçiyorsa bilinir; geçmiyorsa konum satırla belirlenir ve bir
  dağıtımda satırı kayan hata başka bir hata gibi görünür.
- Filtre verildiyse (`--seviye`, `--son`...) proje kökleri ve satırların fonksiyonları
  yalnızca filtreye uyan kayıtlardan öğrenilir: filtrenin dışında kalan bir metin kaydı,
  filtreye uyan bir JSON kaydının fonksiyonunu bildiremez.
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

- Saat dilimi yazılmamış kayıtlar (Laravel'in varsayılan biçimi) raporun diliminde yazılmış
  sayılır ([saat dilimleri](nasil-calisir.md#saat-dilimleri)). Saat dilimi yazılı ve yazılmamış
  kayıtlar aynı logda karışıyorsa ve uygulamanın dilimi ilk yazılı dilimden farklıysa
  dilimsizlerin zamanı kayar; rapor kaç kaydın dilimsiz olduğunu söyler. `--saat-dilimi`
  dilimsiz bir logdaki zamanları değiştiremez.
- Saat dilimi adla (`Europe/Istanbul`) verilemez, farkla verilir (`+03:00`); yaz saati
  değişikliğini kayıtlardaki farklar zaten taşır.
- JSON satırlarında yalnızca Monolog'un alan adları tanınır (`message`, `level_name`,
  `datetime`); başka bir JSON şeması (ör. Elastic'in ECS'i, `@timestamp`) kayıt sayılmaz.
  `--desen` yalnızca metin satırları içindir.
- Bir logda tek bir metin biçimi okunur: `--desen` verilince Laravel'in biçimindeki satırlar
  kayıt başlatmaz. JSON satırları her biçimde okunur.
- `--zaman-bicimi` yalnızca metin satırlarına uygulanır; JSON satırlarının zamanı ISO 8601
  olmalıdır. Saat dilimi kısaltmaları (`CET`, `EST`) ve adları (`Europe/Istanbul`) farka
  çevrilmez; o kayıtlar dilimi yazılmamış sayılır.
- Bir desenle okumak daha yavaştır: her satır düzenli ifadeyle sınanır. 500 MB'lık bir log 3,8
  yerine 6,4 saniyede okunur.
- Bellek dosyanın boyuyla değil, farklı mesajların sayısı ve uzunluğuyla büyür
  ([akışlı okuma](nasil-calisir.md#akışlı-okuma-ve-bellek)). En kötü durum, maskelemenin hiçbir
  şeyini değiştiremediği, her mesajı başka olan bir logdur: her mesaj bir kez saklanır. Her
  satırında başka bir kullanıcı adı olan 40 bin satırlık bir log 23 MB tutar; her biri başka
  bir toplu SQL sorgusu taşıyan 1 500 istisnalık 34 MB'lık bir log 208 MB.
- 262 144'ten fazla farklı ham mesaj varsa "farklı mesaj" sayısı tahmindir (±%0,4); önünde
  `≈` yazar. Öteki bütün sayılar tamdır.
- `--ani-artis` ile borudan gelen standart girdi (`zcat ... | veylog -`) okunurken geçici bir
  dosyaya kopyalanır: girdi kadar disk yeri gerekir. Dosya oluşturulur oluşturulmaz silinir;
  diskte kalmaz. Dosyaya yönlendirilmiş standart girdi (`veylog - < laravel.log`) kopyalanmaz.
- Bir satırın 256 MB'tan uzun kısmı okunmaz; bir başlık satırının ve bir JSON kaydının
  mesajının ilk 1 MB'ı, bir mesajın ilk 64 KB'ı kullanılır; bir kaydın devam satırlarının
  256 MB'tan ve bir milyon satırdan sonrası saklanmaz
  ([akışlı okuma](nasil-calisir.md#akışlı-okuma-ve-bellek)). İlk 64 KB'ı aynı olan iki dev
  mesaj aynı mesaj sayılır. Gerçek loglarda satırlar bunun çok altındadır.
- Tek seferde bir log okunur. Birden çok dosya standart girdiden birleştirilebilir:
  `cat storage/logs/laravel-*.log | veylog -`.
- `--son` ve yalnızca saat verilmiş filtre sınırları için log iki kez okunur: önce en yeni
  kaydı bulmak için taranır. Borudan gelen standart girdi bu yüzden geçici bir dosyaya
  kopyalanır; girdi kadar disk yeri gerekir, yer yoksa rapor çıkmaz.
- Filtreler yalnızca seviyeye ve zamana göredir; mesaja, kanala ya da bağlama göre süzmek için
  JSON çıktısı `jq` ile süzülebilir.

## Canlı izleme

- `--izle` yalnızca metin biçiminde çalışır; `--karsilastir` ile kullanılamaz (izlenen logun
  "sonra" yarısı hiç bitmez).
- Okunan son kayıt, ondan sonra bir kayıt başlığı gelene ya da girdi bir aralık boyunca sessiz
  kalana kadar depoya alınmaz: raporda en geç bir aralık (`--aralik`, varsayılan 2 sn) sonra
  görünür. Yazan süreç bir kaydın ya da satırın ortasında bir aralıktan uzun duraklarsa
  kalanı ayrı satırlar olur; devam satırları yetim sayılır.
- Drain'in değişken konumları izlemede parça parça öğrenilir; her çizimde gerekiyorsa bir
  kümeleme turu daha yapılır. Log bir kerede okunurken en fazla iki tur olduğu için, değişken
  konumu çok olan bir logda izlemenin kalıpları bir kerede okumadakilerden nadiren farklı
  olabilir.
- `copytruncate` ile döndürülen bir dosyaya, kesildikten sonra bir aralıkta eski boyundan
  fazlası yazılırsa döndürme fark edilmez ve okumaya eski konumdan devam edilir (`tail -f`'in
  de sınırı). Dosyayı yeniden adlandıran döndürmede (logrotate'in varsayılanı) bu olmaz.
- `copytruncate`'te son okumayla kesme arasında yazılan kayıtlar okunmaz: kopyalanan dosyaya
  giderler, izlenen dosyadan silinirler.
- Dosyayı yeniden adlandıran döndürmede eski dosya, döndürme fark edilince bırakılır. Dosyayı
  açık tutan süreçler (`queue:work` gibi uzun yaşayan işçiler) yeniden başlayana kadar eski
  dosyaya yazar; bırakıldıktan sonra yazdıkları okunmaz. PHP-FPM her istekte dosyayı yeniden
  açtığı için onun kayıtları yeni dosyaya gider.
- Ağ üzerindeki bir dosya sisteminde (NFS, SMB) dosyanın boyu gecikmeyle güncellenebilir;
  okunandan kısa görünürse dosya kesilmiş sayılır ve baştan okunur, kayıtlar iki kez sayılır.
- Zaman filtreleri izlemede her çizimde yeniden uygulanır (kayan pencere); kalıplar, farklı
  mesaj sayısı ve saat dilimi sayıları ise o ana kadar okunan bütün kayıtlar içindir, yalnızca
  penceredekiler için değil. Pencere ve ani artışlar her çizimde kayıt sütunlarını dolaşır:
  milyonlarca kayıtlık bir izlemede bir çizim onlarca milisaniye sürer.
- Windows'ta döndürme dosya numarasıyla anlaşılır. Dosya numarası vermeyen bir dosya
  sisteminde (bazı ağ sürücüleri) oluşturulma zamanına ve boya bakılır: orada yeniden
  adlandırılan dosyanın yerine açılan yeni dosya eskisinin oluşturulma zamanını alabilir ve
  eskisinden kısa değilse fark edilmez. Unix'te düğüm numarasıyla kesin anlaşılır.
- Döndürmeden sonra eski dosyanın kayıtları raporda kalır ama örnekleri yeniden okunamaz;
  ikinci turda bölünen bir alt grubun ya da bir ani artışın örneği eski dosyadaysa, aynı
  kalıpta aynı seviyedeki ilk kaydın mesajı yazılır.
- Standart girdide (`tail -f ... | veylog --izle -`) `--ani-artis` örnekleri için girdi geçici
  bir dosyaya kopyalanır. Kopya oluşturulamazsa, yazılamazsa ya da 256 MB'ı aşarsa bırakılır;
  ondan sonraki örnekler depodakilerden seçilir. `/tmp` birçok dağıtımda bellekte olduğu
  için kopya o kadar bellek de tutabilir.
- Terminalde ekran ANSI kodlarıyla temizlenir. Windows'un eski konsolunda bu kodlar program
  tarafından açılır; açılamazsa çizimler art arda akar.
- Rapor ekrandan uzunsa başı yukarı kayar; durum satırı ve uyarılar en alttadır. Tablo
  `--ilk` ile kısaltılabilir.

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

## Arayüz dili

- Logun içeriği çevrilmez: Türkçe bir logun mesajları İngilizce raporda da Türkçedir.
- JSON'un alan adları ve yer tutucuları her dilde Türkçedir (`kaliplar`, `<SAYI>`); bilerek:
  JSON'u okuyan betikler dile bağlı olmasın.
- Yalnızca İngilizce ve Türkçe vardır; dil ortamın yerel ayarından (`LANG`) seçilmez, Türkçe
  ayarlı bir sistemde de varsayılan İngilizcedir. `VEYLOG_LANG=tr` (ya da `VEYLOG_DIL=tr`)
  kalıcı bir seçimdir.

## Terminal

- Yazı yönünü değiştiren Unicode karakterleri (ör. U+202E, "Trojan Source") temizlenmez.
  Kontrol karakteri değiller ve terminali yönetemezler, ama destekleyen terminallerde ve
  GitHub'da yazının görünüşünü karıştırabilirler.
