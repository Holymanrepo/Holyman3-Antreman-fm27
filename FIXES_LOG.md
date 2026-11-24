# FM27 Trainer · Kalıcı Teknik Düzeltme ve Mimari Kayıt Defteri (FIXES_LOG)

Bu dosya, yapılan bellek, taktik, kadro ve kurulum düzeltmelerinin sonraki güncellemelerde asla ezilmemesi veya tekrar aynı hataların yaşanmaması için kalıcı referans kaydıdır.

## 1. Taktik Değişimi ve İlk 11 Dizilişi (`agent/src/tactic.rs`, `app/src/live.rs`)
- **Sorun:** FM27 belleğinde kulübün `simatch::PREPARED_TACTICS_CREATOR_ARRAY` nesnesinin **4 ayrı kopyası** bulunur:
  1. `0x...dc4a8`: Canlı takım seçimi nesnesi (`blocks == None`, `+0x20` aktif taktik indeksi, `+0x28` aktif 26 kişilik kadro).
  2. `0x...dd678` (`+0x11d0` ofsetinde): İkincil takım seçimi kopyası.
  3. `0x...870e0`: Taktik ekranı / blok dizisi nesnesi #1 (`+0x08` 3 taktik bloğu vektörü, `+0x20` aktif taktik, `+0x28` 26 kişilik kadro).
  4. `0x...891e8` (`+0x2108` ofsetinde): Taktik ekranı / blok dizisi nesnesi #2.
- **Kalıcı Çözüm:**
  - `find_club_tactic_objects` ilkRTTI taramasında kulübümüze ait (`count_mine == Some`) tüm `PREPARED_TACTICS_CREATOR_ARRAY` adreslerini (`all_mine_addrs`) ve `+0x11d0`, `+0x2108`, `+0x2170`, `+0x5808`, `+0x7978` komşularını yakalayıp `CACHED_OBJS` içinde önbelleğe alır (`< 1ms` erişim).
  - `set_active_tactic` ve `set_lineup` çağrıldığında önbellekteki **tüm** kopyaların `+0x20` (`SEL_CURRENT`) ve `+0x28` (`SEL_SLOTS`) alanları aynı anda güncellenir ve eski taktiğin `oop_table` sırasından yeni taktiğin `oop_table` sırasına oyuncu mevkileri (`oop_order: 0=KL .. 10=SNT`) otomatik eşlenir.
  - `select_tactic` ve `apply_lineup` çağrıldığında devam eden arka plan okuması (`self.rx = None`) iptal edilir ki eski veri yeni yazılan verinin üstüne yazmasın.

## 2. Sürekli Canlı Bağlantı ve Butonların Kilitlenmemesi (`app/src/live.rs`, `app/src/gui.rs`)
- **Sorun:** Arka plan canlı bağlantı taraması (her 2 saniyede bir `refresh`) çalışırken `self.live.busy()` `true` döndüğü için Antrenman, Kadro ve Taktik sekmelerindeki butonlar gri/kilitli kalıyordu.
- **Kalıcı Çözüm:**
  - `rx_is_user_action` ve `train_is_user_action` bayrakları eklendi.
  - `busy()` yalnızca kullanıcı bir butona tıkladığında `true` döner; arka plan taramaları (`background_busy()`) hiçbir zaman arayüz butonlarını kilitlemez.
  - Kullanıcı bir butona tıkladığında devam eden arka plan okuması ezilerek kullanıcı komutu anında çalıştırılır.

## 3. Saha Dizilişi (Pitch View) Orta Saha ve Mevki Hizalaması (`app/src/tactics.rs`)
- **Kalıcı Çözüm:**
  - `places()` içinde oyuncunun gerçek saha çizgisini belirlerken FM'nin doğrudan saha koordinatına bağlı olan `oop_line` (topsuz diziliş hattı) önceliklidir.
  - `pitch_coordinates()` içinde Merkez Orta Saha (`row_key = 4`), Merkez Ofansif Orta Saha (`row_key = 5`) ve Kanatlar (`row_key = 6`) ayrı satırlardadır; merkez orta sahalar sağ kanada kaymaz.

## 4. Tek `FM27 Graphics Klasörü` & Dinamik Kullanıcı Yolu (`app/src/gui.rs`, `installer/Setup.cs`)
- **Kalıcı Çözüm:**
  - Sabit kullanıcı adı (`C:\Users\Emre\...`) kaldırıldı; her zaman `%USERPROFILE%\Documents\Sports Interactive\Football Manager 27\graphics` (veya OneDrive `Documents`) dinamik olarak algılanır.
  - Ayarlar ekranındaki ikinci `Faces` kutusu kaldırıldı; oyuncu yüzleri ve kulüp logoları doğrudan tek `FM27 Graphics Klasörü` içinden otomatik bulunur.
  - Uygulama açılışındaki dil seçimi pop-up'ı kaldırıldı (dil seçimi `Setup.exe` kurulum ekranında ve `Ayarlar` sekmesinde yapılır).

## 5. Easy Talk Sekmesi (`app/src/gui.rs`, `app/src/live.rs`)
- **Kalıcı Çözüm:**
  - `Easy Talk` sekmesi şimdilik sol menüden (`sidebar`) gizlendi ve arka plan `easy_talks` sorgusu kapatıldı.

## 6. Windows Program Ekle/Kaldır (`Uninstall`) Kaydı (`installer/Setup.cs`)
- **Kalıcı Çözüm:**
  - `Setup.exe` kurulum sırasında kurulum klasörüne `Uninstall FM27 Trainer.exe` kopyalar ve `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\FM27Trainer_Holyman3` kayıt defteri anahtarını oluşturur.
  - Böylece Windows **Ayarlar → Uygulamalar → Yüklü Uygulamalar** (ve Denetim Masası Program Ekle/Kaldır) listesinde **FM27 Trainer · Holyman3** logosuyla görünür ve tek tıkla kaldırılabilir.
