#!/bin/sh
# Installs the goat-hooker CLI (`hooker`) from the last GitHub release.
#
#   curl -fsSL https://raw.githubusercontent.com/zuygui/goat-hooker/main/install.sh | sh
#
# HOOKER_VERSION=0.3.0 for another version, HOOKER_INSTALL_DIR to install elsewhere
# (default: ~/.local/bin).
set -eu

repo="zuygui/goat-hooker"
dir="${HOOKER_INSTALL_DIR:-$HOME/.local/bin}"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) target=x86_64-unknown-linux-gnu ;;
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  *)
    echo "No build of goat-hooker for $(uname -s) $(uname -m): install it with \`cargo install goat-hooker\`" >&2
    exit 1
    ;;
esac

if [ -n "${HOOKER_VERSION:-}" ]; then
  tag="v${HOOKER_VERSION#v}"
else
  tag=$(curl -fsSL "https://api.github.com/repos/$repo/releases/latest" |
    sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1)
fi
[ -n "$tag" ] || { echo "Can't find the last release of $repo" >&2; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo "Downloading hooker $tag ($target)..."
curl -fsSL "https://github.com/$repo/releases/download/$tag/hooker-$tag-$target.tar.gz" | tar -xz -C "$tmp"
mkdir -p "$dir"
install -m 755 "$tmp/hooker-$tag-$target/hooker" "$dir/hooker"
echo "Installed $dir/hooker"

case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "Add $dir to your PATH, e.g. echo 'export PATH=\"$dir:\$PATH\"' >> ~/.profile" ;;
esac
