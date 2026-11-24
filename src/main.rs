#![cfg_attr(not(test), windows_subsystem = "windows")]

mod dimmer;
mod tray;
mod win32_monitors;

fn main() {
    #[cfg(windows)]
    tray::run_app();
}
