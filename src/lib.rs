//! Goat Hooker : gestionnaire de hooks Git (utilisé par la CLI `hooker` et l'app graphique)

pub mod cache;
pub mod config;
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
