//! History of the runs: each run of a trigger (by Git, `hooker run` or the app) is added to
//! `.git/goat-hooker/history.jsonl`, with the result and the duration of each hook.

use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::{config::Trigger, errors::GoatHookerError, git, runner::HookResult};

/// Runs kept (the oldest ones are dropped)
const MAX_RUNS: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Passed,
    Failed,
    Skipped,
    Denied,
    Cached,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookRun {
    pub name: String,
    pub project: String,
    pub status: Status,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Run {
    /// Seconds since the epoch
    pub at: u64,
    pub trigger: String,
    pub branch: Option<String>,
    pub duration_ms: u64,
    pub hooks: Vec<HookRun>,
}

impl Run {
    pub fn passed(&self) -> bool {
        self.hooks
            .iter()
            .all(|h| matches!(h.status, Status::Passed | Status::Cached))
    }
}

fn file(root: &Path) -> Result<PathBuf, GoatHookerError> {
    Ok(git::data_dir(root)?.join("history.jsonl"))
}

/// Adds a run to the history
pub fn record(
    root: &Path,
    trigger: Trigger,
    results: &[HookResult],
    duration: Duration,
) -> Result<(), GoatHookerError> {
    let run = Run {
        at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or_default(),
        trigger: trigger.as_str().to_string(),
        branch: git::current_branch(root),
        duration_ms: duration.as_millis() as u64,
        hooks: results
            .iter()
            .map(|r| HookRun {
                name: r.name.clone(),
                project: crate::graph::project_name(root, &r.base_dir),
                status: if r.cached {
                    Status::Cached
                } else if r.denied {
                    Status::Denied
                } else if r.skipped {
                    Status::Skipped
                } else if r.success {
                    Status::Passed
                } else {
                    Status::Failed
                },
                duration_ms: r.duration.as_millis() as u64,
            })
            .collect(),
    };
    let file = file(root)?;
    fs::create_dir_all(file.parent().unwrap())?;
    let mut out = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)?;
    writeln!(out, "{}", serde_json::to_string(&run).unwrap_or_default())?;
    drop(out);

    // Keeps the last runs
    let content = fs::read_to_string(&file)?;
    let lines: Vec<&str> = content.lines().collect();
    if lines.len() > MAX_RUNS + MAX_RUNS / 5 {
        fs::write(&file, lines[lines.len() - MAX_RUNS..].join("\n") + "\n")?;
    }
    Ok(())
}

/// The last runs, newest first
pub fn load(root: &Path, limit: usize) -> Vec<Run> {
    let Ok(content) = file(root).and_then(|f| Ok(fs::read_to_string(f)?)) else {
        return vec![];
    };
    content
        .lines()
        .rev()
        .filter_map(|l| serde_json::from_str(l).ok())
        .take(limit)
        .collect()
}

/// Hooks by average duration (the ones that really ran), slowest first:
/// (project, name, average, runs)
pub fn slowest(runs: &[Run]) -> Vec<(String, String, Duration, usize)> {
    let mut totals: BTreeMap<(String, String), (u64, usize)> = BTreeMap::new();
    for run in runs {
        for hook in &run.hooks {
            if matches!(hook.status, Status::Passed | Status::Failed) {
                let entry = totals
                    .entry((hook.project.clone(), hook.name.clone()))
                    .or_default();
                entry.0 += hook.duration_ms;
                entry.1 += 1;
            }
        }
    }
    let mut slowest: Vec<(String, String, Duration, usize)> = totals
        .into_iter()
        .map(|((project, name), (total, count))| {
            (
                project,
                name,
                Duration::from_millis(total / count as u64),
                count,
            )
        })
        .collect();
    slowest.sort_by_key(|s| std::cmp::Reverse(s.2));
    slowest
}

/// Totals of a hook over runs (`hooker history --export`)
#[derive(Debug, Clone, Default, Serialize)]
pub struct HookStats {
    pub project: String,
    pub name: String,
    pub trigger: String,
    pub runs: usize,
    pub passed: usize,
    pub failed: usize,
    pub cached: usize,
    pub skipped: usize,
    /// Average duration of the runs that really ran
    pub average_ms: u64,
    /// Time spent in this hook
    pub total_ms: u64,
    /// Seconds since the epoch
    pub last_run: u64,
}

/// Totals by hook, the ones taking the most time first
pub fn stats(runs: &[Run]) -> Vec<HookStats> {
    let mut stats: BTreeMap<(String, String, String), HookStats> = BTreeMap::new();
    for run in runs {
        for hook in &run.hooks {
            let entry = stats
                .entry((run.trigger.clone(), hook.project.clone(), hook.name.clone()))
                .or_insert_with(|| HookStats {
                    project: hook.project.clone(),
                    name: hook.name.clone(),
                    trigger: run.trigger.clone(),
                    ..Default::default()
                });
            entry.runs += 1;
            entry.last_run = entry.last_run.max(run.at);
            match hook.status {
                Status::Passed => entry.passed += 1,
                Status::Failed | Status::Denied => entry.failed += 1,
                Status::Cached => entry.cached += 1,
                Status::Skipped => entry.skipped += 1,
            }
            if matches!(hook.status, Status::Passed | Status::Failed) {
                entry.total_ms += hook.duration_ms;
            }
        }
    }
    let mut stats: Vec<HookStats> = stats
        .into_values()
        .map(|mut s| {
            let ran = (s.passed + s.failed) as u64;
            s.average_ms = s.total_ms.checked_div(ran).unwrap_or_default();
            s
        })
        .collect();
    stats.sort_by_key(|s| std::cmp::Reverse(s.total_ms));
    stats
}

/// The totals as CSV (one line per hook)
pub fn stats_csv(stats: &[HookStats]) -> String {
    let field = |text: &str| {
        if text.contains([',', '"', '\n']) {
            format!("\"{}\"", text.replace('"', "\"\""))
        } else {
            text.to_string()
        }
    };
    let mut csv = String::from(
        "project,name,trigger,runs,passed,failed,cached,skipped,average_ms,total_ms,last_run\n",
    );
    for s in stats {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{}\n",
            field(&s.project),
            field(&s.name),
            field(&s.trigger),
            s.runs,
            s.passed,
            s.failed,
            s.cached,
            s.skipped,
            s.average_ms,
            s.total_ms,
            s.last_run
        ));
    }
    csv
}

/// `3 min ago`, `2 h ago`, `5 d ago`
pub fn ago(at: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default();
    let seconds = now.saturating_sub(at);
    match seconds {
        0..60 => "just now".to_string(),
        60..3600 => format!("{} min ago", seconds / 60),
        3600..86400 => format!("{} h ago", seconds / 3600),
        _ => format!("{} d ago", seconds / 86400),
    }
}

#[cfg(test)]
mod stats_tests {
    use super::*;

    fn run(at: u64, hooks: &[(&str, Status, u64)]) -> Run {
        Run {
            at,
            trigger: "pre-commit".into(),
            branch: None,
            duration_ms: 0,
            hooks: hooks
                .iter()
                .map(|(name, status, ms)| HookRun {
                    name: name.to_string(),
                    project: "root".into(),
                    status: status.clone(),
                    duration_ms: *ms,
                })
                .collect(),
        }
    }

    #[test]
    fn totals_by_hook() {
        let runs = [
            run(
                1,
                &[
                    ("lint", Status::Passed, 100),
                    ("test, slow", Status::Failed, 4000),
                ],
            ),
            run(
                2,
                &[
                    ("lint", Status::Cached, 0),
                    ("test, slow", Status::Passed, 2000),
                ],
            ),
        ];
        let stats = stats(&runs);
        assert_eq!(stats[0].name, "test, slow");
        assert_eq!(
            (stats[0].runs, stats[0].failed, stats[0].average_ms),
            (2, 1, 3000)
        );
        assert_eq!(
            (stats[1].cached, stats[1].average_ms, stats[1].last_run),
            (1, 100, 2)
        );
        let csv = stats_csv(&stats);
        assert!(
            csv.lines()
                .nth(1)
                .unwrap()
                .starts_with("root,\"test, slow\",pre-commit,2,1,1")
        );
    }
}
