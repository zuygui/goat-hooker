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
    matcher::{MatchedHook, Matcher},
    runner::{self, HookInput, HookResult, Output, Progress},
};

use crate::cli::ui::{self, KO, OK, PENDING};

/// Config explicite, sinon celle à la racine du dépôt (`None` si elle n'existe pas)
pub fn resolve_config(root: &Path, config_path: Option<PathBuf>) -> Option<PathBuf> {
    match config_path {
        Some(path) => Some(path),
        None => Some(root.join(HOOK_CONFIG_FILENAME)).filter(|p| p.exists()),
    }
}

/// Options de `hooker run`
pub struct RunFlags {
    pub dry_run: bool,
    pub verbose: bool,
    /// Lancé par Git : rien à afficher s'il n'y a pas de hooks
    pub from_git: bool,
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
    } = flags;
    // `HOOKER_SKIP=1 git commit` pour passer les hooks
    if std::env::var(git::SKIP_ENV).is_ok_and(|v| !v.is_empty() && v != "0") {
        return Ok(());
    }

    let root = ui::current_repo()?;

    // Si la config n'existe pas, on ne bloque pas Git
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

    // Git donne au pre-push les refs poussées sur stdin.
    // On le lit pour filtrer les hooks, puis on le redonne à chaque commande.
    let input = HookInput {
        args: args.to_vec(),
        stdin: read_stdin(trigger)?,
        force_color: std::io::stderr().is_terminal(),
    };

    if dry_run {
        let matcher = Matcher::new(&configs)?;
        let files = runner::changed_files(trigger, &root, &input)?;
        let hooks = matcher.hooks_to_run(trigger, files.as_deref());
        print_dry_run(trigger, &hooks, &root);
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
    reporter.finish(results.len());

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
        .filter(|r| !r.success)
        .map(|r| r.name.as_str())
        .collect();
    if !failed.is_empty() {
        return Err(GoatHookerError::HookFailed(failed.join("`, `")));
    }
    Ok(())
}

/// Lit stdin pour le pre-push (sauf si lancé à la main depuis un terminal)
fn read_stdin(trigger: Trigger) -> Result<Option<String>, GoatHookerError> {
    if trigger != Trigger::PrePush || std::io::stdin().is_terminal() {
        return Ok(None);
    }

    let mut content = String::new();
    std::io::stdin().read_to_string(&mut content)?;
    Ok(Some(content))
}

/// Une ligne de la liste des hooks : `✔ name   project   120ms`
fn hook_line(glyph: &str, name: &str, width: usize, project: &str, extra: &str) -> String {
    format!(
        "{glyph} {name:<width$}  {}  {}",
        project.dimmed(),
        extra.dimmed()
    )
}

/// Affiche l'avancement : un spinner par hook dans un terminal, une ligne par hook sinon
struct Reporter {
    trigger: Trigger,
    root: PathBuf,
    multi: MultiProgress,
    interactive: bool,
    state: Mutex<ReporterState>,
}

#[derive(Default)]
struct ReporterState {
    bars: Vec<ProgressBar>,
    labels: Vec<(String, String)>,
    width: usize,
    project_width: usize,
}

impl Reporter {
    fn new(trigger: Trigger, root: &Path) -> Self {
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

    fn on_progress(&self, progress: Progress) {
        let mut state = self.state.lock().unwrap();
        match progress {
            Progress::Planned(hooks) => self.plan(&mut state, hooks),
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
                let glyph = if result.success {
                    OK.green().to_string()
                } else {
                    KO.red().to_string()
                };
                let line = hook_line(
                    &glyph,
                    &name,
                    state.width,
                    &format!("{project:<w$}", w = state.project_width),
                    &ui::duration(result.duration),
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

    fn plan(&self, state: &mut ReporterState, hooks: &[MatchedHook]) {
        if hooks.is_empty() {
            return;
        }
        eprintln!(
            "{} {}",
            format!("🐐 {}", self.trigger.as_str()).bold(),
            format!("· {}", ui::plural(hooks.len(), "hook")).dimmed()
        );

        state.labels = hooks
            .iter()
            .map(|m| {
                (
                    m.hook.name.clone(),
                    ui::project_label(&self.root, &m.base_dir),
                )
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

    /// Les hooks qui n'ont pas tourné (séquentiel arrêté par un échec).
    /// Consomme le reporter : les spinners ne doivent plus redessiner par-dessus la suite.
    fn finish(self, ran: usize) {
        let state = self.state.into_inner().unwrap();
        for (index, (name, project)) in state.labels.iter().enumerate().skip(ran) {
            let line = hook_line(
                &"–".dimmed().to_string(),
                name,
                state.width,
                &format!("{project:<w$}", w = state.project_width),
                "skipped",
            );
            match state.bars.get(index) {
                Some(bar) => {
                    bar.set_style(ProgressStyle::with_template("{msg}").unwrap());
                    bar.finish_with_message(line);
                }
                None => eprintln!("{line}"),
            }
        }
        let had_bars = !state.bars.is_empty();
        drop(state.bars);
        drop(self.multi);
        // indicatif laisse le curseur au bout de la dernière ligne
        if had_bars && self.interactive {
            eprintln!();
        }
    }
}

/// Sortie des hooks en échec (de tous avec `--verbose`)
fn print_outputs(results: &[HookResult], root: &Path, verbose: bool) {
    for result in results.iter().filter(|r| verbose || !r.success) {
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

fn print_summary(results: &[HookResult], elapsed: Duration) {
    let failed = results.iter().filter(|r| !r.success).count();
    let passed = results.len() - failed;
    let time = format!("in {}", ui::duration(elapsed)).dimmed();

    eprintln!();
    if failed == 0 {
        eprintln!(
            "{} {} {time}",
            OK.green(),
            format!("{} passed", ui::plural(passed, "hook"))
                .green()
                .bold()
        );
    } else {
        eprintln!(
            "{} {}{} {time}",
            KO.red(),
            format!("{failed} failed").red().bold(),
            if passed > 0 {
                format!(", {passed} passed")
            } else {
                String::new()
            },
        );
        eprintln!(
            "  {} fix the errors above, or skip the hooks once with {}",
            "hint:".dimmed(),
            ui::command("HOOKER_SKIP=1 git ...")
        );
    }
}

fn print_dry_run(trigger: Trigger, hooks: &[MatchedHook], root: &Path) {
    if hooks.is_empty() {
        eprintln!(
            "{}",
            format!("🐐 No {} hooks to run", trigger.as_str()).dimmed()
        );
        return;
    }
    eprintln!(
        "{} {}",
        format!("🐐 {}", trigger.as_str()).bold(),
        format!("· {} would run", ui::plural(hooks.len(), "hook")).dimmed()
    );
    for matched in hooks {
        eprintln!();
        eprintln!(
            "{} {} {}",
            PENDING.dimmed(),
            matched.hook.name.bold(),
            format!("· {}", ui::project_label(root, &matched.base_dir)).dimmed()
        );
        for cmd in matched.hook.commands() {
            eprintln!("  {} {cmd}", "$".dimmed());
        }
        for file in &matched.files {
            eprintln!("  {} {}", "📄".dimmed(), file.display());
        }
    }
}
