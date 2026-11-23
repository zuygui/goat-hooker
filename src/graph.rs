//! Graph of the hooks of a trigger: each hook (or each step of a hook) is a node,
//! linked to the nodes it waits for (`needs`) or receives values from (`inputs`).
//!
//! References:
//! - `lint`: a hook of the same project (or a sibling step, from a step)
//! - `lint/types`: a step of a hook
//! - `packages/api:codegen`: a hook of another project (`root:` for the root)
//! - `codegen.schema` (in `inputs`): the output `schema` of the node `codegen`

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
};

use crate::{
    config::{BUILTIN_OUTPUTS, Config, Hook, Step, Trigger},
    errors::GoatHookerError,
    matcher::MatchedHook,
};

/// A value received from another node
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    /// Name in the commands (`{name}`, `$name`)
    pub name: String,
    /// Node giving the value
    pub node: usize,
    /// Output of that node (`files`, `stdout`, `status` or declared)
    pub output: String,
}

#[derive(Debug)]
pub struct Node<'a> {
    pub hook: &'a Hook,
    pub step: Option<&'a Step>,
    /// Project: `root` or its folder relative to the root
    pub project: String,
    pub base_dir: PathBuf,
    /// Targeted files, relative to `base_dir` (empty when the hook runs as a dependency)
    pub files: Vec<PathBuf>,
    /// Nodes it waits for (explicit and through `inputs`), without duplicates
    pub needs: Vec<usize>,
    pub inputs: Vec<Input>,
    /// Declared outputs
    pub outputs: Vec<String>,
    /// Variables read and written (`reads`, `writes` of the hook)
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    /// Nodes writing a variable this one reads: it runs after them, even if they fail
    pub after: Vec<usize>,
    /// Why it isn't allowed to run (not trusted, forbidden variable): it and everything
    /// after it are cut
    pub denied: Option<String>,
}

impl Node<'_> {
    /// `lint` ou `lint › types`
    pub fn label(&self) -> String {
        match self.step {
            Some(step) => format!("{} › {}", self.hook.name, step.id),
            None => self.hook.name.clone(),
        }
    }

    /// Full reference: `packages/web:lint/types`
    pub fn reference(&self) -> String {
        let step = self.step.map(|s| format!("/{}", s.id)).unwrap_or_default();
        format!("{}:{}{step}", self.project, self.hook.name)
    }

    pub fn commands(&self) -> Vec<&str> {
        match self.step {
            Some(step) => vec![step.cmd.as_str()],
            None => self.hook.commands(),
        }
    }
}

/// The nodes, in an order where each one comes after the ones it waits for
#[derive(Debug, Default)]
pub struct Graph<'a> {
    pub nodes: Vec<Node<'a>>,
}

/// A hook of the trigger, before being split into nodes
struct Entry<'a> {
    hook: &'a Hook,
    project: String,
    base_dir: PathBuf,
}

/// Target of a reference
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Target {
    Hook(usize),
    Step(usize, usize),
}

fn error(name: &str, reason: String) -> GoatHookerError {
    GoatHookerError::InvalidHook {
        name: name.to_string(),
        reason,
    }
}

/// `root`, ``, `.` → `root`
fn normalize_project(project: &str) -> String {
    match project.trim_matches('/') {
        "" | "." | "root" => "root".to_string(),
        other => other.trim_start_matches("./").to_string(),
    }
}

/// Name of a project from its folder
pub fn project_name(root: &Path, dir: &Path) -> String {
    match dir.strip_prefix(root) {
        Ok(rel) => normalize_project(&rel.to_string_lossy()),
        Err(_) => dir.display().to_string(),
    }
}

/// Builds the graph of the hooks to run.
/// `selected`: the matching hooks (trigger + files); the hooks they wait for
/// are added even if they match no file.
pub fn build<'a>(
    configs: &'a [Config],
    trigger: Trigger,
    selected: &[MatchedHook<'a>],
) -> Result<Graph<'a>, GoatHookerError> {
    let root = configs
        .first()
        .and_then(|c| c.base_dir.clone())
        .unwrap_or_default();

    // All the hooks, to resolve the references
    let mut entries = Vec::new();
    for config in configs {
        let base_dir = config.base_dir.clone().unwrap_or_default();
        let project = project_name(&root, &base_dir);
        for hook in config.hooks.iter().flatten() {
            entries.push(Entry {
                hook,
                project: project.clone(),
                base_dir: base_dir.clone(),
            });
        }
    }

    crate::variables::check(configs)?;
    let denials = crate::variables::denials(configs)?;
    let resolver = Resolver {
        entries: &entries,
        trigger,
    };

    // Hooks to run: the matching ones, then the ones they wait for
    let mut files: HashMap<usize, Vec<PathBuf>> = HashMap::new();
    let mut queue = VecDeque::new();
    for matched in selected {
        let Some(index) = entries
            .iter()
            .position(|e| std::ptr::eq(e.hook, matched.hook))
        else {
            continue;
        };
        if files.insert(index, matched.files.clone()).is_none() {
            queue.push_back(index);
        }
    }
    while let Some(index) = queue.pop_front() {
        for target in resolver.references_of(index)? {
            let dependency = match target {
                Target::Hook(i) | Target::Step(i, _) => i,
            };
            if dependency != index && !files.contains_key(&dependency) {
                files.insert(dependency, vec![]);
                queue.push_back(dependency);
            }
        }
    }

    // One node per hook, or per step
    let mut included: Vec<usize> = files.keys().copied().collect();
    included.sort();
    let mut nodes = Vec::new();
    let mut node_of: HashMap<Target, usize> = HashMap::new();
    let mut nodes_of_hook: HashMap<usize, Vec<usize>> = HashMap::new();
    for &index in &included {
        let entry = &entries[index];
        let steps: Vec<Option<(usize, &Step)>> = match entry.hook.steps.as_ref() {
            Some(steps) => steps.iter().enumerate().map(Some).collect(),
            None => vec![None],
        };
        for step in steps {
            let id = nodes.len();
            match step {
                Some((s, _)) => node_of.insert(Target::Step(index, s), id),
                None => node_of.insert(Target::Hook(index), id),
            };
            nodes_of_hook.entry(index).or_default().push(id);
            nodes.push(Node {
                hook: entry.hook,
                step: step.map(|(_, s)| s),
                project: entry.project.clone(),
                base_dir: entry.base_dir.clone(),
                files: files[&index].clone(),
                needs: vec![],
                inputs: vec![],
                outputs: step
                    .map(|(_, s)| s.outputs.clone())
                    .unwrap_or_else(|| entry.hook.outputs.clone())
                    .unwrap_or_default(),
                reads: entry.hook.reads.clone().unwrap_or_default(),
                writes: entry.hook.writes.clone().unwrap_or_default(),
                after: vec![],
                denied: denials
                    .get(&format!("{}:{}", entry.project, entry.hook.name))
                    .cloned()
                    .or_else(|| untrusted(&root, entry.hook)),
            });
        }
    }

    // The nodes of a target (all the steps for a hook with steps)
    let targets_nodes = |target: Target| -> Vec<usize> {
        match target {
            Target::Step(..) => node_of.get(&target).copied().into_iter().collect(),
            Target::Hook(i) => nodes_of_hook.get(&i).cloned().unwrap_or_default(),
        }
    };

    // Liens
    for &index in &included {
        let entry = &entries[index];
        let hook = entry.hook;
        let hook_nodes = nodes_of_hook[&index].clone();

        // `needs` and `inputs` of the hook: they apply to all its steps
        let mut hook_needs = Vec::new();
        for reference in hook.needs.iter().flatten() {
            let target = resolver.resolve(index, None, reference, &hook.name)?;
            hook_needs.extend(targets_nodes(target));
        }
        let mut hook_inputs = Vec::new();
        for (name, source) in hook.inputs.iter().flatten() {
            let (node, output) =
                resolve_input(&resolver, &nodes, &node_of, index, None, source, &hook.name)?;
            hook_inputs.push(Input {
                name: name.clone(),
                node,
                output,
            });
        }

        for (position, &node) in hook_nodes.iter().enumerate() {
            let mut needs = hook_needs.clone();
            let mut inputs = hook_inputs.clone();

            if let Some(step) = nodes[node].step {
                let label = format!("{}/{}", hook.name, step.id);
                for reference in step.needs.iter().flatten() {
                    let target = resolver.resolve(index, Some(position), reference, &label)?;
                    needs.extend(targets_nodes(target));
                }
                for (name, source) in step.inputs.iter().flatten() {
                    let (from, output) = resolve_input(
                        &resolver,
                        &nodes,
                        &node_of,
                        index,
                        Some(position),
                        source,
                        &label,
                    )?;
                    inputs.retain(|i: &Input| i.name != *name);
                    inputs.push(Input {
                        name: name.clone(),
                        node: from,
                        output,
                    });
                }
            }

            needs.extend(inputs.iter().map(|i| i.node));
            let mut seen = HashSet::new();
            needs.retain(|n| *n != node && seen.insert(*n));
            nodes[node].needs = needs;
            nodes[node].inputs = inputs;
        }
    }

    // Readers of a variable run after its writers (a hook reading and writing it, like a
    // counter, doesn't wait for the other ones doing the same)
    for reader in 0..nodes.len() {
        let mut after: Vec<usize> = (0..nodes.len())
            .filter(|&writer| {
                writer != reader
                    && !nodes[reader].needs.contains(&writer)
                    && nodes[reader].reads.iter().any(|v| {
                        nodes[writer].writes.contains(v)
                            && !(nodes[reader].writes.contains(v)
                                && nodes[writer].reads.contains(v))
                    })
            })
            .collect();
        // Steps of a same hook share its variables: no order between them for that
        after.retain(|&w| !std::ptr::eq(nodes[w].hook, nodes[reader].hook));
        nodes[reader].after = after;
    }

    sort(nodes)
}

/// Reason why a hook of another repository can't run yet
fn untrusted(root: &Path, hook: &Hook) -> Option<String> {
    let origin = hook.origin.as_ref()?;
    if std::env::var_os(crate::trust::TRUST_ALL_ENV).is_some_and(|v| v == "1")
        || crate::trust::is_trusted(root, origin)
    {
        return None;
    }
    Some(format!(
        "{} ({}) isn't trusted yet: review it with `hooker trust`",
        origin.path, origin.url
    ))
}

/// Resolves `hook.output` into (node, output)
fn resolve_input(
    resolver: &Resolver,
    nodes: &[Node],
    node_of: &HashMap<Target, usize>,
    index: usize,
    step: Option<usize>,
    source: &str,
    label: &str,
) -> Result<(usize, String), GoatHookerError> {
    let Some((reference, output)) = source.rsplit_once('.') else {
        return Err(error(
            label,
            format!("input `{source}` must look like `hook.output`"),
        ));
    };
    let target = resolver.resolve(index, step, reference, label)?;
    let node = match target {
        Target::Step(..) => node_of[&target],
        Target::Hook(i) => match node_of.get(&Target::Hook(i)) {
            Some(node) => *node,
            None => {
                return Err(error(
                    label,
                    format!(
                        "`{reference}` has steps, take the output of one of them (`{reference}/<step>.{output}`)"
                    ),
                ));
            }
        },
    };
    let declared = &nodes[node].outputs;
    if !BUILTIN_OUTPUTS.contains(&output) && !declared.iter().any(|o| o == output) {
        return Err(error(
            label,
            format!(
                "`{reference}` has no output `{output}` (outputs: {})",
                BUILTIN_OUTPUTS
                    .iter()
                    .map(|o| o.to_string())
                    .chain(declared.iter().cloned())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    }
    Ok((node, output.to_string()))
}

struct Resolver<'e, 'a> {
    entries: &'e [Entry<'a>],
    trigger: Trigger,
}

impl Resolver<'_, '_> {
    /// All the targets referenced by a hook (and its steps)
    fn references_of(&self, index: usize) -> Result<Vec<Target>, GoatHookerError> {
        let hook = self.entries[index].hook;
        let mut targets = Vec::new();
        let input_ref = |source: &String| -> String {
            source
                .rsplit_once('.')
                .map(|(r, _)| r.to_string())
                .unwrap_or_else(|| source.clone())
        };

        for reference in hook.needs.iter().flatten() {
            targets.push(self.resolve(index, None, reference, &hook.name)?);
        }
        for source in hook.inputs.iter().flatten().map(|(_, s)| s) {
            targets.push(self.resolve(index, None, &input_ref(source), &hook.name)?);
        }
        for (position, step) in hook.steps.iter().flatten().enumerate() {
            let label = format!("{}/{}", hook.name, step.id);
            for reference in step.needs.iter().flatten() {
                targets.push(self.resolve(index, Some(position), reference, &label)?);
            }
            for source in step.inputs.iter().flatten().map(|(_, s)| s) {
                targets.push(self.resolve(index, Some(position), &input_ref(source), &label)?);
            }
        }
        Ok(targets)
    }

    /// Resolves a reference seen from the hook `index` (and its step `step`)
    fn resolve(
        &self,
        index: usize,
        step: Option<usize>,
        reference: &str,
        label: &str,
    ) -> Result<Target, GoatHookerError> {
        let entry = &self.entries[index];
        let (project, path) = match reference.split_once(':') {
            Some((project, path)) => (normalize_project(project), path),
            None => (entry.project.clone(), reference),
        };
        let (hook_name, step_id) = match path.split_once('/') {
            Some((hook, step)) => (hook, Some(step)),
            None => (path, None),
        };

        // From a step, `types` is first a sibling step
        if step.is_some()
            && !reference.contains([':', '/'])
            && let Some(position) = entry
                .hook
                .steps
                .iter()
                .flatten()
                .position(|s| s.id == hook_name)
        {
            return Ok(Target::Step(index, position));
        }

        let Some(target) = self
            .entries
            .iter()
            .position(|e| e.project == project && e.hook.name == hook_name)
        else {
            return Err(error(
                label,
                format!("`{reference}`: no hook `{hook_name}` in project `{project}`"),
            ));
        };

        let target_hook = self.entries[target].hook;
        if target_hook.trigger != Some(self.trigger) {
            return Err(error(
                label,
                format!(
                    "`{reference}` is a {} hook, it can only wait for {} hooks",
                    target_hook.trigger.map(|t| t.as_str()).unwrap_or("-"),
                    self.trigger.as_str()
                ),
            ));
        }

        match step_id {
            None => Ok(Target::Hook(target)),
            Some(step_id) => target_hook
                .steps
                .iter()
                .flatten()
                .position(|s| s.id == step_id)
                .map(|position| Target::Step(target, position))
                .ok_or_else(|| {
                    error(
                        label,
                        format!("`{reference}`: `{hook_name}` has no step `{step_id}`"),
                    )
                }),
        }
    }
}

/// Topological sort (stable: order of the file on ties), with detection of the cycles
fn sort(mut nodes: Vec<Node>) -> Result<Graph, GoatHookerError> {
    let count = nodes.len();
    let mut remaining: Vec<usize> = nodes
        .iter()
        .map(|n| n.needs.len() + n.after.len())
        .collect();
    let mut dependents = vec![Vec::new(); count];
    for (node, n) in nodes.iter().enumerate() {
        for &need in n.needs.iter().chain(&n.after) {
            dependents[need].push(node);
        }
    }

    let mut ready: Vec<usize> = (0..count).filter(|&n| remaining[n] == 0).collect();
    let mut order = Vec::with_capacity(count);
    while let Some(node) = ready.first().copied() {
        ready.remove(0);
        order.push(node);
        for &dependent in &dependents[node] {
            remaining[dependent] -= 1;
            if remaining[dependent] == 0 {
                let position = ready.partition_point(|&r| r < dependent);
                ready.insert(position, dependent);
            }
        }
    }

    if order.len() < count {
        let cycle: Vec<String> = (0..count)
            .filter(|&n| remaining[n] > 0)
            .map(|n| format!("`{}`", nodes[n].label()))
            .collect();
        return Err(error(
            &nodes[(0..count).find(|&n| remaining[n] > 0).unwrap()]
                .hook
                .name
                .clone(),
            format!("dependency cycle between {}", cycle.join(", ")),
        ));
    }

    // Numbers the nodes again in the found order
    let mut new_index = vec![0; count];
    for (new, &old) in order.iter().enumerate() {
        new_index[old] = new;
    }
    for node in &mut nodes {
        for need in node.needs.iter_mut().chain(node.after.iter_mut()) {
            *need = new_index[*need];
        }
        for input in &mut node.inputs {
            input.node = new_index[input.node];
        }
    }
    let mut slots: Vec<Option<Node>> = nodes.into_iter().map(Some).collect();
    let nodes = order
        .iter()
        .map(|&old| slots[old].take().unwrap())
        .collect();
    Ok(Graph { nodes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::ConfigLoader, matcher::Matcher};
    use std::fs;
    use tempfile::tempdir;

    /// Configs of a test monorepo: (folder, json)
    fn load(files: &[(&str, &str)]) -> (tempfile::TempDir, Vec<Config>) {
        let dir = tempdir().unwrap();
        for (folder, json) in files {
            let path = dir.path().join(folder);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("hooker.config.json"), json).unwrap();
        }
        let configs = ConfigLoader::new()
            .load_recursive(&dir.path().join("hooker.config.json"))
            .unwrap();
        (dir, configs)
    }

    /// Graph of all the pre-commit hooks
    fn graph_of(configs: &[Config]) -> Result<Graph<'_>, GoatHookerError> {
        let matcher = Matcher::new(configs).unwrap();
        let hooks = matcher.hooks_to_run(Trigger::PreCommit, None);
        build(configs, Trigger::PreCommit, &hooks)
    }

    fn labels(graph: &Graph) -> Vec<String> {
        graph.nodes.iter().map(|n| n.label()).collect()
    }

    #[test]
    fn order_follows_needs() {
        let (_dir, configs) = load(&[(
            ".",
            r#"{ "hooks": [
                { "name": "lint", "trigger": "pre-commit", "needs": ["codegen"], "cmd": "true" },
                { "name": "codegen", "trigger": "pre-commit", "cmd": "true" }
            ] }"#,
        )]);
        let graph = graph_of(&configs).unwrap();
        assert_eq!(labels(&graph), vec!["codegen", "lint"]);
        assert_eq!(graph.nodes[1].needs, vec![0]);
    }

    #[test]
    fn steps_and_inputs() {
        let (_dir, configs) = load(&[(
            ".",
            r#"{ "hooks": [
                { "name": "codegen", "trigger": "pre-commit", "outputs": ["schema"], "cmd": "true" },
                { "name": "lint", "trigger": "pre-commit", "steps": [
                    { "id": "types", "needs": ["eslint"], "inputs": { "schema": "codegen.schema" }, "cmd": "true" },
                    { "id": "eslint", "cmd": "true" }
                ] }
            ] }"#,
        )]);
        let graph = graph_of(&configs).unwrap();
        assert_eq!(
            labels(&graph),
            vec!["codegen", "lint › eslint", "lint › types"]
        );

        let types = &graph.nodes[2];
        assert_eq!(types.needs, vec![1, 0]);
        assert_eq!(
            types.inputs,
            vec![Input {
                name: "schema".into(),
                node: 0,
                output: "schema".into()
            }]
        );
    }

    #[test]
    fn references_across_projects() {
        let (_dir, configs) = load(&[
            (".", r#"{ "imports": ["api", "web"], "hooks": [] }"#),
            (
                "api",
                r#"{ "hooks": [ { "name": "codegen", "trigger": "pre-commit", "include": ["*.proto"], "cmd": "true" } ] }"#,
            ),
            (
                "web",
                r#"{ "hooks": [ { "name": "lint", "trigger": "pre-commit", "inputs": { "generated": "api:codegen.files" }, "cmd": "true" } ] }"#,
            ),
        ]);

        // only `lint` matches: `codegen` is added as a dependency, without files
        let matcher = Matcher::new(&configs).unwrap();
        let lint: Vec<_> = matcher
            .hooks_to_run(Trigger::PreCommit, None)
            .into_iter()
            .filter(|m| m.hook.name == "lint")
            .collect();
        let graph = build(&configs, Trigger::PreCommit, &lint).unwrap();

        assert_eq!(labels(&graph), vec!["codegen", "lint"]);
        assert_eq!(graph.nodes[0].project, "api");
        assert_eq!(graph.nodes[0].reference(), "api:codegen");
        assert_eq!(graph.nodes[1].inputs[0].output, "files");
    }

    #[test]
    fn needs_on_a_hook_with_steps_waits_for_all_steps() {
        let (_dir, configs) = load(&[(
            ".",
            r#"{ "hooks": [
                { "name": "lint", "trigger": "pre-commit", "steps": [
                    { "id": "a", "cmd": "true" }, { "id": "b", "cmd": "true" }
                ] },
                { "name": "test", "trigger": "pre-commit", "needs": ["lint"], "cmd": "true" }
            ] }"#,
        )]);
        let graph = graph_of(&configs).unwrap();
        assert_eq!(graph.nodes[2].label(), "test");
        assert_eq!(graph.nodes[2].needs, vec![0, 1]);
    }

    #[test]
    fn errors() {
        let cases = [
            (
                r#"{ "name": "a", "trigger": "pre-commit", "needs": ["nope"], "cmd": "true" }"#,
                "no hook `nope`",
            ),
            (
                r#"{ "name": "a", "trigger": "pre-commit", "needs": ["msg"], "cmd": "true" }"#,
                "is a commit-msg hook",
            ),
            (
                r#"{ "name": "a", "trigger": "pre-commit", "inputs": { "x": "b.version" }, "cmd": "true" }"#,
                "has no output `version`",
            ),
            (
                r#"{ "name": "a", "trigger": "pre-commit", "needs": ["b"], "cmd": "true" }"#,
                "dependency cycle",
            ),
        ];
        for (hook, expected) in cases {
            let json = format!(
                r#"{{ "hooks": [ {hook},
                    {{ "name": "b", "trigger": "pre-commit", "needs": ["a"], "cmd": "true" }},
                    {{ "name": "msg", "trigger": "commit-msg", "cmd": "true" }}
                ] }}"#
            );
            let (_dir, configs) = load(&[(".", &json)]);
            let err = graph_of(&configs).unwrap_err().to_string();
            assert!(err.contains(expected), "{err} (expected {expected})");
        }
    }

    #[test]
    fn invalid_links_are_rejected_at_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("hooker.config.json");
        for json in [
            r#"{ "name": "a", "trigger": "pre-commit", "outputs": ["files"], "cmd": "true" }"#,
            r#"{ "name": "a", "trigger": "pre-commit", "inputs": { "my-var": "b.files" }, "cmd": "true" }"#,
            r#"{ "name": "a", "trigger": "pre-commit", "steps": [ { "id": "x", "cmd": "a" }, { "id": "x", "cmd": "b" } ] }"#,
            r#"{ "name": "a", "trigger": "pre-commit", "cmd": "a", "steps": [ { "id": "x", "cmd": "b" } ] }"#,
        ] {
            fs::write(&path, format!(r#"{{ "hooks": [ {json} ] }}"#)).unwrap();
            assert!(Config::load_from_file(&path).is_err(), "{json}");
        }
    }
}
