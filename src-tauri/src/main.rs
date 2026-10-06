// goglite — punkt wejścia
// Na Windows ukryj konsolę w trybie release

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    goglite::run();
}