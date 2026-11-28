//! Bilingual RU / EN localization with Windows UI language auto-detection.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Ru,
    En,
}

impl Lang {
    pub fn as_code(self) -> &'static str {
        match self {
            Lang::Ru => "ru",
            Lang::En => "en",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        match code.trim().to_ascii_lowercase().as_str() {
            "ru" | "ru-ru" => Some(Lang::Ru),
            "en" | "en-us" | "en-gb" => Some(Lang::En),
            _ => None,
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Lang::Ru => Lang::En,
            Lang::En => Lang::Ru,
        }
    }
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetUserDefaultUILanguage() -> u16;
}

/// Detects Windows UI language (`LANG_RUSSIAN` primary ID = 0x19 -> `Lang::Ru`, otherwise `Lang::En`).
pub fn detect_system_lang() -> Lang {
    #[cfg(windows)]
    unsafe {
        let lang_id = GetUserDefaultUILanguage();
        let primary = lang_id & 0x03FF;
        if primary == 0x19 {
            Lang::Ru
        } else {
            Lang::En
        }
    }
    #[cfg(not(windows))]
    {
        Lang::En
    }
}

pub fn format_monitor_label(lang: Lang, number: u32, is_primary: bool) -> String {
    match (lang, is_primary) {
        (Lang::Ru, true) => format!("Монитор {number} (Основной)"),
        (Lang::Ru, false) => format!("Монитор {number}"),
        (Lang::En, true) => format!("Monitor {number} (Primary)"),
        (Lang::En, false) => format!("Monitor {number}"),
    }
}

pub fn tray_tooltip(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Multi-Monitor Tray Dimmer — яркость экранов",
        Lang::En => "Multi-Monitor Tray Dimmer — screen brightness",
    }
}

pub fn btn_night(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Ночной режим",
        Lang::En => "Night Mode",
    }
}

pub fn btn_reset(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Сброс 100%",
        Lang::En => "Reset 100%",
    }
}

pub fn btn_refresh(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Обновить",
        Lang::En => "Refresh",
    }
}

pub fn btn_exit(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Выход",
        Lang::En => "Exit",
    }
}

pub fn autostart_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Загружать с Windows",
        Lang::En => "Start with Windows",
    }
}

pub fn update_check_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Обновления",
        Lang::En => "Updates",
    }
}

pub fn update_checking_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Проверка...",
        Lang::En => "Checking...",
    }
}

pub fn update_uptodate_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Актуально",
        Lang::En => "Up to date",
    }
}

pub fn update_available_label(lang: Lang, ver: &str) -> String {
    match lang {
        Lang::Ru => format!("Обновить v{ver}"),
        Lang::En => format!("Update v{ver}"),
    }
}

pub fn update_downloading_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Загрузка...",
        Lang::En => "Updating...",
    }
}

pub fn update_error_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Ошибка сети",
        Lang::En => "Network err",
    }
}

pub fn toast_brightness_set(lang: Lang, level: u8) -> String {
    match lang {
        Lang::Ru => format!("Яркость установлена: {level}%"),
        Lang::En => format!("Brightness set: {level}%"),
    }
}

pub fn toast_night_on(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Ночной режим включён",
        Lang::En => "Night mode enabled",
    }
}

pub fn toast_day_on(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Дневной режим (100%)",
        Lang::En => "Day mode (100%)",
    }
}

pub fn toast_all_monitors(lang: Lang, level: u8) -> String {
    match lang {
        Lang::Ru => format!("Все мониторы: {level}%"),
        Lang::En => format!("All monitors: {level}%"),
    }
}

pub fn toast_reset_title(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Яркость сброшена",
        Lang::En => "Brightness reset",
    }
}

pub fn toast_refreshed_title(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Мониторы обновлены",
        Lang::En => "Monitors refreshed",
    }
}

pub fn toast_refreshed_body(lang: Lang, count: usize) -> String {
    match lang {
        Lang::Ru => format!("Найдено экранов: {count}"),
        Lang::En => format!("Displays found: {count}"),
    }
}

pub fn toast_autostart(lang: Lang, enabled: bool) -> (&'static str, &'static str) {
    match (lang, enabled) {
        (Lang::Ru, true) => ("Автозапуск", "Включён: запуск вместе с Windows"),
        (Lang::Ru, false) => ("Автозапуск", "Отключён"),
        (Lang::En, true) => ("Autostart", "Enabled: starts with Windows"),
        (Lang::En, false) => ("Autostart", "Disabled"),
    }
}

pub fn sync_label(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Синхронизация всех экранов",
        Lang::En => "Sync all displays",
    }
}

pub fn all_monitors_card_label(lang: Lang, count: usize) -> String {
    match lang {
        Lang::Ru => format!("Все мониторы ({count})"),
        Lang::En => format!("All Monitors ({count})"),
    }
}

pub fn toast_sync(lang: Lang, enabled: bool) -> (&'static str, &'static str) {
    match (lang, enabled) {
        (Lang::Ru, true) => ("Синхронизация", "Включена: общий ползунок и Ctrl+Alt+←/→"),
        (Lang::Ru, false) => ("Синхронизация", "Отключена: каждый монитор отдельно"),
        (Lang::En, true) => ("Sync", "Enabled: unified slider & Ctrl+Alt+←/→"),
        (Lang::En, false) => ("Sync", "Disabled: per-monitor control"),
    }
}

pub fn btn_about(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Об утилите",
        Lang::En => "About",
    }
}

pub fn btn_back(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "← Назад",
        Lang::En => "← Back",
    }
}

pub fn btn_donate(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Поддержать (ЮMoney)",
        Lang::En => "Donate (YooMoney)",
    }
}

pub fn btn_github(_lang: Lang) -> &'static str {
    "GitHub"
}

pub fn about_heading(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Об утилите",
        Lang::En => "About",
    }
}

pub fn toast_active_monitor(lang: Lang, monitor_label: &str, level: u8) -> (&'static str, String) {
    match lang {
        Lang::Ru => ("Активный монитор", format!("{monitor_label} — {level}%")),
        Lang::En => ("Active monitor", format!("{monitor_label} — {level}%")),
    }
}

pub fn about_features(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => concat!(
            "Нативная утилита трея для плавной регулировки\n",
            "яркости экранов (DDC/CI + мягкий оверлей).\n\n",
            "Нажмите Ctrl+Alt и:\n",
            "• ↑ / ↓ — сменить активный монитор\n",
            "• ← / → — яркость (активный / все)\n",
            "• D — Ночной режим (40% ↔ 100%)"
        ),
        Lang::En => concat!(
            "Lightweight native tray utility for smooth\n",
            "multi-monitor dimming (DDC/CI + soft overlay).\n\n",
            "Press Ctrl+Alt and:\n",
            "• ↑ / ↓ — switch active monitor\n",
            "• ← / → — brightness (active / all)\n",
            "• D — Night Mode (40% ↔ 100%)"
        ),
    }
}

pub fn about_donate_text(lang: Lang) -> &'static str {
    match lang {
        Lang::Ru => "Утилита бесплатна и открыта (DiM-SoTiS).\nПоддержать развитие новых утилит:",
        Lang::En => "Free & open-source utility by DiM-SoTiS.\nSupport development of new tools:",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_toggles_lang() {
        assert_eq!(Lang::from_code("ru"), Some(Lang::Ru));
        assert_eq!(Lang::from_code("EN-US"), Some(Lang::En));
        assert_eq!(Lang::from_code("fr"), None);
        assert_eq!(Lang::Ru.toggle(), Lang::En);
        assert_eq!(Lang::En.toggle(), Lang::Ru);
    }

    #[test]
    fn formats_monitor_labels_in_both_languages() {
        assert_eq!(format_monitor_label(Lang::Ru, 1, true), "Монитор 1 (Основной)");
        assert_eq!(format_monitor_label(Lang::En, 2, false), "Monitor 2");
    }
}
