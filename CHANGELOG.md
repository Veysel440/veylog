# Değişiklikler

Bu dosya [Keep a Changelog](https://keepachangelog.com/tr-TR/1.1.0/) biçimini izler; sürüm
numaraları [Anlamsal Sürümleme](https://semver.org/lang/tr/) kurallarına uyar.

## [Yayımlanmamış]

### Bellek ve hız

- Log satır satır, akış halinde okunur; dosyanın tamamı belleğe alınmaz. Bellek dosyanın
  boyuyla değil farklı mesajların sayısı ve uzunluğuyla büyür: 2 GB'lık bir log 4 GB yerine
  27 MB ile, 500 MB'lık bir log 976 MB yerine 16 MB ile özetlenir.
- Okuma hızlandı: 2 GB'lık log 36 yerine 15 saniyede, 500 MB'lık log 8 yerine 3,8 saniyede
  özetlenir. Geçerli UTF-8 satırlar hızlı yoldan denetlenir; maskeleme ve Drain aşağıdaki
  değişikliklerle hızlandı.
- Kayıtların zamanları ve alt grupları sıkıştırılmış sütunlarda tutulur (delta, zigzag ve
  LEB128 kodlaması): kayıt başına 2–3 bayt.
- Kayıtlar okunurken, logdaki sırasıyla kümelenir. Bir mesajın tekrarında ağacın o uzunluktaki
  kısmı değişmediyse Drain'in önceki sonucu kullanılır (sürümlü önbellek); sonuç her kaydı
  aramayla kümelemekle aynıdır. Drain ikinci tura girerse kayıtlar sütunlardan yeniden
  kümelenir; girdi yeniden okunmaz.
- Drain'in kalıpları belirteçlerini ayrı ayrı saklamaz: kümeyi açan maskeli mesaj depoyla
  paylaşılır, belirteç başına 8 bayt tutulur. Her biri başka bir toplu SQL sorgusu taşıyan
  1 500 istisnalık 34 MB'lık bir log 421 MB yerine 208 MB ile okunur.
- Ani artışın örneği gerekirse kayıt dosyadan yeniden okunur (seyrek dizin). Okunan kaydın
  maskeli mesajı ve seviyesi denetlenir: dosya bu arada döndürüldüyse yanlış bir kayıt örnek
  olmaz, uyarı yazılır.
- Borudan gelen standart girdi, yeniden okunması gerekiyorsa (`--ani-artis`, logun sonuna göre
  çözülen bir filtre) okunurken geçici bir dosyaya kopyalanır; dosya hemen silinir, diskte
  kalmaz, Unix'te yalnızca sahibi okuyabilir. Dosyaya yönlendirilmiş standart girdi
  (`veylog - < laravel.log`) kopyalanmaz, dosyanın kendisi yeniden okunur. Kopya yalnızca ani
  artışların örnekleri içinse ve oluşturulamaz ya da yazılamazsa (disk dolu) bir uyarı
  yazılır, rapor yine çıkar.
- 262 144'ten fazla farklı ham mesaj varsa sayısı HyperLogLog ile tahmin edilir (64 KB,
  ±%0,4, Ertl'in sapmasız tahmincisi). Metinde sayının önünde `≈` yazar; JSON'a
  `ham_mesaj_yaklasik` alanı eklendi. Tahmin, tam sayımın bıraktığı sayının altına inmez.
- Dev satırlara ve kayıtlara karşı sınırlar: bir satırın en fazla 256 MB'ı okunur; bir başlık
  satırının ve bir JSON kaydının mesajının ilk 1 MB'ı, bir mesajın ilk 64 KB'ı kullanılır;
  bir kaydın devam satırlarının en fazla 256 MB'ı ve bir milyon satırı saklanır. JSON
  satırlarında yalnızca kaydın gerektirdiği alanlar (`message`, `level_name`, `datetime`,
  bağlamdaki `exception`) kurulur. Satır sonu olmayan 300 MB'lık bir dosya 262 MB, 100 MB'lık
  tek bir başlık satırı 102 MB, bağlamında dev bir dizi taşıyan 50 MB'lık bir JSON satırı
  52 MB bellekle okunur.

### Filtreler

- `--seviye warning`: yalnızca bu seviye ve üstündeki kayıtlar.
- `--baslangic` ve `--bitis`: yalnızca bir zaman aralığındaki kayıtlar (başlangıç dahil, bitiş
  hariç). Yalnızca saat verilirse logun son günü.
- `--son 2sa`: logun son kısmı, en yeni kayıttan geriye. Süre birimleri: `sn`, `dk`, `sa`,
  `g` (ya da `s`, `m`, `h`, `d`); birleştirilebilir: `1sa30dk`.
- Filtreler kalıpları, seviyeleri, zaman aralığını, karşılaştırmayı ve ani artışları süzer;
  dosyanın satır ve kayıt sayıları değişmez. Metin ve Markdown'da `Filtre` satırı, JSON'da
  `filtre` nesnesi yalnızca filtre verilince yazılır. Filtreye uymayan kayıtlar maskelenmez ve
  kümelenmez: 500 MB'lık bir logun son iki saati 1,8 saniyede özetlenir.
- `--son` ve yalnızca saat verilmiş sınırlar logun sonunu bilmeyi gerektirir: log önce bir
  kez taranır, borudan gelen standart girdi taranırken geçici bir dosyaya kopyalanır. Logda
  hiç kayıt yoksa bu sınırlar çözülmez; JSON'da çözülmeyen sınır `null`dur.
- Başlangıç bitişten önce değilse filtrenin aralığı boştur: kullanım hatası, çıkış kodu 2.

### Saat dilimleri

- Kayıt zamanlarındaki saat dilimi (`+03:00`, `Z`) hesaba katılır: bütün zamanlar tek bir
  dilimde, raporun saat diliminde karşılaştırılır ve gösterilir. Yaz saatine geçişi kapsayan ya
  da saat dilimi değiştirilmiş bir uygulamanın logu doğru sıralanır; süreler, karşılaştırma ve
  ani artışlar kaymaz.
- `--saat-dilimi +03:00`: raporun saat dilimi. Verilmezse logda saat dilimi yazılı ilk
  kaydınkidir; saat dilimi yazılmamış kayıtlar bu dilimde sayılır. Filtrelerin ve
  `--karsilastir`'ın zamanları da bu dilimdedir.
- Metin ve Markdown çıktısında `Saat dilimi` satırı: raporun dilimi, kaç kaydın başka bir
  dilimden çevrildiği, kaçında dilim yazılmadığı. JSON'da `saat_dilimi` nesnesi; logda dilim
  yazılmamışsa ve verilmediyse `null`.
- Zamanlar her yerde tek biçimde yazılır: `2026-10-24 14:03:11`. 0.1.0 zaman aralığını ve
  JSON'daki kalıp zamanlarını logdaki yazılışıyla veriyordu (`2026-10-24T14:03:11.123456+03:00`).
- Takvimde olmayan bir saat dilimi (`+25:00`) taşıyan satır kayıt başlığı sayılmaz.

### Log biçimleri

- `Log::error($e)` ile metne çevrilmiş istisnalar okunur: PHP'nin istisna metninden
  (`RuntimeException: ... in dosya:satır`, `Stack trace:`, `Next`) sınıf, fırlatıldığı yer ve
  yığın izi. Kaydın mesajı istisnanın mesajıdır; aynı hata `report($e)` ile loglanmış olsa da
  aynı satırda birleşir. Önceki istisnalar `Next` ile yazıldığında logu yazdıran, en dıştaki
  istisna alınır. Açıklamalı yazılış da tanınır: `Log::error('Ödeme başarısız: ' . $e)`.
- `--desen`: kendi satır biçimi. Monolog'un biçim dizgesi (`config/logging.php`'deki
  `format`) düzenli ifadeye çevrilir; adlandırılmış gruplu bir düzenli ifade de verilebilir
  (`zaman`, `mesaj`; isteğe bağlı `seviye`, `baglam`, `kanal`). JSON satırları her biçimde
  okunur.
- Desenle okunan seviyelerde başka araçların adları ve kısaltmaları da tanınır: `WARN`, `ERR`,
  `FATAL`, `TRACE`; Serilog'un `VRB`, `DBG`, `INF`, `WRN`, `FTL`'si; .NET'in `fail`, `dbug`'ı;
  Java'nın `SEVERE`, `FINE`'ı. Bu adların en az üç harflik tek anlamlı başları (Monolog'un
  `setMaxLevelNameLength` ile kısalttığı `ERRO`, `NOTI`) ve Monolog'un sayıları (`%level%`)
  da okunur. Desene ve zamana uyduğu halde seviyesi tanınmayan satırlar sayılır ve uyarılır.
- `--zaman-bicimi`: zamanın biçimi, PHP'nin `date()` dizgesiyle (Monolog'un `dateFormat`'ı):
  `d.m.Y H:i:s`, `d/M/Y:H:i:s O`, `U`. Laravel'in satır biçiminde de geçerlidir.
- Satırlardan hiçbiri bir kayıt başlatmıyorsa bir uyarı yazılır.
- Sınıf adları ve yollar okunurken JSON kaçışlarından çözülür: Windows'ta aynı hatanın
  bağlamdaki (`\\`) ve istisna metnindeki (`\`) yazılışı aynı parmak izini verir.
- Ani artışın örneği yeniden okunurken kayıt devam satırlarıyla birlikte toplanır: istisna
  metninin mesajı devam satırlarındadır.

### Satır numarasından bağımsız parmak izi

- Bir hatanın yeri dosya ve satırın içinde bulunduğu fonksiyondur:
  `app/Services/SepetHesaplayici.php:88, toplam()`. Bir dağıtım dosyanın üstüne satır eklese
  de aynı hata tablonun aynı satırında kalır; `--karsilastir` onu kaybolan ve yeni çıkan
  saymaz. Fonksiyon, yığın izinde bir sonraki satırda çağrılan fonksiyondur. PHP'nin bir
  uyarısından doğan `ErrorException`'da fırlatıldığı yer yığın izinde de görünür; fonksiyonu o
  satırınkidir.
- Kimliğe fonksiyonun sınıfsız adı katılır; dosya zaten kimliktedir. PHP trait'teki bir
  metodu onu kullanan sınıfın adıyla yazar (`App\Models\Urun->kaydet()`,
  `App\Models\Siparis->kaydet()`): trait'teki bir hata yine tek satırdır.
- Gösterilen satır o hatanın en yeni kaydınınkidir: kod değiştiyse güncel satır.
- Closure'lar tanımlandıkları fonksiyonla ayrılır: `{closure:olustur()}`. Fonksiyon PHP
  8.4'ün closure adından (`{closure:App\Services\SiparisServisi::olustur():12}`) okunur,
  öncesinde yığın izinden bulunur: closure'dan dışa doğru aynı dosyadaki ilk satırın
  fonksiyonu. Rota dosyalarındaki closure'lar `{closure}` diye yazılır.
- Bir dosyanın en üst düzeyindeki kod (dahil edilen dosya, çalıştırılan betik) bir
  fonksiyonun içinde değildir; konumu satırla belirlenir.
- Çağrıları yazılmayan kayıtların (Monolog'un JSON yığın izi, kapalı yığın izi) fonksiyonu
  logun öteki yığın izlerinden öğrenilir: aynı hatanın JSON ve metin kayıtları birleşir. İki
  farklı fonksiyonda görülen satır kullanılmaz.
- `--konum satir`: dosya ve satır, 0.1'deki gibi.
- JSON çıktısında `istisna.metot` (fonksiyonun tam adı) ve `kaliplar.konum` alanları;
  Markdown'ın konum hücresinde fonksiyon.

### Kategorik konumlar

- Koddan gelen değerler biçimlerinden tanınır: ad alanlı sınıf adları
  (`App\Jobs\FaturaGonder`), büyük harfli sabitler (`ORDER_CREATED`), deveAdı ve yilan_adi
  tanımlayıcılar, PHP değişkenleri (`$urun`), noktalı ya da iki noktalı adlar
  (`odeme.basarisiz`, `kurlar:guncelle`), tek ya da çift tırnaklı tek kelimeler (`'fiyat'`).
  İki mesaj yalnızca aynı biçimdeki iki kodsal değerde ayrılıyorsa fark, rakamlı farklar gibi
  eşiği gevşetir: `İş tamamlandı: App\Jobs\FaturaGonder` ile `İş tamamlandı:
  App\Jobs\EpostaGonder` 100 farklı iş beklenmeden tek kalıpta birleşir. Sıradan kelimeler
  (`Ödeme alındı`, `Ödeme reddedildi`; kuyruk adları, durum kelimeleri) eskisi gibi ayrı
  kalır, 100 farklı değer sınırı onlar için sürer. Kodsal biçimli baş belirteçler, rakamlılar
  gibi ağacın ortak `<*>` dalına gider.
- Raporda her `<*>` konumunun en sık beş değeri adetleriyle yazılır, kalanı `+N başka`:
  metinde kalıbın altındaki `değerler:` satırı (sığmayan değerler alt satırlara taşar),
  Markdown'da `Değerler` sütunu, JSON'da her satırın `degiskenler` dizisi (`konum`,
  `degerler`, `baska`). Değerler rapor zamanında depodaki maskeli mesajlardan sayılır; ek
  bellek gerekmez. `Kullanıcı <SAYI> <*> yaptı` gibi sözdiziminden birleşen kalıpların
  değerleri de böylece görünür.
- Yeni örnek log `ornekler/kuyruk.log`: iş sınıfları, olay adları ve durum sabitleri.

### Canlı izleme

- `--izle`: log sonuna kadar okunur ve rapor yazılır; sonra eklenen kayıtlar okunmaya devam
  eder, yeni kayıt geldikçe rapor yeniden çizilir. Girdi yeniden okunmaz: depo okuma
  durumunu korur, her çizim yalnızca yeni baytları işler; Drain değişken bir konum bulursa
  kayıtlar o çizimde yeniden kümelenir. Bir durum satırı vardır: dosya, aralık, kayıt sayısı,
  en yeni kaydın zamanı.
- Terminalde her çizimde ekran ve kaydırma geçmişi temizlenir; durum satırı ve uyarılar
  raporun altındadır, rapor ekrandan uzun olsa da görünür. Windows'un eski konsolunda ekranı
  temizleyen kaçış dizileri program tarafından açılır. Bir dosyaya ya da boruya yazılıyorsa
  çizimler bir ayırıcı satırla art arda yazılır.
- `--aralik 500ms`: girdiye ne sıklıkla bakılacağı ve raporun en sık ne kadarda bir
  çizileceği (varsayılan 2 sn; 100 ms ile 1 sa arası). Süre sözdizimine `ms` birimi eklendi.
- Okunan son kayıt, ondan sonra bir kayıt başlığı gelince ya da girdi bir aralık boyunca sessiz
  kalınca depoya alınır: yığın izi birkaç yazma işlemiyle gelen bir kayıt bölünmez. Satır
  ortasında kesilen okumalar da bekletilir; sessizlikte satır sonu gelmemiş satır da okunur.
  Böylece sessiz bir anda çizilen rapor, o ana kadar gelen baytların bir kerede
  özetlenmesiyle aynıdır (değişken konumu çok olan loglarda kalıplar nadiren farklı olabilir:
  konumlar parça parça öğrenilir). Bir süredir değişmeyen bir dosyanın son kaydı ilk çizimde
  görünür.
- Döndürülen log (logrotate, `copytruncate`) fark edilir ve baştan okunur; önceki kayıtlar
  raporda kalır, durum satırına `döndürüldü` yazılır. Eski dosya önce sonuna kadar okunur:
  döndürülmeden hemen önce yazılan kayıtlar kaybolmaz. Seyrek dizin kayıtların sıra
  numaralarını da tutar; yeni dosyanın kayıtları yeni dosyadaki konumlarıyla yeniden okunur.
- Zaman filtreleri izlemede kayan penceredir: `--son 1sa` her çizimde o ana kadar okunan en
  yeni kayda göre uygulanır; en yeni kayıt, toplu okumadaki gibi seviye filtresine uymayanlarla
  birlikte bulunur. Log ilerledikçe aralık boşalırsa pencerede kayıt kalmaz, izleme sürer.
  Pencerenin dışında ilk kaydı kalan alt grupların örnekleri çizimler arasında saklanır.
- Standart girdi izlenebilir (`tail -f laravel.log | veylog --izle -`): girdi bitince son
  rapor yazılır ve çıkılır. `--ani-artis` örnekleri için tutulan geçici kopya 256 MB'ı aşınca
  bırakılır.
- `--izle` yalnızca metin biçimindedir; `--bicim json`, `--bicim markdown` ve `--karsilastir`
  ile kullanım hatası verir.

### Arayüz dili

- `--dil en` (`--lang en`) ya da `VEYLOG_DIL=en`: yardım, metin raporu, Markdown, uyarılar
  ve hata mesajları İngilizce. İngilizcede ondalık ayracı nokta (`0.7`), yüzde işareti sonda
  (`1%`), süreler `2 h 30 min`; sayıya göre tekil ya da çoğul (`1 record`, `3 records`).
  Kalıplardaki yer tutucular İngilizce gösterilir: `<NUM>`, `<PATH>`, `<EMAIL>`, `<TIME>`.
- Her uzun seçeneğin İngilizce adı her dilde geçerli: `--threshold`, `--location`, `--top`,
  `--all`, `--sort`, `--format`, `--compare`, `--spikes`, `--level`, `--since`, `--until`,
  `--last`, `--timezone`, `--pattern`, `--time-format`, `--watch`, `--interval`, `--lang`.
  Değerlerin de: `severity`, `count`, `newest`, `text`, `method`, `line`. Yazım hatası
  önerileri iki dilin adlarından seçilir.
- Dil argümanlar ayrıştırılmadan önce seçilir: `--lang`'dan önce gelen bir yanlışlığın mesajı
  da seçilen dildedir. Tanınmayan bir `VEYLOG_DIL` değeri iki dilde uyarılır.
- JSON çıktısı her dilde bayt bayt aynıdır: alan adları, değerler ve yer tutucular değişmez;
  standart girdiden okunduysa `dosya` her dilde `<standart girdi>`dir.
- Türkçe yardım metninin başında İngilizce yardımın nasıl açılacağı yazar.

### Komut satırı

- Değerlerde büyük ve küçük harf fark etmez: `--bicim JSON`, `--siralama ADET`,
  `--sıralama ÖNEM`, `--konum SATIR`.
- Kısa seçeneklerin değeri `=` ile de verilebilir: `-n=20`.
- Tek tireyle yazılan uzun seçeneklere (`-hepsi`) ve büyük-küçük harfi karışan kısa
  seçeneklere (`-v`) öneri verilir: `--hepsi`, `-V`.

### Düzeltmeler

- Mesaj bağlamdan, bağlamın geçerli bir JSON olarak başladığı yerden ayrılır: mesajın
  içindeki satır içi bir JSON (`Ödeme {"tutar":50} onaylandı`) mesajda kalır. 0.1.0 mesajı ilk
  ` {"`'de keserdi: JSON'dan sonraki kısım bağlama gider, farklı olaylar tek kalıpta
  birleşirdi.
- Mesajın sonundaki boş bağlam ve ek (`Kuyruk dolu [] []`; `formatter_with` ile kurulan
  Monolog biçimi yazar) mesaja katılmaz: aynı mesaj bağlamlı ve bağlamsız haliyle aynı
  kalıptadır.
- Maskeleme: URL'nin ardındaki ters tırnak URL'ye katılmaz (Guzzle'ın
  `` `POST https://...` resulted in ``'i); uzantıdan sonra süren bir yol (`/api/v1.2/urunler`)
  dosya yolu sayılmaz; alan adı harfle başlayan bir parçayla bitmeyen `ad@sürüm`
  (`laravel/framework@11.9.2`) e-posta sayılmaz.
- Maskelemenin kelime sınırları ve rakamları ASCII'dir: Türkçe mesajlı 400 bin kayıtlık bir
  log 1,7 yerine 0,7 saniyede okunur. Fark yalnızca ASCII olmayan bir harfe yapışık
  rakamlardadır: `Sipariş5`'teki rakam artık maskelenir.
- Drain'de bir yaprak 1 000 kümeye ulaşınca hiçbir kümeye benzemeyen yeni mesajlar yaprağın
  taşma kümesinde toplanır. Her konumu az değer alan ama birleşimleri sayısız olan mesajlar
  (`<ad> <eylem> <nesne>`) her yeni mesajı bütün kümelerle karşılaştırtır, süre mesaj
  sayısının karesiyle artardı: 40 000 böyle satır 33 yerine 1,7 saniyede okunur.
- Fırlatıldığı yer, yolunda parantez (`C:\Program Files (x86)\...`) ya da ` at ` olan
  projelerde de doğru okunur; 0.1.0 ilk `)`'de keser, konumu çağıran satıra kaydırırdı.
- Proje `vendor` adlı bir klasörün altındaysa (`/home/ali/vendor/magaza`) kök doğru bulunur:
  yollardaki her vendor bir aday sayılır, kök altında giriş noktası ya da uygulama kodu olan
  adaydır. 0.1.0 ilk vendor'dan önceki kısmı kök sayar, hiçbir hatanın konumunu bulamazdı.
- Kökü kendi yollarında olmayan istisnaların kökü, logun bütün köklerini dolaşmadan bulunur:
  binlerce kökü (sıfır kesintili dağıtımların klasörleri) ve binlerce böyle istisnası olan
  bir log 15 saniye yerine 0,3 saniyede okunur.
- Karşılaştırmada logun ilk ve son kaydı teste katılmaz: yarıların sınırını onlar belirler,
  rastgele düşmezler. Son kaydı taşıyan bir kalıp, sıklığı değişmediği halde %1 yerine
  %2–%9 ihtimalle artan sayılabiliyordu.

### Diğer

- Örnek logdaki Redis hatasının yığın izine middleware'i çağıran satır eklendi.
- Yardım metninde 92 karakteri aşan satırlar bölündü.
- README'ye Windows PowerShell 5.1'in yönlendirilen çıktıdaki Türkçe harfleri bozduğu notu
  eklendi.

## [0.1.0] - 2026-10-26

İlk sürüm.

### Okuma

- Laravel'in metin biçimindeki kayıtları ve Monolog'un JSON satırları; iki biçim aynı
  dosyada karışık olabilir.
- Laravel'in zaman biçimi ve ISO 8601 (kesir ve saat dilimiyle). Takvimde olmayan zamanlar
  (`2026-02-30`) kayıt başlığı sayılmaz.
- Stack trace gibi devam satırları, dosyanın başındaki yetim satırlar, geçersiz UTF-8 baytları
  ve CRLF satır sonları. UTF-8 BOM her satırın başında atılır; birleştirilmiş dosyalarda
  (`cat a.log b.log | veylog -`) da kayıt kaybolmaz.
- `-` ile standart girdi: `zcat laravel-2026-10-24.log.gz | veylog -`

### Kalıplar

- Değişken kısımların maskelenmesi: URL, dosya yolu, e-posta, UUID, zaman, IP, sayı.
- Drain algoritmasıyla kalıp çıkarma; eşik, iki tarafında da rakam olan farklılıklarda ve
  100'den fazla değer almış konumlarda gevşer. Hiçbir belirteci tutmayan ve tek kelimelik
  mesajları birbirine katan birleşmeler yapılmaz; mesajda yazılı `<*>` sıradan bir kelimedir.
  Esnek eşik kapalıyken sonuç Drain3 ile aynıdır.
- Hata parmak izi: istisna sınıfı ve uygulama kodundaki ilk satır. Vendor, giriş
  noktaları, derlenmiş şablonlar ve middleware'in `$next($request)` satırı atlanır; Blade
  hatalarında konum şablonun kendisidir. Proje kökü sıfır kesintili dağıtımlarda (paylaşılan
  `storage`) ve Vapor'da da doğru bulunur. Linux, macOS ve Windows (Herd, Laragon) yolları.
- Önem, adet ya da ilk görülme zamanına göre sıralanan rapor.

### Analiz

- `--karsilastir`: logu bir anda ikiye bölüp yeni çıkan, kaybolan ve artan kalıpları bulur;
  artış koşullu binom testiyle sınanır.
- `--ani-artis`: kalıpların WARNING ve üstündeki kayıtlarının olağan seviyesinden sıçradığı
  anları medyan, MAD ve değiştirilmiş z puanıyla bulur. Artışın seviyesi, örneği, ilk ve son
  zamanı artışın kendi kayıtlarındandır.

### Çıktı

- Terminal için tablo, betikler için JSON, GitHub için Markdown.
- Linux (statik), macOS ve Windows için derlenmiş programlar; Windows'ta ek bir çalışma zamanı
  gerekmez. Rust kuruluysa crates.io'dan: `cargo install veylog`.
- Logdaki kontrol karakterleri terminale ulaşmaz.
- Türkçe komut satırı: virgüllü ondalık sayılar, Türkçe karakterli seçenek adları, yazım
  hatalarına öneri, ayırt edilebilir çıkış kodları.

[Yayımlanmamış]: https://github.com/Veysel440/veylog/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Veysel440/veylog/releases/tag/v0.1.0
