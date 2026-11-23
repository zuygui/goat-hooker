#!/bin/sh
# Writes the Homebrew formula (CLI) and cask (app) of a release in a tap.
# Usage: homebrew.sh <version> <sha mac arm> <sha mac intel> <sha linux> <sha dmg arm> <sha dmg intel> <tap dir>
set -eu
version=$1 cli_arm=$2 cli_intel=$3 cli_linux=$4 dmg_arm=$5 dmg_intel=$6 tap=$7
url="https://github.com/zuygui/goat-hooker/releases/download/v$version"

cat > "$tap/Formula/goat-hooker.rb" <<RUBY
class GoatHooker < Formula
  desc "Git hooks manager with mono-repo support"
  homepage "https://github.com/zuygui/goat-hooker"
  version "$version"
  license "MIT"

  on_macos do
    on_arm do
      url "$url/hooker-v$version-aarch64-apple-darwin.tar.gz"
      sha256 "$cli_arm"
    end
    on_intel do
      url "$url/hooker-v$version-x86_64-apple-darwin.tar.gz"
      sha256 "$cli_intel"
    end
  end

  on_linux do
    on_intel do
      url "$url/hooker-v$version-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "$cli_linux"
    end
  end

  def install
    bin.install "hooker"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/hooker --version")
  end
end
RUBY

cat > "$tap/Casks/goat-hooker.rb" <<RUBY
cask "goat-hooker" do
  arch arm: "aarch64", intel: "x64"

  version "$version"
  sha256 arm:   "$dmg_arm",
         intel: "$dmg_intel"

  url "$url/Goat.Hooker_#{version}_#{arch}.dmg"
  name "Goat Hooker"
  desc "Desktop app of the goat-hooker Git hooks manager"
  homepage "https://github.com/zuygui/goat-hooker"

  app "Goat Hooker.app"

  # Not notarized: opened without the quarantine warning
  postflight do
    system_command "/usr/bin/xattr", args: ["-dr", "com.apple.quarantine", "#{appdir}/Goat Hooker.app"]
  end
end
RUBY
