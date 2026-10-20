//! The window's language. Texts in the `.slint` files use `@tr` with the bundled catalogue
//! (`lang/ru/LC_MESSAGES/engram-gui.po`); messages made in Rust go through [`t`] and [`tf`].

use std::sync::atomic::{AtomicBool, Ordering};

static RUSSIAN: AtomicBool = AtomicBool::new(false);

/// Make `language` ("auto", "en" or "ru") the language of the window, at once.
pub fn apply(language: &str) {
    let ru = match language {
        "ru" => true,
        "en" => false,
        _ => system_is_russian(),
    };
    RUSSIAN.store(ru, Ordering::Relaxed);
    let _ = slint::select_bundled_translation(if ru { "ru" } else { "en" });
}

fn system_is_russian() -> bool {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()))
        .is_some_and(|v| v.to_ascii_lowercase().starts_with("ru"))
}

/// A message with `{}` placeholders, translated if the window is Russian.
pub fn t(text: &str) -> String {
    if RUSSIAN.load(Ordering::Relaxed) {
        if let Some((_, ru)) = RU.iter().find(|(en, _)| *en == text) {
            return (*ru).to_string();
        }
    }
    text.to_string()
}

/// [`t`], then each `{}` replaced by the next argument.
pub fn tf(text: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::new();
    let mut rest = t(text);
    for a in args {
        match rest.split_once("{}") {
            Some((head, tail)) => {
                out.push_str(head);
                out.push_str(&a.to_string());
                rest = tail.to_string();
            }
            None => break,
        }
    }
    out.push_str(&rest);
    out
}

const RU: &[(&str, &str)] = &[
    ("Starting the engine…", "Запуск движка…"),
    ("Engine ready", "Движок готов"),
    ("Reconnected to the engine", "Связь с движком восстановлена"),
    ("Detached", "Отключено"),
    ("Entry added", "Запись добавлена"),
    ("Entry updated", "Запись обновлена"),
    ("Request allowed", "Запрос разрешён"),
    (
        "Request allowed; this agent may write without asking until the engine restarts",
        "Запрос разрешён; этот агент может писать без вопросов до перезапуска движка",
    ),
    ("Request denied", "Запрос отклонён"),
    ("An approval request timed out", "Время ожидания подтверждения истекло"),
    ("Running engram setup — confirm the password prompt…", "Выполняется engram setup — подтвердите запрос пароля…"),
    ("{} is waiting for your approval", "{} ждёт вашего подтверждения"),
    ("{} (pid {}) exited", "{} (pid {}) завершился"),
    ("Started pid {}. Pick it (or its game process) in the list.", "Запущен pid {}. Выберите его (или процесс игры) в списке."),
    ("Restored {} result(s)", "Восстановлено результатов: {}"),
    ("Added {} ({}) to the address table", "Добавлено в таблицу адресов: {} ({})"),
    ("Wrote {}", "Записано: {}"),
    ("Agent writes: {}", "Запись агентов: {}"),
    ("Attached to {}", "Подключено к {}"),
    ("Attached to {}; loaded {} saved address(es)", "Подключено к {}; загружено сохранённых адресов: {}"),
    ("Cannot write {}: {}", "Не удаётся записать {}: {}"),
    (
        "Saved. The start-up policy applies the next time the engine starts.",
        "Сохранено. Политика запуска применится при следующем запуске движка.",
    ),
    ("Defaults restored in the form — press Save to keep them.", "Значения по умолчанию в форме — нажмите «Сохранить», чтобы оставить их."),
    ("Showing the first {} of {}. Narrow down with Next scan.", "Показаны первые {} из {}. Сузьте поиск кнопкой «Следующий поиск»."),
    ("Double-click a result to add it to the address table.", "Дважды щёлкните результат, чтобы добавить его в таблицу адресов."),
    ("Sampling {} as {} for 3 s…", "Замер {} как {} в течение 3 с…"),
    (
        "Watching {}s to {} for 8 s — trigger the change in-game now",
        "Слежение: {} по адресу {} в течение 8 с — вызовите изменение в игре сейчас",
    ),
    ("Opened {} in {}", "Открыт {} в {}"),
    ("Opened a terminal in {} — start any agent there", "Открыт терминал в {} — запустите там любого агента"),
    ("Opened {}", "Открыто: {}"),
    (
        "Access granted: engramd can now read your programs (the engine restarts)",
        "Доступ выдан: engramd теперь может читать ваши программы (движок перезапустится)",
    ),
    ("Status", "Состояние"),
    ("Scan", "Поиск"),
    ("Detach", "Отключение"),
    ("Pause", "Пауза"),
    ("Undo", "Отмена"),
    ("Reset", "Сброс"),
    ("Add", "Добавление"),
    ("Update", "Обновление"),
    ("Freeze", "Заморозка"),
    ("Remove", "Удаление"),
    ("Policy", "Политика"),
    ("Approval", "Подтверждение"),
    ("Engine", "Движок"),
    ("Inspect", "Изучение"),
];
