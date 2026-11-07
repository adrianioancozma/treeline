#!/usr/bin/env bash
# One-time setup of a Debian/Ubuntu machine (including WSL2) for building APMGIT CLIENT.
# Run it from a terminal on that machine:   bash scripts/setup-linux.sh
# It asks for your sudo password once (for apt). Everything else is installed in your home folder.
set -euo pipefail

echo "==> Installing system packages (needs sudo)"
sudo apt-get update
sudo apt-get install -y \
  build-essential curl wget file git pkg-config xdg-utils \
  libssl-dev libwebkit2gtk-4.1-dev libxdo-dev libayatana-appindicator3-dev librsvg2-dev

echo "==> Rust toolchain"
if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
# shellcheck disable=SC1091
. "$HOME/.cargo/env"
rustc --version

echo "==> Node.js 20+ and pnpm"
if ! command -v node >/dev/null 2>&1 || [ "$(node -p 'process.versions.node.split(".")[0]')" -lt 20 ]; then
  echo "Node.js 20 or newer is required. Install it first, for example:"
  echo "  curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash - && sudo apt-get install -y nodejs"
  exit 1
fi
node --version
# Install pnpm into ~/.local so no sudo is needed. (In WSL the Windows pnpm is also on PATH; ours must win.)
mkdir -p "$HOME/.local/bin"
npm install -g pnpm --prefix "$HOME/.local" >/dev/null
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) echo 'export PATH="$HOME/.local/bin:$PATH"' >> "$HOME/.bashrc";; esac
export PATH="$HOME/.local/bin:$PATH"
pnpm --version

echo
echo "Done. Open a NEW terminal (so PATH is refreshed), then see 'Linux' in README.md to build."
