# Contributing to Treeline

Thank you for helping! Contributions of every kind are welcome: code, tests, documentation, translations, bug reports,
ideas, and — right now most of all — **testing on Linux and on Intel Macs**, where the app has never been run
(see [docs/CROSS_PLATFORM_TESTING.md](docs/CROSS_PLATFORM_TESTING.md)).

## Ways to contribute

* **Test on Linux or an Intel Mac** and report back (15 minutes with the checklist).
* **Report a bug** — what you did, what you expected, what happened, your OS and `git --version`, and a screenshot. For an
  error panel, use **Copy details**.
* **Suggest or build a feature** — see the ideas list in the [README](README.md#contributing). Please describe the problem
  before sending a large change, so we can agree on the approach.
* **Improve the docs** — the guide in `docs/USER_GUIDE.md` is the single source for the docs pages and the in-app help.
* **Add error explanations** — if Git fails in a way the app explains poorly, add a rule (see below).

Send changes however the repository is hosted: open an issue, or a merge/pull request, or a patch (`git format-patch`).

## Setting up

You need Git 2.30+, Node 20+, pnpm and a Rust toolchain, plus the Tauri prerequisites for your OS (see the README).

```sh
pnpm install
pnpm tauri dev                     # run the app with hot reload
pnpm test                          # TypeScript tests
pnpm exec tsc --noEmit             # type check
cd src-tauri && cargo test --lib   # Rust tests (they run real git in temporary folders)
```

**Before you send a change, all three test commands must pass.**

## Principles to keep

1. **Any Git provider.** No GitHub-, GitLab- or Bitbucket-specific features, accounts or APIs. The app drives the system `git`.
2. **Fast.** Git runs off the UI thread. Don't add polling; use the file watcher for local changes. The one exception is the silent background fetch of the active tab (`src/lib/autoFetch.ts`, every 2 minutes by default, set in Settings), because the server cannot notify us. Read-only git commands must keep
   `GIT_OPTIONAL_LOCKS=0` (the `git_cmd()` helper sets it). Don't set `GIT_LITERAL_PATHSPECS` globally — it breaks
   `git stash -u`; pass `LITERAL` only to commands that take user file names.
3. **Errors explain themselves.** A failure must reach the user through the error panel with a title, why, and fixes
   (`src/lib/explain.ts`). Add a rule **and a test** (`src/lib/explain.test.ts`) for a new case.
4. **Risky operations are confirmed.** Anything that can lose work (discard, reset --hard, force push, delete) asks first and
   says what will be lost. "Force" options are never automatic and are offered last, only when the failed operation is known.
5. **Never lose work silently.** Prefer refusing and explaining over guessing (see how `resolve_side` and rebase handle it).
6. **Cross-platform.** Paths, shells and key bindings differ per OS. Don't assume Windows, and don't run commands through
   a shell with user-supplied text. Keep `cfg(...)` blocks small and covered by tests where possible.

## Code conventions

* **Tests with every change.** Pure logic goes in `src/lib/` with a `*.test.ts` file. Git behaviour is covered by Rust tests in
  `src-tauri/src/*.rs` that create real repositories in temporary folders — copy an existing test as a starting point.
* Keep functions small and comment the *why* (especially around git quirks). No dead code or unused dependencies.
* TypeScript is strict; no `any` without a reason. Rust code should build without warnings.
* **User-facing text and documentation use British English** ("initialise", "colour", "licence").
* If behaviour changes, **update `docs/USER_GUIDE.md`**, then run `pnpm site:build` and `node scripts/check-site.mjs`, and commit
  the regenerated `site/docs/` with it.
* Shell scripts must use LF line endings (enforced by `.gitattributes`).

## Commits and changes

* Small, focused commits, each leaving the tests green. Write the commit message as a short summary line, a blank line, and a
  paragraph explaining **why**.
* Don't mix refactoring with behaviour changes.
* Mention how you tested it, and on which OS. Say plainly if you could not test something.

## Reporting security problems

Please don't post security issues publicly. Contact the maintainers privately first.

## Licence

No licence has been chosen yet; the maintainers need to add one before outside contributions can be accepted. Please ask
before sending substantial work.
