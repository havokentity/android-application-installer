#!/usr/bin/env node

/**
 * release.mjs — Bump version, commit, tag, and push to trigger the CI release.
 *
 * Usage:
 *   node scripts/release.mjs <version>         e.g. 1.2.0
 *   node scripts/release.mjs patch|minor|major
 *
 * What it does:
 *   1. Runs bump-version.mjs to update all version files
 *   2. Stages the changed files
 *   3. Commits with message "release: v<version>"
 *   4. Creates a git tag v<version>
 *   5. Pushes the commit and tag to origin
 *
 * The tag push triggers the GitHub Actions build.yml workflow which
 * builds for all platforms and creates a GitHub Release draft.
 *
 * Safety:
 *   - Refuses to release with uncommitted changes (dirty working tree)
 *   - Refuses to downgrade (inherited from bump-version.mjs)
 *   - Requires an explicit version argument
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

  This will bump versions, commit, tag, and push to trigger a CI release.
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

// ─── Bump version ────────────────────────────────────────────────────────────

console.log("");
try {
  runLoud(`node scripts/bump-version.mjs ${arg}`);
} catch {
  // bump-version.mjs already printed the error
  process.exit(1);
}

// ─── Read the version that was just written ──────────────────────────────────

const { readFileSync } = await import("fs");
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

// ─── Git commit + tag + push ─────────────────────────────────────────────────

console.log(`  Creating commit and tag ${tag}...\n`);

runLoud("git add package.json src-tauri/tauri.conf.json src-tauri/Cargo.toml");
runLoud(`git commit -m "release: ${tag}"`);
runLoud(`git tag ${tag}`);

console.log(`\n  Pushing to origin/${branch} with tag ${tag}...\n`);

runLoud(`git push origin ${branch}`);
runLoud(`git push origin ${tag}`);

console.log(`
  ✓  Released ${tag}!

  The GitHub Actions workflow will now:
    1. Build for macOS (ARM64 + x64), Windows, and Linux
    2. Create a GitHub Release draft with all artifacts

  Check progress: https://github.com/havokentity/android-application-installer/actions
  Releases:       https://github.com/havokentity/android-application-installer/releases
`);

