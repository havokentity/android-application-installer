#!/usr/bin/env node

/**
 * update-changelog.mjs — Auto-populate the [Unreleased] section of CHANGES.md
 * from git commit history since the last tag.
 *
 * Usage:
 *   node scripts/update-changelog.mjs            generate from git log
 *   node scripts/update-changelog.mjs --preview   preview without writing
 *
 * Commits are categorised by their prefix:
 *   add:/feat:/feature:  → ### Added
 *   fix:/bugfix:         → ### Fixed
 *   update:/refactor:/chore: → ### Changed
 *   delete:/remove:      → ### Removed
 *   everything else      → ### Other
 *
 * Multi-category commits: if the commit body contains additional lines
 * starting with a recognised prefix, each line is categorised separately.
 * Example commit message:
 *   add: stop app button
 *   fix: layout shift bug
 *   update: header styling
 *
 * Commits starting with "release:" are skipped.
 *
 * The script REPLACES any existing content under ## [Unreleased] — if you've
 * hand-edited that section, back it up first or use --preview to inspect.
 */

import { execSync } from "child_process";
import { readFileSync, writeFileSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";
import { categorizeCommits, buildSections, COMMIT_SEP, LOG_FORMAT } from "./lib/categorize-commits.mjs";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");
const changesPath = resolve(root, "CHANGES.md");

function run(cmd) {
  return execSync(cmd, { cwd: root, encoding: "utf-8", stdio: "pipe" }).trim();
}

const preview = process.argv.includes("--preview");

// ─── Find commits since last tag ────────────────────────────────────────────

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
  console.error("  ✗  Failed to read git log.\n");
  process.exit(1);
}

if (!log) {
  console.log(`\n  No new commits since ${lastTag || "beginning"} — nothing to do.\n`);
  process.exit(0);
}

console.log(`\n  Commits since ${lastTag || "(initial)"}:\n`);

// ─── Categorise commits ─────────────────────────────────────────────────────

const cats = categorizeCommits(log);
const generated = buildSections(cats);

if (!generated) {
  console.log("  No categorisable commits found (only release commits?).\n");
  process.exit(0);
}

// ─── Preview mode ───────────────────────────────────────────────────────────

if (preview) {
  console.log("  ── Preview (not written) ──────────────────────────────\n");
  console.log(`## [Unreleased]\n\n${generated}\n`);
  console.log("  ──────────────────────────────────────────────────────\n");
  process.exit(0);
}

// ─── Write into CHANGES.md ──────────────────────────────────────────────────

let content;
try {
  content = readFileSync(changesPath, "utf-8");
} catch {
  console.error(`  ✗  Could not read ${changesPath}\n`);
  process.exit(1);
}

// Strategy: find the ## [Unreleased] line and replace everything between it
// and the next ## [ heading (or ---) with the generated content.
const unreleasedRe = /^## \[Unreleased\]\s*$/m;

if (!unreleasedRe.test(content)) {
  console.error("  ✗  No ## [Unreleased] heading found in CHANGES.md.\n");
  process.exit(1);
}

// Split at the [Unreleased] heading
const headingIdx = content.search(unreleasedRe);
const afterHeading = content.indexOf("\n", headingIdx) + 1;

// Find the next section boundary (## [ or end-of-file)
const rest = content.slice(afterHeading);
const nextSectionMatch = rest.match(/^(---\s*\n\s*)?## \[/m);
const insertEnd = nextSectionMatch
  ? afterHeading + nextSectionMatch.index
  : content.length;

const before = content.slice(0, afterHeading);
const after = content.slice(insertEnd);

const newContent = `${before}\n${generated}\n\n${after}`;

writeFileSync(changesPath, newContent, "utf-8");

console.log("  ✓  Updated [Unreleased] in CHANGES.md:\n");
for (const heading of generated.split("\n").filter(l => l.startsWith("### "))) {
  const section = generated.split(heading)[1]?.split("###")[0] || "";
  const count = section.split("\n").filter(l => l.startsWith("- ")).length;
  console.log(`     ${heading} (${count} ${count === 1 ? "entry" : "entries"})`);
}
console.log("");
