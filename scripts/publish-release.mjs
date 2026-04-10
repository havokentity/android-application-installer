#!/usr/bin/env node

/**
 * publish-release.mjs — Publish the latest draft GitHub Release.
 *
 * Usage:
 *   node scripts/publish-release.mjs              publish latest draft
 *   node scripts/publish-release.mjs v1.4.2       publish a specific tag
 *
 * Requires: `gh` CLI (GitHub CLI) authenticated.
 */

import { execSync } from "child_process";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");

function run(cmd) {
  return execSync(cmd, { cwd: root, encoding: "utf-8", stdio: "pipe" }).trim();
}

// ─── Resolve which release to publish ────────────────────────────────────────

const arg = process.argv[2];
let tag;

if (arg) {
  tag = arg.startsWith("v") ? arg : `v${arg}`;
  console.log(`\n  Publishing release for tag: ${tag}\n`);
} else {
  // Find the latest draft release
  console.log("\n  Looking for the latest draft release...\n");
  try {
    const drafts = run('gh release list --json tagName,isDraft --jq "[.[] | select(.isDraft)] | .[0].tagName"');
    if (!drafts) {
      console.error("  ✗  No draft releases found.\n");
      process.exit(1);
    }
    tag = drafts;
    console.log(`  Found draft release: ${tag}\n`);
  } catch (e) {
    console.error(`  ✗  Failed to list releases: ${e.message}`);
    console.error("     Make sure `gh` is installed and authenticated.\n");
    process.exit(1);
  }
}

// ─── Verify it's currently a draft ───────────────────────────────────────────

try {
  const isDraft = run(`gh release view ${tag} --json isDraft --jq .isDraft`);
  if (isDraft !== "true") {
    console.log(`  Release ${tag} is already published (not a draft).\n`);
    process.exit(0);
  }
} catch (e) {
  console.error(`  ✗  Release ${tag} not found: ${e.message}\n`);
  process.exit(1);
}

// ─── Publish ─────────────────────────────────────────────────────────────────

try {
  run(`gh release edit ${tag} --draft=false`);
  console.log(`  ✓  Published release ${tag}!\n`);
  console.log(`  View: https://github.com/havokentity/android-application-installer/releases/tag/${tag}\n`);
} catch (e) {
  console.error(`  ✗  Failed to publish: ${e.message}\n`);
  process.exit(1);
}
