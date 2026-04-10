#!/usr/bin/env node

/**
 * release.mjs — Bump version, commit, tag, and push to trigger the CI release.
 *
 * Usage:
 *   node scripts/release.mjs <version>         e.g. 1.2.0
 *   node scripts/release.mjs patch|minor|major
 *   node scripts/release.mjs patch --skip-tests
 *
 * What it does:
 *   1. Runs all tests (vitest + cargo test via `npm run test:all`)
 *   2. Runs bump-version.mjs to update all version files
 *   3. Extracts "What's New" from CHANGES.md for this version
 *   4. Writes .release-notes.md (used by CI for GitHub Release body)
 *   5. Promotes the [Unreleased] section in CHANGES.md to the new version
 *   6. Stages all changed files (including CHANGES.md + .release-notes.md)
 *   7. Commits with message "release: v<version>"
 *   8. Creates a git tag v<version>
 *   9. Pushes the commit and tag to origin
 *
 * The tag push triggers the GitHub Actions build.yml workflow which
 * builds for all platforms and creates a GitHub Release draft with
 * the "What's New" notes from CHANGES.md.
 *
 * Workflow:
 *   1. Add your changes under ## [Unreleased] in CHANGES.md
 *   2. Run: node scripts/release.mjs patch
 *   3. The script promotes [Unreleased] → [x.y.z], generates release notes,
 *      commits, tags, and pushes — all automatically.
 *
 * Safety:
 *   - Runs all tests first (frontend + Rust); aborts on failure
 *   - Refuses to release with uncommitted changes (dirty working tree)
 *   - Refuses to downgrade (inherited from bump-version.mjs)
 *   - Requires an explicit version argument
 *   - Warns (but proceeds) if no CHANGES.md entry is found
 *   - Use --skip-tests to bypass the test gate (not recommended)
 */

import { execSync } from "child_process";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");

function run(cmd, opts = {}) {
  return execSync(cmd, { cwd: root, encoding: "utf-8", stdio: "pipe", ...opts }).trim();
}

function runLoud(cmd) {
  execSync(cmd, { cwd: root, stdio: "inherit" });
}

// ─── Validate args ───────────────────────────────────────────────────────────

const arg = process.argv[2];

if (!arg) {
  console.error(`
  Usage:
    node scripts/release.mjs <version>       e.g. 1.2.0
    node scripts/release.mjs patch           bump patch (x.y.Z)
    node scripts/release.mjs minor           bump minor (x.Y.0)
    node scripts/release.mjs major           bump major (X.0.0)

  Options:
    --skip-tests    Skip running tests before release (not recommended)

  This will run tests, bump versions, commit, tag, and push to trigger a CI release.
`);
  process.exit(1);
}

// ─── Check for dirty working tree ────────────────────────────────────────────

const status = run("git status --porcelain");
if (status) {
  console.error(`
  ✗  Working tree is dirty. Commit or stash your changes first.

  Uncommitted files:
${status.split("\n").map(l => `    ${l}`).join("\n")}
`);
  process.exit(1);
}

// ─── Check we're on a branch that can push ───────────────────────────────────

let branch;
try {
  branch = run("git rev-parse --abbrev-ref HEAD");
} catch {
  console.error("\n  ✗  Could not determine current branch.\n");
  process.exit(1);
}

console.log(`\n  Branch: ${branch}`);

// ─── Run tests ───────────────────────────────────────────────────────────────

const skipTests = process.argv.includes("--skip-tests");

if (skipTests) {
  console.log("\n  ⚠  Skipping tests (--skip-tests flag)\n");
} else {
  console.log("\n  Running all tests before release...\n");

  try {
    runLoud("npm run test:all");
    console.log("\n  ✓  All tests passed\n");
  } catch {
    console.error("\n  ✗  Tests failed. Fix them before releasing.");
    console.error("     Run `npm run test:all` to see failures.");
    console.error("     To release anyway: node scripts/release.mjs <version> --skip-tests\n");
    process.exit(1);
  }
}

// ─── Bump version ────────────────────────────────────────────────────────────

console.log("");
try {
  runLoud(`node scripts/bump-version.mjs ${arg}`);
} catch {
  // bump-version.mjs already printed the error
  process.exit(1);
}

// ─── Read the version that was just written ──────────────────────────────────

const { readFileSync, writeFileSync } = await import("fs");
const tauriConf = JSON.parse(readFileSync(resolve(root, "src-tauri/tauri.conf.json"), "utf-8"));
const version = tauriConf.version;
const tag = `v${version}`;

// ─── Check if tag already exists ─────────────────────────────────────────────

try {
  run(`git rev-parse ${tag}`);
  console.error(`\n  ✗  Tag ${tag} already exists. Choose a different version.\n`);
  process.exit(1);
} catch {
  // Tag doesn't exist — good
}

// ─── Promote [Unreleased] → version heading in CHANGES.md ───────────────────

const changesPath = resolve(root, "CHANGES.md");
try {
  let changesContent = readFileSync(changesPath, "utf-8");
  const today = new Date().toISOString().slice(0, 10);
  const unreleasedRe = /^## \[Unreleased\]\s*$/m;

  if (unreleasedRe.test(changesContent)) {
    changesContent = changesContent.replace(
      unreleasedRe,
      `## [Unreleased]\n\n---\n\n## [${version}] — ${today}`
    );
    writeFileSync(changesPath, changesContent, "utf-8");
    console.log(`  Promoted [Unreleased] → [${version}] in CHANGES.md\n`);
  }
} catch {
  // non-fatal
}

// ─── Extract "What's New" from CHANGES.md ────────────────────────────────────

let releaseNotes = "";

try {
  const changesContent = readFileSync(changesPath, "utf-8");
  releaseNotes = extractVersionNotes(changesContent, version);
} catch {
  // CHANGES.md doesn't exist — that's okay, we'll fall back to git log
}

if (!releaseNotes) {
  // Auto-generate release notes from git log (commits since last tag)
  console.log(`  No CHANGES.md entry for ${version} — generating notes from git log...\n`);
  releaseNotes = generateNotesFromGitLog();

  if (releaseNotes) {
    console.log(`  Auto-generated release notes from commit history.\n`);
  } else {
    console.warn(`\n  ⚠  No commits found since last tag and no CHANGES.md entry.`);
    console.warn(`     The release will proceed without "What's New" notes.`);
    console.warn(`     Consider adding entries under ## [Unreleased] in CHANGES.md before releasing.\n`);
  }
}

// ─── Write release notes file for CI ─────────────────────────────────────────

const notesPath = resolve(root, ".release-notes.md");
const fullNotes = buildReleaseBody(version, releaseNotes);
writeFileSync(notesPath, fullNotes, "utf-8");
console.log(`  Wrote release notes to .release-notes.md\n`);

// ─── Git commit + tag + push ─────────────────────────────────────────────────

console.log(`  Creating commit and tag ${tag}...\n`);

runLoud("git add package.json package-lock.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock CHANGES.md .release-notes.md");

// If bump wrote the same values (e.g. files already at this version), there's nothing to commit
const staged = run("git diff --cached --name-only");
if (staged) {
  runLoud(`git commit -m "release: ${tag}"`);
} else {
  console.log("  (version files already up to date — no commit needed)\n");
}

runLoud(`git tag ${tag}`);

console.log(`\n  Pushing to origin/${branch} with tag ${tag}...\n`);

runLoud(`git push origin ${branch}`);
runLoud(`git push origin ${tag}`);

console.log(`
  ✓  Released ${tag}!

  The GitHub Actions workflow will now:
    1. Build for macOS (ARM64 + x64), Windows, and Linux
    2. Create a GitHub Release draft with all artifacts
    3. Include "What's New" from CHANGES.md in the release notes

  Check progress: https://github.com/havokentity/android-application-installer/actions
  Releases:       https://github.com/havokentity/android-application-installer/releases
`);

// ─── Helper: extract notes for a specific version from CHANGES.md ────────────

/**
 * Parses CHANGES.md and returns the content under the `## [version]` heading.
 * Stops at the next `## [` heading or end of file.
 */
function extractVersionNotes(content, ver) {
  const lines = content.split("\n");
  let capturing = false;
  const result = [];

  for (const line of lines) {
    // Match ## [1.2.3] or ## [Unreleased]
    if (/^## \[/.test(line)) {
      if (capturing) break; // hit the next version → stop
      // Check if this is the version we want (or [Unreleased] if ver === "unreleased")
      if (line.includes(`[${ver}]`)) {
        capturing = true;
        continue; // skip the heading itself
      }
    }
    if (capturing) {
      result.push(line);
    }
  }

  // Trim leading/trailing blank lines and separator lines
  const text = result.join("\n").replace(/^[\s-]+/, "").replace(/[\s-]+$/, "").trim();
  return text;
}

/**
 * Auto-generate release notes from git log since the last tag.
 * Groups commits by conventional-commit-style prefixes (add, fix, update, etc.)
 */
function generateNotesFromGitLog() {
  let lastTag;
  try {
    // Get the tag before the one we're about to create
    lastTag = run("git describe --tags --abbrev=0 HEAD");
  } catch {
    // No previous tags — use all history
    lastTag = null;
  }

  const range = lastTag ? `${lastTag}..HEAD` : "HEAD";
  let log;
  try {
    log = run(`git log ${range} --pretty=format:"%s" --no-merges`);
  } catch {
    return "";
  }

  if (!log) return "";

  const lines = log.split("\n").filter(l => l.trim());

  // Categorize commits
  const added = [];
  const changed = [];
  const fixed = [];
  const other = [];

  for (const line of lines) {
    const clean = line.replace(/^"|"$/g, "").trim();
    if (!clean || clean.startsWith("release:")) continue;

    const lower = clean.toLowerCase();
    if (lower.startsWith("add:") || lower.startsWith("feat:") || lower.startsWith("feature:")) {
      added.push(formatCommitLine(clean));
    } else if (lower.startsWith("fix:") || lower.startsWith("bugfix:")) {
      fixed.push(formatCommitLine(clean));
    } else if (lower.startsWith("update:") || lower.startsWith("refactor:") || lower.startsWith("chore:")) {
      changed.push(formatCommitLine(clean));
    } else {
      other.push(`- ${clean}`);
    }
  }

  const sections = [];
  if (added.length) sections.push(`### Added\n${added.join("\n")}`);
  if (changed.length) sections.push(`### Changed\n${changed.join("\n")}`);
  if (fixed.length) sections.push(`### Fixed\n${fixed.join("\n")}`);
  if (other.length) sections.push(`### Other\n${other.join("\n")}`);

  return sections.join("\n\n");
}

/** Strip the conventional-commit prefix and format as a bullet point. */
function formatCommitLine(line) {
  // Remove "prefix: " from the start
  const colonIdx = line.indexOf(":");
  if (colonIdx > 0 && colonIdx < 20) {
    const rest = line.slice(colonIdx + 1).trim();
    // Capitalize first letter
    return `- ${rest.charAt(0).toUpperCase()}${rest.slice(1)}`;
  }
  return `- ${line}`;
}

/**
 * Builds the full GitHub Release body with downloads table + what's new.
 */
function buildReleaseBody(ver, notes) {
  let body = `## Downloads

| Platform | File |
|----------|------|
| macOS (Apple Silicon) | \`.dmg\` |
| macOS (Intel) | \`.dmg\` |
| Windows (installer) | \`.msi\` or \`-setup.exe\` |
| Windows (portable) | \`-portable.exe\` — no install needed |
| Linux | \`.deb\` or \`.AppImage\` |
`;

  if (notes) {
    body += `\n### What's New in v${ver}\n\n${notes}\n`;
  } else {
    body += `\n### What's New\nSee the [commit history](https://github.com/havokentity/android-application-installer/commits/v${ver}) for changes.\n`;
  }

  body += `\n---\n\nFull changelog: [CHANGES.md](https://github.com/havokentity/android-application-installer/blob/main/CHANGES.md)\n`;
  return body;
}



