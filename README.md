# BudsPulse

Виджет в системном трее и приложение-компаньон для наушников **Nothing**, **CMF by Nothing** и **OnePlus / OPPO / realme (HeyMelody)** на **Windows 10/11** и **Linux** (Arch/CachyOS + KDE Plasma 6, Debian/Ubuntu/Fedora). Телефон не нужен.

Стек: **Tauri v2 + Rust** (бэкенд, протоколы, Bluetooth, DSP), **React 19 + TypeScript + Tailwind + Framer Motion** (Material 3 UI с темами OnePlus / Nothing / CMF).

## Скачать

Готовые сборки — на странице **[Releases](../../releases/latest)**, собирать ничего не нужно:

| Система | Файл | Что это |
|---|---|---|
| Windows 10/11 | `BudsPulse_x.y.z_x64-setup.exe` | установщик (рекомендуется): ярлык в «Пуске», удаление через «Приложения» |
| Windows 10/11 | `BudsPulse_x.y.z_x64_en-US.msi` | то же в формате MSI |
| Windows 10/11 | `BudsPulse-portable.exe` | портативная версия, без установки (положите в постоянную папку, не в «Загрузки») |
| Любой Linux x86_64 | `BudsPulse_x.y.z_amd64.AppImage` | `chmod +x BudsPulse_*.AppImage && ./BudsPulse_*.AppImage` |
| Debian / Ubuntu / Mint | `BudsPulse_x.y.z_amd64.deb` | `sudo apt install ./BudsPulse_*.deb` |
| Fedora / openSUSE | `BudsPulse-x.y.z-1.x86_64.rpm` | `sudo dnf install ./BudsPulse-*.rpm` |

- **Windows:** нужен WebView2 (в Windows 11 уже есть). Сборка не подписана, поэтому SmartScreen может предупредить: «Подробнее» → «Выполнить в любом случае».
- **Linux:** нужны BlueZ и PipeWire; glibc 2.39+ (Ubuntu 24.04+, Fedora 40+, Arch / CachyOS). Иконка в трее — StatusNotifierItem: в KDE Plasma работает сразу, в GNOME нужно расширение AppIndicator.

### Как пользоваться

1. Один раз сопрягите наушники в настройках Bluetooth системы.
2. Запустите BudsPulse — иконка появится в трее (на Windows может спрятаться под стрелкой `^`). **ЛКМ** — окно-виджет, **СКМ** — шумодав ↔ прозрачность, **ПКМ** — меню.
3. Автозагрузка: «Настройки» → «Запускать вместе с системой».

Если на телефоне открыто Nothing X / HeyMelody, оно может занимать канал управления — закройте его, если BudsPulse пишет «Нет связи с наушниками». Без наушников можно посмотреть демо-режим: «Пульт» → «Посмотреть без наушников». Настройки хранятся в `%APPDATA%\dev.budspulse.app` (Windows) или `~/.local/share/dev.budspulse.app` (Linux).

---

## Возможности

| Функция | Nothing / CMF | OnePlus (HeyMelody) |
|---|---|---|
| Батарея L / R / кейс + зарядка, кэш заряда кейса с отметкой «N мин назад» | ✅ `0xC007` / push `0xE001` | ✅ `0x0106` / push `0x0204·01` |
| Статус ношения (в ухе / снят / в кейсе) | ✅ `0xC00A` / push `0xE002` | ✅ `0x0109` / push `0x0204·02` |
| ANC / Прозрачность / Выкл + уровни (Макс, Средний, Слабый, Адаптивный / Умный) | ✅ `0xF00F` | ✅ `0x0404` (битовые маски, Adaptive — бит 11 на Pro 4) |
| Блокировка ANC, когда оба наушника сняты | — | ✅ (+ обработка статуса `0x0E`) |
| Пресеты EQ (Balanced/Voice/… · Dirac Opteo/Rock/… · Bold/Serenade/Dynaudio) | ✅ `0xF010` / CMF `0xF01D` | ✅ `0x0406` |
| Свой аппаратный EQ | ✅ 3 полосы, float32-пакет `0xF041` | ✅ 6 полос (10 на Pro 4), `0x0418` create/save |
| Ultra Bass 1–5 / BassWave −5…+5 | ✅ `0xF051` | ✅ `0x0403·1D` + `0x041B` |
| Игровой режим (низкая задержка) | ✅ `0xF040` | ✅ переключатель `06` |
| Датчик ношения (автопауза в наушниках) | ✅ `0xF004` | ✅ переключатель `04` |
| Пространственный звук | ✅ CMF Buds Pro 2 `0xF052` | ✅ `0x0422` (Выкл/Фикс/Head tracking) или `1B` |
| Двойное подключение | ✅ `0xF01A` | ✅ `11` + `0x0413` |
| Тест прилегания амбушюр | ✅ `0xF014` → `0xE00D` | ✅ `0x0405` → push `04` |
| Найти наушники | ✅ отдельно L/R `0xF002` | ✅ `0x0400` (звонят оба) |
| Жесты L/R | ✅ `0xF003` (в т.ч. скрытые комбинации) | ✅ `0x0401` |
| Hi-Res кодек (AAC / LHDC / LDAC, с предупреждением о перезагрузке) | ✅ `0xF01C` | — (наушники принимают только от телефона) |
| Прошивка, серийный номер, MAC (скрыт по умолчанию) | ✅ | ✅ (+ активный кодек линка `0x0114`) |
| Удаление из сопряжённых устройств системы (WinRT `UnpairAsync` / BlueZ `RemoveDevice`) | ✅ | ✅ |

**Честный UI:** любое переключение показывает пульсацию «ожидание ACK» и меняет состояние только после подтверждения от наушников (ACK `0x7xxx` или read-back у Nothing; отчёт `0x0204·03`/`0x810C` у OnePlus — ACK `0x8404 00` там не считается доказательством). Таймаут → откат и сообщение об ошибке.

**Двойной эквалайзер:**
- *В наушниках* — пресеты, свои полосы, Ultra Bass / BassWave (хранится в DSP наушников).
- *Студийный ПК* — 10-полосный параметрический EQ (PK / LSC / HSC / BP / Notch, Fc, Gain ±20 dB, Q 0.1–10), интерактивный график 20 Гц–20 кГц (перетаскивание, колесо = Q), авто-preamp против клиппинга, AutoEq-пресеты (Rtings → Harman) для CMF Buds Pro 2, Nothing Ear (2024)/(2)/(a), OnePlus Buds 3 / Pro / Pro 2, импорт/экспорт `ParametricEQ.txt`.
  - **Windows:** Equalizer APO — атомарная запись `budspulse_eq.txt` + управляемый блок `Include:` в `config.txt` (APO подхватывает изменения сразу). Кнопка «Установить» один раз выдаёт группе Users права на папку `config` (UAC).
  - **Linux:** PipeWire `filter-chain` (builtin `bq_*`): живой экземпляр `pipewire -c <conf>` + `pw-cli set-param` для изменения полос без перезапуска звука; синк «BudsPulse EQ» становится устройством по умолчанию и выводит на прежний выход. «Установить» дополнительно пишет drop-in `~/.config/pipewire/pipewire.conf.d/99-budspulse-eq.conf`.

**Android-фишки на ПК:**
- Автопауза при снятии наушника (MPRIS2 / Windows GSMTC) и возобновление, если вернуть в течение 3 минут (ставит на паузу только то, что реально играло).
- Авто-прозрачность, пока активен микрофон (PipeWire `Stream/Input/Audio` / WASAPI capture-сессии), с восстановлением прежнего режима.
- Авто-игровой режим при полноэкранном приложении (`SHQueryUserNotificationState` + проверка окна / Steam `SteamLaunch`, gamescope).
- Всплывающая карточка при подключении (L / кейс / R, кодек) без кражи фокуса.

**Подключение без лишнего радиообмена:** BudsPulse каждые 3 с спрашивает у ОС (WinRT `BluetoothDevice.ConnectionStatus` / BlueZ `Device1.Connected`), подключены ли наушники к ПК — это локальный запрос, без обращения к самим наушникам. Канал управления открывается только когда система уже держит Bluetooth-соединение; если ОС подключила другие поддерживаемые наушники — виджет переключается на них сам. Пока наушники не подключены, «Пульт» показывает понятный экран со ссылкой на настройки Bluetooth, а вкладки без содержимого скрываются.

**Трей:** динамическая иконка (две полоски заряда + точка режима: голубая — ANC, янтарная — прозрачность, серая — выкл), подсказка вида `OnePlus Buds Pro 4 • L: 90% | R: 85% | Case: 70% • ANC: High • LHDC 5.0`, ЛКМ — поповер, **СКМ — ANC ↔ Прозрачность с уведомлением**, ПКМ — меню. На Linux используется нативный StatusNotifierItem (`ksni`), поэтому клики работают и в KDE Plasma.

**Демо-режим / симулятор:** четыре виртуальных устройства (OnePlus Buds Pro 4, Buds Pro 3, Nothing Ear (2024), CMF Buds Pro 2), которые говорят **настоящими протоколами** — драйверы, ACK-логика, пуши, запрос времени OnePlus, обрыв линка при закрытии кейса и переподключение проверяются целиком. На вкладке «Устройство» есть кнопки «снять/надеть наушник», «зарядка», «убрать в кейс».

**Инспектор пакетов:** живой лог `[TX]/[RX]` с расшифровкой команд и экспорт диагностики в JSON (папка «Загрузки»).

---

## Запуск

### Windows

Нужно: Node.js 20+, Rust (`stable-x86_64-pc-windows-msvc`), Visual Studio 2022 Build Tools (C++ workload + Windows SDK), WebView2 (есть в Windows 11).

```powershell
npm install
npm run tauri dev                         # разработка
npm run tauri dev -- -- --demo            # сразу в демо-режиме (OnePlus Buds Pro 4)
npm run tauri dev -- -- --demo-model nothing-ear-2024
npm run tauri build                       # установщики NSIS + MSI в src-tauri/target/release/bundle/
```

Релизные сборки: `pwsh scripts/build-windows.ps1` (→ `release/Windows`) и `bash scripts/build-linux.sh` в WSL / Ubuntu (→ `release/Linux`). Скрипты убирают из бинарников пути к домашней папке сборщика. В CI то же делает `.github/workflows/release.yml` (запуск по тегу `v*` или вручную).

### Linux (Arch / CachyOS)

```bash
sudo pacman -S --needed webkit2gtk-4.1 base-devel curl wget file openssl appmenu-gtk-module libappindicator-gtk3 librsvg \
                        bluez bluez-utils pipewire wireplumber
npm install && npm run tauri dev
npm run tauri build                       # .deb / .rpm / AppImage
```
Debian/Ubuntu: `sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`.

### Ключи командной строки

| Ключ | Действие |
|---|---|
| `--demo` | запустить симулятор (модель из настроек или OnePlus Buds Pro 4) |
| `--demo-model <key>` | `oneplus-buds-pro-4`, `oneplus-buds-pro-3`, `nothing-ear-2024`, `cmf-buds-pro-2` |
| `--minimized` | стартовать только в трее (используется автозапуском) |
| `--tab <id>` | открыть вкладку `dashboard` / `sound` / `gestures` / `device` (для разработки) |
| `BUDSPULSE_PIN=1` | (переменная окружения) не прятать окно при потере фокуса — для отладки |

---

## Архитектура

```text
budspulse/
├── src/                         React UI
│   ├── App.tsx, main.tsx, toast.tsx
│   ├── types/earbuds.ts         контракт состояния (зеркало Rust-структур, camelCase)
│   ├── store/useEarbudsStore.ts Zustand + события Tauri (snapshot, packet, navigate)
│   ├── lib/backend.ts           invoke/listen; webMock.ts — предпросмотр в браузере без Tauri
│   ├── lib/biquad.ts            та же RBJ-математика, что и в Rust (график 60 fps)
│   ├── components/              HeaderBar, BatteryRings, NoiseControlCard, QuickTilesGrid,
│   │                            EqualizerTab + PeqGraph, GesturesAutoTab, DeviceToolsTab, ConnectionToast
│   └── presets/autoeqPresets.ts AutoEq (Rtings → Harman) + свои пресеты
├── src-tauri/src/
│   ├── protocols/               sans-IO драйверы: mod.rs (состояние, команды, PendingTracker),
│   │                            nothing.rs (0x55 + CRC16/MODBUS), oneplus.rs (OPOv1 0xAA, LEB128)
│   ├── bt/                      mod.rs (трейт EarbudTransport, сессия: рукопожатие, ACK-таймауты,
│   │                            опрос, переподключение с backoff), windows.rs (WinRT RFCOMM,
│   │                            Winsock AF_BTH по UUID/каналу, BLE GATT), linux.rs (BlueZ Profile1,
│   │                            сырой RFCOMM, BLE GATT через D-Bus), simulator.rs
│   ├── audio_eq/                биквады, авто-preamp, формат APO, windows_apo.rs, linux_pipewire.rs
│   ├── automation/              координатор, media_control.rs (MPRIS/GSMTC), mic_monitor.rs (+ игры)
│   ├── tray.rs                  рендер RGBA-иконки, подсказка, ksni (Linux)
│   ├── state.rs                 ядро: снапшоты, хранилище, тост, кэш кейса, хост автоматики
│   └── lib.rs                   команды Tauri, трей, окна, плагины
├── src-tauri/tests/simulator_e2e.rs   сквозной тест: сессия + драйверы + симулятор
├── tools/linux-check/           проверка Linux-модулей (`cargo check --target x86_64-unknown-linux-gnu`) с Windows
└── docs/protocols/              исследование протоколов с источниками
```

---

## Протоколы и исправления исходного ТЗ

Публично описанные протоколы местами расходятся с реальностью. Всё перепроверено по исходникам ear-web, Gadgetbridge, BudsLink, QuickBuds, melodyctl и реальным HCI-захватам (подробно — `docs/protocols/*.md`):

**Nothing / CMF**
- Golden-кадры ANC High/Mid/Low/Adaptive имели неверный CRC (байты режима верны). Правильно: `…cf 01 01 00 c7 3f`, `…d5 01 02 00 c0 d7`, `…d7 01 03 00 c0 ff`, `…dd 01 04 00 c1 17`.
- Push батареи — `0xE001`, а не `0xE007`. CRC есть только при `CTRL & 0x20`; push-кадры устройства (`55 00 01 …`) идут без CRC.
- Пресеты Nothing: 0 Balanced, 1 Voice, 2 More Treble, 3 More Bass, **5** Custom. У CMF отдельная команда «listening mode» `0xF01D`/`0xC050`: 0 Dirac Opteo … 6 Custom.
- Свой EQ — не `[i8;3]`, а 53-байтовый пакет float32 `0xF041` (частота верхней полки 3400 Гц у Ear (2024), 6900 Гц у CMF Buds Pro 2).

**OnePlus / OPPO**
- **Авторизации нет.** «HELLO» — это обычный handshake `0x0100` (последний байт `12` вне кадра), «REGISTER/token» — ответ хоста на запрос времени `0x0500` с устаревшим timestamp. BudsPulse отвечает на `0x05xx` реальным временем (+ смещение часового пояса для Pro 4).
- `LEN` = `7 + PayLen` (LEB128), кадр = `LEN + 2`. Ответ = команда `| 0x8000`, seq эхом, push — `0x0204` с seq `FF`.
- ANC: **`01` = Выкл, `02` = ANC, `04` = Прозрачность**, уровни `10/20/40/80`, Adaptive (Pro 4) — `00 08`; отчёты используют другие биты (`0x0008` Выкл, `0x0100` Прозрачность).
- BLE — только резерв; основной транспорт RFCOMM `079A` (на Windows — сокет AF_BTH с UUID и портом 0, SDP делает система).

---

## Проверка

```bash
cd src-tauri && cargo test          # 29 юнит-тестов (golden-кадры, CRC, парсеры, ACK-логика, EQ-математика)
                                    # + simulator_e2e: Pro 4 / Ear (2024) / CMF Buds Pro 2 целиком через сессию
cd .. && npm run build              # tsc + vite
cargo check --target x86_64-unknown-linux-gnu --manifest-path tools/linux-check/Cargo.toml
```

## Что не проверено на железе

- Работа с реальными наушниками: протоколы реализованы по проверенным источникам и захватам, но во время разработки физические наушники к ПК подключены не были. Для отладки: вкладка «Устройство» → «Инспектор пакетов» → экспорт JSON.
- **OnePlus Buds Pro 4:** product id `067814`, дерево ANC, 10 полос EQ, `utcTimeZone` взяты из whitelist HeyMelody — опубликованных захватов Pro 4 ещё нет. Под-режимы Adaptive (биты 12–14) только распознаются, не отправляются.
- Пространственный звук OnePlus, вероятно, не даёт эффекта, когда источник звука — ПК; Hi-Res (`18`) наушники принимают только от телефона, поэтому в UI для OnePlus он скрыт.
- На Windows ОС не сообщает активный A2DP-кодек; для OnePlus показывается кодек из ответа наушников `0x0114`, для Nothing — выбранный Hi-Res режим.
- Equalizer APO должен быть установлен и включён для Bluetooth-наушников в Configurator.

## Лицензия

[MIT](LICENSE) © 2026 zer0rlt. Nothing, CMF, OnePlus, OPPO и realme — товарные знаки своих владельцев; проект с ними не связан.
