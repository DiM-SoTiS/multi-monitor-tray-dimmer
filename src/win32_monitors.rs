//! Win32 monitor enumeration, DDC/CI brightness control, and per-monitor click-through dimming overlays.

#![cfg(windows)]

use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::{null, null_mut};

use crate::dimmer::brightness_to_overlay_alpha;

pub type Hwnd = *mut c_void;
pub type Hmonitor = *mut c_void;
pub type Handle = *mut c_void;

const MONITORINFOF_PRIMARY: u32 = 0x00000001;
const MONITOR_DEFAULTTONEAREST: u32 = 0x00000002;

const WS_POPUP: u32 = 0x80000000;
const WS_EX_TOPMOST: u32 = 0x00000008;
const WS_EX_TRANSPARENT: u32 = 0x00000020;
const WS_EX_TOOLWINDOW: u32 = 0x00000080;
const WS_EX_LAYERED: u32 = 0x00080000;
const WS_EX_NOACTIVATE: u32 = 0x08000000;
const LWA_ALPHA: u32 = 0x00000002;
const SW_SHOWNOACTIVATE: i32 = 4;
const SW_HIDE: i32 = 0;

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
    pub label: String,
    pub rect: Rect,
    pub h_monitor: Hmonitor,
    pub overlay_hwnd: Hwnd,
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
    fn GetForegroundWindow() -> Hwnd;
    fn MonitorFromWindow(hwnd: Hwnd, dw_flags: u32) -> Hmonitor;
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
            let short_id = raw_name
                .trim_start_matches("\\\\.\\")
                .to_string();
            let is_primary = (info.dw_flags & MONITORINFOF_PRIMARY) != 0;
            let idx = list.len() + 1;
            let label = if is_primary {
                format!("Монитор {idx} (Основной)")
            } else {
                format!("Монитор {idx}")
            };

            list.push(MonitorEntry {
                id: short_id,
                label,
                rect: info.rc_monitor,
                h_monitor,
                overlay_hwnd: null_mut(),
            });
        }
        1
    }
}

pub fn enumerate_monitors() -> Vec<MonitorEntry> {
    let mut list = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            null_mut(),
            null(),
            enum_monitor_cb,
            &mut list as *mut Vec<MonitorEntry> as isize,
        );
    }
    list
}

pub fn active_monitor_handle() -> Hmonitor {
    unsafe {
        let fg = GetForegroundWindow();
        MonitorFromWindow(fg, MONITOR_DEFAULTTONEAREST)
    }
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
            }
        }
    }
}

/// Applies brightness (`20..=100`) to both DDC/CI (best-effort) and the click-through overlay.
pub fn apply_monitor_brightness(monitor: &MonitorEntry, brightness: u8) {
    unsafe {
        // 1. Try hardware DDC/CI brightness when supported by the display.
        let mut count: u32 = 0;
        if GetNumberOfPhysicalMonitorsFromHMONITOR(monitor.h_monitor, &mut count) != 0 && count > 0 {
            let mut phys = Vec::with_capacity(count as usize);
            for _ in 0..count {
                phys.push(PhysicalMonitor {
                    h_physical_monitor: null_mut(),
                    sz_physical_monitor_description: [0; 128],
                });
            }
            if GetPhysicalMonitorsFromHMONITOR(monitor.h_monitor, count, phys.as_mut_ptr()) != 0 {
                for p in &phys {
                    let _ = SetMonitorBrightness(p.h_physical_monitor, brightness as u32);
                }
                DestroyPhysicalMonitors(count, phys.as_ptr());
            }
        }

        // 2. Always update click-through overlay so dimming works on all monitors/cables.
        if !monitor.overlay_hwnd.is_null() {
            let alpha = brightness_to_overlay_alpha(brightness);
            if alpha == 0 {
                ShowWindow(monitor.overlay_hwnd, SW_HIDE);
            } else {
                SetLayeredWindowAttributes(monitor.overlay_hwnd, 0, alpha, LWA_ALPHA);
                SetWindowPos(
                    monitor.overlay_hwnd,
                    (-1isize) as Hwnd,
                    monitor.rect.left,
                    monitor.rect.top,
                    monitor.rect.right - monitor.rect.left,
                    monitor.rect.bottom - monitor.rect.top,
                    0x0010, // SWP_NOACTIVATE
                );
                ShowWindow(monitor.overlay_hwnd, SW_SHOWNOACTIVATE);
            }
        }
    }
}
