use std::path::Path;

mod config;
mod errors;
mod matcher;

fn main() {
    let path = Path::new("hooker.config.json");

    match config::ConfigLoader::new().load_recursive(path) {
        Ok(configs) => {
            println!("Loaded {} configurations.", configs.len());
            let matcher = matcher::Matcher::new(&configs);

            // Exemple de test de matching
            let test_path = Path::new("src/main.rs");
            let matched_hooks = matcher.match_hooks(test_path);
            println!(
                "File {:?} matches {} hooks.",
                test_path,
                matched_hooks.len()
            );
            for hook in matched_hooks {
                println!(" - Hook: {}", hook.name);
            }
        }
        Err(e) => eprintln!("Error loading configurations: {}", e),
    }
}
