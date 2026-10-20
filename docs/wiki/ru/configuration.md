# Настройки

Настройки хранятся в `~/.config/engram/config.json` (или в `$XDG_CONFIG_HOME/engram`).
Менять их можно в окне (вкладка **Settings**) или командой `engram config`. Неверные или
неизвестные значения в файле заменяются значениями по умолчанию, так что ручная правка файла
Engram не сломает.

```sh
engram config                      # все параметры с описаниями
engram config get policy
engram config set refresh_ms 400
engram config reset freeze_interval_ms
engram config reset                # всё по умолчанию
engram config path
```

| Ключ | По умолчанию | Значения | Что делает |
|---|---|---|---|
| `policy` | `ask` | `ask` `allow` `deny` | Политика записи агентами при старте движка. Текущую можно менять в окне |
| `approval_timeout_s` | `180` | 10–3600 | Сколько запрос агента ждёт твоего ответа |
| `freeze_interval_ms` | `100` | 20–5000 | Как часто перезаписываются замороженные значения |
| `agent` | (первый найденный) | имя агента | Агент для «Open terminal» (запоминается при использовании) |
| `terminal` | (автоопределение) | команда | Эмулятор терминала, можно со своими аргументами (`kitty --single-instance`) |
| `workspace` | `~/.local/share/engram/workspace` | путь | Папка, в которой запускаются агенты |
| `result_rows` | `1000` | 50–1000 | Сколько результатов скана показывает окно |
| `refresh_ms` | `600` | 200–5000 | Как часто окно обновляет живые значения |
| `language` | `auto` | `auto`, `en`, `ru` | Язык окна; `auto` — как в системе. Меняется сразу после сохранения |

При сохранении в окне или через `engram config set` время ожидания и интервал заморозки сразу
применяются к работающему движку. `policy` вступает в силу при следующем запуске движка.

## Файлы

| Путь | Содержимое |
|---|---|
| `~/.config/engram/config.json` | Настройки |
| `~/.local/share/engram/tables/<игра>.json` | Сохранённые таблицы адресов, по одной на игру (с учётом Steam AppID) |
| `~/.local/share/engram/workspace/` | Рабочая папка агента |
| `~/.local/state/engram/engramd.log` | Журнал движка |
| `~/.local/state/engram/launch.log` | Вывод программ, запущенных через `launch` |
| `$XDG_RUNTIME_DIR/engram/engramd.sock` | Сокет движка (права 0600, папка 0700) |
| `<prefix>/share/engram/installed-files` | Манифест для удаления |

## Переменные окружения

| Переменная | Действие |
|---|---|
| `ENGRAM_POLICY` | Политика при старте `engramd`. Важнее настройки; `--policy` важнее обоих |
| `ENGRAM_CLIENT` | Имя клиента для вызовов из CLI в ленте активности |
| `ENGRAM_SOCKET` | Другой путь к сокету |
| `ENGRAMD_BIN`, `ENGRAM_BIN`, `ENGRAM_GUI_BIN` | Использовать конкретный бинарник вместо поиска рядом / в PATH |
| `TERMINAL` | Предпочитаемый терминал, если настройка `terminal` пуста |
| `NO_COLOR` | Отключить цвета в выводе CLI |
| `ENGRAM_GUI_SNAPSHOT` | Для разработчиков: окно сохраняет снимки себя в эту папку |
| `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME`, `XDG_RUNTIME_DIR` | Стандартные пути |
