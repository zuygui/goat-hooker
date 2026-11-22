//! Graphe affiché dans l'éditeur de nœuds : les nœuds d'un trigger, placés automatiquement,
//! et les modifications des liens (`needs`, `inputs`) écrites dans les `hooker.config.json`.

use std::path::{Path, PathBuf};

use goat_hooker::{
    config::{BUILTIN_OUTPUTS, Hook, Trigger},
    errors::GoatHookerError,
    graph,
    matcher::MatchedHook,
};

use crate::repo::Repo;

/// Un nœud de l'éditeur
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewNode {
    /// `lint` ou `lint › types` (comme dans les résultats)
    pub label: String,
    pub hook: String,
    pub step: Option<String>,
    /// `root` ou dossier relatif
    pub project: String,
    pub project_dir: PathBuf,
    /// Sorties : `files`, `stdout`, `status` puis les sorties déclarées
    pub outputs: Vec<String>,
    /// Entrées : (nom, nœud source, sortie)
    pub inputs: Vec<(String, usize, String)>,
    /// Nœuds attendus sans passer de valeur
    pub needs: Vec<usize>,
    /// Variables read and written, and the nodes writing what this one reads
    pub reads: Vec<String>,
    pub writes: Vec<String>,
    pub after: Vec<usize>,
    /// Hook écrit dans un `hooker.config.json` du dépôt (pas chargé depuis un fichier partagé)
    pub editable: bool,
    /// Colonne (niveau de dépendance) et ligne dans la colonne
    pub column: usize,
    pub row: usize,
}

impl ViewNode {
    /// A node without links (for names computed before it exists)
    pub fn empty() -> Self {
        Self::default()
    }
}

/// Lien à ajouter
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// La cible attend la source
    Needs,
    /// La cible reçoit la sortie `output` de la source, sous le nom `name`
    Input { name: String, output: String },
}

impl Repo {
    /// Nœuds d'un trigger (tous les hooks, sans filtre sur les fichiers)
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
                    editable: self.raw_hook(&node.base_dir, &node.hook.name).is_some(),
                    column: 0,
                    row: 0,
                }
            })
            .collect();

        // Colonnes : un nœud est à droite de tous ceux qu'il attend (les nœuds sont triés)
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

        // Lignes : chaque nœud près de ceux auxquels il est relié, pour éviter les fils croisés
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

    /// Projet (index) et hook (index) écrits dans le dépôt pour un dossier et un nom
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

    /// Ajoute un lien `source → target`, puis vérifie le graphe (annulé en cas d'erreur, ex : cycle)
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

    /// Retire `source` des `needs` de `target`
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

    /// Retire l'entrée `name` de `target`
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

    /// Modifie un hook écrit dans le dépôt, enregistre, et vérifie que le graphe est toujours valide
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

/// Référence vers `source`, écrite dans la config de `target`
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

/// Toutes les façons d'écrire la référence vers `source` (pour la retirer)
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

        // entrée depuis un autre projet + attente d'une étape sœur
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

        // retirer les liens
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

        // `lint` attend `test`, qui attend déjà `lint` → cycle
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
