#!/usr/bin/env bash
# Tests and builds the Linux packages (.deb, .AppImage, .rpm). Run after scripts/setup-linux.sh.
# IMPORTANT (WSL): run this from a copy on the Linux filesystem (e.g. ~/apmgit-client), NOT from /mnt/c/...
# - the Windows drive is many times slower and OneDrive folders make it worse.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"

case "$(pwd)" in
  /mnt/*) echo "Refusing to build under /mnt (slow, and breaks file watching). Copy the repo into your Linux home first:"
          echo "  git clone \"$(pwd)\" ~/apmgit-client && cd ~/apmgit-client"
          exit 1;;
esac

echo "==> Installing JavaScript dependencies"
pnpm install --frozen-lockfile

echo "==> TypeScript: type check and tests"
pnpm exec tsc --noEmit
pnpm test

echo "==> Rust: tests (these run real git operations on Linux)"
(cd src-tauri && cargo test --lib)

echo "==> Building packages"
pnpm tauri build

echo
echo "Packages are in src-tauri/target/release/bundle/ (deb/, appimage/, rpm/)"
