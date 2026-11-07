# Releasing a new version

The app checks for updates against `latest.json` on GitHub Releases (address in `src-tauri/tauri.conf.json`,
`plugins.updater.endpoints`). Each update is **signed**; the app refuses anything not signed with our key.

## One-time setup

* The signing key pair was created with `pnpm tauri signer generate`. The **public** key is in `tauri.conf.json`.
  The **private** key lives only on the release machine (`~/.tauri/apmgit-client.key`) and must **never** be
  committed or shared. Back it up: if it is lost, installed copies can never be updated again.
* Create the GitHub repository named in the endpoint (or change the address in `tauri.conf.json` first).

## Steps

1. Raise `version` in `src-tauri/tauri.conf.json`, `package.json` and `src-tauri/Cargo.toml`; commit it.
2. Build with the key available (PowerShell):
   ```
   $env:TAURI_SIGNING_PRIVATE_KEY = "$HOME\.tauri\apmgit-client.key"
   $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""
   pnpm tauri build
   ```
3. `node scripts/make-update-manifest.mjs "What is new in this version"`
4. On GitHub create a release tagged `v<version>` and upload everything from
   `src-tauri/target/release/bundle/release/` (the installer, its `.sig`, and `latest.json`). Publish it as the
   **latest** release.

Running copies then offer the update at start-up and under Settings → Check for updates; the user confirms before
anything is installed. Linux: build in WSL, copy the AppImage and its `.sig` to `bundle/linux/` first.
