// main.rs — thin passthrough to lib.rs (required for mobile builds)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    app_lib::run();
}