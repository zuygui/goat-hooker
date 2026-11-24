//! Editor of a config (`hooker.toml` or `hooker.config.json`): colors, validation while
//! typing and, for JSON, autocompletion of the keys and of the values (triggers, hooks,
//! variables, outputs...).

use std::{
    path::{Path, PathBuf},
    rc::Rc,
};

use anyhow::Result;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, IconName, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    input::{CompletionProvider, Editor, EditorState, InputEvent, Rope, RopeExt},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use lsp_types::{
    CompletionContext, CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit,
    TextEdit,
};

use goat_hooker::config::Config;

use crate::json_complete::{self, Known};

pub enum JsonEditorEvent {
    Saved,
    Close,
}

struct Completions {
    known: Known,
}

impl CompletionProvider for Completions {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        _: CompletionContext,
        _: &mut Window,
        _: &mut App,
    ) -> Task<Result<CompletionResponse>> {
        let content = text.to_string();
        let (position, suggestions) = json_complete::suggest(&content, offset, &self.known);
        // Replaces the token being typed (its quote included)
        // The closing quote added by the editor is replaced too
        let end = if content[offset.min(content.len())..].starts_with('"')
            && content[position.start..offset.min(content.len())].starts_with('"')
        {
            offset + 1
        } else {
            offset
        };
        let range = lsp_types::Range {
            start: text.offset_to_position(position.start),
            end: text.offset_to_position(end),
        };
        let items = suggestions
            .into_iter()
            .map(|s| CompletionItem {
                label: s.label.clone(),
                filter_text: Some(position.prefix.clone()),
                kind: Some(if position.key {
                    CompletionItemKind::PROPERTY
                } else {
                    CompletionItemKind::VALUE
                }),
                detail: Some(s.detail),
                text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                    range,
                    new_text: s.insert,
                })),
                ..Default::default()
            })
            .collect();
        Task::ready(Ok(CompletionResponse::Array(items)))
    }

    fn is_completion_trigger(&self, _: usize, new_text: &str, _: &mut App) -> bool {
        new_text
            .chars()
            .any(|c| c == '"' || c == ':' || c == '[' || c == ',' || c.is_alphanumeric())
    }
}

pub struct JsonEditor {
    pub file: PathBuf,
    label: String,
    state: Entity<EditorState>,
    saved: String,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<JsonEditorEvent> for JsonEditor {}

/// Error of the config, with its line
fn validate(file: &Path, text: &str) -> Option<String> {
    let value = if is_toml(file) {
        match toml::from_str::<toml::Value>(text) {
            Ok(value) => serde_json::to_value(value).unwrap_or_default(),
            Err(err) => return Some(err.to_string()),
        }
    } else {
        match serde_json::from_str(text) {
            Ok(value) => value,
            Err(err) => return Some(format!("Line {}: {err}", err.line())),
        }
    };
    let config: Config = match serde_json::from_value(value) {
        Ok(config) => config,
        Err(err) => return Some(err.to_string()),
    };
    for hook in config.hooks.iter().flatten() {
        if let Err(err) = hook.validate() {
            return Some(err.to_string());
        }
    }
    None
}

fn is_toml(file: &Path) -> bool {
    file.extension().is_some_and(|e| e == "toml")
}

impl JsonEditor {
    pub fn new(
        file: PathBuf,
        label: String,
        known: Known,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let content = std::fs::read_to_string(&file).unwrap_or_default();
        let state = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(if is_toml(&file) { "toml" } else { "json" })
                .line_number(true)
                .default_value(content.clone())
        });
        // The completion reads JSON, TOML only gets colors and validation
        if !is_toml(&file) {
            state.update(cx, |state, cx| {
                state.lsp_mut().completion_provider = Some(Rc::new(Completions { known }));
                cx.notify();
            });
        }
        let subscription =
            cx.subscribe(&state, |this: &mut Self, state, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    this.error = validate(&this.file, &state.read(cx).value());
                    cx.notify();
                }
            });
        Self {
            error: validate(&file, &content),
            file,
            label,
            state,
            saved: content,
            _subscriptions: vec![subscription],
        }
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let text = self.state.read(cx).value().to_string();
        if let Some(error) = validate(&self.file, &text) {
            self.error = Some(error);
            cx.notify();
            return;
        }
        match std::fs::write(&self.file, &text) {
            Ok(()) => {
                self.saved = text;
                cx.emit(JsonEditorEvent::Saved);
            }
            Err(err) => self.error = Some(err.to_string()),
        }
        cx.notify();
    }
}

impl Render for JsonEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let dirty = self.state.read(cx).value().as_ref() != self.saved;
        v_flex()
            .size_full()
            .p_4()
            .gap_3()
            .child(
                h_flex()
                    .gap_2()
                    .child(Icon::empty().path("icons/pencil.svg").small())
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(self.label.clone()),
                    )
                    .when(dirty, |this| {
                        this.child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("modified"),
                        )
                    })
                    .child(div().flex_1())
                    .child(match self.error.as_ref() {
                        Some(_) => h_flex()
                            .gap_1()
                            .text_xs()
                            .text_color(theme.danger)
                            .child(Icon::new(IconName::CircleX).xsmall())
                            .child("invalid"),
                        None => h_flex()
                            .gap_1()
                            .text_xs()
                            .text_color(theme.success)
                            .child(Icon::new(IconName::CircleCheck).xsmall())
                            .child("valid"),
                    })
                    .child(
                        Button::new("json-close")
                            .ghost()
                            .small()
                            .label("Close")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(JsonEditorEvent::Close))),
                    )
                    .child(
                        Button::new("json-save")
                            .primary()
                            .small()
                            .label("Save")
                            .disabled(!dirty || self.error.is_some())
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    ),
            )
            .child(div().text_xs().text_color(theme.muted_foreground).child(
                "Type a quote to get the keys and values that fit, ctrl+space to ask for them",
            ))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(Editor::new(&self.state).h_full()),
            )
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    div()
                        .p_2()
                        .rounded_md()
                        .text_sm()
                        .bg(theme.danger.opacity(0.1))
                        .text_color(theme.danger)
                        .child(error),
                )
            })
    }
}
