//! Goat Hooker : app graphique pour voir et gérer les hooks d'un dépôt

mod app;
mod assets;
mod canvas;
mod discover;
mod editor;
mod graph_view;
mod recents;
mod repo;

use std::path::PathBuf;

use gpui_kit::component::TitleBar;
use gpui_kit::*;

fn main() {
    // Dépôt à ouvrir : le premier argument, sinon le dossier courant s'il est dans un dépôt git.
    // Lancée depuis le Dock ou le menu Démarrer, l'app s'ouvre sur la page d'accueil.
    let dir = std::env::args().nth(1).map(PathBuf::from).or_else(|| {
        std::env::current_dir()
            .ok()
            .filter(|dir| goat_hooker::git::repo_root(dir).is_ok())
    });

    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
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
