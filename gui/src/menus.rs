//! Menu bar: native on macOS, drawn in the title bar on Linux and Windows.
//! The actions are handled by `HookerApp` (see `app.rs`), except `Quit`.

use gpui_kit::component::GlobalState;
use gpui_kit::*;

gpui_kit::actions!(
    goat_hooker,
    [
        OpenRepository,
        ScanFolder,
        CloseRepository,
        Quit,
        NewHook,
        RunPreCommit,
        InstallHooks,
        ReloadConfigs,
        ShowList,
        ShowGraph,
        ToggleTheme,
        GoAll,
        GoNextCommit,
        GoVariables,
        GoBrowse,
        GoReview,
        GoHistory,
        GoDoctor,
        OpenDocs,
        ReportIssue,
        OpenReleases,
    ]
);

const REPOSITORY: &str = "https://github.com/zuygui/goat-hooker";

/// Shortcuts, menus and the global actions
pub fn init(cx: &mut App) {
    // Cmd on macOS, Ctrl elsewhere
    macro_rules! key {
        ($keys:literal) => {
            if cfg!(target_os = "macos") {
                concat!("cmd-", $keys)
            } else {
                concat!("ctrl-", $keys)
            }
        };
    }
    cx.bind_keys([
        KeyBinding::new(key!("o"), OpenRepository, None),
        KeyBinding::new(key!("q"), Quit, None),
        KeyBinding::new(key!("n"), NewHook, None),
        KeyBinding::new(key!("r"), RunPreCommit, None),
        KeyBinding::new(key!("shift-r"), ReloadConfigs, None),
        KeyBinding::new(key!("1"), ShowList, None),
        KeyBinding::new(key!("2"), ShowGraph, None),
        KeyBinding::new(key!("shift-d"), ToggleTheme, None),
    ]);

    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &OpenDocs, cx| cx.open_url(&format!("{REPOSITORY}#readme")));
    cx.on_action(|_: &ReportIssue, cx| cx.open_url(&format!("{REPOSITORY}/issues/new")));
    cx.on_action(|_: &OpenReleases, cx| cx.open_url(crate::update::RELEASES));

    // Drawn by `AppMenuBar` on Linux and Windows
    GlobalState::global_mut(cx).set_app_menus(menus().into_iter().map(Menu::owned).collect());
    cx.set_menus(menus());
}

fn menus() -> Vec<Menu> {
    let mut file = vec![
        MenuItem::action("Open Repository…", OpenRepository),
        MenuItem::action("Scan a Folder…", ScanFolder),
        MenuItem::action("Close Repository", CloseRepository),
    ];
    let mut menus = Vec::new();
    if cfg!(target_os = "macos") {
        // The first menu is the one named after the app
        menus.push(Menu::new("Goat Hooker").items([
            MenuItem::action("Check for Updates", OpenReleases),
            MenuItem::separator(),
            MenuItem::action("Quit Goat Hooker", Quit),
        ]));
    } else {
        file.extend([MenuItem::separator(), MenuItem::action("Quit", Quit)]);
    }
    menus.extend([
        Menu::new("File").items(file),
        Menu::new("Hooks").items([
            MenuItem::action("New Hook…", NewHook),
            MenuItem::action("Run Pre-commit", RunPreCommit),
            MenuItem::separator(),
            MenuItem::action("Install Git Hooks", InstallHooks),
            MenuItem::action("Reload Configs", ReloadConfigs),
        ]),
        Menu::new("View").items([
            MenuItem::action("List", ShowList),
            MenuItem::action("Graph", ShowGraph),
            MenuItem::separator(),
            MenuItem::action("Toggle Dark Mode", ToggleTheme),
        ]),
        Menu::new("Go").items([
            MenuItem::action("All Hooks", GoAll),
            MenuItem::action("Next Commit", GoNextCommit),
            MenuItem::separator(),
            MenuItem::action("Variables", GoVariables),
            MenuItem::action("Browse Shared Hooks", GoBrowse),
            MenuItem::action("Review Updates", GoReview),
            MenuItem::action("History", GoHistory),
            MenuItem::action("Doctor", GoDoctor),
        ]),
        Menu::new("Help").items([
            MenuItem::action("Documentation", OpenDocs),
            MenuItem::action("Report an Issue", ReportIssue),
            MenuItem::action("Releases", OpenReleases),
        ]),
    ]);
    menus
}
