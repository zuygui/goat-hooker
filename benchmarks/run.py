#!/usr/bin/env python3
"""Times `git commit` with Goat Hooker, lefthook, prek, pre-commit and husky + lint-staged.

Each tool gets a fresh repository with the same hooks, and each scenario is timed over
several commits (median). Writes the results to `benchmarks/results.json`.

Requirements: `hooker` (or HOOKER=path), and a folder with `npm install lefthook husky
lint-staged` (TOOLS=path, default /tmp/bench-tools), and `prek` and `pre-commit` in PATH.

    python3 benchmarks/run.py
"""

import json
import os
import shutil
import statistics
import subprocess
import tempfile
import time
from pathlib import Path

RUNS = int(os.environ.get("RUNS", "12"))
HOOKER = os.environ.get("HOOKER") or shutil.which("hooker") or "hooker"
TOOLS = Path(os.environ.get("TOOLS", "/tmp/bench-tools")).resolve()
BIN = TOOLS / "node_modules" / ".bin"

# name, number of hooks, duration of each hook (seconds), files changed by each commit
SCENARIOS = {
    "noop": (1, 0.0, 1),
    "five": (5, 0.2, 1),
    "many": (5, 0.0, 1000),
}
# Same globs written differently, so that lint-staged runs the tasks concurrently
LINT_STAGED_GLOBS = ["*.txt", "**/*.txt", "*.t?t", "a*.txt", "[a]*.txt"]


def sh(cmd, cwd, env=None, check=True):
    # The installed Git hooks call the tools by name
    path = os.pathsep.join([str(Path(HOOKER).parent), os.environ["PATH"]])
    return subprocess.run(
        cmd,
        cwd=cwd,
        shell=True,
        check=check,
        env={**os.environ, "PATH": path, **(env or {})},
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def command(seconds):
    return f"sleep {seconds}" if seconds else "true"


def setup(tool, hooks, seconds, cache, root):
    sh("git init -q -b main && git config user.email b@b && git config user.name b", root)
    sh("git config commit.gpgsign false", root)
    (root / "a.txt").write_text("0\n")
    sh("git add -A && git commit -q -m init --no-verify", root)
    cmd = command(seconds)

    if tool == "goat-hooker":
        lines = ["parallel = true"]
        for i in range(hooks):
            lines += [
                "",
                "[[hooks]]",
                f'name = "hook{i}"',
                'trigger = "pre-commit"',
                'include = ["**/*.txt"]',
                f'cmd = "{cmd}"',
                f"cache = {'true' if cache else 'false'}",
            ]
        (root / "hooker.toml").write_text("\n".join(lines) + "\n")
        sh(f"{HOOKER} install", root)
    elif tool == "lefthook":
        lines = ["pre-commit:", "  parallel: true", "  commands:"]
        for i in range(hooks):
            lines += [f"    hook{i}:", '      glob: "*.txt"', f'      run: "{cmd}"']
        (root / "lefthook.yml").write_text("\n".join(lines) + "\n")
        sh(f"{BIN / 'lefthook'} install", root)
    elif tool in ("pre-commit", "prek"):
        lines = ["repos:", "  - repo: local", "    hooks:"]
        for i in range(hooks):
            lines += [
                f"      - id: hook{i}",
                f"        name: hook{i}",
                "        language: system",
                f"        entry: sh -c '{cmd}'",
                "        files: \\.txt$",
                "        pass_filenames: false",
            ]
        (root / ".pre-commit-config.yaml").write_text("\n".join(lines) + "\n")
        sh(f"{tool} install", root)
        sh(f"{tool} run --all-files", root, check=False)  # builds its environments once
    elif tool == "husky + lint-staged":
        os.symlink(TOOLS / "node_modules", root / "node_modules")
        package = {
            "name": "bench",
            "private": True,
            "lint-staged": {LINT_STAGED_GLOBS[i]: f"sh -c '{cmd}'" for i in range(hooks)},
        }
        (root / "package.json").write_text(json.dumps(package))
        (root / ".husky").mkdir()
        (root / ".husky" / "pre-commit").write_text("npx lint-staged\n")
        sh(f"{BIN / 'husky'}", root)
    elif tool == "none":
        pass
    sh("git add -A && git commit -q --allow-empty -m setup --no-verify", root)


def time_commits(root, same_content, files=1):
    times = []
    if files > 1:
        (root / "many").mkdir(exist_ok=True)
    for run in range(RUNS + 1):
        for i in range(files - 1):
            (root / "many" / f"f{i}.txt").write_text(f"{run} {i}\n")
        if same_content:
            # The same staged files again: a commit retried after a failure
            (root / "a.txt").write_text("same\n")
            sh("git add a.txt", root)
            if run == 0:
                sh("git commit -q -m first", root)
                sh("git reset -q --soft HEAD~1", root)
                continue
        else:
            (root / "a.txt").write_text(f"{run} {time.time()}\n")
            sh("git add -A", root)
        start = time.perf_counter()
        sh(f"git commit -q -m run{run}", root)
        elapsed = time.perf_counter() - start
        if same_content:
            sh("git reset -q --soft HEAD~1", root)
        if run > 0 or same_content:
            times.append(elapsed * 1000)
    return round(statistics.median(times))


def main():
    tools = ["none", "goat-hooker", "lefthook", "prek", "pre-commit", "husky + lint-staged"]
    only = os.environ.get("TOOLS_ONLY")
    if only:
        tools = ["none"] + [t for t in tools if t in only.split(",")]
    results = {"runs": RUNS, "scenarios": {}}
    for name, (hooks, seconds, files) in SCENARIOS.items():
        results["scenarios"][name] = {}
        for tool in tools:
            root = Path(tempfile.mkdtemp(prefix="bench-"))
            try:
                setup(tool, hooks, seconds, False, root)
                results["scenarios"][name][tool] = time_commits(root, False, files)
            finally:
                shutil.rmtree(root, ignore_errors=True)
            print(name, tool, results["scenarios"][name][tool], "ms", flush=True)

    # Retry of a commit with the same files, five hooks of 200 ms
    results["scenarios"]["retry"] = {}
    for tool in tools[1:]:
        root = Path(tempfile.mkdtemp(prefix="bench-"))
        try:
            setup(tool, 5, 0.2, True, root)
            results["scenarios"]["retry"][tool] = time_commits(root, True)
        finally:
            shutil.rmtree(root, ignore_errors=True)
        print("retry", tool, results["scenarios"]["retry"][tool], "ms", flush=True)

    versions = {
        "goat-hooker": subprocess.run([HOOKER, "--version"], capture_output=True, text=True).stdout.strip(),
        "lefthook": subprocess.run([str(BIN / "lefthook"), "version"], capture_output=True, text=True).stdout.strip(),
        "prek": subprocess.run(["prek", "--version"], capture_output=True, text=True).stdout.strip(),
        "pre-commit": subprocess.run(["pre-commit", "--version"], capture_output=True, text=True).stdout.strip(),
        "husky": json.loads((TOOLS / "node_modules/husky/package.json").read_text())["version"],
        "lint-staged": json.loads((TOOLS / "node_modules/lint-staged/package.json").read_text())["version"],
        "git": subprocess.run(["git", "--version"], capture_output=True, text=True).stdout.strip(),
    }
    results["versions"] = versions
    out = Path(os.environ.get("OUT") or Path(__file__).parent / "results.json")
    out.write_text(json.dumps(results, indent=2) + "\n")
    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
