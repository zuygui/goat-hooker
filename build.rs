use std::{
    env, fs,
    path::{Path, PathBuf},
};

use schemars::schema_for;
#[path = "src/config/schema/mod.rs"]
mod config;

fn main() {
    println!("cargo::rerun-if-changed=src/config/*.rs");

    let schema = schema_for!(config::AppConfig);
    let content = serde_json::to_string_pretty(&schema).unwrap();

    let output = get_output_path().join("schema.json");
    fs::write(output, content).unwrap();
}

fn get_output_path() -> PathBuf {
    //<root or manifest path>/target/<profile>/
    let manifest_dir_string = env::var("CARGO_MANIFEST_DIR").unwrap();
    let build_type = env::var("PROFILE").unwrap();
    let path = Path::new(&manifest_dir_string)
        .join("target")
        .join(build_type);
    return PathBuf::from(path);
}
