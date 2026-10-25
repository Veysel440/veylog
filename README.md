# veylog

Laravel log dosyalarını okuyup özetleyen, Rust ile yazılmış bir komut satırı aracı.

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
- Laravel'in tarih biçimini (`2026-10-24 14:03:11`) ve Monolog'un varsayılanı olan ISO 8601
  biçimini (`2026-10-24T14:03:11.123456+03:00`) tanır.
- Dosya bir kaydın ortasından başlıyorsa, hiçbir kayda ait olmayan bu ilk satırları
  "yetim satır" olarak ayrıca sayar.
- Logun kapsadığı zaman aralığını gösterir.
- Kayıtları seviyelerine göre (DEBUG → EMERGENCY) sayar; en önemlisi en üstte.
- Mesajlardaki değişken kısımları (URL, e-posta, UUID, zaman, IP, sayı) yer tutucularla
  maskeler: `Kullanıcı 381 giriş yaptı` → `Kullanıcı <SAYI> giriş yaptı`.
- Maskelenmiş mesajları [Drain](#nasıl-çalışıyor) algoritmasıyla kalıplara ayırır ve en sık
  tekrar eden kalıpları listeler: `Zamanlanmış görev başladı: kurlar:guncelle` ile
  `Zamanlanmış görev başladı: yedek:al` → `Zamanlanmış görev başladı: <*>`.
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

Kalıplar: 45 farklı mesaj → maskeleyince 27 → Drain ile 23 kalıp
     9  Kullanıcı <SAYI> <*> yaptı
     6  Sipariş <SAYI> oluşturuldu
     6  Sipariş <SAYI> için ödeme alındı
     6  cURL error <SAYI>: Operation timed out after <SAYI> milliseconds with <SAYI> bytes receiv…
     4  Undefined array key "kargo_ucreti"
     3  Sepet önbellekten okundu
     3  SQLSTATE[<SAYI>]: Integrity constraint violation: <SAYI> Duplicate entry '<EPOSTA>' for k…
     3  Ödeme sağlayıcısı yanıt vermedi, sipariş <SAYI> için yeniden denenecek (deneme <SAYI>/<SA…
     2  Zamanlanmış görev başladı: <*>
     2  Attempt to read property "ad" on null
```

## Nasıl çalışıyor?

```text
laravel.log → kayıtlara ayır → maskele → Drain → kalıplar
              59 kayıt          27 farklı   23 kalıp
              45 farklı mesaj   mesaj
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
| Benzerlik eşiği | 0,7 | Bir mesajın kümeye katılması için gereken en düşük benzerlik. |
| En fazla çocuk | 100 | Bir düğümün en fazla dal sayısı; dolunca yeni belirteçler `<*>` dalına gider. |

Eşiğin örnek logdaki etkisi:

| Eşik | Kalıp | Etkisi |
|---|---|---|
| 0,4 | 22 | İki farklı yavaş SQL sorgusu anlamsız tek bir kalıpta birleşir. |
| **0,7** | 23 | Sorgular ayrı kalır; zamanlanmış görevler birleşir. |
| 0,8 | 25 | `giriş` ve `çıkış` ayrılır, ama zamanlanmış görevler de ayrılır. |

Yanlışlıkla birleştirilen yeni bir hata eski bir kalıbın içinde kaybolacağı için varsayılan
eşik temkinli seçildi. Geliştirme sırasında aynı ayarlarla [Drain3](https://github.com/IBM/Drain3)
(IBM, Python) ile karşılaştırıldı; örnek logda ve rastgele üretilmiş mesajlarda aynı kümeleri
veriyor.

### Bilinen sınırlamalar

- Drain sözdizimine bakar, anlama bakmaz: `Kullanıcı <SAYI> giriş yaptı` ile
  `Kullanıcı <SAYI> çıkış yaptı` aynı yapıda olduğu için `Kullanıcı <SAYI> <*> yaptı`
  kalıbında birleşir.
- Kısa mesajlarda maskelenemeyen tek bir değişken bile eşiği geçmeyi engeller:
  `Yanıt süresi 120ms` ile `Yanıt süresi 95ms` (3 belirtecin 2'si aynı, 0,67 < 0,7) ayrı
  kalıplar olur.
- Boşluk içeren bir değer (ör. `Ali Veli`) belirteç sayısını değiştirdiği için aynı kalıptan
  gelen mesajlar farklı kümelere düşebilir.
- Stack trace henüz kullanılmıyor: aynı mesajla farklı yerlerde fırlatılan iki hata
  (ör. `Undefined array key "id"`) tek kalıpta birleşir.
- Aynı dosyada farklı zaman biçimleri ya da saat dilimleri karışırsa zaman aralığı yanlış
  sıralanabilir.
- Ayarlar henüz komut satırından değiştirilemiyor.
- Dosyanın tamamı belleğe okunuyor; çok büyük dosyalar için satır satır okuma ileride eklenecek.

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

# Sadece ilk 20 satır
./target/release/veylog /yol/storage/logs/laravel.log | head -20
```

## Proje yapısı

```text
src/
├── main.rs    programın girişi: dosyayı okur, özeti yazar
├── kayit.rs   log metnini kayıtlara ayırır: başlık, seviye, mesaj, devam satırları
├── maske.rs   mesajlardaki değişkenleri (sayı, IP, e-posta...) yer tutucularla değiştirir
└── drain.rs   Drain algoritması: maskelenmiş mesajlardan kalıp çıkarır
tests/
└── cli.rs     programı gerçekten çalıştıran testler: argümanlar, hata mesajları, çıkış kodları
```

Dış kütüphane olarak yalnızca [regex](https://crates.io/crates/regex) kullanılır.

## Testler

```bash
cargo test
```

İki tür test çalışır: `src` içindeki birim testleri her parçayı ayrı ayrı sınar;
`tests/cli.rs` ise derlenmiş programı çalıştırıp kullanıcının gördüğünü kontrol eder.

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
- [ ] Rakama duyarlı benzerlik eşiği: kısa mesajlardaki sayısal değişkenler
- [ ] Kalıp raporu: en yüksek seviye, ilk/son görülme zamanı, örnek mesaj
- [ ] Hata parmak izi: istisna sınıfı ve uygulamadaki ilk satır
- [ ] Komut satırı seçenekleri: benzerlik eşiği, gösterilecek kalıp sayısı
- [ ] JSON ve Markdown çıktısı
- [ ] İki zaman aralığını karşılaştırma: yeni çıkan, kaybolan ve artan hatalar
- [ ] Ani hata artışlarını yakalama (medyan + MAD)