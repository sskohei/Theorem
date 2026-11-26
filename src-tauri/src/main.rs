// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if let Some(success) = theorem_lib::pdf::compiler::run_pdf_worker_if_requested() {
        std::process::exit(if success { 0 } else { 1 });
    }
    theorem_lib::run()
}
