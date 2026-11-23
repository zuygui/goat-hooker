//! Form to add / edit a hook

use gpui_kit::component::{
    ActiveTheme, IndexPath, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputState, Textarea, TextareaState},
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use goat_hooker::config::{FileAccess, Hook, Permissions, Trigger};

type TriggerSelect = SelectState<Vec<&'static str>>;

pub enum EditorEvent {
    Save {
        index: Option<usize>,
        hook: Box<Hook>,
    },
    Cancel,
}

pub struct HookEditor {
    /// Edited hook (`None` = new hook)
    pub index: Option<usize>,
    name: Entity<InputState>,
    include: Entity<InputState>,
    exclude: Entity<InputState>,
    commands: Entity<TextareaState>,
    trigger: Entity<TriggerSelect>,
    selected_trigger: Trigger,
    stage_fixed: bool,
    reads: Entity<InputState>,
    writes: Entity<InputState>,
    /// `None`: default of the hook (everything for a hook of the repository)
    files: Option<FileAccess>,
    network: Option<bool>,
    env: Entity<InputState>,
    /// The hook as it was: the fields the form doesn't show are kept
    original: Hook,
    compact: bool,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for HookEditor {}

impl HookEditor {
    pub fn new(
        index: Option<usize>,
        hook: Option<&Hook>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let hook = hook.cloned().unwrap_or_default();
        let selected_trigger = hook.trigger.unwrap_or(Trigger::PreCommit);

        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("lint")
                .default_value(hook.name.clone())
        });
        let include = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("src/**/*.rs, *.toml  (empty = all files)")
                .default_value(hook.include.clone().unwrap_or_default().join(", "))
        });
        let exclude = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("src/generated/**")
                .default_value(hook.exclude.clone().unwrap_or_default().join(", "))
        });
        let commands = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(4)
                .placeholder("cargo fmt --check -- {files}")
                .default_value(hook.commands().join("\n"))
        });

        let list = |items: &Option<Vec<String>>| items.clone().unwrap_or_default().join(", ");
        let reads = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("version, token")
                .default_value(list(&hook.reads))
        });
        let writes = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("version")
                .default_value(list(&hook.writes))
        });
        let permissions = hook.permissions.clone().unwrap_or_default();
        let env = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("NPM_TOKEN, CARGO_HOME  (empty = all of them)")
                .default_value(list(&permissions.env))
        });

        let triggers: Vec<&'static str> = Trigger::ALL.iter().map(|t| t.as_str()).collect();
        let index_of = Trigger::ALL
            .iter()
            .position(|t| *t == selected_trigger)
            .unwrap_or(0);
        let trigger =
            cx.new(|cx| SelectState::new(triggers, Some(IndexPath::new(index_of)), window, cx));

        let subscription = cx.subscribe_in(
            &trigger,
            window,
            |this, _, event: &SelectEvent<Vec<&'static str>>, _, cx| {
                let SelectEvent::Confirm(Some(value)) = event else {
                    return;
                };
                if let Some(trigger) = Trigger::ALL.iter().find(|t| t.as_str() == *value) {
                    this.selected_trigger = *trigger;
                    cx.notify();
                }
            },
        );

        Self {
            index,
            name,
            include,
            exclude,
            commands,
            trigger,
            selected_trigger,
            stage_fixed: hook.stage_fixed.unwrap_or(false),
            reads,
            writes,
            files: permissions.files,
            network: permissions.network,
            env,
            original: hook.clone(),
            compact: false,
            error: None,
            _subscriptions: vec![subscription],
        }
    }

    /// Builds the hook from the form
    fn build_hook(&self, cx: &App) -> Result<Hook, String> {
        let name = self.name.read(cx).value().trim().to_string();
        if name.is_empty() {
            return Err("The hook needs a name".to_string());
        }

        let commands: Vec<String> = self
            .commands
            .read(cx)
            .value()
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if commands.is_empty() && self.original.steps.is_none() {
            return Err("Add at least one command".to_string());
        }

        let patterns = |input: &Entity<InputState>| -> Option<Vec<String>> {
            let patterns: Vec<String> = input
                .read(cx)
                .value()
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();
            (!patterns.is_empty()).then_some(patterns)
        };

        let (cmd, cmds) = match commands.len() {
            1 => (commands.into_iter().next(), None),
            _ => (None, Some(commands)),
        };

        let permissions = Permissions {
            files: self.files,
            network: self.network,
            env: patterns(&self.env),
        };
        // A hook with steps keeps them: its commands aren't edited here
        let (cmd, cmds) = if self.original.steps.is_some() {
            (None, None)
        } else {
            (cmd, cmds)
        };
        Ok(Hook {
            name,
            trigger: Some(self.selected_trigger),
            include: patterns(&self.include),
            exclude: patterns(&self.exclude),
            cmd,
            cmds,
            stage_fixed: self.stage_fixed.then_some(true),
            reads: patterns(&self.reads),
            writes: patterns(&self.writes),
            permissions: (permissions != Permissions::default()).then_some(permissions),
            // needs, inputs, outputs, steps, cache... stay as they were
            ..self.original.clone()
        })
    }

    /// Narrow layout, next to the graph
    pub fn compact(mut self) -> Self {
        self.compact = true;
        self
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        match self.build_hook(cx) {
            Ok(hook) => cx.emit(EditorEvent::Save {
                index: self.index,
                hook: Box::new(hook),
            }),
            Err(err) => {
                self.error = Some(err);
                cx.notify();
            }
        }
    }
}

fn field(
    label: &'static str,
    hint: Option<&'static str>,
    input: impl IntoElement,
    cx: &App,
) -> Div {
    v_flex()
        .gap_1()
        .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
        .child(input)
        .when_some(hint, |this, hint| {
            this.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(hint),
            )
        })
}

impl HookEditor {
    /// Choice between a few values, as buttons
    fn choice<T: Copy + PartialEq + 'static>(
        &self,
        id: &'static str,
        options: &[(&'static str, T)],
        current: T,
        set: fn(&mut Self, T),
        cx: &mut Context<Self>,
    ) -> Div {
        let mut row = h_flex().gap_1();
        for (i, (label, value)) in options.iter().enumerate() {
            let value = *value;
            let button = Button::new((id, i))
                .xsmall()
                .label(*label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    set(this, value);
                    cx.notify();
                }));
            row = row.child(if value == current {
                button.primary()
            } else {
                button.ghost()
            });
        }
        row
    }

    fn render_permissions(&self, cx: &mut Context<Self>) -> Div {
        let files = self.choice(
            "perm-files",
            &[
                ("default", None),
                ("read only", Some(FileAccess::Read)),
                ("modify", Some(FileAccess::Write)),
            ],
            self.files,
            |this, v| this.files = v,
            cx,
        );
        let network = self.choice(
            "perm-network",
            &[
                ("default", None),
                ("allowed", Some(true)),
                ("blocked", Some(false)),
            ],
            self.network,
            |this, v| this.network = v,
            cx,
        );
        v_flex()
            .gap_2()
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child("Permissions"))
            .child(h_flex().gap_3().child(div().w(px(70.)).text_sm().child("Files")).child(files))
            .child(h_flex().gap_3().child(div().w(px(70.)).text_sm().child("Network")).child(network))
            .child(field("Environment", Some("Variables it gets besides PATH, HOME, GIT_*…"), Input::new(&self.env), cx))
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Default: everything for a hook of this repository, read-only files without network for a hook of another one."),
            )
    }
}

impl Render for HookEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = if self.index.is_some() {
            "Edit hook"
        } else {
            "New hook"
        };
        let stage_fixed = self.stage_fixed;

        let compact = self.compact;
        v_flex()
            .gap_4()
            .max_w(px(760.))
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(if compact {
                v_flex()
                    .gap_4()
                    .child(field("Name", None, Input::new(&self.name), cx))
                    .child(field("Trigger", None, Select::new(&self.trigger), cx))
            } else {
                h_flex()
                    .gap_4()
                    .child(field("Name", None, Input::new(&self.name), cx).flex_1())
                    .child(field("Trigger", None, Select::new(&self.trigger), cx).w(px(220.)))
            })
            .child(field(
                "Files",
                Some("Glob patterns relative to the project, separated by commas. `*` stays in its folder, `**` goes through folders."),
                Input::new(&self.include),
                cx,
            ))
            .child(field("Ignored files", None, Input::new(&self.exclude), cx))
            .child(field(
                "Commands",
                Some("One command per line, run from the project folder. {files} is replaced by the targeted files, Git arguments are $1, $2…"),
                Textarea::new(&self.commands),
                cx,
            ))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Switch::new("stage-fixed")
                            .checked(stage_fixed)
                            .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                this.stage_fixed = *checked;
                                cx.notify();
                            })),
                    )
                    .child(div().text_sm().child("Re-stage the files after the hook (formatters)")),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(field("Reads", Some("Variables, as $name"), Input::new(&self.reads), cx).flex_1())
                    .child(field("Writes", Some("echo \"name=value\" >> $HOOKER_VARS"), Input::new(&self.writes), cx).flex_1()),
            )
            .child(self.render_permissions(cx))
            .when_some(self.error.clone(), |this, error| {
                this.child(div().text_sm().text_color(cx.theme().danger).child(error))
            })
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("cancel")
                            .ghost()
                            .label("Cancel")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(EditorEvent::Cancel))),
                    )
                    .child(
                        Button::new("save")
                            .primary()
                            .label("Save")
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            )
    }
}
