#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && args[1] == "doctor" {
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let report = rt.block_on(ruby_core::doctor::RubyDoctor::run_diagnostics(false));
        if args.iter().any(|a| a == "--json") {
            println!("{}", serde_json::to_string_pretty(&report).unwrap());
        } else {
            println!("{}", report.to_markdown());
        }
        return;
    }
    ruby_desktop::run();
}
