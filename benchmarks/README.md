# Benchmarks

`run.py` times `git commit` in fresh repositories with the same hooks for each tool:

- **noop**: one hook that does nothing (the cost of the tool itself)
- **five**: five hooks of 200 ms each on the staged file (every tool runs them in parallel, except pre-commit, which runs hooks one after the other)
- **retry**: the same commit again with the same staged files, as after a failed commit message (Goat Hooker takes the results of the hooks with `cache` from its cache)

Each number is the median of 15 commits, in milliseconds; `results.json` has the last results and the versions used.

Setup:

- lefthook, husky and lint-staged installed with npm
- pre-commit installed with pip
- hooks run by the `git commit` of each tool's own installation: husky calls `npx lint-staged`, as its documentation shows

```sh
mkdir -p /tmp/bench-tools && cd /tmp/bench-tools && npm init -y && npm install lefthook husky lint-staged
pip install pre-commit
cargo build --release
HOOKER=target/release/hooker python3 benchmarks/run.py
```

Caveats:

- small numbers on a 2-core Linux VM, with git 2.43
- the scenarios measure the overhead of the tools and how they schedule hooks, not the speed of the hooks themselves
