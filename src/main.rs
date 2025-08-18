use crate::config::loader::load_cfg;

mod config;
mod errors;

fn main() {
    match load_cfg() {
        Ok(cfg) => {
            println!("Configuration loaded successfully: {:?}", cfg);
        }
        Err(e) => {
            eprintln!("Error loading configuration: {}", e);
            std::process::exit(1);
        }
    }
}
