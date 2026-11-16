#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod capture;
mod config;
mod core;
mod diagnostics;
mod difficulty;
mod errors;
mod hud;
mod ocr;
mod panel;
mod tracker;
mod ui;
mod vision;
mod worker;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args
        .get(1)
        .is_some_and(|s| s == "--ocr-file" || s == "--ocr-sequence")
    {
        let result = if args[1] == "--ocr-sequence" {
            diagnostics::sequence_report(&args[2..])
        } else {
            ocr::file_report(&args[2..])
        };
        let output = result.unwrap_or_else(|e| serde_json::json!({"error": e}));
        if let Some(path) = args
            .iter()
            .position(|a| a == "--output")
            .and_then(|i| args.get(i + 1))
        {
            let _ = std::fs::write(path, serde_json::to_string_pretty(&output).unwrap());
        } else {
            println!("{}", output);
        }
        if output.get("error").is_some() {
            std::process::exit(1);
        }
        return;
    }
    if let Err(e) = ui::run() {
        ui::error(&e);
    }
}
