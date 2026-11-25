//! Native Win32 system tray icon, per-monitor submenus, global hotkeys, and dark toast popup.

#![cfg(windows)]

use std::collections::BTreeMap;
use std::ffi::c_void;
use std::ptr::{null, null_mut};

use crate::dimmer::{
    config_file_path, load_config, save_config, step_brightness, toggle_night_mode,
    BRIGHTNESS_STEP, DEFAULT_BRIGHTNESS, MAX_BRIGHTNESS,
};
use crate::win32_monitors::{
    active_monitor_handle, apply_monitor_brightness, create_overlay_window, destroy_overlay_windows,
    enumerate_monitors, Hwnd, MonitorEntry, Rect,
};

const WM_APP_TRAY: u32 = 0x8000 + 1;
const WM_DESTROY: u32 = 0x0002;
const WM_PAINT: u32 = 0x000F;
const WM_COMMAND: u32 = 0x0111;
const WM_TIMER: u32 = 0x0113;
const WM_HOTKEY: u32 = 0x0312;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_RBUTTONUP: u32 = 0x0205;
const WM_LBUTTONDBLCLK: u32 = 0x0203;

const HOTKEY_BRIGHT_UP: usize = 1;
const HOTKEY_BRIGHT_DOWN: usize = 2;
const HOTKEY_NIGHT_TOGGLE: usize = 3;
const MOD_ALT: u32 = 0x0001;
const MOD_CONTROL: u32 = 0x0002;
const MOD_NOREPEAT: u32 = 0x4000;
const VK_UP: u32 = 0x26;
const VK_DOWN: u32 = 0x28;
const VK_D: u32 = 0x44;

const IDM_NIGHT_ALL: usize = 1001;
const IDM_RESET_ALL: usize = 1002;
const IDM_REFRESH_MONITORS: usize = 1003;
const IDM_EXIT: usize = 1004;
const IDM_MONITOR_BASE: usize = 2000; // 2000 + monitor_idx * 10 + preset_idx

const PRESETS: [u8; 5] = [100, 80, 60, 40, 20];
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
const SW_SHOWNOACTIVATE: i32 = 4;
const SW_HIDE: i32 = 0;

const MF_STRING: u32 = 0x00000000;
const MF_CHECKED: u32 = 0x00000008;
const MF_SEPARATOR: u32 = 0x00000800;
const MF_POPUP: u32 = 0x00000010;
const TPM_RIGHTALIGN: u32 = 0x0008;
const TPM_BOTTOMALIGN: u32 = 0x0020;

const SPI_GETWORKAREA: u32 = 0x0030;
const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: u32 = 2;
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

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleW(lp_module_name: *const u16) -> Hinstance;
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
    fn PostQuitMessage(n_exit_code: i32);
    fn ShowWindow(h_wnd: Hwnd, n_cmd_show: i32) -> i32;
    fn InvalidateRect(h_wnd: Hwnd, lp_rect: *const Rect, b_erase: i32) -> i32;
    fn SetTimer(h_wnd: Hwnd, id: usize, elapse: u32, func: *const c_void) -> usize;
    fn KillTimer(h_wnd: Hwnd, id: usize) -> i32;
    fn CreatePopupMenu() -> Hmenu;
    fn AppendMenuW(h_menu: Hmenu, u_flags: u32, u_id_new_item: usize, lp_new_item: *const u16) -> i32;
    fn DestroyMenu(h_menu: Hmenu) -> i32;
    fn TrackPopupMenu(
        h_menu: Hmenu,
        u_flags: u32,
        x: i32,
        y: i32,
        n_reserved: i32,
        h_wnd: Hwnd,
        prc_rect: *const Rect,
    ) -> i32;
    fn GetCursorPos(lp_point: *mut Point) -> i32;
    fn SetForegroundWindow(h_wnd: Hwnd) -> i32;
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
    overlay_class: Vec<u16>,
    monitors: Vec<MonitorEntry>,
    levels: BTreeMap<String, u8>,
    toast_hwnd: Hwnd,
    toast_title: String,
    toast_body: String,
}

impl AppState {
    fn refresh_monitors(&mut self) {
        destroy_overlay_windows(&mut self.monitors);
        self.monitors = enumerate_monitors();
        for m in &mut self.monitors {
            m.overlay_hwnd = create_overlay_window(self.h_inst, &self.overlay_class, m.rect);
            let level = *self.levels.get(&m.id).unwrap_or(&DEFAULT_BRIGHTNESS);
            self.levels.entry(m.id.clone()).or_insert(level);
            apply_monitor_brightness(m, level);
        }
    }

    fn set_monitor_level(&mut self, idx: usize, level: u8, notify: bool) {
        let label = if let Some(m) = self.monitors.get(idx) {
            self.levels.insert(m.id.clone(), level);
            apply_monitor_brightness(m, level);
            if let Some(cfg_path) = config_file_path() {
                save_config(&cfg_path, &self.levels);
            }
            Some(m.label.clone())
        } else {
            None
        };
        if notify {
            if let Some(lbl) = label {
                self.show_toast(&lbl, &format!("Яркость установлена: {level}%"));
            }
        }
    }

    fn set_all_levels(&mut self, level: u8, label: &str) {
        for m in &self.monitors {
            self.levels.insert(m.id.clone(), level);
            apply_monitor_brightness(m, level);
        }
        if let Some(cfg_path) = config_file_path() {
            save_config(&cfg_path, &self.levels);
        }
        self.show_toast(label, &format!("Все мониторы: {level}%"));
    }

    fn step_active_monitor(&mut self, delta: i16) {
        let active_h = active_monitor_handle();
        let idx = self
            .monitors
            .iter()
            .position(|m| m.h_monitor == active_h)
            .unwrap_or(0);
        if let Some(m) = self.monitors.get(idx) {
            let cur = *self.levels.get(&m.id).unwrap_or(&DEFAULT_BRIGHTNESS);
            let next = step_brightness(cur, delta);
            self.set_monitor_level(idx, next, true);
        }
    }

    fn toggle_night_all(&mut self) {
        let first = self
            .monitors
            .first()
            .and_then(|m| self.levels.get(&m.id).copied())
            .unwrap_or(DEFAULT_BRIGHTNESS);
        let next = toggle_night_mode(first);
        let title = if next < MAX_BRIGHTNESS {
            "Ночной режим включён"
        } else {
            "Дневной режим (100%)"
        };
        self.set_all_levels(next, title);
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

unsafe fn show_context_menu(hwnd: Hwnd, state: &AppState) {
    let menu = CreatePopupMenu();

    for (idx, m) in state.monitors.iter().enumerate() {
        let cur = *state.levels.get(&m.id).unwrap_or(&DEFAULT_BRIGHTNESS);
        let sub = CreatePopupMenu();
        for (p_idx, pct) in PRESETS.iter().enumerate() {
            let label = to_wide(&format!("{pct}%"));
            AppendMenuW(
                sub,
                MF_STRING | if cur == *pct { MF_CHECKED } else { 0 },
                IDM_MONITOR_BASE + idx * 10 + p_idx,
                label.as_ptr(),
            );
        }
        let m_label = to_wide(&format!("{} — {cur}%", m.label));
        AppendMenuW(menu, MF_POPUP, sub as usize, m_label.as_ptr());
    }

    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    let night_w = to_wide("Ночной режим (Ctrl+Alt+D)");
    let reset_w = to_wide("Сбросить всё на 100%");
    let refresh_w = to_wide("Обновить список мониторов");
    let exit_w = to_wide("Выход");

    AppendMenuW(menu, MF_STRING, IDM_NIGHT_ALL, night_w.as_ptr());
    AppendMenuW(menu, MF_STRING, IDM_RESET_ALL, reset_w.as_ptr());
    AppendMenuW(menu, MF_STRING, IDM_REFRESH_MONITORS, refresh_w.as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, IDM_EXIT, exit_w.as_ptr());

    let mut pt = Point { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    TrackPopupMenu(menu, TPM_RIGHTALIGN | TPM_BOTTOMALIGN, pt.x, pt.y, 0, hwnd, null());
    DestroyMenu(menu);
}

extern "system" fn main_wnd_proc(hwnd: Hwnd, msg: u32, w_param: usize, l_param: isize) -> isize {
    unsafe {
        match msg {
            WM_APP_TRAY => {
                let mouse_msg = (l_param & 0xFFFF) as u32;
                if mouse_msg == WM_LBUTTONUP || mouse_msg == WM_RBUTTONUP {
                    if let Some(state) = get_state_mut(hwnd) {
                        show_context_menu(hwnd, state);
                    }
                } else if mouse_msg == WM_LBUTTONDBLCLK {
                    if let Some(state) = get_state_mut(hwnd) {
                        state.toggle_night_all();
                    }
                }
                0
            }
            WM_HOTKEY => {
                if let Some(state) = get_state_mut(hwnd) {
                    match w_param {
                        HOTKEY_BRIGHT_UP => state.step_active_monitor(BRIGHTNESS_STEP),
                        HOTKEY_BRIGHT_DOWN => state.step_active_monitor(-BRIGHTNESS_STEP),
                        HOTKEY_NIGHT_TOGGLE => state.toggle_night_all(),
                        _ => {}
                    }
                }
                0
            }
            WM_COMMAND => {
                if let Some(state) = get_state_mut(hwnd) {
                    let cmd = w_param & 0xFFFF;
                    match cmd {
                        IDM_NIGHT_ALL => state.toggle_night_all(),
                        IDM_RESET_ALL => state.set_all_levels(MAX_BRIGHTNESS, "Яркость сброшена"),
                        IDM_REFRESH_MONITORS => {
                            state.refresh_monitors();
                            state.show_toast("Мониторы обновлены", &format!("Найдено экранов: {}", state.monitors.len()));
                        }
                        IDM_EXIT => PostQuitMessage(0),
                        c if c >= IDM_MONITOR_BASE => {
                            let rel = c - IDM_MONITOR_BASE;
                            let m_idx = rel / 10;
                            let p_idx = rel % 10;
                            if let Some(&pct) = PRESETS.get(p_idx) {
                                state.set_monitor_level(m_idx, pct, true);
                            }
                        }
                        _ => {}
                    }
                }
                0
            }
            WM_DESTROY => {
                PostQuitMessage(0);
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
        enable_per_monitor_dpi_awareness();
        let h_inst = GetModuleHandleW(null());
        let main_class = to_wide("MultiMonitorTrayDimmerMain");
        let overlay_class = to_wide("MultiMonitorTrayDimmerOverlay");
        let toast_class = to_wide("MultiMonitorTrayDimmerToast");

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

        let saved = config_file_path().map(|p| load_config(&p)).unwrap_or_default();
        let mut state = Box::new(AppState {
            h_inst,
            overlay_class,
            monitors: Vec::new(),
            levels: saved,
            toast_hwnd,
            toast_title: String::new(),
            toast_body: String::new(),
        });
        state.refresh_monitors();

        let state_ptr = Box::into_raw(state);
        SetWindowLongPtrW(main_hwnd, -21, state_ptr as isize);
        SetWindowLongPtrW(toast_hwnd, -21, state_ptr as isize);

        RegisterHotKey(main_hwnd, HOTKEY_BRIGHT_UP as i32, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_UP);
        RegisterHotKey(main_hwnd, HOTKEY_BRIGHT_DOWN as i32, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_DOWN);
        RegisterHotKey(main_hwnd, HOTKEY_NIGHT_TOGGLE as i32, MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, VK_D);

        let icon = create_tray_icon();
        let mut tip = [0u16; 128];
        let tip_w = to_wide("Multi-Monitor Tray Dimmer — яркость экранов");
        let n = tip_w.len().min(tip.len() - 1);
        tip[..n].copy_from_slice(&tip_w[..n]);

        let nid = NotifyIconDataW {
            cb_size: std::mem::size_of::<NotifyIconDataW>() as u32,
            h_wnd: main_hwnd,
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
        };
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
        Shell_NotifyIconW(NIM_DELETE, &nid);
        DestroyIcon(icon);

        let mut boxed_state = Box::from_raw(state_ptr);
        destroy_overlay_windows(&mut boxed_state.monitors);
    }
}
