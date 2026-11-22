# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/zuygui/goat-hooker/compare/v0.1.0...v0.2.0) - 2026-11-22

### Added

- *(gui)* node graph editor
- *(config)* reusable hooks files with params and with
- *(cli)* show the dependencies in run, dry-run and list
- *(runner)* run the hooks graph and pass values between hooks
- *(graph)* build the graph of the hooks of a trigger
- *(config)* add needs, inputs, outputs and steps to hooks

### Other

- run goat-hooker on itself
- *(readme)* document the hooks graph

## [0.1.0](https://github.com/zuygui/goat-hooker/releases/tag/v0.1.0) - 2026-11-22

### Added

- *(cli)* add status and an interactive add, show spinners and a summary
- *(runner)* report progress and keep the colors of captured outputs
- *(gui)* add goat-hooker-gui, a desktop app to manage the hooks
- *(cli)* add list, update, schema and run --dry-run commands
- *(runner)* run hooks in parallel and re-stage fixed files
- *(git)* keep unstaged changes out of pre-commit hooks
- *(config)* load hooks files from local paths and git repositories
- *(run)* pass matched files to commands and filter pre-push hooks
- *(matcher)* return matched files relative to each project
- *(config)* support glob imports and configs discovery
- *(git)* add pushed files and config files helpers
- *(cli)* add init, install, run and completion commands
- *(git)* add staged files and hooks dir helpers
- *(config)* support all git hook triggers and validate hooks
- *(config)* add recursive loader with cycle detection and unit tests
- *(matcher)* add support for exclude patterns in hook matching
- *(matcher)* match hooks to files using include patterns
- *(config)* load main & subproject configs recursively
- *(config)* add json config support
- config schema

### Other

- *(readme)* document the installers and the release process
- add crates.io metadata and share the version in the workspace
- *(readme)* document status, add and the run output
- *(readme)* document the new features and the desktop app
- build and test the whole workspace
- move hooks installation and trigger runs into the library
- *(readme)* document mono-repos support
- restore rust workflow with fmt and clippy checks
- restore readme, license and contributing guides
- *(matcher)* precompile patterns and match paths relative to projects
- *(deps)* add clap and colored, move tempfile to dev-dependencies
- initial commit
- initial commit
