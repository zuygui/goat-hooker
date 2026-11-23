# Desktop app

`goat-hooker-gui [repository]` opens a repository in a desktop app (built with [GPUI](https://www.gpui.rs) and [GPUI Kit](https://gpui-kit.com)):

- browse the hooks by trigger or by project of the mono-repo
- see which hooks will run on the next commit, and on which staged files
- run a hook, or the whole pre-commit, and read its output
- add, edit and delete hooks without touching the JSON
- install the Git hooks
- **Graph** view: the hooks of a trigger as connected nodes, like in Blender
  - nodes are laid out from the config (no positions to save): each node sits right of what it waits for
  - drag from an output to a node to pass it as an input (`inputs`), or to its *waits for* row to add an order dependency (`needs`)
  - `×` on a link removes it; cycles are refused
  - run the trigger and see each node's result and duration
  - edits are written to the `hooker.config.json` files, so the JSON stays the source of truth
  - double-click the background (or drop a wire on it) to create a hook, hover a node to delete it
  - variables are drawn from the hooks writing them to the hooks reading them
- **History**: the last runs and the slowest hooks
- the output of a hook shows while it runs
- **Impact**: pick a file and see the hooks it triggers on commit and push, and the projects depending on it
- **Project graph**: the projects of the mono-repo and their dependencies
- **Fix files**: runs the fixers on the changed files and shows their diff, to stage or revert each file
- turn a hook off on this computer only (written to `hooker.local.json`, used by Git too)
- **Command palette** (⌘/Ctrl+K): search the actions, the pages and the hooks, open or run them
- **Doctor**: what could keep the hooks from running, with a button for the fixes the app can do
- the **Source** of a hook: its JSON in the config file, the scripts it runs, the hooks file it loads; **Edit the JSON** opens an editor with colors and autocompletion (keys, triggers, hooks, variables, outputs) that checks the file while you type
- the graph edits a node in a panel next to it, and its changes can be undone (⌘/Ctrl+Z)
- **Variables**: value, writers and readers of each variable
- **Browse hooks**: search the hooks shared on GitHub, set their parameters and add them
- **To review**: the commands and permissions of the hooks of other repositories, to trust them
- a repository without config offers to import husky, lint-staged, lefthook or pre-commit
- a menu bar (native on macOS, in the title bar on Linux and Windows) with shortcuts: ⌘/Ctrl+O open a repository, ⌘/Ctrl+N new hook, ⌘/Ctrl+R run the pre-commit, ⌘/Ctrl+Shift+R reload the configs, ⌘/Ctrl+1 / 2 list or graph, ⌘/Ctrl+Shift+D dark mode
