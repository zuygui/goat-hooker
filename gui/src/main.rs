//! Goat Hooker: desktop app to see and manage the hooks of a repository

mod app;
mod assets;
mod canvas;
mod discover;
mod editor;
mod graph_view;
mod json_complete;
mod json_editor;
mod menus;
mod recents;
mod repo;
mod update;

use std::path::PathBuf;

use gpui_kit::component::TitleBar;
use gpui_kit::*;

fn main() {
    // Repository to open: the first argument, else the current folder if it is in a git repository.
    // Launched from the Dock or the Start menu, the app opens on the welcome page.
    let dir = std::env::args().nth(1).map(PathBuf::from).or_else(|| {
        std::env::current_dir()
            .ok()
            .filter(|dir| goat_hooker::git::repo_root(dir).is_ok())
    });

    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            canvas::bind_keys(cx);
            menus::init(cx);
            cx.on_window_closed(|cx, _| cx.quit()).detach();

            let bounds = Bounds::centered(None, size(px(1180.), px(760.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(860.), px(520.))),
                ..TitleBar::window_options()
            };

            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| app::HookerApp::new(dir, window, cx))
            })
            .expect("failed to open window");
            cx.activate(true);
        });
}
