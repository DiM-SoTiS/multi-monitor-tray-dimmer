//! Pure brightness calculation, overlay alpha mapping, and persistent config.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::i18n::Lang;

pub const MIN_BRIGHTNESS: u8 = 20;
pub const MAX_BRIGHTNESS: u8 = 100;
pub const DEFAULT_BRIGHTNESS: u8 = 100;
pub const NIGHT_BRIGHTNESS: u8 = 40;
pub const BRIGHTNESS_STEP: i16 = 10;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppConfig {
    pub levels: BTreeMap<String, u8>,
    pub lang: Option<Lang>,
    pub sync_all: bool,
}

/// Clamps brightness percentage into `[20, 100]`.
pub fn clamp_brightness(value: i16) -> u8 {
    value.clamp(MIN_BRIGHTNESS as i16, MAX_BRIGHTNESS as i16) as u8
}

/// Adjusts brightness by `delta` (e.g. +10 or -10), clamped to `[20, 100]`.
pub fn step_brightness(current: u8, delta: i16) -> u8 {
    clamp_brightness(current as i16 + delta)
}

/// Converts brightness percentage (`20..=100`) into black overlay alpha (`0..=204`).
/// `100%` -> `0` (fully transparent, overlay hidden), `20%` -> `204` (80% darkened).
pub fn brightness_to_overlay_alpha(brightness: u8) -> u8 {
    let clamped = clamp_brightness(brightness as i16);
    let darkness = (MAX_BRIGHTNESS - clamped) as u16;
    ((darkness * 255) / 100) as u8
}

/// Toggles between normal (`100%`) and night mode (`40%`).
pub fn toggle_night_mode(current: u8) -> u8 {
    if current <= NIGHT_BRIGHTNESS + 5 {
        MAX_BRIGHTNESS
    } else {
        NIGHT_BRIGHTNESS
    }
}

pub fn config_file_path() -> Option<PathBuf> {
    env::var("APPDATA")
        .ok()
        .map(|dir| PathBuf::from(dir).join("multi-monitor-tray-dimmer").join("config.ini"))
}

pub fn parse_config(raw: &str) -> AppConfig {
    let mut cfg = AppConfig::default();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = trimmed.split_once('=') {
            let k = key.trim();
            let v = val.trim();
            if k.eq_ignore_ascii_case("lang") {
                cfg.lang = Lang::from_code(v);
            } else if k.eq_ignore_ascii_case("sync") {
                cfg.sync_all = matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on");
            } else if let Ok(pct) = v.parse::<i16>() {
                cfg.levels.insert(k.to_string(), clamp_brightness(pct));
            }
        }
    }
    cfg
}

pub fn serialize_config(cfg: &AppConfig) -> String {
    let mut out = String::new();
    if let Some(lang) = cfg.lang {
        out.push_str(&format!("lang={}\n", lang.as_code()));
    }
    out.push_str(&format!("sync={}\n", if cfg.sync_all { 1 } else { 0 }));
    for (k, v) in &cfg.levels {
        out.push_str(&format!("{k}={}\n", clamp_brightness(*v as i16)));
    }
    out
}

pub fn load_config(path: &Path) -> AppConfig {
    fs::read_to_string(path)
        .map(|s| parse_config(&s))
        .unwrap_or_default()
}

pub fn save_config(path: &Path, cfg: &AppConfig) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, serialize_config(cfg));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_and_steps_brightness() {
        assert_eq!(clamp_brightness(150), 100);
        assert_eq!(clamp_brightness(5), 20);
        assert_eq!(step_brightness(100, -10), 90);
        assert_eq!(step_brightness(25, -10), 20);
        assert_eq!(step_brightness(95, 10), 100);
    }

    #[test]
    fn maps_brightness_to_overlay_alpha() {
        assert_eq!(brightness_to_overlay_alpha(100), 0);
        assert_eq!(brightness_to_overlay_alpha(60), 102);
        assert_eq!(brightness_to_overlay_alpha(20), 204);
    }

    #[test]
    fn toggles_night_mode_cleanly() {
        assert_eq!(toggle_night_mode(100), 40);
        assert_eq!(toggle_night_mode(40), 100);
        assert_eq!(toggle_night_mode(30), 100);
    }

    #[test]
    fn roundtrips_monitor_config_and_lang() {
        let mut cfg = AppConfig::default();
        cfg.lang = Some(Lang::En);
        cfg.sync_all = true;
        cfg.levels.insert("DISPLAY1".to_string(), 70);
        cfg.levels.insert("DISPLAY2".to_string(), 40);
        let serialized = serialize_config(&cfg);
        let parsed = parse_config(&serialized);
        assert_eq!(parsed.lang, Some(Lang::En));
        assert!(parsed.sync_all);
        assert_eq!(parsed.levels.get("DISPLAY1"), Some(&70));
        assert_eq!(parsed.levels.get("DISPLAY2"), Some(&40));
    }
}
