+++
status = "closed"
opened = "2026-02-27"
closed = "2026-02-27"
+++

# Issue 656: Rename Script

Reproducible script to rename all "ghostty" references in `gui/` to "termsurf."
Re-runnable after upstream Ghostty merges.

## Problem

After Issues 611 and 613, many "ghostty" references were selectively renamed.
But ~7,300+ references remain throughout `gui/` — C API names (`GHOSTTY_*`),
header guards, env vars, file names, build system, shell integration,
translations, Swift types, GTK identifiers, etc.

## Approach

A bash script at `rename-ghostty.sh` (repo root) using a protect → substitute →
restore → file-rename → verify pipeline.

### How it works

1. **Protect** — Replace patterns that must stay as "ghostty" with unique
   placeholders (`__PROTECT_01__` through `__PROTECT_19__`)
2. **Substitute** — Seven ordered substitutions: bundle IDs, D-Bus paths,
   domain, `GHOSTTY_` prefix, `GHOSTTY` standalone, `Ghostty`, `ghostty`
3. **Restore** — Replace placeholders back to original ghostty text
4. **File renames** — `git mv` all files/directories with "ghostty" in their
   name (idempotent, skips if source doesn't exist or isn't tracked)
5. **Verify** — Report remaining "ghostty" references and check for leftover
   placeholders

### Protected patterns (NOT renamed)

| #   | Pattern                             | Reason                      |
| --- | ----------------------------------- | --------------------------- |
| 01  | `ghostty-themes`                    | Upstream theme package      |
| 02  | `ghostty-org/ghostty`               | Upstream GitHub repo path   |
| 03  | `mitchellh/ghostty`                 | Upstream author repo path   |
| 04  | `ghostty-org`                       | Upstream GitHub org         |
| 05  | `deps.files.ghostty.org`            | Upstream CDN                |
| 06  | `release.files.ghostty.org`         | Upstream CDN                |
| 07  | `tip.files.ghostty.org`             | Upstream CDN                |
| 08  | `ghostty.cachix.org`                | Upstream Nix cache          |
| 09  | `discord.gg/ghostty`                | Upstream Discord            |
| 10  | `snapcraft.io/ghostty`              | Upstream Snap store         |
| 11  | `Ghostty contributors`              | Copyright attribution       |
| 12  | `namespace-profile-ghostty`         | CI runner names             |
| 13  | `config.ghostty`                    | Config file extension       |
| 14  | `theme.ghostty`                     | Theme file extension        |
| 15  | `.ghosttycrash`                     | Crash file extension        |
| 16  | `*.ghostty`                         | Vim filetype detection glob |
| 17  | `appendingPathExtension("ghostty")` | Swift config extension      |
| 18  | `ghostty.qcow2`                     | VM disk image name          |
| 19  | `.ghostty.png`                      | Test screenshot suffix      |

### Text substitutions (in order)

1. `com.mitchellh.ghostty` → `com.termsurf`
2. `/com/mitchellh/ghostty` → `/com/termsurf`
3. `ghostty.org` → `termsurf.com`
4. `GHOSTTY_` → `TERMSURF_`
5. `GHOSTTY` → `TERMSURF`
6. `Ghostty` → `TermSurf`
7. `ghostty` → `termsurf`

### File renames (~70 files + ~10 directories)

See `rename-ghostty.sh` Phase 4 for the full list.

## Experiment 1: Initial script

### Hypothesis

A single sed pass (protect → substitute → restore) followed by `git mv` renames
will correctly rename all ~7,300 references while preserving the ~19 protected
patterns.

### Test

```bash
./rename-ghostty.sh
git grep -i ghostty gui/   # all matches should be protected
cd gui && zig build         # should compile
```

### Result

Pass. First run missed `GHOSTTY` (all-caps without trailing underscore) — 14
references in `@GHOSTTY@` template vars, `ID_ICON_GHOSTTY`, man page headers,
etc. Added `GHOSTTY` → `TERMSURF` rule after `GHOSTTY_` → `TERMSURF_`. Second
run caught all remaining.

Also required fixing `build.zig.zon` fingerprint (changed from
`0x64407a2a0b4147e5` to `0x219646a7bd32ceea` because package name changed from
`ghostty` to `termsurf`).

Final state:

- 249 + 147 = 396 files processed (two passes)
- 70 file/directory renames via `git mv`
- 552 remaining references — all protected patterns
- 1 binary match (`macos.dmp` crash dump) — expected
- Build: `zig build` succeeds
- App: `TermSurf-Debug.app` runs

## Conclusion

`rename-ghostty.sh` works. Re-runnable after upstream merges.
