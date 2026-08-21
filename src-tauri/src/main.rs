// Prevents a console window from appearing beside the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    mamacine_lib::run()
}
