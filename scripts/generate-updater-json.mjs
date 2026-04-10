#!/usr/bin/env node

/**
 * generate-updater-json.mjs — Build the updater.json manifest from GitHub Release assets.
 *
 * Usage:
 *   node scripts/generate-updater-json.mjs v1.6.0
 *
 * This script:
 *   1. Fetches the GitHub Release for the given tag
 *   2. Finds the .sig signature files uploaded by tauri-action
 *   3. Maps each platform to its update URL and signature
 *   4. Writes updater.json to the project root
 *
 * Requires: GITHUB_TOKEN env var (provided by GitHub Actions).
 */

import { writeFileSync } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const root = resolve(__dirname, "..");

const REPO = "havokentity/android-application-installer";

// ─── Platform → asset pattern mapping ─────────────────────────────────────────
// Tauri v2 updater expects these platform keys.
// The .sig files contain the Ed25519 signature for each artifact.
const PLATFORM_MAP = {
  "darwin-aarch64": {
    artifact: /\.app\.tar\.gz$/,
    sig: /\.app\.tar\.gz\.sig$/,
  },
  "darwin-x86_64": {
    artifact: /\.app\.tar\.gz$/,
    sig: /\.app\.tar\.gz\.sig$/,
  },
  "linux-x86_64": {
    artifact: /\.AppImage\.tar\.gz$/,
    sig: /\.AppImage\.tar\.gz\.sig$/,
  },
  "windows-x86_64": {
    artifact: /\.nsis\.zip$/,
    sig: /\.nsis\.zip\.sig$/,
  },
};

// macOS builds produce two .app.tar.gz files — distinguish by arch substring
const MACOS_ARCH_HINTS = {
  "darwin-aarch64": ["aarch64", "arm64"],
  "darwin-x86_64": ["x86_64", "x64", "intel"],
};

// ─── Main ─────────────────────────────────────────────────────────────────────

const tag = process.argv[2];
if (!tag) {
  console.error("\n  Usage: node scripts/generate-updater-json.mjs <tag>\n");
  process.exit(1);
}

const version = tag.replace(/^v/, "");
const token = process.env.GITHUB_TOKEN;
if (!token) {
  console.error("\n  ✗  GITHUB_TOKEN env var is required.\n");
  process.exit(1);
}

async function fetchJSON(url) {
  const res = await fetch(url, {
    headers: {
      Authorization: `token ${token}`,
      Accept: "application/vnd.github.v3+json",
    },
  });
  if (!res.ok) throw new Error(`GitHub API ${res.status}: ${await res.text()}`);
  return res.json();
}

async function fetchText(url) {
  const res = await fetch(url, {
    headers: {
      Authorization: `token ${token}`,
      Accept: "application/octet-stream",
    },
  });
  if (!res.ok) throw new Error(`Failed to fetch ${url}: ${res.status}`);
  return res.text();
}

async function main() {
  console.log(`\n  Generating updater.json for ${tag}...\n`);

  // Fetch release — use the releases list API because /releases/tags/{tag}
  // returns 404 for draft releases (which is what tauri-action creates).
  let release;
  try {
    // Try the direct endpoint first (works for published releases)
    release = await fetchJSON(`https://api.github.com/repos/${REPO}/releases/tags/${tag}`);
  } catch {
    // Fall back to listing releases and finding the draft by tag name
    console.log(`  Release not found by tag — searching drafts...\n`);
    const releases = await fetchJSON(`https://api.github.com/repos/${REPO}/releases?per_page=10`);
    release = releases.find((r) => r.tag_name === tag);
    if (!release) {
      throw new Error(`No release found for tag ${tag} (checked published and drafts)`);
    }
    console.log(`  Found draft release: ${release.name || release.tag_name}\n`);
  }

  const assets = release.assets;
  const pubDate = release.published_at || release.created_at;
  const notes = release.body || `See https://github.com/${REPO}/releases/tag/${tag} for details.`;

  console.log(`  Found ${assets.length} release assets.\n`);
  console.log(`  Assets: ${assets.map((a) => a.name).join(", ")}\n`);

  const platforms = {};

  for (const [platform, patterns] of Object.entries(PLATFORM_MAP)) {
    // Find the matching artifact
    let artifactAsset;
    let sigAsset;

    if (platform.startsWith("darwin-")) {
      // For macOS, disambiguate by arch hints
      const hints = MACOS_ARCH_HINTS[platform];
      artifactAsset = assets.find(
        (a) => patterns.artifact.test(a.name) && hints.some((h) => a.name.toLowerCase().includes(h))
      );
      sigAsset = assets.find(
        (a) => patterns.sig.test(a.name) && hints.some((h) => a.name.toLowerCase().includes(h))
      );
    } else {
      artifactAsset = assets.find((a) => patterns.artifact.test(a.name));
      sigAsset = assets.find((a) => patterns.sig.test(a.name));
    }

    if (!artifactAsset) {
      console.log(`  ⚠  No artifact found for ${platform}, skipping.`);
      continue;
    }

    if (!sigAsset) {
      console.log(`  ⚠  No .sig file found for ${platform}, skipping.`);
      continue;
    }

    // Download the signature content (use API URL for draft release compatibility)
    const sigUrl = sigAsset.url; // api.github.com/repos/.../assets/{id}
    const signature = await fetchText(sigUrl);

    platforms[platform] = {
      signature: signature.trim(),
      url: artifactAsset.browser_download_url,
    };

    console.log(`  ✓  ${platform}: ${artifactAsset.name}`);
  }

  if (Object.keys(platforms).length === 0) {
    console.error("\n  ✗  No platform artifacts found. Is the release fully built?\n");
    process.exit(1);
  }

  const updaterJson = {
    version,
    notes,
    pub_date: pubDate,
    platforms,
  };

  const outPath = resolve(root, "updater.json");
  writeFileSync(outPath, JSON.stringify(updaterJson, null, 2) + "\n", "utf-8");
  console.log(`\n  ✓  Wrote updater.json (${Object.keys(platforms).length} platforms)\n`);
}

main().catch((e) => {
  console.error(`\n  ✗  ${e.message}\n`);
  process.exit(1);
});

