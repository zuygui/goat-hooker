//! Source of a hook in its detail: its JSON in the config file, the scripts its commands run,
//! and the hooks file it loads (from the repository or another one).

use std::path::{Path, PathBuf};

use goat_hooker::config::Hook;

use super::*;

/// Extensions of the scripts a command can run
const SCRIPT_EXTENSIONS: [&str; 10] = [
    "sh", "bash", "zsh", "py", "js", "mjs", "ts", "rb", "pl", "ps1",
];
/// Lines shown of a file
const MAX_LINES: usize = 400;

/// Scripts of the project run by the commands of a hook
fn scripts(dir: &Path, hook: &Hook) -> Vec<PathBuf> {
    let mut scripts: Vec<PathBuf> = Vec::new();
    for cmd in hook.commands() {
        for token in cmd.split(|c: char| c.is_whitespace() || c == ';' || c == '&' || c == '|') {
            let token = token.trim_matches(|c| c == '"' || c == '\'' || c == '(' || c == ')');
            if token.is_empty()
                || token.starts_with('-')
                || token.contains('{')
                || token.contains('$')
            {
                continue;
            }
            let looks_like_script = token.contains('/')
                || Path::new(token)
                    .extension()
                    .is_some_and(|e| SCRIPT_EXTENSIONS.contains(&e.to_string_lossy().as_ref()));
            let path = dir.join(token.trim_start_matches("./"));
            if looks_like_script && path.is_file() && !scripts.contains(&path) {
                scripts.push(path);
            }
        }
    }
    scripts.truncate(5);
    scripts
}

/// Line (from 1) where a hook is written in its file
fn line_of(content: &str, name: &str) -> Option<usize> {
    let needle = serde_json::to_string(name).ok()?;
    content
        .lines()
        .position(|l| l.contains("\"name\"") && l.contains(&needle))
        .map(|l| l + 1)
}

/// Text of the JSON object containing the line `line` (from 1): (first line, text)
fn raw_object(content: &str, line: usize) -> Option<(usize, String)> {
    let lines: Vec<&str> = content.lines().collect();
    // The `{` opening the object: on the line, or before it
    let start = (0..line).rev().find(|&i| {
        let l = lines.get(i).copied().unwrap_or_default();
        l.contains('{') && (i + 1 == line || !l.contains('}'))
    })?;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let opening = lines[start].rfind('{')?;
    for (i, l) in lines.iter().enumerate().skip(start) {
        let chars: Vec<char> = l.chars().collect();
        let from = if i == start {
            l[..opening].chars().count()
        } else {
            0
        };
        for &c in &chars[from..] {
            if in_string {
                match (escaped, c) {
                    (true, _) => escaped = false,
                    (false, '\\') => escaped = true,
                    (false, '"') => in_string = false,
                    _ => {}
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((start + 1, lines[start..=i].join("\n")));
                    }
                }
                _ => {}
            }
        }
    }
    None
}

/// GitHub page of a file of a repository
fn github_url(url: &str, rev: Option<&str>, path: &str) -> Option<String> {
    let repo = url
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .strip_prefix("https://github.com/")?;
    Some(format!(
        "https://github.com/{repo}/blob/{}/{}",
        rev.unwrap_or("HEAD"),
        path.trim_start_matches("./")
    ))
}

impl HookerApp {
    fn code_view(&self, content: &str, first_line: usize, cx: &App) -> Div {
        let theme = cx.theme();
        let mut code = v_flex()
            .p_3()
            .rounded_md()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .font_family("monospace")
            .text_xs();
        let lines: Vec<&str> = content.lines().collect();
        for (i, line) in lines.iter().take(MAX_LINES).enumerate() {
            code = code.child(
                h_flex()
                    .gap_3()
                    .child(
                        div()
                            .w(px(28.))
                            .flex_shrink_0()
                            .text_color(theme.muted_foreground.opacity(0.6))
                            .child(format!("{:>3}", first_line + i)),
                    )
                    .child(div().whitespace_nowrap().child(line.replace('\t', "    "))),
            );
        }
        if lines.len() > MAX_LINES {
            code = code.child(
                div()
                    .text_color(theme.muted_foreground)
                    .child(format!("… {} more lines", lines.len() - MAX_LINES)),
            );
        }
        code
    }

    fn file_header(
        &self,
        id: &'static str,
        index: usize,
        path: &Path,
        label: String,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = cx.theme().clone();
        let open = path.to_path_buf();
        let reveal = path.to_path_buf();
        h_flex()
            .gap_2()
            .child(
                lucide("terminal")
                    .small()
                    .text_color(theme.muted_foreground),
            )
            .child(div().flex_1().text_sm().truncate().child(label))
            .child(
                Button::new((id, index))
                    .ghost()
                    .xsmall()
                    .label("Open")
                    .tooltip("Open with the default editor")
                    .on_click(cx.listener(move |_, _, _, cx| cx.open_with_system(&open))),
            )
            .child(
                Button::new((id, index + 1000))
                    .ghost()
                    .xsmall()
                    .icon(IconName::FolderOpen)
                    .tooltip("Show in the folder")
                    .on_click(cx.listener(move |_, _, _, cx| cx.reveal_path(&reveal))),
            )
    }

    /// Card with the source of a hook
    pub(super) fn source_card(
        &self,
        project: &Project,
        hook: &Hook,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = cx.theme().clone();
        let expanded = self.pages.show_source;
        let dir = project.dir();

        let mut card = v_flex()
            .gap_2()
            .p_4()
            .rounded_lg()
            .bg(theme.secondary.opacity(0.5))
            .border_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .child(div().flex_1().child(section("SOURCE", cx)))
                    .child(
                        Button::new("toggle-source")
                            .ghost()
                            .xsmall()
                            .icon(if expanded {
                                IconName::ChevronUp
                            } else {
                                IconName::ChevronDown
                            })
                            .label(if expanded { "Hide" } else { "Show the code" })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.pages.show_source = !this.pages.show_source;
                                cx.notify();
                            })),
                    ),
            );

        // The hook in its config file
        let content = std::fs::read_to_string(&project.file).unwrap_or_default();
        let line = line_of(&content, &hook.name);
        let relative = project
            .file
            .strip_prefix(
                self.repo
                    .as_ref()
                    .map(|r| r.root.as_path())
                    .unwrap_or(Path::new("")),
            )
            .unwrap_or(&project.file)
            .display()
            .to_string();
        card = card.child(self.file_header(
            "open-config",
            0,
            &project.file,
            match line {
                Some(line) => format!("{relative}:{line}"),
                None => relative,
            },
            cx,
        ));
        if expanded {
            match line.and_then(|l| raw_object(&content, l)) {
                // As written in the file
                Some((first, text)) => card = card.child(self.code_view(&text, first, cx)),
                None => {
                    let json = serde_json::to_string_pretty(hook).unwrap_or_default();
                    card = card.child(self.code_view(&json, 1, cx));
                }
            }
        }

        // Scripts run by the commands
        for (i, script) in scripts(&dir, hook).iter().enumerate() {
            let label = script
                .strip_prefix(&dir)
                .unwrap_or(script)
                .display()
                .to_string();
            card = card.child(self.file_header("open-script", 10 + i, script, label, cx));
            if expanded {
                let content = std::fs::read_to_string(script).unwrap_or_else(|e| e.to_string());
                card = card.child(self.code_view(&content, 1, cx));
            }
        }

        // The hooks file it loads
        if let Some(path) = hook.path.as_ref() {
            let file = match hook.from.as_ref() {
                Some(url) => goat_hooker::remote::repo_dir(url, hook.rev.as_deref()).join(path),
                None => dir.join(path),
            };
            let label = match hook.from.as_ref() {
                Some(url) => format!("{path} · {url}"),
                None => path.clone(),
            };
            card = card.child(self.file_header("open-hooks-file", 30, &file, label, cx));
            if let Some(page) = hook
                .from
                .as_ref()
                .and_then(|url| github_url(url, hook.rev.as_deref(), path))
            {
                card = card.child(
                    h_flex().child(
                        Button::new("open-github")
                            .ghost()
                            .xsmall()
                            .icon(lucide("github"))
                            .label("View on GitHub")
                            .on_click(cx.listener(move |_, _, _, cx| cx.open_url(&page))),
                    ),
                );
            }
            if expanded {
                let content = std::fs::read_to_string(&file)
                    .unwrap_or_else(|_| "Not fetched yet: run the hooks or `hooker update`".into());
                card = card.child(self.code_view(&content, 1, cx));
            }
        }
        card
    }
}

#[cfg(test)]
mod tests {
    use super::{github_url, line_of, raw_object, scripts};
    use goat_hooker::config::Hook;

    #[test]
    fn finds_scripts_and_lines() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("scripts")).unwrap();
        std::fs::write(dir.path().join("scripts/check.sh"), "echo ok").unwrap();
        std::fs::write(dir.path().join("lint.py"), "print(1)").unwrap();
        let hook = Hook {
            name: "x".into(),
            cmd: Some("./scripts/check.sh {files} && python lint.py --fix; echo $HOME".into()),
            ..Default::default()
        };
        let found = scripts(dir.path(), &hook);
        assert_eq!(
            found,
            vec![
                dir.path().join("scripts/check.sh"),
                dir.path().join("lint.py")
            ]
        );
        assert_eq!(
            line_of("{\n  \"hooks\": [\n    { \"name\": \"x\" }", "x"),
            Some(3)
        );
        let json = "{\n  \"hooks\": [\n    {\n      \"name\": \"x\",\n      \"cmd\": \"echo }\"\n    },\n    { \"name\": \"y\" }\n  ]\n}";
        assert_eq!(
            raw_object(json, 4),
            Some((
                3,
                "    {\n      \"name\": \"x\",\n      \"cmd\": \"echo }\"\n    },".to_string()
            ))
        );
        assert_eq!(
            raw_object(json, 7),
            Some((7, "    { \"name\": \"y\" }".to_string()))
        );
        assert_eq!(
            github_url("https://github.com/a/b.git", None, "./hooks/r.json").as_deref(),
            Some("https://github.com/a/b/blob/HEAD/hooks/r.json")
        );
    }
}
