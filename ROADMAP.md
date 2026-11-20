# mpkg Roadmap

## Phase 3: Dependency Resolution ✅
- Semver constraints
- Virtual packages (`provides`)
- Upgrade workflows
- Lockfile write
- Orphan detection

## Phase 4: CLI Polish ✅
- Human-readable, colored output
- `--json` mode for GUI embedding
- Progress reporting (`on_download`, `on_install`)
- Command parity (`install`, `upgrade`, `remove`, `autoremove`, `lock`, `list`)
- `mpkg --version`, `mpkg --help`

## Phase 5: Integration & Packaging 🚧
- Embed into Ferrite/CraftOS
- Maintainer scripts (`pre/post-install`)
- Trigger hooks (file triggers, config changes)
- Full test suite in CI
- Publish to crates.io

---
*Last updated: 2026-11-21*