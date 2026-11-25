# Multi-Monitor Tray Dimmer

> Нативная Windows-утилита в системном трее (Rust, ~155 КБ, без консольного окна и сторонних зависимостей) для быстрой регулировки яркости и затемнения **каждого монитора по отдельности** из трея или по глобальным хоткеям.
>
> *Lightweight native Windows system-tray utility (Rust, ~155 KB, zero dependencies) to adjust brightness & dimming per monitor via DDC/CI + smooth overlay and global hotkeys.*

## Скачать / Download

- **[Скачать последнюю версию (.exe) — GitHub Releases](https://github.com/DiM-SoTiS/multi-monitor-tray-dimmer/releases/latest)**
- **Все наши утилиты и приложения:** [DiM-SoTiS / Apps Catalog](https://github.com/DiM-SoTiS/apps-catalog)

Просто скачайте `multi-monitor-tray-dimmer.exe` и запустите — установка не требуется (portable).

## Возможности / Features
- **Работает в трее возле часов** — без окна терминала и нагрузки на систему.
- **Раздельное управление каждым монитором:** индивидуальное подменю для каждого экрана (`100%`, `80%`, `60%`, `40%`, `20%`) через аппаратный **DDC/CI** (где поддерживается) + плавный некликабельный оверлей затемнения (работает на любом мониторе, даже ниже аппаратного минимума).
- **Глобальные хоткеи:**
  - `Ctrl + Alt + ↑` — повысить яркость активного монитора на `+10%`.
  - `Ctrl + Alt + ↓` — понизить яркость активного монитора на `-10%`.
  - `Ctrl + Alt + D` — быстрый «Ночной режим» (`40%` ↔ `100%`).
- **Всплывающее уведомление у часов** с текущим уровнем яркости и автозакрытием через 2.5 секунды.
- **Сохранение профилей** в `%APPDATA%\multi-monitor-tray-dimmer\config.ini`.

## Сборка из исходников / Build from source

```powershell
cargo test
cargo build --release
```

Готовый исполняемый файл: `target\release\multi-monitor-tray-dimmer.exe`.

## Поддержать разработку / Support & Donate

Утилита бесплатна и с открытым исходным кодом. Если она оказалась полезной и экономит вам время, вы можете поддержать выход новых бесплатных утилит для Windows и Android:

- Поставьте звезду (**★ Star**) этому репозиторию — это сильно помогает другим пользователям найти утилиту.
- **Донат / Поддержка автора (ЮMoney):** [Поддержать разработку](https://yoomoney.ru/fundraise/1LP6PQKIMJN.261125)
- Загляните в наш общий каталог релизов: **[DiM-SoTiS/apps-catalog](https://github.com/DiM-SoTiS/apps-catalog)**

## Обратная связь / Feedback
Нашли баг или есть идея для новой функции? Создайте [Issue](https://github.com/DiM-SoTiS/multi-monitor-tray-dimmer/issues).
