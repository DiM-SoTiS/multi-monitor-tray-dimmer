//! Native Win32 system tray icon, Cosmic Glass HUD flyout, global hotkeys, and dark toast popup.

#![cfg(windows)]

use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use crate::autostart::{is_autostart_enabled, set_autostart_enabled};
use crate::dimmer::{
    config_file_path, cycle_monitor_index, load_config, save_config, step_brightness,
    toggle_night_mode, AppConfig, BRIGHTNESS_STEP, DEFAULT_BRIGHTNESS, MAX_BRIGHTNESS,
};
use crate::flyout::{
    compute_flyout_height, hit_test_control, hit_test_slider, x_to_brightness, FlyoutControl,
    FooterAction, FLYOUT_WIDTH,
};
use crate::flyout_ui::{open_external_url, paint_flyout, FlyoutViewModel, SliderCardItem};
use crate::i18n::{
    all_monitors_card_label, detect_system_lang, toast_active_monitor, toast_all_monitors,
    toast_autostart, toast_brightness_set, toast_day_on, toast_night_on, toast_refreshed_body,
    toast_refreshed_title, toast_reset_title, toast_sync, tray_tooltip, Lang,
};
use crate::updater::{check_latest_release, download_and_spawn_replace, UpdateState};
use crate::win32_monitors::{
    apply_overlay_only, create_overlay_window, destroy_overlay_windows, enumerate_monitors,
    DdcWorker, Hwnd, MonitorEntry, Rect,
};

const DONATE_URL: &str = "https://yoomoney.ru/fundraise/1LP6PQKIMJN.261125";
const GITHUB_REPO_URL: &str = "https://github.com/DiM-SoTiS/multi-monitor-tray-dimmer";

const WM_APP_TRAY: u32 = 0x8000 + 1;
const WM_APP_UPDATE_RESULT: u32 = 0x8000 + 2;
const WM_DESTROY: u32 = 0x0002;
const WM_ACTIVATE: u32 = 0x0006;
const WM_PAINT: u32 = 0x000F;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_KEYDOWN: u32 = 0x0100;
const WM_TIMER: u32 = 0x0113;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_LBUTTONDBLCLK: u32 = 0x0203;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_HOTKEY: u32 = 0x0312;

const WA_INACTIVE: usize = 0;
const VK_ESCAPE: usize = 0x1B;

const HOTKEY_BRIGHT_UP: usize = 1;
const HOTKEY_BRIGHT_DOWN: usize = 2;
const HOTKEY_NIGHT_TOGGLE: usize = 3;
const HOTKEY_SYNC_DOWN: usize = 4;
const HOTKEY_SYNC_UP: usize = 5;
const MOD_ALT: u32 = 0x0001;
const MOD_CONTROL: u32 = 0x0002;
const MOD_NOREPEAT: u32 = 0x4000;
const VK_LEFT: u32 = 0x25;
const VK_UP: u32 = 0x26;
const VK_RIGHT: u32 = 0x27;
const VK_DOWN: u32 = 0x28;
const VK_D: u32 = 0x44;

const TOAST_TIMER_ID: usize = 1;
const TOAST_DURATION_MS: u32 = 2500;

const NIM_ADD: u32 = 0x00000000;
const NIM_DELETE: u32 = 0x00000002;
const NIF_MESSAGE: u32 = 0x00000001;
const NIF_ICON: u32 = 0x00000002;
const NIF_TIP: u32 = 0x00000004;

const WS_POPUP: u32 = 0x80000000;
const WS_EX_TOPMOST: u32 = 0x00000008;
const WS_EX_TOOLWINDOW: u32 = 0x00000080;
const WS_EX_NOACTIVATE: u32 = 0x08000000;
const SW_HIDE: i32 = 0;
const SW_SHOWNOACTIVATE: i32 = 4;
const SW_SHOW: i32 = 5;

const SPI_GETWORKAREA: u32 = 0x0030;
const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWA_BORDER_COLOR: u32 = 34;
const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
const DWMWCP_ROUND: u32 = 2;
const DWMSBT_TRANSIENTWINDOW: u32 = 3;

const DT_LEFT: u32 = 0x00000000;
const DT_NOPREFIX: u32 = 0x00000800;
const DT_END_ELLIPSIS: u32 = 0x00008000;
const TRANSPARENT_BK: i32 = 1;

type Hinstance = *mut c_void;
type Hicon = *mut c_void;
type Hmenu = *mut c_void;
type Hbrush = *mut c_void;
type Hdc = *mut c_void;
type Hfont = *mut c_void;
type Hbitmap = *mut c_void;

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    w_param: usize,
    l_param: isize,
    time: u32,
    pt: Point,
}

#[repr(C)]
struct WndClassW {
    style: u32,
    lpfn_wnd_proc: extern "system" fn(Hwnd, u32, usize, isize) -> isize,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: Hinstance,
    h_icon: Hicon,
    h_cursor: *mut c_void,
    hbr_background: Hbrush,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
}

#[repr(C)]
struct NotifyIconDataW {
    cb_size: u32,
    h_wnd: Hwnd,
    u_id: u32,
    u_flags: u32,
    u_callback_message: u32,
    h_icon: Hicon,
    sz_tip: [u16; 128],
    dw_state: u32,
    dw_state_mask: u32,
    sz_info: [u16; 256],
    u_timeout_or_version: u32,
    sz_info_title: [u16; 64],
    dw_info_flags: u32,
}

#[repr(C)]
struct PaintStruct {
    hdc: Hdc,
    f_erase: i32,
    rc_paint: Rect,
    f_restore: i32,
    f_inc_update: i32,
    rgb_reserved: [u8; 32],
}

#[repr(C)]
struct IconInfo {
    f_icon: i32,
    x_hotspot: u32,
    y_hotspot: u32,
    hbm_mask: Hbitmap,
    hbm_color: Hbitmap,
}

const ERROR_ALREADY_EXISTS: u32 = 183;

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(lp_module_name: *const u16) -> Hinstance;
    fn CreateMutexW(
        lp_mutex_attributes: *mut c_void,
        b_initial_owner: i32,
        lp_name: *const u16,
    ) -> *mut c_void;
    fn GetLastError() -> u32;
    fn CloseHandle(h_object: *mut c_void) -> i32;
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassW(lp_wnd_class: *const WndClassW) -> u16;
    fn CreateWindowExW(
        dw_ex_style: u32,
        lp_class_name: *const u16,
        lp_window_name: *const u16,
        dw_style: u32,
        x: i32,
        y: i32,
        n_width: i32,
        n_height: i32,
        h_wnd_parent: Hwnd,
        h_menu: Hmenu,
        h_instance: Hinstance,
        lp_param: *mut c_void,
    ) -> Hwnd;
    fn DefWindowProcW(h_wnd: Hwnd, msg: u32, w_param: usize, l_param: isize) -> isize;
    fn GetMessageW(lp_msg: *mut Msg, h_wnd: Hwnd, min: u32, max: u32) -> i32;
    fn TranslateMessage(lp_msg: *const Msg) -> i32;
    fn DispatchMessageW(lp_msg: *const Msg) -> isize;
    fn PostMessageW(h_wnd: Hwnd, msg: u32, w_param: usize, l_param: isize) -> i32;
    fn PostQuitMessage(n_exit_code: i32);
    fn ShowWindow(h_wnd: Hwnd, n_cmd_show: i32) -> i32;
    fn IsWindowVisible(h_wnd: Hwnd) -> i32;
    fn InvalidateRect(h_wnd: Hwnd, lp_rect: *const Rect, b_erase: i32) -> i32;
    fn SetTimer(h_wnd: Hwnd, id: usize, elapse: u32, func: *const c_void) -> usize;
    fn KillTimer(h_wnd: Hwnd, id: usize) -> i32;
    fn SetForegroundWindow(h_wnd: Hwnd) -> i32;
    fn SetCapture(h_wnd: Hwnd) -> Hwnd;
    fn ReleaseCapture() -> i32;
    fn SystemParametersInfoW(ui_action: u32, ui_param: u32, pv_param: *mut c_void, f_win_ini: u32) -> i32;
    fn BeginPaint(h_wnd: Hwnd, lp_paint: *mut PaintStruct) -> Hdc;
    fn EndPaint(h_wnd: Hwnd, lp_paint: *const PaintStruct) -> i32;
    fn FillRect(hdc: Hdc, lprc: *const Rect, hbr: Hbrush) -> i32;
    fn FrameRect(hdc: Hdc, lprc: *const Rect, hbr: Hbrush) -> i32;
    fn DrawTextW(hdc: Hdc, lpch_text: *const u16, cch_text: i32, lprc: *mut Rect, format: u32) -> i32;
    fn CreateIconIndirect(piconinfo: *const IconInfo) -> Hicon;
    fn DestroyIcon(h_icon: Hicon) -> i32;
    fn SetWindowLongPtrW(h_wnd: Hwnd, n_index: i32, dw_new_long: isize) -> isize;
    fn GetWindowLongPtrW(h_wnd: Hwnd, n_index: i32) -> isize;
    fn RegisterHotKey(h_wnd: Hwnd, id: i32, fs_modifiers: u32, vk: u32) -> i32;
    fn UnregisterHotKey(h_wnd: Hwnd, id: i32) -> i32;
    fn SetWindowPos(
        h_wnd: Hwnd,
        h_wnd_insert_after: Hwnd,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        u_flags: u32,
    ) -> i32;
    fn SetProcessDpiAwarenessContext(value: isize) -> i32;
    fn SetProcessDPIAware() -> i32;
    fn RegisterWindowMessageW(lp_string: *const u16) -> u32;
}

unsafe fn enable_per_monitor_dpi_awareness() {
    // DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2 = -4
    if SetProcessDpiAwarenessContext(-4) == 0 {
        let _ = SetProcessDPIAware();
    }
}

#[link(name = "gdi32")]
extern "system" {
    fn CreateSolidBrush(color: u32) -> Hbrush;
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
    fn CreateBitmap(w: i32, h: i32, planes: u32, bpp: u32, bits: *const c_void) -> Hbitmap;
}

#[link(name = "shell32")]
extern "system" {
    fn Shell_NotifyIconW(dw_message: u32, lp_data: *const NotifyIconDataW) -> i32;
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(hwnd: Hwnd, dw_attribute: u32, pv_attribute: *const c_void, cb_attribute: u32) -> i32;
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Generates the Telemetry Sigil Tray Version (32x32, 4x supersampled, transparent background):
/// Overlapping dual monitors — rear monitor in Electric Cyan (#00F0FF), front monitor in Amber (#FF9900)
/// with rising brightness bars inside and a clean monitor stand.
unsafe fn create_tray_icon() -> Hicon {
    let w = 32usize;
    let h = 32usize;
    let mut pixels = vec![0u32; w * h];

    let blend = |bg: (f32, f32, f32, f32), fg: (f32, f32, f32, f32)| -> (f32, f32, f32, f32) {
        let a = fg.3 + bg.3 * (1.0 - fg.3);
        if a <= 0.0001 {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let r = (fg.0 * fg.3 + bg.0 * bg.3 * (1.0 - fg.3)) / a;
        let g = (fg.1 * fg.3 + bg.1 * bg.3 * (1.0 - fg.3)) / a;
        let b = (fg.2 * fg.3 + bg.2 * bg.3 * (1.0 - fg.3)) / a;
        (r, g, b, a)
    };

    // Signed distance to a rounded box centered at (cx, cy) with half-extents (hx, hy) and corner radius rad
    let sd_rbox = |px: f32, py: f32, cx: f32, cy: f32, hx: f32, hy: f32, rad: f32| -> f32 {
        let qx = (px - cx).abs() - (hx - rad);
        let qy = (py - cy).abs() - (hy - rad);
        let ox = qx.max(0.0);
        let oy = qy.max(0.0);
        (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - rad
    };

    for y in 0..h {
        for x in 0..w {
            let mut acc = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for sy in 0..4 {
                for sx in 0..4 {
                    let px = x as f32 + (sx as f32 + 0.5) * 0.25;
                    let py = y as f32 + (sy as f32 + 0.5) * 0.25;
                    let mut col = (0.0f32, 0.0f32, 0.0f32, 0.0f32); // 100% transparent tray background

                    // 1. Rear monitor (Electric Cyan #00F0FF) — top-right: x in [11..30], y in [4..19]
                    let d_rear = sd_rbox(px, py, 20.5, 11.5, 9.5, 7.2, 2.2);
                    if d_rear <= 0.0 && d_rear >= -2.5 {
                        col = blend(col, (0.0, 240.0, 255.0, 1.0));
                    }
                    // Rear monitor small foot indicator
                    if (20.5..=24.0).contains(&px) && (19.6..=22.0).contains(&py) {
                        col = blend(col, (0.0, 240.0, 255.0, 1.0));
                    }

                    // 2. Front monitor (Amber #FF9900) — bottom-left: x in [2..23], y in [10..25]
                    let d_front = sd_rbox(px, py, 12.5, 17.5, 10.2, 7.2, 2.2);
                    if d_front <= 0.0 {
                        // Dark translucent interior so it cleanly occludes the rear cyan monitor
                        col = (14.0, 14.0, 18.0, 0.94);

                        // Rising stepped brightness bars inside front screen
                        if (5.0..=20.0).contains(&px) && (13.0..=22.0).contains(&py) {
                            let t = (px - 5.0) / 15.0; // 0.0 .. 1.0
                            let bar_top = 22.0 - t * 8.2;
                            if py >= bar_top {
                                // Stepped vertical brightness gradient (brighter on left/bottom, amber glow)
                                let step = ((px - 5.0) / 3.75).floor();
                                let intensity = 0.45 + (3.0 - step).clamp(0.0, 3.0) * 0.18;
                                col = blend(col, (255.0, 153.0, 0.0, intensity));
                            } else {
                                col = blend(col, (255.0, 153.0, 0.0, 0.14));
                            }
                        }

                        // Bold amber frame stroke
                        if d_front >= -2.5 {
                            col = (255.0, 153.0, 0.0, 1.0);
                        }
                    }

                    // Front monitor neck & base stand in Amber
                    if ((10.6..=14.4).contains(&px) && (24.5..=27.2).contains(&py))
                        || ((7.2..=17.8).contains(&px) && (26.6..=29.2).contains(&py))
                    {
                        col = (255.0, 153.0, 0.0, 1.0);
                    }

                    acc.0 += col.0 * col.3;
                    acc.1 += col.1 * col.3;
                    acc.2 += col.2 * col.3;
                    acc.3 += col.3;
                }
            }
            let a = (acc.3 / 16.0).clamp(0.0, 1.0);
            if a > 0.001 {
                let r = (acc.0 / acc.3).clamp(0.0, 255.0) as u32;
                let g = (acc.1 / acc.3).clamp(0.0, 255.0) as u32;
                let b = (acc.2 / acc.3).clamp(0.0, 255.0) as u32;
                let au = (a * 255.0) as u32;
                pixels[y * w + x] = (au << 24) | (r << 16) | (g << 8) | b;
            }
        }
    }

    let mask = vec![0u8; (w * h) / 8];
    let hbm_color = CreateBitmap(w as i32, h as i32, 1, 32, pixels.as_ptr() as *const c_void);
    let hbm_mask = CreateBitmap(w as i32, h as i32, 1, 1, mask.as_ptr() as *const c_void);
    let info = IconInfo {
        f_icon: 1,
        x_hotspot: 0,
        y_hotspot: 0,
        hbm_mask,
        hbm_color,
    };
    let icon = CreateIconIndirect(&info);
    DeleteObject(hbm_color);
    DeleteObject(hbm_mask);
    icon
}

struct AppState {
    h_inst: Hinstance,
    main_hwnd: Hwnd,
    flyout_hwnd: Hwnd,
    overlay_class: Vec<u16>,
    monitors: Vec<MonitorEntry>,
    selected_monitor: usize,
    taskbar_created_msg: u32,
    tray_icon: Hicon,
    config: AppConfig,
    lang: Lang,
    autostart: bool,
    about_open: bool,
    ddc: DdcWorker,
    dragging_monitor: Option<usize>,
    update_state: UpdateState,
    update_rx: Option<Receiver<UpdateState>>,
    toast_hwnd: Hwnd,
    toast_title: String,
    toast_body: String,
}

unsafe fn build_notify_icon_data(hwnd: Hwnd, icon: Hicon, lang: Lang) -> NotifyIconDataW {
    let mut tip = [0u16; 128];
    let tip_w = to_wide(tray_tooltip(lang));
    let n = tip_w.len().min(tip.len() - 1);
    tip[..n].copy_from_slice(&tip_w[..n]);

    NotifyIconDataW {
        cb_size: std::mem::size_of::<NotifyIconDataW>() as u32,
        h_wnd: hwnd,
        u_id: 1,
        u_flags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        u_callback_message: WM_APP_TRAY,
        h_icon: icon,
        sz_tip: tip,
        dw_state: 0,
        dw_state_mask: 0,
        sz_info: [0; 256],
        u_timeout_or_version: 0,
        sz_info_title: [0; 64],
        dw_info_flags: 0,
    }
}

impl AppState {
    fn save_cfg(&self) {
        if let Some(cfg_path) = config_file_path() {
            save_config(&cfg_path, &self.config);
        }
    }

    fn refresh_monitors(&mut self) {
        destroy_overlay_windows(&mut self.monitors);
        self.monitors = enumerate_monitors();
        let ddc = &self.ddc;
        for m in &mut self.monitors {
            m.overlay_hwnd = create_overlay_window(self.h_inst, &self.overlay_class, m.rect);
            let level = *self.config.levels.get(&m.id).unwrap_or(&DEFAULT_BRIGHTNESS);
            self.config.levels.entry(m.id.clone()).or_insert(level);
            apply_overlay_only(m, level);
            ddc.send(m, level);
        }
        if self.selected_monitor >= self.monitors.len() {
            self.selected_monitor = self
                .monitors
                .iter()
                .position(|m| m.is_primary)
                .unwrap_or(0);
        }
    }

    fn set_monitor_level(&mut self, idx: usize, level: u8, notify: bool, send_ddc: bool) {
        let lang = self.lang;
        let label = if let Some(m) = self.monitors.get_mut(idx) {
            self.config.levels.insert(m.id.clone(), level);
            apply_overlay_only(m, level);
            if send_ddc {
                self.ddc.send(m, level);
            }
            Some(m.display_label(lang))
        } else {
            None
        };
        if label.is_some() {
            self.save_cfg();
        }
        if notify {
            if let Some(lbl) = label {
                self.show_toast(&lbl, &toast_brightness_set(lang, level));
            }
        }
    }

    fn set_all_levels(&mut self, level: u8, title: &str) {
        self.set_all_levels_live(level, true, true);
        self.show_toast(title, &toast_all_monitors(self.lang, level));
    }

    fn set_all_levels_live(&mut self, level: u8, send_ddc: bool, persist: bool) {
        for m in &mut self.monitors {
            self.config.levels.insert(m.id.clone(), level);
            apply_overlay_only(m, level);
            if send_ddc {
                self.ddc.send(m, level);
            }
        }
        if persist {
            self.save_cfg();
        }
    }

    fn switch_active_monitor(&mut self, delta: isize) {
        if self.monitors.is_empty() {
            return;
        }
        if self.config.sync_all {
            self.config.sync_all = false;
            self.save_cfg();
        }
        self.selected_monitor = cycle_monitor_index(self.selected_monitor, self.monitors.len(), delta);
        if let Some(m) = self.monitors.get(self.selected_monitor) {
            let level = *self.config.levels.get(&m.id).unwrap_or(&DEFAULT_BRIGHTNESS);
            let label = m.display_label(self.lang);
            let (title, body) = toast_active_monitor(self.lang, &label, level);
            self.show_toast(title, &body);
        }
        unsafe {
            if IsWindowVisible(self.flyout_hwnd) != 0 {
                reposition_flyout(self);
            } else {
                InvalidateRect(self.flyout_hwnd, null(), 0);
            }
        }
    }

    fn step_selected_or_all(&mut self, delta: i16) {
        if self.monitors.is_empty() {
            return;
        }
        if self.config.sync_all {
            let primary_idx = self
                .monitors
                .iter()
                .position(|m| m.is_primary)
                .unwrap_or(0);
            let cur = self
                .monitors
                .get(primary_idx)
                .and_then(|m| self.config.levels.get(&m.id).copied())
                .unwrap_or(DEFAULT_BRIGHTNESS);
            let next = step_brightness(cur, delta);
            let title = all_monitors_card_label(self.lang, self.monitors.len());
            self.set_all_levels(next, &title);
        } else {
            let idx = self.selected_monitor.min(self.monitors.len() - 1);
            let cur = self
                .monitors
                .get(idx)
                .and_then(|m| self.config.levels.get(&m.id).copied())
                .unwrap_or(DEFAULT_BRIGHTNESS);
            let next = step_brightness(cur, delta);
            self.set_monitor_level(idx, next, true, true);
        }
        unsafe {
            InvalidateRect(self.flyout_hwnd, null(), 0);
        }
    }

    fn toggle_sync(&mut self) {
        let next = !self.config.sync_all;
        self.config.sync_all = next;
        if next {
            let primary_level = self
                .monitors
                .iter()
                .find(|m| m.is_primary)
                .or_else(|| self.monitors.first())
                .and_then(|m| self.config.levels.get(&m.id).copied())
                .unwrap_or(DEFAULT_BRIGHTNESS);
            self.set_all_levels_live(primary_level, true, false);
        }
        self.save_cfg();
        let (title, body) = toast_sync(self.lang, next);
        self.show_toast(title, body);
        unsafe {
            reposition_flyout(self);
        }
    }

    fn build_flyout_vm(&self) -> FlyoutViewModel<'_> {
        let cards = if self.config.sync_all {
            let level = self
                .monitors
                .iter()
                .find(|m| m.is_primary)
                .or_else(|| self.monitors.first())
                .and_then(|m| self.config.levels.get(&m.id).copied())
                .unwrap_or(DEFAULT_BRIGHTNESS);
            vec![SliderCardItem {
                label: all_monitors_card_label(self.lang, self.monitors.len()),
                brightness: level,
                accent_amber: true,
                is_active: true,
            }]
        } else {
            self.monitors
                .iter()
                .enumerate()
                .map(|(idx, m)| SliderCardItem {
                    label: m.display_label(self.lang),
                    brightness: *self.config.levels.get(&m.id).unwrap_or(&DEFAULT_BRIGHTNESS),
                    accent_amber: m.is_primary,
                    is_active: idx == self.selected_monitor,
                })
                .collect()
        };
        FlyoutViewModel {
            lang: self.lang,
            sync_all: self.config.sync_all,
            autostart: self.autostart,
            about_open: self.about_open,
            monitor_count: self.monitors.len(),
            cards,
            update_state: &self.update_state,
        }
    }

    fn toggle_night_all(&mut self) {
        let first = self
            .monitors
            .first()
            .and_then(|m| self.config.levels.get(&m.id).copied())
            .unwrap_or(DEFAULT_BRIGHTNESS);
        let next = toggle_night_mode(first);
        let title = if next < MAX_BRIGHTNESS {
            toast_night_on(self.lang)
        } else {
            toast_day_on(self.lang)
        };
        self.set_all_levels(next, title);
    }

    fn toggle_lang(&mut self) {
        let next = self.lang.toggle();
        self.lang = next;
        self.config.lang = Some(next);
        self.save_cfg();
        unsafe {
            InvalidateRect(self.flyout_hwnd, null(), 0);
        }
    }

    fn toggle_autostart(&mut self) {
        let next = !self.autostart;
        if set_autostart_enabled(next).is_ok() {
            self.autostart = next;
            let (title, body) = toast_autostart(self.lang, next);
            self.show_toast(title, body);
            unsafe {
                InvalidateRect(self.flyout_hwnd, null(), 0);
            }
        }
    }

    fn trigger_update_action(&mut self) {
        match &self.update_state {
            UpdateState::Checking | UpdateState::Downloading => {}
            UpdateState::Available { download_url, .. } => {
                let url = download_url.clone();
                self.update_state = UpdateState::Downloading;
                unsafe {
                    InvalidateRect(self.flyout_hwnd, null(), 0);
                }
                if download_and_spawn_replace(&url).is_ok() {
                    unsafe {
                        PostQuitMessage(0);
                    }
                } else {
                    self.update_state = UpdateState::Error;
                    unsafe {
                        InvalidateRect(self.flyout_hwnd, null(), 0);
                    }
                }
            }
            _ => {
                self.update_state = UpdateState::Checking;
                let (tx, rx) = mpsc::channel();
                self.update_rx = Some(rx);
                let hwnd = self.main_hwnd as isize;
                thread::spawn(move || {
                    let res = check_latest_release();
                    let _ = tx.send(res);
                    unsafe {
                        PostMessageW(hwnd as Hwnd, WM_APP_UPDATE_RESULT, 0, 0);
                    }
                });
                unsafe {
                    InvalidateRect(self.flyout_hwnd, null(), 0);
                }
            }
        }
    }

    fn show_toast(&mut self, title: &str, body: &str) {
        self.toast_title = title.to_string();
        self.toast_body = body.to_string();
        unsafe {
            let mut work_area = Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
            };
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                &mut work_area as *mut Rect as *mut c_void,
                0,
            );
            let width = 340;
            let height = 84;
            let x = work_area.right - width - 16;
            let y = work_area.bottom - height - 16;

            let corner = DWMWCP_ROUND;
            DwmSetWindowAttribute(
                self.toast_hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner as *const u32 as *const c_void,
                4,
            );
            SetWindowPos(self.toast_hwnd, (-1isize) as Hwnd, x, y, width, height, 0x0010);
            ShowWindow(self.toast_hwnd, SW_SHOWNOACTIVATE);
            InvalidateRect(self.toast_hwnd, null(), 1);
            KillTimer(self.toast_hwnd, TOAST_TIMER_ID);
            SetTimer(self.toast_hwnd, TOAST_TIMER_ID, TOAST_DURATION_MS, null());
        }
    }
}

unsafe fn get_state_mut<'a>(hwnd: Hwnd) -> Option<&'a mut AppState> {
    let ptr = GetWindowLongPtrW(hwnd, -21) as *mut AppState;
    if ptr.is_null() {
        None
    } else {
        Some(&mut *ptr)
    }
}

unsafe fn reposition_flyout(state: &AppState) {
    let mut work_area = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };
    SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        &mut work_area as *mut Rect as *mut c_void,
        0,
    );

    let width = FLYOUT_WIDTH;
    let height = compute_flyout_height(state.monitors.len(), state.config.sync_all, state.about_open);
    let x = work_area.right - width - 12;
    let y = work_area.bottom - height - 12;

    SetWindowPos(state.flyout_hwnd, (-1isize) as Hwnd, x, y, width, height, 0x0010);
    InvalidateRect(state.flyout_hwnd, null(), 0);
}

unsafe fn toggle_flyout(state: &mut AppState) {
    if IsWindowVisible(state.flyout_hwnd) != 0 {
        state.about_open = false;
        state.dragging_monitor = None;
        ShowWindow(state.flyout_hwnd, SW_HIDE);
        return;
    }

    state.about_open = false;
    let dark: u32 = 1;
    let corner: u32 = DWMWCP_ROUND;
    let border: u32 = 0x00_3A_2A_1E;
    let backdrop: u32 = DWMSBT_TRANSIENTWINDOW;
    DwmSetWindowAttribute(
        state.flyout_hwnd,
        DWMWA_USE_IMMERSIVE_DARK_MODE,
        &dark as *const u32 as *const c_void,
        4,
    );
    DwmSetWindowAttribute(
        state.flyout_hwnd,
        DWMWA_WINDOW_CORNER_PREFERENCE,
        &corner as *const u32 as *const c_void,
        4,
    );
    DwmSetWindowAttribute(
        state.flyout_hwnd,
        DWMWA_BORDER_COLOR,
        &border as *const u32 as *const c_void,
        4,
    );
    DwmSetWindowAttribute(
        state.flyout_hwnd,
        DWMWA_SYSTEMBACKDROP_TYPE,
        &backdrop as *const u32 as *const c_void,
        4,
    );

    reposition_flyout(state);
    ShowWindow(state.flyout_hwnd, SW_SHOW);
    SetForegroundWindow(state.flyout_hwnd);
    InvalidateRect(state.flyout_hwnd, null(), 0);
}

extern "system" fn main_wnd_proc(hwnd: Hwnd, msg: u32, w_param: usize, l_param: isize) -> isize {
    unsafe {
        match msg {
            WM_APP_TRAY => {
                let mouse_msg = (l_param & 0xFFFF) as u32;
                if mouse_msg == WM_LBUTTONUP || mouse_msg == WM_RBUTTONUP {
                    if let Some(state) = get_state_mut(hwnd) {
                        toggle_flyout(state);
                    }
                } else if mouse_msg == WM_LBUTTONDBLCLK {
                    if let Some(state) = get_state_mut(hwnd) {
                        state.toggle_night_all();
                    }
                }
                0
            }
            WM_APP_UPDATE_RESULT => {
                if let Some(state) = get_state_mut(hwnd) {
                    if let Some(rx) = state.update_rx.take() {
                        if let Ok(res) = rx.try_recv() {
                            state.update_state = res;
                            InvalidateRect(state.flyout_hwnd, null(), 0);
                        }
                    }
                }
                0
            }
            WM_HOTKEY => {
                if let Some(state) = get_state_mut(hwnd) {
                    match w_param {
                        HOTKEY_BRIGHT_UP => state.switch_active_monitor(-1),
                        HOTKEY_BRIGHT_DOWN => state.switch_active_monitor(1),
                        HOTKEY_NIGHT_TOGGLE => state.toggle_night_all(),
                        HOTKEY_SYNC_DOWN => state.step_selected_or_all(-BRIGHTNESS_STEP),
                        HOTKEY_SYNC_UP => state.step_selected_or_all(BRIGHTNESS_STEP),
                        _ => {}
                    }
                }
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                0
            }
            _ => {
                if let Some(state) = get_state_mut(hwnd) {
                    if state.taskbar_created_msg != 0 && msg == state.taskbar_created_msg {
                        let nid = build_notify_icon_data(hwnd, state.tray_icon, state.lang);
                        Shell_NotifyIconW(NIM_ADD, &nid);
                        return 0;
                    }
                }
                DefWindowProcW(hwnd, msg, w_param, l_param)
            }
        }
    }
}

unsafe fn apply_slider_drag(state: &mut AppState, card_idx: usize, x: i32, send_ddc: bool, persist: bool) {
    let level = x_to_brightness(x);
    if state.config.sync_all {
        state.set_all_levels_live(level, send_ddc, persist);
    } else if let Some(m) = state.monitors.get_mut(card_idx) {
        state.selected_monitor = card_idx;
        state.config.levels.insert(m.id.clone(), level);
        apply_overlay_only(m, level);
        if send_ddc {
            state.ddc.send(m, level);
        }
        if persist {
            state.save_cfg();
        }
    }
    InvalidateRect(state.flyout_hwnd, null(), 0);
}

extern "system" fn flyout_wnd_proc(hwnd: Hwnd, msg: u32, w_param: usize, l_param: isize) -> isize {
    unsafe {
        match msg {
            WM_ERASEBKGND => 1,
            WM_ACTIVATE => {
                if (w_param & 0xFFFF) == WA_INACTIVE {
                    if let Some(state) = get_state_mut(hwnd) {
                        state.dragging_monitor = None;
                        state.about_open = false;
                    }
                    ShowWindow(hwnd, SW_HIDE);
                }
                0
            }
            WM_KEYDOWN => {
                if w_param == VK_ESCAPE {
                    if let Some(state) = get_state_mut(hwnd) {
                        if state.about_open {
                            state.about_open = false;
                            reposition_flyout(state);
                            return 0;
                        }
                        state.dragging_monitor = None;
                    }
                    ShowWindow(hwnd, SW_HIDE);
                }
                0
            }
            WM_LBUTTONDOWN => {
                let x = (l_param & 0xFFFF) as i16 as i32;
                let y = ((l_param >> 16) & 0xFFFF) as i16 as i32;
                if let Some(state) = get_state_mut(hwnd) {
                    if !state.about_open {
                        if let Some(card_idx) =
                            hit_test_slider(x, y, state.monitors.len(), state.config.sync_all)
                        {
                            state.dragging_monitor = Some(card_idx);
                            SetCapture(hwnd);
                            apply_slider_drag(state, card_idx, x, false, false);
                            return 0;
                        }
                    }

                    if let Some(ctrl) = hit_test_control(
                        x,
                        y,
                        state.monitors.len(),
                        state.config.sync_all,
                        state.about_open,
                    ) {
                        match ctrl {
                            FlyoutControl::LangToggle => state.toggle_lang(),
                            FlyoutControl::SyncToggle => state.toggle_sync(),
                            FlyoutControl::AutostartToggle => state.toggle_autostart(),
                            FlyoutControl::UpdateAction => state.trigger_update_action(),
                            FlyoutControl::AboutBack => {
                                state.about_open = false;
                                reposition_flyout(state);
                            }
                            FlyoutControl::AboutDonate => {
                                open_external_url(DONATE_URL);
                                ShowWindow(hwnd, SW_HIDE);
                            }
                            FlyoutControl::AboutGithub => {
                                open_external_url(GITHUB_REPO_URL);
                                ShowWindow(hwnd, SW_HIDE);
                            }
                            FlyoutControl::Footer(action) => match action {
                                FooterAction::NightMode => {
                                    state.toggle_night_all();
                                    InvalidateRect(hwnd, null(), 0);
                                }
                                FooterAction::ResetAll => {
                                    let title = toast_reset_title(state.lang);
                                    state.set_all_levels(MAX_BRIGHTNESS, title);
                                    InvalidateRect(hwnd, null(), 0);
                                }
                                FooterAction::RefreshMonitors => {
                                    state.refresh_monitors();
                                    let title = toast_refreshed_title(state.lang);
                                    let body = toast_refreshed_body(state.lang, state.monitors.len());
                                    state.show_toast(title, &body);
                                    reposition_flyout(state);
                                }
                                FooterAction::About => {
                                    state.about_open = true;
                                    reposition_flyout(state);
                                }
                                FooterAction::Exit => {
                                    ShowWindow(hwnd, SW_HIDE);
                                    PostQuitMessage(0);
                                }
                            },
                        }
                    }
                }
                0
            }
            WM_MOUSEMOVE => {
                if let Some(state) = get_state_mut(hwnd) {
                    if let Some(card_idx) = state.dragging_monitor {
                        let x = (l_param & 0xFFFF) as i16 as i32;
                        apply_slider_drag(state, card_idx, x, false, false);
                    }
                }
                0
            }
            WM_LBUTTONUP => {
                if let Some(state) = get_state_mut(hwnd) {
                    if let Some(card_idx) = state.dragging_monitor.take() {
                        ReleaseCapture();
                        let x = (l_param & 0xFFFF) as i16 as i32;
                        apply_slider_drag(state, card_idx, x, true, true);
                    }
                }
                0
            }
            WM_PAINT => {
                let mut ps = PaintStruct {
                    hdc: null_mut(),
                    f_erase: 0,
                    rc_paint: Rect {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    },
                    f_restore: 0,
                    f_inc_update: 0,
                    rgb_reserved: [0; 32],
                };
                let hdc = BeginPaint(hwnd, &mut ps);
                if let Some(state) = get_state_mut(hwnd) {
                    let vm = state.build_flyout_vm();
                    paint_flyout(hdc, &vm);
                }
                EndPaint(hwnd, &ps);
                0
            }
            _ => DefWindowProcW(hwnd, msg, w_param, l_param),
        }
    }
}

extern "system" fn overlay_wnd_proc(hwnd: Hwnd, msg: u32, w_param: usize, l_param: isize) -> isize {
    unsafe {
        if msg == WM_PAINT {
            let mut ps = PaintStruct {
                hdc: null_mut(),
                f_erase: 0,
                rc_paint: Rect {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
                f_restore: 0,
                f_inc_update: 0,
                rgb_reserved: [0; 32],
            };
            let hdc = BeginPaint(hwnd, &mut ps);
            let black = CreateSolidBrush(0x00_00_00_00);
            FillRect(hdc, &ps.rc_paint, black);
            DeleteObject(black);
            EndPaint(hwnd, &ps);
            return 0;
        }
        DefWindowProcW(hwnd, msg, w_param, l_param)
    }
}

extern "system" fn toast_wnd_proc(hwnd: Hwnd, msg: u32, w_param: usize, l_param: isize) -> isize {
    unsafe {
        match msg {
            WM_TIMER => {
                if w_param == TOAST_TIMER_ID {
                    KillTimer(hwnd, TOAST_TIMER_ID);
                    ShowWindow(hwnd, SW_HIDE);
                }
                0
            }
            WM_PAINT => {
                let mut ps = PaintStruct {
                    hdc: null_mut(),
                    f_erase: 0,
                    rc_paint: Rect {
                        left: 0,
                        top: 0,
                        right: 0,
                        bottom: 0,
                    },
                    f_restore: 0,
                    f_inc_update: 0,
                    rgb_reserved: [0; 32],
                };
                let hdc = BeginPaint(hwnd, &mut ps);
                let bg = CreateSolidBrush(0x00_23_17_12);
                let border = CreateSolidBrush(0x00_48_B9_F0); // amber border
                let rect = Rect {
                    left: 0,
                    top: 0,
                    right: 340,
                    bottom: 84,
                };
                FillRect(hdc, &rect, bg);
                FrameRect(hdc, &rect, border);
                DeleteObject(bg);
                DeleteObject(border);

                let (title, body) = get_state_mut(hwnd)
                    .map(|s| (s.toast_title.clone(), s.toast_body.clone()))
                    .unwrap_or_default();

                let font_face = to_wide("Segoe UI");
                let title_font = CreateFontW(16, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 5, 0, font_face.as_ptr());
                let body_font = CreateFontW(15, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, font_face.as_ptr());

                SetBkMode(hdc, TRANSPARENT_BK);
                let old_font = SelectObject(hdc, title_font);
                SetTextColor(hdc, 0x00_48_B9_F0);
                let mut title_rect = Rect {
                    left: 16,
                    top: 14,
                    right: 324,
                    bottom: 36,
                };
                let title_w = to_wide(&title);
                DrawTextW(hdc, title_w.as_ptr(), -1, &mut title_rect, DT_LEFT | DT_NOPREFIX);

                SelectObject(hdc, body_font);
                SetTextColor(hdc, 0x00_FF_F2_EE);
                let mut body_rect = Rect {
                    left: 16,
                    top: 40,
                    right: 324,
                    bottom: 72,
                };
                let body_w = to_wide(&body);
                DrawTextW(
                    hdc,
                    body_w.as_ptr(),
                    -1,
                    &mut body_rect,
                    DT_LEFT | DT_NOPREFIX | DT_END_ELLIPSIS,
                );

                SelectObject(hdc, old_font);
                DeleteObject(title_font);
                DeleteObject(body_font);
                EndPaint(hwnd, &ps);
                0
            }
            _ => DefWindowProcW(hwnd, msg, w_param, l_param),
        }
    }
}

pub fn run_app() {
    unsafe {
        let mutex_name = to_wide("Local\\MultiMonitorTrayDimmerSingleInstance");
        let mutex_handle = CreateMutexW(null_mut(), 0, mutex_name.as_ptr());
        if !mutex_handle.is_null() && GetLastError() == ERROR_ALREADY_EXISTS {
            CloseHandle(mutex_handle);
            return;
        }

        enable_per_monitor_dpi_awareness();
        let h_inst = GetModuleHandleW(null());
        let main_class = to_wide("MultiMonitorTrayDimmerMain");
        let overlay_class = to_wide("MultiMonitorTrayDimmerOverlay");
        let toast_class = to_wide("MultiMonitorTrayDimmerToast");
        let flyout_class = to_wide("MultiMonitorTrayDimmerFlyout");

        RegisterClassW(&WndClassW {
            style: 0,
            lpfn_wnd_proc: main_wnd_proc,
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: h_inst,
            h_icon: null_mut(),
            h_cursor: null_mut(),
            hbr_background: null_mut(),
            lpsz_menu_name: null(),
            lpsz_class_name: main_class.as_ptr(),
        });

        RegisterClassW(&WndClassW {
            style: 0,
            lpfn_wnd_proc: overlay_wnd_proc,
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: h_inst,
            h_icon: null_mut(),
            h_cursor: null_mut(),
            hbr_background: null_mut(),
            lpsz_menu_name: null(),
            lpsz_class_name: overlay_class.as_ptr(),
        });

        RegisterClassW(&WndClassW {
            style: 0,
            lpfn_wnd_proc: toast_wnd_proc,
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: h_inst,
            h_icon: null_mut(),
            h_cursor: null_mut(),
            hbr_background: null_mut(),
            lpsz_menu_name: null(),
            lpsz_class_name: toast_class.as_ptr(),
        });

        RegisterClassW(&WndClassW {
            style: 0,
            lpfn_wnd_proc: flyout_wnd_proc,
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_instance: h_inst,
            h_icon: null_mut(),
            h_cursor: null_mut(),
            hbr_background: null_mut(),
            lpsz_menu_name: null(),
            lpsz_class_name: flyout_class.as_ptr(),
        });

        let main_hwnd = CreateWindowExW(
            0,
            main_class.as_ptr(),
            main_class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            h_inst,
            null_mut(),
        );

        let toast_hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            toast_class.as_ptr(),
            toast_class.as_ptr(),
            WS_POPUP,
            0,
            0,
            340,
            84,
            null_mut(),
            null_mut(),
            h_inst,
            null_mut(),
        );

        let flyout_hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            flyout_class.as_ptr(),
            flyout_class.as_ptr(),
            WS_POPUP,
            0,
            0,
            FLYOUT_WIDTH,
            320,
            null_mut(),
            null_mut(),
            h_inst,
            null_mut(),
        );

        let saved = config_file_path().map(|p| load_config(&p)).unwrap_or_default();
        let lang = saved.lang.unwrap_or_else(detect_system_lang);
        let autostart = is_autostart_enabled();
        let taskbar_created_msg = RegisterWindowMessageW(to_wide("TaskbarCreated").as_ptr());
        let icon = create_tray_icon();
        let mut state = Box::new(AppState {
            h_inst,
            main_hwnd,
            flyout_hwnd,
            overlay_class,
            monitors: Vec::new(),
            selected_monitor: usize::MAX,
            taskbar_created_msg,
            tray_icon: icon,
            config: saved,
            lang,
            autostart,
            about_open: false,
            ddc: DdcWorker::spawn(),
            dragging_monitor: None,
            update_state: UpdateState::Idle,
            update_rx: None,
            toast_hwnd,
            toast_title: String::new(),
            toast_body: String::new(),
        });
        state.refresh_monitors();

        let state_ptr = Box::into_raw(state);
        SetWindowLongPtrW(main_hwnd, -21, state_ptr as isize);
        SetWindowLongPtrW(toast_hwnd, -21, state_ptr as isize);
        SetWindowLongPtrW(flyout_hwnd, -21, state_ptr as isize);

        RegisterHotKey(main_hwnd, HOTKEY_BRIGHT_UP as i32, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_UP);
        RegisterHotKey(main_hwnd, HOTKEY_BRIGHT_DOWN as i32, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_DOWN);
        RegisterHotKey(main_hwnd, HOTKEY_NIGHT_TOGGLE as i32, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_D);
        RegisterHotKey(main_hwnd, HOTKEY_SYNC_DOWN as i32, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_LEFT);
        RegisterHotKey(main_hwnd, HOTKEY_SYNC_UP as i32, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_RIGHT);

        let nid = build_notify_icon_data(main_hwnd, icon, lang);
        Shell_NotifyIconW(NIM_ADD, &nid);

        let mut msg = Msg {
            hwnd: null_mut(),
            message: 0,
            w_param: 0,
            l_param: 0,
            time: 0,
            pt: Point { x: 0, y: 0 },
        };
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        UnregisterHotKey(main_hwnd, HOTKEY_BRIGHT_UP as i32);
        UnregisterHotKey(main_hwnd, HOTKEY_BRIGHT_DOWN as i32);
        UnregisterHotKey(main_hwnd, HOTKEY_NIGHT_TOGGLE as i32);
        UnregisterHotKey(main_hwnd, HOTKEY_SYNC_DOWN as i32);
        UnregisterHotKey(main_hwnd, HOTKEY_SYNC_UP as i32);
        Shell_NotifyIconW(NIM_DELETE, &nid);
        DestroyIcon(icon);

        let mut boxed_state = Box::from_raw(state_ptr);
        destroy_overlay_windows(&mut boxed_state.monitors);
        if !mutex_handle.is_null() {
            CloseHandle(mutex_handle);
        }
    }
}
