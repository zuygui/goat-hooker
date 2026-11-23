//! Pages next to the hooks list: variables shared between hooks, hooks to browse on GitHub,
//! and hooks of other repositories to review before they run.

use std::collections::BTreeMap;

use goat_hooker::{
    config::{FileAccess, Hook},
    hub::{SharedFile, SharedRepo},
    import::Imported,
    variables::Usage,
};

use super::*;
use crate::repo::Untrusted;

type SearchResult = Result<(Vec<SharedRepo>, Option<String>), String>;

/// State of the pages
pub struct PagesState {
    pub variables: Vec<Usage>,
    pub variables_error: Option<String>,
    pub untrusted: Vec<Untrusted>,
    /// Configs of other hooks managers found in a repository without hooker.config.json
    pub import: Option<Imported>,
    hub_query: Entity<InputState>,
    hub_loading: bool,
    hub_results: Option<SearchResult>,
    /// (repository, file) picked in the results
    hub_selected: Option<(usize, usize)>,
    hub_params: Vec<(String, String, Entity<InputState>)>,
    hub_name: Option<Entity<InputState>>,
    hub_project: usize,
    _subscription: Subscription,
}

impl PagesState {
    pub fn new(window: &mut Window, cx: &mut Context<HookerApp>) -> Self {
        let hub_query = cx.new(|cx| {
            InputState::new(window, cx).placeholder("rust, eslint, commits… (empty: everything)")
        });
        let subscription = cx.subscribe(&hub_query, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.search_hub(cx);
            }
        });
        Self {
            _subscription: subscription,
            variables: vec![],
            variables_error: None,
            untrusted: vec![],
            import: None,
            hub_query,
            hub_loading: false,
            hub_results: None,
            hub_selected: None,
            hub_params: vec![],
            hub_name: None,
            hub_project: 0,
        }
    }
}

fn value_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Chips telling what a hook may do
fn permission_chips(hook: &Hook, cx: &App) -> Div {
    let theme = cx.theme();
    let granted = hook.granted();
    let (safe, risky) = (theme.success, theme.warning);
    h_flex()
        .gap_1()
        .flex_wrap()
        .child(match granted.files {
            FileAccess::Read => chip("read files", safe),
            FileAccess::Write => chip("modifies files", risky),
        })
        .child(if granted.network {
            chip("network", risky)
        } else {
            chip("no network", safe)
        })
        .child(match &granted.env {
            Some(env) if env.is_empty() => chip("basic env", safe),
            Some(env) => chip(format!("env: {}", env.join(", ")), risky),
            None => chip("whole env", theme.danger),
        })
        .children(
            hook.writes
                .iter()
                .flatten()
                .map(|v| chip(format!("writes ${v}"), theme.info)),
        )
        .children(
            hook.reads
                .iter()
                .flatten()
                .map(|v| chip(format!("reads ${v}"), theme.info)),
        )
}

fn code_line(text: impl Into<SharedString>, cx: &App) -> Div {
    let theme = cx.theme();
    div()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .px_2()
        .py_1()
        .rounded_md()
        .bg(theme.secondary)
        .text_xs()
        .font_family("monospace")
        .child(text.into())
}

fn card(cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .p_4()
        .gap_3()
        .rounded_lg()
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
}

impl HookerApp {
    /// Data of the pages, after a reload
    pub(super) fn refresh_pages(&mut self) {
        let Some(repo) = self.repo.as_ref() else {
            self.pages.variables.clear();
            self.pages.untrusted.clear();
            self.pages.import = None;
            return;
        };
        match repo.variables() {
            Ok(variables) => {
                self.pages.variables = variables;
                self.pages.variables_error = None;
            }
            Err(err) => {
                self.pages.variables.clear();
                self.pages.variables_error = Some(err.to_string());
            }
        }
        self.pages.untrusted = repo.untrusted();
        self.pages.import = if repo.projects.is_empty() {
            repo.import_preview()
        } else {
            None
        };
    }

    /// Opens a hook of the list from its `project:name` reference
    fn select_reference(&mut self, reference: &str, cx: &mut Context<Self>) {
        let Some(repo) = self.repo.as_ref() else {
            return;
        };
        let Some((project, name)) = reference.rsplit_once(':') else {
            return;
        };
        let project = if project == "root" { "." } else { project };
        let found = repo.projects.iter().enumerate().find_map(|(p, pr)| {
            (pr.name == project)
                .then(|| pr.hooks().iter().position(|h| h.name == name))
                .flatten()
                .map(|h| HookRef {
                    project: p,
                    hook: h,
                })
        });
        if let Some(hook_ref) = found {
            self.filter = Filter::All;
            self.graph_mode = false;
            self.selected = Some(hook_ref);
            cx.notify();
        }
    }

    /// Rows of the TOOLS section of the sidebar
    pub(super) fn tools_rows(&self, cx: &mut Context<Self>) -> Div {
        let theme = cx.theme().clone();
        let review = self.pages.untrusted.len();
        v_flex()
            .gap_0p5()
            .child(self.nav_row(
                "variables",
                lucide("variable"),
                None,
                "Variables",
                Some(self.pages.variables.len()),
                Filter::Variables,
                cx,
            ))
            .child(self.nav_row(
                "browse",
                lucide("globe"),
                None,
                "Browse hooks",
                None,
                Filter::Browse,
                cx,
            ))
            .when(review > 0, |this| {
                this.child(self.nav_row(
                    "review",
                    lucide("shield-alert"),
                    Some(theme.warning),
                    "To review",
                    Some(review),
                    Filter::Review,
                    cx,
                ))
            })
    }

    pub(super) fn render_page(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.filter {
            Filter::Variables => self.render_variables(cx),
            Filter::Review => self.render_review(cx),
            _ => self.render_browse(window, cx),
        }
    }

    fn page(&self, title: &str, subtitle: &str, body: impl IntoElement, cx: &App) -> AnyElement {
        let theme = cx.theme();
        div()
            .size_full()
            .overflow_y_scrollbar()
            .child(
                v_flex()
                    .p_6()
                    .gap_4()
                    .max_w(px(980.))
                    .child(
                        v_flex()
                            .gap_1()
                            .child(
                                div()
                                    .text_2xl()
                                    .font_weight(FontWeight::BOLD)
                                    .child(title.to_string()),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.muted_foreground)
                                    .child(subtitle.to_string()),
                            ),
                    )
                    .child(body),
            )
            .into_any_element()
    }

    // MARK: variables

    fn render_variables(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let subtitle = "Values shared between hooks. A hook declares what it `writes` (echo \"name=value\" >> $HOOKER_VARS) and `reads` ($name): readers run after writers. Values are kept between commits.";

        if let Some(error) = self.pages.variables_error.clone() {
            return self.page(
                "Variables",
                subtitle,
                div().text_sm().text_color(theme.danger).child(error),
                cx,
            );
        }
        if self.pages.variables.is_empty() {
            let example = r#""variables": { "version": { "description": "Next version", "default": "0.1.0", "writers": ["bump"] } }"#;
            return self.page(
                "Variables",
                subtitle,
                card(cx)
                    .child(div().text_sm().child("No variables yet. Declare them in hooker.config.json:"))
                    .child(code_line(example, cx))
                    .child(code_line(
                        r#"{ "name": "bump", "writes": ["version"], "cmd": "echo version=0.2.0 >> $HOOKER_VARS" }"#,
                        cx,
                    )),
                cx,
            );
        }

        let mut list = v_flex().gap_3();
        for (i, usage) in self.pages.variables.clone().into_iter().enumerate() {
            let variable = &usage.declared.variable;
            let name = usage.declared.name.clone();
            let is_default = usage.value == variable.default;

            let hooks_column = |title: &'static str,
                                hooks: &[String],
                                allowed: Option<&Vec<String>>,
                                id: &'static str,
                                cx: &mut Context<Self>| {
                let mut column = v_flex().flex_1().gap_1().child(section(title, cx));
                if hooks.is_empty() {
                    column = column.child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("no hook"),
                    );
                }
                for (k, reference) in hooks.iter().enumerate() {
                    let reference = reference.clone();
                    column = column.child(
                        h_flex()
                            .id((id, i * 100 + k))
                            .gap_1p5()
                            .text_sm()
                            .cursor_pointer()
                            .hover(|this| this.text_color(theme.primary))
                            .child(lucide("zap").xsmall().text_color(theme.muted_foreground))
                            .child(reference.clone())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.select_reference(&reference, cx)
                            })),
                    );
                }
                column.child(div().text_xs().text_color(theme.muted_foreground).child(
                    match allowed {
                        Some(list) => format!("allowed: {}", list.join(", ")),
                        None => "every hook is allowed".to_string(),
                    },
                ))
            };

            let writers = hooks_column(
                "WRITTEN BY",
                &usage.writers,
                variable.writers.as_ref(),
                "writer",
                cx,
            );
            let readers = hooks_column(
                "READ BY",
                &usage.readers,
                variable.readers.as_ref(),
                "reader",
                cx,
            );

            list = list.child(
                card(cx)
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                div()
                                    .font_family("monospace")
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("${name}")),
                            )
                            .child(match usage.value.clone() {
                                Some(value) => chip(value, theme.info),
                                None => chip("no value", theme.muted_foreground),
                            })
                            .when(is_default && usage.value.is_some(), |this| {
                                this.child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child("default"),
                                )
                            })
                            .child(div().flex_1())
                            .child(div().text_xs().text_color(theme.muted_foreground).child(
                                format!(
                                    "declared in {}",
                                    if usage.declared.project == "root" {
                                        "root".to_string()
                                    } else {
                                        usage.declared.project.clone()
                                    }
                                ),
                            ))
                            .when(!is_default, |this| {
                                let name = name.clone();
                                this.child(
                                    Button::new(("reset-var", i))
                                        .ghost()
                                        .xsmall()
                                        .label("Reset")
                                        .tooltip("Back to the default value")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(repo) = this.repo.as_ref()
                                                && let Err(err) = repo.reset_variable(&name)
                                            {
                                                this.notice = Some((err.to_string(), false));
                                            }
                                            this.refresh_pages();
                                            cx.notify();
                                        })),
                                )
                            }),
                    )
                    .when_some(variable.description.clone(), |this, d| {
                        this.child(div().text_sm().text_color(theme.muted_foreground).child(d))
                    })
                    .child(h_flex().gap_6().items_start().child(writers).child(readers))
                    .children(usage.violations.iter().map(|v| {
                        h_flex()
                            .gap_1p5()
                            .text_sm()
                            .text_color(theme.danger)
                            .child(Icon::new(IconName::CircleX).small())
                            .child(v.clone())
                    })),
            );
        }
        self.page("Variables", subtitle, list, cx)
    }

    // MARK: review

    fn render_review(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let subtitle = "Hooks loaded from other repositories don't run until you trust them. Check what they run and what they may do; they must be trusted again when they change.";
        if self.pages.untrusted.is_empty() {
            return self.page(
                "Review",
                subtitle,
                h_flex()
                    .gap_2()
                    .text_sm()
                    .child(Icon::new(IconName::CircleCheck).text_color(theme.success))
                    .child("Every hook from other repositories is trusted"),
                cx,
            );
        }

        let mut list = v_flex().gap_3();
        for (i, untrusted) in self.pages.untrusted.clone().into_iter().enumerate() {
            let origin = untrusted.origin.clone();
            let mut hooks = v_flex().gap_3();
            for hook in &untrusted.hooks {
                hooks = hooks.child(
                    v_flex()
                        .gap_1p5()
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
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(hook.name.clone()),
                                )
                                .child(permission_chips(hook, cx)),
                        )
                        .children(
                            hook.commands()
                                .into_iter()
                                .map(|c| code_line(format!("$ {c}"), cx)),
                        ),
                );
            }
            list = list.child(
                card(cx)
                    .border_color(theme.warning.opacity(0.5))
                    .child(
                        h_flex()
                            .gap_2()
                            .child(lucide("package").text_color(theme.warning))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(origin.path.clone()),
                                    )
                                    .child(
                                        div().text_xs().text_color(theme.muted_foreground).child(
                                            format!(
                                                "{}{}",
                                                origin.url,
                                                origin
                                                    .rev
                                                    .as_ref()
                                                    .map(|r| format!(" @ {r}"))
                                                    .unwrap_or_default()
                                            ),
                                        ),
                                    ),
                            )
                            .child(
                                Button::new(("trust", i))
                                    .primary()
                                    .small()
                                    .icon(lucide("shield-check"))
                                    .label("Trust")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(repo) = this.repo.as_ref() {
                                            match repo.trust(&origin) {
                                                Ok(()) => {
                                                    this.notice = Some((
                                                        format!("Trusted {}", origin.path),
                                                        true,
                                                    ))
                                                }
                                                Err(err) => {
                                                    this.notice = Some((err.to_string(), false))
                                                }
                                            }
                                        }
                                        this.refresh_pages();
                                        if this.pages.untrusted.is_empty() {
                                            this.filter = Filter::All;
                                        }
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(hooks),
            );
        }
        self.page("Review", subtitle, list, cx)
    }

    // MARK: browse

    /// First visit of the Browse page: lists everything
    pub(super) fn open_browse(&mut self, cx: &mut Context<Self>) {
        if self.pages.hub_results.is_none() && !self.pages.hub_loading {
            self.search_hub(cx);
        }
    }

    fn search_hub(&mut self, cx: &mut Context<Self>) {
        let query = self.pages.hub_query.read(cx).value().to_string();
        self.pages.hub_loading = true;
        self.pages.hub_selected = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let results = cx
                .background_spawn(async move {
                    goat_hooker::hub::search(&query).map_err(|e| e.to_string())
                })
                .await;
            this.update(cx, |this, cx| {
                this.pages.hub_loading = false;
                this.pages.hub_results = Some(results);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn pick_shared(
        &mut self,
        repo: usize,
        file: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Ok((repos, _))) = self.pages.hub_results.as_ref() else {
            return;
        };
        let shared = &repos[repo].files[file];
        let params: Vec<(String, String, String)> = shared
            .params
            .iter()
            .map(|(name, param)| {
                (
                    name.clone(),
                    param.description.clone().unwrap_or_default(),
                    param.default.as_ref().map(value_text).unwrap_or_default(),
                )
            })
            .collect();
        let default_name = shared.default_name();
        self.pages.hub_params = params
            .into_iter()
            .map(|(name, description, default)| {
                let input = cx.new(|cx| InputState::new(window, cx).default_value(default));
                (name, description, input)
            })
            .collect();
        self.pages.hub_name =
            Some(cx.new(|cx| InputState::new(window, cx).default_value(default_name)));
        self.pages.hub_selected = Some((repo, file));
        cx.notify();
    }

    fn add_shared(&mut self, cx: &mut Context<Self>) {
        let (Some((r, f)), Some(Ok((repos, _)))) =
            (self.pages.hub_selected, self.pages.hub_results.as_ref())
        else {
            return;
        };
        let repo_found: SharedRepo = repos[r].clone();
        let file: SharedFile = repo_found.files[f].clone();
        let name = self
            .pages
            .hub_name
            .as_ref()
            .map(|n| n.read(cx).value().trim().to_string())
            .unwrap_or_default();
        let mut with = BTreeMap::new();
        for (param, _, input) in &self.pages.hub_params {
            let value = input.read(cx).value().to_string();
            let default = file
                .params
                .iter()
                .find(|(n, _)| n == param)
                .and_then(|(_, p)| p.default.as_ref())
                .map(value_text)
                .unwrap_or_default();
            if value != default {
                let value = serde_json::from_str::<serde_json::Value>(&value)
                    .ok()
                    .filter(|v| v.is_number() || v.is_boolean())
                    .unwrap_or(serde_json::Value::String(value));
                with.insert(param.clone(), value);
            }
        }
        let project = self.pages.hub_project;
        let Some(repo) = self.repo.as_mut() else {
            return;
        };
        match repo.add_shared(project, &name, &repo_found, &file, with) {
            Ok(()) => {
                self.notice = Some((
                    format!(
                        "Added {name}: {} hooks from {}",
                        file.hooks.len(),
                        repo_found.name
                    ),
                    true,
                ));
                self.reload();
            }
            Err(err) => self.notice = Some((err.to_string(), false)),
        }
        cx.notify();
    }

    fn render_browse(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let search = h_flex()
            .gap_2()
            .child(
                div().flex_1().child(
                    Input::new(&self.pages.hub_query).prefix(
                        Icon::new(IconName::Search)
                            .small()
                            .text_color(theme.muted_foreground),
                    ),
                ),
            )
            .child(
                Button::new("hub-search")
                    .primary()
                    .label("Search GitHub")
                    .loading(self.pages.hub_loading)
                    .on_click(cx.listener(|this, _, _, cx| this.search_hub(cx))),
            );

        let mut body = v_flex().gap_4().child(search);
        match self.pages.hub_results.clone() {
            None => {
                body = body.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("Repositories share hooks with `share` in their root hooker.config.json. Search them, then pick a hooks file to add it to a project."),
                );
            }
            Some(Err(err)) => {
                body = body.child(div().text_sm().text_color(theme.danger).child(err));
            }
            Some(Ok((repos, note))) => {
                if let Some(note) = note {
                    body = body.child(
                        h_flex()
                            .gap_1p5()
                            .text_sm()
                            .text_color(theme.warning)
                            .child(Icon::new(IconName::Info).small())
                            .child(note),
                    );
                }
                if repos.is_empty() {
                    body = body.child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("No shared hooks match"),
                    );
                }

                let mut results = v_flex().w(px(340.)).flex_shrink_0().gap_3();
                for (r, repo) in repos.iter().enumerate() {
                    let mut files = v_flex().gap_1();
                    for (f, file) in repo.files.iter().enumerate() {
                        let selected = self.pages.hub_selected == Some((r, f));
                        files = files.child(
                            v_flex()
                                .id(("hub-file", r * 100 + f))
                                .p_2()
                                .gap_0p5()
                                .rounded_md()
                                .cursor_pointer()
                                .when(selected, |this| this.bg(theme.accent))
                                .hover(|this| this.bg(theme.accent.opacity(0.6)))
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(file.path.clone()),
                                )
                                .when_some(file.description.clone(), |this, d| {
                                    this.child(
                                        div().text_xs().text_color(theme.muted_foreground).child(d),
                                    )
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.pick_shared(r, f, window, cx)
                                })),
                        );
                    }
                    results = results.child(
                        v_flex()
                            .gap_1()
                            .child(
                                h_flex().gap_1p5().child(lucide("github").small()).child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(repo.name.clone()),
                                ),
                            )
                            .when_some(repo.description.clone(), |this, d| {
                                this.child(
                                    div().text_xs().text_color(theme.muted_foreground).child(d),
                                )
                            })
                            .child(files),
                    );
                }

                let detail = match self.pages.hub_selected {
                    Some((r, f)) if r < repos.len() && f < repos[r].files.len() => {
                        self.render_shared_file(&repos[r], &repos[r].files[f], cx)
                    }
                    _ => div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child("Pick a hooks file")
                        .into_any_element(),
                };
                body = body.child(
                    h_flex()
                        .gap_4()
                        .items_start()
                        .child(results)
                        .child(div().flex_1().min_w_0().child(detail)),
                );
            }
        }
        self.page(
            "Browse hooks",
            "Hooks shared on GitHub: added hooks are trusted in the version you reviewed.",
            body,
            cx,
        )
    }

    fn render_shared_file(
        &self,
        repo: &SharedRepo,
        file: &SharedFile,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme().clone();
        let mut hooks = v_flex().gap_3();
        for hook in &file.hooks {
            hooks = hooks.child(
                v_flex()
                    .gap_1()
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
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(hook.name.clone()),
                            )
                            .when_some(hook.trigger, |this, t| {
                                this.child(chip(t.as_str(), trigger_color(Some(t), cx)))
                            }),
                    )
                    .child(permission_chips(&hook.hook, cx))
                    .children(
                        hook.commands
                            .iter()
                            .map(|c| code_line(format!("$ {c}"), cx)),
                    ),
            );
        }

        let mut params = v_flex().gap_2();
        for (name, description, input) in &self.pages.hub_params {
            params = params.child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(div().text_sm().font_family("monospace").child(name.clone()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(description.clone()),
                            ),
                    )
                    .child(Input::new(input).small()),
            );
        }

        let mut projects = h_flex().gap_1().flex_wrap();
        for (i, project) in self.repo.iter().flat_map(|r| r.projects.iter()).enumerate() {
            let selected = i == self.pages.hub_project;
            let button = Button::new(("hub-project", i))
                .xsmall()
                .label(project_label(project))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.pages.hub_project = i;
                    cx.notify();
                }));
            projects = projects.child(if selected {
                button.primary()
            } else {
                button.ghost()
            });
        }

        card(cx)
            .child(
                v_flex()
                    .gap_0p5()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(file.path.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("from {}", repo.url)),
                    )
                    .when_some(file.description.clone(), |this, d| {
                        this.child(div().pt_1().text_sm().child(d))
                    }),
            )
            .child(section("HOOKS", cx))
            .child(hooks)
            .when(!self.pages.hub_params.is_empty(), |this| {
                this.child(section("PARAMETERS", cx)).child(params)
            })
            .child(section("ADD TO", cx))
            .child(projects)
            .when_some(self.pages.hub_name.clone(), |this, name| {
                this.child(
                    h_flex()
                        .gap_2()
                        .child(div().text_sm().child("Name"))
                        .child(div().w(px(200.)).child(Input::new(&name).small())),
                )
            })
            .child(
                h_flex().child(
                    Button::new("hub-add")
                        .primary()
                        .icon(IconName::Plus)
                        .label("Add and trust")
                        .on_click(cx.listener(|this, _, _, cx| this.add_shared(cx))),
                ),
            )
            .into_any_element()
    }

    // MARK: import

    /// Button importing the configs of other hooks managers (repository without config)
    pub(super) fn import_action(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let imported = self.pages.import.as_ref()?;
        let theme = cx.theme().clone();
        let sources: Vec<String> = imported
            .sources
            .iter()
            .map(|s| s.display().to_string())
            .collect();
        Some(
            v_flex()
                .items_center()
                .gap_2()
                .child(
                    Button::new("import")
                        .primary()
                        .icon(lucide("import"))
                        .label(format!("Import {} hooks", imported.hooks_count()))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(repo) = this.repo.as_mut() {
                                match repo.import() {
                                    Ok(imported) => {
                                        this.notice = Some((
                                            format!(
                                                "Imported {} hooks{}",
                                                imported.hooks_count(),
                                                if imported.notes.is_empty() {
                                                    String::new()
                                                } else {
                                                    format!(" · {}", imported.notes.join(" · "))
                                                }
                                            ),
                                            imported.notes.is_empty(),
                                        ));
                                        this.reload();
                                        this.select_first(cx);
                                    }
                                    Err(err) => this.notice = Some((err.to_string(), false)),
                                }
                            }
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("from {}", sources.join(", "))),
                )
                .into_any_element(),
        )
    }

    /// Permissions and variables of a hook, for its detail
    pub(super) fn hook_access(&self, hook: &Hook, cx: &App) -> Option<Div> {
        let has_access =
            hook.permissions.is_some() || hook.reads.is_some() || hook.writes.is_some();
        has_access.then(|| permission_chips(hook, cx))
    }
}
