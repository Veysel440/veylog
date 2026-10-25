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

- Log dosyasındaki satırları ve kayıtları sayar. Stack trace gibi devam satırları kayıt sayılmaz;
  yalnızca `[2026-10-24 14:03:11] production.ERROR: ...` biçimindeki başlıklar sayılır.
- Kayıtları seviyelerine göre (DEBUG → EMERGENCY) gruplar ve en önemlisi en üstte olacak şekilde listeler.

```text
$ veylog ornekler/laravel.log
ornekler/laravel.log: 203 satır, 59 kayıt
  CRITICAL     2
  ERROR       16
  WARNING      6
  NOTICE       1
  INFO        31
  DEBUG        3
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
- [ ] Kodu modüllere ayırma
- [ ] Çok satırlı kayıtları (stack trace) kayda bağlama, zaman aralığını gösterme
- [ ] Değişken kısımları maskeleme (sayı, IP, e-posta, UUID → `<SAYI>`, `<IP>`...)
- [ ] Drain algoritmasıyla tekrar eden kalıpları bulma
- [ ] JSON ve Markdown çıktısı
- [ ] İki zaman aralığını karşılaştırma: yeni çıkan, kaybolan ve artan hatalar
- [ ] Ani hata artışlarını yakalama (medyan + MAD)