# Değişiklikler

Bu dosya [Keep a Changelog](https://keepachangelog.com/tr-TR/1.1.0/) biçimini izler; sürüm
numaraları [Anlamsal Sürümleme](https://semver.org/lang/tr/) kurallarına uyar.

## [Yayımlanmamış]

### Bellek

- Log satır satır, akış halinde okunur; dosyanın tamamı belleğe alınmaz. Bellek dosyanın
  boyuyla değil farklı mesajların sayısıyla büyür: 2 GB'lık bir log 4 GB yerine 30 MB ile,
  500 MB'lık bir log 976 MB yerine 17 MB ile özetlenir. Süre değişmedi.
- Kayıtların zamanları ve alt grupları sıkıştırılmış sütunlarda tutulur (delta, zigzag ve
  LEB128 kodlaması): kayıt başına 2–3 bayt.
- Kayıtlar okunurken, logdaki sırasıyla kümelenir. Bir mesajın tekrarında ağacın o uzunluktaki
  kısmı değişmediyse Drain'in önceki sonucu kullanılır (sürümlü önbellek); sonuç her kaydı
  aramayla kümelemekle aynıdır. Drain ikinci tura girerse kayıtlar sütunlardan yeniden
  kümelenir; girdi yeniden okunmaz.
- Ani artışın örneği gerekirse kayıt dosyadan yeniden okunur (seyrek dizin). Okunan kaydın
  maskeli mesajı ve seviyesi denetlenir: dosya bu arada döndürüldüyse yanlış bir kayıt örnek
  olmaz, uyarı yazılır. Standart girdi `--ani-artis` ile okunurken geçici bir dosyaya
  kopyalanır; dosya hemen silinir, diskte kalmaz, Unix'te yalnızca sahibi okuyabilir.
  Geçici dosya oluşturulamazsa rapor yine çıkar.
- 262 144'ten fazla farklı ham mesaj varsa sayısı HyperLogLog ile tahmin edilir (64 KB,
  ±%0,4, Ertl'in sapmasız tahmincisi). Metinde sayının önünde `≈` yazar; JSON'a
  `ham_mesaj_yaklasik` alanı eklendi.
- Bir satırın en fazla 256 MB'ı okunur, bir kaydın devam satırlarının en fazla 256 MB'ı
  saklanır: satır sonu olmayan dev bir dosya belleği dolduramaz.

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
