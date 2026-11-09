/**
 * Releases that carry the July 2025 security fixes for cloning and submodules (CVE-2025-48384 and
 * others: a trailing carriage return in a submodule path could write files outside the repository and
 * run hooks). Every maintained line got its own fix; a newer line than the table already has it.
 */
const PATCHED_PATCH_LEVEL: Record<number, number> = { 43: 7, 44: 4, 45: 4, 46: 4, 47: 3, 48: 2, 49: 1, 50: 1 };

/** Reads "git version 2.43.0.windows.1" (or just "2.43.0") into [major, minor, patch]. */
export function parseGitVersion(line: string): [number, number, number] | null {
  const m = /(\d+)\.(\d+)\.(\d+)/.exec(line);
  return m ? [Number(m[1]), Number(m[2]), Number(m[3])] : null;
}

/** True when this Git is older than the security fixes above. Unknown versions are not reported. */
export function gitNeedsSecurityUpdate(versionLine: string | null): boolean {
  const v = versionLine ? parseGitVersion(versionLine) : null;
  if (!v) return false;
  const [major, minor, patch] = v;
  if (major > 2) return false;
  if (major < 2) return true;
  if (minor > 50) return false;
  const fixed = PATCHED_PATCH_LEVEL[minor];
  return fixed === undefined ? true : patch < fixed;
}

/** The notice shown at start-up for an outdated Git. */
export const outdatedGitMessage = (versionLine: string) =>
  `Your Git (${versionLine.replace(/^git version /, "")}) is older than the July 2025 security fixes for cloning and submodules. ` +
  "Update Git before cloning or updating submodules of a repository you do not fully trust. " +
  "(Some Linux distributions backport the fixes without changing the version number.)";
