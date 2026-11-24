use std::{
    collections::{BTreeMap, VecDeque},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

use colored::Colorize;

use crate::{
    config::{Config, FileAccess, Trigger},
    errors::GoatHookerError,
    git::{self, UnstagedStash},
    graph::{self, Graph, Node},
    matcher::{MatchedHook, Matcher},
};

/// Replaced in the commands with the targeted files (escaped for the shell)
pub const FILES_PLACEHOLDER: &str = "{files}";

/// Values of the variables during a run, shared by all the hooks
#[derive(Debug, Default)]
pub struct Shared {
    values: Mutex<BTreeMap<String, String>>,
    changed: AtomicBool,
    /// Results of the hooks that passed (`None`: no cache)
    pub cache: Option<crate::cache::Cache>,
    /// Current branch, for `branches` and `skip_branches` (`None`: detached HEAD)
    pub branch: Option<String>,
}

impl Shared {
    /// Values stored for the repository (or the defaults)
    pub fn load(root: &Path, configs: &[Config]) -> Result<Self, GoatHookerError> {
        let declared = crate::variables::declared(configs)?;
        Ok(Self {
            values: Mutex::new(crate::variables::load(root, &declared)),
            changed: AtomicBool::new(false),
            cache: None,
            branch: git::current_branch(root),
        })
    }

    /// Saves the values if a hook wrote one
    pub fn save(&self, root: &Path) -> Result<(), GoatHookerError> {
        if self.changed.load(Ordering::Relaxed) {
            crate::variables::save(root, &self.values.lock().unwrap())?;
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<String> {
        self.values.lock().unwrap().get(name).cloned()
    }

    fn set(&self, name: &str, value: String) {
        self.values.lock().unwrap().insert(name.to_string(), value);
        self.changed.store(true, Ordering::Relaxed);
    }
}

/// Receives the output of a command while it runs
pub type Live<'a> = &'a (dyn Fn(&str) + Sync);

/// Environment and restrictions of a command
struct Sandbox<'a> {
    /// More environment variables (`$HOOKER_VARS`, variables read by the hook)
    extra_env: Vec<(String, String)>,
    /// `Some`: only these environment variables besides the basic ones
    env: Option<Vec<String>>,
    network: bool,
    /// The command is stopped and fails at this time (`timeout` of the hook)
    deadline: Option<Instant>,
    /// Output of the command while it runs (captured output only)
    live: Option<Live<'a>>,
}

impl Default for Sandbox<'_> {
    fn default() -> Self {
        Self {
            extra_env: vec![],
            env: None,
            network: true,
            deadline: None,
            live: None,
        }
    }
}

/// Environment variables every hook gets, even with `permissions.env`
const BASIC_ENV: [&str; 22] = [
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "TERM",
    "COLORTERM",
    "NO_COLOR",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "TMPDIR",
    "TMP",
    "TEMP",
    "SYSTEMROOT",
    "WINDIR",
    "COMSPEC",
    "PATHEXT",
    "USERPROFILE",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
];

/// What Git gives to the hook
#[derive(Debug, Clone, Default)]
pub struct HookInput {
    /// Arguments of the hook (`$1`, `$2`...)
    pub args: Vec<String>,
    /// stdin (refs of the pre-push)
    pub stdin: Option<String>,
    /// Keeps the colors of the tools even when their output is captured
    pub force_color: bool,
    /// Runs every hook, even the ones that passed with the same files
    pub no_cache: bool,
    /// CI: the hooks run on these files (changed since the base), nothing is put aside or
    /// staged, and a hook modifying files fails (they should have been fixed before the commit)
    pub ci_files: Option<Vec<PathBuf>>,
    /// Hooks not to run (`HOOKER_SKIP=lint,test`): names, `project:name` or `hook/step`
    pub skip: Vec<String>,
}

/// Progress of a run (to show spinners)
pub enum Progress<'a> {
    /// Nodes that will run, in the order of the graph
    Planned(&'a Graph<'a>),
    /// The node `index` starts
    Started(usize),
    /// The node `index` is done (or skipped, if a dependency failed)
    Finished(usize, &'a HookResult),
    /// Output of the node `index` while it runs (captured output only)
    Output(usize, &'a str),
}

/// Value received from another node
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Text(String),
    /// A list of files (output `files`)
    List(Vec<String>),
}

/// Receives the progress (can be called from several threads)
pub type OnProgress<'a> = &'a (dyn Fn(Progress<'_>) + Sync);

/// Where the output of the commands goes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// Straight to the terminal, with the progress messages (CLI)
    Inherit,
    /// Captured and returned in the result, without showing anything (parallel, GUI)
    Capture,
}

#[derive(Debug, Clone)]
pub struct CommandResult {
    pub cmd: String,
    pub success: bool,
    /// Output (stdout + stderr) in `Capture` mode
    pub output: String,
}

#[derive(Debug, Clone, Default)]
pub struct HookResult {
    /// Name of the hook (`hook › step` for a step)
    pub name: String,
    /// Folder of the project of the hook
    pub base_dir: PathBuf,
    pub success: bool,
    /// Not run: a node it waits for failed, or skipped on purpose (see `note`)
    pub skipped: bool,
    /// Why it was skipped on purpose (`HOOKER_SKIP`, branch)
    pub note: Option<String>,
    /// Not allowed: not run (not trusted, forbidden variable) or stopped for doing what it
    /// may not do. The nodes after it (needs and variables) are skipped
    pub denied: bool,
    /// Not run: it already passed with the same files (its output is replayed)
    pub cached: bool,
    pub duration: Duration,
    pub commands: Vec<CommandResult>,
    /// Targeted files, relative to `base_dir` (output `files`)
    pub files: Vec<PathBuf>,
    /// Outputs written to `$HOOKER_OUTPUT`
    pub outputs: BTreeMap<String, String>,
    /// Files to stage again (`stage_fixed`), as absolute paths
    pub files_to_stage: Vec<PathBuf>,
}

impl HookResult {
    pub fn failed(&self) -> bool {
        !self.success && !self.skipped
    }

    /// Output `stdout`: the output of all the commands
    pub fn stdout(&self) -> String {
        self.commands
            .iter()
            .map(|c| c.output.as_str())
            .collect::<String>()
            .trim_end()
            .to_string()
    }
}

/// Files to filter the hooks with:
/// staged files for the pre-commit, files of the pushed commits for the pre-push.
/// `None` → the other triggers run all their hooks.
pub fn changed_files(
    trigger: Trigger,
    root: &Path,
    input: &HookInput,
) -> Result<Option<Vec<PathBuf>>, GoatHookerError> {
    match (trigger, input.stdin.as_deref()) {
        (Trigger::PreCommit, _) => Ok(Some(git::staged_files(root)?)),
        (Trigger::PrePush, Some(refs)) => {
            let remote = input.args.first().map(String::as_str).unwrap_or_default();
            Ok(Some(git::pushed_files(root, remote, refs)?))
        }
        _ => Ok(None),
    }
}

/// Runs hooks without links between them (see `run_graph`)
pub fn run_all(
    hooks: &[MatchedHook],
    input: &HookInput,
    parallel: bool,
    output: Output,
    on_progress: Option<OnProgress>,
) -> Result<Vec<HookResult>, GoatHookerError> {
    let graph = Graph {
        nodes: hooks.iter().map(standalone_node).collect(),
    };
    run_graph(
        &graph,
        input,
        parallel,
        output,
        on_progress,
        &Shared::default(),
    )
}

/// Node without links for a single hook
fn standalone_node<'a>(matched: &MatchedHook<'a>) -> Node<'a> {
    Node {
        hook: matched.hook,
        step: None,
        project: String::new(),
        base_dir: matched.base_dir.clone(),
        files: matched.files.clone(),
        needs: vec![],
        inputs: vec![],
        outputs: vec![],
        reads: matched.hook.reads.clone().unwrap_or_default(),
        writes: matched.hook.writes.clone().unwrap_or_default(),
        after: vec![],
        denied: None,
        affected_by: None,
    }
}

/// Runs a graph: each node starts when the ones it waits for passed,
/// with their outputs as inputs. If a node it waits for fails, the node is skipped.
/// In parallel, all the ready nodes run at the same time; otherwise one by one.
/// In parallel with `Output::Inherit`, each node is shown when it is done.
pub fn run_graph(
    graph: &Graph,
    input: &HookInput,
    parallel: bool,
    output: Output,
    on_progress: Option<OnProgress>,
    shared: &Shared,
) -> Result<Vec<HookResult>, GoatHookerError> {
    let notify = |progress: Progress| {
        if let Some(on_progress) = on_progress {
            on_progress(progress);
        }
    };
    notify(Progress::Planned(graph));

    let count = graph.nodes.len();
    let mut results: Vec<Option<HookResult>> = (0..count).map(|_| None).collect();
    let mut remaining: Vec<usize> = graph
        .nodes
        .iter()
        .map(|n| n.needs.len() + n.after.len())
        .collect();
    let mut dependents = vec![Vec::new(); count];
    for (index, node) in graph.nodes.iter().enumerate() {
        for &need in node.needs.iter().chain(&node.after) {
            dependents[need].push(index);
        }
    }
    let mut ready: VecDeque<usize> = (0..count).filter(|&i| remaining[i] == 0).collect();
    let mut cut = vec![false; count];

    // The nodes waiting for `index` may be ready
    let release = |index: usize, remaining: &mut Vec<usize>, ready: &mut VecDeque<usize>| {
        for &dependent in &dependents[index] {
            remaining[dependent] -= 1;
            if remaining[dependent] == 0 {
                ready.push_back(dependent);
            }
        }
    };

    // In parallel, the outputs are captured then shown node by node
    let node_output = if parallel { Output::Capture } else { output };
    let print_lock = Mutex::new(());

    std::thread::scope(|scope| -> Result<(), GoatHookerError> {
        let (sender, receiver) = mpsc::channel();
        let mut running = 0;

        loop {
            while running == 0 || parallel {
                let Some(index) = ready.pop_front() else {
                    break;
                };
                let node = &graph.nodes[index];

                // A denied node cuts its whole lineage, through `needs` and variables
                let cut_above = node.needs.iter().chain(&node.after).any(|&n| cut[n]);
                if cut_above
                    || node
                        .needs
                        .iter()
                        .any(|&need| results[need].as_ref().is_some_and(|r| !r.success))
                {
                    let result = HookResult {
                        name: node.label(),
                        base_dir: node.base_dir.clone(),
                        skipped: true,
                        files: node.files.clone(),
                        ..Default::default()
                    };
                    cut[index] = cut_above;
                    notify(Progress::Finished(index, &result));
                    results[index] = Some(result);
                    release(index, &mut remaining, &mut ready);
                    continue;
                }
                if let Some(note) = skip_note(node, input, shared) {
                    // Skipped on purpose: what waits for it still runs
                    let result = HookResult {
                        name: node.label(),
                        base_dir: node.base_dir.clone(),
                        success: true,
                        skipped: true,
                        note: Some(note),
                        files: node.files.clone(),
                        ..Default::default()
                    };
                    notify(Progress::Finished(index, &result));
                    results[index] = Some(result);
                    release(index, &mut remaining, &mut ready);
                    continue;
                }
                if let Some(reason) = node.denied.as_ref() {
                    let result = HookResult {
                        name: node.label(),
                        base_dir: node.base_dir.clone(),
                        denied: true,
                        files: node.files.clone(),
                        commands: vec![CommandResult {
                            cmd: "permissions".to_string(),
                            success: false,
                            output: format!("{reason}\n"),
                        }],
                        ..Default::default()
                    };
                    cut[index] = true;
                    notify(Progress::Finished(index, &result));
                    results[index] = Some(result);
                    release(index, &mut remaining, &mut ready);
                    continue;
                }

                let values = input_values(node, &results, graph);
                notify(Progress::Started(index));
                running += 1;
                let sender = sender.clone();
                scope.spawn(move || {
                    let live = move |text: &str| {
                        if let Some(on_progress) = on_progress {
                            on_progress(Progress::Output(index, text));
                        }
                    };
                    let live: Option<Live> =
                        (on_progress.is_some() && node_output == Output::Capture).then_some(&live);
                    let result = run_node(node, input, &values, node_output, shared, live);
                    let _ = sender.send((index, result));
                });
            }

            if running == 0 {
                break;
            }
            let (index, result) = receiver.recv().expect("hook thread stopped");
            running -= 1;
            let result = result?;
            cut[index] = result.denied;

            if parallel && output == Output::Inherit {
                let _guard = print_lock.lock().unwrap();
                print_result(&result);
            }
            notify(Progress::Finished(index, &result));
            results[index] = Some(result);
            release(index, &mut remaining, &mut ready);
        }
        Ok(())
    })?;

    Ok(results.into_iter().flatten().collect())
}

/// Why a node is skipped on purpose: named in `HOOKER_SKIP`, or not for this branch
fn skip_note(node: &Node, input: &HookInput, shared: &Shared) -> Option<String> {
    let hook = &node.hook.name;
    let names = [
        hook.clone(),
        node.label(),
        node.reference(),
        format!("{}:{hook}", node.project),
    ];
    if input
        .skip
        .iter()
        .any(|skip| names.iter().any(|name| name == skip.trim()))
    {
        return Some("HOOKER_SKIP".to_string());
    }
    if !node.hook.runs_on_branch(shared.branch.as_deref()) {
        return Some(match shared.branch.as_deref() {
            Some(branch) => format!("not on {branch}"),
            None => "detached HEAD".to_string(),
        });
    }
    None
}

/// Values of the inputs of a node, from the results of the nodes it waits for
fn input_values(
    node: &Node,
    results: &[Option<HookResult>],
    graph: &Graph,
) -> Vec<(String, Value)> {
    node.inputs
        .iter()
        .map(|input| {
            let source = results[input.node].as_ref();
            let value = match input.output.as_str() {
                "files" => {
                    let from = &graph.nodes[input.node];
                    Value::List(
                        from.files
                            .iter()
                            .map(|f| {
                                relative_to(&node.base_dir, &from.base_dir.join(f))
                                    .to_string_lossy()
                                    .to_string()
                            })
                            .collect(),
                    )
                }
                "stdout" => Value::Text(source.map(|r| r.stdout()).unwrap_or_default()),
                "status" => Value::Text(
                    match source {
                        Some(r) if r.success => "success",
                        _ => "failure",
                    }
                    .to_string(),
                ),
                output => Value::Text(
                    source
                        .and_then(|r| r.outputs.get(output).cloned())
                        .unwrap_or_default(),
                ),
            };
            (input.name.clone(), value)
        })
        .collect()
}

/// Path of `path` seen from `dir` (`../api/src/lib.rs`)
fn relative_to(dir: &Path, path: &Path) -> PathBuf {
    let dir: Vec<Component> = dir.components().collect();
    let path_components: Vec<Component> = path.components().collect();
    let common = dir
        .iter()
        .zip(&path_components)
        .take_while(|(a, b)| a == b)
        .count();
    if common == 0 {
        return path.to_path_buf();
    }
    let mut relative = PathBuf::new();
    for _ in common..dir.len() {
        relative.push("..");
    }
    for component in &path_components[common..] {
        relative.push(component);
    }
    relative
}

/// Runs all the hooks of a trigger, like Git would:
/// filters on the files, puts aside the unstaged changes (pre-commit),
/// stages the fixed files again (`stage_fixed`).
pub fn run_trigger(
    root: &Path,
    configs: &[Config],
    trigger: Trigger,
    input: &HookInput,
    output: Output,
    on_progress: Option<OnProgress>,
) -> Result<Vec<HookResult>, GoatHookerError> {
    let parallel = configs.first().and_then(|c| c.parallel).unwrap_or(false);
    let matcher = Matcher::new(configs)?;

    let files = match input.ci_files.as_ref() {
        Some(files) => Some(files.clone()),
        None => changed_files(trigger, root, input)?,
    };
    let hooks = matcher.hooks_to_run(trigger, files.as_deref());
    if hooks.is_empty() {
        return Ok(vec![]);
    }

    // The pre-commit hooks must see exactly what will be committed
    let mut stash = if trigger == Trigger::PreCommit && input.ci_files.is_none() {
        UnstagedStash::save(root)?
    } else {
        None
    };

    let graph = graph::build(configs, trigger, &hooks)?;
    let start = Instant::now();
    let mut shared = Shared::load(root, configs)?;
    if !input.no_cache {
        shared.cache = Some(crate::cache::Cache::load(root)?);
    }
    let results = run_graph(&graph, input, parallel, output, on_progress, &shared)?;
    if let Some(cache) = shared.cache.as_ref() {
        cache.save()?;
    }
    if input.ci_files.is_none() {
        // The history is a bonus: it never stops a commit
        let _ = crate::history::record(root, trigger, &results, start.elapsed());
    }
    // The values are kept only if the run passed (the commit or push happens)
    if results.iter().all(|r| r.success) {
        shared.save(root)?;
    }

    // Stages the fixed files again (formatters) before putting the unstaged changes back
    let files_to_stage: Vec<PathBuf> = results
        .iter()
        .filter(|r| r.success)
        .flat_map(|r| r.files_to_stage.clone())
        .collect();
    if input.ci_files.is_none() {
        git::add(root, &files_to_stage)?;
    }

    if let Some(stash) = stash.as_mut() {
        stash.restore()?;
    }

    Ok(results)
}

/// Shows the result of a hook run in `Capture` mode
pub fn print_result(result: &HookResult) {
    println!("{}", format!("🐐 Hook `{}`", result.name).bold());
    for cmd in &result.commands {
        println!("🚧 Executing command: `{}`", cmd.cmd);
        print!("{}", cmd.output);
        if cmd.success {
            println!("{}", "✅ Command executed successfully".green());
        } else {
            eprintln!("{}", "❌ Command failed".red());
        }
    }
}

/// Runs the commands of a single hook (without its dependencies) from the folder of its project
pub fn run_hook(
    matched: &MatchedHook,
    input: &HookInput,
    output: Output,
    shared: &Shared,
) -> Result<HookResult, GoatHookerError> {
    run_hook_live(matched, input, output, shared, None)
}

/// Like `run_hook`, with the output of the commands while they run (captured output only)
pub fn run_hook_live(
    matched: &MatchedHook,
    input: &HookInput,
    output: Output,
    shared: &Shared,
    live: Option<Live>,
) -> Result<HookResult, GoatHookerError> {
    run_node(&standalone_node(matched), input, &[], output, shared, live)
}

/// Runs the commands of a node, with the values of its inputs
fn run_node(
    node: &Node,
    input: &HookInput,
    values: &[(String, Value)],
    output: Output,
    shared: &Shared,
    live: Option<Live>,
) -> Result<HookResult, GoatHookerError> {
    if output == Output::Inherit {
        println!("{}", format!("🐐 Running hook `{}`", node.label()).bold());
    }

    // The outputs of the node are written to this file (`echo "name=value" >> $HOOKER_OUTPUT`)
    let output_file = std::env::temp_dir().join(format!(
        "goat-hooker-{}-{}.out",
        std::process::id(),
        OUTPUT_FILES.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&output_file, "")?;
    // Variables written by the node (`echo "name=value" >> $HOOKER_VARS`)
    let vars_file = output_file.with_extension("vars");
    std::fs::write(&vars_file, "")?;

    let granted = node.hook.granted();
    let mut sandbox = Sandbox {
        extra_env: vec![(
            "HOOKER_VARS".to_string(),
            vars_file.to_string_lossy().to_string(),
        )],
        env: granted.env.clone(),
        network: granted.network,
        deadline: node.hook.timeout().map(|t| Instant::now() + t),
        live,
    };
    let read_variables: Vec<(String, String)> = node
        .reads
        .iter()
        .map(|name| (name.clone(), shared.get(name).unwrap_or_default()))
        .collect();
    sandbox.extra_env.extend(read_variables.iter().cloned());

    // Already passed with the same files, commands, inputs and variables: replayed
    let inputs_text: Vec<(String, String)> = values
        .iter()
        .map(|(name, value)| {
            let text = match value {
                Value::Text(text) => text.clone(),
                Value::List(items) => items.join("\n"),
            };
            (name.clone(), text)
        })
        .collect();
    let cache_key = |shared: &Shared| -> Option<String> {
        shared.cache.as_ref()?;
        if !crate::cache::cacheable(node) {
            return None;
        }
        crate::cache::key(node, &inputs_text, &read_variables, &input.args).ok()
    };
    let key_before = cache_key(shared);
    if let (Some(cache), Some(key)) = (shared.cache.as_ref(), key_before.as_ref())
        && let Some(entry) = cache.get(key)
    {
        let _ = std::fs::remove_file(&output_file);
        let _ = std::fs::remove_file(&vars_file);
        for (name, value) in &entry.variables {
            shared.set(name, value.clone());
        }
        return Ok(HookResult {
            name: node.label(),
            base_dir: node.base_dir.clone(),
            success: true,
            cached: true,
            files: node.files.clone(),
            outputs: entry.outputs.clone(),
            commands: vec![CommandResult {
                cmd: "cached".to_string(),
                success: true,
                output: entry.stdout.clone(),
            }],
            ..Default::default()
        });
    }

    // Read-only files: kept to check (and revert) them after the hook. In CI, every hook:
    // a formatter changing files means they weren't fixed before the commit
    let ci = input.ci_files.is_some();
    let protected: Vec<(PathBuf, Vec<u8>)> = if granted.files == FileAccess::Read || ci {
        node.files
            .iter()
            .map(|f| node.base_dir.join(f))
            .filter_map(|f| std::fs::read(&f).ok().map(|content| (f, content)))
            .collect()
    } else {
        vec![]
    };

    let start = Instant::now();
    let mut result = HookResult {
        name: node.label(),
        base_dir: node.base_dir.clone(),
        success: true,
        files: node.files.clone(),
        ..Default::default()
    };

    for cmd in node.commands() {
        let cmd_result = execute_in(
            cmd,
            &node.base_dir,
            input,
            &node.files,
            values,
            Some(&output_file),
            output,
            &sandbox,
        );
        let cmd_result = match cmd_result {
            Ok(r) => r,
            Err(err) => {
                let _ = std::fs::remove_file(&output_file);
                let _ = std::fs::remove_file(&vars_file);
                return Err(err);
            }
        };
        let failed = !cmd_result.success;
        result.commands.push(cmd_result);
        if failed {
            result.success = false;
            break;
        }
    }
    result.duration = start.elapsed();
    result.outputs = parse_outputs(&std::fs::read_to_string(&output_file).unwrap_or_default());
    let written = parse_outputs(&std::fs::read_to_string(&vars_file).unwrap_or_default());
    let _ = std::fs::remove_file(&output_file);
    let _ = std::fs::remove_file(&vars_file);

    // Permissions: the hook fails when it did what it isn't allowed to
    let mut denied: Vec<String> = written
        .keys()
        .filter(|name| !node.writes.contains(name))
        .map(|name| {
            format!("wrote the variable `{name}` without declaring it in `writes`, it was ignored")
        })
        .collect();
    for (file, content) in &protected {
        if std::fs::read(file).ok().as_ref() != Some(content) {
            let _ = std::fs::write(file, content);
            let relative = file.strip_prefix(&node.base_dir).unwrap_or(file).display();
            if ci && granted.files == FileAccess::Write {
                // Not a permission issue: the committed file isn't fixed
                result.success = false;
                result.commands.push(CommandResult {
                    cmd: "ci".to_string(),
                    success: false,
                    output: format!("{relative} isn't fixed: run the hooks before committing\n"),
                });
            } else {
                denied.push(format!(
                    "modified {relative} without the permission to modify files, the change was reverted"
                ));
            }
        }
    }
    // Its variables are kept only if it passed and did nothing it may not do
    if result.success && denied.is_empty() {
        // Cached with the files as they are after it (a formatter changed them: the next
        // run on the fixed files is skipped, a run on unformatted files isn't)
        if let (Some(cache), Some(_)) = (shared.cache.as_ref(), key_before)
            && let Some(after) = cache_key(shared)
        {
            cache.put(
                after,
                crate::cache::Entry {
                    at: 0,
                    outputs: result.outputs.clone(),
                    variables: written.clone(),
                    stdout: result.stdout(),
                },
            );
        }
        for (name, value) in written {
            shared.set(&name, value);
        }
    }
    if !denied.is_empty() {
        result.success = false;
        result.denied = true;
        result.commands.push(CommandResult {
            cmd: "permissions".to_string(),
            success: false,
            output: denied.join("\n") + "\n",
        });
    }

    if result.success && node.hook.stage_fixed.unwrap_or(false) {
        result.files_to_stage = node
            .files
            .iter()
            .map(|f| node.base_dir.join(f))
            .filter(|f| f.exists())
            .collect();
    }

    Ok(result)
}

/// Number of the output files (one per node run)
static OUTPUT_FILES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Reads the `name=value` lines of `$HOOKER_OUTPUT` (the last value wins)
fn parse_outputs(content: &str) -> BTreeMap<String, String> {
    content
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(name, value)| (name.trim().to_string(), value.to_string()))
        .filter(|(name, _)| !name.is_empty())
        .collect()
}

/// `sh`, without network when it isn't allowed and the system can block it:
/// `unshare` (Linux user namespaces) or `sandbox-exec` (macOS).
/// The note says when it couldn't be blocked.
fn sandboxed_shell(network: bool) -> (Command, Option<&'static str>) {
    if network {
        return (Command::new(shell()), None);
    }
    static UNSHARE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if cfg!(target_os = "linux")
        && *UNSHARE.get_or_init(|| {
            Command::new("unshare")
                .args(["--user", "--map-root-user", "--net", "true"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        })
    {
        let mut command = Command::new("unshare");
        command.args(["--user", "--map-root-user", "--net", "sh"]);
        return (command, None);
    }
    if cfg!(target_os = "macos") && Path::new("/usr/bin/sandbox-exec").exists() {
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command.args([
            "-p",
            "(version 1)(allow default)(deny network-outbound (remote ip))",
            "sh",
        ]);
        return (command, None);
    }
    (
        Command::new(shell()),
        Some("goat-hooker: the network can't be blocked on this system\n"),
    )
}

/// The shell running the commands: `sh`, or on Windows the one of Git for Windows
/// (found next to git when it isn't in the PATH)
pub fn shell() -> PathBuf {
    static SHELL: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    SHELL
        .get_or_init(|| {
            if !cfg!(windows) {
                return PathBuf::from("sh");
            }
            let in_path = Command::new("sh")
                .args(["-c", "exit 0"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if in_path {
                return PathBuf::from("sh");
            }
            // git --exec-path: <Git>/mingw64/libexec/git-core → <Git>/bin/sh.exe
            let exec_path = Command::new("git")
                .arg("--exec-path")
                .output()
                .ok()
                .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim()));
            exec_path
                .iter()
                .flat_map(|p| p.ancestors().skip(1).take(4))
                .flat_map(|dir| [dir.join("bin/sh.exe"), dir.join("usr/bin/sh.exe")])
                .find(|sh| sh.is_file())
                .unwrap_or_else(|| PathBuf::from("sh"))
        })
        .clone()
}

/// Escapes a path for the shell (`it's.rs` → `'it'\''s.rs'`)
fn shell_quote(path: &Path) -> String {
    let path = path.to_string_lossy();
    // The shell of Git for Windows prefers `/`
    let path = if cfg!(windows) {
        path.replace('\\', "/")
    } else {
        path.to_string()
    };
    format!("'{}'", path.replace('\'', r"'\''"))
}

/// Runs a shell command from `dir`.
/// - the arguments of Git are given as positional parameters (`$1`, `$2`...)
/// - `{files}` is replaced with the targeted files, also in `$HOOKER_FILES` (one per line)
/// - stdin is given again to the command (refs of the pre-push)
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
fn execute_command(
    cmd: &str,
    dir: &Path,
    input: &HookInput,
    files: &[PathBuf],
    values: &[(String, Value)],
    output_file: Option<&Path>,
    output: Output,
) -> Result<CommandResult, GoatHookerError> {
    execute_in(
        cmd,
        dir,
        input,
        files,
        values,
        output_file,
        output,
        &Sandbox::default(),
    )
}

/// Stops a command and what it started (its process group on unix)
fn kill_tree(pid: u32) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-s", "KILL", "--", &format!("-{pid}")])
            .status();
        let _ = Command::new("kill")
            .args(["-s", "KILL", "--", &pid.to_string()])
            .status();
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .status();
    }
}

/// What the threads watching a command send
enum Event {
    Output(Vec<u8>),
    Exit(std::io::Result<std::process::ExitStatus>),
}

/// A started command
struct Prepared {
    child: std::process::Child,
    /// stdout and stderr together, for a command started without a shell
    merged: Option<std::io::PipeReader>,
    network_note: Option<&'static str>,
}

/// Builtins of the shell that aren't programs: commands starting with them need `sh`
const SHELL_ONLY: [&str; 22] = [
    ".", ":", "alias", "bg", "cd", "command", "eval", "exec", "exit", "export", "fg", "getopts",
    "hash", "jobs", "read", "readonly", "return", "set", "shift", "source", "trap", "unset",
];

/// Arguments of a command that doesn't need a shell: words made of plain characters, and
/// `{files}` / `{name}` placeholders as whole words (one argument per file).
/// `None` when the shell would change something (quotes, `$`, globs, `&&`, redirections...).
fn direct_argv(
    cmd: &str,
    files: &[PathBuf],
    values: &[(String, Value)],
) -> Option<Vec<std::ffi::OsString>> {
    let plain = |word: &str| {
        word.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./:@%+=,^".contains(&b))
    };
    let mut argv: Vec<std::ffi::OsString> = Vec::new();
    for (i, word) in cmd.split_ascii_whitespace().enumerate() {
        if word == FILES_PLACEHOLDER {
            argv.extend(files.iter().map(|f| f.as_os_str().to_owned()));
            continue;
        }
        if let Some(name) = word.strip_prefix('{').and_then(|w| w.strip_suffix('}')) {
            match values.iter().find(|(n, _)| n == name)?.1 {
                Value::Text(ref text) => argv.push(text.into()),
                Value::List(ref items) => argv.extend(items.iter().map(Into::into)),
            }
            continue;
        }
        // `NAME=value cmd` sets a variable for the command: left to the shell
        if !plain(word) || (i == 0 && (word.contains('=') || SHELL_ONLY.contains(&word))) {
            return None;
        }
        argv.push(word.into());
    }
    // The program must be a word of the command, not a placeholder
    let first = cmd.split_ascii_whitespace().next()?;
    (!first.starts_with('{')).then_some(argv)
}

/// Output until the end of a stopped command, then the event of its end
fn continue_until_exit(
    events: &std::sync::mpsc::Receiver<Event>,
    receive: &mut impl FnMut(Vec<u8>),
) -> std::io::Result<Event> {
    loop {
        match events.recv() {
            Ok(Event::Output(chunk)) => receive(chunk),
            Ok(exit) => return Ok(exit),
            Err(_) => return Err(std::io::Error::other("the command can't be watched")),
        }
    }
}

/// Like `execute_command`, with the environment and restrictions of the hook
#[allow(clippy::too_many_arguments)]
fn execute_in(
    cmd: &str,
    dir: &Path,
    input: &HookInput,
    files: &[PathBuf],
    values: &[(String, Value)],
    output_file: Option<&Path>,
    output: Output,
    sandbox: &Sandbox,
) -> Result<CommandResult, GoatHookerError> {
    if output == Output::Inherit {
        println!("🚧 Executing command: `{cmd}`");
    }

    let quoted_files: Vec<String> = files.iter().map(|f| shell_quote(f)).collect();
    let mut shell_cmd = cmd.replace(FILES_PLACEHOLDER, &quoted_files.join(" "));
    // `{name}`: value of an input, escaped for the shell
    for (name, value) in values {
        let quoted = match value {
            Value::Text(text) => shell_quote(Path::new(text)),
            Value::List(items) => items
                .iter()
                .map(|i| shell_quote(Path::new(i)))
                .collect::<Vec<_>>()
                .join(" "),
        };
        shell_cmd = shell_cmd.replace(&format!("{{{name}}}"), &quoted);
    }
    // `$name`: the same value as an environment variable (one line per file)
    let env_values: Vec<(String, String)> = values
        .iter()
        .map(|(name, value)| {
            let value = match value {
                Value::Text(text) => text.clone(),
                Value::List(items) => items.join("\n"),
            };
            (name.clone(), value)
        })
        .collect();
    let files_env: Vec<String> = files
        .iter()
        .map(|f| f.to_string_lossy().to_string())
        .collect();

    // A simple command (`cargo fmt -- {files}`) is started without a shell: one process
    // less per hook. Anything the shell would interpret goes through `sh -c`.
    let direct = if cfg!(unix) && sandbox.network {
        direct_argv(cmd, files, values)
    } else {
        None
    };
    let prepare = |direct: Option<&[std::ffi::OsString]>| -> std::io::Result<Prepared> {
        let (mut command, network_note) = match direct {
            Some(argv) => {
                let mut command = Command::new(&argv[0]);
                command.args(&argv[1..]);
                (command, None)
            }
            None => sandboxed_shell(sandbox.network),
        };
        if let Some(allowed) = sandbox.env.as_ref() {
            command.env_clear();
            for (name, value) in std::env::vars_os() {
                let name_str = name.to_string_lossy();
                if BASIC_ENV.contains(&name_str.as_ref())
                    || name_str.starts_with("GIT_")
                    || name_str.starts_with("HOOKER_")
                    || allowed.iter().any(|a| *a == name_str)
                {
                    command.env(name, value);
                }
            }
        }
        command.envs(sandbox.extra_env.iter().cloned());
        if direct.is_none() {
            command
                .arg("-c")
                // stderr redirected to stdout to keep the order of the messages in capture mode
                .arg(match output {
                    Output::Inherit => shell_cmd.clone(),
                    Output::Capture => format!("{{ {shell_cmd}\n}} 2>&1"),
                })
                .arg("hooker") // $0
                .args(&input.args);
        }
        command
            .current_dir(dir)
            .env("HOOKER_FILES", files_env.join("\n"))
            .envs(env_values.iter().cloned())
            .envs(output_file.map(|f| ("HOOKER_OUTPUT", f.as_os_str().to_owned())))
            .envs(
                (input.force_color && output == Output::Capture)
                    .then_some([("FORCE_COLOR", "1"), ("CLICOLOR_FORCE", "1")])
                    .into_iter()
                    .flatten(),
            )
            .stdin(if input.stdin.is_some() {
                Stdio::piped()
            } else if output == Output::Capture {
                Stdio::null()
            } else {
                Stdio::inherit()
            });

        let mut merged = None;
        match output {
            // Redirect the standard output and error to the parent process
            Output::Inherit => {
                command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
            }
            // Without a shell, both go to the same pipe, in the order they are written
            Output::Capture if direct.is_some() => {
                let (reader, writer) = std::io::pipe()?;
                command.stdout(writer.try_clone()?).stderr(writer);
                merged = Some(reader);
            }
            Output::Capture => {
                command.stdout(Stdio::piped()).stderr(Stdio::piped());
            }
        };

        // With a timeout, the command gets its own process group: everything it started is
        // stopped
        #[cfg(unix)]
        if sandbox.deadline.is_some() {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        let child = command.spawn()?;
        // `command` keeps the write end of the pipe: dropped so that the output ends
        drop(command);
        Ok(Prepared {
            child,
            merged,
            network_note,
        })
    };
    let prepared = match prepare(direct.as_deref()) {
        // Not a program: maybe a shell function or alias, `sh` tells
        Err(e) if direct.is_some() && e.kind() == std::io::ErrorKind::NotFound => prepare(None),
        other => other,
    }?;
    let Prepared {
        mut child,
        merged,
        network_note,
    } = prepared;

    if let (Some(content), Some(mut child_stdin)) = (input.stdin.as_ref(), child.stdin.take()) {
        // The command may not read stdin: a closed pipe is ignored
        let _ = child_stdin.write_all(content.as_bytes());
    }

    // The output is read while the command runs (live output, timeout). The readers send
    // what they read: the command can then be waited for without waiting for the processes it
    // started, which may keep its output open (`sleep` started by Git Bash on Windows survives
    // the end of `sh`).
    let (sender, events) = std::sync::mpsc::channel::<Event>();
    for mut pipe in [
        merged.map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let sender = sender.clone();
        std::thread::spawn(move || {
            let mut buffer = [0u8; 8192];
            while let Ok(n) = pipe.read(&mut buffer) {
                if n == 0 || sender.send(Event::Output(buffer[..n].to_vec())).is_err() {
                    break;
                }
            }
        });
    }
    // The end of the command is sent too: nothing is polled, a fast command costs nothing
    let pid = child.id();
    std::thread::spawn(move || {
        let _ = sender.send(Event::Exit(child.wait()));
    });

    let mut collected = Vec::<u8>::new();
    let mut receive = |chunk: Vec<u8>| {
        if let Some(live) = sandbox.live {
            live(&String::from_utf8_lossy(&chunk));
        }
        collected.extend_from_slice(&chunk);
    };
    let mut timed_out = false;
    let status = loop {
        let event = match sandbox.deadline {
            Some(deadline) => {
                match events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                    Ok(event) => event,
                    Err(_) => {
                        // Too long: stopped, then its end is waited for
                        kill_tree(pid);
                        timed_out = true;
                        continue_until_exit(&events, &mut receive)?
                    }
                }
            }
            None => events
                .recv()
                .map_err(|_| std::io::Error::other("the command can't be watched"))?,
        };
        match event {
            Event::Output(chunk) => receive(chunk),
            Event::Exit(status) => break status?,
        }
    };
    // The rest of the output: until it is closed, or a moment without anything once the
    // command is over (processes it left behind keep it open)
    let grace = Duration::from_millis(if timed_out { 200 } else { 1000 });
    while let Ok(Event::Output(chunk)) = events.recv_timeout(grace) {
        receive(chunk);
    }
    let success = status.success() && !timed_out;

    if output == Output::Inherit {
        if timed_out {
            eprintln!("{}", "⏱ Command stopped: timeout".red());
        } else if success {
            println!("{}", "✅ Command executed successfully".green());
        } else {
            eprintln!("{}", format!("❌ Command failed with {status}").red());
        }
    }

    let mut captured = String::new();
    if let Some(note) = network_note {
        captured.push_str(note);
    }
    captured.push_str(&String::from_utf8_lossy(&collected));
    if timed_out {
        captured.push_str("\n⏱ stopped: the hook took longer than its `timeout`\n");
    }

    Ok(CommandResult {
        cmd: cmd.to_string(),
        success,
        output: captured,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Hook;
    use tempfile::tempdir;

    fn read(dir: &Path, file: &str) -> String {
        std::fs::read_to_string(dir.join(file)).unwrap()
    }

    fn input(args: &[&str], stdin: Option<&str>) -> HookInput {
        HookInput {
            args: args.iter().map(|a| a.to_string()).collect(),
            stdin: stdin.map(String::from),
            force_color: false,
            no_cache: false,
            ci_files: None,
            skip: vec![],
        }
    }

    fn hook(name: &str, cmd: &str) -> Hook {
        Hook {
            name: name.to_string(),
            trigger: Some(Trigger::PreCommit),
            cmd: Some(cmd.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn command_status() {
        let dir = tempdir().unwrap();
        let ok = execute_command(
            "true",
            dir.path(),
            &input(&[], None),
            &[],
            &[],
            None,
            Output::Inherit,
        );
        assert!(ok.unwrap().success);
        let ko = execute_command(
            "exit 3",
            dir.path(),
            &input(&[], None),
            &[],
            &[],
            None,
            Output::Inherit,
        );
        assert!(!ko.unwrap().success);
    }

    #[test]
    fn command_gets_git_args_and_runs_in_base_dir() {
        let dir = tempdir().unwrap();
        let res = execute_command(
            r#"echo "$1" > out.txt"#,
            dir.path(),
            &input(&["MSG_FILE"], None),
            &[],
            &[],
            None,
            Output::Inherit,
        );
        assert!(res.unwrap().success);
        assert_eq!(read(dir.path(), "out.txt"), "MSG_FILE\n");
    }

    #[test]
    fn command_gets_files() {
        let dir = tempdir().unwrap();
        let files = vec![PathBuf::from("src/a.rs"), PathBuf::from("it's a file.rs")];

        // {files} is escaped: each file stays a single argument
        let res = execute_command(
            r#"for f in {files}; do echo "$f"; done"#,
            dir.path(),
            &input(&[], None),
            &files,
            &[],
            None,
            Output::Capture,
        )
        .unwrap();
        assert_eq!(res.output, "src/a.rs\nit's a file.rs\n");

        let res = execute_command(
            r#"echo "$HOOKER_FILES""#,
            dir.path(),
            &input(&[], None),
            &files,
            &[],
            None,
            Output::Capture,
        )
        .unwrap();
        assert_eq!(res.output, "src/a.rs\nit's a file.rs\n");
    }

    #[test]
    fn command_gets_stdin() {
        let dir = tempdir().unwrap();
        let res = execute_command(
            "cat",
            dir.path(),
            &input(&[], Some("refs/heads/main abc\n")),
            &[],
            &[],
            None,
            Output::Capture,
        )
        .unwrap();
        assert_eq!(res.output, "refs/heads/main abc\n");

        // a command that doesn't read stdin must not crash
        let big = "x".repeat(1 << 20);
        let res = execute_command(
            "true",
            dir.path(),
            &input(&[], Some(&big)),
            &[],
            &[],
            None,
            Output::Capture,
        );
        assert!(res.unwrap().success);
    }

    #[test]
    fn capture_keeps_stderr_in_order() {
        let dir = tempdir().unwrap();
        let res = execute_command(
            "echo a; echo b >&2; echo c",
            dir.path(),
            &input(&[], None),
            &[],
            &[],
            None,
            Output::Capture,
        )
        .unwrap();
        assert_eq!(res.output, "a\nb\nc\n");
    }

    #[test]
    fn run_in_parallel() {
        let dir = tempdir().unwrap();
        let hooks = [
            hook("a", "sleep 0.3; echo a"),
            hook("b", "sleep 0.3; echo b"),
            hook("c", "exit 1"),
        ];
        let matched: Vec<MatchedHook> = hooks
            .iter()
            .map(|h| MatchedHook {
                hook: h,
                base_dir: dir.path().to_path_buf(),
                files: vec![],
                affected_by: None,
            })
            .collect();

        let start = std::time::Instant::now();
        let results = run_all(&matched, &input(&[], None), true, Output::Capture, None).unwrap();
        assert!(
            start.elapsed().as_millis() < 550,
            "hooks didn't run in parallel"
        );

        // in parallel, all the hooks run even if one fails
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].commands[0].output, "a\n");
        assert!(!results[2].success);
    }

    #[test]
    fn independent_hooks_still_run_after_a_failure() {
        let dir = tempdir().unwrap();
        let hooks = [hook("fail", "exit 1"), hook("other", "true")];
        let matched: Vec<MatchedHook> = hooks
            .iter()
            .map(|h| MatchedHook {
                hook: h,
                base_dir: dir.path().to_path_buf(),
                files: vec![],
                affected_by: None,
            })
            .collect();

        let results = run_all(&matched, &input(&[], None), false, Output::Capture, None).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].failed());
        assert!(results[1].success);
    }

    #[test]
    fn stage_fixed_files() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        let mut fmt = hook("fmt", "true");
        fmt.stage_fixed = Some(true);
        let matched = MatchedHook {
            hook: &fmt,
            base_dir: dir.path().to_path_buf(),
            files: vec![PathBuf::from("a.rs"), PathBuf::from("deleted.rs")],
            affected_by: None,
        };

        let result = run_hook(
            &matched,
            &input(&[], None),
            Output::Capture,
            &Shared::default(),
        )
        .unwrap();
        assert_eq!(result.files_to_stage, vec![dir.path().join("a.rs")]);
    }

    #[test]
    fn report_progress() {
        let dir = tempdir().unwrap();
        let hooks = [
            hook("a", "true"),
            hook("b", "exit 1"),
            hook("never", "true"),
        ];
        let matched: Vec<MatchedHook> = hooks
            .iter()
            .map(|h| MatchedHook {
                hook: h,
                base_dir: dir.path().to_path_buf(),
                files: vec![],
                affected_by: None,
            })
            .collect();

        let events = Mutex::new(Vec::new());
        let on_progress = |progress: Progress| {
            let event = match progress {
                Progress::Planned(graph) => format!("planned {}", graph.nodes.len()),
                Progress::Started(i) => format!("started {i}"),
                Progress::Finished(i, r) => format!("finished {i} {}", r.success),
                Progress::Output(..) => return,
            };
            events.lock().unwrap().push(event);
        };
        run_all(
            &matched,
            &input(&[], None),
            false,
            Output::Capture,
            Some(&on_progress),
        )
        .unwrap();

        assert_eq!(
            events.into_inner().unwrap(),
            vec![
                "planned 3",
                "started 0",
                "finished 0 true",
                "started 1",
                "finished 1 false",
                "started 2",
                "finished 2 true"
            ]
        );
    }

    #[test]
    fn force_color_env() {
        let dir = tempdir().unwrap();
        let mut colored = input(&[], None);
        colored.force_color = true;
        let res = execute_command(
            r#"echo "$FORCE_COLOR""#,
            dir.path(),
            &colored,
            &[],
            &[],
            None,
            Output::Capture,
        )
        .unwrap();
        assert_eq!(res.output, "1\n");
    }

    /// Graph of a test monorepo, run on all the pre-commit hooks
    fn run_configs(files: &[(&str, &str)], parallel: bool) -> (tempfile::TempDir, Vec<HookResult>) {
        let dir = tempdir().unwrap();
        for (folder, json) in files {
            let path = dir.path().join(folder);
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(path.join("hooker.config.json"), json).unwrap();
        }
        let configs = crate::config::ConfigLoader::new()
            .load_recursive(&dir.path().join("hooker.config.json"))
            .unwrap();
        let matcher = Matcher::new(&configs).unwrap();
        let hooks = matcher.hooks_to_run(Trigger::PreCommit, None);
        let graph = graph::build(&configs, Trigger::PreCommit, &hooks).unwrap();
        let results = run_graph(
            &graph,
            &input(&[], None),
            parallel,
            Output::Capture,
            None,
            &Shared::default(),
        )
        .unwrap();
        (dir, results)
    }

    fn by_name<'r>(results: &'r [HookResult], name: &str) -> &'r HookResult {
        results.iter().find(|r| r.name == name).unwrap()
    }

    #[test]
    fn outputs_flow_to_inputs() {
        for parallel in [false, true] {
            let (_dir, results) = run_configs(
                &[(
                    ".",
                    r#"{ "hooks": [
                        { "name": "codegen", "trigger": "pre-commit", "outputs": ["schema"],
                          "cmd": "echo generating; echo schema=gen/schema.ts >> $HOOKER_OUTPUT" },
                        { "name": "lint", "trigger": "pre-commit", "steps": [
                            { "id": "types", "inputs": { "schema": "codegen.schema", "log": "codegen.stdout", "ok": "codegen.status" },
                              "cmd": "echo tsc {schema} \"$schema\" \"[$log]\" \"$ok\"" }
                        ] }
                    ] }"#,
                )],
                parallel,
            );
            let types = by_name(&results, "lint › types");
            assert_eq!(
                types.stdout(),
                "tsc gen/schema.ts gen/schema.ts [generating] success"
            );
        }
    }

    #[test]
    fn files_are_relative_to_the_consumer() {
        let (dir, results) = run_configs(
            &[
                (".", r#"{ "imports": ["api", "web"], "hooks": [] }"#),
                (
                    "api",
                    r#"{ "hooks": [ { "name": "codegen", "trigger": "pre-commit", "cmd": "true" } ] }"#,
                ),
                (
                    "web",
                    r#"{ "hooks": [ { "name": "lint", "trigger": "pre-commit",
                        "inputs": { "generated": "api:codegen.files" }, "cmd": "for f in {generated}; do echo \"$f\"; done" } ] }"#,
                ),
            ],
            false,
        );
        let _ = dir;
        // without staged files (`None`), `codegen` has no files: the list is empty
        assert_eq!(by_name(&results, "lint").stdout(), "");
        assert_eq!(
            relative_to(Path::new("/repo/web"), Path::new("/repo/api/src/lib.rs")),
            PathBuf::from("../api/src/lib.rs")
        );
    }

    #[test]
    fn failure_skips_the_dependents_only() {
        let (_dir, results) = run_configs(
            &[(
                ".",
                r#"{ "hooks": [
                    { "name": "build", "trigger": "pre-commit", "cmd": "exit 1" },
                    { "name": "test", "trigger": "pre-commit", "needs": ["build"], "cmd": "true" },
                    { "name": "deploy", "trigger": "pre-commit", "needs": ["test"], "cmd": "true" },
                    { "name": "lint", "trigger": "pre-commit", "cmd": "true" }
                ] }"#,
            )],
            true,
        );
        assert!(by_name(&results, "build").failed());
        assert!(by_name(&results, "test").skipped);
        assert!(by_name(&results, "deploy").skipped);
        assert!(by_name(&results, "lint").success);
        assert_eq!(results.len(), 4);
    }

    #[test]
    fn steps_wait_for_each_other() {
        let (_dir, results) = run_configs(
            &[(
                ".",
                r#"{ "hooks": [ { "name": "ci", "trigger": "pre-commit", "steps": [
                    { "id": "second", "needs": ["first"], "cmd": "cat order.txt; echo second >> order.txt" },
                    { "id": "first", "cmd": "sleep 0.2; echo first > order.txt" }
                ] } ] }"#,
            )],
            true,
        );
        assert_eq!(by_name(&results, "ci › second").stdout(), "first");
    }

    #[test]
    fn parse_output_file() {
        let outputs = parse_outputs("a=1\nb=x=y\n\nnot an output\na=2\n");
        assert_eq!(outputs.get("a").unwrap(), "2");
        assert_eq!(outputs.get("b").unwrap(), "x=y");
        assert_eq!(outputs.len(), 2);
    }

    #[test]
    fn variables_flow_between_hooks() {
        for parallel in [false, true] {
            let (_dir, results) = run_configs(
                &[(
                    ".",
                    r#"{ "variables": { "version": { "default": "0.0.0", "writers": ["bump"] } },
                       "hooks": [
                        { "name": "changelog", "trigger": "pre-commit", "reads": ["version"], "cmd": "echo \"v$version\"" },
                        { "name": "bump", "trigger": "pre-commit", "writes": ["version"],
                          "cmd": "echo version=1.2.0 >> $HOOKER_VARS" },
                        { "name": "sneaky", "trigger": "pre-commit", "cmd": "echo other=1 >> $HOOKER_VARS" }
                    ] }"#,
                )],
                parallel,
            );
            // the reader runs after the writer, even if it comes first in the file
            assert_eq!(by_name(&results, "changelog").stdout(), "v1.2.0");
            // writing an undeclared variable fails the hook
            let sneaky = by_name(&results, "sneaky");
            assert!(!sneaky.success);
            assert!(sneaky.stdout().contains("without declaring it"));
        }
    }

    #[test]
    fn denied_hooks_cut_their_lineage() {
        for parallel in [false, true] {
            let (_dir, results) = run_configs(
                &[(
                    ".",
                    r#"{ "variables": { "token": { "readers": ["deploy"] }, "version": {} },
                       "hooks": [
                        { "name": "lint", "trigger": "pre-commit", "reads": ["token"], "cmd": "echo lint" },
                        { "name": "after lint", "trigger": "pre-commit", "needs": ["lint"], "cmd": "echo after" },
                        { "name": "bump", "trigger": "pre-commit", "writes": ["version"],
                          "cmd": "echo version=2 >> $HOOKER_VARS; echo other=1 >> $HOOKER_VARS" },
                        { "name": "changelog", "trigger": "pre-commit", "reads": ["version"], "cmd": "echo $version" },
                        { "name": "independent", "trigger": "pre-commit", "cmd": "echo ok" }
                    ] }"#,
                )],
                parallel,
            );
            // not allowed to read `token`: not run, and what waits for it is skipped
            let lint = by_name(&results, "lint");
            assert!(lint.denied && !lint.success);
            assert!(lint.stdout().contains("isn't allowed to read"));
            assert!(by_name(&results, "after lint").skipped);
            // wrote an undeclared variable: denied, its readers are skipped too
            assert!(by_name(&results, "bump").denied);
            assert!(by_name(&results, "changelog").skipped);
            // the others still run
            assert!(by_name(&results, "independent").success);
        }
    }

    #[test]
    fn read_only_files_are_reverted() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "original\n").unwrap();
        let hook = Hook {
            name: "formatter".into(),
            trigger: Some(Trigger::PreCommit),
            cmd: Some("echo changed > a.txt".into()),
            permissions: Some(crate::config::Permissions {
                files: Some(FileAccess::Read),
                ..Default::default()
            }),
            ..Default::default()
        };
        let matched = MatchedHook {
            hook: &hook,
            base_dir: dir.path().to_path_buf(),
            files: vec![PathBuf::from("a.txt")],
            affected_by: None,
        };
        let result = run_hook(
            &matched,
            &input(&[], None),
            Output::Capture,
            &Shared::default(),
        )
        .unwrap();
        assert!(!result.success);
        assert!(result.stdout().contains("reverted"));
        assert_eq!(read(dir.path(), "a.txt"), "original\n");
    }

    #[test]
    fn environment_is_restricted() {
        let dir = tempdir().unwrap();
        let sandbox = Sandbox {
            extra_env: vec![],
            env: Some(vec!["HOOKER_TEST_ALLOWED".into()]),
            ..Default::default()
        };
        // SAFETY: tests of this module don't read these variables concurrently
        unsafe {
            std::env::set_var("HOOKER_TEST_ALLOWED", "yes");
            std::env::set_var("SECRET_TOKEN_FOR_TEST", "leak");
        }
        let res = execute_in(
            "echo \"[$HOOKER_TEST_ALLOWED][$SECRET_TOKEN_FOR_TEST]\"",
            dir.path(),
            &input(&[], None),
            &[],
            &[],
            None,
            Output::Capture,
            &sandbox,
        )
        .unwrap();
        assert_eq!(res.output.trim(), "[yes][]");
    }

    #[test]
    fn cache_skips_hooks_that_passed() {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&root)
                .output()
                .unwrap()
        };
        git(&["init", "-q"]);
        git(&["config", "core.autocrlf", "false"]);
        std::fs::write(
            root.join("hooker.config.json"),
            r#"{ "hooks": [
                { "name": "upper", "trigger": "pre-commit", "include": ["*.txt"], "stage_fixed": true,
                  "cmd": "for f in {files}; do tr a-z A-Z < $f > $f.tmp && mv $f.tmp $f; done" }
            ] }"#,
        )
        .unwrap();
        let configs = crate::config::ConfigLoader::new()
            .load_recursive(&root.join("hooker.config.json"))
            .unwrap();
        let run = || {
            run_trigger(
                &root,
                &configs,
                Trigger::PreCommit,
                &input(&[], None),
                Output::Capture,
                None,
            )
            .unwrap()
        };

        std::fs::write(root.join("a.txt"), "hello\n").unwrap();
        git(&["add", "a.txt"]);
        assert!(!run()[0].cached);
        assert_eq!(read(&root, "a.txt"), "HELLO\n");
        // same (fixed) files: skipped
        assert!(run()[0].cached);
        // unformatted again: the formatter runs, it isn't skipped
        std::fs::write(root.join("a.txt"), "hello\n").unwrap();
        git(&["add", "a.txt"]);
        assert!(!run()[0].cached);
        assert_eq!(read(&root, "a.txt"), "HELLO\n");
    }

    /// A repository with a config, and a way to run its pre-commit on all its staged files
    fn repo_with(config: &str) -> (tempfile::TempDir, PathBuf, Vec<Config>) {
        let dir = tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(&root)
                .output()
                .unwrap()
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "core.autocrlf", "false"]);
        std::fs::write(root.join("hooker.config.json"), config).unwrap();
        std::fs::write(root.join("a.txt"), "a\n").unwrap();
        git(&["add", "a.txt"]);
        let configs = crate::config::ConfigLoader::new()
            .load_recursive(&root.join("hooker.config.json"))
            .unwrap();
        (dir, root, configs)
    }

    #[test]
    fn hooks_are_skipped_by_name_and_branch() {
        let (_dir, root, configs) = repo_with(
            r#"{ "hooks": [
                { "name": "lint", "trigger": "pre-commit", "cmd": "true" },
                { "name": "test", "trigger": "pre-commit", "cmd": "false", "needs": ["lint"] },
                { "name": "deploy", "trigger": "pre-commit", "cmd": "false", "branches": ["release/*"] },
                { "name": "wip", "trigger": "pre-commit", "cmd": "false", "skip_branches": ["ma*"] }
            ] }"#,
        );
        let input = HookInput {
            skip: vec!["test".into()],
            no_cache: true,
            ..Default::default()
        };
        let results = run_trigger(
            &root,
            &configs,
            Trigger::PreCommit,
            &input,
            Output::Capture,
            None,
        )
        .unwrap();
        assert!(results.iter().all(|r| r.success), "{results:#?}");
        assert_eq!(
            by_name(&results, "test").note.as_deref(),
            Some("HOOKER_SKIP")
        );
        assert_eq!(
            by_name(&results, "deploy").note.as_deref(),
            Some("not on main")
        );
        assert!(by_name(&results, "wip").skipped);
        assert!(!by_name(&results, "lint").skipped);
    }

    #[test]
    fn a_hook_is_stopped_after_its_timeout() {
        let (_dir, root, configs) = repo_with(
            r#"{ "hooks": [
                { "name": "slow", "trigger": "pre-commit", "cmd": "echo started; sleep 20", "timeout": "300ms" }
            ] }"#,
        );
        let start = Instant::now();
        let lines = Mutex::new(String::new());
        let on_progress = |progress: Progress| {
            if let Progress::Output(_, text) = progress {
                lines.lock().unwrap().push_str(text);
            }
        };
        let results = run_trigger(
            &root,
            &configs,
            Trigger::PreCommit,
            &HookInput::default(),
            Output::Capture,
            Some(&on_progress),
        )
        .unwrap();
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(results[0].failed());
        assert!(results[0].stdout().contains("timeout"));
        // the output came while the hook was running
        assert!(lines.lock().unwrap().contains("started"));
    }

    #[test]
    fn simple_commands_skip_the_shell() {
        let files = vec![PathBuf::from("a b.rs"), PathBuf::from("c.rs")];
        let values = vec![("schema".to_string(), Value::Text("x.json".into()))];
        let argv = |cmd: &str| {
            direct_argv(cmd, &files, &values).map(|a| {
                a.iter()
                    .map(|s| s.to_string_lossy().to_string())
                    .collect::<Vec<_>>()
            })
        };
        assert_eq!(
            argv("cargo fmt -- {files}").unwrap(),
            ["cargo", "fmt", "--", "a b.rs", "c.rs"]
        );
        assert_eq!(
            argv("tsc --project {schema}").unwrap(),
            ["tsc", "--project", "x.json"]
        );
        for shell in [
            "cargo fmt && cargo test",
            "echo $HOME",
            "eslint 'src/**'",
            "ls *.rs",
            "cd api",
            "FOO=1 make",
            "echo hi > out",
            "{files}",
            "x {unknown}",
        ] {
            assert!(argv(shell).is_none(), "{shell}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn direct_command_output_and_failure() {
        let dir = tempdir().unwrap();
        // stdout and stderr in the captured output, in order
        std::fs::write(
            dir.path().join("both.sh"),
            "#!/bin/sh\necho out\necho err >&2\nexit 3\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            dir.path().join("both.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        let result = execute_command(
            "./both.sh",
            dir.path(),
            &input(&[], None),
            &[],
            &[],
            None,
            Output::Capture,
        )
        .unwrap();
        assert!(!result.success);
        assert_eq!(result.output, "out\nerr\n");

        // Not a program: `sh` says so, the hook fails
        let result = execute_command(
            "no-such-program-hooker",
            dir.path(),
            &input(&[], None),
            &[],
            &[],
            None,
            Output::Capture,
        )
        .unwrap();
        assert!(!result.success);
        assert!(result.output.contains("not found"), "{}", result.output);
    }

    #[test]
    fn a_fast_command_isnt_slowed_down() {
        let dir = tempdir().unwrap();
        let start = Instant::now();
        for _ in 0..20 {
            let result = execute_command(
                "git --version",
                dir.path(),
                &input(&[], None),
                &[],
                &[],
                None,
                Output::Capture,
            )
            .unwrap();
            assert!(result.success);
        }
        // Waiting for the end used to poll every 10 ms
        assert!(
            start.elapsed() < Duration::from_millis(2000),
            "{:?}",
            start.elapsed()
        );
    }
}
