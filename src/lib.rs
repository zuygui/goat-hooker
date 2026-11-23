//! Goat Hooker: Git hooks manager (used by the `hooker` CLI and the desktop app)

pub mod cache;
pub mod config;
pub mod deps;
pub mod doctor;
pub mod errors;
pub mod fix;
pub mod git;
pub mod graph;
pub mod history;
pub mod hub;
pub mod import;
pub mod install;
pub mod matcher;
pub mod notify;
pub mod remote;
pub mod runner;
pub mod templates;
pub mod trust;
pub mod variables;
pub mod watch;
