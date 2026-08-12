Bu dosya Agy'nin uzun süreli hafızasıdır. Agy, projede yapılan önemli değişiklikleri, kurulan mimariyi, kullanılan kütüphaneleri ve kullanıcının özel isteklerini her büyük değişiklikten sonra buraya not etmelidir. Agy her yeni oturumda çalışmaya başlamadan önce ilk olarak bu dosyayı okumalıdır.

## Mimari Kurallar ve Sınırlar
- **Maksimum Satır Sınırı**: Projedeki hiçbir kod dosyası (özellikle `.rs` dosyaları) **600-700 satırı** geçmemelidir.
- **Ultra Modüler Yapı**: Tüm modüller (özellikle protokol implementasyonları ve orchestrator bileşenleri) olabildiğince küçük, işlevsel ve izole parçalara bölünmelidir.

## Proje Gelişim Geçmişi ve Kayıtlar

### [12.08.2026] Projenin Temelleri ve Protokol Yapısı Kuruldu
- **Yapılan İşlem**: Proje Cargo ile Rust projesi olarak başlatıldı (`openbrute`).
- **Kurulan Mimari**: `BruteTarget` adında bir async trait tanımlandı. Bu trait, protokol motorlarını ana zamanlayıcıdan (orchestrator) izole eder. `Orchestrator` ise Tokio kanalları (`mpsc`), `Semaphore` ve asenkron görev havuzu ile eş zamanlılığı, hata yönetimini ve opsiyonel rate limiting işlevlerini yönetir.
- **Desteklenen Protokoller**:
  - **HTTP/HTTPS**: GET/POST; Basic Auth, HTML Form Auth ve JSON API Auth modları eklendi.
  - **SSH**: `russh` kütüphanesi ile asenkron şifre denemesi eklendi.
  - **FTP**: `suppaftp` ile asenkron bağlantı ve kimlik doğrulama eklendi.
  - **SMTP**: `lettre` kütüphanesinin async transport `test_connection` (NOOP tabanlı) yeteneği ile hızlı auth testi eklendi.
  - **SQL**: `sqlx` kullanılarak MySQL ve PostgreSQL sunucularına asenkron bağlantı testi eklendi.
- **Kullanılan Başlıca Kütüphaneler**:
  - `tokio` (runtime)
  - `clap` (CLI argüman ayrıştırma)
  - `reqwest` (HTTP)
  - `russh` & `russh-keys` (SSH)
  - `suppaftp` (FTP)
  - `lettre` (SMTP)
  - `sqlx` (MySQL/PostgreSQL)
  - `thiserror` & `anyhow` (Hata yönetimi)

### [12.08.2026] Cilalama ve GitHub Üzerinde Yayınlama
- **Yapılan İşlem**: 
  - Gelişmiş bir `.gitignore` dosyası oluşturuldu.
  - Apache License 2.0 lisans dosyası (`LICENSE`) eklendi.
  - Görsellerle zenginleştirilmiş, örnek komutlar içeren detaylı bir `README.md` hazırlandı.
  - Proje git ile commit edildi ve `gh` (GitHub CLI) kullanılarak [Koray-Ozt/openBrute](https://github.com/Koray-Ozt/openBrute) deposu oluşturulup kodlar başarıyla GitHub'a yüklendi.
