#!/usr/bin/env node

/**
 * bump-version.mjs — Sync app version across all config files.
 *
 * Usage:
 *   node scripts/bump-version.mjs <version>
 *   node scripts/bump-version.mjs patch|minor|major
 *   node scripts/bump-version.mjs              (shows current versions)
 *
 * Files updated:
 *   - package.json            → "version"
 *   - src-tauri/tauri.conf.json → "version"
 *   - src-tauri/Cargo.toml      → version (in [package])
 *
 * Safety: refuses to set a version lower than the current highest.
 */

import { readFileSync, writeFileSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");

// ─── Helpers ──────────────────────────────────────────────────────────────────

function parseSemver(v) {
  const m = v.match(/^(\d+)\.(\d+)\.(\d+)/);
  if (!m) return null;
  return { major: +m[1], minor: +m[2], patch: +m[3] };
}

function semverToString(s) {
  return `${s.major}.${s.minor}.${s.patch}`;
}

/** Compare two semver objects. Returns -1, 0, or 1. */
function compareSemver(a, b) {
  if (a.major !== b.major) return a.major > b.major ? 1 : -1;
  if (a.minor !== b.minor) return a.minor > b.minor ? 1 : -1;
  if (a.patch !== b.patch) return a.patch > b.patch ? 1 : -1;
  return 0;
}

// ─── File definitions ─────────────────────────────────────────────────────────

const files = [
  {
    label: "package.json",
    path: resolve(root, "package.json"),
    read(content) {
      return JSON.parse(content).version;
    },
    write(content, version) {
      const json = JSON.parse(content);
      json.version = version;
      return JSON.stringify(json, null, 2) + "\n";
    },
  },
  {
    label: "src-tauri/tauri.conf.json",
    path: resolve(root, "src-tauri/tauri.conf.json"),
    read(content) {
      return JSON.parse(content).version;
    },
    write(content, version) {
      const json = JSON.parse(content);
      json.version = version;
      return JSON.stringify(json, null, 2) + "\n";
    },
  },
  {
    label: "src-tauri/Cargo.toml",
    path: resolve(root, "src-tauri/Cargo.toml"),
    read(content) {
      const m = content.match(/^version\s*=\s*"([^"]+)"/m);
      return m ? m[1] : null;
    },
    write(content, version) {
      return content.replace(
        /^(version\s*=\s*)"[^"]+"/m,
        `$1"${version}"`
      );
    },
  },
];

// ─── Read current versions ────────────────────────────────────────────────────

const current = files.map((f) => {
  const content = readFileSync(f.path, "utf-8");
  const version = f.read(content);
  return { ...f, content, version, semver: parseSemver(version) };
});

const highest = current.reduce((best, f) =>
  compareSemver(f.semver, best) > 0 ? f.semver : best,
  { major: 0, minor: 0, patch: 0 }
);

// ─── No argument → show status ───────────────────────────────────────────────

const arg = process.argv[2];

if (!arg) {
  console.log("\n  Current versions:\n");
  const synced = current.every((f) => f.version === current[0].version);
  for (const f of current) {
    const tag = f.version === semverToString(highest) ? "" : " ← out of date";
    console.log(`    ${f.label.padEnd(30)} ${f.version}${tag}`);
  }
  console.log();
  if (!synced) {
    console.log(`  ⚠  Versions are out of sync. Highest is ${semverToString(highest)}.`);
    console.log(`     Run:  node scripts/bump-version.mjs ${semverToString(highest)}\n`);
  } else {
    console.log("  ✓  All versions are in sync.\n");
  }
  console.log("  Usage:");
  console.log("    node scripts/bump-version.mjs <version>       e.g. 1.2.0");
  console.log("    node scripts/bump-version.mjs patch           1.1.2 → 1.1.3");
  console.log("    node scripts/bump-version.mjs minor           1.1.2 → 1.2.0");
  console.log("    node scripts/bump-version.mjs major           1.1.2 → 2.0.0\n");
  process.exit(0);
}

// ─── Resolve target version ──────────────────────────────────────────────────

let target;

if (arg === "patch") {
  target = { ...highest, patch: highest.patch + 1 };
} else if (arg === "minor") {
  target = { major: highest.major, minor: highest.minor + 1, patch: 0 };
} else if (arg === "major") {
  target = { major: highest.major + 1, minor: 0, patch: 0 };
} else {
  target = parseSemver(arg);
  if (!target) {
    console.error(`\n  ✗  Invalid version: "${arg}". Expected semver like 1.2.3 or patch|minor|major.\n`);
    process.exit(1);
  }
}

// ─── Safety: refuse downgrade ────────────────────────────────────────────────

if (compareSemver(target, highest) < 0) {
  console.error(
    `\n  ✗  Refusing to downgrade: ${semverToString(target)} < ${semverToString(highest)} (current highest).\n` +
    `     Use a version ≥ ${semverToString(highest)}.\n`
  );
  process.exit(1);
}

// ─── Write ───────────────────────────────────────────────────────────────────

const targetStr = semverToString(target);

console.log(`\n  Updating all version files to ${targetStr}:\n`);

for (const f of current) {
  const updated = f.write(f.content, targetStr);
  writeFileSync(f.path, updated, "utf-8");
  const changed = f.version !== targetStr;
  console.log(`    ${changed ? "✓" : "·"} ${f.label.padEnd(30)} ${f.version} → ${targetStr}`);
}

console.log(`\n  Done. Don't forget to:\n`);
console.log(`    git add -A && git commit -m "bump: v${targetStr}"`);
console.log(`    git tag v${targetStr}`);
console.log(`    git push --tags\n`);

