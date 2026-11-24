//! Hooks of the pre-commit ecosystem: a repository with a `.pre-commit-hooks.yaml` used with
//! `from` + `id`, and the tools their commands need.
//!
//! The tools come from the computer first: when the program of a hook is in the PATH, it is
//! used as it is. Otherwise (or with `isolate = true`), an environment is built once in the
//! cache with the toolchain of the computer: a virtualenv for Python, `npm install` for Node,
//! `cargo install` for Rust... No runtime is ever downloaded: a missing toolchain is an error
//! that says what to install.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

use serde_json::Value;

use crate::{config::Hook, errors::GoatHookerError};

/// The file listing the hooks of a repository of the pre-commit ecosystem
pub const MANIFEST: &str = ".pre-commit-hooks.yaml";

/// What a hook needs before it runs
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tool {
    /// pre-commit `language`
    pub language: String,
    /// Repository of the hook (`None` for a local hook with `additional_dependencies`)
    pub repo: Option<PathBuf>,
    /// URL and rev, to name the environment
    pub source: String,
    pub dependencies: Vec<String>,
    /// `Some(true)`: always an environment, `Some(false)`: never, `None`: when the program
    /// isn't installed
    pub isolate: Option<bool>,
    /// First word of the command
    pub program: String,
}

/// Changes of the environment of the commands of a hook
#[derive(Debug, Default, PartialEq)]
pub struct Prepared {
    /// Folders put first in the PATH
    pub path: Vec<PathBuf>,
    /// Other variables (`GEM_HOME`...)
    pub env: Vec<(String, String)>,
}

/// Languages whose tools are built in an environment
const BUILT: [&str; 6] = ["python", "node", "rust", "golang", "ruby", "conda"];
/// Languages run as they are
const AS_IS: [&str; 8] = [
    "system",
    "script",
    "fail",
    "pygrep",
    "unsupported",
    "unsupported_script",
    "docker_image",
    "docker",
];

/// Reads the hook `id` of a repository's `.pre-commit-hooks.yaml`
pub fn manifest_hook(repo: &Path, id: &str) -> Result<Value, GoatHookerError> {
    let file = repo.join(MANIFEST);
    let content = fs::read_to_string(&file).map_err(|_| GoatHookerError::InvalidHook {
        name: id.to_string(),
        reason: format!("the repository has no {MANIFEST}"),
    })?;
    let hooks: Value =
        serde_yaml_ng::from_str(&content).map_err(|err| GoatHookerError::InvalidHook {
            name: id.to_string(),
            reason: format!("invalid {MANIFEST}: {err}"),
        })?;
    let found = hooks
        .as_array()
        .into_iter()
        .flatten()
        .find(|h| h.get("id").and_then(Value::as_str) == Some(id))
        .cloned();
    found.ok_or_else(|| {
        let ids: Vec<&str> = hooks
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|h| h.get("id").and_then(Value::as_str))
            .collect();
        GoatHookerError::InvalidHook {
            name: id.to_string(),
            reason: format!("no hook `{id}` in {MANIFEST} (hooks: {})", ids.join(", ")),
        }
    })
}

fn strings(value: Option<&Value>) -> Vec<String> {
    crate::import::strings(value)
}

/// Quotes a word for `sh`, unless it doesn't need it
fn quote(word: &str) -> String {
    if !word.is_empty()
        && word
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./:@%+=,^".contains(&b))
    {
        word.to_string()
    } else {
        format!("'{}'", word.replace('\'', r"'\''"))
    }
}

/// Completes a hook using `from` + `id` with its definition in the repository: command,
/// files, trigger and the tool it needs. What the hook sets itself wins.
pub fn complete(hook: &mut Hook, repo: &Path, url: &str) -> Result<(), GoatHookerError> {
    let id = hook.id.clone().unwrap_or_default();
    let definition = manifest_hook(repo, &id)?;
    let get = |key: &str| definition.get(key);
    let language = get("language")
        .and_then(Value::as_str)
        .unwrap_or("system")
        .to_string();
    let entry = get("entry").and_then(Value::as_str).unwrap_or_default();
    let args = match hook.args.clone() {
        Some(args) => args,
        None => strings(get("args")),
    };
    let pass_filenames = get("pass_filenames").and_then(Value::as_bool) != Some(false);

    if hook.name.is_empty() {
        hook.name = id.clone();
    }
    if hook.trigger.is_none() {
        hook.trigger = strings(get("stages"))
            .iter()
            .find_map(|s| crate::import::precommit_stage(s))
            .or(Some(crate::config::Trigger::PreCommit));
    }
    if hook.cmd.is_none() && hook.cmds.is_none() {
        hook.cmd = Some(command(&language, entry, &args, pass_filenames, repo, &id));
    }
    // Files: the ones of the hook, else the ones of its definition
    if hook.include.is_none() && hook.include_regex.is_none() {
        let mut include = crate::import::types_globs(&strings(get("types")));
        include.extend(crate::import::types_globs(&strings(get("types_or"))));
        hook.include = (!include.is_empty()).then_some(include);
        hook.include_regex = get("files")
            .and_then(Value::as_str)
            .filter(|r| !r.is_empty())
            .map(String::from);
    }
    if hook.types.is_none() {
        let kinds = crate::import::kinds(&strings(get("types")));
        hook.types = (!kinds.is_empty()).then_some(kinds);
    }
    if hook.exclude.is_none() && hook.exclude_regex.is_none() {
        let exclude = crate::import::types_globs(&strings(get("exclude_types")));
        hook.exclude = (!exclude.is_empty()).then_some(exclude);
        hook.exclude_regex = get("exclude")
            .and_then(Value::as_str)
            .filter(|r| !r.is_empty() && *r != "^$")
            .map(String::from);
    }
    if hook.always_run.is_none() && get("always_run").and_then(Value::as_bool) == Some(true) {
        hook.always_run = Some(true);
    }
    // Formatters and fixers change the files: they are staged again
    if hook.stage_fixed.is_none() && pass_filenames {
        hook.stage_fixed = Some(true);
    }
    // A hook of another repository reads the files by default: these ones may fix them
    if hook.permissions.as_ref().is_none_or(|p| p.files.is_none()) {
        let mut permissions = hook.permissions.clone().unwrap_or_default();
        permissions.files = Some(crate::config::FileAccess::Write);
        hook.permissions = Some(permissions);
    }

    let mut dependencies = strings(get("additional_dependencies"));
    dependencies.extend(hook.additional_dependencies.clone().unwrap_or_default());
    hook.tool = Some(Tool {
        language,
        repo: Some(repo.to_path_buf()),
        source: format!("{url}#{}", hook.rev.clone().unwrap_or_default()),
        dependencies,
        isolate: hook.isolate,
        program: entry
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string(),
    });
    Ok(())
}

/// The command of a hook of the pre-commit ecosystem
fn command(
    language: &str,
    entry: &str,
    args: &[String],
    pass_filenames: bool,
    repo: &Path,
    id: &str,
) -> String {
    let args: Vec<String> = args.iter().map(|a| quote(a)).collect();
    let files = if pass_filenames { " {files}" } else { "" };
    let mut words: Vec<String> = Vec::new();
    match language {
        // The entry is the message
        "fail" => {
            return format!(
                "printf '%s\\n' {}{files}; exit 1",
                quote(if entry.is_empty() { id } else { entry })
            );
        }
        // The entry is a Python regex, looked for in the files
        "pygrep" => {
            let exe = std::env::current_exe()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "hooker".into());
            words.push(quote(&exe));
            words.push("pygrep".into());
            words.extend(args);
            words.push("--".into());
            words.push(quote(entry));
            return format!("{}{files}", words.join(" "));
        }
        // A script of the repository
        "script" | "unsupported_script" => {
            let mut parts = entry.split_whitespace();
            let script = parts.next().unwrap_or_default();
            words.push(quote(&repo.join(script).to_string_lossy()));
            words.extend(parts.map(String::from));
        }
        "docker_image" => {
            words.extend(docker_run());
            words.push(entry.to_string());
        }
        "docker" => {
            words.extend(docker_run());
            let mut parts = entry.split_whitespace();
            let program = parts.next().unwrap_or_default();
            words.push(format!("--entrypoint {}", quote(program)));
            words.push(docker_tag(repo));
            words.extend(parts.map(String::from));
        }
        _ => words.push(entry.to_string()),
    }
    words.extend(args);
    format!("{}{files}", words.join(" "))
}

fn docker_run() -> Vec<String> {
    vec![
        "docker run --rm".into(),
        "-v \"$PWD:/src:rw,Z\" --workdir /src".into(),
    ]
}

/// Image built from a repository with a Dockerfile (`language: docker`)
fn docker_tag(repo: &Path) -> String {
    format!("goat-hooker-{:016x}", fnv(&repo.to_string_lossy()))
}

fn fnv(text: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in text.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// `program` is in the PATH (or is a path to a file)
fn installed(program: &str) -> bool {
    if program.is_empty() {
        return false;
    }
    if program.contains('/') || program.contains('\\') {
        return Path::new(program).is_file();
    }
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let extensions: &[&str] = if cfg!(windows) {
        &["", ".exe", ".cmd", ".bat"]
    } else {
        &[""]
    };
    std::env::split_paths(&path).any(|dir| {
        extensions
            .iter()
            .any(|ext| dir.join(format!("{program}{ext}")).is_file())
    })
}

/// What the commands of a hook need: nothing, the computer's tool, or an environment
/// built once in the cache. The error says what is missing.
pub fn prepare(tool: &Tool) -> Result<Prepared, String> {
    let language = tool.language.as_str();
    if AS_IS.contains(&language) {
        if language.starts_with("docker") && !installed("docker") {
            return Err("this hook runs in Docker: install Docker".to_string());
        }
        if language == "docker"
            && let Some(repo) = tool.repo.as_ref()
        {
            build_docker_image(repo)?;
        }
        return Ok(Prepared::default());
    }
    if !BUILT.contains(&language) {
        return Err(format!(
            "hooks in {language} aren't supported yet: use a `cmd` with a tool of your computer"
        ));
    }
    let system = tool.isolate != Some(true) && tool.dependencies.is_empty();
    if system && installed(&tool.program) {
        return Ok(Prepared::default());
    }
    if tool.isolate == Some(false) {
        return Err(format!(
            "`{}` isn't installed. Install it, or remove `isolate = false` to let Goat Hooker \
             build it with your {} toolchain",
            tool.program,
            toolchain(language)
        ));
    }
    build(tool)
}

fn toolchain(language: &str) -> &'static str {
    match language {
        "python" => "Python",
        "node" => "Node.js (npm)",
        "rust" => "Rust (cargo)",
        "golang" => "Go",
        "ruby" => "Ruby (gem)",
        "conda" => "conda",
        _ => "system",
    }
}

/// Folder of the environment of a tool in the cache
pub fn env_dir(tool: &Tool) -> PathBuf {
    let key = format!(
        "{}|{}|{}",
        tool.language,
        tool.source,
        tool.dependencies.join(",")
    );
    crate::remote::cache_dir()
        .join("envs")
        .join(format!("{}-{:016x}", tool.language, fnv(&key)))
}

fn bin_dir(env: &Path, language: &str) -> PathBuf {
    match (language, cfg!(windows)) {
        ("python" | "conda", true) => env.join("Scripts"),
        ("node", true) => env.to_path_buf(),
        _ => env.join("bin"),
    }
}

fn prepared(env: &Path, language: &str) -> Prepared {
    let mut prepared = Prepared {
        path: vec![bin_dir(env, language)],
        env: vec![],
    };
    match language {
        "ruby" => {
            prepared.env = vec![
                ("GEM_HOME".into(), env.to_string_lossy().into()),
                ("GEM_PATH".into(), env.to_string_lossy().into()),
            ]
        }
        "python" | "conda" => {
            prepared.env = vec![("VIRTUAL_ENV".into(), env.to_string_lossy().into())]
        }
        _ => {}
    }
    prepared
}

/// Builds the environment once (other processes wait for it), then returns it
fn build(tool: &Tool) -> Result<Prepared, String> {
    let env = env_dir(tool);
    let ready = env.join(".goat-hooker-ready");
    if ready.exists() {
        return Ok(prepared(&env, &tool.language));
    }
    let _lock = Lock::take(&env.with_extension("lock"))?;
    if ready.exists() {
        return Ok(prepared(&env, &tool.language));
    }
    let _ = fs::remove_dir_all(&env);
    fs::create_dir_all(&env).map_err(|e| e.to_string())?;
    eprintln!(
        "📦 Building the {} environment of `{}` (once)...",
        tool.language, tool.program
    );
    let result = match tool.language.as_str() {
        "python" => build_python(tool, &env),
        "node" => build_node(tool, &env),
        "rust" => build_rust(tool, &env),
        "golang" => build_go(tool, &env),
        "ruby" => build_ruby(tool, &env),
        "conda" => build_conda(tool, &env),
        other => Err(format!("hooks in {other} aren't supported yet")),
    };
    if let Err(err) = result {
        let _ = fs::remove_dir_all(&env);
        return Err(err);
    }
    fs::write(&ready, "").map_err(|e| e.to_string())?;
    Ok(prepared(&env, &tool.language))
}

/// A command building an environment: its output is the error when it fails
fn run(command: &mut Command, what: &str) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|e| format!("can't run {what}: {e}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "{what} failed:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

fn missing(program: &str, language: &str) -> String {
    format!(
        "`{program}` is needed to build this {} hook: install it (Goat Hooker never downloads toolchains)",
        toolchain(language)
    )
}

fn build_python(tool: &Tool, env: &Path) -> Result<(), String> {
    let mut packages: Vec<String> = tool
        .repo
        .iter()
        .filter(|r| r.join("setup.py").exists() || r.join("pyproject.toml").exists())
        .map(|r| r.to_string_lossy().to_string())
        .collect();
    packages.extend(tool.dependencies.iter().cloned());
    // uv when it is there: much faster
    if installed("uv") {
        run(
            Command::new("uv").args(["venv", "--quiet"]).arg(env),
            "uv venv",
        )?;
        if !packages.is_empty() {
            run(
                Command::new("uv")
                    .args(["pip", "install", "--quiet", "--python"])
                    .arg(env)
                    .args(&packages),
                "uv pip install",
            )?;
        }
        return Ok(());
    }
    let python = ["python3", "python"]
        .into_iter()
        .find(|p| installed(p))
        .ok_or_else(|| missing("python3", "python"))?;
    run(
        Command::new(python).args(["-m", "venv"]).arg(env),
        "python -m venv",
    )?;
    if !packages.is_empty() {
        let pip = bin_dir(env, "python").join(if cfg!(windows) { "pip.exe" } else { "pip" });
        run(
            Command::new(pip)
                .args(["install", "--quiet", "--disable-pip-version-check"])
                .args(&packages),
            "pip install",
        )?;
    }
    Ok(())
}

fn build_node(tool: &Tool, env: &Path) -> Result<(), String> {
    if !installed("npm") {
        return Err(missing("npm", "node"));
    }
    let mut packages: Vec<String> = tool
        .repo
        .iter()
        .filter(|r| r.join("package.json").exists())
        .map(|r| r.to_string_lossy().to_string())
        .collect();
    packages.extend(tool.dependencies.iter().cloned());
    if packages.is_empty() {
        return Ok(());
    }
    run(
        Command::new(if cfg!(windows) { "npm.cmd" } else { "npm" })
            .args(["install", "--global", "--no-audit", "--no-fund", "--prefix"])
            .arg(env)
            .args(&packages),
        "npm install",
    )
}

fn build_rust(tool: &Tool, env: &Path) -> Result<(), String> {
    if !installed("cargo") {
        return Err(missing("cargo", "rust"));
    }
    if let Some(repo) = tool.repo.as_ref().filter(|r| r.join("Cargo.toml").exists()) {
        run(
            Command::new("cargo")
                .args(["install", "--quiet", "--path"])
                .arg(repo)
                .arg("--root")
                .arg(env),
            "cargo install",
        )?;
    }
    // `cli:name:version`: a binary to install; the other dependencies are libraries of the
    // hook's own crate, which cargo already handles
    for dependency in &tool.dependencies {
        let Some(cli) = dependency.strip_prefix("cli:") else {
            continue;
        };
        let mut parts = cli.splitn(2, ':');
        let name = parts.next().unwrap_or_default();
        let mut command = Command::new("cargo");
        command
            .args(["install", "--quiet", name])
            .arg("--root")
            .arg(env);
        if let Some(version) = parts.next() {
            command.args(["--version", version]);
        }
        run(&mut command, "cargo install")?;
    }
    Ok(())
}

fn build_go(tool: &Tool, env: &Path) -> Result<(), String> {
    if !installed("go") {
        return Err(missing("go", "golang"));
    }
    let bin = env.join("bin");
    if let Some(repo) = tool.repo.as_ref().filter(|r| r.join("go.mod").exists()) {
        run(
            Command::new("go")
                .args(["install", "./..."])
                .current_dir(repo)
                .env("GOBIN", &bin),
            "go install",
        )?;
    }
    for dependency in &tool.dependencies {
        let target = if dependency.contains('@') {
            dependency.clone()
        } else {
            format!("{dependency}@latest")
        };
        run(
            Command::new("go")
                .args(["install", &target])
                .env("GOBIN", &bin),
            "go install",
        )?;
    }
    Ok(())
}

fn build_ruby(tool: &Tool, env: &Path) -> Result<(), String> {
    if !installed("gem") {
        return Err(missing("gem", "ruby"));
    }
    let mut gems: Vec<String> = Vec::new();
    if let Some(repo) = tool.repo.as_ref() {
        let specs: Vec<PathBuf> = fs::read_dir(repo)
            .map(|d| {
                d.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|e| e == "gemspec"))
                    .collect()
            })
            .unwrap_or_default();
        for spec in specs {
            run(
                Command::new("gem")
                    .arg("build")
                    .arg(&spec)
                    .current_dir(repo),
                "gem build",
            )?;
        }
        gems.extend(
            fs::read_dir(repo)
                .map_err(|e| e.to_string())?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == "gem"))
                .map(|p| p.to_string_lossy().to_string()),
        );
    }
    gems.extend(tool.dependencies.iter().cloned());
    if gems.is_empty() {
        return Ok(());
    }
    run(
        Command::new("gem")
            .args(["install", "--no-document", "--install-dir"])
            .arg(env)
            .arg("--bindir")
            .arg(env.join("bin"))
            .args(&gems),
        "gem install",
    )
}

fn build_conda(tool: &Tool, env: &Path) -> Result<(), String> {
    let conda = ["mamba", "conda"]
        .into_iter()
        .find(|p| installed(p))
        .ok_or_else(|| missing("conda", "conda"))?;
    let mut command = Command::new(conda);
    command
        .args(["create", "--yes", "--quiet", "--prefix"])
        .arg(env);
    if let Some(file) = tool
        .repo
        .as_ref()
        .map(|r| r.join("environment.yml"))
        .filter(|f| f.exists())
    {
        run(
            Command::new(conda)
                .args(["env", "create", "--yes", "--quiet", "--prefix"])
                .arg(env)
                .arg("--file")
                .arg(file),
            "conda env create",
        )?;
        if tool.dependencies.is_empty() {
            return Ok(());
        }
        command = Command::new(conda);
        command
            .args(["install", "--yes", "--quiet", "--prefix"])
            .arg(env);
    }
    run(command.args(&tool.dependencies), "conda")
}

fn build_docker_image(repo: &Path) -> Result<(), String> {
    let tag = docker_tag(repo);
    let exists = Command::new("docker")
        .args(["image", "inspect", &tag])
        .output()
        .is_ok_and(|o| o.status.success());
    if exists {
        return Ok(());
    }
    eprintln!("📦 Building the Docker image of the hook (once)...");
    run(
        Command::new("docker")
            .args(["build", "--tag", &tag])
            .arg(repo),
        "docker build",
    )
}

/// A folder created as a lock: only one process builds an environment
struct Lock(PathBuf);

impl Lock {
    fn take(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let start = Instant::now();
        loop {
            match fs::create_dir(path) {
                Ok(()) => return Ok(Lock(path.to_path_buf())),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    // Left by a process that stopped: taken over after a while
                    let stale = fs::metadata(path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|m| m.elapsed().ok())
                        .is_some_and(|age| age > Duration::from_secs(30 * 60));
                    if stale {
                        let _ = fs::remove_dir(path);
                        continue;
                    }
                    if start.elapsed() > Duration::from_secs(30 * 60) {
                        return Err("another process is still building this environment".into());
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(e) => return Err(e.to_string()),
            }
        }
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.0);
    }
}

/// `hooker pygrep`: the `pygrep` language of pre-commit. Prints the matching lines of the
/// files; fails when a line matches (or, with `--negate`, when a file has no match).
pub fn pygrep(
    pattern: &str,
    files: &[PathBuf],
    ignore_case: bool,
    multiline: bool,
    negate: bool,
) -> Result<bool, String> {
    // The flags of pre-commit: `re.IGNORECASE`, and `re.MULTILINE | re.DOTALL`
    let flags = match (ignore_case, multiline) {
        (true, true) => "(?ims)",
        (true, false) => "(?i)",
        (false, true) => "(?ms)",
        (false, false) => "",
    };
    let regex = fancy_regex::Regex::new(&format!("{flags}{pattern}"))
        .map_err(|e| format!("invalid regex `{pattern}`: {e}"))?;
    let mut passed = true;
    for file in files {
        let Ok(bytes) = fs::read(file) else { continue };
        let content = String::from_utf8_lossy(&bytes);
        let shown = file.display();
        if negate {
            if !regex.is_match(content.as_ref()).unwrap_or(false) {
                println!("{shown}");
                passed = false;
            }
        } else if multiline {
            if let Ok(Some(found)) = regex.find(content.as_ref()) {
                let line = content[..found.start()].matches('\n').count() + 1;
                println!("{shown}:{line}:{}", found.as_str());
                passed = false;
            }
        } else {
            for (number, line) in content.lines().enumerate() {
                if regex.is_match(line).unwrap_or(false) {
                    println!("{shown}:{}:{line}", number + 1);
                    passed = false;
                }
            }
        }
    }
    Ok(passed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Trigger;
    use tempfile::tempdir;

    fn repo(manifest: &str) -> tempfile::TempDir {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join(MANIFEST), manifest).unwrap();
        dir
    }

    #[test]
    fn completes_a_hook_from_its_definition() {
        let dir = repo(
            r#"
- id: ruff
  name: ruff
  entry: ruff check --force-exclude
  language: python
  types_or: [python, pyi]
  args: []
  require_serial: true
- id: check-yaml
  name: check yaml
  entry: check-yaml
  language: python
  types: [yaml]
  exclude: ^vendor/
"#,
        );
        let mut hook = Hook {
            from: Some("https://github.com/astral-sh/ruff-pre-commit".into()),
            rev: Some("v0.8.0".into()),
            id: Some("ruff".into()),
            args: Some(vec!["--fix".into()]),
            ..Default::default()
        };
        complete(
            &mut hook,
            dir.path(),
            "https://github.com/astral-sh/ruff-pre-commit",
        )
        .unwrap();
        assert_eq!(hook.name, "ruff");
        assert_eq!(hook.trigger, Some(Trigger::PreCommit));
        assert_eq!(
            hook.cmd.as_deref(),
            Some("ruff check --force-exclude --fix {files}")
        );
        assert_eq!(
            hook.include,
            Some(vec!["**/*.py".into(), "**/*.pyi".into()])
        );
        assert_eq!(hook.stage_fixed, Some(true));
        let tool = hook.tool.unwrap();
        assert_eq!(tool.language, "python");
        assert_eq!(tool.program, "ruff");

        let mut yaml = Hook {
            id: Some("check-yaml".into()),
            ..Default::default()
        };
        complete(&mut yaml, dir.path(), "x").unwrap();
        assert_eq!(yaml.exclude_regex.as_deref(), Some("^vendor/"));

        let mut unknown = Hook {
            id: Some("nope".into()),
            ..Default::default()
        };
        let err = complete(&mut unknown, dir.path(), "x")
            .unwrap_err()
            .to_string();
        assert!(err.contains("ruff, check-yaml"), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn commands_of_the_languages() {
        let repo = Path::new("/repo");
        assert_eq!(
            command("script", "bin/check.sh --strict", &[], true, repo, "x"),
            "/repo/bin/check.sh --strict {files}"
        );
        assert_eq!(
            command("fail", "No .orig files", &[], true, repo, "x"),
            "printf '%s\\n' 'No .orig files' {files}; exit 1"
        );
        assert!(
            command(
                "pygrep",
                r"(?i)fixme",
                &["--ignore-case".into()],
                true,
                repo,
                "x"
            )
            .ends_with(r"pygrep --ignore-case -- '(?i)fixme' {files}")
        );
        assert_eq!(
            command("system", "make lint", &["a b".into()], false, repo, "x"),
            "make lint 'a b'"
        );
    }

    #[test]
    fn system_tools_first() {
        let tool = Tool {
            language: "python".into(),
            program: "git".into(),
            ..Default::default()
        };
        assert_eq!(prepare(&tool).unwrap(), Prepared::default());

        let missing = Tool {
            language: "python".into(),
            program: "no-such-tool-hooker".into(),
            isolate: Some(false),
            ..Default::default()
        };
        let err = prepare(&missing).unwrap_err();
        assert!(err.contains("isn't installed"), "{err}");

        let unsupported = Tool {
            language: "haskell".into(),
            program: "x".into(),
            ..Default::default()
        };
        assert!(
            prepare(&unsupported)
                .unwrap_err()
                .contains("aren't supported")
        );
    }

    #[test]
    fn pygrep_lines() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("a.py");
        fs::write(&file, "ok\n# FIXME later\n").unwrap();
        assert!(!pygrep("fixme", std::slice::from_ref(&file), true, false, false).unwrap());
        assert!(pygrep("fixme", std::slice::from_ref(&file), false, false, false).unwrap());
        assert!(pygrep("ok", std::slice::from_ref(&file), false, false, true).unwrap());
        assert!(!pygrep("absent", std::slice::from_ref(&file), false, false, true).unwrap());
        // Python regexes: lookarounds
        fs::write(&file, "x = 1  # noqa\ny = 2  # noqa: E501\n").unwrap();
        assert!(
            !pygrep(
                r"(?i)# noqa(?!: )",
                std::slice::from_ref(&file),
                false,
                false,
                false
            )
            .unwrap()
        );
        fs::write(&file, "y = 2  # noqa: E501\n").unwrap();
        assert!(pygrep(r"(?i)# noqa(?!: )", &[file], false, false, false).unwrap());
    }

    #[test]
    fn python_environment_with_the_system_python() {
        if !installed("uv") && !installed("python3") && !installed("python") {
            return;
        }
        let dir = tempdir().unwrap();
        let env = dir.path().join("env");
        let tool = Tool {
            language: "python".into(),
            program: "x".into(),
            ..Default::default()
        };
        build_python(&tool, &env).unwrap();
        let bin = bin_dir(&env, "python");
        let python = if cfg!(windows) {
            "python.exe"
        } else {
            "python"
        };
        assert!(
            bin.join(python).exists(),
            "{:?}",
            fs::read_dir(&bin).map(|d| d.count())
        );
        let prepared = prepared(&env, "python");
        assert_eq!(prepared.path, vec![bin]);
    }

    #[test]
    fn one_builder_at_a_time() {
        let dir = tempdir().unwrap();
        let lock = dir.path().join("env.lock");
        let first = Lock::take(&lock).unwrap();
        assert!(lock.exists());
        let waiting = {
            let lock = lock.clone();
            std::thread::spawn(move || {
                let _second = Lock::take(&lock).unwrap();
                Instant::now()
            })
        };
        std::thread::sleep(Duration::from_millis(300));
        let released = Instant::now();
        drop(first);
        assert!(waiting.join().unwrap() >= released);
    }
}
