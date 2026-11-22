//! Éditeur de nœuds : les hooks d'un trigger en nœuds reliés par des fils, comme dans Blender.
//!
//! - fil pointillé : ordre (`needs`), du haut d'un nœud vers la prise « waits for »
//! - fil plein : donnée (`inputs`), d'une sortie vers une entrée
//! - tirer depuis une sortie et lâcher sur un nœud : crée une entrée (ou une attente sur « waits for »)
//! - glisser le fond : se déplacer ; Ctrl/Cmd + molette : zoomer

use std::{cell::Cell, collections::HashMap, path::PathBuf, rc::Rc, time::Duration};

use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use goat_hooker::config::Trigger;

use crate::graph_view::{Link, ViewNode};

// Géométrie des nœuds, à zoom 1
const NODE_WIDTH: f32 = 240.;
const HEADER: f32 = 36.;
const ROW: f32 = 24.;
const PADDING: f32 = 10.;
const COLUMN_GAP: f32 = 120.;
const ROW_GAP: f32 = 32.;
const ORIGIN: (f32, f32) = (40., 76.);
const SOCKET: f32 = 10.;
/// Prise « done » de l'en-tête : le fil tiré depuis elle crée une attente (`needs`)
const DONE: usize = usize::MAX;

pub enum CanvasEvent {
    Link {
        target: usize,
        source: usize,
        link: Link,
    },
    RemoveNeed {
        target: usize,
        source: usize,
    },
    RemoveInput {
        target: usize,
        name: String,
    },
    /// Ouvrir le hook dans la vue liste
    Select(usize),
    SetTrigger(Trigger),
    Run(Trigger),
}

/// Résultat du dernier lancement d'un nœud
#[derive(Clone, Copy)]
pub struct NodeStatus {
    pub success: bool,
    pub skipped: bool,
    pub duration: Duration,
}

enum Drag {
    Pan {
        mouse: Point<Pixels>,
        offset: Point<Pixels>,
    },
    /// Fil tiré depuis la sortie `output` du nœud `from`
    Wire {
        from: usize,
        output: usize,
        mouse: Point<Pixels>,
    },
}

pub struct GraphCanvas {
    pub trigger: Trigger,
    triggers: Vec<Trigger>,
    nodes: Vec<ViewNode>,
    error: Option<String>,
    statuses: HashMap<(PathBuf, String), NodeStatus>,
    running: bool,
    offset: Point<Pixels>,
    zoom: f32,
    drag: Option<Drag>,
    /// Position du canvas dans la fenêtre (pour les événements souris)
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl EventEmitter<CanvasEvent> for GraphCanvas {}

/// Lignes à gauche d'un nœud : « waits for », une par attente, une par entrée, « + input »
fn left_rows(node: &ViewNode) -> usize {
    1 + node.needs.len() + node.inputs.len() + usize::from(node.editable)
}

fn node_height(node: &ViewNode) -> f32 {
    HEADER + left_rows(node).max(node.outputs.len()) as f32 * ROW + PADDING
}

fn trigger_color(trigger: Trigger, cx: &App) -> Hsla {
    let theme = cx.theme();
    match trigger {
        Trigger::PreCommit => theme.blue,
        Trigger::PrePush => theme.magenta,
        Trigger::CommitMsg | Trigger::PrepareCommitMsg => theme.cyan,
        _ => theme.yellow,
    }
}

/// Nom d'entrée libre pour une sortie (`schema`, sinon `codegen_files`...)
fn input_name(target: &ViewNode, source: &ViewNode, output: &str) -> String {
    let base = if ["files", "stdout", "status"].contains(&output) {
        let hook: String = source
            .step
            .as_deref()
            .unwrap_or(&source.hook)
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '_'
                }
            })
            .collect();
        let hook = hook.trim_matches('_');
        let hook = if hook.is_empty() || hook.starts_with(|c: char| c.is_ascii_digit()) {
            format!("in_{hook}")
        } else {
            hook.to_string()
        };
        format!("{hook}_{output}")
    } else {
        output.to_string()
    };

    let mut name = base.clone();
    let mut n = 2;
    while target
        .inputs
        .iter()
        .any(|(existing, _, _)| *existing == name)
    {
        name = format!("{base}{n}");
        n += 1;
    }
    name
}

impl GraphCanvas {
    pub fn new() -> Self {
        Self {
            trigger: Trigger::PreCommit,
            triggers: vec![],
            nodes: vec![],
            error: None,
            statuses: HashMap::new(),
            running: false,
            offset: point(px(0.), px(0.)),
            zoom: 1.,
            drag: None,
            bounds: Rc::new(Cell::new(Bounds::default())),
        }
    }

    pub fn nodes(&self) -> &[ViewNode] {
        &self.nodes
    }

    pub fn set_data(
        &mut self,
        triggers: Vec<Trigger>,
        nodes: Result<Vec<ViewNode>, String>,
        statuses: HashMap<(PathBuf, String), NodeStatus>,
        running: bool,
        cx: &mut Context<Self>,
    ) {
        self.triggers = triggers;
        match nodes {
            Ok(nodes) => {
                self.nodes = nodes;
                self.error = None;
            }
            Err(err) => {
                self.nodes = vec![];
                self.error = Some(err);
            }
        }
        self.statuses = statuses;
        self.running = running;
        cx.notify();
    }

    fn reset_view(&mut self, cx: &mut Context<Self>) {
        self.offset = point(px(0.), px(0.));
        self.zoom = 1.;
        cx.notify();
    }

    /// Position (en haut à gauche) de chaque nœud, à zoom 1
    fn layout(&self) -> Vec<(f32, f32)> {
        let mut order: Vec<usize> = (0..self.nodes.len()).collect();
        order.sort_by_key(|&i| (self.nodes[i].column, self.nodes[i].row));
        let mut column_heights: Vec<f32> = Vec::new();
        let mut positions = vec![(0., 0.); self.nodes.len()];
        for i in order {
            let node = &self.nodes[i];
            if column_heights.len() <= node.column {
                column_heights.resize(node.column + 1, 0.);
            }
            let x = ORIGIN.0 + node.column as f32 * (NODE_WIDTH + COLUMN_GAP);
            let y = ORIGIN.1 + column_heights[node.column];
            column_heights[node.column] += node_height(node) + ROW_GAP;
            positions[i] = (x, y);
        }
        positions
    }

    /// Point du canvas (zoom 1) → point local à l'écran
    fn to_screen(&self, (x, y): (f32, f32)) -> Point<Pixels> {
        point(
            self.offset.x + px(x * self.zoom),
            self.offset.y + px(y * self.zoom),
        )
    }

    /// Point local à l'écran → point du canvas (zoom 1)
    fn to_canvas(&self, local: Point<Pixels>) -> (f32, f32) {
        (
            f32::from(local.x - self.offset.x) / self.zoom,
            f32::from(local.y - self.offset.y) / self.zoom,
        )
    }

    fn local(&self, window_position: Point<Pixels>) -> Point<Pixels> {
        window_position - self.bounds.get().origin
    }

    // MARK: souris

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.drag = Some(Drag::Pan {
            mouse: event.position,
            offset: self.offset,
        });
        cx.notify();
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let local = self.local(event.position);
        match self.drag.as_mut() {
            Some(Drag::Pan { mouse, offset }) => {
                self.offset = *offset + (event.position - *mouse);
                cx.notify();
            }
            Some(Drag::Wire { mouse, .. }) => {
                *mouse = local;
                cx.notify();
            }
            None => {}
        }
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(Drag::Wire { from, output, .. }) = self.drag.take() {
            let (x, y) = self.to_canvas(self.local(event.position));
            let layout = self.layout();
            let target = self.nodes.iter().enumerate().position(|(i, node)| {
                let (nx, ny) = layout[i];
                x >= nx && x <= nx + NODE_WIDTH && y >= ny && y <= ny + node_height(node)
            });
            if let Some(target) = target.filter(|t| *t != from) {
                let (_, ny) = layout[target];
                let on_waits_for = y >= ny + HEADER && y < ny + HEADER + ROW;
                let source = &self.nodes[from];
                let link = if output == DONE || on_waits_for {
                    Link::Needs
                } else {
                    let output_name = source.outputs[output].clone();
                    Link::Input {
                        name: input_name(&self.nodes[target], source, &output_name),
                        output: output_name,
                    }
                };
                cx.emit(CanvasEvent::Link {
                    target,
                    source: from,
                    link,
                });
            }
        } else {
            self.drag = None;
        }
        cx.notify();
    }

    fn on_scroll(&mut self, event: &ScrollEvent, window: &mut Window, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(window.line_height());
        if event.modifiers.secondary(cx) {
            // Zoom autour du pointeur
            let local = self.local(event.position);
            let old = self.zoom;
            let step = (f32::from(delta.y) * 0.0025).clamp(-0.15, 0.15);
            let zoom = (old * (1. + step)).clamp(0.35, 2.);
            let ratio = zoom / old;
            self.offset = point(
                local.x - (local.x - self.offset.x) * ratio,
                local.y - (local.y - self.offset.y) * ratio,
            );
            self.zoom = zoom;
        } else {
            self.offset += delta;
        }
        cx.notify();
    }

    // MARK: rendu

    /// Fils à dessiner (points locaux) : (départ, arrivée, couleur, pointillé)
    fn wires(
        &self,
        layout: &[(f32, f32)],
        cx: &App,
    ) -> Vec<(Point<Pixels>, Point<Pixels>, Hsla, bool)> {
        let theme = cx.theme();
        let data_color = trigger_color(self.trigger, cx);
        let mut wires = Vec::new();

        for (index, node) in self.nodes.iter().enumerate() {
            let (x, y) = layout[index];
            for &need in &node.needs {
                let (sx, sy) = layout[need];
                wires.push((
                    self.to_screen((sx + NODE_WIDTH, sy + HEADER / 2.)),
                    self.to_screen((x, y + HEADER + ROW / 2.)),
                    theme.muted_foreground,
                    true,
                ));
            }
            for (k, (_, source, output)) in node.inputs.iter().enumerate() {
                let (sx, sy) = layout[*source];
                let output_row = self.nodes[*source]
                    .outputs
                    .iter()
                    .position(|o| o == output)
                    .unwrap_or(0);
                let row = 1 + node.needs.len() + k;
                wires.push((
                    self.to_screen((
                        sx + NODE_WIDTH,
                        sy + HEADER + (output_row as f32 + 0.5) * ROW,
                    )),
                    self.to_screen((x, y + HEADER + (row as f32 + 0.5) * ROW)),
                    data_color,
                    false,
                ));
            }
        }

        if let Some(Drag::Wire {
            from,
            output,
            mouse,
        }) = self.drag.as_ref()
        {
            let (sx, sy) = layout[*from];
            let (y, color, dashed) = if *output == DONE {
                (sy + HEADER / 2., theme.muted_foreground, true)
            } else {
                (
                    sy + HEADER + (*output as f32 + 0.5) * ROW,
                    data_color,
                    false,
                )
            };
            wires.push((self.to_screen((sx + NODE_WIDTH, y)), *mouse, color, dashed));
        }
        wires
    }

    fn render_node(
        &self,
        index: usize,
        (x, y): (f32, f32),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let node = &self.nodes[index];
        let z = self.zoom;
        let s = |v: f32| px(v * z);
        let color = trigger_color(self.trigger, cx);
        let top_left = self.to_screen((x, y));
        let status = self
            .statuses
            .get(&(node.project_dir.clone(), node.label.clone()))
            .copied();

        // Ligne de texte à gauche ou à droite, à la ligne `row` sous l'en-tête
        let row_box = |row: usize| {
            div()
                .absolute()
                .top(s(HEADER + row as f32 * ROW))
                .h(s(ROW))
                .w(s(NODE_WIDTH / 2. + 40.))
                .flex()
                .items_center()
                .text_size(s(11.))
        };
        let socket = |color: Hsla, filled: bool| {
            div()
                .absolute()
                .size(s(SOCKET))
                .top(s((ROW - SOCKET) / 2.))
                .rounded_full()
                .border(s(1.5))
                .border_color(color)
                .when(filled, |this| this.bg(color))
                .when(!filled, |this| this.bg(theme.background))
        };

        let mut card = div()
            .id(("node", index))
            .absolute()
            .left(top_left.x)
            .top(top_left.y)
            .w(s(NODE_WIDTH))
            .h(s(node_height(node)))
            .rounded(s(8.))
            .border(s(1.))
            .border_color(theme.border)
            .bg(theme.background)
            .shadow_md()
            // Le fond ne doit pas bouger quand on clique sur un nœud
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());

        // En-tête : nom, projet, résultat
        card = card.child(
            h_flex()
                .id(("node-header", index))
                .absolute()
                .top_0()
                .left_0()
                .w_full()
                .h(s(HEADER))
                .pl(s(10.))
                .pr(s(16.))
                .gap(s(6.))
                .rounded_t(s(8.))
                .border_b(s(1.))
                .border_color(theme.border)
                .bg(color.opacity(0.08))
                .cursor_pointer()
                .child(div().w(s(3.)).h(s(16.)).rounded(s(2.)).bg(color))
                .child(
                    div()
                        .flex_1()
                        .text_size(s(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(node.label.clone()),
                )
                .child(
                    div()
                        .text_size(s(10.))
                        .text_color(theme.muted_foreground)
                        .child(node.project.clone()),
                )
                .when(!node.editable, |this| {
                    this.child(
                        div()
                            .text_size(s(10.))
                            .text_color(theme.info)
                            .child("shared"),
                    )
                })
                .children(status.map(|status| {
                    let (icon, color) = if status.skipped {
                        (IconName::Minus, theme.muted_foreground)
                    } else if status.success {
                        (IconName::CircleCheck, theme.success)
                    } else {
                        (IconName::CircleX, theme.danger)
                    };
                    h_flex()
                        .gap(s(3.))
                        .text_size(s(10.))
                        .text_color(theme.muted_foreground)
                        .when(!status.skipped, |this| {
                            this.child(format!("{:.1}s", status.duration.as_secs_f32()))
                        })
                        .child(Icon::new(icon).size(s(14.)).text_color(color))
                }))
                .on_click(cx.listener(move |_, _, _, cx| cx.emit(CanvasEvent::Select(index)))),
        );

        // Prise « done » : ce qui attend ce nœud part d'ici
        let waited = self.nodes.iter().any(|n| n.needs.contains(&index));
        card = card.child(
            socket(theme.muted_foreground, waited)
                .id(("done", index))
                .top(s((HEADER - SOCKET) / 2.))
                .right(s(-SOCKET / 2.))
                .cursor_crosshair()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        let mouse = this.local(event.position);
                        this.drag = Some(Drag::Wire {
                            from: index,
                            output: DONE,
                            mouse,
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
        );

        // Gauche : « waits for », attentes, entrées, « + input »
        let mut row = 0;
        card = card.child(
            row_box(row)
                .left_0()
                .pl(s(14.))
                .text_color(theme.muted_foreground)
                .child("waits for")
                .child(
                    socket(theme.muted_foreground, !node.needs.is_empty()).left(s(-SOCKET / 2.)),
                ),
        );
        row += 1;
        for &need in &node.needs {
            let label = self.label_from(index, need);
            card = card.child(
                row_box(row)
                    .left_0()
                    .pl(s(14.))
                    .gap(s(4.))
                    .child(div().text_color(theme.muted_foreground).child("⇢"))
                    .child(div().truncate().child(label))
                    .when(node.editable, |this| {
                        this.child(self.remove_button(
                            ("remove-need", index * 1000 + need),
                            CanvasEvent::RemoveNeed {
                                target: index,
                                source: need,
                            },
                            cx,
                        ))
                    }),
            );
            row += 1;
        }
        for (name, source, output) in &node.inputs {
            let from = format!("{}.{output}", self.label_from(index, *source));
            card = card.child(
                row_box(row)
                    .left_0()
                    .pl(s(14.))
                    .gap(s(4.))
                    .child(
                        div()
                            .text_color(color)
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!("{{{name}}}")),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_color(theme.muted_foreground)
                            .child(from),
                    )
                    .child(socket(color, true).left(s(-SOCKET / 2.)))
                    .when(node.editable, |this| {
                        this.child(self.remove_button(
                            SharedString::from(format!("remove-input-{index}-{name}")),
                            CanvasEvent::RemoveInput {
                                target: index,
                                name: name.clone(),
                            },
                            cx,
                        ))
                    }),
            );
            row += 1;
        }
        if node.editable {
            card = card.child(
                row_box(row)
                    .left_0()
                    .pl(s(14.))
                    .text_color(theme.muted_foreground.opacity(0.7))
                    .child("+ drop an output here")
                    .child(socket(theme.border, false).left(s(-SOCKET / 2.))),
            );
        }

        // Droite : sorties (on tire un fil depuis leur prise)
        for (k, output) in node.outputs.iter().enumerate() {
            let builtin = k < 3;
            let used = self.nodes.iter().any(|n| {
                n.inputs
                    .iter()
                    .any(|(_, from, o)| *from == index && o == output)
            });
            card = card.child(
                row_box(k)
                    .right_0()
                    .justify_end()
                    .pr(s(14.))
                    .text_color(if builtin {
                        theme.muted_foreground
                    } else {
                        theme.foreground
                    })
                    .when(!builtin, |this| this.font_weight(FontWeight::MEDIUM))
                    .child(output.clone())
                    .child(
                        socket(color, used || !builtin)
                            .id(("output", index * 100 + k))
                            .right(s(-SOCKET / 2.))
                            .cursor_crosshair()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                                    let mouse = this.local(event.position);
                                    this.drag = Some(Drag::Wire {
                                        from: index,
                                        output: k,
                                        mouse,
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                    ),
            );
        }

        card
    }

    fn remove_button(
        &self,
        id: impl Into<ElementId>,
        event: CanvasEvent,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = cx.theme().clone();
        let event = Rc::new(Cell::new(Some(event)));
        div()
            .id(id)
            .px(px(3. * self.zoom))
            .rounded(px(3.))
            .text_color(theme.muted_foreground)
            .cursor_pointer()
            .hover(|this| this.bg(theme.danger.opacity(0.15)).text_color(theme.danger))
            .child("×")
            .on_click(cx.listener(move |_, _, _, cx| {
                if let Some(event) = event.take() {
                    cx.emit(event);
                }
            }))
    }

    /// Nom d'un nœud vu depuis un autre (avec son projet s'il est ailleurs)
    fn label_from(&self, from: usize, node: usize) -> String {
        let target = &self.nodes[node];
        if target.project == self.nodes[from].project {
            target.label.clone()
        } else {
            format!("{}:{}", target.project, target.label)
        }
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let mut tabs = h_flex().gap_1();
        for trigger in &self.triggers {
            let trigger = *trigger;
            let selected = trigger == self.trigger;
            let button = Button::new(SharedString::from(format!("tab-{}", trigger.as_str())))
                .small()
                .label(trigger.as_str())
                .on_click(
                    cx.listener(move |_, _, _, cx| cx.emit(CanvasEvent::SetTrigger(trigger))),
                );
            tabs = tabs.child(if selected {
                button.primary()
            } else {
                button.ghost()
            });
        }

        h_flex()
            .absolute()
            .top_0()
            .left_0()
            .w_full()
            .px_3()
            .py_2()
            .gap_2()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.background.opacity(0.92))
            .child(tabs)
            .child(div().flex_1())
            .child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Drag from an output to a node · ⌘/Ctrl + scroll to zoom"),
            )
            .child(
                Button::new("fit")
                    .ghost()
                    .small()
                    .label(format!("{:.0}%", self.zoom * 100.))
                    .tooltip("Reset the view")
                    .on_click(cx.listener(|this, _, _, cx| this.reset_view(cx))),
            )
            .child(
                Button::new("run-graph")
                    .primary()
                    .small()
                    .icon(IconName::Play)
                    .label("Run")
                    .loading(self.running)
                    .on_click(
                        cx.listener(|this, _, _, cx| cx.emit(CanvasEvent::Run(this.trigger))),
                    ),
            )
    }
}

impl Render for GraphCanvas {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let layout = self.layout();
        let wires = self.wires(&layout, cx);
        let bounds = self.bounds.clone();
        let wire_width = px(2. * self.zoom.max(0.6));
        let grid = theme.border.opacity(0.5);
        let (offset, zoom) = (self.offset, self.zoom);

        let nodes: Vec<AnyElement> = (0..self.nodes.len())
            .map(|i| self.render_node(i, layout[i], cx).into_any_element())
            .collect();

        div()
            .id("graph-canvas")
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(theme.secondary.opacity(0.35))
            .when(matches!(self.drag, Some(Drag::Pan { .. })), |this| {
                this.cursor_grabbing()
            })
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_scroll(cx.listener(Self::on_scroll))
            // Grille de points et fils, sous les nœuds
            .child(
                canvas(
                    move |b, _, _| bounds.set(b),
                    move |b, _, window, _| {
                        let step = 24. * zoom;
                        let start_x = f32::from(offset.x).rem_euclid(step);
                        let start_y = f32::from(offset.y).rem_euclid(step);
                        let mut y = start_y;
                        while y < f32::from(b.size.height) {
                            let mut x = start_x;
                            while x < f32::from(b.size.width) {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        b.origin + point(px(x), px(y)),
                                        size(px(1.5), px(1.5)),
                                    ),
                                    grid,
                                ));
                                x += step;
                            }
                            y += step;
                        }

                        for (from, to, color, dashed) in wires {
                            let (from, to) = (b.origin + from, b.origin + to);
                            let dx = px((f32::from(to.x - from.x).abs() / 2.).max(40.));
                            let mut path = PathBuilder::stroke(wire_width);
                            if dashed {
                                path = path.dash_array(&[px(6.), px(4.)]);
                            }
                            path.move_to(from);
                            path.cubic_bezier_to(
                                to,
                                from + point(dx, px(0.)),
                                to - point(dx, px(0.)),
                            );
                            if let Ok(path) = path.build() {
                                window.paint_path(path, color);
                            }
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            .children(nodes)
            .when(self.nodes.is_empty(), |this| {
                this.child(
                    div()
                        .absolute()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(theme.muted_foreground)
                        .child(match self.error.clone() {
                            Some(error) => error,
                            None => format!("No {} hooks", self.trigger.as_str()),
                        }),
                )
            })
            .when_some(
                self.error.clone().filter(|_| !self.nodes.is_empty()),
                |this, error| {
                    this.child(
                        div()
                            .absolute()
                            .bottom_3()
                            .left_3()
                            .p_2()
                            .rounded_md()
                            .text_sm()
                            .bg(theme.danger.opacity(0.12))
                            .text_color(theme.danger)
                            .child(error),
                    )
                },
            )
            .child(self.render_toolbar(cx))
    }
}
