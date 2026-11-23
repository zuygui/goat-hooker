use std::{
    io::{IsTerminal, Read},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

use colored::Colorize;
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressStyle};

use goat_hooker::{
    config::{ConfigLoader, HOOK_CONFIG_FILENAME, Trigger},
    errors::GoatHookerError,
    git,
    graph::{self, Graph},
    matcher::Matcher,
    runner::{self, HookInput, HookResult, Output, Progress},
};

use crate::cli::ui::{self, KO, OK, PENDING};

/// Explicit config, else the one at the root of the repository (`None` if it doesn't exist)
pub fn resolve_config(root: &Path, config_path: Option<PathBuf>) -> Option<PathBuf> {
    match config_path {
        Some(path) => Some(path),
        None => Some(root.join(HOOK_CONFIG_FILENAME)).filter(|p| p.exists()),
    }
}

/// Options of `hooker run`
pub struct RunFlags {
    pub dry_run: bool,
    pub verbose: bool,
    /// Run by Git: nothing to show when there are no hooks
    pub from_git: bool,
    pub no_cache: bool,
}

pub fn run_hooks(
    trigger: Trigger,
    config_path: Option<PathBuf>,
    args: &[String],
    flags: RunFlags,
) -> Result<(), GoatHookerError> {
    let RunFlags {
        dry_run,
        verbose,
        from_git,
        no_cache,
    } = flags;
    // `HOOKER_SKIP=1 git commit` to skip the hooks
    if std::env::var(git::SKIP_ENV).is_ok_and(|v| !v.is_empty() && v != "0") {
        return Ok(());
    }

    let root = ui::current_repo()?;

    // Without config, Git isn't blocked
    let Some(config_path) = resolve_config(&root, config_path) else {
        eprintln!(
            "{}",
            format!(
                "⚠️ No {HOOK_CONFIG_FILENAME} found at {}, skipping {} hooks",
                root.display(),
                trigger.as_str()
            )
            .yellow()
        );
        return Ok(());
    };

    let configs = ConfigLoader::new().load_recursive(&config_path)?;

    // Git gives the pushed refs to the pre-push on stdin.
    // It is read to filter the hooks, then given again to each command.
    let input = HookInput {
        args: args.to_vec(),
        stdin: read_stdin(trigger)?,
        force_color: std::io::stderr().is_terminal(),
        no_cache,
        ci_files: None,
    };

    if dry_run {
        let matcher = Matcher::new(&configs)?;
        let files = runner::changed_files(trigger, &root, &input)?;
        let hooks = matcher.hooks_to_run(trigger, files.as_deref());
        let graph = graph::build(&configs, trigger, &hooks)?;
        print_dry_run(trigger, &graph, &root);
        return Ok(());
    }

    let start = Instant::now();
    let reporter = Reporter::new(trigger, &root);
    let results = {
        let on_progress = |progress: Progress| reporter.on_progress(progress);
        runner::run_trigger(
            &root,
            &configs,
            trigger,
            &input,
            Output::Capture,
            Some(&on_progress),
        )?
    };
    reporter.finish();

    if results.is_empty() {
        if !from_git {
            eprintln!(
                "{}",
                format!("🐐 No {} hooks to run", trigger.as_str()).dimmed()
            );
        }
        return Ok(());
    }

    print_outputs(&results, &root, verbose);
    print_summary(&results, start.elapsed());

    let failed: Vec<&str> = results
        .iter()
        .filter(|r| r.failed())
        .map(|r| r.name.as_str())
        .collect();
    if !failed.is_empty() {
        return Err(GoatHookerError::HookFailed(failed.join("`, `")));
    }
    Ok(())
}

/// Reads stdin for the pre-push (unless run by hand from a terminal)
fn read_stdin(trigger: Trigger) -> Result<Option<String>, GoatHookerError> {
    if trigger != Trigger::PrePush || std::io::stdin().is_terminal() {
        return Ok(None);
    }

    let mut content = String::new();
    std::io::stdin().read_to_string(&mut content)?;
    Ok(Some(content))
}

/// A row of the list of the hooks: `✔ name   project   120ms`
fn hook_line(glyph: &str, name: &str, width: usize, project: &str, extra: &str) -> String {
    format!(
        "{glyph} {name:<width$}  {}  {}",
        project.dimmed(),
        extra.dimmed()
    )
}

/// Shows the progress: a spinner per hook in a terminal, a line per hook otherwise
pub(crate) struct Reporter {
    trigger: Trigger,
    root: PathBuf,
    multi: MultiProgress,
    interactive: bool,
    state: Mutex<ReporterState>,
}

#[derive(Default)]
pub(crate) struct ReporterState {
    bars: Vec<ProgressBar>,
    labels: Vec<(String, String)>,
    /// Nodes each node waits for (`← codegen`)
    needs: Vec<String>,
    width: usize,
    project_width: usize,
}

impl Reporter {
    pub(crate) fn new(trigger: Trigger, root: &Path) -> Self {
        let interactive = std::io::stderr().is_terminal();
        let multi = MultiProgress::with_draw_target(if interactive {
            ProgressDrawTarget::stderr()
        } else {
            ProgressDrawTarget::hidden()
        });
        Self {
            trigger,
            root: root.to_path_buf(),
            multi,
            interactive,
            state: Mutex::new(ReporterState::default()),
        }
    }

    pub(crate) fn on_progress(&self, progress: Progress) {
        let mut state = self.state.lock().unwrap();
        match progress {
            Progress::Planned(graph) => self.plan(&mut state, graph),
            Progress::Started(index) => {
                if let Some(bar) = state.bars.get(index) {
                    bar.set_style(
                        ProgressStyle::with_template("{spinner:.cyan} {msg}")
                            .unwrap()
                            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
                    );
                    bar.enable_steady_tick(Duration::from_millis(80));
                }
            }
            Progress::Finished(index, result) => {
                let (name, project) = state.labels[index].clone();
                let (glyph, extra) = if result.cached {
                    (
                        OK.green().dimmed().to_string(),
                        "cached".dimmed().to_string(),
                    )
                } else if result.skipped {
                    ("–".dimmed().to_string(), "skipped".to_string())
                } else if result.denied {
                    ("⛔".to_string(), "denied".red().to_string())
                } else if result.success {
                    (OK.green().to_string(), ui::duration(result.duration))
                } else {
                    (KO.red().to_string(), ui::duration(result.duration))
                };
                let extra = match state.needs[index].as_str() {
                    "" => extra,
                    needs => format!("{extra:<8} ← {needs}"),
                };
                let line = hook_line(
                    &glyph,
                    &name,
                    state.width,
                    &format!("{project:<w$}", w = state.project_width),
                    &extra,
                );
                match state.bars.get(index) {
                    Some(bar) if self.interactive => {
                        bar.set_style(ProgressStyle::with_template("{msg}").unwrap());
                        bar.finish_with_message(line);
                    }
                    _ => eprintln!("{line}"),
                }
            }
        }
    }

    fn plan(&self, state: &mut ReporterState, graph: &Graph) {
        if graph.nodes.is_empty() {
            return;
        }
        eprintln!(
            "{} {}",
            format!("🐐 {}", self.trigger.as_str()).bold(),
            format!("· {}", ui::plural(graph.nodes.len(), "hook")).dimmed()
        );

        state.labels = graph
            .nodes
            .iter()
            .map(|n| (n.label(), ui::project_label(&self.root, &n.base_dir)))
            .collect();
        state.needs = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(index, n)| {
                let mut needs: Vec<String> = n
                    .needs
                    .iter()
                    .map(|&need| label_from(graph, index, need))
                    .collect();
                if let Some(project) = n.affected_by.as_ref() {
                    needs.push(format!("{project} changed"));
                }
                needs.join(", ")
            })
            .collect();
        state.width = state
            .labels
            .iter()
            .map(|(n, _)| n.chars().count())
            .max()
            .unwrap_or(0);
        state.project_width = state
            .labels
            .iter()
            .map(|(_, p)| p.chars().count())
            .max()
            .unwrap_or(0);

        if !self.interactive {
            return;
        }
        for (name, project) in &state.labels {
            let bar = self.multi.add(ProgressBar::new_spinner());
            bar.set_style(ProgressStyle::with_template("{prefix:.dim} {msg}").unwrap());
            bar.set_prefix(PENDING);
            bar.set_message(format!(
                "{name:<w$}  {}",
                format!("{project:<pw$}", pw = state.project_width).dimmed(),
                w = state.width
            ));
            bar.tick();
            state.bars.push(bar);
        }
    }

    /// Consumes the reporter: the spinners must not draw over what follows anymore.
    pub(crate) fn finish(self) {
        let state = self.state.into_inner().unwrap();
        let had_bars = !state.bars.is_empty();
        drop(state.bars);
        drop(self.multi);
        // indicatif leaves the cursor at the end of the last line
        if had_bars && self.interactive {
            eprintln!();
        }
    }
}

/// Output of the failed hooks (of all of them with `--verbose`)
pub(crate) fn print_outputs(results: &[HookResult], root: &Path, verbose: bool) {
    for result in results
        .iter()
        .filter(|r| !r.skipped && (verbose || !r.success))
    {
        let glyph = if result.success { OK.green() } else { KO.red() };
        eprintln!();
        eprintln!(
            "{} {} {}",
            glyph,
            result.name.bold(),
            format!("· {}", ui::project_label(root, &result.base_dir)).dimmed()
        );
        for cmd in &result.commands {
            eprintln!("  {} {}", "$".dimmed(), cmd.cmd.dimmed());
            for line in cmd.output.lines() {
                eprintln!("  {} {line}", "│".dimmed());
            }
        }
    }
}

pub(crate) fn print_summary(results: &[HookResult], elapsed: Duration) {
    let denied = results.iter().filter(|r| r.denied).count();
    let failed = results.iter().filter(|r| r.failed()).count() - denied;
    let skipped = results.iter().filter(|r| r.skipped).count();
    let passed = results.len() - failed - skipped - denied;
    let time = format!("in {}", ui::duration(elapsed)).dimmed();

    eprintln!();
    if failed == 0 && denied == 0 {
        eprintln!(
            "{} {} {time}",
            OK.green(),
            format!("{} passed", ui::plural(passed, "hook"))
                .green()
                .bold()
        );
    } else {
        let mut counts = Vec::new();
        if failed > 0 {
            counts.push(format!("{failed} failed").red().bold().to_string());
        }
        if denied > 0 {
            counts.push(format!("{denied} denied").red().bold().to_string());
        }
        eprintln!(
            "{} {}{}{} {time}",
            KO.red(),
            counts.join(", "),
            if skipped > 0 {
                format!(", {skipped} skipped")
            } else {
                String::new()
            },
            if passed > 0 {
                format!(", {passed} passed")
            } else {
                String::new()
            },
        );
        if results
            .iter()
            .any(|r| r.denied && r.stdout().contains("hooker trust"))
        {
            eprintln!(
                "  {} review the hooks of other repositories with {}",
                "hint:".dimmed(),
                ui::command("hooker trust")
            );
        } else {
            eprintln!(
                "  {} fix the errors above, or skip the hooks once with {}",
                "hint:".dimmed(),
                ui::command("HOOKER_SKIP=1 git ...")
            );
        }
    }
}

/// Name of a node seen from another one: with its project when it is elsewhere (`api:codegen`)
fn label_from(graph: &Graph, from: usize, node: usize) -> String {
    let target = &graph.nodes[node];
    if target.project == graph.nodes[from].project {
        target.label()
    } else {
        format!("{}:{}", target.project, target.label())
    }
}

fn print_dry_run(trigger: Trigger, graph: &Graph, root: &Path) {
    if graph.nodes.is_empty() {
        eprintln!(
            "{}",
            format!("🐐 No {} hooks to run", trigger.as_str()).dimmed()
        );
        return;
    }
    eprintln!(
        "{} {}",
        format!("🐐 {}", trigger.as_str()).bold(),
        format!(
            "· {} would run, in this order",
            ui::plural(graph.nodes.len(), "hook")
        )
        .dimmed()
    );
    for (index, node) in graph.nodes.iter().enumerate() {
        eprintln!();
        eprintln!(
            "{} {} {}",
            PENDING.dimmed(),
            node.label().bold(),
            format!("· {}", ui::project_label(root, &node.base_dir)).dimmed()
        );
        let needs: Vec<String> = node
            .needs
            .iter()
            .filter(|need| !node.inputs.iter().any(|i| i.node == **need))
            .map(|&need| label_from(graph, index, need))
            .collect();
        if !needs.is_empty() {
            eprintln!("  {} waits for {}", "←".dimmed(), needs.join(", "));
        }
        if let Some(project) = node.affected_by.as_ref() {
            eprintln!(
                "  {} runs because {} changed (it depends on it)",
                "←".dimmed(),
                project.cyan()
            );
        }
        for input in &node.inputs {
            eprintln!(
                "  {} {} = {}.{}",
                "←".dimmed(),
                format!("{{{}}}", input.name).cyan(),
                label_from(graph, index, input.node),
                input.output
            );
        }
        for cmd in node.commands() {
            eprintln!("  {} {cmd}", "$".dimmed());
        }
        for file in &node.files {
            eprintln!("  {} {}", "📄".dimmed(), file.display());
        }
    }
}
