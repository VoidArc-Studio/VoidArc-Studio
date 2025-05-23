mod compositor;
mod launcher;

use std::env;
use std::process;

fn main() {
    // Initialize logging
    env_logger::init();

    // Parse command-line arguments
    let args: Vec<String> = env::args().collect();
    let program = args.get(0).map(|s| s.as_str()).unwrap_or("blue-desktop-environment");

    // Determine which component to run based on arguments
    match args.get(1).map(|s| s.as_str()) {
        Some("--launcher") => {
            log::info!("Starting Blue Launcher");
            if let Err(e) = launcher::main() {
                log::error!("Launcher failed: {}", e);
                process::exit(1);
            }
        }
        Some("--compositor") | None => {
            log::info!("Starting Blue Compositor");
            if let Err(e) = compositor::main() {
                log::error!("Compositor failed: {}", e);
                process::exit(1);
            }
        }
        Some(arg) => {
            log::error!("Unknown argument: {}. Use --launcher or --compositor", arg);
            eprintln!("Usage: {} [--launcher | --compositor]", program);
            process::exit(1);
        }
    }
}
