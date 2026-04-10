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
 *   3. Extracts "What's New" from the [Unreleased] section of CHANGES.md
 *      (falls back to [x.y.z] section, then auto-generates from git log)
 *   4. Promotes [Unreleased] → [x.y.z] — <date> in CHANGES.md
 *   5. Writes .release-notes.md (used by CI for GitHub Release body)
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
import { categorizeCommits, buildSections, COMMIT_SEP, LOG_FORMAT } from "./lib/categorize-commits.mjs";

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

// ─── Extract "What's New" from CHANGES.md ────────────────────────────────────
// 1. Try the [Unreleased] section first (the intended workflow)
// 2. Fall back to a [x.y.z] section if someone already promoted it manually
// 3. Last resort: auto-generate from git log

const changesPath = resolve(root, "CHANGES.md");
let changesContent = "";
let releaseNotes = "";

try {
  changesContent = readFileSync(changesPath, "utf-8");

  // First, try the [Unreleased] section
  releaseNotes = extractVersionNotes(changesContent, "Unreleased");
  if (releaseNotes) {
    console.log(`  Found [Unreleased] section in CHANGES.md — using it for release notes.\n`);
  } else {
    // Maybe the user already renamed it to [x.y.z]
    releaseNotes = extractVersionNotes(changesContent, version);
    if (releaseNotes) {
      console.log(`  Found [${version}] section in CHANGES.md.\n`);
    }
  }
} catch {
  // CHANGES.md doesn't exist — that's okay, we'll fall back to git log
}

if (!releaseNotes) {
  // Auto-generate release notes from git log (commits since last tag)
  console.log(`  No CHANGES.md entry found — generating notes from git log...\n`);
  releaseNotes = generateNotesFromGitLog();

  if (releaseNotes) {
    console.log(`  Auto-generated release notes from commit history.\n`);
  } else {
    console.warn(`\n  ⚠  No commits found since last tag and no CHANGES.md entry.`);
    console.warn(`     The release will proceed without "What's New" notes.`);
    console.warn(`     Consider adding entries under ## [Unreleased] in CHANGES.md before releasing.\n`);
  }
}

// ─── Promote [Unreleased] → [x.y.z] in CHANGES.md ──────────────────────────

if (changesContent && /^## \[Unreleased\]/m.test(changesContent)) {
  const today = new Date().toISOString().slice(0, 10); // YYYY-MM-DD
  const promoted = changesContent.replace(
    /^## \[Unreleased\]\s*$/m,
    `## [Unreleased]\n\n---\n\n## [${version}] — ${today}`
  );
  writeFileSync(changesPath, promoted, "utf-8");
  console.log(`  Promoted [Unreleased] → [${version}] — ${today} in CHANGES.md\n`);
}

// ─── Write release notes file for CI ─────────────────────────────────────────

const notesPath = resolve(root, ".release-notes.md");
const fullNotes = buildReleaseBody(version, releaseNotes);
writeFileSync(notesPath, fullNotes, "utf-8");
console.log(`  Wrote release notes to .release-notes.md\n`);

// ─── Git commit + tag + push ─────────────────────────────────────────────────

console.log(`  Creating commit and tag ${tag}...\n`);

runLoud("git add package.json package-lock.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock .release-notes.md CHANGES.md");

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
    lastTag = run("git describe --tags --abbrev=0 HEAD");
  } catch {
    lastTag = null;
  }

  const range = lastTag ? `${lastTag}..HEAD` : "HEAD";
  let log;
  try {
    log = run(`git log ${range} --pretty=format:${LOG_FORMAT} --no-merges`);
  } catch {
    return "";
  }

  if (!log) return "";
  return buildSections(categorizeCommits(log));
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

> **🍎 macOS:** On first launch, right-click the app → **Open** → click **Open** to bypass the Gatekeeper prompt.
`;

  if (notes) {
    body += `\n### What's New in v${ver}\n\n${notes}\n`;
  } else {
    body += `\n### What's New\nSee the [commit history](https://github.com/havokentity/android-application-installer/commits/v${ver}) for changes.\n`;
  }

  body += `\n---\n\nFull changelog: [CHANGES.md](https://github.com/havokentity/android-application-installer/blob/main/CHANGES.md)\n`;
  return body;
}



