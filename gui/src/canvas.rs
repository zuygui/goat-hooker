//! Node editor: the hooks of a trigger as nodes linked by wires, like in Blender.
//!
//! - dashed wire: order (`needs`), from the top of a node to the “waits for” socket
//! - solid wire: data (`inputs`), from an output to an input
//! - drag from an output and drop on a node: creates an input (or a wait on “waits for”)
//! - drag the background: move; Ctrl/Cmd + scroll: zoom

use std::{cell::Cell, collections::HashMap, path::PathBuf, rc::Rc, time::Duration};

use gpui_kit::component::{
    ActiveTheme, Icon, IconName, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use goat_hooker::config::Trigger;

use crate::graph_view::{Link, ViewNode};

// Geometry of the nodes, at zoom 1
const NODE_WIDTH: f32 = 240.;
const HEADER: f32 = 36.;
const ROW: f32 = 24.;
const PADDING: f32 = 10.;
const COLUMN_GAP: f32 = 120.;
const ROW_GAP: f32 = 32.;
const ORIGIN: (f32, f32) = (40., 76.);
const SOCKET: f32 = 10.;
/// “done” socket of the header: a wire dragged from it creates a wait (`needs`)
const DONE: usize = usize::MAX;

gpui_kit::actions!(graph_canvas, [UndoGraph, RedoGraph]);

/// Shortcuts of the canvas (when it has the focus)
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-z", UndoGraph, Some("GraphCanvas")),
        KeyBinding::new("ctrl-z", UndoGraph, Some("GraphCanvas")),
        KeyBinding::new("cmd-shift-z", RedoGraph, Some("GraphCanvas")),
        KeyBinding::new("ctrl-shift-z", RedoGraph, Some("GraphCanvas")),
        KeyBinding::new("ctrl-y", RedoGraph, Some("GraphCanvas")),
    ]);
}

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
    /// Open the hook in the list view
    Select(usize),
    /// New hook of the current trigger, linked to `link` (source node, output or order)
    Create {
        project: usize,
        name: String,
        cmd: String,
        link: Option<(usize, Link)>,
    },
    Delete(usize),
    Undo,
    Redo,
    SetTrigger(Trigger),
    Run(Trigger),
}

/// A wire to paint (local points)
struct Wire {
    from: Point<Pixels>,
    to: Point<Pixels>,
    color: Hsla,
    /// Dash and gap lengths (`None`: solid)
    dash: Option<[f32; 2]>,
    /// Width and opacity factor (hovered node)
    emphasis: f32,
}

/// Result of the last run of a node
#[derive(Clone, Copy)]
pub struct NodeStatus {
    pub success: bool,
    pub skipped: bool,
    pub denied: bool,
    pub cached: bool,
    pub duration: Duration,
}

enum Drag {
    Pan {
        mouse: Point<Pixels>,
        offset: Point<Pixels>,
    },
    /// Wire dragged from the output `output` of the node `from`
    Wire {
        from: usize,
        output: usize,
        mouse: Point<Pixels>,
    },
}

/// Form creating a hook, opened by a double-click or a wire dropped on the background
struct NewHook {
    /// Where it is shown (local point)
    at: Point<Pixels>,
    /// Wire it comes from: (node, output index or `DONE`)
    from: Option<(usize, usize)>,
    project: usize,
    name: Entity<InputState>,
    cmd: Entity<InputState>,
}

pub struct GraphCanvas {
    pub trigger: Trigger,
    /// Projects of the repository (label, folder), to create hooks in
    projects: Vec<String>,
    new_hook: Option<NewHook>,
    hovered: Option<usize>,
    show_help: bool,
    focus: FocusHandle,
    /// Changes can be undone / redone
    can_undo: bool,
    can_redo: bool,
    _subscriptions: Vec<Subscription>,
    triggers: Vec<Trigger>,
    nodes: Vec<ViewNode>,
    error: Option<String>,
    statuses: HashMap<(PathBuf, String), NodeStatus>,
    running: bool,
    offset: Point<Pixels>,
    zoom: f32,
    drag: Option<Drag>,
    /// Position of the canvas in the window (for the mouse events)
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl EventEmitter<CanvasEvent> for GraphCanvas {}

/// Rows on the left of a node: “waits for”, one per wait, one per input, “+ input”
fn left_rows(node: &ViewNode) -> usize {
    1 + node.needs.len() + node.inputs.len() + usize::from(node.editable)
}

/// Footer listing the variables of a node
fn has_variables(node: &ViewNode) -> bool {
    !node.reads.is_empty() || !node.writes.is_empty()
}

fn node_height(node: &ViewNode) -> f32 {
    HEADER
        + left_rows(node).max(node.outputs.len()) as f32 * ROW
        + if has_variables(node) { ROW } else { 0. }
        + PADDING
}

/// Link created by a wire from the output `output` (or `DONE`) of `source`
fn link_for(source: &ViewNode, output: usize, target: Option<&ViewNode>) -> Link {
    if output == DONE {
        return Link::Needs;
    }
    let output_name = source.outputs[output].clone();
    let name = match target {
        Some(target) => input_name(target, source, &output_name),
        None => input_name(&ViewNode::empty(), source, &output_name),
    };
    Link::Input {
        name,
        output: output_name,
    }
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

/// Free input name for an output (`schema`, else `codegen_files`...)
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
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
            can_undo: false,
            can_redo: false,
            trigger: Trigger::PreCommit,
            projects: vec![],
            new_hook: None,
            hovered: None,
            show_help: false,
            _subscriptions: vec![],
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

    pub fn set_history(&mut self, can_undo: bool, can_redo: bool) {
        self.can_undo = can_undo;
        self.can_redo = can_redo;
    }

    pub fn set_projects(&mut self, projects: Vec<String>) {
        self.projects = projects;
    }

    /// Opens the form creating a hook at `at` (local point)
    fn open_new_hook(
        &mut self,
        at: Point<Pixels>,
        from: Option<(usize, usize)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.projects.is_empty() {
            return;
        }
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Hook name"));
        let cmd = cx.new(|cx| InputState::new(window, cx).placeholder("Command, e.g. npm test"));
        name.update(cx, |input, cx| input.focus(window, cx));
        // Enter in the command creates it
        let subscription = cx.subscribe(&cmd, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.submit_new_hook(cx);
            }
        });
        self._subscriptions = vec![subscription];
        // A wire from a node of another project creates the hook in that project
        let project = from
            .and_then(|(node, _)| {
                let project = &self.nodes[node].project;
                self.projects
                    .iter()
                    .position(|p| p == project || (project == "root" && p == "."))
            })
            .unwrap_or(0);
        self.new_hook = Some(NewHook {
            at,
            from,
            project,
            name,
            cmd,
        });
        cx.notify();
    }

    fn submit_new_hook(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.new_hook.take() else {
            return;
        };
        let name = form.name.read(cx).value().trim().to_string();
        let cmd = form.cmd.read(cx).value().trim().to_string();
        if name.is_empty() || cmd.is_empty() {
            self.new_hook = Some(form);
            return;
        }
        let link = form
            .from
            .map(|(node, output)| (node, link_for(&self.nodes[node], output, None)));
        cx.emit(CanvasEvent::Create {
            project: form.project,
            name,
            cmd,
            link,
        });
        cx.notify();
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

    /// Position (top left) of each node, at zoom 1
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

    /// Point of the canvas (zoom 1) → local point on the screen
    fn to_screen(&self, (x, y): (f32, f32)) -> Point<Pixels> {
        point(
            self.offset.x + px(x * self.zoom),
            self.offset.y + px(y * self.zoom),
        )
    }

    /// Local point on the screen → point of the canvas (zoom 1)
    fn to_canvas(&self, local: Point<Pixels>) -> (f32, f32) {
        (
            f32::from(local.x - self.offset.x) / self.zoom,
            f32::from(local.y - self.offset.y) / self.zoom,
        )
    }

    fn local(&self, window_position: Point<Pixels>) -> Point<Pixels> {
        window_position - self.bounds.get().origin
    }

    // MARK: mouse

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.click_count >= 2 {
            let at = self.local(event.position);
            self.open_new_hook(at, None, window, cx);
            return;
        }
        self.new_hook = None;
        window.focus(&self.focus, cx);
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

    fn on_mouse_up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Drag::Wire { from, output, .. }) = self.drag.take() {
            let (x, y) = self.to_canvas(self.local(event.position));
            let layout = self.layout();
            let target = self.nodes.iter().enumerate().position(|(i, node)| {
                let (nx, ny) = layout[i];
                x >= nx && x <= nx + NODE_WIDTH && y >= ny && y <= ny + node_height(node)
            });
            match target {
                Some(target) if target != from => {
                    let (_, ny) = layout[target];
                    let on_waits_for = y >= ny + HEADER && y < ny + HEADER + ROW;
                    let source = &self.nodes[from];
                    let link = if on_waits_for {
                        Link::Needs
                    } else {
                        link_for(source, output, Some(&self.nodes[target]))
                    };
                    cx.emit(CanvasEvent::Link {
                        target,
                        source: from,
                        link,
                    });
                }
                Some(_) => {}
                // Dropped on the background: a new hook receiving it
                None => {
                    let at = self.local(event.position);
                    self.open_new_hook(at, Some((from, output)), window, cx);
                }
            }
        } else {
            self.drag = None;
        }
        cx.notify();
    }

    fn on_scroll(&mut self, event: &ScrollEvent, window: &mut Window, cx: &mut Context<Self>) {
        let delta = event.delta.pixel_delta(window.line_height());
        if event.modifiers.secondary(cx) {
            // Zoom around the pointer
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

    // MARK: render

    /// Wires to paint (local points): (start, end, color, dashed)
    fn wires(&self, layout: &[(f32, f32)], cx: &App) -> Vec<Wire> {
        let theme = cx.theme();
        let data_color = trigger_color(self.trigger, cx);
        let variable_color = theme.magenta;
        let mut wires = Vec::new();
        // With a hovered node, its wires stand out and the others fade
        let emphasis = |a: usize, b: usize| match self.hovered {
            Some(h) if h == a || h == b => 1.6,
            Some(_) => 0.35,
            None => 1.,
        };

        for (index, node) in self.nodes.iter().enumerate() {
            let (x, y) = layout[index];
            for &need in &node.needs {
                let (sx, sy) = layout[need];
                wires.push(Wire {
                    from: self.to_screen((sx + NODE_WIDTH, sy + HEADER / 2.)),
                    to: self.to_screen((x, y + HEADER + ROW / 2.)),
                    color: theme.muted_foreground,
                    dash: Some([6., 4.]),
                    emphasis: emphasis(index, need),
                });
            }
            for (k, (_, source, output)) in node.inputs.iter().enumerate() {
                let (sx, sy) = layout[*source];
                let output_row = self.nodes[*source]
                    .outputs
                    .iter()
                    .position(|o| o == output)
                    .unwrap_or(0);
                let row = 1 + node.needs.len() + k;
                wires.push(Wire {
                    from: self.to_screen((
                        sx + NODE_WIDTH,
                        sy + HEADER + (output_row as f32 + 0.5) * ROW,
                    )),
                    to: self.to_screen((x, y + HEADER + (row as f32 + 0.5) * ROW)),
                    color: data_color,
                    dash: None,
                    emphasis: emphasis(index, *source),
                });
            }
            // Variables: from the writer to the footer of the reader
            for &writer in &node.after {
                let (sx, sy) = layout[writer];
                let writer_node = &self.nodes[writer];
                wires.push(Wire {
                    from: self.to_screen((
                        sx + NODE_WIDTH,
                        sy + node_height(writer_node) - PADDING - ROW / 2.,
                    )),
                    to: self.to_screen((x, y + node_height(node) - PADDING - ROW / 2.)),
                    color: variable_color,
                    dash: Some([2., 4.]),
                    emphasis: emphasis(index, writer),
                });
            }
        }

        if let Some(Drag::Wire {
            from,
            output,
            mouse,
        }) = self.drag.as_ref()
        {
            let (sx, sy) = layout[*from];
            let (y, color, dash) = if *output == DONE {
                (sy + HEADER / 2., theme.muted_foreground, Some([6., 4.]))
            } else {
                (sy + HEADER + (*output as f32 + 0.5) * ROW, data_color, None)
            };
            wires.push(Wire {
                from: self.to_screen((sx + NODE_WIDTH, y)),
                to: *mouse,
                color,
                dash,
                emphasis: 1.4,
            });
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

        // Text row on the left or on the right, at the row `row` under the header
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
            .border_color(if self.hovered == Some(index) {
                color
            } else {
                theme.border
            })
            .bg(theme.background)
            .shadow_md()
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                if *hovered {
                    this.hovered = Some(index);
                } else if this.hovered == Some(index) {
                    this.hovered = None;
                }
                cx.notify();
            }))
            // The background must not move when a node is clicked
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());

        // Header: name, project, result
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
                .when(node.denied.is_some(), |this| {
                    this.child(
                        div()
                            .id(("denied", index))
                            .px(s(4.))
                            .rounded(s(3.))
                            .text_size(s(10.))
                            .bg(theme.danger.opacity(0.12))
                            .text_color(theme.danger)
                            .child("denied")
                            .tooltip({
                                let reason = node.denied.clone().unwrap_or_default();
                                move |window, cx| {
                                    gpui_kit::component::tooltip::Tooltip::new(reason.clone())
                                        .build(window, cx)
                                }
                            }),
                    )
                })
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
                    } else if status.denied {
                        (IconName::CircleX, theme.danger)
                    } else if status.success {
                        (IconName::CircleCheck, theme.success)
                    } else {
                        (IconName::CircleX, theme.danger)
                    };
                    h_flex()
                        .gap(s(3.))
                        .text_size(s(10.))
                        .text_color(theme.muted_foreground)
                        .when(!status.skipped && !status.denied, |this| {
                            this.child(if status.cached {
                                "cached".to_string()
                            } else {
                                format!("{:.1}s", status.duration.as_secs_f32())
                            })
                        })
                        .child(Icon::new(icon).size(s(14.)).text_color(color))
                }))
                .when(node.editable && self.hovered == Some(index), |this| {
                    this.child(
                        div()
                            .id(("delete-node", index))
                            .p(s(2.))
                            .rounded(s(4.))
                            .text_color(theme.muted_foreground)
                            .hover(|this| {
                                this.bg(theme.danger.opacity(0.15)).text_color(theme.danger)
                            })
                            .child(Icon::empty().path("icons/trash.svg").size(s(13.)))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.stop_propagation();
                                cx.emit(CanvasEvent::Delete(index));
                            })),
                    )
                })
                .on_click(cx.listener(move |_, _, _, cx| cx.emit(CanvasEvent::Select(index)))),
        );

        // “done” socket: what waits for this node starts from here
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

        // Left: “waits for”, waits, inputs, “+ input”
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

        // Right: outputs (a wire is dragged from their socket)
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

        // Footer: variables read and written
        if has_variables(node) {
            let variable_color = theme.magenta;
            let footer_top = node_height(node) - PADDING - ROW;
            card = card.child(
                h_flex()
                    .absolute()
                    .left_0()
                    .top(s(footer_top))
                    .w_full()
                    .h(s(ROW))
                    .px(s(14.))
                    .gap(s(4.))
                    .border_t(s(1.))
                    .border_color(theme.border)
                    .text_size(s(10.))
                    .overflow_hidden()
                    .children(node.reads.iter().map(|v| {
                        div()
                            .px(s(4.))
                            .rounded(s(3.))
                            .bg(variable_color.opacity(0.1))
                            .text_color(variable_color)
                            .child(format!("reads ${v}"))
                    }))
                    .children(node.writes.iter().map(|v| {
                        div()
                            .px(s(4.))
                            .rounded(s(3.))
                            .bg(variable_color.opacity(0.2))
                            .text_color(variable_color)
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!("writes ${v}"))
                    })),
            );
        }

        card
    }

    fn render_new_hook(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let form = self.new_hook.as_ref()?;
        let theme = cx.theme().clone();
        let color = trigger_color(self.trigger, cx);
        let bounds = self.bounds.get();
        // Keeps the form inside the canvas
        let x = form.at.x.min(bounds.size.width - px(320.)).max(px(8.));
        let y = form.at.y.min(bounds.size.height - px(260.)).max(px(56.));

        let subtitle = match form.from {
            Some((node, DONE)) => format!("runs after {}", self.nodes[node].label),
            Some((node, output)) => format!(
                "receives {}.{}",
                self.nodes[node].label, self.nodes[node].outputs[output]
            ),
            None => format!("runs on {}", self.trigger.as_str()),
        };

        let mut projects = h_flex().gap_1().flex_wrap();
        if self.projects.len() > 1 {
            for (i, project) in self.projects.iter().enumerate() {
                let button = Button::new(("new-hook-project", i))
                    .xsmall()
                    .label(if project == "." {
                        "root".to_string()
                    } else {
                        project.clone()
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(form) = this.new_hook.as_mut() {
                            form.project = i;
                        }
                        cx.notify();
                    }));
                projects = projects.child(if i == form.project {
                    button.primary()
                } else {
                    button.ghost()
                });
            }
        }

        Some(
            v_flex()
                .id("new-hook")
                .absolute()
                .left(x)
                .top(y)
                .w(px(310.))
                .p_3()
                .gap_2()
                .rounded_lg()
                .border_1()
                .border_color(color.opacity(0.6))
                .bg(theme.background)
                .shadow_lg()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_scroll(|_, _, cx| cx.stop_propagation())
                .child(
                    h_flex()
                        .gap_2()
                        .child(div().w(px(3.)).h(px(16.)).rounded(px(2.)).bg(color))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child("New hook"))
                        .child(
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .truncate()
                                .child(subtitle),
                        ),
                )
                .when(self.projects.len() > 1, |this| this.child(projects))
                .child(Input::new(&form.name).small())
                .child(Input::new(&form.cmd).small())
                .child(
                    div().text_xs().text_color(theme.muted_foreground).child(
                        "{files}: targeted files · $name: inputs · edit the rest in the list",
                    ),
                )
                .child(
                    h_flex()
                        .gap_2()
                        .justify_end()
                        .child(
                            Button::new("new-hook-cancel")
                                .ghost()
                                .small()
                                .label("Cancel")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.new_hook = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("new-hook-create")
                                .primary()
                                .small()
                                .label("Create")
                                .on_click(cx.listener(|this, _, _, cx| this.submit_new_hook(cx))),
                        ),
                )
                .into_any_element(),
        )
    }

    /// Legend of the wires and of the gestures
    fn render_help(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let line = |color: Hsla, style: &'static str, text: &'static str| {
            h_flex()
                .gap_2()
                .child(
                    div()
                        .w(px(28.))
                        .text_color(color)
                        .font_weight(FontWeight::BOLD)
                        .child(style),
                )
                .child(text)
        };
        v_flex()
            .id("canvas-help")
            .absolute()
            .bottom_3()
            .left_3()
            .p_3()
            .gap_1()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.background.opacity(0.95))
            .shadow_md()
            .text_xs()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(line(trigger_color(self.trigger, cx), "───", "a value: output → input ({name}, $name)"))
            .child(line(theme.muted_foreground, "- - -", "an order: runs after (needs)"))
            .child(line(theme.magenta, "·····", "a variable: written → read"))
            .child(div().h(px(4.)))
            .child("Double-click the background: new hook")
            .child("Drag from an output: to a node (input), its “waits for” row (order) or the background (new hook)")
            .child("Hover a node: its links, its delete button · Click its title: open it")
            .child("Drag the background to move · ⌘/Ctrl + scroll to zoom")
            .into_any_element()
    }
}

impl GraphCanvas {
    fn placeholder_text(&self) -> String {
        match self.error.clone() {
            Some(error) => error,
            None => format!(
                "No {} hooks yet · double-click to add one",
                self.trigger.as_str()
            ),
        }
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

    /// Name of a node seen from another one (with its project when it is elsewhere)
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
                Button::new("undo")
                    .ghost()
                    .small()
                    .icon(Icon::empty().path("icons/undo.svg"))
                    .tooltip("Undo (⌘/Ctrl+Z)")
                    .when(!self.can_undo, |b| {
                        gpui_kit::component::Disableable::disabled(b, true)
                    })
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(CanvasEvent::Undo))),
            )
            .child(
                Button::new("redo")
                    .ghost()
                    .small()
                    .icon(Icon::empty().path("icons/redo.svg"))
                    .tooltip("Redo (⌘/Ctrl+Shift+Z)")
                    .when(!self.can_redo, |b| {
                        gpui_kit::component::Disableable::disabled(b, true)
                    })
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(CanvasEvent::Redo))),
            )
            .child(
                Button::new("add-hook")
                    .ghost()
                    .small()
                    .icon(IconName::Plus)
                    .label("Hook")
                    .tooltip("New hook (or double-click the background)")
                    .on_click(cx.listener(|this, _, window, cx| {
                        let bounds = this.bounds.get();
                        let at = point(bounds.size.width / 2. - px(155.), px(90.));
                        this.open_new_hook(at, None, window, cx);
                    })),
            )
            .child(
                Button::new("help")
                    .ghost()
                    .small()
                    .icon(IconName::Info)
                    .tooltip("Legend and gestures")
                    .when(self.show_help, |b| {
                        gpui_kit::component::Selectable::selected(b, true)
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_help = !this.show_help;
                        cx.notify();
                    })),
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
            .key_context("GraphCanvas")
            .track_focus(&self.focus)
            .on_action(cx.listener(|_, _: &UndoGraph, _, cx| cx.emit(CanvasEvent::Undo)))
            .on_action(cx.listener(|_, _: &RedoGraph, _, cx| cx.emit(CanvasEvent::Redo)))
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
            // Grid of dots and wires, under the nodes
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

                        for wire in wires {
                            let (from, to) = (b.origin + wire.from, b.origin + wire.to);
                            let dx = px((f32::from(to.x - from.x).abs() / 2.).max(40.));
                            let width = px(f32::from(wire_width) * wire.emphasis.max(1.));
                            let mut path = PathBuilder::stroke(width);
                            if let Some([dash, gap]) = wire.dash {
                                path = path.dash_array(&[px(dash), px(gap)]);
                            }
                            path.move_to(from);
                            path.cubic_bezier_to(
                                to,
                                from + point(dx, px(0.)),
                                to - point(dx, px(0.)),
                            );
                            if let Ok(path) = path.build() {
                                window.paint_path(path, wire.color.opacity(wire.emphasis.min(1.)));
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
                        .child(self.placeholder_text()),
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
            .when(self.show_help, |this| this.child(self.render_help(cx)))
            .children(self.render_new_hook(cx))
    }
}
