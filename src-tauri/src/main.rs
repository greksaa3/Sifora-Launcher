// The launcher is a GUI application: no console window is attached, in any
// build. Diagnostics go through the in-app ring buffer (`app::tail_log`) and
// through the renderer log bridge, so nothing is lost.
#![windows_subsystem = "windows"]

fn main() {
    sifora_launcher_lib::run();
}