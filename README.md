# veylog

Laravel log dosyalarını okuyup özetleyen, Rust ile yazılmış bir komut satırı aracı.

> 🚧 Proje geliştirme aşamasında. Rust öğrenirken adım adım yazılıyor; aşağıdaki
> [Yol haritası](#yol-haritası) neyin hazır, neyin planlı olduğunu gösterir.

## Neden?

Bir Laravel uygulamasının `storage/logs/laravel.log` dosyası binlerce satıra ulaşabilir.
Aynı hata yüzlerce kez tekrarlanır, stack trace'ler satır sayısını şişirir ve gerçekten
önemli olan kayıtlar bu kalabalıkta kaybolur. veylog'un amacı bu dosyayı birkaç saniyede
okunabilir bir özete çevirmek: kaç kayıt var, hangi seviyede, hangi hatalar tekrar ediyor
ve ne zaman başladı.

## Şu an ne yapıyor?

- Log dosyasını kayıtlara ayırır: `[2026-10-24 14:03:11] production.ERROR: ...` biçimindeki
  başlık satırı yeni bir kayıt başlatır; stack trace gibi devam satırları bir sonraki başlığa
  kadar o kayda bağlanır.
- Dosya bir kaydın ortasından başlıyorsa, hiçbir kayda ait olmayan bu ilk satırları
  "yetim satır" olarak ayrıca sayar.
- Logun kapsadığı zaman aralığını gösterir.
- Kayıtları seviyelerine göre (DEBUG → EMERGENCY) sayar; en önemlisi en üstte.
- Mesajlardaki değişken kısımları (URL, e-posta, UUID, zaman, IP, sayı) yer tutucularla
  maskeler: `Kullanıcı 381 giriş yaptı` → `Kullanıcı <SAYI> giriş yaptı`. Böylece aynı türden
  mesajlar birlikte sayılır ve en sık tekrar edenler listelenir.

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

Mesajlar: 45 farklı mesaj, maskeleyince 27
     8  Kullanıcı <SAYI> giriş yaptı
     6  Sipariş <SAYI> için ödeme alındı
     6  Sipariş <SAYI> oluşturuldu
     6  cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> bytes receiv…
     4  Undefined array key "kargo_ucreti"
     3  Sepet önbellekten okundu
     3  Ödeme sağlayıcısı yanıt vermedi, sipariş <SAYI> için yeniden denenecek (deneme <SAYI>/<SA…
     2  Attempt to read property "ad" on null
     2  Redis bağlantısı kurulamadı: Connection refused [tcp://<IP>:<SAYI>]
     2  İade talebi oluşturuldu
```

## Kurulum

[Rust](https://rustup.rs) 1.85 veya daha yeni bir sürüm gerekir.

```bash
git clone https://github.com/Veysel440/veylog.git
cd veylog
cargo build --release
```

Derlenen program `target/release/veylog` konumunda oluşur.

## Kullanım

```bash
# Derlemeden doğrudan çalıştırmak için
cargo run -- ornekler/laravel.log

# Derlenmiş programla
./target/release/veylog /yol/storage/logs/laravel.log
```

## Proje yapısı

```text
src/
├── main.rs    programın girişi: dosyayı okur, özeti ekrana yazar
├── kayit.rs   log metnini kayıtlara ayırır: başlık, seviye, mesaj, devam satırları
└── maske.rs   mesajlardaki değişkenleri (sayı, IP, e-posta...) yer tutucularla değiştirir
```

Dış kütüphane olarak yalnızca [regex](https://crates.io/crates/regex) kullanılır.

## Testler

```bash
cargo test
```

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
- [ ] Drain algoritmasıyla tekrar eden kalıpları bulma
- [ ] JSON ve Markdown çıktısı
- [ ] İki zaman aralığını karşılaştırma: yeni çıkan, kaybolan ve artan hatalar
- [ ] Ani hata artışlarını yakalama (medyan + MAD)