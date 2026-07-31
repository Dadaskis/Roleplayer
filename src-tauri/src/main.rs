//! Desktop entrypoint. On release builds the console is hidden.

// In release builds on Windows, switch the subsystem from "console" to
// "windows" so no black terminal window flashes beside the app; debug builds
// keep the console for logs. Compile-time only, and a no-op on other
// platforms where the subsystem attribute does not exist.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `run()` wires everything and starts the Tauri event loop; it does not
    // return until the app exits, so there is nothing to do after it.
    roleplayer_app_lib::run()
}
