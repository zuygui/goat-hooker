//! Goat Hooker: Git hooks manager (used by the `hooker` CLI and the desktop app)

pub mod cache;
pub mod config;
pub mod deps;
pub mod errors;
pub mod git;
pub mod graph;
pub mod history;
pub mod hub;
pub mod import;
pub mod install;
pub mod matcher;
pub mod remote;
pub mod runner;
pub mod trust;
pub mod variables;
