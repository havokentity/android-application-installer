# Release Guide

Step-by-step instructions for releasing a new version of **Android Application Installer**.

---

## Prerequisites

- [ ] [Node.js](https://nodejs.org/) ≥ 18 installed
- [ ] [Rust](https://rustup.rs/) stable toolchain installed (for Rust tests)
- [ ] `gh` CLI installed and authenticated (only for `npm run release:publish`)
- [ ] Git remote `origin` points to `havokentity/android-application-installer`
- [ ] GitHub Actions secrets configured (`TAURI_SIGNING_PRIVATE_KEY`, etc.)

---

## Quick Reference

```bash
# Full release (most common):
npm run changelog              # populate [Unreleased] from commits
# review CHANGES.md, edit if needed, commit edits
npm run release:patch          # or release:minor / release:major

# After CI completes:
npm run release:publish        # publish the draft GitHub Release
```

---

## Step-by-Step Release Process

### 1. Ensure you're on `main` with a clean tree

If you're on a **feature branch**, merge to `main` first:

```bash
# Switch to main and pull latest
git checkout main
git pull origin main

# Merge your feature branch
git merge feature/<branch-name>

# Resolve conflicts if any, then push
git push origin main
```

> **Why `main`?** The release script pushes to the current branch. Releases must come from `main` so that:
> - The CI workflow's `update-updater-json` job commits `updater.json` back to `main`
> - Tags are on the `main` history
> - The changelog and version files are on `main`

Verify you're on `main` with a clean working tree:

```bash
git status
# Should show: On branch main, nothing to commit, working tree clean
```

If there are uncommitted changes, commit or stash them first. The release script will **refuse to run** on a dirty tree.

---

### 2. Generate changelog entries from commits

Run the changelog script to auto-populate the `[Unreleased]` section of `CHANGES.md` from commits since the last tag:

```bash
# Preview first (no file changes)
npm run changelog:preview

# If it looks good, write it
npm run changelog
```

**What this does:**
- Finds all commits since the last `v*` tag
- Categorizes them by prefix (`add:` → Added, `fix:` → Fixed, etc.)
- Replaces the content under `## [Unreleased]` in `CHANGES.md`

**Commit prefixes and their categories:**

| Prefix | Section |
|--------|---------|
| `add:` / `feat:` / `feature:` | ### Added |
| `fix:` / `bugfix:` | ### Fixed |
| `update:` / `refactor:` / `chore:` | ### Changed |
| `delete:` / `remove:` | ### Removed |
| `release:` | _(skipped)_ |
| _(other)_ | ### Other |

---

### 3. Review and edit the changelog

Open `CHANGES.md` and review the auto-generated entries under `## [Unreleased]`:

- **Improve wording** — the auto-generated entries are raw commit messages; rewrite them to be user-facing
- **Add bold summaries** — format entries as `- **Bold summary** — detailed description`
- **Group related changes** — combine multiple commits about the same feature into a single entry
- **Remove noise** — drop trivial commits (typo fixes, formatting, etc.) if not user-relevant
- **Verify sections** — ensure entries are in the right category (Added/Fixed/Changed/Removed)

Example of a well-formatted entry:
```markdown
### Fixed
- **Device status log spam** — "Device update: N device(s) connected" messages repeated every few seconds even when nothing changed; now only logs on actual state changes
```

If you made manual edits, **commit them**:

```bash
git add CHANGES.md
git commit -m "update: changelog for next release"
```

> **Important:** The working tree must be clean before running the release script. If you edited `CHANGES.md`, commit the edits first.

---

### 4. Run the release

Choose the appropriate bump level:

```bash
# Patch release (bug fixes, small changes): 1.8.4 → 1.8.5
npm run release:patch

# Minor release (new features, backward compatible): 1.8.4 → 1.9.0
npm run release:minor

# Major release (breaking changes): 1.8.4 → 2.0.0
npm run release:major

# Or set an explicit version:
npm run release -- 2.0.0
```

**What the release script does automatically:**

1. ✅ **Checks for clean working tree** — aborts if there are uncommitted changes
2. ✅ **Shows current branch** — so you can verify you're on `main`
3. ✅ **Runs all tests** — `npm run test:all` (frontend + Rust); aborts on failure
4. ✅ **Bumps version** — updates all 5 version files (`package.json`, `package-lock.json`, `tauri.conf.json`, `Cargo.toml`, `Cargo.lock`)
5. ✅ **Checks tag doesn't exist** — refuses if `v<version>` tag already exists
6. ✅ **Extracts release notes** — reads `[Unreleased]` section from `CHANGES.md` (falls back to `[x.y.z]` section, then auto-generates from git log)
7. ✅ **Promotes changelog** — renames `[Unreleased]` → `[x.y.z] — YYYY-MM-DD` in `CHANGES.md`
8. ✅ **Writes `.release-notes.md`** — used by CI for the GitHub Release body
9. ✅ **Commits** — `release: v<version>` with all changed files
10. ✅ **Tags** — creates `v<version>` tag
11. ✅ **Pushes** — pushes the commit and tag to `origin`

**To skip tests** (not recommended):
```bash
npm run release:patch -- --skip-tests
```

---

### 5. Monitor the CI build

After the push, the GitHub Actions workflow triggers automatically:

1. Check progress: https://github.com/havokentity/android-application-installer/actions
2. The workflow builds for:
   - macOS ARM64 (Apple Silicon)
   - macOS x64 (Intel)
   - Windows x64
   - Linux x64
3. On success, it creates a **GitHub Release draft** with all platform artifacts
4. The `update-updater-json` job then:
   - Fetches `.sig` files from the release
   - Writes `updater.json`
   - Commits `updater.json` to `main`

> **Build time:** Typically 15–25 minutes for all platforms.

---

### 6. Publish the GitHub Release

Once CI completes and the draft release has all artifacts:

**Option A — CLI (recommended):**
```bash
npm run release:publish
```
This finds the latest draft release and publishes it. To publish a specific tag:
```bash
npm run release:publish -- v1.8.5
```

**Option B — GitHub web UI:**
1. Go to https://github.com/havokentity/android-application-installer/releases
2. Find the draft release
3. Review the release notes and artifacts
4. Click **Publish release**

---

### 7. Pull the `updater.json` commit

After CI completes, it commits an updated `updater.json` to `main`. Pull it locally:

```bash
git pull origin main
```

---

## Post-Release Checklist

- [ ] CI build completed successfully for all platforms
- [ ] GitHub Release published (not draft)
- [ ] All platform artifacts present (macOS ×2, Windows ×3, Linux ×2)
- [ ] `updater.json` updated on `main` (CI does this automatically)
- [ ] Pulled latest `main` locally (`git pull`)
- [ ] Update documentation if needed (see `docs/updating-docs.md`)

---

## Choosing the Release Level

| Level | When to use | Example |
|-------|------------|---------|
| **patch** | Bug fixes, small improvements, no new features | `1.8.4 → 1.8.5` |
| **minor** | New features (backward compatible), significant improvements | `1.8.4 → 1.9.0` |
| **major** | Breaking changes, major redesigns | `1.8.4 → 2.0.0` |

---

## Troubleshooting

### "Working tree is dirty"

```
✗  Working tree is dirty. Commit or stash your changes first.
```

Commit or stash all changes before releasing:
```bash
git add -A && git commit -m "update: pre-release changes"
# or
git stash
```

### "Tag already exists"

```
✗  Tag v1.8.5 already exists. Choose a different version.
```

The version was already released. Use a higher version number.

### "Tests failed"

```
✗  Tests failed. Fix them before releasing.
```

Fix failing tests, commit the fixes, then re-run the release. Or skip tests (not recommended):
```bash
npm run release:patch -- --skip-tests
```

### "Refusing to downgrade"

```
✗  Refusing to downgrade: 1.7.0 < 1.8.4 (current highest).
```

The bump script won't set a version lower than the current highest. Use a version ≥ the current version.

### CI build fails

1. Check the [Actions tab](https://github.com/havokentity/android-application-installer/actions) for error details
2. Fix the issue, then either:
   - Delete the tag and re-release:
     ```bash
     git tag -d v1.8.5
     git push origin :refs/tags/v1.8.5
     # fix the issue, commit
     npm run release -- 1.8.5
     ```
   - Or release a new patch version with the fix

### Draft release has no artifacts

The CI build may still be running. Wait for all build jobs to complete before publishing.

### `updater.json` not updated

The `update-updater-json` CI job only runs on tag pushes. If it failed:
1. Check the CI logs
2. You can manually run: `GITHUB_TOKEN=<token> node scripts/generate-updater-json.mjs v1.8.5`
3. Commit and push `updater.json` to `main`

---

## Complete Example: Patch Release from a Feature Branch

```bash
# 1. Switch to main and merge feature branch
git checkout main
git pull origin main
git merge feature/qrCode-pairing
git push origin main

# 2. Generate changelog
npm run changelog:preview           # inspect what will be generated
npm run changelog                   # write to CHANGES.md

# 3. Review and polish changelog entries
# (edit CHANGES.md manually if needed)
git add CHANGES.md
git commit -m "update: changelog for v1.8.5"

# 4. Release
npm run release:patch               # tests → bump → commit → tag → push

# 5. Wait for CI (~20 min), then publish
npm run release:publish

# 6. Pull the updater.json commit from CI
git pull origin main
```

