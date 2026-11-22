//! Formulaire d'ajout / modification d'un hook

use gpui_kit::component::{
    ActiveTheme, IndexPath,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputState, Textarea, TextareaState},
    select::{Select, SelectEvent, SelectState},
    switch::Switch,
    v_flex,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use goat_hooker::config::{Hook, Trigger};

type TriggerSelect = SelectState<Vec<&'static str>>;

pub enum EditorEvent {
    Save {
        index: Option<usize>,
        hook: Box<Hook>,
    },
    Cancel,
}

pub struct HookEditor {
    /// Hook modifié (`None` = nouveau hook)
    pub index: Option<usize>,
    name: Entity<InputState>,
    include: Entity<InputState>,
    exclude: Entity<InputState>,
    commands: Entity<TextareaState>,
    trigger: Entity<TriggerSelect>,
    selected_trigger: Trigger,
    stage_fixed: bool,
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
            error: None,
            _subscriptions: vec![subscription],
        }
    }

    /// Construit le hook à partir du formulaire
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
        if commands.is_empty() {
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

        Ok(Hook {
            name,
            trigger: Some(self.selected_trigger),
            include: patterns(&self.include),
            exclude: patterns(&self.exclude),
            cmd,
            cmds,
            stage_fixed: self.stage_fixed.then_some(true),
            ..Default::default()
        })
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

impl Render for HookEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = if self.index.is_some() {
            "Edit hook"
        } else {
            "New hook"
        };
        let stage_fixed = self.stage_fixed;

        v_flex()
            .gap_4()
            .max_w(px(760.))
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(field("Name", None, Input::new(&self.name), cx).flex_1())
                    .child(field("Trigger", None, Select::new(&self.trigger), cx).w(px(220.))),
            )
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
