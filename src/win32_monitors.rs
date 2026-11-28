//! Win32 monitor enumeration (matched to Windows system numbering), async DDC/CI worker,
//! and zero-lag per-monitor click-through dimming overlays.

#![cfg(windows)]

use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{null, null_mut};
use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::Duration;

use crate::dimmer::brightness_to_overlay_alpha;
use crate::i18n::{format_monitor_label, Lang};

pub type Hwnd = *mut c_void;
pub type Hmonitor = *mut c_void;
pub type Handle = *mut c_void;

const MONITORINFOF_PRIMARY: u32 = 0x00000001;

const WS_POPUP: u32 = 0x80000000;
const WS_EX_TOPMOST: u32 = 0x00000008;
const WS_EX_TRANSPARENT: u32 = 0x00000020;
const WS_EX_TOOLWINDOW: u32 = 0x00000080;
const WS_EX_LAYERED: u32 = 0x00080000;
const WS_EX_NOACTIVATE: u32 = 0x08000000;
const LWA_ALPHA: u32 = 0x00000002;
const SW_SHOWNOACTIVATE: i32 = 4;
const SW_HIDE: i32 = 0;

const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_SHOWWINDOW: u32 = 0x0040;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[repr(C)]
struct MonitorInfoExW {
    cb_size: u32,
    rc_monitor: Rect,
    rc_work: Rect,
    dw_flags: u32,
    sz_device: [u16; 32],
}

#[repr(C)]
struct PhysicalMonitor {
    h_physical_monitor: Handle,
    sz_physical_monitor_description: [u16; 128],
}

#[derive(Clone)]
pub struct MonitorEntry {
    pub id: String,
    pub system_number: u32,
    pub is_primary: bool,
    pub rect: Rect,
    pub h_monitor: Hmonitor,
    pub overlay_hwnd: Hwnd,
    pub overlay_visible: bool,
}

impl MonitorEntry {
    pub fn display_label(&self, lang: Lang) -> String {
        format_monitor_label(lang, self.system_number, self.is_primary)
    }
}

#[link(name = "user32")]
extern "system" {
    fn EnumDisplayMonitors(
        hdc: *mut c_void,
        lprc_clip: *const Rect,
        lpfn_enum: extern "system" fn(Hmonitor, *mut c_void, *mut Rect, isize) -> i32,
        dw_data: isize,
    ) -> i32;
    fn GetMonitorInfoW(h_monitor: Hmonitor, lpmi: *mut MonitorInfoExW) -> i32;
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
        h_menu: *mut c_void,
        h_instance: *mut c_void,
        lp_param: *mut c_void,
    ) -> Hwnd;
    fn DestroyWindow(h_wnd: Hwnd) -> i32;
    fn SetLayeredWindowAttributes(hwnd: Hwnd, cr_key: u32, b_alpha: u8, dw_flags: u32) -> i32;
    fn ShowWindow(h_wnd: Hwnd, n_cmd_show: i32) -> i32;
    fn SetWindowPos(
        h_wnd: Hwnd,
        h_wnd_insert_after: Hwnd,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        u_flags: u32,
    ) -> i32;
}

#[link(name = "dxva2")]
extern "system" {
    fn GetNumberOfPhysicalMonitorsFromHMONITOR(
        h_monitor: Hmonitor,
        pdw_number_of_physical_monitors: *mut u32,
    ) -> i32;
    fn GetPhysicalMonitorsFromHMONITOR(
        h_monitor: Hmonitor,
        dw_physical_monitor_array_size: u32,
        p_physical_monitor_array: *mut PhysicalMonitor,
    ) -> i32;
    fn SetMonitorBrightness(h_monitor: Handle, dw_new_brightness: u32) -> i32;
    fn DestroyPhysicalMonitors(
        dw_physical_monitor_array_size: u32,
        p_physical_monitor_array: *const PhysicalMonitor,
    ) -> i32;
}

/// Extracts the Windows display number from device IDs like `DISPLAY1` -> `Some(1)`.
pub fn parse_display_number(id: &str) -> Option<u32> {
    let digits: String = id.chars().rev().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let forward: String = digits.chars().rev().collect();
    forward.parse::<u32>().ok()
}

extern "system" fn enum_monitor_cb(
    h_monitor: Hmonitor,
    _hdc: *mut c_void,
    _rc: *mut Rect,
    dw_data: isize,
) -> i32 {
    unsafe {
        let list = &mut *(dw_data as *mut Vec<MonitorEntry>);
        let mut info = MonitorInfoExW {
            cb_size: size_of::<MonitorInfoExW>() as u32,
            rc_monitor: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            rc_work: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            dw_flags: 0,
            sz_device: [0; 32],
        };
        if GetMonitorInfoW(h_monitor, &mut info) != 0 {
            let end = info.sz_device.iter().position(|&c| c == 0).unwrap_or(32);
            let raw_name = String::from_utf16_lossy(&info.sz_device[..end]);
            let short_id = raw_name.trim_start_matches("\\\\.\\").to_string();
            let is_primary = (info.dw_flags & MONITORINFOF_PRIMARY) != 0;
            let sys_num = parse_display_number(&short_id).unwrap_or((list.len() + 1) as u32);

            list.push(MonitorEntry {
                id: short_id,
                system_number: sys_num,
                is_primary,
                rect: info.rc_monitor,
                h_monitor,
                overlay_hwnd: null_mut(),
                overlay_visible: false,
            });
        }
        1
    }
}

/// Enumerates monitors and sorts them by Windows system display number (`DISPLAY1`, `DISPLAY2`, ...).
pub fn enumerate_monitors() -> Vec<MonitorEntry> {
    let mut list: Vec<MonitorEntry> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            null_mut(),
            null(),
            enum_monitor_cb,
            &mut list as *mut Vec<MonitorEntry> as isize,
        );
    }
    list.sort_by_key(|m| (m.system_number, m.id.clone()));
    list
}

pub fn create_overlay_window(h_inst: *mut c_void, class_name: &[u16], rect: Rect) -> Hwnd {
    unsafe {
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE,
            class_name.as_ptr(),
            class_name.as_ptr(),
            WS_POPUP,
            rect.left,
            rect.top,
            width,
            height,
            null_mut(),
            null_mut(),
            h_inst,
            null_mut(),
        )
    }
}

pub fn destroy_overlay_windows(monitors: &mut [MonitorEntry]) {
    unsafe {
        for m in monitors {
            if !m.overlay_hwnd.is_null() {
                DestroyWindow(m.overlay_hwnd);
                m.overlay_hwnd = null_mut();
                m.overlay_visible = false;
            }
        }
    }
}

/// Updates only the instant click-through overlay without forcing DWM re-layout when already visible.
pub fn apply_overlay_only(monitor: &mut MonitorEntry, brightness: u8) {
    unsafe {
        if !monitor.overlay_hwnd.is_null() {
            let alpha = brightness_to_overlay_alpha(brightness);
            if alpha == 0 {
                if monitor.overlay_visible {
                    ShowWindow(monitor.overlay_hwnd, SW_HIDE);
                    monitor.overlay_visible = false;
                }
            } else {
                SetLayeredWindowAttributes(monitor.overlay_hwnd, 0, alpha, LWA_ALPHA);
                if !monitor.overlay_visible {
                    SetWindowPos(
                        monitor.overlay_hwnd,
                        (-1isize) as Hwnd,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                    ShowWindow(monitor.overlay_hwnd, SW_SHOWNOACTIVATE);
                    monitor.overlay_visible = true;
                }
            }
        }
    }
}

/// Applies hardware DDC/CI brightness (blocking I2C; run on background worker).
pub fn apply_ddc_raw(h_monitor: isize, brightness: u8) {
    unsafe {
        let hm = h_monitor as Hmonitor;
        let mut count: u32 = 0;
        if GetNumberOfPhysicalMonitorsFromHMONITOR(hm, &mut count) != 0 && count > 0 {
            let mut phys = Vec::with_capacity(count as usize);
            for _ in 0..count {
                phys.push(PhysicalMonitor {
                    h_physical_monitor: null_mut(),
                    sz_physical_monitor_description: [0; 128],
                });
            }
            if GetPhysicalMonitorsFromHMONITOR(hm, count, phys.as_mut_ptr()) != 0 {
                for p in &phys {
                    let _ = SetMonitorBrightness(p.h_physical_monitor, brightness as u32);
                }
                DestroyPhysicalMonitors(count, phys.as_ptr());
            }
        }
    }
}

/// Background DDC/CI worker that coalesces rapid brightness changes so the UI thread never stutters.
pub struct DdcWorker {
    tx: Sender<(isize, u8)>,
}

impl DdcWorker {
    pub fn spawn() -> Self {
        let (tx, rx) = mpsc::channel::<(isize, u8)>();
        thread::spawn(move || {
            while let Ok((first_hm, first_val)) = rx.recv() {
                let mut pending: HashMap<isize, u8> = HashMap::new();
                pending.insert(first_hm, first_val);
                // Coalesce bursts for 35ms so rapid hotkey/slider steps don't queue stale I2C writes.
                thread::sleep(Duration::from_millis(35));
                while let Ok((hm, val)) = rx.try_recv() {
                    pending.insert(hm, val);
                }
                for (hm, val) in pending {
                    apply_ddc_raw(hm, val);
                }
            }
        });
        Self { tx }
    }

    pub fn send(&self, monitor: &MonitorEntry, brightness: u8) {
        let _ = self.tx.send((monitor.h_monitor as isize, brightness));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_system_display_numbers() {
        assert_eq!(parse_display_number("DISPLAY1"), Some(1));
        assert_eq!(parse_display_number("DISPLAY2"), Some(2));
        assert_eq!(parse_display_number("DISPLAY12"), Some(12));
        assert_eq!(parse_display_number("GENERIC"), None);
    }
}
