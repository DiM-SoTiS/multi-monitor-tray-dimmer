//! Windows startup registry integration (HKCU\Software\Microsoft\Windows\CurrentVersion\Run).

#![cfg(windows)]

use std::ffi::c_void;
use std::ptr::{null, null_mut};

type Hkey = isize;

const HKEY_CURRENT_USER: Hkey = -2147483647; // 0x80000001
const KEY_READ: u32 = 0x20019;
const KEY_WRITE: u32 = 0x20006;
const REG_SZ: u32 = 1;

const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE_NAME: &str = "MultiMonitorTrayDimmer";

#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(
        h_key: Hkey,
        lp_sub_key: *const u16,
        ul_options: u32,
        sam_desired: u32,
        phk_result: *mut Hkey,
    ) -> i32;
    fn RegQueryValueExW(
        h_key: Hkey,
        lp_value_name: *const u16,
        lp_reserved: *mut u32,
        lp_type: *mut u32,
        lp_data: *mut u8,
        lpcb_data: *mut u32,
    ) -> i32;
    fn RegSetValueExW(
        h_key: Hkey,
        lp_value_name: *const u16,
        reserved: u32,
        dw_type: u32,
        lp_data: *const u8,
        cb_data: u32,
    ) -> i32;
    fn RegDeleteValueW(h_key: Hkey, lp_value_name: *const u16) -> i32;
    fn RegCloseKey(h_key: Hkey) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleFileNameW(h_module: *mut c_void, lp_filename: *mut u16, n_size: u32) -> u32;
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn is_autostart_enabled() -> bool {
    unsafe {
        let subkey = to_wide(RUN_SUBKEY);
        let val_name = to_wide(VALUE_NAME);
        let mut hkey: Hkey = 0;
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return false;
        }
        let exists = RegQueryValueExW(
            hkey,
            val_name.as_ptr(),
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
        ) == 0;
        RegCloseKey(hkey);
        exists
    }
}

pub fn set_autostart_enabled(enabled: bool) -> Result<(), String> {
    unsafe {
        let subkey = to_wide(RUN_SUBKEY);
        let val_name = to_wide(VALUE_NAME);
        let mut hkey: Hkey = 0;
        let rc = RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_WRITE, &mut hkey);
        if rc != 0 {
            return Err(format!("RegOpenKeyExW failed: {rc}"));
        }

        let res = if enabled {
            let mut buf = [0u16; 1024];
            let len = GetModuleFileNameW(null_mut(), buf.as_mut_ptr(), buf.len() as u32) as usize;
            if len == 0 {
                Err("GetModuleFileNameW failed".to_string())
            } else {
                let exe_path = String::from_utf16_lossy(&buf[..len]);
                let quoted = to_wide(&format!("\"{exe_path}\""));
                let byte_len = (quoted.len() * 2) as u32;
                let set_rc = RegSetValueExW(
                    hkey,
                    val_name.as_ptr(),
                    0,
                    REG_SZ,
                    quoted.as_ptr() as *const u8,
                    byte_len,
                );
                if set_rc == 0 {
                    Ok(())
                } else {
                    Err(format!("RegSetValueExW failed: {set_rc}"))
                }
            }
        } else {
            let del_rc = RegDeleteValueW(hkey, val_name.as_ptr());
            // ERROR_FILE_NOT_FOUND (2) is fine when disabling
            if del_rc == 0 || del_rc == 2 {
                Ok(())
            } else {
                Err(format!("RegDeleteValueW failed: {del_rc}"))
            }
        };

        RegCloseKey(hkey);
        let _ = null::<u16>();
        res
    }
}
