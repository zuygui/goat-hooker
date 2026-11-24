# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.0](https://github.com/zuygui/goat-hooker/compare/v0.4.1...v1.0.0) - 2026-11-24

### Added

- hooks of the pre-commit ecosystem with `from` + `id`
- run lefthook, pre-commit and husky configs as they are
- *(config)* hooker.toml, with hooker.config.json still read
- a website on GitHub Pages

### Fixed

- read the index on Windows (core.filemode = false)
- timeout and hooker fix on Windows

### Other

- a full documentation
- real files in the matcher regex test (Windows paths)
- benchmark with prek and 1,000 staged files
- read the repository instead of starting git, and skip the shell
- Merge pull request #21 from zuygui/feat/site
- benchmark with Goat Hooker 0.4.0
- a quickstart page

## [0.4.1](https://github.com/zuygui/goat-hooker/compare/v0.4.0...v0.4.1) - 2026-11-23

### Other

- hooker ready to use

## [0.4.0](https://github.com/zuygui/goat-hooker/compare/v0.3.0...v0.4.0) - 2026-11-23

### Added

- *(gui)* command palette, live output, impact, project graph and fixes
- hooker fix, init templates, history export and notifications
- branches, targeted skip, timeout and hooker.local.json
- add hooker watch to run the pre-commit hooks ahead of the commit
- add hooker doctor and a Doctor page in the app
- *(gui)* add a menu bar with shortcuts
- run the hooks of the projects affected by a change

### Other

- Merge pull request #17 from zuygui/feat/smart
- rewrite the README and move the reference to docs/
- install only the Git hooks the configs use, and fewer git commands per run
- branches, skip, timeout, local changes, fix, templates and the new pages
- translate the last French comments to English

## [0.3.0](https://github.com/zuygui/goat-hooker/compare/v0.2.0...v0.3.0) - 2026-11-23

### Added

- Windows support
- Homebrew, cargo-binstall, install script and update notice
- history of the runs
- *(gui)* JSON editor with autocompletion and colors
- updates, changes and pinning of the hooks of other repositories
- hooker ci and a GitHub Action
- cache the hooks that passed
- a hook without permission cuts its lineage
- *(gui)* create and delete hooks on the canvas, show variables, legend
- *(gui)* variables, browse and review pages, import from other hooks managers
- *(cli)* search hooks shared on GitHub and add them
- variables shared between hooks, hook permissions and trusted remote hooks
- *(cli)* import the configs of husky, lint-staged, lefthook and pre-commit
- *(config)* share hooks files and add a catalog of ready-to-use hooks

### Fixed

- *(git)* give Git plain paths on Windows (without the \\?\ prefix)
- glob imports on Windows, and tests independent of line endings

### Other

- translate all the comments to English
- translate a comment to English
- Merge remote-tracking branch 'origin/main' into feat/cache-ci
- denied hooks cut their lineage

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
