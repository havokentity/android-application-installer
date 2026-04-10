/**
 * categorize-commits.mjs — Shared commit categorization logic.
 *
 * Used by both update-changelog.mjs and release.mjs to avoid duplication.
 */

/**
 * Strip a conventional-commit prefix and format as a markdown bullet.
 *   "add: stop app button" → "- Stop app button"
 */
export function formatCommitLine(line) {
  const colonIdx = line.indexOf(":");
  if (colonIdx > 0 && colonIdx < 20) {
    const rest = line.slice(colonIdx + 1).trim();
    return `- ${rest.charAt(0).toUpperCase()}${rest.slice(1)}`;
  }
  return `- ${line}`;
}

/**
 * Categorize git log output into Keep a Changelog sections.
 *
 * Supports multi-prefix commits: body lines starting with a recognised
 * prefix are categorised independently of the subject line.
 *
 * @param {string} log  Raw git log output using the COMMIT_SEP format
 * @returns {{ added: string[], changed: string[], fixed: string[], removed: string[], other: string[] }}
 */
export function categorizeCommits(log) {
  const added = [];
  const changed = [];
  const fixed = [];
  const removed = [];
  const other = [];

  const commits = log.split(COMMIT_SEP).filter(c => c.trim());

  for (const commit of commits) {
    const lines = commit.split("\n").filter(l => l.trim());
    let subjectHandled = false;

    for (const line of lines) {
      const clean = line.replace(/^"|"$/g, "").trim();
      if (!clean || clean.startsWith("release:")) continue;

      const lower = clean.toLowerCase();
      if (lower.startsWith("add:") || lower.startsWith("feat:") || lower.startsWith("feature:")) {
        added.push(formatCommitLine(clean));
        subjectHandled = true;
      } else if (lower.startsWith("fix:") || lower.startsWith("bugfix:")) {
        fixed.push(formatCommitLine(clean));
        subjectHandled = true;
      } else if (lower.startsWith("update:") || lower.startsWith("refactor:") || lower.startsWith("chore:")) {
        changed.push(formatCommitLine(clean));
        subjectHandled = true;
      } else if (lower.startsWith("delete:") || lower.startsWith("remove:")) {
        removed.push(formatCommitLine(clean));
        subjectHandled = true;
      } else if (!subjectHandled) {
        other.push(`- ${clean}`);
        subjectHandled = true;
      }
    }
  }

  return { added, changed, fixed, removed, other };
}

/**
 * Build markdown sections string from categorized commits.
 * @param {{ added: string[], changed: string[], fixed: string[], removed: string[], other: string[] }} cats
 * @returns {string}
 */
export function buildSections(cats) {
  const sections = [];
  if (cats.added.length)   sections.push(`### Added\n${cats.added.join("\n")}`);
  if (cats.changed.length) sections.push(`### Changed\n${cats.changed.join("\n")}`);
  if (cats.fixed.length)   sections.push(`### Fixed\n${cats.fixed.join("\n")}`);
  if (cats.removed.length) sections.push(`### Removed\n${cats.removed.join("\n")}`);
  if (cats.other.length)   sections.push(`### Other\n${cats.other.join("\n")}`);
  return sections.join("\n\n");
}

/** Separator used in git log --pretty format to split commits. */
export const COMMIT_SEP = "---COMMIT_SEP---";

/** Git log format string that includes subject + body with separators. */
export const LOG_FORMAT = `"${COMMIT_SEP}%n%s%n%b"`;
