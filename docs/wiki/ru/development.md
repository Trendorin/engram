# Разработка

## Архитектура

```
 engram-gui ─┐
 engram CLI ─┼── Unix-сокет, JSON построчно ──► engramd ──► /proc/<pid>/mem, ptrace
 engram mcp ─┘   ($XDG_RUNTIME_DIR/engram, 0600, проверка uid)
     ▲
     └── OpenCode / Claude Code / Codex / Gemini (MCP или CLI)
```

`engramd` хранит единственную сессию: цель, скан с одним шагом отмены, таблицу адресов,
заморозки, политику подтверждений и журнал активности. Все клиенты равноправны и умеют сами
запустить движок.

- Запрос: `{"id":1,"method":"scan","params":{...},"client":"mcp:opencode"}`
- Ответ: `{"id":1,"ok":true,"result":{...}}` или `{"id":1,"ok":false,"error":{"code","message","hint"}}`
- Соединение, которое отправило `subscribe`, становится потоком событий: `activity`, `target`,
  `scan`, `table`, `progress`, `notify`, `approval_request`, `approval_resolved`, `policy`,
  `memory`, `launch`.
- Инструменты с `Access::Mutate` защищены: вызовы от всех клиентов, кроме окна, ждут
  `approval_respond` из окна или подчиняются политике.

## Крейты

| Крейт | Модули |
|---|---|
| `engram-core` | `process` (список с учётом Wine/Proton), `maps`, `memory` (`process_vm_readv`, `/proc/pid/mem`), `scan`, `pointer`, `expr`, `value`, `analyze`, `disasm` (iced-x86), `watch` (отладочные регистры) |
| `engram-proto` | `tools/` (каталог, по файлу на группу), `config` (настройки), `client`, `paths`, `agentkit` (рабочая папка и шаблон AGENTS.md) |
| `engramd` | `daemon` (состояние, события, подтверждения), `target`, `table` (+ заморозка), `tools/` (обработчики, по файлу на группу), `params` |
| `engram-cli` | `main` (маршрутизация), `help`, `config`, `setup` (doctor/setup/agent-init), `uninstall`, `mcp`, `term` |
| `engram-gui` | `ui/*.slint` (виды; `state.slint` — глобальный `App`), `ctx`, `events`, `wire`, `view`, `poll`, `inspect`, `settings`, `launcher`, `engine`, `format` |

## Как добавить инструмент

1. Добавь `ToolDef` в нужную группу `crates/engram-proto/src/tools/<group>.rs`. Описание пиши
   для языковой модели: что делает инструмент, когда его вызывать, что делать дальше. Если он
   меняет цель, ставь `Access::Mutate`.
2. Обработай его в `crates/engramd/src/tools/<group>.rs` (`dispatch` плюс функция). Для
   `Mutate` перед изменениями вызывай `d.approve(client, ...)`.
3. Ошибки возвращай как `RpcError::new("snake_case_code", msg).hint("что делать дальше")`.
4. Перегенерируй справочник: `cargo run -p engram -- tools --markdown > docs/wiki/en/tools.md`.
5. Если меняется работа агентов, обнови `crates/engram-proto/assets/AGENTS.md`.

Справка CLI и список инструментов MCP подхватят его сами. Тест `every_tool_is_routed`
упадёт, если у инструмента из каталога нет обработчика.

Новую группу добавь в `GROUPS` в `engram-proto/src/tools/mod.rs` и в цепочку маршрутизации в
`engramd/src/tools/mod.rs`.

## Как добавить настройку

Добавь поле со значением по умолчанию в `Config` (`crates/engram-proto/src/config.rs`),
запись в `KEYS` и ветки в `get` и `set` (с проверкой). `engram config` покажет её сам. Для
окна добавь поле в `ui/settings.slint` и `src/settings.rs`.

## Сборка, тесты, линтер

```sh
cargo build                      # отладочная сборка
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
```

`.cargo/config.toml` указывает Slint на `/usr/bin/qmake6`; переопределяется через `QMAKE=...`.
Slint 1.18 требует фичу `backend-qt` для Qt-бэкенда. `build.rs` выбирает стиль `native`:
`qt`, если Qt найден, иначе `fluent`. `SLINT_STYLE` переопределяет выбор.

## Ручная проверка

Не тестируй на своей настоящей сессии и установке: используй отдельные runtime- и
data-каталоги или временный `HOME`.

```sh
export XDG_RUNTIME_DIR=/run/user/$UID/engram-dev XDG_DATA_HOME=/tmp/engram-dev/data XDG_STATE_HOME=/tmp/engram-dev/state
mkdir -p -m700 $XDG_RUNTIME_DIR
cargo build
target/debug/engram launch -- target/debug/engram-testbed /tmp/engram-dev/ctl &
target/debug/engram attach name=engram-testbed
target/debug/engram scan value=37
echo loot > /tmp/engram-dev/ctl          # стрелы 37 → 36, золото +25
target/debug/engram scan_next value=36
```

Тестовая программа (`tools/testbed`) — маленькая «игра» с известной структурой. Через
управляющий файл она принимает команды `hit`, `loot`, `move` и `quit`.

Для окна: если `XDG_RUNTIME_DIR` изменён, укажи Wayland абсолютный путь к настоящему сокету
(`WAYLAND_DISPLAY=/run/user/$UID/wayland-1`). `ENGRAM_GUI_SNAPSHOT=<dir>` заставляет окно
сохранять снимки `snap-NNN.ppm` каждые 1,5 с. Управляй сессией через CLI с
`ENGRAM_CLIENT=<агент>`, чтобы видеть действия агента и подтверждения. Останавливай процессы
по точному имени (`pkill -x engramd`).

Установщик проверяй только с временным `HOME`, оставив `CARGO_HOME` и `RUSTUP_HOME` на
настоящем тулчейне: он ставит в `$HOME/.local` и удаляет оттуда.

## Релизы

1. Обнови версию в `Cargo.toml` (`[workspace.package]`) и `CHANGELOG.md`.
2. Сделай коммит, поставь тег `vX.Y.Z` и отправь его.
3. Workflow `release` собирает проект на Ubuntu 24.04, запускает `packaging/package.sh` и
   публикует `engram-X.Y.Z-x86_64-linux.tar.gz` вместе с `.sha256` в GitHub Release.

`packaging/package.sh` работает и локально, результат кладёт в `dist/`.

## Ограничения и идеи

- Только Linux x86-64 (точки останова и дизассемблер привязаны к x86).
- Идеи: редактор структур на основе `dissect`, панель просмотра памяти, импорт таблиц
  Cheat Engine `.CT`, пакет в AUR.
