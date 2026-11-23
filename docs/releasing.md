# Releasing

Releases are automatic, from the [conventional commits](https://www.conventionalcommits.org):

1. every push on `main` opens or updates a `chore: release` pull request, with the new version and the `CHANGELOG.md`
2. merging it publishes `goat-hooker` on crates.io, creates the `vX.Y.Z` tag and the GitHub release
3. the CLI archives and the app installers are built for every platform and added to the release

The repository needs a `CARGO_REGISTRY_TOKEN` secret (crates.io token with the `publish-new` and `publish-update` scopes), and GitHub Actions allowed to create pull requests (Settings → Actions → General).

The Homebrew tap (`zuygui/homebrew-tap`, with `Formula/` and `Casks/`) is updated by the release workflow when the `HOMEBREW_TAP_TOKEN` secret holds a token allowed to push to it.
