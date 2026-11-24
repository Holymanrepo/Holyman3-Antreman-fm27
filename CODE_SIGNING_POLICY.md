# Code Signing Policy / İmzalama Politikası

Free code signing provided by [SignPath.io](https://signpath.io), certificate by
[SignPath Foundation](https://signpath.org).

Açık kaynak kod imzalama altyapısı [SignPath.io](https://signpath.io) tarafından, sertifika ise
[SignPath Foundation](https://signpath.org) tarafından ücretsiz olarak sağlanmaktadır.

## What is signed / İmzalanan Dosyalar

The Windows executable (`FM27.Trainer.exe`) and Windows installer (`FM27.Trainer.Setup.exe`) of each release.
They are built directly from this repository by GitHub Actions (`.github/workflows/release.yml`) and signed
only after an authorized project maintainer approves the signing request.

Her sürümde yer alan Windows çalıştırılabilir dosyası (`FM27.Trainer.exe`) ve Windows kurulum sihirbazı
(`FM27.Trainer.Setup.exe`), doğrudan bu GitHub deposundan GitHub Actions (`.github/workflows/release.yml`)
tarafından derlenir ve yalnızca yetkili proje yöneticisinin onayından sonra imzalanır.

## Team roles / Ekip Rolleri

- Committers and reviewers / Geliştirici ve İnceleyenler: [Holymanrepo (Holyman3)](https://github.com/Holymanrepo)
- Approvers / Onaylayanlar: [Holymanrepo (Holyman3)](https://github.com/Holymanrepo)

## Privacy policy / Gizlilik Politikası

This program operates locally on the user's computer for single-player Football Manager 27 and will not transfer
any personal information or telemetry to external networked systems, except checking GitHub Releases (`api.github.com`)
for new versions of this open-source project and downloading updates when requested by the user.

Bu program tamamen kullanıcının kendi bilgisayarında yerel olarak (tek oyunculu Football Manager 27 için) çalışır;
hiçbir kişisel veriyi veya telemetri bilgisini dış sunuculara göndermez (yalnızca GitHub Releases üzerinden yeni sürüm
kontrolü yapar ve kullanıcı istediğinde güncellemeyi indirir).
