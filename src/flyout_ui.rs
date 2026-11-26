//! Native Win32 double-buffered Cosmic Glass HUD renderer for the tray flyout panel.

#![cfg(windows)]

use std::ffi::c_void;
use std::ptr::null;

use crate::flyout::{
    about_button_rects, brightness_to_x, cards_start_y, compute_flyout_height, footer_button_rects,
    lang_pill_rect, settings_row_rects, sync_row_rect, FooterAction, ABOUT_HEIGHT, CARD_GAP,
    CARD_HEIGHT, FLYOUT_WIDTH, HEADER_HEIGHT, PAD_X, SLIDER_LEFT, SLIDER_RIGHT,
};
use crate::i18n::{
    about_donate_text, about_features, about_heading, autostart_label, btn_about, btn_back,
    btn_donate, btn_exit, btn_github, btn_night, btn_refresh, btn_reset, sync_label,
    update_available_label, update_check_label, update_checking_label, update_downloading_label,
    update_error_label, update_uptodate_label, Lang,
};
use crate::updater::{current_version, UpdateState};
use crate::win32_monitors::{Hwnd, Rect};

type Hdc = *mut c_void;
type Hbrush = *mut c_void;
type Hfont = *mut c_void;
type Hbitmap = *mut c_void;

// Win32 COLORREF is 0x00_BB_GG_RR (Cosmic Glass HUD palette)
const COL_BG: u32 = 0x00_17_0E_0B; // #0B0E17 deep obsidian
const COL_HEADER_LINE: u32 = 0x00_36_25_1C; // subtle glass divider
const COL_CARD_BG: u32 = 0x00_26_18_12; // #121826 card surface
const COL_CARD_BORDER: u32 = 0x00_46_30_24; // #243046 glass hairline
const COL_BTN_BG: u32 = 0x00_2D_1E_16; // #161E2D button surface
const COL_BTN_EXIT_BG: u32 = 0x00_24_18_2B; // subtle dark crimson tint
const COL_TRACK_BG: u32 = 0x00_3A_27_1E; // #1E273A slider track
const COL_CYAN: u32 = 0x00_FF_F0_00; // #00F0FF Electric Cyan
const COL_AMBER: u32 = 0x00_00_99_FF; // #FF9900 Amber
const COL_EMERALD: u32 = 0x00_88_FF_00; // #00FF88 Emerald
const COL_CORAL: u32 = 0x00_78_55_FF; // #FF5578 Coral
const COL_TEXT: u32 = 0x00_FF_F4_F0; // #F0F4FF primary text
const COL_MUTED: u32 = 0x00_B5_9B_8E; // #8E9BB5 secondary text
const COL_DARK_TEXT: u32 = 0x00_14_0D_09; // dark text on bright pill

const DT_LEFT: u32 = 0x0000_0000;
const DT_CENTER: u32 = 0x0000_0001;
const DT_RIGHT: u32 = 0x0000_0002;
const DT_VCENTER: u32 = 0x0000_0004;
const DT_WORDBREAK: u32 = 0x0000_0010;
const DT_SINGLELINE: u32 = 0x0000_0020;
const DT_NOPREFIX: u32 = 0x0000_0800;
const DT_END_ELLIPSIS: u32 = 0x0000_8000;
const TRANSPARENT_BK: i32 = 1;
const SRCCOPY: u32 = 0x00CC_0020;

#[link(name = "user32")]
extern "system" {
    fn FillRect(hdc: Hdc, lprc: *const Rect, hbr: Hbrush) -> i32;
    fn DrawTextW(hdc: Hdc, lpch_text: *const u16, cch_text: i32, lprc: *mut Rect, format: u32)
        -> i32;
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateSolidBrush(color: u32) -> Hbrush;
    fn CreatePen(style: i32, width: i32, color: u32) -> *mut c_void;
    fn RoundRect(hdc: Hdc, left: i32, top: i32, right: i32, bottom: i32, w: i32, h: i32) -> i32;
    fn Ellipse(hdc: Hdc, left: i32, top: i32, right: i32, bottom: i32) -> i32;
    fn DeleteObject(ho: *mut c_void) -> i32;
    fn SetTextColor(hdc: Hdc, color: u32) -> u32;
    fn SetBkMode(hdc: Hdc, mode: i32) -> i32;
    fn SelectObject(hdc: Hdc, h: *mut c_void) -> *mut c_void;
    fn CreateFontW(
        h: i32,
        w: i32,
        esc: i32,
        ori: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strike: u32,
        charset: u32,
        out_prec: u32,
        clip_prec: u32,
        quality: u32,
        pitch: u32,
        face: *const u16,
    ) -> Hfont;
    fn CreateCompatibleDC(hdc: Hdc) -> Hdc;
    fn DeleteDC(hdc: Hdc) -> i32;
    fn CreateCompatibleBitmap(hdc: Hdc, cx: i32, cy: i32) -> Hbitmap;
    fn BitBlt(
        hdc: Hdc,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        hdc_src: Hdc,
        x1: i32,
        y1: i32,
        rop: u32,
    ) -> i32;
}

#[link(name = "shell32")]
extern "system" {
    fn ShellExecuteW(
        hwnd: Hwnd,
        lp_operation: *const u16,
        lp_file: *const u16,
        lp_parameters: *const u16,
        lp_directory: *const u16,
        n_show_cmd: i32,
    ) -> isize;
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn open_external_url(url: &str) {
    unsafe {
        let open = to_wide("open");
        let target = to_wide(url);
        ShellExecuteW(std::ptr::null_mut(), open.as_ptr(), target.as_ptr(), null(), null(), 5);
    }
}

pub struct SliderCardItem {
    pub label: String,
    pub brightness: u8,
    pub accent_amber: bool,
}

pub struct FlyoutViewModel<'a> {
    pub lang: Lang,
    pub sync_all: bool,
    pub autostart: bool,
    pub about_open: bool,
    pub monitor_count: usize,
    pub cards: Vec<SliderCardItem>,
    pub update_state: &'a UpdateState,
}

unsafe fn draw_round_box(
    hdc: Hdc,
    r: (i32, i32, i32, i32),
    radius: i32,
    fill: u32,
    border: u32,
) {
    let brush = CreateSolidBrush(fill);
    let pen = CreatePen(0, 1, border);
    let old_b = SelectObject(hdc, brush);
    let old_p = SelectObject(hdc, pen);
    RoundRect(hdc, r.0, r.1, r.2, r.3, radius, radius);
    SelectObject(hdc, old_b);
    SelectObject(hdc, old_p);
    DeleteObject(brush);
    DeleteObject(pen);
}

unsafe fn draw_text(hdc: Hdc, text: &str, r: (i32, i32, i32, i32), color: u32, flags: u32) {
    SetTextColor(hdc, color);
    let mut rect = Rect {
        left: r.0,
        top: r.1,
        right: r.2,
        bottom: r.3,
    };
    let w = to_wide(text);
    DrawTextW(hdc, w.as_ptr(), -1, &mut rect, flags | DT_NOPREFIX);
}

unsafe fn draw_checkbox(hdc: Hdc, r: (i32, i32, i32, i32), checked: bool, label: &str, accent: u32) {
    let box_size = 16;
    let cy = (r.1 + r.3) / 2;
    let bx = r.0 + 2;
    let by = cy - box_size / 2;
    let box_rect = (bx, by, bx + box_size, by + box_size);
    if checked {
        draw_round_box(hdc, box_rect, 5, accent, accent);
        draw_text(
            hdc,
            "✓",
            box_rect,
            COL_DARK_TEXT,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
    } else {
        draw_round_box(hdc, box_rect, 5, COL_CARD_BG, COL_CARD_BORDER);
    }
    draw_text(
        hdc,
        label,
        (bx + box_size + 8, r.1, r.2, r.3),
        COL_TEXT,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
}

pub unsafe fn paint_flyout(hdc_win: Hdc, vm: &FlyoutViewModel<'_>) {
    let width = FLYOUT_WIDTH;
    let height = compute_flyout_height(vm.monitor_count, vm.sync_all, vm.about_open);

    let mem_dc = CreateCompatibleDC(hdc_win);
    let mem_bmp = CreateCompatibleBitmap(hdc_win, width, height);
    let old_bmp = SelectObject(mem_dc, mem_bmp);

    let bg_brush = CreateSolidBrush(COL_BG);
    let full = Rect {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    FillRect(mem_dc, &full, bg_brush);
    DeleteObject(bg_brush);

    SetBkMode(mem_dc, TRANSPARENT_BK);
    let face = to_wide("Segoe UI");
    let font_title = CreateFontW(16, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr());
    let font_ui = CreateFontW(14, 0, 0, 0, 500, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr());
    let font_bold = CreateFontW(14, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr());
    let font_small = CreateFontW(13, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr());

    // Header
    let old_font = SelectObject(mem_dc, font_title);
    let header_title = if vm.about_open {
        about_heading(vm.lang)
    } else {
        "Tray Dimmer"
    };
    draw_text(
        mem_dc,
        header_title,
        (PAD_X, 8, width - PAD_X - 70, HEADER_HEIGHT - 6),
        COL_CYAN,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );

    // RU | EN pill
    let lang_r = lang_pill_rect();
    draw_round_box(mem_dc, lang_r, 10, COL_BTN_BG, COL_CYAN);
    SelectObject(mem_dc, font_bold);
    let lang_text = match vm.lang {
        Lang::Ru => "RU | en",
        Lang::En => "ru | EN",
    };
    draw_text(
        mem_dc,
        lang_text,
        lang_r,
        COL_TEXT,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    // Header separator line
    let div_brush = CreateSolidBrush(COL_HEADER_LINE);
    let div_rect = Rect {
        left: PAD_X,
        top: HEADER_HEIGHT - 1,
        right: width - PAD_X,
        bottom: HEADER_HEIGHT,
    };
    FillRect(mem_dc, &div_rect, div_brush);
    DeleteObject(div_brush);

    if vm.about_open {
        paint_about_body(mem_dc, vm, font_bold, font_ui, font_small);
    } else {
        paint_controls_body(mem_dc, vm, font_bold, font_ui, font_small);
    }

    SelectObject(mem_dc, old_font);
    DeleteObject(font_title);
    DeleteObject(font_ui);
    DeleteObject(font_bold);
    DeleteObject(font_small);

    BitBlt(hdc_win, 0, 0, width, height, mem_dc, 0, 0, SRCCOPY);
    SelectObject(mem_dc, old_bmp);
    DeleteObject(mem_bmp);
    DeleteDC(mem_dc);
}

unsafe fn paint_controls_body(
    hdc: Hdc,
    vm: &FlyoutViewModel<'_>,
    font_bold: Hfont,
    font_ui: Hfont,
    font_small: Hfont,
) {
    // 1. Sync checkbox row
    SelectObject(hdc, font_ui);
    draw_checkbox(hdc, sync_row_rect(), vm.sync_all, sync_label(vm.lang), COL_AMBER);

    // 2. Monitor slider cards
    let start_y = cards_start_y();
    for (idx, card) in vm.cards.iter().enumerate() {
        let top = start_y + (idx as i32) * (CARD_HEIGHT + CARD_GAP);
        let card_r = (PAD_X, top, FLYOUT_WIDTH - PAD_X, top + CARD_HEIGHT);
        let accent = if card.accent_amber { COL_AMBER } else { COL_CYAN };
        draw_round_box(hdc, card_r, 12, COL_CARD_BG, COL_CARD_BORDER);

        SelectObject(hdc, font_ui);
        draw_text(
            hdc,
            &card.label,
            (PAD_X + 12, top + 8, FLYOUT_WIDTH - PAD_X - 58, top + 28),
            COL_TEXT,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );

        SelectObject(hdc, font_bold);
        let pct_str = format!("{}%", card.brightness);
        draw_text(
            hdc,
            &pct_str,
            (FLYOUT_WIDTH - PAD_X - 56, top + 8, FLYOUT_WIDTH - PAD_X - 12, top + 28),
            accent,
            DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
        );

        // Slider track
        let track_y = top + 46;
        draw_round_box(
            hdc,
            (SLIDER_LEFT, track_y - 3, SLIDER_RIGHT, track_y + 3),
            6,
            COL_TRACK_BG,
            COL_TRACK_BG,
        );

        // Filled active track
        let thumb_x = brightness_to_x(card.brightness);
        if thumb_x > SLIDER_LEFT {
            draw_round_box(
                hdc,
                (SLIDER_LEFT, track_y - 3, thumb_x, track_y + 3),
                6,
                accent,
                accent,
            );
        }

        // Thumb circle
        let thumb_brush = CreateSolidBrush(COL_TEXT);
        let thumb_pen = CreatePen(0, 2, accent);
        let old_b = SelectObject(hdc, thumb_brush);
        let old_p = SelectObject(hdc, thumb_pen);
        Ellipse(hdc, thumb_x - 7, track_y - 7, thumb_x + 7, track_y + 7);
        SelectObject(hdc, old_b);
        SelectObject(hdc, old_p);
        DeleteObject(thumb_brush);
        DeleteObject(thumb_pen);
    }

    // 3. Settings row: Autostart checkbox + Update pill
    let (auto_r, upd_r) = settings_row_rects(vm.monitor_count, vm.sync_all);
    SelectObject(hdc, font_small);
    draw_checkbox(hdc, auto_r, vm.autostart, autostart_label(vm.lang), COL_CYAN);

    let (upd_text, upd_border, upd_col) = match vm.update_state {
        UpdateState::Idle => (update_check_label(vm.lang).to_string(), COL_CARD_BORDER, COL_MUTED),
        UpdateState::Checking => {
            (update_checking_label(vm.lang).to_string(), COL_CYAN, COL_CYAN)
        }
        UpdateState::UpToDate => {
            (update_uptodate_label(vm.lang).to_string(), COL_EMERALD, COL_EMERALD)
        }
        UpdateState::Available { version, .. } => (
            update_available_label(vm.lang, version),
            COL_AMBER,
            COL_AMBER,
        ),
        UpdateState::Downloading => {
            (update_downloading_label(vm.lang).to_string(), COL_AMBER, COL_AMBER)
        }
        UpdateState::Error => (update_error_label(vm.lang).to_string(), COL_CORAL, COL_CORAL),
    };
    draw_round_box(hdc, upd_r, 10, COL_BTN_BG, upd_border);
    draw_text(
        hdc,
        &upd_text,
        upd_r,
        upd_col,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );

    // 4. Footer buttons
    SelectObject(hdc, font_ui);
    for (action, r) in footer_button_rects(vm.monitor_count, vm.sync_all) {
        let (label, fill, border, text_col) = match action {
            FooterAction::NightMode => (btn_night(vm.lang), COL_BTN_BG, COL_AMBER, COL_TEXT),
            FooterAction::ResetAll => (btn_reset(vm.lang), COL_BTN_BG, COL_CYAN, COL_TEXT),
            FooterAction::RefreshMonitors => {
                (btn_refresh(vm.lang), COL_BTN_BG, COL_CARD_BORDER, COL_TEXT)
            }
            FooterAction::About => (btn_about(vm.lang), COL_BTN_BG, COL_CARD_BORDER, COL_CYAN),
            FooterAction::Exit => (btn_exit(vm.lang), COL_BTN_EXIT_BG, COL_CORAL, COL_CORAL),
        };
        draw_round_box(hdc, r, 10, fill, border);
        draw_text(
            hdc,
            label,
            r,
            text_col,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
    }
}

unsafe fn paint_about_body(
    hdc: Hdc,
    vm: &FlyoutViewModel<'_>,
    font_bold: Hfont,
    font_ui: Hfont,
    font_small: Hfont,
) {
    // Info card
    let info_r = (PAD_X, HEADER_HEIGHT + 10, FLYOUT_WIDTH - PAD_X, HEADER_HEIGHT + 150);
    draw_round_box(hdc, info_r, 12, COL_CARD_BG, COL_CARD_BORDER);

    SelectObject(hdc, font_bold);
    let ver_line = format!("Multi-Monitor Tray Dimmer v{}", current_version());
    draw_text(
        hdc,
        &ver_line,
        (PAD_X + 12, info_r.1 + 10, info_r.2 - 12, info_r.1 + 30),
        COL_AMBER,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );

    SelectObject(hdc, font_small);
    draw_text(
        hdc,
        about_features(vm.lang),
        (PAD_X + 12, info_r.1 + 34, info_r.2 - 12, info_r.3 - 8),
        COL_TEXT,
        DT_LEFT | DT_WORDBREAK,
    );

    // Donate card
    let donate_card_r = (PAD_X, info_r.3 + 10, FLYOUT_WIDTH - PAD_X, ABOUT_HEIGHT - 96);
    draw_round_box(hdc, donate_card_r, 12, COL_CARD_BG, COL_AMBER);
    SelectObject(hdc, font_small);
    draw_text(
        hdc,
        about_donate_text(vm.lang),
        (
            PAD_X + 12,
            donate_card_r.1 + 8,
            donate_card_r.2 - 12,
            donate_card_r.3 - 6,
        ),
        COL_MUTED,
        DT_LEFT | DT_WORDBREAK,
    );

    // Buttons: Donate, GitHub, Back
    let (donate_r, github_r, back_r) = about_button_rects();
    SelectObject(hdc, font_bold);
    draw_round_box(hdc, donate_r, 10, COL_AMBER, COL_AMBER);
    draw_text(
        hdc,
        btn_donate(vm.lang),
        donate_r,
        COL_DARK_TEXT,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    SelectObject(hdc, font_ui);
    draw_round_box(hdc, github_r, 10, COL_BTN_BG, COL_CYAN);
    draw_text(
        hdc,
        btn_github(vm.lang),
        github_r,
        COL_CYAN,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    draw_round_box(hdc, back_r, 10, COL_BTN_BG, COL_CARD_BORDER);
    draw_text(
        hdc,
        btn_back(vm.lang),
        back_r,
        COL_TEXT,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}
