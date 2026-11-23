//! Command palette (⌘K / Ctrl+K): the actions of the menus, the pages, and every hook
//! (open it, or run it), searched by name.

use std::rc::Rc;

use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};

use super::*;

/// What an entry of the palette does
enum Target {
    /// Dispatched by the palette itself (an Action of the menus)
    Action,
    Open(HookRef),
    Run(HookRef),
}

impl HookerApp {
    pub(super) fn toggle_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.palette.take().is_some() {
            self.focus_handle.focus(window, cx);
        } else {
            let state = cx.new(|cx| CommandState::new(window, cx));
            state.update(cx, |state, cx| state.focus(window, cx));
            self.palette = Some(state);
        }
        cx.notify();
    }

    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.palette = None;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(super) fn render_palette(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let state = self.palette.as_ref()?;
        let theme = cx.theme().clone();
        let has_hooks = self.repo.as_ref().is_some_and(|r| !r.projects.is_empty());

        let action = |label: &str, icon: Icon, action: Box<dyn Action>| {
            (
                CommandItem::new()
                    .label(label.to_string())
                    .icon(icon)
                    .action(action),
                Target::Action,
            )
        };
        let mut groups: Vec<(&str, Vec<(CommandItem, Target)>)> = Vec::new();

        let mut actions = vec![
            action(
                "Open repository…",
                Icon::new(IconName::FolderOpen),
                Box::new(OpenRepository),
            ),
            action(
                "Scan a folder…",
                lucide("folder-git-2"),
                Box::new(ScanFolder),
            ),
        ];
        if has_hooks {
            actions.extend([
                action("New hook…", Icon::new(IconName::Plus), Box::new(NewHook)),
                action(
                    "Run pre-commit",
                    Icon::new(IconName::Play),
                    Box::new(RunPreCommit),
                ),
                action(
                    "Run pre-push",
                    Icon::new(IconName::Play),
                    Box::new(RunPrePush),
                ),
                action("Fix changed files", lucide("wand"), Box::new(GoFixes)),
                action(
                    "Install Git hooks",
                    lucide("git-commit-horizontal"),
                    Box::new(InstallHooks),
                ),
                action("Reload configs", lucide("redo"), Box::new(ReloadConfigs)),
                action("List view", lucide("list"), Box::new(ShowList)),
                action("Graph view", lucide("workflow"), Box::new(ShowGraph)),
            ]);
        }
        actions.extend([
            action("Toggle dark mode", lucide("moon"), Box::new(ToggleTheme)),
            action(
                "Close repository",
                lucide("house"),
                Box::new(CloseRepository),
            ),
            action("Documentation", lucide("book-open"), Box::new(OpenDocs)),
            action("Report an issue", lucide("github"), Box::new(ReportIssue)),
        ]);
        groups.push(("Actions", actions));

        if has_hooks {
            groups.push((
                "Go to",
                vec![
                    action("All hooks", lucide("layers"), Box::new(GoAll)),
                    action("Next commit", lucide("list-checks"), Box::new(GoNextCommit)),
                    action("Variables", lucide("variable"), Box::new(GoVariables)),
                    action("Impact of a file", lucide("crosshair"), Box::new(GoImpact)),
                    action("Project graph", lucide("network"), Box::new(GoProjects)),
                    action("History", lucide("clock"), Box::new(GoHistory)),
                    action("Doctor", lucide("stethoscope"), Box::new(GoDoctor)),
                    action("Browse shared hooks", lucide("globe"), Box::new(GoBrowse)),
                    action("Review", lucide("shield-alert"), Box::new(GoReview)),
                ],
            ));

            let mut open = Vec::new();
            let mut run = Vec::new();
            if let Some(repo) = self.repo.as_ref() {
                for (p, project) in repo.projects.iter().enumerate() {
                    for (h, hook) in project.hooks().iter().enumerate() {
                        let hook_ref = HookRef {
                            project: p,
                            hook: h,
                        };
                        let detail = format!(
                            "{} · {}",
                            project_label(project),
                            hook.trigger.map(|t| t.as_str()).unwrap_or("shared")
                        );
                        let keywords = [project_label(project), detail.clone()];
                        open.push((
                            CommandItem::new()
                                .label(format!("{}   {detail}", hook.name))
                                .icon(trigger_icon(hook.trigger))
                                .keywords(keywords.clone()),
                            Target::Open(hook_ref),
                        ));
                        if !hook.is_import() {
                            run.push((
                                CommandItem::new()
                                    .label(format!("Run {}   {detail}", hook.name))
                                    .icon(Icon::new(IconName::Play))
                                    .keywords(keywords),
                                Target::Run(hook_ref),
                            ));
                        }
                    }
                }
            }
            groups.push(("Hooks", open));
            groups.push(("Run a hook", run));
        }

        let mut command = Command::new(state)
            .placeholder("Search actions, pages and hooks…")
            .max_h(px(420.));
        let mut targets: Vec<Vec<Target>> = Vec::new();
        for (label, entries) in groups {
            let mut group = CommandGroup::new().label(label);
            let mut group_targets = Vec::new();
            for (item, target) in entries {
                group = group.item(item);
                group_targets.push(target);
            }
            command = command.group(group);
            targets.push(group_targets);
        }
        let targets = Rc::new(targets);
        let app = cx.entity().downgrade();
        let cancel_app = app.clone();
        let command = command
            .on_confirm(move |index, window, cx| {
                let target = targets.get(index.section).and_then(|g| g.get(index.row));
                let _ = app.update(cx, |this, cx| {
                    this.close_palette(window, cx);
                    match target {
                        Some(Target::Open(hook_ref)) => {
                            this.graph_mode = false;
                            this.filter = Filter::All;
                            this.selected = Some(*hook_ref);
                            this.editor = None;
                            this.json_editor = None;
                        }
                        Some(Target::Run(hook_ref)) => {
                            this.graph_mode = false;
                            this.filter = Filter::All;
                            this.selected = Some(*hook_ref);
                            this.run_hook(*hook_ref, cx);
                        }
                        Some(Target::Action) | None => {}
                    }
                    cx.notify();
                });
            })
            .on_cancel(move |window, cx| {
                let _ = cancel_app.update(cx, |this, cx| this.close_palette(window, cx));
            });

        Some(
            div()
                .id("palette-backdrop")
                .absolute()
                .inset_0()
                .flex()
                .justify_center()
                .pt(px(72.))
                .bg(gpui_kit::black().opacity(0.25))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.close_palette(window, cx)),
                )
                .child(
                    div()
                        .id("palette")
                        .w(px(600.))
                        .h_auto()
                        .max_h(px(480.))
                        .rounded_lg()
                        .border_1()
                        .border_color(theme.border)
                        .bg(theme.popover)
                        .shadow_lg()
                        .overflow_hidden()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(command),
                )
                .into_any_element(),
        )
    }
}
