// Content of the landing page that isn't read from the repository.

export const YES = "yes";
export const PART = "part";
export const NO = "no";

export const compareTools = ["Goat Hooker", "lefthook", "prek", "pre-commit", "husky + lint-staged"];

export const compareCommon: [string, string[]][] = [
  ["Runs hooks on the staged files they target", [YES, YES, YES, YES, PART]],
  ["Hides unstaged changes while hooks run", [YES, YES, YES, YES, PART]],
  ["Stages again the files formatters fixed", [YES, YES, NO, NO, PART]],
  ["Runs hooks in parallel", [YES, YES, PART, PART, PART]],
  ["Shared hooks from Git repositories", [YES, YES, YES, YES, NO]],
  ["Hooks of the pre-commit ecosystem", [YES, NO, YES, YES, NO]],
  ["Local changes for one computer", [YES, YES, NO, NO, NO]],
  ["Skips one hook by name", [YES, YES, YES, YES, NO]],
  ["A config per project of a mono-repo", [YES, PART, YES, NO, NO]],
  ["Installs the tools the hooks need", [PART, NO, YES, YES, NO]],
];

export const compareOnly = [
  "Skips hooks whose files didn't change (cache)",
  "Runs the hooks of the projects a change affects",
  "Hooks that pass values to each other (graph)",
  "Sandbox: read-only files, no network",
  "Review of shared hooks before they run",
  "Runs hooks while you stage (hooker watch)",
  "Desktop app",
  "Runs lefthook and husky configs as they are",
];

/** The commands, each with what it replaces and a terminal */
export const tools = [
  {
    id: "run",
    command: "hooker run",
    title: "Hooks on every commit",
    replaces: "husky, lefthook, pre-commit",
    text: "Runs the hooks of a trigger on the staged files they target, in parallel, in the order of their links. Unstaged changes are put aside, formatter fixes are staged again.",
    term: [
      ["$ ", 'git commit -m "feat: add numbers"'],
      ["🐐 ", "pre-commit · 2 hooks"],
      ["✔ ", "fmt     root  81ms"],
      ["✔ ", "clippy  root  995ms   ← fmt"],
      ["", ""],
      ["✔ ", "2 hooks passed in 1.10s"],
    ],
    doc: "/goat-hooker/docs/running/",
  },
  {
    id: "watch",
    command: "hooker watch",
    title: "Commits that don't wait",
    replaces: "waiting on git commit",
    text: "Runs the cached pre-commit hooks each time you stage files. When you commit, their results are already in the cache: the commit finishes at once.",
    term: [
      ["$ ", "hooker watch"],
      ["👀 ", "Watching the staged files · 2 hooks: clippy, test"],
      ["✔ ", "clippy 5.61s  ✔ test 8.02s  ready to commit"],
      ["$ ", 'git commit -m "fix: overflow"'],
      ["✔ ", "2 hooks passed in 21ms"],
    ],
    doc: "/goat-hooker/docs/running/#ahead-of-the-commit-watch",
  },
  {
    id: "fix",
    command: "hooker fix",
    title: "Formatters, on demand",
    replaces: "running formatters by hand",
    text: "Runs the hooks that fix files on everything you changed since the last commit, without staging anything, and lists what they fixed.",
    term: [
      ["$ ", "hooker fix"],
      ["✔ ", "prettier  web   212ms"],
      ["✔ ", "rustfmt   core  48ms"],
      ["", ""],
      ["   ", "Fixed 3 files: web/src/app.tsx, web/src/api.ts, core/src/lib.rs"],
    ],
    doc: "/goat-hooker/docs/running/#fixing-files-fix",
  },
  {
    id: "import",
    command: "hooker install --force",
    title: "Keep your config, switch the tool",
    replaces: "migrating from husky, lefthook or pre-commit",
    text: "Goat Hooker runs your lefthook.yml, .pre-commit-config.yaml or husky + lint-staged as they are. Nothing to rewrite, and hooker import converts them to a hooker.toml when you want more.",
    term: [
      ["$ ", "hooker install --force"],
      ["✔ ", "Git hooks installed in .git/hooks"],
      ["$ ", "git commit -m \"feat: faster\""],
      ["🐐 ", "pre-commit · 2 hooks"],
      ["✔ ", "eslint  root  41ms"],
      ["✔ ", "prettier  root  38ms"],
    ],
    doc: "/goat-hooker/docs/migrating/",
  },
  {
    id: "doctor",
    command: "hooker doctor",
    title: "Why didn't my hook run?",
    replaces: "guessing",
    text: "Checks the Git hooks, the tools your commands need, patterns that match nothing, unused variables and untrusted hooks, and gives hints from the history.",
    term: [
      ["$ ", "hooker doctor"],
      ["✘ ", "`eslint` isn't installed"],
      ["  ", "used by `lint` (web)"],
      ["! ", "`docs` (core) matches no file"],
      ["→ ", "`clippy` (root) takes 5.6s on each commit"],
    ],
    doc: "/goat-hooker/docs/running/#doctor",
  },
  {
    id: "ci",
    command: "hooker ci",
    title: "The same hooks in CI",
    replaces: "copying your checks into the CI config",
    text: "Runs the hooks on the files a pull request changes, with a GitHub Action that annotates the failures, so CI checks exactly what the hooks check.",
    term: [
      ["", "- uses: actions/checkout@v4"],
      ["", "  with: { fetch-depth: 0 }"],
      ["", "- uses: zuygui/goat-hooker@main"],
      ["", "  with:"],
      ["", "    trigger: pre-commit"],
    ],
    doc: "/goat-hooker/docs/ci/",
  },
];

/** Configs shown by the tabs of the "built in" section */
export const features = [
  {
    id: "monorepo",
    label: "Mono-repos",
    title: "A config per project",
    text: "Each project of the mono-repo has its own hooker.toml and its hooks run from its folder, on its files. Dependencies are read from Cargo, package.json, go.mod and pyproject: change core, and the tests of api run too.",
    file: "packages/api/hooker.toml",
    code: `depends_on = ["../core"]

[[hooks]]
name = "lint"
trigger = "pre-commit"
include = ["src/**/*.ts"]
cmd = "eslint {files}"

[[hooks]]
name = "test"
trigger = "pre-push"
include = ["src/**"]
cmd = "npm test"
affected = true`,
  },
  {
    id: "cache",
    label: "Cache",
    title: "Hooks that already passed don't run again",
    text: "A hook whose files, command and inputs didn't change since it passed is taken from the cache. A commit retried after a rejected message takes milliseconds.",
    file: "hooker.toml",
    code: `[[hooks]]
name = "clippy"
trigger = "pre-commit"
include = ["**/*.rs"]
cmd = "cargo clippy -- -D warnings"
cache = true`,
  },
  {
    id: "graph",
    label: "Graph",
    title: "Hooks that work together",
    text: "Hooks wait for each other and pass values: codegen writes its schema, types receives it. Steps of one hook run as separate nodes, in parallel when they can.",
    file: "hooker.toml",
    code: `[[hooks]]
name = "codegen"
trigger = "pre-commit"
outputs = ["schema"]
cmd = "npm run codegen && echo schema=gen/schema.ts >> $HOOKER_OUTPUT"

[[hooks]]
name = "lint"
trigger = "pre-commit"

[[hooks.steps]]
id = "eslint"
cmd = "eslint {files}"

[[hooks.steps]]
id = "types"
needs = ["eslint"]
inputs = { schema = "codegen.schema" }
cmd = "tsc --noEmit {schema}"`,
  },
  {
    id: "shared",
    label: "Shared hooks",
    title: "Hooks from other repositories",
    text: "Load a hooks file from a Git repository, at a tag or a commit, with parameters. hooker search finds the shared ones on GitHub, and the catalogue covers Rust, JavaScript and Python.",
    file: "hooker.toml",
    code: `[[hooks]]
name = "rust"
from = "https://github.com/zuygui/goat-hooker"
path = "hooks/rust.json"
with = { clippy_args = "--all-targets" }

[[hooks]]
name = "team rules"
from = "https://github.com/acme/hooks"
rev = "v1"
path = "rust.json"`,
  },
  {
    id: "ecosystem",
    label: "pre-commit hooks",
    title: "The whole pre-commit ecosystem",
    text: "Any hook with a .pre-commit-hooks.yaml works with from + id: ruff, prettier, shellcheck, codespell... Your installed tools first; otherwise Goat Hooker builds them once with the toolchains of your computer (venv or uv, npm, cargo, go), and never downloads a runtime.",
    file: "hooker.toml",
    code: `[[hooks]]
from = "https://github.com/astral-sh/ruff-pre-commit"
rev = "v0.8.0"
id = "ruff"
args = ["--fix"]

[[hooks]]
from = "https://github.com/pre-commit/pre-commit-hooks"
rev = "v5.0.0"
id = "trailing-whitespace"

[[hooks]]
from = "https://github.com/pre-commit/mirrors-prettier"
rev = "v3.1.0"
id = "prettier"
# its own npm install, at this rev
isolate = true`,
  },
  {
    id: "permissions",
    label: "Permissions",
    title: "Safe with other people's hooks",
    text: "Shared hooks run once you reviewed them, and again only after you review what changed. They read files without changing them, have no network and a filtered environment, unless they're allowed more.",
    file: "hooker.toml",
    code: `[[hooks]]
name = "license check"
trigger = "pre-commit"
cmd = "./check-licenses"
permissions = { files = "read", network = false, env = ["LICENSE_KEY"] }`,
  },
  {
    id: "local",
    label: "Local changes",
    title: "Your computer, your rules",
    text: "hooker.local.toml changes the config on your computer only: turn a slow hook off, give one a timeout, add your own, without touching what the team shares.",
    file: "hooker.local.toml",
    code: `[[hooks]]
name = "e2e"
disabled = true

[[hooks]]
name = "clippy"
timeout = "2m"

[[hooks]]
name = "notes"
trigger = "post-commit"
cmd = "./my-notes.sh"`,
  },
];

/** Colors TOML for the code blocks of the page (no runtime highlighter) */
export function highlightToml(code: string): string {
  const escape = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  return code
    .split("\n")
    .map((line) => {
      if (/^\s*#/.test(line)) return `<span class="t-com">${escape(line)}</span>`;
      if (/^\s*\[/.test(line) && !line.includes("=")) return `<span class="t-key">${escape(line)}</span>`;
      return escape(line).replace(
        /("(?:[^"\\]|\\.)*"|'[^']*')|^(\s*)([\w.-]+)(\s*=)|([{,]\s*)([\w-]+)(\s*=)|\b(true|false)\b|\b(\d+)\b/g,
        (match, string, indent, key, eq, before, inlineKey, inlineEq, literal, number) => {
          if (string) return `<span class="t-str">${string}</span>`;
          if (key) return `${indent}<span class="t-key">${key}</span>${eq}`;
          if (inlineKey) return `${before}<span class="t-key">${inlineKey}</span>${inlineEq}`;
          if (literal) return `<span class="t-lit">${literal}</span>`;
          if (number) return `<span class="t-num">${number}</span>`;
          return match;
        },
      );
    })
    .join("\n");
}
