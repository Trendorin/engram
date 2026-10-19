# Командная строка

`engram` — командная строка для людей и агентов. Движок запускается автоматически.

## Команды

| Команда | Что делает |
|---|---|
| `engram <tool> key=value ...` | Вызывает инструмент и печатает JSON (в терминале — с отступами) |
| `engram <tool> '<json>'` | То же, но аргументы в JSON |
| `engram tools` | Список инструментов по группам (`[approval]` — агентам нужно подтверждение) |
| `engram tools --markdown` | Печатает [справочник инструментов](../en/tools.md) |
| `engram help <tool>` | Аргументы, значения по умолчанию и пример |
| `engram doctor` | Проверяет бинарники, доступ к памяти, движок, агентов и настройки |
| `engram setup` | Выдаёт движку доступ к памяти (один раз спросит пароль) |
| `engram agent-init [dir]` | Записывает AGENTS.md и настройку MCP в папку |
| `engram mcp` | Запускает MCP-сервер на stdio |
| `engram config ...` | Показывает и меняет [настройки](configuration.md) |
| `engram launch -- <cmd> [args]` | Запускает программу дочерним процессом движка и ждёт её завершения |
| `engram events` | Поток событий сессии (строки JSON) |
| `engram daemon start\|stop\|status` | Управление движком |
| `engram uninstall [--purge]` | Удаляет Engram ([подробнее](install.md#удаление)) |
| `engram version` | Версия |

`--pretty` / `--compact` принудительно задают формат JSON. Код выхода 1 означает, что
инструмент вернул ошибку; она печатается как `{"error": {"code", "message", "hint"}}`.

## Аргументы

Пары `key=value` превращаются в JSON. Числа и `true`/`false` становятся числами и логическими
значениями, всё остальное остаётся строкой. Поэтому `address=[[Game.exe+10]+18]` не нужно
экранировать сверх того, что требует оболочка. Для массивов и объектов передай один аргумент в JSON.

```sh
engram processes query=minecraft
engram attach pid=12345
engram scan value=37
engram scan_next compare=decreased
engram scan_results limit=20
engram analyze address=0x55d0c3a17e50
engram table_add address=0x55d0c3a17e50 type=i32 description=Arrows
engram write '{"address":"0x55d0c3a17e50","value":"74","expected":"37","reason":"Удвоить стрелы"}'
engram pointer_scan address=0x55d0c3a17e50 max_depth=4
engram watch address=0x55d0c3a17e50 kind=write duration_ms=6000
```

Если команду запускает агент, имя клиента в ленте активности определяется само
(`claude-code`, `opencode`, `codex`, `gemini`). Переопределить его можно через `ENGRAM_CLIENT`.

## Параметр запуска Steam

```
engram launch -- %command%
```

Игра становится дочерним процессом движка, поэтому Engram читает её без `engram setup`.
`engram launch` работает, пока работает игра, так что Steam видит настоящее время работы.
Для Proton-игр подключайся к процессу `Game.exe`, а не к запускальщику.

## Скрипты

Если вывод не в терминал, CLI печатает компактный JSON, поэтому его удобно разбирать через `jq`:

```sh
engram scan_results limit=1000 | jq -r '.results[] | select(.where=="heap") | .address'
```
