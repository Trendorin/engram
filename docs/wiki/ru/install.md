# Установка

Engram работает на Linux x86-64. В нём три программы, которые ставятся вместе:

| Программа | Роль |
|---|---|
| `engramd` | Движок. Хранит сессию и единственный читает память других программ |
| `engram` | Командная строка, MCP-сервер для агентов, `setup`, `doctor`, `config`, `uninstall` |
| `engram-gui` | Окно (по желанию) |

## Из пакета

В каждом [релизе](https://github.com/Trendorin/engram/releases/latest) есть пакеты для Arch Linux,
Debian 13+ и Fedora 44+, AppImage для любого дистрибутива с glibc 2.35+ и `SHA256SUMS`.

```sh
sudo pacman -U ./engram-*-x86_64.pkg.tar.zst     # Arch, CachyOS, Manjaro
sudo apt install ./engram_*_amd64.deb            # Debian
sudo dnf install ./engram-*.x86_64.rpm           # Fedora
chmod +x Engram-*-x86_64.AppImage                # всё остальное
```

Пакеты кладут программы в `/usr/bin` и чисто удаляются менеджером пакетов. AppImage открывает
окно, а с аргументами работает как командная строка `engram` (`./Engram-*.AppImage doctor`,
`./Engram-*.AppImage mcp` в настройке MCP агента). Его файлы только для чтения, поэтому
`engram setup` там не даст доступ к памяти — запускайте игры через Engram.

## Из исходников

Нужны Rust 1.88 или новее, компилятор C/C++, pkg-config, заголовки fontconfig и xkbcommon
для окна и, для нативного вида, Qt 6. `install.sh` всё это проверит и предложит поставить
недостающее (ответ по умолчанию — да, в том числе с `--yes`):

| Дистрибутив | Пакеты, которые установщик предложит поставить |
|---|---|
| Arch / CachyOS / Manjaro | `base-devel rust libcap qt6-base fontconfig libxkbcommon` |
| Debian / Ubuntu | `build-essential pkg-config curl ca-certificates libcap2-bin qt6-base-dev qt6-base-dev-tools libfontconfig-dev libxkbcommon-dev` |
| Fedora | `gcc gcc-c++ pkgconf-pkg-config cargo rust libcap qt6-qtbase-devel fontconfig-devel libxkbcommon-devel` |
| openSUSE | `gcc gcc-c++ pkgconf-pkg-config cargo rust libcap-progs qt6-base-devel fontconfig-devel libxkbcommon-devel` |

В Debian и Ubuntu Rust старее 1.88, как и в некоторых других системах. Тогда установщик предложит
поставить текущий stable Rust через [rustup](https://rustup.rs) в `~/.cargo` — без `sudo` и без
правки профиля оболочки; если rustup уже есть, он добавит toolchain stable, а ваш toolchain по
умолчанию не тронет. Без терминала и без `sudo` без пароля он вместо этого покажет команду.
`--no-packages` всё это отключает.

```sh
git clone https://github.com/Trendorin/engram
cd engram
./install.sh
```

Первая сборка компилирует UI-библиотеку и занимает несколько минут, следующие проходят быстро.
Без Qt 6 окно всё равно соберётся, но будет в собственном стиле Slint, а не в стиле системы.

## Что делает установщик

В терминале `./install.sh` работает интерактивно:

1. проверяет систему (Rust, компилятор, библиотеки, Qt) и предлагает поставить недостающее;
2. спрашивает, куда ставить (по умолчанию `~/.local`), нужно ли окно и выдать ли сразу
   доступ к памяти;
3. собирает проект (при установке из исходников) и показывает прогресс;
4. копирует программы в `<prefix>/bin`, добавляет пункт меню и иконку, пишет манифест
   для удаления `<prefix>/share/engram/installed-files`;
5. готовит рабочую папку агента (`~/.local/share/engram/workspace`);
6. по желанию запускает `engram setup`.

Если Engram уже установлен, установщик предлагает **Обновить**, **Удалить** или **Выйти**.

Флаги для скриптов и CI:

| Флаг | Значение |
|---|---|
| `--yes`, `-y` | Без вопросов: значения по умолчанию плюс остальные флаги |
| `--prefix DIR` | Ставить в `DIR/bin` и `DIR/share` |
| `--no-gui` | Только движок, CLI и MCP-сервер |
| `--setup` / `--no-setup` | Выдать доступ к памяти после установки или не спрашивать |
| `--no-packages` | Не ставить пакеты и Rust, только назвать, чего не хватает |
| `--uninstall` | Удалить Engram (то же, что `engram uninstall`) |
| `--purge` | Вместе с `--uninstall`: удалить ещё настройки, таблицы и логи |

Сам Engram ставится без `sudo`. Установщик просит его только для того, чтобы поставить
недостающие пакеты дистрибутива; кроме этого пароль спрашивает только `engram setup`, через
pkexec (окно рабочего стола) или sudo, и только для одного `setcap` на `engramd`.

## Доступ к памяти: `engram setup`

Почти во всех дистрибутивах стоит Yama `ptrace_scope=1`: программа может читать память
другой программы, только если сама её запустила. Есть два варианта:

- **`engram setup`** (рекомендуется). Выдаёт `CAP_SYS_PTRACE` только `engramd`, после этого
  Engram подключается к любой программе твоего пользователя. Окно и агенты остаются без
  привилегий.
- **Запускать игры через Engram**: поле «Start a program» в окне или
  `engram launch -- <команда>`. В Steam укажи параметр запуска `engram launch -- %command%`.

Замена `engramd` (обновление или переустановка) снимает это право, поэтому после неё снова
выполни `engram setup`. Текущее состояние показывает `engram doctor`.

## Обновление

Запусти новый `install.sh` и выбери **Обновить**. Настройки и таблицы сохранятся.

## Удаление

```sh
engram uninstall           # спросит подтверждение, данные оставит
engram uninstall --purge   # удалит также настройки, таблицы, рабочую папку агента и логи
```

Удаляется ровно то, что записано в манифесте. Останавливаются движок и окно только этой
установки, другие копии Engram не трогаются. Если ты подключал Engram к агенту глобально
(например, `claude mcp add engram`), удали эту запись и в самом агенте.
