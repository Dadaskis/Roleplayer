//! Desktop entrypoint. On release builds the console is hidden.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    roleplayer_app_lib::run()
}
