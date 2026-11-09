# Security

## Reporting a problem

Please **do not open a public issue** for a security problem. Use GitHub's private reporting instead:
on the repository page choose **Security → Report a vulnerability**. Include what you did, what you
expected and what happened, and the version shown at the bottom right of the window.

Only the latest release is supported; fixes are released as a new version and installed through the
app's update check (Settings → the gear icon).

## What Treeline does to protect you

Treeline is a desktop client for Git. It runs the `git` program installed on your computer, so it
treats every repository, branch name, file name and commit message as untrusted input.

- **Repositories that can run programs.** A folder handed to you with its own `.git` can carry
  settings or hooks that make Git run programs. Treeline asks once before opening a repository whose
  own configuration contains such settings or hook scripts, and runs nothing from it before you answer.
  Settings that would run a program on every `status` (`core.fsmonitor`) or when showing a diff
  (external diff programs) are always switched off for the app's own Git calls.
- **Only folders you opened.** The backend accepts a folder only if you chose it with the folder picker,
  it is in your saved tabs and bookmarks, or it lies inside one of those (for example a submodule).
- **No writing through links.** Conflict files are read and written only if neither the file nor any
  folder above it is a symbolic link or junction leading out of the repository.
- **Names are names.** A branch, tag or commit name that Git would read as an option (it starts with
  `-`) is refused.
- **Credentials stay hidden.** A token or password embedded in a remote URL is replaced by `***` in
  error and progress messages.
- **Strict content policy.** The window loads only its own code and styles, with no remote content,
  no inline scripts and no `eval`.
- **Signed updates.** Updates are verified against a public key built into the app.
- **Outdated Git.** At start-up Treeline tells you if your Git predates the 2025 security fixes for
  cloning and submodules.

Things Treeline deliberately does **not** do: it does not disable Git hooks, filters (Git LFS needs
them), merge drivers, credential helpers or commit signing, because normal workflows depend on them.
For those, the question before opening a repository is the protection. Open repositories only from
sources you trust.

The full audit record, with what was fixed, how it was verified and what is knowingly accepted, is in
[docs/SECURITY_AUDIT.md](docs/SECURITY_AUDIT.md).
