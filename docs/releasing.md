# Releasing

Nothing is released until you ask for it. To release, push a commit `chore: release vX.Y.Z` on `main`:

```sh
git commit --allow-empty -m "chore: release v0.5.0"
git push
```

A pull request whose squashed commit is titled `chore: release v0.5.0` works too.

The release workflow then:

1. sets the version to `X.Y.Z` and writes `CHANGELOG.md` from the [conventional commits](https://www.conventionalcommits.org) since the last release, in a `chore: bump version to vX.Y.Z` commit (skipped when the commit already has this version)
2. publishes `goat-hooker` on crates.io, creates the `vX.Y.Z` tag and the GitHub release
3. builds the CLI archives and the app installers for every platform and adds them to the release

Other pushes release nothing, whatever their type (`feat:`, `fix:`...). The version must be higher than the current one.

The repository needs a `CARGO_REGISTRY_TOKEN` secret (crates.io token with the `publish-new` and `publish-update` scopes).

The Homebrew tap (`zuygui/homebrew-tap`, with `Formula/` and `Casks/`) is updated by the release workflow when the `HOMEBREW_TAP_TOKEN` secret holds a token allowed to push to it.
