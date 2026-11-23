//! Tool pages: the impact of a file, the graph of the projects, and the fixes of the
//! changed files (with their diff, before you stage them).

use std::{collections::BTreeMap, path::PathBuf};

use goat_hooker::{deps::Dependencies, fix, impact::Impact};

use super::pages::{card_div, code_line};
use super::*;

/// Size of a project box in the project graph
const BOX: (f32, f32) = (230., 64.);
const GAP: (f32, f32) = (90., 28.);

pub struct ToolsState {
    impact_input: Entity<InputState>,
    /// File and its impact
    impact: Option<(PathBuf, Result<Impact, String>)>,
    fixes: Option<Fixes>,
    fixing: bool,
    _subscription: Subscription,
}

struct Fixes {
    results: Vec<HookResult>,
    files: Vec<FixedFile>,
    error: Option<String>,
}

struct FixedFile {
    path: PathBuf,
    original: Option<Vec<u8>>,
    diff: String,
    state: FileState,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FileState {
    Fixed,
    Staged,
    Reverted,
}

impl ToolsState {
    pub fn new(window: &mut Window, cx: &mut Context<HookerApp>) -> Self {
        let impact_input = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Path of a file, like packages/api/src/main.rs")
        });
        let subscription = cx.subscribe(&impact_input, |this, input, event: &InputEvent, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                let path = input.read(cx).value().to_string();
                this.show_impact(&path, cx);
            }
        });
        Self {
            impact_input,
            impact: None,
            fixes: None,
            fixing: false,
            _subscription: subscription,
        }
    }
}

impl HookerApp {
    fn relative(&self, path: &std::path::Path) -> String {
        let root = self
            .repo
            .as_ref()
            .map(|r| r.root.clone())
            .unwrap_or_default();
        let root = root.canonicalize().unwrap_or(root);
        path.strip_prefix(&root)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| path.display().to_string())
    }

    fn project_name_of(&self, dir: &std::path::Path) -> String {
        match self.relative(dir).as_str() {
            "" => "root".to_string(),
            other => other.to_string(),
        }
    }

    // MARK: impact

    fn show_impact(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(repo) = self.repo.as_ref() else {
            return;
        };
        let path = path.trim();
        if path.is_empty() {
            return;
        }
        let root = repo.root.canonicalize().unwrap_or(repo.root.clone());
        let file = root.join(path);
        let impact = repo
            .load_configs()
            .and_then(|configs| goat_hooker::impact::of(&configs, &file))
            .map_err(|e| e.to_string());
        self.pages.tools.impact = Some((file, impact));
        cx.notify();
    }

    pub(super) fn render_impact(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let subtitle = "What a change of a file runs: its hooks on commit and push, and the projects depending on its project (their `affected` hooks run too).";
        let Some(repo) = self.repo.as_ref() else {
            return div().into_any_element();
        };
        let root = repo.root.canonicalize().unwrap_or(repo.root.clone());
        let changed = fix::changed_files(&root).unwrap_or_default();

        let input = self.pages.tools.impact_input.clone();
        let mut picks = h_flex().gap_1().flex_wrap();
        for (i, file) in changed.iter().take(12).enumerate() {
            let relative = self.relative(file);
            let input = input.clone();
            picks = picks.child(
                Button::new(("impact-pick", i))
                    .xsmall()
                    .ghost()
                    .label(relative.clone())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        input.update(cx, |input, cx| {
                            input.set_value(relative.clone(), window, cx)
                        });
                        this.show_impact(&relative, cx);
                    })),
            );
        }

        let mut body = v_flex()
            .gap_4()
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().child(Input::new(&input)))
                    .child(
                        Button::new("impact-go")
                            .small()
                            .primary()
                            .label("Show")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let path =
                                    this.pages.tools.impact_input.read(cx).value().to_string();
                                this.show_impact(&path, cx);
                            })),
                    ),
            )
            .when(!changed.is_empty(), |this| {
                this.child(
                    v_flex()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("CHANGED FILES"),
                        )
                        .child(picks),
                )
            });

        match self.pages.tools.impact.as_ref() {
            None => {}
            Some((_, Err(error))) => {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(theme.danger)
                        .child(error.clone()),
                );
            }
            Some((file, Ok(impact))) => {
                let mut card = card_div(cx).child(
                    h_flex()
                        .gap_2()
                        .text_sm()
                        .child(Icon::new(IconName::File).small())
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(self.relative(file)),
                        )
                        .when(!file.exists(), |this| {
                            this.child(chip("not found", theme.warning))
                        }),
                );
                card = card.child(
                    h_flex()
                        .gap_2()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("Project:")
                        .child(
                            div().text_color(theme.foreground).child(
                                impact
                                    .project
                                    .as_deref()
                                    .map(|p| self.project_name_of(p))
                                    .unwrap_or_else(|| "none".into()),
                            ),
                        ),
                );
                if !impact.dependents.is_empty() {
                    let names: Vec<String> = impact
                        .dependents
                        .iter()
                        .map(|p| self.project_name_of(p))
                        .collect();
                    card = card.child(
                        h_flex()
                            .gap_2()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("Depended on by:")
                            .child(div().text_color(theme.foreground).child(names.join(", "))),
                    );
                }

                let mut hooks = v_flex().gap_1();
                if impact.hooks.is_empty() {
                    hooks = hooks.child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("No hook targets this file"),
                    );
                }
                for (i, hook) in impact.hooks.iter().enumerate() {
                    let project = self.project_name_of(&hook.project);
                    let reference = format!(
                        "{}:{}",
                        if project == "root" { "root" } else { &project },
                        hook.name
                    );
                    hooks = hooks.child(
                        h_flex()
                            .id(("impact-hook", i))
                            .gap_2()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .text_sm()
                            .cursor_pointer()
                            .hover(|s| s.bg(theme.secondary))
                            .child(
                                trigger_icon(Some(hook.trigger))
                                    .small()
                                    .text_color(trigger_color(Some(hook.trigger), cx)),
                            )
                            .child(div().w(px(90.)).child(hook.trigger.as_str()))
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(hook.name.clone()),
                            )
                            .child(div().text_color(theme.muted_foreground).child(project))
                            .when_some(hook.affected_by.as_deref(), |this, by| {
                                this.child(chip(
                                    format!("affected: {} changed", self.project_name_of(by)),
                                    theme.info,
                                ))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select_reference(&reference, cx)
                            })),
                    );
                }
                card = card.child(section("HOOKS", cx)).child(hooks);
                body = body.child(card);
            }
        }
        self.page("Impact", subtitle, body, cx)
    }

    // MARK: projects

    pub(super) fn render_projects(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let subtitle = "Projects of the repository and what they depend on (Cargo path dependencies, workspace packages, go.mod replace, pyproject, `depends_on`). Hooks with `affected` run when a project they depend on changes.";
        let Some(repo) = self.repo.as_ref() else {
            return div().into_any_element();
        };
        let configs = repo.load_configs().unwrap_or_default();
        let all = Dependencies::detect(&configs).all();
        let deps: BTreeMap<PathBuf, Vec<PathBuf>> = all.into_iter().collect();

        // Column of a project: one more than the deepest project it depends on
        fn level(
            project: &PathBuf,
            deps: &BTreeMap<PathBuf, Vec<PathBuf>>,
            seen: &mut Vec<PathBuf>,
        ) -> usize {
            if seen.contains(project) {
                return 0;
            }
            seen.push(project.clone());
            let level = deps
                .get(project)
                .into_iter()
                .flatten()
                .filter(|d| deps.contains_key(*d))
                .map(|d| level(d, deps, seen) + 1)
                .max()
                .unwrap_or(0);
            seen.pop();
            level
        }
        let mut columns: BTreeMap<usize, Vec<PathBuf>> = BTreeMap::new();
        for project in deps.keys() {
            columns
                .entry(level(project, &deps, &mut vec![]))
                .or_default()
                .push(project.clone());
        }
        let mut positions: BTreeMap<PathBuf, (f32, f32)> = BTreeMap::new();
        for (column, projects) in &columns {
            for (row, project) in projects.iter().enumerate() {
                positions.insert(
                    project.clone(),
                    (
                        *column as f32 * (BOX.0 + GAP.0),
                        row as f32 * (BOX.1 + GAP.1),
                    ),
                );
            }
        }
        let width = columns.len() as f32 * (BOX.0 + GAP.0);
        let height = columns
            .values()
            .map(|c| c.len() as f32 * (BOX.1 + GAP.1))
            .fold(0., f32::max);

        // Wires: from a dependency (left) to the project using it (right)
        let mut wires: Vec<(Point<Pixels>, Point<Pixels>)> = Vec::new();
        for (project, project_deps) in &deps {
            let Some(&(x, y)) = positions.get(project) else {
                continue;
            };
            for dep in project_deps {
                if let Some(&(dx, dy)) = positions.get(dep) {
                    wires.push((
                        point(px(dx + BOX.0), px(dy + BOX.1 / 2.)),
                        point(px(x), px(y + BOX.1 / 2.)),
                    ));
                }
            }
        }
        let wire_color = theme.muted_foreground.opacity(0.6);

        let mut graph = div()
            .relative()
            .w(px(width.max(BOX.0)))
            .h(px(height.max(BOX.1)))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        for (from, to) in wires {
                            let (from, to) = (bounds.origin + from, bounds.origin + to);
                            let dx = px((f32::from(to.x - from.x).abs() / 2.).max(30.));
                            let mut path = PathBuilder::stroke(px(1.5));
                            path.move_to(from);
                            path.cubic_bezier_to(
                                to,
                                from + point(dx, px(0.)),
                                to - point(dx, px(0.)),
                            );
                            if let Ok(path) = path.build() {
                                window.paint_path(path, wire_color);
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            );

        for (i, (project, (x, y))) in positions.iter().enumerate() {
            let index = repo
                .projects
                .iter()
                .position(|p| p.dir().canonicalize().ok().as_ref() == Some(project));
            let hooks = index
                .map(|i| repo.projects[i].hooks().len())
                .unwrap_or_default();
            let affected = index
                .map(|i| {
                    repo.projects[i]
                        .hooks()
                        .iter()
                        .filter(|h| h.affected == Some(true))
                        .count()
                })
                .unwrap_or_default();
            let used_by = deps.values().filter(|d| d.contains(project)).count();
            graph = graph.child(
                v_flex()
                    .id(("project-box", i))
                    .absolute()
                    .left(px(*x))
                    .top(px(*y))
                    .w(px(BOX.0))
                    .h(px(BOX.1))
                    .px_3()
                    .py_2()
                    .gap_0p5()
                    .rounded_lg()
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.background)
                    .shadow_sm()
                    .cursor_pointer()
                    .hover(|s| s.border_color(theme.primary))
                    .child(
                        h_flex()
                            .gap_1p5()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(Icon::new(IconName::Folder).xsmall())
                            .child(div().truncate().child(self.project_name_of(project))),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!(
                                "{} hook{}{}{}",
                                hooks,
                                if hooks == 1 { "" } else { "s" },
                                if affected > 0 {
                                    format!(" · {affected} affected")
                                } else {
                                    String::new()
                                },
                                if used_by > 0 {
                                    format!(" · used by {used_by}")
                                } else {
                                    String::new()
                                }
                            )),
                    )
                    .when_some(index, |this, index| {
                        this.on_click(cx.listener(move |this, _, _, cx| {
                            this.set_filter(Filter::Project(index), cx)
                        }))
                    }),
            );
        }

        let has_links = deps.values().any(|d| !d.is_empty());
        let body = v_flex()
            .gap_3()
            .when(!has_links, |this| {
                this.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("No dependency between the projects was found. Add `depends_on` to a config to declare one."),
                )
            })
            .child(card_div(cx).overflow_hidden().child(
                div().id("project-graph").overflow_x_scroll().p_2().child(graph),
            ));
        self.page("Project graph", subtitle, body, cx)
    }

    // MARK: fixes

    /// Runs the hooks fixing files on the changed files (like `hooker fix`)
    pub(super) fn run_fixes(&mut self, cx: &mut Context<Self>) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        if self.pages.tools.fixing {
            return;
        }
        self.pages.tools.fixing = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let fixes = cx
                .background_spawn(async move {
                    let root = repo.root.canonicalize().unwrap_or(repo.root.clone());
                    let fixed = repo
                        .load_configs()
                        .and_then(|configs| fix::run(&root, &configs, None, None));
                    match fixed {
                        Ok(fixed) => Fixes {
                            files: fixed
                                .originals
                                .into_iter()
                                .map(|(path, original)| FixedFile {
                                    diff: fix::diff(&path, original.as_deref()),
                                    path,
                                    original,
                                    state: FileState::Fixed,
                                })
                                .collect(),
                            results: fixed.results,
                            error: None,
                        },
                        Err(err) => Fixes {
                            results: vec![],
                            files: vec![],
                            error: Some(err.to_string()),
                        },
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                this.pages.tools.fixing = false;
                this.pages.tools.fixes = Some(fixes);
                this.reload();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn stage_fixed(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        let Some(repo) = self.repo.as_ref() else {
            return;
        };
        let root = repo.root.clone();
        let Some(fixes) = self.pages.tools.fixes.as_mut() else {
            return;
        };
        let files: Vec<&mut FixedFile> = fixes
            .files
            .iter_mut()
            .enumerate()
            .filter(|(i, f)| index.is_none_or(|index| index == *i) && f.state == FileState::Fixed)
            .map(|(_, f)| f)
            .collect();
        let paths: Vec<PathBuf> = files.iter().map(|f| f.path.clone()).collect();
        match goat_hooker::git::add(&root, &paths) {
            Ok(()) => {
                for file in files {
                    file.state = FileState::Staged;
                }
            }
            Err(err) => self.notice = Some((err.to_string(), false)),
        }
        self.reload();
        cx.notify();
    }

    fn revert_fixed(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        let Some(fixes) = self.pages.tools.fixes.as_mut() else {
            return;
        };
        for (i, file) in fixes.files.iter_mut().enumerate() {
            if index.is_some_and(|index| index != i) || file.state != FileState::Fixed {
                continue;
            }
            let restored = match &file.original {
                Some(content) => std::fs::write(&file.path, content),
                None => std::fs::remove_file(&file.path),
            };
            match restored {
                Ok(()) => file.state = FileState::Reverted,
                Err(err) => self.notice = Some((err.to_string(), false)),
            }
        }
        cx.notify();
    }

    pub(super) fn render_fixes(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let subtitle = "Runs the hooks fixing files (`stage_fixed`: formatters, `--fix` linters) on the files changed since the last commit, and shows what they changed before you stage it. Also `hooker fix`.";
        let fixing = self.pages.tools.fixing;
        let pending = self
            .pages
            .tools
            .fixes
            .as_ref()
            .is_some_and(|f| f.files.iter().any(|f| f.state == FileState::Fixed));

        let actions = h_flex()
            .gap_2()
            .child(
                Button::new("fix-run")
                    .small()
                    .primary()
                    .icon(lucide("wand"))
                    .label("Fix changed files")
                    .loading(fixing)
                    .on_click(cx.listener(|this, _, _, cx| this.run_fixes(cx))),
            )
            .when(pending, |this| {
                this.child(
                    Button::new("fix-stage-all")
                        .small()
                        .label("Stage all")
                        .on_click(cx.listener(|this, _, _, cx| this.stage_fixed(None, cx))),
                )
                .child(
                    Button::new("fix-revert-all")
                        .small()
                        .ghost()
                        .label("Revert all")
                        .on_click(cx.listener(|this, _, _, cx| this.revert_fixed(None, cx))),
                )
            });

        let mut body = v_flex().gap_4().child(actions);
        if let Some(fixes) = self.pages.tools.fixes.as_ref() {
            if let Some(error) = fixes.error.as_ref() {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(theme.danger)
                        .child(error.clone()),
                );
            } else if fixes.results.is_empty() {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("No `stage_fixed` hook targets the changed files"),
                );
            } else {
                let mut ran = h_flex().gap_1().flex_wrap();
                for result in &fixes.results {
                    let color = if result.failed() {
                        theme.danger
                    } else {
                        theme.success
                    };
                    ran = ran.child(chip(result.name.clone(), color));
                }
                body = body.child(
                    h_flex()
                        .gap_2()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("Ran")
                        .child(ran)
                        .when(fixes.files.is_empty(), |this| {
                            this.child("· nothing to fix")
                        }),
                );
                for result in fixes.results.iter().filter(|r| r.failed()) {
                    body = body.child(code_line(result.stdout(), cx).text_color(theme.danger));
                }
            }

            for (i, file) in fixes.files.iter().enumerate() {
                let state = match file.state {
                    FileState::Fixed => None,
                    FileState::Staged => Some(("staged", theme.success)),
                    FileState::Reverted => Some(("reverted", theme.muted_foreground)),
                };
                let mut diff = v_flex()
                    .p_2()
                    .rounded_md()
                    .bg(theme.secondary)
                    .text_xs()
                    .font_family("monospace");
                for line in file.diff.lines().take(400) {
                    let (bg, fg) = match line.chars().next() {
                        Some('+') => (theme.success.opacity(0.12), theme.foreground),
                        Some('-') => (theme.danger.opacity(0.12), theme.foreground),
                        Some('@') => (gpui_kit::transparent_black(), theme.muted_foreground),
                        _ => (gpui_kit::transparent_black(), theme.foreground),
                    };
                    diff = diff.child(
                        div()
                            .px_1()
                            .bg(bg)
                            .text_color(fg)
                            .whitespace_nowrap()
                            .child(line.to_string()),
                    );
                }
                body = body.child(
                    card_div(cx)
                        .child(
                            h_flex()
                                .gap_2()
                                .child(Icon::new(IconName::File).small())
                                .child(
                                    div()
                                        .flex_1()
                                        .text_sm()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(self.relative(&file.path)),
                                )
                                .when_some(state, |this, (label, color)| {
                                    this.child(chip(label, color))
                                })
                                .when(file.state == FileState::Fixed, |this| {
                                    this.child(
                                        Button::new(("fix-stage", i))
                                            .xsmall()
                                            .label("Stage")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.stage_fixed(Some(i), cx)
                                            })),
                                    )
                                    .child(
                                        Button::new(("fix-revert", i))
                                            .xsmall()
                                            .ghost()
                                            .label("Revert")
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.revert_fixed(Some(i), cx)
                                            })),
                                    )
                                }),
                        )
                        .when(file.state != FileState::Reverted, |this| {
                            this.child(div().id(("fix-diff", i)).overflow_x_scroll().child(diff))
                        }),
                );
            }
        }
        self.page("Fix files", subtitle, body, cx)
    }
}
