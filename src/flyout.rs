//! Cosmic Glass HUD frosted-glass flyout panel geometry & hit-testing:
//! per-monitor or synced sliders, RU/EN switcher, autostart checkbox,
//! GitHub update pill, and bilingual About / Donate view.

use crate::dimmer::{MAX_BRIGHTNESS, MIN_BRIGHTNESS};

pub const FLYOUT_WIDTH: i32 = 368;
pub const HEADER_HEIGHT: i32 = 44;
pub const SYNC_ROW_HEIGHT: i32 = 32;
pub const CARD_HEIGHT: i32 = 68;
pub const CARD_GAP: i32 = 8;
pub const PAD_X: i32 = 14;
pub const SETTINGS_HEIGHT: i32 = 36;
pub const FOOTER_HEIGHT: i32 = 86;
pub const ABOUT_HEIGHT: i32 = 342;
pub const SLIDER_LEFT: i32 = PAD_X + 12;
pub const SLIDER_RIGHT: i32 = FLYOUT_WIDTH - PAD_X - 12;

/// Number of slider cards displayed: 1 unified card when `sync_all` is enabled, otherwise per monitor.
pub fn visible_card_count(monitor_count: usize, sync_all: bool) -> usize {
    if sync_all {
        1
    } else {
        monitor_count.max(1)
    }
}

/// Returns `(left, top, right, bottom)` for the `RU | EN` language switcher pill in the header.
pub fn lang_pill_rect() -> (i32, i32, i32, i32) {
    (FLYOUT_WIDTH - PAD_X - 60, 10, FLYOUT_WIDTH - PAD_X, 32)
}

/// Returns `(left, top, right, bottom)` for the `Sync all displays` toggle row below the header.
pub fn sync_row_rect() -> (i32, i32, i32, i32) {
    (PAD_X, HEADER_HEIGHT + 4, FLYOUT_WIDTH - PAD_X, HEADER_HEIGHT + SYNC_ROW_HEIGHT)
}

/// Top Y coordinate where monitor slider cards begin.
pub fn cards_start_y() -> i32 {
    HEADER_HEIGHT + SYNC_ROW_HEIGHT + 6
}

/// Computes total flyout window height for normal or About view.
pub fn compute_flyout_height(monitor_count: usize, sync_all: bool, about_open: bool) -> i32 {
    if about_open {
        return ABOUT_HEIGHT;
    }
    let count = visible_card_count(monitor_count, sync_all) as i32;
    cards_start_y() + count * (CARD_HEIGHT + CARD_GAP) + SETTINGS_HEIGHT + FOOTER_HEIGHT
}

/// Converts a client X coordinate inside the slider track into a clamped brightness (`20..=100`).
pub fn x_to_brightness(x: i32) -> u8 {
    let span = (SLIDER_RIGHT - SLIDER_LEFT).max(1);
    let clamped = (x - SLIDER_LEFT).clamp(0, span);
    let ratio = clamped as f32 / span as f32;
    let raw = MIN_BRIGHTNESS as f32 + ratio * (MAX_BRIGHTNESS - MIN_BRIGHTNESS) as f32;
    raw.round().clamp(MIN_BRIGHTNESS as f32, MAX_BRIGHTNESS as f32) as u8
}

/// Converts a brightness value (`20..=100`) to the slider thumb X position.
pub fn brightness_to_x(brightness: u8) -> i32 {
    let clamped = brightness.clamp(MIN_BRIGHTNESS, MAX_BRIGHTNESS);
    let ratio = (clamped - MIN_BRIGHTNESS) as f32 / (MAX_BRIGHTNESS - MIN_BRIGHTNESS) as f32;
    SLIDER_LEFT + (ratio * (SLIDER_RIGHT - SLIDER_LEFT) as f32).round() as i32
}

/// Returns the visible card index if `(x, y)` hits a slider row (in normal view).
pub fn hit_test_slider(x: i32, y: i32, monitor_count: usize, sync_all: bool) -> Option<usize> {
    if x < SLIDER_LEFT - 10 || x > SLIDER_RIGHT + 10 {
        return None;
    }
    let count = visible_card_count(monitor_count, sync_all);
    let start_y = cards_start_y();
    for idx in 0..count {
        let card_top = start_y + (idx as i32) * (CARD_HEIGHT + CARD_GAP);
        let slider_top = card_top + 30;
        let slider_bottom = card_top + CARD_HEIGHT - 4;
        if y >= slider_top && y <= slider_bottom {
            return Some(idx);
        }
    }
    None
}

/// Returns `(autostart_rect, update_pill_rect)` for the settings row above the footer.
pub fn settings_row_rects(
    monitor_count: usize,
    sync_all: bool,
) -> ((i32, i32, i32, i32), (i32, i32, i32, i32)) {
    let total_h = compute_flyout_height(monitor_count, sync_all, false);
    let top = total_h - FOOTER_HEIGHT - SETTINGS_HEIGHT + 2;
    let bottom = top + 28;
    let update_left = FLYOUT_WIDTH - PAD_X - 118;
    let autostart_rect = (PAD_X, top, update_left - 8, bottom);
    let update_rect = (update_left, top, FLYOUT_WIDTH - PAD_X, bottom);
    (autostart_rect, update_rect)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlyoutControl {
    LangToggle,
    SyncToggle,
    AutostartToggle,
    UpdateAction,
    Footer(FooterAction),
    AboutBack,
    AboutDonate,
    AboutGithub,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FooterAction {
    NightMode,
    ResetAll,
    RefreshMonitors,
    About,
    Exit,
}

/// Returns the footer button rectangles:
/// Row 1: NightMode, ResetAll (2 columns)
/// Row 2: RefreshMonitors, About, Exit (3 columns)
pub fn footer_button_rects(
    monitor_count: usize,
    sync_all: bool,
) -> [(FooterAction, (i32, i32, i32, i32)); 5] {
    let total_h = compute_flyout_height(monitor_count, sync_all, false);
    let top1 = total_h - FOOTER_HEIGHT + 6;
    let btn_h = 32;
    let gap = 8;
    let inner_w = FLYOUT_WIDTH - PAD_X * 2;
    let half_w = (inner_w - gap) / 2;
    let third_w = (inner_w - gap * 2) / 3;
    let x1 = PAD_X;
    let x2 = PAD_X + half_w + gap;
    let top2 = top1 + btn_h + gap;
    let tx2 = PAD_X + third_w + gap;
    let tx3 = PAD_X + (third_w + gap) * 2;

    [
        (FooterAction::NightMode, (x1, top1, x1 + half_w, top1 + btn_h)),
        (FooterAction::ResetAll, (x2, top1, FLYOUT_WIDTH - PAD_X, top1 + btn_h)),
        (FooterAction::RefreshMonitors, (x1, top2, x1 + third_w, top2 + btn_h)),
        (FooterAction::About, (tx2, top2, tx2 + third_w, top2 + btn_h)),
        (FooterAction::Exit, (tx3, top2, FLYOUT_WIDTH - PAD_X, top2 + btn_h)),
    ]
}

/// Returns `(donate_rect, github_rect, back_rect)` for the About view.
pub fn about_button_rects() -> ((i32, i32, i32, i32), (i32, i32, i32, i32), (i32, i32, i32, i32)) {
    let btn_h = 34;
    let gap = 8;
    let y1 = ABOUT_HEIGHT - 86;
    let y2 = y1 + btn_h + gap;
    let inner_w = FLYOUT_WIDTH - PAD_X * 2;
    let donate_w = (inner_w * 2) / 3 - 4;
    let donate = (PAD_X, y1, PAD_X + donate_w, y1 + btn_h);
    let github = (PAD_X + donate_w + gap, y1, FLYOUT_WIDTH - PAD_X, y1 + btn_h);
    let back = (PAD_X, y2, FLYOUT_WIDTH - PAD_X, y2 + btn_h);
    (donate, github, back)
}

fn in_rect(x: i32, y: i32, r: (i32, i32, i32, i32)) -> bool {
    x >= r.0 && x <= r.2 && y >= r.1 && y <= r.3
}

/// Hit-tests all clickable controls in the flyout (except sliders).
pub fn hit_test_control(
    x: i32,
    y: i32,
    monitor_count: usize,
    sync_all: bool,
    about_open: bool,
) -> Option<FlyoutControl> {
    if in_rect(x, y, lang_pill_rect()) {
        return Some(FlyoutControl::LangToggle);
    }
    if about_open {
        let (donate, github, back) = about_button_rects();
        if in_rect(x, y, donate) {
            return Some(FlyoutControl::AboutDonate);
        }
        if in_rect(x, y, github) {
            return Some(FlyoutControl::AboutGithub);
        }
        if in_rect(x, y, back) {
            return Some(FlyoutControl::AboutBack);
        }
        return None;
    }
    if in_rect(x, y, sync_row_rect()) {
        return Some(FlyoutControl::SyncToggle);
    }
    let (auto_r, upd_r) = settings_row_rects(monitor_count, sync_all);
    if in_rect(x, y, auto_r) {
        return Some(FlyoutControl::AutostartToggle);
    }
    if in_rect(x, y, upd_r) {
        return Some(FlyoutControl::UpdateAction);
    }
    for (act, r) in footer_button_rects(monitor_count, sync_all) {
        if in_rect(x, y, r) {
            return Some(FlyoutControl::Footer(act));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_slider_x_to_brightness_and_back() {
        assert_eq!(x_to_brightness(SLIDER_LEFT), 20);
        assert_eq!(x_to_brightness(SLIDER_RIGHT), 100);
        assert_eq!(x_to_brightness(SLIDER_LEFT - 50), 20);
        assert_eq!(x_to_brightness(SLIDER_RIGHT + 50), 100);
        let mid_x = brightness_to_x(60);
        assert_eq!(x_to_brightness(mid_x), 60);
    }

    #[test]
    fn sync_mode_collapses_cards_to_one() {
        assert_eq!(visible_card_count(3, false), 3);
        assert_eq!(visible_card_count(3, true), 1);
        assert!(
            compute_flyout_height(3, true, false) < compute_flyout_height(3, false, false)
        );
    }

    #[test]
    fn hit_tests_sliders_sync_and_about_controls() {
        let mid_x = (SLIDER_LEFT + SLIDER_RIGHT) / 2;
        assert_eq!(hit_test_slider(mid_x, cards_start_y() + 44, 3, false), Some(0));
        assert_eq!(
            hit_test_slider(mid_x, cards_start_y() + (CARD_HEIGHT + CARD_GAP) + 44, 3, false),
            Some(1)
        );
        assert_eq!(
            hit_test_slider(mid_x, cards_start_y() + (CARD_HEIGHT + CARD_GAP) + 44, 3, true),
            None
        );

        let sr = sync_row_rect();
        assert_eq!(
            hit_test_control((sr.0 + sr.2) / 2, (sr.1 + sr.3) / 2, 3, false, false),
            Some(FlyoutControl::SyncToggle)
        );

        let (donate, github, back) = about_button_rects();
        assert_eq!(
            hit_test_control((donate.0 + donate.2) / 2, (donate.1 + donate.3) / 2, 3, false, true),
            Some(FlyoutControl::AboutDonate)
        );
        assert_eq!(
            hit_test_control((github.0 + github.2) / 2, (github.1 + github.3) / 2, 3, false, true),
            Some(FlyoutControl::AboutGithub)
        );
        assert_eq!(
            hit_test_control((back.0 + back.2) / 2, (back.1 + back.3) / 2, 3, false, true),
            Some(FlyoutControl::AboutBack)
        );
    }
}
