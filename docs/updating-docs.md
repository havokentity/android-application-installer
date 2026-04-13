# Updating Documentation

Guide for keeping the project's documentation in sync with code changes.

---

## When to Update Docs

Update documentation **before releasing**, ideally as part of the feature branch or as a final step before merging to `main`. The release process does NOT auto-update any docs other than `CHANGES.md`.

---

## Documentation Files

| File | Purpose | When to update |
|------|---------|---------------|
| `CHANGES.md` | Version changelog | Every release (auto-populated by `npm run changelog`, promoted by release script) |
| `docs/architecture.md` | Internal architecture reference | New commands, modules, hooks, components, or architectural changes |
| `docs/feature-analysis.md` | Feature tracking checklist | New features added or completed |
| `docs/wireless-adb-guide.md` | Wireless ADB user guide | Changes to wireless ADB functionality |
| `README.md` | Public-facing project overview | New features, UI changes, screenshots, download info |
| `docs/agent.md` | AI agent context document | Structural changes, new conventions, new scripts |
| `docs/release-guide.md` | Release process instructions | Changes to release scripts or workflow |
| `docs/updating-docs.md` | This file | Changes to documentation structure |

---

## File-by-File Update Checklist

### CHANGES.md

**Updated:** Every release (mostly automated).

The changelog is auto-populated from git commits and promoted during the release process. Manual polishing is recommended — see `docs/release-guide.md` step 3.

**Format rules:**
- Entries under `## [Unreleased]` (auto-populated by `npm run changelog`)
- Use `### Added`, `### Fixed`, `### Changed`, `### Removed` sub-headings
- Format: `- **Bold summary** — detailed description`
- Version sections separated by `---`

You do NOT need to manually write entries if you use prefixed commits (`add:`, `fix:`, `update:`, etc.) — the changelog script does it for you.

---

### docs/architecture.md

**Updated when:** Adding/removing Tauri commands, Rust modules, React components, hooks, changing state management patterns, or modifying the project structure.

#### Sections to check:

1. **Source Layout** (the directory tree)
   - [ ] New files/modules listed
   - [ ] Removed files cleaned up
   - [ ] File descriptions accurate

2. **Key Design Decisions**
   - [ ] New architectural patterns documented
   - [ ] Existing pattern descriptions still accurate

3. **Frontend State Management** (hooks table)
   - [ ] New hooks added to the table
   - [ ] Hook domain descriptions accurate

4. **Tauri Commands (IPC)** table
   - [ ] New commands added with file location and purpose
   - [ ] Removed commands cleaned up

5. **Auto-Updater** section
   - [ ] Still accurate if updater logic changed

6. **CI / CD** section
   - [ ] Reflects current workflow

7. **Version Management** section
   - [ ] Version file list correct
   - [ ] Script commands accurate

8. **Developer Scripts** table
   - [ ] New scripts added
   - [ ] Removed scripts cleaned up

9. **Data Directory** section
   - [ ] New data files listed (e.g., new JSON configs)

---

### docs/feature-analysis.md

**Updated when:** Completing a pending feature, adding new features to the roadmap, or completing code quality / UX / architecture improvements.

#### How to update:

1. **Mark completed items** — change `[ ]` to `[x]` and add a description
2. **Add new pending items** — add `[ ]` items under the right category
3. **Update the summary table** at the bottom — update the "Done" and "Remaining" counts for each category and the totals row
4. Keep descriptions concise but informative — mention the key components involved

---

### README.md

**Updated when:** Adding user-facing features, changing the UI significantly, updating downloads, or changing the project structure.

#### Sections to check:

1. **Features** section
   - [ ] New features listed under the correct sub-heading (Core, Tools & Setup, Interface, Cross-Platform)
   - [ ] Feature descriptions are user-facing (not technical)

2. **Screenshots** section
   - [ ] Screenshots reflect the current UI
   - [ ] Take new screenshots if the UI layout changed significantly
   - [ ] Include both dark and light themes, landscape and portrait modes
   - [ ] Screenshots go in the `screenshots/` folder

3. **Downloads** section
   - [ ] Platform table is accurate
   - [ ] macOS instructions still correct

4. **Getting Started** section
   - [ ] Prerequisites still accurate
   - [ ] Build commands still correct

5. **How It Works** section
   - [ ] Managed tools table accurate
   - [ ] AAB flow diagram still correct

6. **Project Structure** (text version inside `<details>`)
   - [ ] Directory tree matches actual structure
   - [ ] New files listed, removed files cleaned up

7. **CI / CD** section
   - [ ] Release commands accurate
   - [ ] macOS code signing instructions correct
   - [ ] Auto-updater signing instructions correct

8. **Tech Stack** table
   - [ ] Libraries and versions accurate

---

### docs/wireless-adb-guide.md

**Updated when:** Changes to wireless ADB pairing, connection, disconnection, or mDNS discovery.

#### Sections to check:

1. **Prerequisites** — ADB version requirements, Android version
2. **Quick Start** — step-by-step instructions still accurate
3. **Network Discovery** — scan behavior
4. **Tips & Troubleshooting** — add new common issues and solutions
5. **Architecture** diagram — file references, command flow
6. **Test coverage** numbers

---

### docs/agent.md

**Updated when:** Structural project changes, new conventions, new scripts, or significant architecture changes.

#### Sections to check:

1. **Repository Structure** tree
2. **Version Files** table
3. **npm Scripts Reference** tables
4. **Commit Message Conventions**
5. **Branch Conventions**
6. **Architecture Quick Reference** (hooks table, Rust modules table)
7. **CI / CD Pipeline**
8. **Key Files to Know** reference table

---

### SVG Diagrams (`docs/diagrams/`)

**Updated when:** The project structure, AAB flow, or UI layout changes significantly.

Each diagram has a light and dark variant:
- `project-structure-dark.svg` / `project-structure-light.svg`
- `aab-flow-dark.svg` / `aab-flow-light.svg`
- `ui-layout-dark.svg` / `ui-layout-light.svg`

If you update a diagram, update **both** light and dark variants.

---

## Workflow: Updating Docs with a Feature

When implementing a feature:

1. **During development** — update `CHANGES.md` entries under `[Unreleased]` as you go (or rely on `npm run changelog` later)
2. **Before merging to `main`** — update:
   - `docs/architecture.md` (new commands, modules, hooks, components)
   - `docs/feature-analysis.md` (mark items done, add new items)
   - `README.md` (new user-facing features)
   - Feature-specific docs (e.g., `wireless-adb-guide.md`)
3. **During release** — the release script handles `CHANGES.md` promotion automatically
4. **After release** — update screenshots if the UI changed

### Commit convention for docs:

```bash
git commit -m "update: docs for <feature-name>"
# or
git commit -m "update: architecture.md — added <new-module>"
```

---

## Test Count Tracking

Several docs reference test counts. Update these when adding tests:

| Location | What it tracks |
|----------|---------------|
| `docs/feature-analysis.md` | References test counts in feature descriptions |
| `docs/wireless-adb-guide.md` | Test coverage section at the bottom |
| `CHANGES.md` | Test counts mentioned in release entries (e.g., "total: 399") |

To get current counts:
```bash
# Frontend test count
npm test 2>&1 | Select-String "Tests"

# Rust test count
cd src-tauri; cargo test 2>&1 | Select-String "test result"
```


