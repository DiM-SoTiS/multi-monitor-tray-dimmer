#![cfg_attr(not(test), windows_subsystem = "windows")]

mod autostart;
mod dimmer;
mod flyout;
mod flyout_ui;
mod i18n;
mod tray;
mod updater;
mod win32_monitors;

fn main() {
    #[cfg(windows)]
    tray::run_app();
}
