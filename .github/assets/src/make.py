#!/usr/bin/env python3
"""Draws the images of the README in the style of the banner, in a light and a dark version:

- features-*.svg: what Goat Hooker and the other hooks managers do
- speed-*.svg: the benchmark of `benchmarks/results.json`
- cli-*.svg: a commit in the terminal
- app-*.png, graph-*.png: screenshots of the app (from `screenshots/`) in a frame

    python3 .github/assets/src/make.py
"""

import base64
import json
import subprocess
from html import escape
from pathlib import Path

HERE = Path(__file__).parent
ASSETS = HERE.parent
ROOT = ASSETS.parent.parent

PINK = "#F3678B"
TEAL = "#65DDDB"
FONT = "Inter, -apple-system, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif"
MONO = "'JetBrains Mono', SFMono-Regular, Menlo, Consolas, 'Liberation Mono', monospace"

THEMES = {
    "dark": {
        "bg": "#242222",
        "fg": "#F4F2F2",
        "muted": "#A9A4A4",
        "line": "#3A3737",
        "panel": "#1B1A1A",
        "highlight": "#2E3B3B",
        "no": "#5E5959",
        "wave": 0.22,
    },
    "light": {
        "bg": "#FFFFFF",
        "fg": "#1F1D1D",
        "muted": "#6E6868",
        "line": "#ECEAEA",
        "panel": "#F7F6F6",
        "highlight": "#E8FAFA",
        "no": "#C9C4C4",
        "wave": 0.28,
    },
}

# The wave of the banner (830 x 155), drawn at the bottom of every image
WAVE = (HERE / "wave.txt").read_text().strip()


def card(width, height, theme, body):
    t = THEMES[theme]
    scale = width / 830
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" fill="none">
<defs><clipPath id="card"><rect width="{width}" height="{height}" rx="20"/></clipPath></defs>
<g clip-path="url(#card)">
<rect width="{width}" height="{height}" rx="20" fill="{t['bg']}"/>
<path transform="translate(0 {height - 155 * scale + 40 * scale}) scale({scale})" d="{WAVE}" fill="{TEAL}" fill-opacity="{t['wave']}"/>
{body}
</g>
</svg>
"""


def text(x, y, content, size=14, color=None, weight=400, anchor="start", family=FONT, extra=""):
    return (
        f'<text x="{x}" y="{y}" font-family="{family}" font-size="{size}" font-weight="{weight}" '
        f'fill="{color}" text-anchor="{anchor}" {extra}>{escape(content)}</text>'
    )


# MARK: features

TOOLS = ["Goat Hooker", "lefthook", "prek", "pre-commit", "husky + lint-staged"]
YES, PART, NO = "yes", "part", "no"

COMMON = [
    ("Runs hooks on the staged files they target", [YES, YES, YES, YES, PART]),
    ("Hides unstaged changes while hooks run", [YES, YES, YES, YES, PART]),
    ("Stages again the files formatters fixed", [YES, YES, NO, NO, PART]),
    ("Runs hooks in parallel", [YES, YES, PART, PART, PART]),
    ("Shared hooks from Git repositories", [YES, YES, YES, YES, NO]),
    ("Hooks of the pre-commit ecosystem", [YES, NO, YES, YES, NO]),
    ("Local changes for one computer", [YES, YES, NO, NO, NO]),
    ("Skips one hook by name", [YES, YES, YES, YES, NO]),
    ("A config per project of a mono-repo", [YES, PART, YES, NO, NO]),
    ("Installs the tools the hooks need", [PART, NO, YES, YES, NO]),
]
ONLY = [
    "Skips hooks whose files didn't change (cache)",
    "Runs the hooks of the projects a change affects",
    "Hooks that pass values to each other (graph)",
    "Sandbox: read-only files, no network",
    "Review of shared hooks before they run",
    "Runs hooks while you stage (hooker watch)",
    "Desktop app",
    "Runs lefthook and husky configs as they are",
]


def mark(cx, cy, kind, t):
    if kind == YES:
        return (
            f'<circle cx="{cx}" cy="{cy}" r="9" fill="{TEAL}"/>'
            f'<path d="M{cx - 4} {cy} l3 3 l5.5 -6" stroke="{t["bg"]}" stroke-width="2.2" '
            'stroke-linecap="round" stroke-linejoin="round" fill="none"/>'
        )
    if kind == PART:
        return (
            f'<circle cx="{cx}" cy="{cy}" r="8" stroke="{TEAL}" stroke-width="2" fill="none"/>'
            f'<path d="M{cx} {cy - 8} a8 8 0 0 1 0 16 z" fill="{TEAL}"/>'
        )
    return f'<rect x="{cx - 6}" y="{cy - 1.25}" width="12" height="2.5" rx="1.25" fill="{t["no"]}"/>'


def text_width(content, size):
    """Width of a text in Inter, from the average width of its characters"""
    narrow = sum(content.count(c) for c in "iljtf.,:;'()|! ")
    wide = sum(content.count(c) for c in "mwMW")
    return size * (0.56 * len(content) - 0.28 * narrow + 0.3 * wide)


def features(theme):
    t = THEMES[theme]
    col = 96
    row = 32
    top = 112
    rows = len(COMMON) + len(ONLY)
    # The labels decide where the columns start
    longest = max(text_width(label, 14) for label in [label for label, _ in COMMON] + ONLY)
    first = int(max(360, 40 + longest + 28))
    width = first + col * len(TOOLS) + 40
    xs = [first + col * i + col / 2 for i in range(len(TOOLS))]
    parts = [
        text(40, 52, "How Goat Hooker compares", 24, PINK, 700),
        text(40, 78, "Features of the Git hooks managers, built in or with their usual setup", 14, t["muted"]),
    ]
    # Column of Goat Hooker: drawn first (under the marks), sized once the rows are placed
    column = len(parts)
    for i, tool in enumerate(TOOLS):
        color = PINK if i == 0 else t["fg"]
        lines = tool.split(" + ")
        if len(lines) == 2:
            parts.append(text(xs[i], top - 6, lines[0] + " +", 12.5, color, 600, "middle"))
            parts.append(text(xs[i], top + 9, lines[1], 12.5, color, 600, "middle"))
        else:
            parts.append(text(xs[i], top + 2, tool, 13, color, 700 if i == 0 else 600, "middle"))
    y = top + 20

    def section(label, y):
        return text(40, y + 22, label.upper(), 11.5, PINK, 700, extra='letter-spacing="1.2"')

    parts.append(section("Everyday hooks", y))
    y += 40
    for label, marks in COMMON:
        parts.append(f'<line x1="40" x2="{width - 40}" y1="{y}" y2="{y}" stroke="{t["line"]}"/>')
        parts.append(text(40, y + 21, label, 14, t["fg"]))
        for i, kind in enumerate(marks):
            parts.append(mark(xs[i], y + 16, kind, t))
        y += row
    parts.append(section("Only in Goat Hooker", y))
    y += 40
    for label in ONLY:
        parts.append(f'<line x1="40" x2="{width - 40}" y1="{y}" y2="{y}" stroke="{t["line"]}"/>')
        parts.append(text(40, y + 21, label, 14, t["fg"]))
        for i in range(len(TOOLS)):
            parts.append(mark(xs[i], y + 16, YES if i == 0 else NO, t))
        y += row
    parts.insert(
        column,
        f'<rect x="{first + 4}" y="{top - 26}" width="{col - 8}" height="{y + 8 - (top - 26)}" rx="12" fill="{t["highlight"]}"/>',
    )
    y += 34
    parts.append(mark(48, y - 4, YES, t))
    parts.append(text(64, y + 1, "built in", 12.5, t["muted"]))
    parts.append(mark(148, y - 4, PART, t))
    parts.append(text(164, y + 1, "partly, or with another tool", 12.5, t["muted"]))
    parts.append(mark(362, y - 4, NO, t))
    parts.append(text(378, y + 1, "no", 12.5, t["muted"]))
    parts.append(
        text(
            40,
            y + 26,
            "lefthook 2.2, prek 0.5, pre-commit 4.7, husky 9.1 + lint-staged 17.6, November 2026",
            12,
            t["muted"],
        )
    )
    # Room for the wave under the last line
    height = int(y + 26 + 90)
    return card(width, height, theme, "\n".join(parts))


# MARK: speed

SCENARIOS = [
    ("noop", "One hook that does nothing", "the cost of the tool itself"),
    ("five", "Five hooks of 200 ms", "how the hooks are scheduled"),
    ("many", "1,000 staged files", "five hooks matching them"),
    ("retry", "The same commit again", "after a failed commit message, same staged files"),
]


def speed(theme, results):
    t = THEMES[theme]
    width = 830
    label_w, left = 170, 40
    bar_x = left + label_w
    bar_max = width - bar_x - 110
    bar_h, gap = 20, 10
    parts = [
        text(40, 52, "Time of a git commit", 24, PINK, 700),
        text(40, 78, "Median of 15 commits, in milliseconds (lower is better)", 14, t["muted"]),
    ]
    y = 110
    for key, title, subtitle in SCENARIOS:
        values = results["scenarios"][key]
        tools = [
            tool
            for tool in ["goat-hooker", "lefthook", "prek", "pre-commit", "husky + lint-staged"]
            if tool in values
        ]
        top = max(values[tool] for tool in tools)
        parts.append(text(left, y + 14, title, 15, t["fg"], 700))
        parts.append(text(left + 9 * len(title) + 22, y + 14, subtitle, 13, t["muted"]))
        y += 28
        for tool in tools:
            value = values[tool]
            w = max(3, bar_max * value / top)
            ours = tool == "goat-hooker"
            name = "Goat Hooker" if ours else tool
            parts.append(text(left, y + 15, name, 13.5, PINK if ours else t["fg"], 700 if ours else 400))
            parts.append(
                f'<rect x="{bar_x}" y="{y}" width="{w:.1f}" height="{bar_h}" rx="6" '
                f'fill="{PINK if ours else TEAL}" fill-opacity="{1 if ours else 0.75}"/>'
            )
            parts.append(text(bar_x + w + 10, y + 15, f"{value} ms", 13.5, t["fg"], 700 if ours else 400))
            y += bar_h + gap
        y += 22
    versions = results["versions"]
    parts.append(
        text(
            40,
            y + 4,
            f"Goat Hooker {versions['goat-hooker'].split()[-1]}, lefthook {versions['lefthook']}, "
            f"prek {versions['prek'].split()[-1]}, pre-commit {versions['pre-commit'].split()[-1]}, "
            f"husky {versions['husky']} + lint-staged {versions['lint-staged']}, "
            f"{versions['git'].replace('git version', 'git')}, 2-core Linux VM",
            12,
            t["muted"],
        )
    )
    parts.append(text(40, y + 22, "Reproduce with python3 benchmarks/run.py", 12, t["muted"]))
    return card(width, y + 70, theme, "\n".join(parts))


# MARK: cli

# (text, color) runs per line; colors: fg, muted, ok, ko, pink, teal
CLI = [
    [("$ ", "muted"), ('git commit -m "feat: add numbers"', "fg")],
    [("🐐 pre-commit", "fg:bold"), (" · 2 hooks", "muted")],
    [("✔ ", "ok"), ("fmt     ", "fg"), ("root  ", "muted"), ("81ms", "muted")],
    [("✔ ", "ok"), ("clippy  ", "fg"), ("root  ", "muted"), ("995ms   ", "muted"), ("← fmt", "teal")],
    [],
    [("✔ 2 hooks passed", "ok"), (" in 1.10s", "muted")],
    [("🐐 commit-msg", "fg:bold"), (" · 1 hook", "muted")],
    [("✔ ", "ok"), ("message  ", "fg"), ("root  ", "muted"), ("10ms", "muted")],
    [],
    [("$ ", "muted"), ('git commit -m "added stuff"', "fg")],
    [("🐐 pre-commit", "fg:bold"), (" · 2 hooks", "muted")],
    [("✔ ", "ok"), ("fmt     ", "fg"), ("root  ", "muted"), ("cached", "teal")],
    [("✔ ", "ok"), ("clippy  ", "fg"), ("root  ", "muted"), ("cached  ", "teal"), ("← fmt", "teal")],
    [],
    [("✔ 2 hooks passed", "ok"), (" in 17ms", "muted")],
    [("🐐 commit-msg", "fg:bold"), (" · 1 hook", "muted")],
    [("✘ message", "ko:bold"), (" · root", "muted")],
    [("  $ grep -qE '^(feat|fix|docs|chore)(\\(.+\\))?: ' \"$1\"", "muted")],
    [],
    [("✘ 1 failed", "ko"), (" in 20ms", "muted")],
    [("  hint: ", "muted"), ("fix the errors above, or skip them once with ", "muted"), ("HOOKER_SKIP=message git ...", "fg")],
]


def cli(theme):
    t = THEMES[theme]
    colors = {"fg": t["fg"], "muted": t["muted"], "ok": "#3DBE8B", "ko": PINK, "pink": PINK, "teal": "#2FB8B6" if theme == "light" else TEAL}
    width = 830
    line_h = 21
    top = 92
    height = top + len(CLI) * line_h + 64
    parts = [
        f'<rect x="30" y="30" width="{width - 60}" height="{height - 60}" rx="14" fill="{t["panel"]}" stroke="{t["line"]}"/>',
        f'<circle cx="54" cy="54" r="6" fill="{PINK}"/><circle cx="74" cy="54" r="6" fill="#F5C45E"/><circle cx="94" cy="54" r="6" fill="{TEAL}"/>',
        text(width / 2, 59, "~/taste — git commit", 12.5, t["muted"], 500, "middle"),
        f'<line x1="30" x2="{width - 30}" y1="76" y2="76" stroke="{t["line"]}"/>',
    ]
    y = top + 14
    for runs in CLI:
        spans = []
        for content, style in runs:
            name, _, weight = style.partition(":")
            spans.append(
                f'<tspan fill="{colors[name]}" font-weight="{700 if weight == "bold" else 400}">{escape(content)}</tspan>'
            )
        if spans:
            parts.append(
                f'<text x="54" y="{y}" font-family="{MONO}" font-size="13.5" xml:space="preserve">{"".join(spans)}</text>'
            )
        y += line_h
    return card(width, height, theme, "\n".join(parts))


# MARK: screenshots


def framed(theme, screenshot):
    """A screenshot of the app on the card of the banner"""
    t = THEMES[theme]
    data = base64.b64encode(screenshot.read_bytes()).decode()
    w, h = 1180, 760
    width, height = w + 100, h + 100
    shadow = "0.45" if theme == "dark" else "0.18"
    body = f"""<defs>
<clipPath id="shot"><rect x="50" y="44" width="{w}" height="{h}" rx="12"/></clipPath>
<filter id="shadow" x="-10%" y="-10%" width="120%" height="125%"><feDropShadow dx="0" dy="10" stdDeviation="14" flood-color="#000" flood-opacity="{shadow}"/></filter>
</defs>
<rect x="50" y="44" width="{w}" height="{h}" rx="12" fill="{t['bg']}" filter="url(#shadow)"/>
<image x="50" y="44" width="{w}" height="{h}" clip-path="url(#shot)" href="data:image/png;base64,{data}"/>
<rect x="50.5" y="44.5" width="{w - 1}" height="{h - 1}" rx="12" stroke="{t['line']}" fill="none"/>"""
    return card(width, height, theme, body)


def main():
    results = json.loads((ROOT / "benchmarks" / "results.json").read_text())
    for theme in THEMES:
        (ASSETS / f"features-{theme}.svg").write_text(features(theme))
        (ASSETS / f"speed-{theme}.svg").write_text(speed(theme, results))
        (ASSETS / f"cli-{theme}.svg").write_text(cli(theme))
        for name in ["app", "graph"]:
            shot = HERE / "screenshots" / f"{name}-{theme}.png"
            if shot.exists():
                svg = HERE / f"{name}-{theme}.svg"
                svg.write_text(framed(theme, shot))
                subprocess.run(
                    ["rsvg-convert", str(svg), "-o", str(ASSETS / f"{name}-{theme}.png")], check=True
                )
                svg.unlink()
    print("done")


if __name__ == "__main__":
    main()
