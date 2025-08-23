use std::path::Path;

mod config;
mod errors;
mod matcher;

fn main() {
    let path = Path::new("hooker.config.json");

    match config::Config::load_recursive(path) {
        Ok(configs) => {
            println!("✅ Loaded {} config(s)", configs.len());
            for cfg in configs {
                println!(
                    "- Config at {:?} with {} hooks",
                    cfg.base_dir,
                    cfg.hooks.as_ref().map(|h| h.len()).unwrap_or(0)
                );
            }
        }
        Err(e) => eprintln!("❌ Failed to load config: {}", e),
    }
}
