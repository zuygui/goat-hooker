//! Graph shown in the node editor: the nodes of a trigger, placed automatically,
//! and the changes of the links (`needs`, `inputs`) written to the `hooker.config.json` files.

use std::path::{Path, PathBuf};

use goat_hooker::{
    config::{BUILTIN_OUTPUTS, Hook, Trigger},
    errors::GoatHookerError,
    graph,
    matcher::MatchedHook,
};

use crate::repo::Repo;

/// A node of the editor
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewNode {
    /// `lint` or `lint › types` (like in the results)
    pub label: String,
    pub hook: String,
    pub step: Option<String>,
    /// `root` or relative folder
    pub project: String,
    pub project_dir: PathBuf,
    /// Outputs: `files`, `stdout`, `status` then the declared outputs
    pub outputs: Vec<String>,
    /// Inputs: (name, source node, output)
    pub inputs: Vec<(String, usize, String)>,
    /// Nodes it waits for without receiving a value
    pub needs: Vec<usize>,
    /// Variables read and written, and the nodes writing what this one reads
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub after: Vec<usize>,
    /// Why it isn't allowed to run (its lineage is cut)
    pub denied: Option<String>,
    /// Hook written in a `hooker.config.json` of the repository (not loaded from a shared file)
    pub editable: bool,
    /// Column (dependency level) and row in the column
    pub column: usize,
    pub row: usize,
}

impl ViewNode {
    /// A node without links (for names computed before it exists)
    pub fn empty() -> Self {
        Self::default()
    }
}

/// Link to add
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// The target waits for the source
    Needs,
    /// The target receives the output `output` of the source, named `name`
    Input { name: String, output: String },
}

impl Repo {
    /// Nodes of a trigger (all the hooks, without filtering on the files)
    pub fn graph_view(&self, trigger: Trigger) -> Result<Vec<ViewNode>, GoatHookerError> {
        let configs = self.load_configs()?;
        let mut selected = Vec::new();
        for config in &configs {
            let base_dir = config.base_dir.clone().unwrap_or_default();
            for hook in config.hooks.iter().flatten() {
                if hook.trigger == Some(trigger) {
                    selected.push(MatchedHook {
                        hook,
                        base_dir: base_dir.clone(),
                        files: vec![],
                    });
                }
            }
        }
        let graph = graph::build(&configs, trigger, &selected)?;

        let mut nodes: Vec<ViewNode> = graph
            .nodes
            .iter()
            .map(|node| {
                let inputs: Vec<(String, usize, String)> = node
                    .inputs
                    .iter()
                    .map(|i| (i.name.clone(), i.node, i.output.clone()))
                    .collect();
                ViewNode {
                    label: node.label(),
                    hook: node.hook.name.clone(),
                    step: node.step.map(|s| s.id.clone()),
                    project: node.project.clone(),
                    project_dir: node.base_dir.clone(),
                    outputs: BUILTIN_OUTPUTS
                        .iter()
                        .map(|o| o.to_string())
                        .chain(node.outputs.iter().cloned())
                        .collect(),
                    needs: node
                        .needs
                        .iter()
                        .copied()
                        .filter(|need| !inputs.iter().any(|(_, from, _)| from == need))
                        .collect(),
                    inputs,
                    reads: node.reads.clone(),
                    writes: node.writes.clone(),
                    after: node.after.clone(),
                    denied: node.denied.clone(),
                    editable: self.raw_hook(&node.base_dir, &node.hook.name).is_some(),
                    column: 0,
                    row: 0,
                }
            })
            .collect();

        // Columns: a node is right of all the ones it waits for (the nodes are sorted)
        let mut columns: Vec<Vec<usize>> = Vec::new();
        for index in 0..nodes.len() {
            let column = graph.nodes[index]
                .needs
                .iter()
                .chain(&graph.nodes[index].after)
                .map(|&need| nodes[need].column + 1)
                .max()
                .unwrap_or(0);
            if columns.len() <= column {
                columns.resize(column + 1, Vec::new());
            }
            nodes[index].column = column;
            columns[column].push(index);
        }

        // Rows: each node near the ones it is linked to, to avoid crossed wires
        let needs: Vec<&Vec<usize>> = graph.nodes.iter().map(|n| &n.needs).collect();
        let mut row = vec![0usize; nodes.len()];
        let place = |columns: &mut Vec<Vec<usize>>, row: &mut Vec<usize>, from_dependents: bool| {
            let order: Vec<usize> = if from_dependents {
                vec![0]
            } else {
                (1..columns.len()).collect()
            };
            for c in order {
                let mut keyed: Vec<(f32, usize)> = columns[c]
                    .iter()
                    .map(|&i| {
                        let linked: Vec<usize> = if from_dependents {
                            (0..needs.len())
                                .filter(|&d| needs[d].contains(&i))
                                .collect()
                        } else {
                            needs[i]
                                .iter()
                                .chain(&graph.nodes[i].after)
                                .copied()
                                .collect()
                        };
                        let key = if linked.is_empty() {
                            f32::MAX
                        } else {
                            linked.iter().map(|&l| row[l] as f32).sum::<f32>() / linked.len() as f32
                        };
                        (key, i)
                    })
                    .collect();
                keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
                columns[c] = keyed.into_iter().map(|(_, i)| i).collect();
                for (r, &i) in columns[c].iter().enumerate() {
                    row[i] = r;
                }
            }
        };
        for (r, &i) in columns.first().into_iter().flatten().enumerate() {
            row[i] = r;
        }
        place(&mut columns, &mut row, false);
        if columns.len() > 1 {
            place(&mut columns, &mut row, true);
            place(&mut columns, &mut row, false);
        }
        for (index, node) in nodes.iter_mut().enumerate() {
            node.row = row[index];
        }

        Ok(nodes)
    }

    /// Project (index) and hook (index) written in the repository for a folder and a name
    fn raw_hook(&self, dir: &Path, name: &str) -> Option<(usize, usize)> {
        let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        self.projects.iter().enumerate().find_map(|(p, project)| {
            let project_dir = project.dir();
            let project_dir = project_dir.canonicalize().unwrap_or(project_dir);
            if project_dir != dir {
                return None;
            }
            project
                .hooks()
                .iter()
                .position(|h| h.name == name && !h.is_import())
                .map(|h| (p, h))
        })
    }

    /// Adds a link `source → target`, then checks the graph (reverted on error, e.g. a cycle)
    pub fn add_link(
        &mut self,
        trigger: Trigger,
        target: &ViewNode,
        source: &ViewNode,
        link: Link,
    ) -> Result<(), GoatHookerError> {
        let reference = reference_from(target, source);
        self.edit_hook(trigger, target, |hook, step| {
            let (needs, inputs) = match step {
                Some(id) => {
                    let step = hook
                        .steps
                        .iter_mut()
                        .flatten()
                        .find(|s| s.id == id)
                        .unwrap();
                    (&mut step.needs, &mut step.inputs)
                }
                None => (&mut hook.needs, &mut hook.inputs),
            };
            match link {
                Link::Needs => {
                    let needs = needs.get_or_insert_with(Vec::new);
                    if !needs.contains(&reference) {
                        needs.push(reference.clone());
                    }
                }
                Link::Input { name, output } => {
                    inputs
                        .get_or_insert_with(Default::default)
                        .insert(name, format!("{reference}.{output}"));
                }
            }
        })
    }

    /// Removes `source` from the `needs` of `target`
    pub fn remove_need(
        &mut self,
        trigger: Trigger,
        target: &ViewNode,
        source: &ViewNode,
    ) -> Result<(), GoatHookerError> {
        let forms = reference_forms(target, source);
        self.edit_hook(trigger, target, |hook, step| {
            let needs = match step {
                Some(id) => {
                    &mut hook
                        .steps
                        .iter_mut()
                        .flatten()
                        .find(|s| s.id == id)
                        .unwrap()
                        .needs
                }
                None => &mut hook.needs,
            };
            if let Some(list) = needs.as_mut() {
                list.retain(|r| !forms.contains(r));
                if list.is_empty() {
                    *needs = None;
                }
            }
        })
    }

    /// Removes the input `name` of `target`
    pub fn remove_input(
        &mut self,
        trigger: Trigger,
        target: &ViewNode,
        name: &str,
    ) -> Result<(), GoatHookerError> {
        let name = name.to_string();
        self.edit_hook(trigger, target, move |hook, step| {
            let inputs = match step {
                Some(id) => {
                    &mut hook
                        .steps
                        .iter_mut()
                        .flatten()
                        .find(|s| s.id == id)
                        .unwrap()
                        .inputs
                }
                None => &mut hook.inputs,
            };
            if let Some(map) = inputs.as_mut() {
                map.remove(&name);
                if map.is_empty() {
                    *inputs = None;
                }
            }
        })
    }

    /// Creates a hook of the trigger in a project (from the canvas)
    pub fn create_hook(
        &mut self,
        trigger: Trigger,
        project: usize,
        name: &str,
        cmd: &str,
    ) -> Result<(), GoatHookerError> {
        let hook = Hook {
            name: name.trim().to_string(),
            trigger: Some(trigger),
            cmd: Some(cmd.trim().to_string()),
            ..Default::default()
        };
        hook.validate()?;
        let project = &mut self.projects[project];
        if project.hooks().iter().any(|h| h.name == hook.name) {
            return Err(GoatHookerError::InvalidHook {
                name: hook.name,
                reason: "this project already has a hook with this name".to_string(),
            });
        }
        project.save_hook(None, hook)
    }

    /// Deletes a node: its hook, or its step. Refused (and reverted) when other hooks use it.
    pub fn delete_node(
        &mut self,
        trigger: Trigger,
        node: &ViewNode,
    ) -> Result<(), GoatHookerError> {
        let Some((project, index)) = self.raw_hook(&node.project_dir, &node.hook) else {
            return Err(GoatHookerError::InvalidHook {
                name: node.hook.clone(),
                reason: "this hook comes from a shared hooks file, remove the hook loading it"
                    .to_string(),
            });
        };
        let backup = self.projects[project].raw.clone();
        let mut hook = self.projects[project].hooks()[index].clone();
        let delete_hook = match node.step.as_ref() {
            Some(step) => {
                let steps = hook.steps.get_or_insert_with(Vec::new);
                steps.retain(|s| s.id != *step);
                steps.is_empty()
            }
            None => true,
        };
        if delete_hook {
            self.projects[project].delete_hook(index)?;
        } else {
            self.projects[project].save_hook(Some(index), hook)?;
        }

        if let Err(err) = self.graph_view(trigger) {
            let file = self.projects[project].file.clone();
            self.projects[project].raw = backup;
            self.projects[project].raw.write_to_file(&file)?;
            return Err(GoatHookerError::InvalidHook {
                name: node.label.clone(),
                reason: format!("other hooks use it, remove their links first ({err})"),
            });
        }
        Ok(())
    }

    /// Changes a hook written in the repository, saves it, and checks that the graph is still valid
    fn edit_hook(
        &mut self,
        trigger: Trigger,
        target: &ViewNode,
        edit: impl FnOnce(&mut Hook, Option<&str>),
    ) -> Result<(), GoatHookerError> {
        let Some((project, index)) = self.raw_hook(&target.project_dir, &target.hook) else {
            return Err(GoatHookerError::InvalidHook {
                name: target.hook.clone(),
                reason: "this hook comes from a shared hooks file, edit it there".to_string(),
            });
        };

        let backup = self.projects[project].raw.clone();
        let mut hook = self.projects[project].hooks()[index].clone();
        edit(&mut hook, target.step.as_deref());
        self.projects[project].save_hook(Some(index), hook)?;

        if let Err(err) = self.graph_view(trigger) {
            let file = self.projects[project].file.clone();
            self.projects[project].raw = backup;
            self.projects[project].raw.write_to_file(&file)?;
            return Err(err);
        }
        Ok(())
    }
}

/// Reference to `source`, written in the config of `target`
fn reference_from(target: &ViewNode, source: &ViewNode) -> String {
    if source.project_dir == target.project_dir {
        match (&source.step, source.hook == target.hook) {
            (Some(step), true) => step.clone(),
            (Some(step), false) => format!("{}/{step}", source.hook),
            (None, _) => source.hook.clone(),
        }
    } else {
        let step = source
            .step
            .as_ref()
            .map(|s| format!("/{s}"))
            .unwrap_or_default();
        format!("{}:{}{step}", source.project, source.hook)
    }
}

/// Every way to write the reference to `source` (to remove it)
fn reference_forms(target: &ViewNode, source: &ViewNode) -> Vec<String> {
    let step = source
        .step
        .as_ref()
        .map(|s| format!("/{s}"))
        .unwrap_or_default();
    let mut forms = vec![
        reference_from(target, source),
        format!("{}{step}", source.hook),
        format!("{}:{}{step}", source.project, source.hook),
    ];
    if source.project == "root" {
        forms.push(format!(".:{}{step}", source.hook));
    }
    if let Some(s) = source.step.as_ref() {
        forms.push(s.clone());
    }
    forms
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, process::Command};

    fn setup() -> (tempfile::TempDir, Repo) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(&root)
            .status()
            .unwrap();
        fs::write(
            root.join("hooker.config.json"),
            r#"{ "imports": ["api"], "hooks": [
                { "name": "lint", "trigger": "pre-commit", "steps": [
                    { "id": "eslint", "cmd": "true" }, { "id": "types", "cmd": "true" }
                ] },
                { "name": "test", "trigger": "pre-commit", "needs": ["lint"], "cmd": "true" }
            ] }"#,
        )
        .unwrap();
        fs::create_dir_all(root.join("api")).unwrap();
        fs::write(
            root.join("api/hooker.config.json"),
            r#"{ "hooks": [ { "name": "codegen", "trigger": "pre-commit", "outputs": ["schema"], "cmd": "true" } ] }"#,
        )
        .unwrap();
        let repo = Repo::open(&root).unwrap();
        (dir, repo)
    }

    fn node<'a>(nodes: &'a [ViewNode], label: &str) -> &'a ViewNode {
        nodes.iter().find(|n| n.label == label).unwrap()
    }

    #[test]
    fn layout_in_columns() {
        let (_dir, repo) = setup();
        let nodes = repo.graph_view(Trigger::PreCommit).unwrap();
        let columns: Vec<(String, usize, usize)> = nodes
            .iter()
            .map(|n| (n.label.clone(), n.column, n.row))
            .collect();
        assert_eq!(
            columns,
            vec![
                ("lint › eslint".into(), 0, 0),
                ("lint › types".into(), 0, 1),
                ("test".into(), 1, 0),
                ("codegen".into(), 0, 2),
            ]
        );
        assert_eq!(
            node(&nodes, "codegen").outputs,
            vec!["files", "stdout", "status", "schema"]
        );
        assert!(node(&nodes, "test").editable);
    }

    #[test]
    fn edit_links() {
        let (_dir, mut repo) = setup();
        let trigger = Trigger::PreCommit;
        let nodes = repo.graph_view(trigger).unwrap();

        // input from another project + wait for a sibling step
        repo.add_link(
            trigger,
            node(&nodes, "lint › types"),
            node(&nodes, "codegen"),
            Link::Input {
                name: "schema".into(),
                output: "schema".into(),
            },
        )
        .unwrap();
        repo.add_link(
            trigger,
            node(&nodes, "lint › types"),
            node(&nodes, "lint › eslint"),
            Link::Needs,
        )
        .unwrap();

        repo.reload().unwrap();
        let lint = &repo.projects[0].hooks()[0];
        let types = &lint.steps.as_ref().unwrap()[1];
        assert_eq!(types.needs, Some(vec!["eslint".to_string()]));
        assert_eq!(
            types.inputs.as_ref().unwrap().get("schema").unwrap(),
            "api:codegen.schema"
        );

        let nodes = repo.graph_view(trigger).unwrap();
        let types = node(&nodes, "lint › types");
        assert_eq!(types.inputs[0].0, "schema");
        assert_eq!(types.column, 1);

        // remove the links
        repo.remove_input(trigger, types, "schema").unwrap();
        repo.remove_need(trigger, types, node(&nodes, "lint › eslint"))
            .unwrap();
        repo.reload().unwrap();
        let types = &repo.projects[0].hooks()[0].steps.as_ref().unwrap()[1];
        assert!(types.needs.is_none() && types.inputs.is_none());
    }

    #[test]
    fn cycles_are_refused_and_reverted() {
        let (_dir, mut repo) = setup();
        let trigger = Trigger::PreCommit;
        let nodes = repo.graph_view(trigger).unwrap();

        // `lint` waits for `test`, which already waits for `lint` → cycle
        let err = repo
            .add_link(
                trigger,
                node(&nodes, "lint › eslint"),
                node(&nodes, "test"),
                Link::Needs,
            )
            .unwrap_err();
        assert!(err.to_string().contains("cycle"), "{err}");

        repo.reload().unwrap();
        assert!(repo.graph_view(trigger).is_ok());
    }
}
