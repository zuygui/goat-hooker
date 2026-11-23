//! Main window, in three panes:
//! filters (sidebar) → list of the hooks → detail of the selected hook, with a status bar at the bottom.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Sizable, Theme, ThemeMode, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    menu::AppMenuBar,
    scroll::ScrollableElement,
    status_bar::StatusBar,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use goat_hooker::{
    config::{Hook, Trigger},
    runner::HookResult,
};

use crate::{
    canvas::{CanvasEvent, GraphCanvas, NodeStatus},
    discover::{self, FoundRepo},
    editor::{EditorEvent, HookEditor},
    graph_view::ViewNode,
    menus::*,
    recents,
    repo::{PreviewItem, Project, Repo},
};

const REPOSITORY_URL: &str = "https://github.com/zuygui/goat-hooker";

/// What the middle list shows
#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    NextCommit,
    Trigger(Trigger),
    Project(usize),
    /// Pages that replace the list and the detail
    Variables,
    Browse,
    Review,
    History,
    Doctor,
    Impact,
    Projects,
    Fixes,
}

impl Filter {
    fn is_page(self) -> bool {
        matches!(
            self,
            Filter::Variables
                | Filter::Browse
                | Filter::Review
                | Filter::History
                | Filter::Doctor
                | Filter::Impact
                | Filter::Projects
                | Filter::Fixes
        )
    }
}

mod pages;
mod palette;
mod source;
mod tools;

/// A hook in a project
#[derive(Clone, Copy, PartialEq, Eq)]
struct HookRef {
    project: usize,
    hook: usize,
}

/// Key of a result: folder of the project + name of the hook
type RunKey = (PathBuf, String);

/// Result of the last run, in the list
enum RunStatus {
    Passed,
    Failed,
    Skipped,
    Denied,
}

/// Form open in the detail pane
struct OpenEditor {
    project: usize,
    view: Entity<HookEditor>,
}

pub struct HookerApp {
    repo: Option<Repo>,
    /// Error when opening the repository / loading the configs
    error: Option<String>,
    /// Temporary message (install, save...)
    notice: Option<(String, bool)>,
    filter: Filter,
    selected: Option<HookRef>,
    search: Entity<InputState>,
    editor: Option<OpenEditor>,
    preview: Vec<PreviewItem>,
    results: HashMap<RunKey, HookResult>,
    running: HashSet<RunKey>,
    running_all: bool,
    /// Recently opened repositories (welcome page)
    recents: Vec<PathBuf>,
    /// Repositories found on the computer (`None` while searching)
    found: Option<Vec<FoundRepo>>,
    show_all_found: bool,
    /// Node editor in place of the list
    graph_mode: bool,
    graph: Entity<GraphCanvas>,
    /// The graph must be computed again (config or results changed)
    graph_dirty: bool,
    pages: pages::PagesState,
    /// Config files before the last changes, and the undone ones
    undo_stack: Vec<Vec<(PathBuf, Option<String>)>>,
    /// A newer version was released
    update: Option<String>,
    redo_stack: Vec<Vec<(PathBuf, Option<String>)>>,
    /// JSON editor of a config, in place of the detail
    json_editor: Option<Entity<crate::json_editor::JsonEditor>>,
    /// Output of the hooks while they run
    live: std::sync::Arc<std::sync::Mutex<HashMap<RunKey, String>>>,
    /// The window is redrawn regularly while hooks run (live output)
    ticking: bool,
    /// Command palette (⌘K), when open
    palette: Option<Entity<gpui_kit::component::command::CommandState>>,
    /// Root of the window: receives the actions of the menus and the shortcuts
    focus_handle: FocusHandle,
    /// Menu bar in the title bar (Linux and Windows; native on macOS)
    menu_bar: Entity<AppMenuBar>,
    _subscriptions: Vec<Subscription>,
}

/// Lucide icon added by the app (see `assets.rs`)
fn lucide(name: &str) -> Icon {
    Icon::empty().path(format!("icons/{name}.svg"))
}

fn trigger_icon(trigger: Option<Trigger>) -> Icon {
    match trigger {
        Some(Trigger::PreCommit) => lucide("git-commit-horizontal"),
        Some(Trigger::PrepareCommitMsg | Trigger::CommitMsg | Trigger::ApplyPatchMsg) => {
            lucide("message-square")
        }
        Some(Trigger::PrePush) => lucide("upload"),
        Some(Trigger::PostMerge | Trigger::PreRebase | Trigger::PostRewrite) => lucide("git-merge"),
        Some(Trigger::PostCheckout) => lucide("git-branch"),
        Some(_) => lucide("zap"),
        None => lucide("package"),
    }
}

fn trigger_color(trigger: Option<Trigger>, cx: &App) -> Hsla {
    let theme = cx.theme();
    match trigger {
        Some(Trigger::PreCommit) => theme.blue,
        Some(Trigger::PrePush) => theme.magenta,
        Some(Trigger::CommitMsg | Trigger::PrepareCommitMsg) => theme.cyan,
        Some(_) => theme.yellow,
        None => theme.muted_foreground,
    }
}

fn project_label(project: &Project) -> String {
    if project.name == "." {
        "root".to_string()
    } else {
        project.name.clone()
    }
}

/// `1 project`, `2 projects`
fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n > 1 { "s" } else { "" })
}

/// Small colored label
fn chip(text: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .px_1p5()
        .py_0p5()
        .rounded_md()
        .text_xs()
        .bg(color.opacity(0.12))
        .text_color(color)
        .child(text.into())
}

fn section(title: &'static str, cx: &App) -> Div {
    div()
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(title)
}

impl HookerApp {
    /// Without a folder, the app opens on the welcome page
    pub fn new(dir: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search hooks"));
        let subscription = cx.subscribe(&search, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.select_first(cx);
                cx.notify();
            }
        });

        let graph = cx.new(GraphCanvas::new);
        let graph_subscription = cx.subscribe_in(
            &graph,
            window,
            |this, _, event: &CanvasEvent, window, cx| {
                this.on_canvas_event(event, window, cx);
                cx.notify();
            },
        );

        let mut app = Self {
            repo: None,
            error: None,
            notice: None,
            filter: Filter::All,
            selected: None,
            search,
            editor: None,
            preview: vec![],
            results: HashMap::new(),
            running: HashSet::new(),
            running_all: false,
            recents: recents::load(),
            found: None,
            show_all_found: false,
            graph_mode: false,
            graph,
            graph_dirty: true,
            pages: pages::PagesState::new(window, cx),
            undo_stack: vec![],
            update: None,
            redo_stack: vec![],
            json_editor: None,
            live: Default::default(),
            palette: None,
            ticking: false,
            focus_handle: cx.focus_handle(),
            menu_bar: AppMenuBar::new(cx),
            _subscriptions: vec![subscription, graph_subscription],
        };
        app.focus_handle.focus(window, cx);
        if let Some(dir) = dir {
            app.open(dir);
            app.select_first(cx);
        }
        app.scan_repositories(cx);
        cx.spawn(async move |this, cx| {
            let update = cx
                .background_spawn(async move { crate::update::newer_version() })
                .await;
            this.update(cx, |this, cx| {
                this.update = update;
                cx.notify();
            })
            .ok();
        })
        .detach();
        app
    }

    /// Actions of the menus that need a repository with a config
    fn on_menu_action(
        &mut self,
        action: &dyn std::any::Any,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action.is::<NewHook>() {
            self.open_editor(self.target_project(), None, window, cx);
        } else if action.is::<RunPreCommit>() {
            self.run_pre_commit(cx);
        } else if action.is::<RunPrePush>() {
            self.run_trigger(Trigger::PrePush, cx);
        } else if action.is::<InstallHooks>() {
            self.install(cx);
        } else if action.is::<ReloadConfigs>() {
            self.reload();
            cx.notify();
        } else if action.is::<ShowList>() || action.is::<ShowGraph>() {
            self.graph_mode = action.is::<ShowGraph>();
            self.graph_dirty = true;
            if self.filter.is_page() {
                self.set_filter(Filter::All, cx);
            }
            cx.notify();
        } else {
            let filter = if action.is::<GoAll>() {
                Filter::All
            } else if action.is::<GoNextCommit>() {
                Filter::NextCommit
            } else if action.is::<GoVariables>() {
                Filter::Variables
            } else if action.is::<GoBrowse>() {
                Filter::Browse
            } else if action.is::<GoReview>() {
                Filter::Review
            } else if action.is::<GoHistory>() {
                Filter::History
            } else if action.is::<GoDoctor>() {
                Filter::Doctor
            } else if action.is::<GoImpact>() {
                Filter::Impact
            } else if action.is::<GoProjects>() {
                Filter::Projects
            } else if action.is::<GoFixes>() {
                self.run_fixes(cx);
                Filter::Fixes
            } else {
                return;
            };
            self.graph_mode = false;
            self.set_filter(filter, cx);
        }
    }

    // MARK: data

    fn open(&mut self, dir: PathBuf) {
        self.filter = Filter::All;
        self.selected = None;
        self.editor = None;
        self.results.clear();
        match Repo::open(&dir) {
            Ok(repo) => {
                self.recents = recents::add(repo.root.clone());
                self.repo = Some(repo);
                self.error = None;
            }
            Err(err) => {
                self.repo = None;
                self.error = Some(err.to_string());
            }
        }
        self.refresh_preview();
        self.graph_dirty = true;
        self.refresh_pages();
    }

    fn reload(&mut self) {
        let Some(repo) = self.repo.as_mut() else {
            return;
        };
        self.error = repo.reload().err().map(|e| e.to_string());
        self.refresh_preview();
        self.graph_dirty = true;
        self.refresh_pages();
    }

    fn refresh_preview(&mut self) {
        self.preview = match self.repo.as_ref().map(|r| r.preview_pre_commit()) {
            Some(Ok(preview)) => preview,
            Some(Err(err)) => {
                self.error = Some(err.to_string());
                vec![]
            }
            None => vec![],
        };
    }

    fn run_key(&self, project: &Project, hook: &Hook) -> RunKey {
        let dir = project.dir();
        (dir.canonicalize().unwrap_or(dir), hook.name.clone())
    }

    /// Result of a hook, or of its steps together (`lint › eslint`, `lint › types`...)
    fn last_status(&self, key: &RunKey) -> Option<RunStatus> {
        let results: Vec<&HookResult> = match self.results.get(key) {
            Some(result) => vec![result],
            None => {
                let prefix = format!("{} › ", key.1);
                self.results
                    .iter()
                    .filter(|((dir, name), _)| *dir == key.0 && name.starts_with(&prefix))
                    .map(|(_, result)| result)
                    .collect()
            }
        };
        if results.is_empty() {
            None
        } else if results.iter().any(|r| r.denied) {
            Some(RunStatus::Denied)
        } else if results.iter().any(|r| r.failed()) {
            Some(RunStatus::Failed)
        } else if results.iter().all(|r| r.skipped) {
            Some(RunStatus::Skipped)
        } else {
            Some(RunStatus::Passed)
        }
    }

    fn preview_for(&self, project: &Project, hook: &Hook) -> Option<&PreviewItem> {
        let (dir, name) = self.run_key(project, hook);
        self.preview
            .iter()
            .find(|p| p.hook == name && p.project_dir == dir)
    }

    /// Hooks loaded from a shared file (`path`) that will run on the next commit
    fn shared_preview(&self, project: &Project) -> Vec<&PreviewItem> {
        let dir = project.dir();
        let dir = dir.canonicalize().unwrap_or(dir);
        self.preview
            .iter()
            .filter(|p| p.project_dir == dir)
            .filter(|p| {
                !project
                    .hooks()
                    .iter()
                    .any(|h| !h.is_import() && h.name == p.hook)
            })
            .collect()
    }

    fn runs_next_commit(&self, project: &Project, hook: &Hook) -> bool {
        if hook.is_import() {
            !self.shared_preview(project).is_empty()
        } else {
            self.preview_for(project, hook).is_some()
        }
    }

    /// Hooks of the middle list, by filter and search
    fn visible_hooks(&self, cx: &App) -> Vec<HookRef> {
        self.hooks_for(self.filter, cx)
    }

    fn hooks_for(&self, filter: Filter, cx: &App) -> Vec<HookRef> {
        let Some(repo) = self.repo.as_ref() else {
            return vec![];
        };
        let query = self.search.read(cx).value().to_lowercase();

        let mut hooks = Vec::new();
        for (p, project) in repo.projects.iter().enumerate() {
            for (h, hook) in project.hooks().iter().enumerate() {
                let keep = match filter {
                    Filter::All => true,
                    Filter::Project(i) => i == p,
                    Filter::Trigger(trigger) => hook.trigger == Some(trigger),
                    Filter::NextCommit => self.runs_next_commit(project, hook),
                    Filter::Variables
                    | Filter::Browse
                    | Filter::Review
                    | Filter::History
                    | Filter::Doctor
                    | Filter::Impact
                    | Filter::Projects
                    | Filter::Fixes => false,
                };
                let matches_search = query.is_empty()
                    || hook.name.to_lowercase().contains(&query)
                    || hook
                        .commands()
                        .iter()
                        .any(|c| c.to_lowercase().contains(&query));
                if keep && matches_search {
                    hooks.push(HookRef {
                        project: p,
                        hook: h,
                    });
                }
            }
        }
        hooks
    }

    fn select_first(&mut self, cx: &App) {
        let visible = self.visible_hooks(cx);
        if !self.selected.is_some_and(|s| visible.contains(&s)) {
            self.selected = visible.first().copied();
        }
    }

    fn set_filter(&mut self, filter: Filter, cx: &mut Context<Self>) {
        self.filter = filter;
        self.editor = None;
        self.json_editor = None;
        if filter == Filter::Browse {
            self.open_browse(cx);
        }
        if filter == Filter::NextCommit {
            self.reload();
        }
        self.selected = None;
        self.select_first(cx);
        cx.notify();
    }

    fn hook(&self, hook_ref: HookRef) -> Option<(&Project, &Hook)> {
        let project = self.repo.as_ref()?.projects.get(hook_ref.project)?;
        Some((project, project.hooks().get(hook_ref.hook)?))
    }

    // MARK: actions

    fn pick_folder(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open repository".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(dir) = paths.into_iter().next() else {
                return;
            };
            this.update(cx, |this, cx| {
                this.open(dir);
                this.select_first(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Back to the welcome page
    fn go_home(&mut self, cx: &mut Context<Self>) {
        self.repo = None;
        self.error = None;
        self.notice = None;
        self.editor = None;
        self.selected = None;
        self.recents = recents::load();
        cx.notify();
    }

    /// Searches the git repositories of the computer in the background
    fn scan_repositories(&mut self, cx: &mut Context<Self>) {
        self.found = None;
        let mut roots = discover::default_roots();
        roots.extend(recents::load_scan_roots());
        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn(async move { discover::find_repositories(&roots) })
                .await;
            this.update(cx, |this, cx| {
                this.found = Some(found);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Adds a folder to the scanned folders (e.g. Documents, which the app avoids by default)
    fn pick_scan_folder(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Scan this folder".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(dir) = paths.into_iter().next() else {
                return;
            };
            this.update(cx, |this, cx| {
                recents::add_scan_root(dir);
                this.scan_repositories(cx);
            })
            .ok();
        })
        .detach();
    }

    fn open_recent(&mut self, dir: PathBuf, cx: &mut Context<Self>) {
        self.open(dir);
        self.select_first(cx);
        cx.notify();
    }

    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mode = if cx.theme().mode.is_dark() {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
        Theme::change(mode, Some(window), cx);
        cx.notify();
    }

    fn install(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.repo.as_mut() else {
            return;
        };
        self.notice = Some(match repo.install() {
            Ok(skipped) if skipped.is_empty() => ("Git hooks installed".to_string(), true),
            Ok(skipped) => (
                format!(
                    "Git hooks installed, {} hook(s) written by hand were kept",
                    skipped.len()
                ),
                true,
            ),
            Err(err) => (err.to_string(), false),
        });
        cx.notify();
    }

    fn init_config(&mut self, cx: &mut Context<Self>) {
        if let Some(Err(err)) = self.repo.as_mut().map(|r| r.init_config()) {
            self.notice = Some((err.to_string(), false));
        }
        cx.notify();
    }

    /// Project to add a hook to: the one of the filter, else the one of the selected hook, else the root
    fn target_project(&self) -> usize {
        match (self.filter, self.selected) {
            (Filter::Project(p), _) => p,
            (_, Some(selected)) => selected.project,
            _ => 0,
        }
    }

    fn open_editor(
        &mut self,
        project: usize,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let hook = index
            .and_then(|i| self.hook(HookRef { project, hook: i }))
            .map(|(_, hook)| hook.clone());

        let compact = self.graph_mode;
        let view = cx.new(|cx| {
            let editor = HookEditor::new(index, hook.as_ref(), window, cx);
            if compact { editor.compact() } else { editor }
        });
        let subscription = cx.subscribe(&view, |this, _, event: &EditorEvent, cx| {
            match event {
                EditorEvent::Save { index, hook } => this.save_hook(*index, (**hook).clone(), cx),
                EditorEvent::Cancel => this.editor = None,
            }
            cx.notify();
        });
        self._subscriptions.push(subscription);
        self.editor = Some(OpenEditor { project, view });
        cx.notify();
    }

    fn save_hook(&mut self, index: Option<usize>, hook: Hook, cx: &App) {
        let Some(project_index) = self.editor.as_ref().map(|e| e.project) else {
            return;
        };
        self.checkpoint();
        let Some(project) = self
            .repo
            .as_mut()
            .and_then(|r| r.projects.get_mut(project_index))
        else {
            return;
        };

        let file = project.file.display().to_string();
        match project.save_hook(index, hook) {
            Ok(()) => {
                let hook_index = index.unwrap_or(project.hooks().len() - 1);
                self.editor = None;
                self.notice = Some((format!("Saved {file}"), true));
                self.reload();
                self.selected = Some(HookRef {
                    project: project_index,
                    hook: hook_index,
                });
                self.select_first(cx);
            }
            Err(err) => self.notice = Some((err.to_string(), false)),
        }
    }

    fn delete_hook(&mut self, hook_ref: HookRef, cx: &mut Context<Self>) {
        self.checkpoint();
        let Some(project) = self
            .repo
            .as_mut()
            .and_then(|r| r.projects.get_mut(hook_ref.project))
        else {
            return;
        };
        match project.delete_hook(hook_ref.hook) {
            Ok(()) => {
                self.reload();
                self.selected = None;
                self.select_first(cx);
            }
            Err(err) => self.notice = Some((err.to_string(), false)),
        }
        cx.notify();
    }

    /// Runs a hook in the background
    fn run_hook(&mut self, hook_ref: HookRef, cx: &mut Context<Self>) {
        let (Some(repo), Some((project, hook))) = (self.repo.clone(), self.hook(hook_ref)) else {
            return;
        };
        let key = self.run_key(project, hook);
        let project = project.clone();
        let name = hook.name.clone();
        self.running.insert(key.clone());
        self.live.lock().unwrap().insert(key.clone(), String::new());
        self.start_ticking(cx);
        cx.notify();

        let live = self.live.clone();
        let live_key = key.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let on_output = |text: &str| {
                        if let Some(buffer) = live.lock().unwrap().get_mut(&live_key) {
                            buffer.push_str(text);
                        }
                    };
                    repo.run_hook(&project, &name, Some(&on_output))
                })
                .await;
            this.update(cx, |this, cx| {
                this.running.remove(&key);
                this.live.lock().unwrap().remove(&key);
                match result {
                    Ok(result) => {
                        this.results.insert(key, result);
                        this.graph_dirty = true;
                    }
                    Err(err) => this.notice = Some((err.to_string(), false)),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Redraws the window while hooks run, to show their output
    fn start_ticking(&mut self, cx: &mut Context<Self>) {
        if self.ticking {
            return;
        }
        self.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(150))
                    .await;
                let busy = this
                    .update(cx, |this, cx| {
                        let busy = this.running_all || !this.running.is_empty();
                        this.ticking = busy;
                        cx.notify();
                        busy
                    })
                    .unwrap_or(false);
                if !busy {
                    break;
                }
            }
        })
        .detach();
    }

    /// Turns a hook off (or on) on this computer: `hooker.local.json`, used by Git too
    fn toggle_local(&mut self, hook_ref: HookRef, disabled: bool, cx: &mut Context<Self>) {
        let Some(name) = self.hook(hook_ref).map(|(_, h)| h.name.clone()) else {
            return;
        };
        if let Some(repo) = self.repo.as_mut()
            && let Err(err) = repo.set_locally_disabled(hook_ref.project, &name, disabled)
        {
            self.notice = Some((err.to_string(), false));
        }
        self.reload();
        cx.notify();
    }

    /// Runs the whole pre-commit, like `git commit`
    fn run_pre_commit(&mut self, cx: &mut Context<Self>) {
        self.run_trigger(Trigger::PreCommit, cx);
    }

    /// Runs all the hooks of a trigger, in the order of the graph
    fn run_trigger(&mut self, trigger: Trigger, cx: &mut Context<Self>) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        self.running_all = true;
        self.graph_dirty = true;
        self.start_ticking(cx);
        cx.notify();

        let live = self.live.clone();
        cx.spawn(async move |this, cx| {
            let results = cx
                .background_spawn(async move {
                    // Node index → key of its result, to show its output while it runs
                    let keys = std::sync::Mutex::new(Vec::<RunKey>::new());
                    let on_progress = |progress: goat_hooker::runner::Progress| match progress {
                        goat_hooker::runner::Progress::Planned(graph) => {
                            *keys.lock().unwrap() = graph
                                .nodes
                                .iter()
                                .map(|n| {
                                    let dir =
                                        n.base_dir.canonicalize().unwrap_or(n.base_dir.clone());
                                    (dir, n.label())
                                })
                                .collect();
                        }
                        goat_hooker::runner::Progress::Started(index) => {
                            if let Some(key) = keys.lock().unwrap().get(index) {
                                live.lock().unwrap().insert(key.clone(), String::new());
                            }
                        }
                        goat_hooker::runner::Progress::Output(index, text) => {
                            if let Some(key) = keys.lock().unwrap().get(index)
                                && let Some(buffer) = live.lock().unwrap().get_mut(key)
                            {
                                buffer.push_str(text);
                            }
                        }
                        goat_hooker::runner::Progress::Finished(..) => {}
                    };
                    repo.run_trigger(trigger, Some(&on_progress))
                })
                .await;
            this.update(cx, |this, cx| {
                this.running_all = false;
                this.live.lock().unwrap().clear();
                this.graph_dirty = true;
                match results {
                    Ok(results) => {
                        let denied = results.iter().filter(|r| r.denied).count();
                        let failed = results.iter().filter(|r| r.failed()).count() - denied;
                        let skipped = results.iter().filter(|r| r.skipped).count();
                        let name = trigger.as_str();
                        this.notice = Some(if failed == 0 && denied == 0 {
                            (format!("{name} passed ({} hooks)", results.len()), true)
                        } else {
                            let mut parts = Vec::new();
                            if failed > 0 {
                                parts.push(format!("{failed} failed"));
                            }
                            if denied > 0 {
                                parts.push(format!("{denied} denied"));
                            }
                            let mut message = format!("{name} failed: {}", parts.join(", "));
                            if skipped > 0 {
                                message.push_str(&format!(", {skipped} skipped"));
                            }
                            (message, false)
                        });
                        for result in results {
                            let dir = result
                                .base_dir
                                .canonicalize()
                                .unwrap_or(result.base_dir.clone());
                            this.results.insert((dir, result.name.clone()), result);
                        }
                    }
                    Err(err) => this.notice = Some((err.to_string(), false)),
                }
                this.reload();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // MARK: graph

    /// Triggers with at least one hook (in the order of `Trigger::ALL`)
    fn graph_triggers(&self) -> Vec<Trigger> {
        let Some(repo) = self.repo.as_ref() else {
            return vec![];
        };
        let configs = repo.load_configs().unwrap_or_default();
        Trigger::ALL
            .iter()
            .copied()
            .filter(|trigger| {
                configs
                    .iter()
                    .flat_map(|c| c.hooks.iter().flatten())
                    .any(|h| h.trigger == Some(*trigger))
            })
            .collect()
    }

    fn refresh_graph(&mut self, cx: &mut Context<Self>) {
        self.graph_dirty = false;
        let Some(repo) = self.repo.as_ref() else {
            return;
        };
        let triggers = self.graph_triggers();
        let mut trigger = self.graph.read(cx).trigger;
        if !triggers.contains(&trigger)
            && let Some(first) = triggers.first()
        {
            trigger = *first;
        }
        let nodes = repo.graph_view(trigger).map_err(|e| e.to_string());
        let statuses = self
            .results
            .iter()
            .map(|(key, result)| {
                (
                    key.clone(),
                    NodeStatus {
                        success: result.success,
                        skipped: result.skipped,
                        denied: result.denied,
                        cached: result.cached,
                        duration: result.duration,
                    },
                )
            })
            .collect();
        let running = self.running_all;
        let projects: Vec<String> = repo.projects.iter().map(|p| p.name.clone()).collect();
        let (can_undo, can_redo) = (!self.undo_stack.is_empty(), !self.redo_stack.is_empty());
        self.graph.update(cx, |graph, cx| {
            graph.set_history(can_undo, can_redo);
            graph.set_projects(projects);
            graph.trigger = trigger;
            graph.set_data(triggers, nodes, statuses, running, cx);
        });
    }

    fn on_canvas_event(
        &mut self,
        event: &CanvasEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let nodes: Vec<ViewNode> = self.graph.read(cx).nodes().to_vec();
        let trigger = self.graph.read(cx).trigger;
        if matches!(
            event,
            CanvasEvent::Link { .. }
                | CanvasEvent::RemoveNeed { .. }
                | CanvasEvent::RemoveInput { .. }
                | CanvasEvent::Create { .. }
                | CanvasEvent::Delete(_)
        ) {
            self.checkpoint();
        }
        let result = match event {
            CanvasEvent::Undo => {
                self.undo(false);
                Ok(())
            }
            CanvasEvent::Redo => {
                self.undo(true);
                Ok(())
            }
            CanvasEvent::SetTrigger(trigger) => {
                let trigger = *trigger;
                self.graph.update(cx, |graph, _| graph.trigger = trigger);
                Ok(())
            }
            CanvasEvent::Run(trigger) => {
                self.run_trigger(*trigger, cx);
                Ok(())
            }
            CanvasEvent::Select(index) => {
                // Edited next to the graph
                if let Some(hook_ref) = self.node_hook(&nodes[*index]) {
                    self.selected = Some(hook_ref);
                    self.json_editor = None;
                    self.open_editor(hook_ref.project, Some(hook_ref.hook), window, cx);
                }
                Ok(())
            }
            CanvasEvent::Link {
                target,
                source,
                link,
            } => self.repo.as_mut().map_or(Ok(()), |repo| {
                repo.add_link(trigger, &nodes[*target], &nodes[*source], link.clone())
            }),
            CanvasEvent::RemoveNeed { target, source } => {
                self.repo.as_mut().map_or(Ok(()), |repo| {
                    repo.remove_need(trigger, &nodes[*target], &nodes[*source])
                })
            }
            CanvasEvent::Create {
                project,
                name,
                cmd,
                link,
            } => self.create_from_canvas(trigger, *project, name, cmd, link.clone(), cx),
            CanvasEvent::Delete(index) => self
                .repo
                .as_mut()
                .map_or(Ok(()), |repo| repo.delete_node(trigger, &nodes[*index])),
            CanvasEvent::RemoveInput { target, name } => {
                self.repo.as_mut().map_or(Ok(()), |repo| {
                    repo.remove_input(trigger, &nodes[*target], name)
                })
            }
        };
        if let Err(err) = result {
            self.notice = Some((err.to_string(), false));
        } else if matches!(
            event,
            CanvasEvent::Link { .. }
                | CanvasEvent::RemoveNeed { .. }
                | CanvasEvent::RemoveInput { .. }
        ) {
            self.notice = None;
        }
        self.reload();
    }

    /// Opens a config file in the JSON editor
    fn open_json(&mut self, project: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(repo) = self.repo.as_ref() else {
            return;
        };
        let Some(p) = repo.projects.get(project) else {
            return;
        };
        let file = p.file.clone();
        let label = file
            .strip_prefix(&repo.root)
            .unwrap_or(&file)
            .display()
            .to_string();
        let known = repo.known(project);
        let view = cx.new(|cx| crate::json_editor::JsonEditor::new(file, label, known, window, cx));
        let subscription = cx.subscribe(
            &view,
            |this, _, event: &crate::json_editor::JsonEditorEvent, cx| {
                match event {
                    crate::json_editor::JsonEditorEvent::Saved => {
                        this.notice = Some(("Saved".into(), true));
                        this.reload();
                    }
                    crate::json_editor::JsonEditorEvent::Close => this.json_editor = None,
                }
                cx.notify();
            },
        );
        self._subscriptions.push(subscription);
        self.editor = None;
        self.json_editor = Some(view);
        cx.notify();
    }

    /// Panel next to the graph: the node being edited
    fn render_graph_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let selected = self.selected;
        let body: AnyElement = match (self.json_editor.as_ref(), self.editor.as_ref()) {
            (Some(json), _) => div().size_full().child(json.clone()).into_any_element(),
            (None, Some(editor)) => div()
                .size_full()
                .overflow_y_scrollbar()
                .child(div().p_4().child(editor.view.clone()))
                .into_any_element(),
            _ => div().into_any_element(),
        };
        v_flex()
            .w(px(if self.json_editor.is_some() {
                560.
            } else {
                420.
            }))
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(theme.border)
            .bg(theme.background)
            .child(
                h_flex()
                    .px_3()
                    .py_2()
                    .gap_1()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(div().flex_1())
                    .when_some(
                        selected.filter(|_| self.json_editor.is_none()),
                        |this, s| {
                            this.child(
                                Button::new("panel-json")
                                    .ghost()
                                    .xsmall()
                                    .icon(lucide("pencil"))
                                    .label("JSON")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.open_json(s.project, window, cx)
                                    })),
                            )
                        },
                    )
                    .child(
                        Button::new("panel-list")
                            .ghost()
                            .xsmall()
                            .icon(lucide("list"))
                            .label("Open in the list")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.graph_mode = false;
                                this.filter = Filter::All;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("panel-close")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.editor = None;
                                this.json_editor = None;
                                cx.notify();
                            })),
                    ),
            )
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    /// Hook created on the canvas, linked to the node its wire comes from
    fn create_from_canvas(
        &mut self,
        trigger: Trigger,
        project: usize,
        name: &str,
        cmd: &str,
        link: Option<(usize, crate::graph_view::Link)>,
        cx: &mut Context<Self>,
    ) -> Result<(), goat_hooker::errors::GoatHookerError> {
        let source = link
            .as_ref()
            .map(|(node, _)| self.graph.read(cx).nodes()[*node].clone());
        let Some(repo) = self.repo.as_mut() else {
            return Ok(());
        };
        repo.create_hook(trigger, project, name, cmd)?;
        if let (Some(source), Some((_, link))) = (source, link) {
            let project_dir = repo.projects[project].dir();
            let project_dir = project_dir.canonicalize().unwrap_or(project_dir);
            let created = repo.graph_view(trigger)?.into_iter().find(|n| {
                n.hook == name.trim()
                    && n.project_dir
                        .canonicalize()
                        .unwrap_or(n.project_dir.clone())
                        == project_dir
            });
            if let Some(created) = created {
                repo.add_link(trigger, &created, &source, link)?;
            }
        }
        Ok(())
    }

    /// Hook of the list a node belongs to
    fn node_hook(&self, node: &ViewNode) -> Option<HookRef> {
        let repo = self.repo.as_ref()?;
        let dir = node
            .project_dir
            .canonicalize()
            .unwrap_or(node.project_dir.clone());
        repo.projects.iter().enumerate().find_map(|(p, project)| {
            let project_dir = project.dir();
            if project_dir.canonicalize().unwrap_or(project_dir) != dir {
                return None;
            }
            let hooks = project.hooks();
            hooks
                .iter()
                .position(|h| h.name == node.hook)
                .or_else(|| hooks.iter().position(|h| h.is_import()))
                .map(|h| HookRef {
                    project: p,
                    hook: h,
                })
        })
    }

    // MARK: undo

    /// Content of the config files
    fn snapshot(&self) -> Vec<(PathBuf, Option<String>)> {
        self.repo
            .iter()
            .flat_map(|r| r.projects.iter())
            .map(|p| (p.file.clone(), std::fs::read_to_string(&p.file).ok()))
            .collect()
    }

    /// Before a change: it can be undone
    fn checkpoint(&mut self) {
        let snapshot = self.snapshot();
        self.undo_stack.push(snapshot);
        if self.undo_stack.len() > 50 {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Undo (or redo) the last change of the config files
    fn undo(&mut self, redo: bool) {
        let (from, to) = if redo {
            (&mut self.redo_stack, &mut self.undo_stack)
        } else {
            (&mut self.undo_stack, &mut self.redo_stack)
        };
        let Some(snapshot) = from.pop() else {
            return;
        };
        let current: Vec<(PathBuf, Option<String>)> = snapshot
            .iter()
            .map(|(file, _)| (file.clone(), std::fs::read_to_string(file).ok()))
            .collect();
        to.push(current);
        for (file, content) in snapshot {
            let result = match content {
                Some(content) => std::fs::write(&file, content),
                None => std::fs::remove_file(&file),
            };
            if let Err(err) = result {
                self.notice = Some((err.to_string(), false));
            }
        }
        self.editor = None;
        self.reload();
    }

    // MARK: render

    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let repo_name = self
            .repo
            .as_ref()
            .and_then(|r| r.root.file_name())
            .map(|n| n.to_string_lossy().to_string());

        TitleBar::new().child(
            h_flex()
                .w_full()
                .pr_2()
                .gap_2()
                .justify_between()
                .child(
                    h_flex()
                        .gap_1p5()
                        .text_sm()
                        .child(div().font_weight(FontWeight::SEMIBOLD).child("Goat Hooker"))
                        .when(!cfg!(target_os = "macos"), |this| {
                            this.child(self.menu_bar.clone())
                        })
                        .when_some(self.update.clone(), |this, version| {
                            this.child(
                                Button::new("update-available")
                                    .xsmall()
                                    .primary()
                                    .label(format!("v{version} available"))
                                    .tooltip("Open the release to download it")
                                    .on_click(|_, _, cx| cx.open_url(crate::update::RELEASES)),
                            )
                        })
                        .when_some(repo_name, |this, repo_name| {
                            this.child(div().text_color(theme.muted_foreground).child("—"))
                                .child(
                                    lucide("folder-git-2")
                                        .small()
                                        .text_color(theme.muted_foreground),
                                )
                                .child(div().text_color(theme.muted_foreground).child(repo_name))
                        }),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .when(
                            self.repo.as_ref().is_some_and(|r| !r.projects.is_empty()),
                            |this| {
                                let list = Button::new("mode-list")
                                    .xsmall()
                                    .icon(lucide("list"))
                                    .label("List")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.graph_mode = false;
                                        cx.notify();
                                    }));
                                let graph = Button::new("mode-graph")
                                    .xsmall()
                                    .icon(lucide("workflow"))
                                    .label("Graph")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.graph_mode = true;
                                        this.graph_dirty = true;
                                        cx.notify();
                                    }));
                                let (list, graph) = if self.graph_mode {
                                    (list.ghost(), graph.primary())
                                } else {
                                    (list.primary(), graph.ghost())
                                };
                                this.child(list).child(graph).child(div().w_2())
                            },
                        )
                        .when(self.repo.is_some(), |this| {
                            this.child(
                                Button::new("home")
                                    .ghost()
                                    .xsmall()
                                    .icon(lucide("house"))
                                    .tooltip("Home")
                                    .on_click(cx.listener(|this, _, _, cx| this.go_home(cx))),
                            )
                        })
                        .child(
                            Button::new("theme")
                                .ghost()
                                .xsmall()
                                .icon(if theme.mode.is_dark() {
                                    lucide("sun")
                                } else {
                                    lucide("moon")
                                })
                                .tooltip("Toggle dark mode")
                                .on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.toggle_theme(window, cx)
                                    }),
                                ),
                        )
                        .child(
                            Button::new("open")
                                .ghost()
                                .xsmall()
                                .icon(IconName::FolderOpen)
                                .label("Open…")
                                .on_click(cx.listener(|this, _, _, cx| this.pick_folder(cx))),
                        ),
                ),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn nav_row(
        &self,
        id: impl Into<ElementId>,
        icon: impl Into<Icon>,
        icon_color: Option<Hsla>,
        label: impl Into<SharedString>,
        count: Option<usize>,
        filter: Filter,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let selected = self.filter == filter;
        let icon: Icon = icon.into();

        h_flex()
            .id(id)
            .px_2()
            .py_1()
            .gap_2()
            .rounded_md()
            .text_sm()
            .cursor_pointer()
            .when(selected, |this| {
                this.bg(theme.sidebar_accent)
                    .text_color(theme.sidebar_accent_foreground)
                    .font_weight(FontWeight::MEDIUM)
            })
            .hover(|this| this.bg(theme.sidebar_accent.opacity(0.6)))
            .child(
                icon.small()
                    .text_color(icon_color.unwrap_or(theme.muted_foreground)),
            )
            .child(div().flex_1().truncate().child(label.into()))
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .children(count.map(|c| c.to_string())),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.set_filter(filter, cx)))
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let mut nav = v_flex().gap_0p5();
        let mut triggers = v_flex().gap_0p5();
        let mut projects = v_flex().gap_0p5();

        if let Some(repo) = self.repo.as_ref() {
            let total: usize = repo.projects.iter().map(|p| p.hooks().len()).sum();
            nav = nav
                .child(self.nav_row(
                    "all",
                    lucide("layers"),
                    None,
                    "All hooks",
                    Some(total),
                    Filter::All,
                    cx,
                ))
                .child(self.nav_row(
                    "next",
                    lucide("list-checks"),
                    None,
                    "Next commit",
                    Some(self.hooks_for(Filter::NextCommit, cx).len()),
                    Filter::NextCommit,
                    cx,
                ));

            for trigger in Trigger::ALL {
                let count = repo
                    .projects
                    .iter()
                    .flat_map(|p| p.hooks())
                    .filter(|h| h.trigger == Some(trigger))
                    .count();
                if count > 0 {
                    triggers = triggers.child(self.nav_row(
                        SharedString::from(format!("trigger-{}", trigger.as_str())),
                        trigger_icon(Some(trigger)),
                        Some(trigger_color(Some(trigger), cx)),
                        trigger.as_str(),
                        Some(count),
                        Filter::Trigger(trigger),
                        cx,
                    ));
                }
            }

            for (i, project) in repo.projects.iter().enumerate() {
                projects = projects.child(self.nav_row(
                    ("project", i),
                    IconName::Folder,
                    None,
                    project_label(project),
                    Some(project.hooks().len()),
                    Filter::Project(i),
                    cx,
                ));
            }
        }

        v_flex()
            .w(px(230.))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(theme.sidebar_border)
            .bg(theme.sidebar)
            .text_color(theme.sidebar_foreground)
            .child(
                div().p_2().child(
                    Input::new(&self.search)
                        .small()
                        .prefix(
                            Icon::new(IconName::Search)
                                .small()
                                .text_color(theme.muted_foreground),
                        )
                        .cleanable(true),
                ),
            )
            .child(
                div().flex_1().overflow_y_scrollbar().child(
                    v_flex()
                        .px_2()
                        .pb_3()
                        .gap_4()
                        .child(nav)
                        .child(
                            v_flex()
                                .gap_1()
                                .child(div().px_2().child(section("TRIGGERS", cx)))
                                .child(triggers),
                        )
                        .child(
                            v_flex()
                                .gap_1()
                                .child(div().px_2().child(section("PROJECTS", cx)))
                                .child(projects),
                        )
                        .child(
                            v_flex()
                                .gap_1()
                                .child(div().px_2().child(section("TOOLS", cx)))
                                .child(self.tools_rows(cx)),
                        ),
                ),
            )
    }

    fn list_title(&self) -> String {
        match self.filter {
            Filter::All => "All hooks".to_string(),
            Filter::NextCommit => "Next commit".to_string(),
            Filter::Trigger(trigger) => trigger.as_str().to_string(),
            Filter::Variables => "Variables".to_string(),
            Filter::Browse => "Browse hooks".to_string(),
            Filter::Review => "Review".to_string(),
            Filter::History => "History".to_string(),
            Filter::Doctor => "Doctor".to_string(),
            Filter::Impact => "Impact".to_string(),
            Filter::Projects => "Project graph".to_string(),
            Filter::Fixes => "Fix files".to_string(),
            Filter::Project(i) => self
                .repo
                .as_ref()
                .and_then(|r| r.projects.get(i))
                .map(project_label)
                .unwrap_or_default(),
        }
    }

    fn render_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let hooks = self.visible_hooks(cx);
        let count = hooks.len();

        let mut list = v_flex().gap_0p5().p_2();
        for hook_ref in hooks {
            let Some((project, hook)) = self.hook(hook_ref) else {
                continue;
            };
            let selected = self.selected == Some(hook_ref);
            let key = self.run_key(project, hook);
            let status = if self.running.contains(&key) {
                Some(
                    Icon::new(IconName::LoaderCircle)
                        .xsmall()
                        .text_color(theme.muted_foreground),
                )
            } else {
                self.last_status(&key).map(|status| match status {
                    RunStatus::Passed => Icon::new(IconName::CircleCheck)
                        .xsmall()
                        .text_color(theme.success),
                    RunStatus::Failed => Icon::new(IconName::CircleX)
                        .xsmall()
                        .text_color(theme.danger),
                    RunStatus::Denied => lucide("shield-alert").xsmall().text_color(theme.danger),
                    RunStatus::Skipped => Icon::new(IconName::Minus)
                        .xsmall()
                        .text_color(theme.muted_foreground),
                })
            };
            let staged = if hook.is_import() {
                let shared = self.shared_preview(project);
                (!shared.is_empty()).then(|| shared.iter().map(|p| p.files.len()).sum())
            } else {
                self.preview_for(project, hook).map(|p| p.files.len())
            };
            let id = hook_ref.project * 10_000 + hook_ref.hook;

            list = list.child(
                v_flex()
                    .id(("hook", id))
                    .px_3()
                    .py_2()
                    .gap_1()
                    .rounded_md()
                    .cursor_pointer()
                    .when(selected, |this| this.bg(theme.accent))
                    .hover(|this| this.bg(theme.accent.opacity(0.6)))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                trigger_icon(hook.trigger)
                                    .small()
                                    .text_color(trigger_color(hook.trigger, cx)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_sm()
                                    .truncate()
                                    .when(selected, |this| this.font_weight(FontWeight::MEDIUM))
                                    .child(hook.name.clone()),
                            )
                            .children(status),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .pl_6()
                            .child(chip(project_label(project), theme.muted_foreground))
                            .when_some(hook.trigger, |this, trigger| {
                                this.child(chip(trigger.as_str(), trigger_color(Some(trigger), cx)))
                            })
                            .when(hook.is_import(), |this| {
                                this.child(chip("shared", theme.info))
                            })
                            .when_some(staged, |this, n| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child(format!("{n} file{}", if n > 1 { "s" } else { "" })),
                                )
                            }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected = Some(hook_ref);
                        this.json_editor = None;
                        this.editor = None;
                        cx.notify();
                    })),
            );
        }
        if count == 0 {
            list = list.child(
                div()
                    .p_6()
                    .text_sm()
                    .text_center()
                    .text_color(theme.muted_foreground)
                    .child(match self.filter {
                        Filter::NextCommit => "No pre-commit hook targets the staged files",
                        _ => "No hooks",
                    }),
            );
        }

        let header_action = if self.filter == Filter::NextCommit {
            Button::new("run-all")
                .primary()
                .xsmall()
                .icon(IconName::Play)
                .label("Run all")
                .loading(self.running_all)
                .on_click(cx.listener(|this, _, _, cx| this.run_pre_commit(cx)))
        } else {
            Button::new("add")
                .ghost()
                .xsmall()
                .icon(IconName::Plus)
                .tooltip("Add a hook")
                .on_click(cx.listener(|this, _, window, cx| {
                    let project = this.target_project();
                    this.open_editor(project, None, window, cx)
                }))
        };

        v_flex()
            .w(px(320.))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .px_4()
                    .h(px(52.))
                    .gap_2()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.list_title()),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(count.to_string()),
                    )
                    .child(div().flex_1())
                    .child(header_action),
            )
            .child(div().flex_1().overflow_y_scrollbar().child(list))
    }

    fn render_detail(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();

        if let Some(json) = self.json_editor.as_ref() {
            return div().size_full().child(json.clone()).into_any_element();
        }

        if let Some(editor) = self.editor.as_ref() {
            return div()
                .size_full()
                .overflow_y_scrollbar()
                .child(div().p_6().child(editor.view.clone()))
                .into_any_element();
        }

        let Some((hook_ref, (project, hook))) = self
            .selected
            .and_then(|s| self.hook(s).map(|found| (s, found)))
        else {
            return self.render_placeholder("Select a hook", None, cx);
        };

        let key = self.run_key(project, hook);
        let running = self.running.contains(&key);
        let uses_files = matches!(hook.trigger, Some(Trigger::PreCommit | Trigger::PrePush));
        let off_here = self
            .repo
            .as_ref()
            .is_some_and(|r| r.locally_disabled(project).contains(&hook.name));
        // Output while it runs (alone, or with its trigger)
        let live_output = (running || self.running_all)
            .then(|| self.live.lock().unwrap().get(&key).cloned())
            .flatten();

        // Header: project / trigger, name, actions
        let header = v_flex()
            .gap_2()
            .child(
                h_flex()
                    .gap_1p5()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(Icon::new(IconName::Folder).xsmall())
                    .child(project_label(project))
                    .when_some(hook.trigger, |this, trigger| {
                        this.child("/")
                            .child(trigger_icon(Some(trigger)).xsmall())
                            .child(trigger.as_str())
                    }),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .text_2xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .truncate()
                            .child(hook.name.clone()),
                    )
                    .when(!hook.is_import(), |this| {
                        this.child(
                            Button::new("run")
                                .primary()
                                .small()
                                .icon(IconName::Play)
                                .label("Run")
                                .loading(running)
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.run_hook(hook_ref, cx)),
                                ),
                        )
                        .child(
                            Button::new("edit")
                                .ghost()
                                .small()
                                .icon(lucide("pencil"))
                                .tooltip("Edit")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open_editor(
                                        hook_ref.project,
                                        Some(hook_ref.hook),
                                        window,
                                        cx,
                                    )
                                })),
                        )
                    })
                    .when(!hook.is_import(), |this| {
                        this.child(
                            Button::new("local-off")
                                .ghost()
                                .small()
                                .icon(lucide(if off_here { "eye" } else { "eye-off" }))
                                .tooltip(if off_here {
                                    "Turn it on again on this computer"
                                } else {
                                    "Turn it off on this computer only (hooker.local.json)"
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.toggle_local(hook_ref, !off_here, cx)
                                })),
                        )
                    })
                    .child(
                        Button::new("delete")
                            .ghost()
                            .small()
                            .icon(lucide("trash"))
                            .tooltip(if hook.is_import() { "Remove" } else { "Delete" })
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.delete_hook(hook_ref, cx)),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .when_some(hook.trigger, |this, trigger| {
                        this.child(chip(trigger.as_str(), trigger_color(Some(trigger), cx)))
                    })
                    .when(hook.stage_fixed.unwrap_or(false), |this| {
                        this.child(chip("re-stage fixed files", theme.success))
                    })
                    .when(hook.is_import(), |this| {
                        this.child(chip("shared hooks", theme.info))
                    })
                    .when(off_here, |this| {
                        this.child(chip("off on this computer", theme.warning))
                    })
                    .when_some(hook.timeout.clone(), |this, timeout| {
                        this.child(chip(format!("timeout {timeout}"), theme.muted_foreground))
                    })
                    .when_some(hook.branches.clone(), |this, branches| {
                        this.child(chip(format!("on {}", branches.join(", ")), theme.info))
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(project.file.display().to_string()),
            );

        let card = || {
            v_flex()
                .gap_2()
                .p_4()
                .rounded_lg()
                .bg(theme.secondary.opacity(0.5))
                .border_1()
                .border_color(theme.border)
        };
        let code_block = || {
            v_flex()
                .gap_0p5()
                .p_3()
                .rounded_md()
                .bg(theme.background)
                .border_1()
                .border_color(theme.border)
                .font_family("monospace")
                .text_xs()
        };

        let mut body = v_flex().gap_3();

        if hook.is_import() {
            let source = match hook.from.as_ref() {
                Some(url) => format!(
                    "`{}` from {url}{}",
                    hook.path.clone().unwrap_or_default(),
                    hook.rev
                        .as_ref()
                        .map(|r| format!(" ({r})"))
                        .unwrap_or_default()
                ),
                None => format!("`{}`", hook.path.clone().unwrap_or_default()),
            };
            let shared = self.shared_preview(project);
            if !shared.is_empty() {
                let mut next = code_block();
                for item in shared {
                    next = next.child(
                        h_flex()
                            .gap_1p5()
                            .child(
                                lucide("git-commit-horizontal")
                                    .xsmall()
                                    .text_color(theme.blue),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(item.hook.clone()),
                            )
                            .child(
                                div().text_color(theme.muted_foreground).child(
                                    item.files
                                        .iter()
                                        .map(|f| f.display().to_string())
                                        .collect::<Vec<_>>()
                                        .join(", "),
                                ),
                            ),
                    );
                }
                body = body.child(card().child(section("NEXT COMMIT", cx)).child(next));
            }
            body = body.child(
                card()
                    .child(section("SHARED HOOKS", cx))
                    .child(div().text_sm().child(format!("Loads the hooks of {source}.")))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(
                        "They run from this project, with the permissions of hooks from other repositories.",
                    )),
            );
            if let Some(selected) = self.selected
                && let Some(remote) = self.remote_card(selected, hook, cx)
            {
                body = body.child(remote);
            }
            body = body.child(self.source_card(project, hook, cx));
            return self.wrap_detail(header, body);
        }

        if uses_files {
            let mut patterns = h_flex().gap_1().flex_wrap();
            match hook.include.as_ref() {
                Some(include) => {
                    for pattern in include {
                        patterns = patterns.child(chip(pattern.clone(), theme.foreground));
                    }
                }
                None => patterns = patterns.child(chip("all files", theme.foreground)),
            }
            for pattern in hook.exclude.iter().flatten() {
                patterns = patterns.child(chip(format!("not {pattern}"), theme.danger));
            }

            let mut files_card = card().child(section("FILES", cx)).child(patterns);
            if let Some(preview) = self.preview_for(project, hook) {
                let mut staged = code_block();
                for file in &preview.files {
                    staged = staged.child(
                        h_flex()
                            .gap_1p5()
                            .child(
                                Icon::new(IconName::File)
                                    .xsmall()
                                    .text_color(theme.muted_foreground),
                            )
                            .child(file.display().to_string()),
                    );
                }
                files_card = files_card
                    .child(
                        div()
                            .pt_2()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Staged files it will check"),
                    )
                    .child(staged);
            }
            body = body.child(files_card);
        }

        let mut commands = code_block();
        for cmd in hook.commands() {
            commands = commands.child(
                h_flex()
                    .gap_2()
                    .child(div().text_color(theme.muted_foreground).child("$"))
                    .child(cmd.to_string()),
            );
        }
        body = body.child(card().child(section("COMMANDS", cx)).child(commands));
        if !hook.is_import()
            && let Some(access) = self.hook_access(hook, cx)
        {
            body = body.child(card().child(section("ACCESS", cx)).child(access));
        }
        body = body.child(self.source_card(project, hook, cx));

        // Last run
        let mut last_run = card().child(section("LAST RUN", cx));
        if let Some(text) = live_output {
            let lines: Vec<&str> = text.lines().collect();
            let start = lines.len().saturating_sub(300);
            let mut output = code_block();
            for line in &lines[start..] {
                output = output.child(div().child(line.to_string()));
            }
            last_run = last_run
                .child(
                    h_flex()
                        .gap_1p5()
                        .text_sm()
                        .child(lucide("clock").small().text_color(theme.primary))
                        .child("Running…"),
                )
                .when(!lines.is_empty(), |this| this.child(output));
        } else {
            match self.results.get(&key) {
                None => {
                    last_run = last_run.child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(if running { "Running…" } else { "Not run yet" }),
                    );
                }
                Some(result) => {
                    let mut output = code_block();
                    for cmd in &result.commands {
                        output = output.child(
                            div()
                                .text_color(theme.muted_foreground)
                                .child(format!("$ {}", cmd.cmd)),
                        );
                        for line in cmd.output.lines() {
                            output = output.child(div().child(line.to_string()));
                        }
                    }
                    last_run =
                        last_run
                            .child(
                                h_flex()
                                    .gap_1p5()
                                    .text_sm()
                                    .child(if result.skipped {
                                        Icon::new(IconName::Minus)
                                            .small()
                                            .text_color(theme.muted_foreground)
                                    } else if result.success {
                                        Icon::new(IconName::CircleCheck)
                                            .small()
                                            .text_color(theme.success)
                                    } else {
                                        Icon::new(IconName::CircleX)
                                            .small()
                                            .text_color(theme.danger)
                                    })
                                    .child(if result.skipped {
                                        "Skipped (a hook before it failed or was denied)"
                                    } else if result.denied {
                                        "Denied: it did or needed what it isn't allowed to"
                                    } else if result.cached {
                                        "Passed (cached: same files as a run that passed)"
                                    } else if result.success {
                                        "Passed"
                                    } else {
                                        "Failed"
                                    })
                                    .child(div().text_color(theme.muted_foreground).child(
                                        format!("in {:.2}s", result.duration.as_secs_f64()),
                                    )),
                            )
                            .child(output);
                }
            }
        }
        body = body.child(last_run);

        self.wrap_detail(header, body)
    }

    fn wrap_detail(&self, header: Div, body: Div) -> AnyElement {
        div()
            .size_full()
            .overflow_y_scrollbar()
            .child(v_flex().p_6().gap_5().child(header).child(body))
            .into_any_element()
    }

    fn render_placeholder(
        &self,
        message: &str,
        action: Option<AnyElement>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .child(div().text_3xl().child("🐐"))
            .child(
                div()
                    .text_color(theme.muted_foreground)
                    .child(message.to_string()),
            )
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    div()
                        .max_w(px(480.))
                        .text_sm()
                        .text_color(theme.danger)
                        .child(error),
                )
            })
            .children(action)
            .into_any_element()
    }

    /// A repository row on the welcome page (click: opens it)
    fn repo_row(
        &self,
        id: impl Into<ElementId>,
        path: &std::path::Path,
        configured: bool,
        removable: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let id: ElementId = id.into();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let open_path = path.to_path_buf();
        let remove_path = path.to_path_buf();

        h_flex()
            .id(id.clone())
            .px_3()
            .py_2()
            .gap_3()
            .rounded_md()
            .cursor_pointer()
            .hover(|this| this.bg(theme.accent))
            .child(
                lucide("folder-git-2")
                    .small()
                    .text_color(theme.muted_foreground),
            )
            .child(
                v_flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(name))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .truncate()
                            .child(path.display().to_string()),
                    ),
            )
            .child(if configured {
                chip("configured", theme.success)
            } else {
                chip("not set up", theme.muted_foreground)
            })
            .when(removable, |this| {
                this.child(
                    Button::new(ElementId::NamedChild(
                        std::sync::Arc::new(id),
                        "remove".into(),
                    ))
                    .ghost()
                    .xsmall()
                    .icon(lucide("x"))
                    .tooltip("Remove from the list")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.recents = recents::remove(&remove_path);
                        cx.notify();
                    })),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| this.open_recent(open_path.clone(), cx)))
    }

    /// Welcome page: open a repository, recent repositories, getting started
    fn render_welcome(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();

        let mut recent = v_flex().gap_0p5();
        for (i, path) in self.recents.iter().enumerate() {
            let configured = path
                .join(goat_hooker::config::HOOK_CONFIG_FILENAME)
                .exists();
            recent = recent.child(self.repo_row(("recent", i), path, configured, true, cx));
        }
        if self.recents.is_empty() {
            recent = recent.child(
                div()
                    .px_3()
                    .py_2()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child("The repositories you open will appear here"),
            );
        }

        // Repositories found on the computer (without the recent ones)
        let mut on_computer = v_flex().gap_0p5();
        match self.found.as_ref() {
            None => {
                on_computer = on_computer.child(
                    h_flex()
                        .px_3()
                        .py_2()
                        .gap_2()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(Icon::new(IconName::LoaderCircle).small())
                        .child("Looking for git repositories…"),
                );
            }
            Some(found) => {
                let found: Vec<&FoundRepo> = found
                    .iter()
                    .filter(|r| !self.recents.contains(&r.path))
                    .collect();
                let shown = if self.show_all_found { found.len() } else { 6 };
                for (i, repo) in found.iter().take(shown).enumerate() {
                    on_computer = on_computer.child(self.repo_row(
                        ("found", i),
                        &repo.path,
                        repo.configured,
                        false,
                        cx,
                    ));
                }
                if found.is_empty() {
                    on_computer = on_computer.child(
                        div()
                            .px_3()
                            .py_2()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("No other git repository found, scan a folder where you keep your projects"),
                    );
                } else if found.len() > shown {
                    on_computer = on_computer.child(
                        div().px_1().child(
                            Button::new("show-all-found")
                                .ghost()
                                .small()
                                .label(format!("Show all ({})", found.len()))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.show_all_found = true;
                                    cx.notify();
                                })),
                        ),
                    );
                }
            }
        }

        let step = |n: &'static str, title: &'static str, text: &'static str| {
            h_flex()
                .gap_3()
                .items_start()
                .child(
                    div()
                        .size_6()
                        .flex_shrink_0()
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .bg(theme.primary.opacity(0.1))
                        .child(n),
                )
                .child(
                    v_flex()
                        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(title))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(text),
                        ),
                )
        };

        div()
            .size_full()
            .overflow_y_scrollbar()
            .child(
                v_flex().w_full().items_center().py_12().child(
                    v_flex()
                        .w(px(560.))
                        .gap_8()
                        // Header
                        .child(
                            h_flex()
                                .gap_4()
                                .child(img("images/goat.png").size(px(96.)))
                                .child(
                                    v_flex()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_3xl()
                                                .font_weight(FontWeight::BOLD)
                                                .child("Goat Hooker"),
                                        )
                                        .child(
                                            div()
                                                .text_color(theme.muted_foreground)
                                                .child("Git hooks for every project of your repository"),
                                        ),
                                ),
                        )
                        .when_some(self.error.clone(), |this, error| {
                            this.child(
                                h_flex()
                                    .gap_2()
                                    .p_3()
                                    .rounded_md()
                                    .text_sm()
                                    .bg(theme.danger.opacity(0.1))
                                    .text_color(theme.danger)
                                    .child(Icon::new(IconName::CircleAlert).small())
                                    .child(error),
                            )
                        })
                        .child(
                            Button::new("open-repository")
                                .primary()
                                .icon(IconName::FolderOpen)
                                .label("Open a repository…")
                                .on_click(cx.listener(|this, _, _, cx| this.pick_folder(cx))),
                        )
                        // Recent repositories
                        .child(
                            v_flex()
                                .gap_2()
                                .child(section("RECENT", cx))
                                .child(recent),
                        )
                        // Repositories of the computer
                        .child(
                            v_flex()
                                .gap_2()
                                .child(
                                    h_flex()
                                        .gap_1()
                                        .child(section("ON THIS COMPUTER", cx))
                                        .child(div().flex_1())
                                        .child(
                                            Button::new("rescan")
                                                .ghost()
                                                .xsmall()
                                                .icon(IconName::RefreshCw)
                                                .tooltip("Search again")
                                                .loading(self.found.is_none())
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.scan_repositories(cx)
                                                })),
                                        )
                                        .child(
                                            Button::new("scan-folder")
                                                .ghost()
                                                .xsmall()
                                                .icon(IconName::FolderOpen)
                                                .label("Scan a folder…")
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.pick_scan_folder(cx)
                                                })),
                                        ),
                                )
                                .child(on_computer),
                        )
                        // Getting started
                        .child(
                            v_flex()
                                .gap_3()
                                .child(section("GET STARTED", cx))
                                .child(step(
                                    "1",
                                    "Open a git repository",
                                    "The folder of your project, or of your mono-repo",
                                ))
                                .child(step(
                                    "2",
                                    "Add hooks",
                                    "Commands to run before a commit or a push: linters, formatters, tests…",
                                ))
                                .child(step(
                                    "3",
                                    "Install the Git hooks",
                                    "From the status bar: from now on, Git runs them for you",
                                )),
                        )
                        // Links
                        .child(
                            h_flex()
                                .gap_1()
                                .pt_4()
                                .border_t_1()
                                .border_color(theme.border)
                                .child(
                                    Button::new("docs")
                                        .ghost()
                                        .small()
                                        .icon(lucide("book-open"))
                                        .label("Documentation")
                                        .on_click(|_, _, cx| {
                                            cx.open_url(&format!("{REPOSITORY_URL}#readme"))
                                        }),
                                )
                                .child(
                                    Button::new("github")
                                        .ghost()
                                        .small()
                                        .icon(lucide("github"))
                                        .label("GitHub")
                                        .on_click(|_, _, cx| cx.open_url(REPOSITORY_URL)),
                                )
                                .child(div().flex_1())
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child(concat!("v", env!("CARGO_PKG_VERSION"))),
                                ),
                        ),
                ),
            )
            .into_any_element()
    }

    fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let Some(repo) = self.repo.as_ref() else {
            return StatusBar::new().left(div().text_xs().child("No repository"));
        };

        let installed = if repo.installed {
            h_flex()
                .gap_1p5()
                .text_color(theme.success)
                .child(div().size_1p5().rounded_full().bg(theme.success))
                .child("Git hooks installed")
                .into_any_element()
        } else {
            Button::new("install")
                .ghost()
                .xsmall()
                .label("○ Git hooks not installed — Install")
                .on_click(cx.listener(|this, _, _, cx| this.install(cx)))
                .into_any_element()
        };

        let hooks: usize = repo.projects.iter().map(|p| p.hooks().len()).sum();
        let parallel = repo
            .projects
            .first()
            .is_some_and(|p| p.raw.parallel.unwrap_or(false));

        StatusBar::new()
            .text_xs()
            .left(installed)
            .when_some(repo.branch.clone(), |this, branch| {
                this.left(
                    h_flex()
                        .gap_1()
                        .text_color(theme.muted_foreground)
                        .child(lucide("git-branch").xsmall())
                        .child(branch),
                )
            })
            .left(
                div()
                    .text_color(theme.muted_foreground)
                    .child(format!("{} staged files", repo.staged_files)),
            )
            .when(self.running_all || !self.running.is_empty(), |this| {
                this.left(
                    h_flex()
                        .gap_1()
                        .child(Icon::new(IconName::LoaderCircle).xsmall())
                        .child("Running…"),
                )
            })
            .when(parallel, |this| {
                this.right(
                    h_flex()
                        .gap_1()
                        .text_color(theme.muted_foreground)
                        .child(lucide("zap").xsmall())
                        .child("parallel"),
                )
            })
            .right(div().text_color(theme.muted_foreground).child(format!(
                "{} · {}",
                plural(repo.projects.len(), "project"),
                plural(hooks, "hook")
            )))
            .right(
                div()
                    .text_color(theme.muted_foreground)
                    .child(concat!("v", env!("CARGO_PKG_VERSION"))),
            )
    }
}

impl Render for HookerApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        if self.graph_mode && self.graph_dirty {
            self.refresh_graph(cx);
        }

        let body = match self.repo.as_ref() {
            None => self.render_welcome(cx),
            Some(repo) if repo.projects.is_empty() => {
                let create = Button::new("init")
                    .label("Create hooker.config.json")
                    .on_click(cx.listener(|this, _, _, cx| this.init_config(cx)));
                let action = match self.import_action(cx) {
                    Some(import) => v_flex()
                        .items_center()
                        .gap_3()
                        .child(import)
                        .child(create.ghost())
                        .into_any_element(),
                    None => create.primary().into_any_element(),
                };
                self.render_placeholder(
                    "This repository has no goat-hooker configuration yet",
                    Some(action),
                    cx,
                )
            }
            Some(_) if self.filter.is_page() => h_flex()
                .size_full()
                .child(self.render_sidebar(cx))
                .child(
                    div()
                        .flex_1()
                        .h_full()
                        .overflow_hidden()
                        .child(self.render_page(window, cx)),
                )
                .into_any_element(),
            Some(_) if self.graph_mode => h_flex()
                .size_full()
                .child(div().flex_1().h_full().min_w_0().child(self.graph.clone()))
                .when(
                    self.editor.is_some() || self.json_editor.is_some(),
                    |this| this.child(self.render_graph_panel(cx)),
                )
                .into_any_element(),
            Some(_) => h_flex()
                .size_full()
                .child(self.render_sidebar(cx))
                .child(self.render_list(cx))
                .child(
                    div()
                        .flex_1()
                        .h_full()
                        .overflow_hidden()
                        .child(self.render_detail(cx)),
                )
                .into_any_element(),
        };

        v_flex()
            .relative()
            .size_full()
            .key_context("HookerApp")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &OpenRepository, _, cx| this.pick_folder(cx)))
            .on_action(cx.listener(|this, _: &ScanFolder, _, cx| this.pick_scan_folder(cx)))
            .on_action(cx.listener(|this, _: &CloseRepository, _, cx| this.go_home(cx)))
            .on_action(
                cx.listener(|this, _: &ToggleTheme, window, cx| this.toggle_theme(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &TogglePalette, window, cx| this.toggle_palette(window, cx)),
            )
            .when(
                self.repo.as_ref().is_some_and(|r| !r.projects.is_empty()),
                |this| {
                    fn on<A: Action>(this: Div, cx: &mut Context<HookerApp>) -> Div {
                        this.on_action(cx.listener(|this, action: &A, window, cx| {
                            this.on_menu_action(action, window, cx)
                        }))
                    }
                    let this = on::<NewHook>(this, cx);
                    let this = on::<RunPreCommit>(this, cx);
                    let this = on::<InstallHooks>(this, cx);
                    let this = on::<ReloadConfigs>(this, cx);
                    let this = on::<ShowList>(this, cx);
                    let this = on::<ShowGraph>(this, cx);
                    let this = on::<GoAll>(this, cx);
                    let this = on::<GoNextCommit>(this, cx);
                    let this = on::<GoVariables>(this, cx);
                    let this = on::<GoBrowse>(this, cx);
                    let this = on::<GoReview>(this, cx);
                    let this = on::<GoDoctor>(this, cx);
                    let this = on::<GoImpact>(this, cx);
                    let this = on::<GoProjects>(this, cx);
                    let this = on::<GoFixes>(this, cx);
                    let this = on::<RunPrePush>(this, cx);
                    on::<GoHistory>(this, cx)
                },
            )
            .child(self.render_title_bar(cx))
            .when_some(self.notice.clone(), |this, (message, success)| {
                let color = if success { theme.success } else { theme.danger };
                this.child(
                    h_flex()
                        .px_4()
                        .py_1p5()
                        .gap_2()
                        .text_sm()
                        .bg(color.opacity(0.1))
                        .border_b_1()
                        .border_color(color.opacity(0.3))
                        .child(
                            Icon::new(if success {
                                IconName::CircleCheck
                            } else {
                                IconName::CircleAlert
                            })
                            .small()
                            .text_color(color),
                        )
                        .child(message)
                        .child(div().flex_1())
                        .child(
                            Button::new("close-notice")
                                .ghost()
                                .xsmall()
                                .icon(IconName::Close)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.notice = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .when_some(
                self.error.clone().filter(|_| self.repo.is_some()),
                |this, error| {
                    this.child(
                        div()
                            .px_4()
                            .py_1p5()
                            .text_sm()
                            .text_color(theme.danger)
                            .bg(theme.danger.opacity(0.1))
                            .child(error),
                    )
                },
            )
            .child(div().flex_1().overflow_hidden().child(body))
            .when(self.repo.is_some(), |this| {
                this.child(self.render_status_bar(cx))
            })
            .when_some(self.render_palette(cx), |this, palette| this.child(palette))
    }
}
