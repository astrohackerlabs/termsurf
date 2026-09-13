# Homebrew

Issue 26091322413429 Experiment 1 prepares the shared
`astrohackerlabs/astrohacker` tap for NuTorch 2.0.4 and TermSurf 0.3.25.
The instructions below describe that candidate. Publication and fresh-install
qualification are pending; do not deploy these instructions before qualification.
See [shared-tap rollout](homebrew-shared-tap.md) for ordering and migration.

The next release uses Homebrew-installed NuTorch as the default shell. The cask
requires `astrohackerlabs/astrohacker/nutorch` without a version constraint; the
TermSurf archive contains no shell or LibTorch copy. Installed qualification is
pending in Issue 26091215074416 Experiment 7. Current supported installation is
Apple silicon on macOS Tahoe 26.x, matching the NuTorch binary distribution.

**Canonical user install and operator release documentation** for Astrohacker
on Apple silicon macOS. Shell / direct install (`install.sh`) is **retired** as
a product channel; do not revive public bootstrap install as the primary path.

Full environment variable taxonomy: [`docs/environment.md`](./environment.md).

Astrohacker ships to macOS through the `astrohackerlabs/astrohacker` Homebrew tap.
There is **one desktop download**: the cask `termsurf`. It installs
Astrohacker TermSurf, Web, and related helpers, with NuTorch installed as a formula
dependency. The app lands in **`/Applications/Astrohacker TermSurf.app`**.

## Public command surface

Released PATH names (machine-readable for gates):

<!-- released-wrappers -->
ahterm
ahweb
ahcalc
ahebx
ahnexus
ah-chromiumd
<!-- /released-wrappers -->

Released payload roots (machine-readable for legal/notice gates; top-level
paths in the release tarball besides bare CLI binaries):

<!-- released-payload-roots -->
ahcalc
ahebx
ahnexus
ah-chromiumd
<!-- /released-payload-roots -->

| Command | Role |
| --- | --- |
| `ahterm` | Astrohacker TermSurf (app executable + PATH launcher) |
| `nutorch` | NuTorch shell, supplied by the separate Homebrew dependency |
| `ahweb` | Open URLs / browser panes in Terminal |
| `ahcalc` | Scientific calculator TermSurf app (full-pane web UI) |
| `ahebx` | EarthBucks miner TermSurf app (full-pane web UI) |
| `ahnexus` | Nexus TermSurf chat shell (full-pane web UI + Rust server) |

Reserved (not shipping until the product ships): `ahwallet`.

There is **no** `ah` / `astrohacker` meta CLI dispatcher today.

Engine helpers (implementation; on PATH for packaging/debug):

- `ah-chromiumd`

**Not released:** `ah-webkitd` / webkit (archived; Issue 26072120115614).
Gecko / `ah-geckod` was never a shipped product engine (Issue 26072121272459).

Engine **selectors** for `ahweb`: family name `chromium` is the shipped
product engine. Historical names such as `webkit` are not shipped in the
Homebrew cask.

`TermSurf` remains the **protocol** name (`termsurf.proto`, `libtermsurf_*`,
`TERMSURF_*` env). It is not the product brand and is not the PATH CLI name

Retired cask aliases are unsupported. Users install `termsurf` only. The public
GitHub source and release asset host
is `astrohackerlabs/termsurf` (local default `~/dev/termsurf`), and its Homebrew
tap is `astrohackerlabs/astrohacker` (local default `~/dev/homebrew-astrohacker`).

Astrohacker Wallet is planned for a future update of this **same** cask—not a
second formula.

## Install

```nu
brew tap astrohackerlabs/astrohacker
brew trust astrohackerlabs/astrohacker
brew install --cask termsurf
```

NuTorch is installed automatically from the same trusted tap as a dependency.
There is no separate NuTorch tap, trust or installation step for TermSurf users.

Upgrade:

```nu
brew update
brew upgrade --cask termsurf
```

## Signing model

Distribution uses ad-hoc codesign in the cask postflight (quarantine clear +
`codesign --sign -`) until Developer ID notarization is in place.
`brew trust` trusts the tap source; it does not notarize the app with Apple.

Legal files are injected into the app under `Contents/Resources/legal/` during
`scripts/release.nu` packaging (after the app is copied into the stage tree).
That changes sealed app contents relative to any earlier signature; the
**cask postflight ad-hoc re-sign is the intentional installed contract** for
those Resources until Developer ID notarization lands.

Normal install/reinstall/uninstall of Astrohacker-owned opt artifacts must not
require `sudo` (helpers are Homebrew `artifact`s).

## Installed layout

- `Astrohacker TermSurf.app` → `/Applications/Astrohacker TermSurf.app`
  (executable `Contents/MacOS/ahterm`)
- **Legal (authoritative for installed users):**
  `/Applications/Astrohacker TermSurf.app/Contents/Resources/legal/`
  (`LICENSE`, `NOTICE`, `TRADEMARKS.md`, `third_party/...`)
- PATH: `ahterm`, `ahweb`, `ahcalc`, `ahebx`,
  `ahnexus`, engine helpers
- Shell dependency: `/opt/homebrew/bin/nutorch`. With no explicit `command`,
  TermSurf selects that path (then `/usr/local/bin/nutorch`), even from a GUI
  launch without Homebrew on PATH. An explicit shell setting takes precedence;
  remove an old ahsh `command` setting to use the new default.
- Chromium tree →
  `/opt/homebrew/opt/astrohacker-terminal-ah-chromiumd/`
- ahcalc package payload →
  `/opt/homebrew/opt/astrohacker-terminal-ahcalc/` (when installed as artifact)
  or under Caskroom stage `ahcalc/` (binary links `ahcalc/dist/ahcalc`)
- ahebx package payload →
  `/opt/homebrew/opt/astrohacker-terminal-ahebx/` (when installed as artifact)
  or under Caskroom stage `ahebx/` (binary links `ahebx/dist/ahebx`)
- ahnexus package payload →
  `/opt/homebrew/opt/astrohacker-terminal-ahnexus/` (when installed as artifact)
  or under Caskroom stage `ahnexus/` (binary links `ahnexus/ahnexus`; SPA in
  `ahnexus/ui/`)
## Release tarball contract

Asset name: `astrohacker-<version>-aarch64-apple-darwin.tar.gz`

Top-level contents:

- `Astrohacker TermSurf.app/` (with `Contents/MacOS/ahterm` and
  `Contents/Resources/legal/`)
- `LICENSE`, `NOTICE`, `TRADEMARKS.md` (tarball root mirror of product legal)
- `legal/third_party/` (Chromium credits/LICENSE
  copyrights, Nushell/Reedline LICENSE copies)
- `ahweb`
- `ahcalc/` (payload: `dist/ahcalc`, `build/client/` SPA, `public/`)
- `ahebx/` (payload: `dist/ahebx`, `build/client/` SPA, `public/`)
- `ahnexus/` (payload: `ahnexus` binary + `ui/` Vite SPA)
- `ah-chromiumd/`

Gate before publish: `scripts/check-release-legal-notices.nu` (NOTICE
legal-manifest vs released wrappers + payload roots).

## Release / publish (agents and humans)

Canonical three-repository Homebrew release flow. Packaging scripts live in the
**private** monorepo; they are not synced to the public source tree.

### Topology

| Role | Local default | GitHub |
| --- | --- | --- |
| Private monorepo | this repo | private business monorepo |
| Public TermSurf source | `~/dev/termsurf` | `astrohackerlabs/termsurf` |
| Homebrew tap | `~/dev/homebrew-astrohacker` | `astrohackerlabs/homebrew-astrohacker` |

Cask file: `~/dev/homebrew-astrohacker/Casks/termsurf.rb`

Env overrides: `ASTROHACKER_TERMINAL_PUBLIC_REPO`,
`ASTROHACKER_TERMINAL_PUBLIC_GITHUB_REPO`,
`ASTROHACKER_TERMINAL_HOMEBREW_TAP_REPO` (legacy `TERMSURF_*` aliases still
accepted by scripts).

### Scripts

| Script | Role |
| --- | --- |
| `scripts/release-termsurf.nu` | Canonical fork verification, incremental release build, package, publish, and local cask installation transaction |
| `scripts/lib/release_forks.nu` | Enforce `patches/release-manifest.json` |
| `scripts/build.nu` | Build components / `all --release` |
| `scripts/release.nu` | Lower-level package/publish helper used by the canonical command |
| `scripts/sync-public-source.nu` | Sync allowlisted paths into public checkout |

### Canonical release command

The human release operator runs one command from the clean private monorepo:

```nu
scripts/release-termsurf.nu
```

With no option it selects the next patch version after the greatest strict
version found across public releases, public tags, and the remote cask. To
select a higher unused version:

```nu
scripts/release-termsurf.nu --version 0.4.0
```

#### Build cache preservation

“Clean private monorepo” means a clean Git worktree, not an empty build
directory. Release builds are incremental by default. `--release` selects
release-mode artifacts and must never imply `--clean`; normal dependency
tracking rebuilds only the inputs that changed.

Preserve valid outputs and caches for every shipped project, especially:

- Chromium `forks/chromium/src/out/Default`;
- Ghostty Zig and Xcode outputs under `forks/ghostty`;
- Rust/Cargo target directories; and

Never remove these for a routine release. A clean is allowed only after a stale
or corrupt artifact is diagnosed, must be scoped to that component, and
requires an explicit rebuild-cost warning and user approval before deletion.
The approval to destroy build state is separate from the operator's publication
confirmation.

If an external step fails after packaging, the command records the exact
version, archive, and digest under ignored `dist/` state. Running the command
again resumes those same bytes; `--resume X.Y.Z` may state the expected saved
version explicitly. It never invents another recovery version.

Before confirmation the command performs read-only version, repository, tool,
and credential discovery. After the operator types the exact confirmation, it:

1. sets first-party product Cargo package versions to the selected release
   version (`ahweb`, `ah-chromiumd`), refreshes their `Cargo.lock`
   files, commits that bump on private `main` when needed, and pushes it so
   the monorepo stays aligned with `origin/main`. This step never rewrites
   anything under `forks/` (fork trees are out of scope; `ahterm` still gets
   the release stamp from `ASTROHACKER_VERSION` / `TERMSURF_VERSION`);
2. proves or reconstructs all released fork inputs from the tracked cumulative
   patch manifest (Ghostty, Nushell, Reedline, Chromium; WebKit/Gecko historical
   archives are excluded from ship; editor fork excluded);
3. incrementally builds every shipped component in release mode with one
   version while preserving valid build outputs and caches;
4. packages one archive and freezes its SHA-256;
5. syncs and pushes the allowlisted public source when it changed;
6. creates or safely resumes the matching tag, GitHub release asset, and cask
   without deleting or overwriting a conflict;
7. refreshes Homebrew and installs or reinstalls the published cask; and
8. prints the exact release identities and asks the operator to test the app
   manually.

The release command runs no product tests, smokes, browser checks, screenshots,
or UI automation. Publication and product qualification are separate. Agents
may implement or review the command, but the human operator owns its publishing
confirmation and the resulting app acceptance.

### Lower-level helpers

Flags for `scripts/release.nu <version>`:

- Package only (default if publish unset):
  `ASTROHACKER_TERMINAL_RELEASE_PACKAGE_ONLY=1`
  or simply omit `ASTROHACKER_TERMINAL_RELEASE_PUBLISH=1`
- Publish:
  `ASTROHACKER_TERMINAL_RELEASE_PUBLISH=1`
- Publish an archive already created by package-only mode, without restaging or
  retarring it:
  `ASTROHACKER_TERMINAL_RELEASE_USE_EXISTING_PACKAGE=1` plus
  `ASTROHACKER_TERMINAL_RELEASE_EXPECTED_SHA256=<sha256>`
- The canonical command sets
  `ASTROHACKER_TERMINAL_RELEASE_SKIP_PRODUCT_QUALIFICATION=1`; lower-level
  resource-root smokes while retaining artifact-presence, dependency,
  topology, and legal-integrity assertions needed to construct the archive.

Publish mode requires **clean** public and tap worktrees. It only rewrites cask
`version` and `sha256`. Commit any binary/postflight content changes on the tap
**before** publish mode.

### Lower-level manual flow

The following describes the components orchestrated by
`scripts/release-termsurf.nu`. It is recovery/reference material, not the
normal operator interface.

1. **Preflight version** (remote-facing):

   ```nu
   gh release list --repo astrohackerlabs/termsurf --limit 5
   git -C ~/dev/termsurf ls-remote origin 'refs/heads/main' 'refs/tags/v*'
   git -C ~/dev/homebrew-astrohacker fetch origin
   git -C ~/dev/homebrew-astrohacker show origin/main:Casks/termsurf.rb | rg 'version |sha256 '
   ```

   Choose next version from max(public release, tag, remote cask).

2. **Land product changes** in private monorepo; push tap **content** changes
   (not version/sha) if needed so the tap is clean for publish.

3. **Full release build** (`scripts/build.nu termsurf` ships Terminal components
   only — no editor; preserve all valid incremental build outputs):

   ```sh
   TERMSURF_VERSION=<version> \
   ASTROHACKER_VERSION=<version> \
     scripts/build.nu termsurf --release
   ```

   Version contract:

   - First-party product crate package versions under the monorepo root track the
     Homebrew release version (`ahweb`, `ah-chromiumd`, `ahnexus`). The
     canonical command rewrites and commits those manifests before building so
     `CARGO_PKG_VERSION` matches the cask. It also rewrites `code/termsurf/ts/ahcalc`,
     and `code/earthbucks/ts/ahebx` `package.json` `"version"` and
     their `app/cli/embedded-version.ts` stamps to the same X.Y.Z so
     compile-time stamps match the cask and the post-build tree stays clean.
     Do not leave those packages stuck at a placeholder such as `0.1.0` across
     releases. Do not rewrite package versions under `forks/`.
   - `TERMSURF_VERSION=<version>` is the `ahterm` app/helper version input.
     `ahterm` is the only shipped wrapper that uses the terminal helper/action
     convention: `ahterm +version` and `ahterm +help`.
   - `ASTROHACKER_VERSION=<version>` is the release version input for Rust
     product/helper binaries (first-party crate versions are also aligned to
     the same release).
   - Every shipped non-`ahterm` wrapper must support `--version` and `--help`.
     The first `--version` line must use the same `<version>`:

     | Wrapper | Expected first line |
     | --- | --- |
     | `ahweb --version` | `Astrohacker Web <version>` |
     | `ahcalc --version` | `Astrohacker Calculator <version>` |
     | `ahebx --version` | `Astrohacker EarthBucks <version>` |
     | `ahnexus --version` | `Astrohacker Nexus <version>` |
     | `ah-chromiumd --version` | `Astrohacker Chromium Engine <version>` |

     Runtime/component versions, such as Nushell or browser ABI versions, may be
     shown only as secondary detail after the product release line.

4. **Package-only**:

   ```sh
   ASTROHACKER_TERMINAL_RELEASE_PACKAGE_ONLY=1 \
   ASTROHACKER_TERMINAL_RELEASE_PUBLISH=0 \
     scripts/release.nu <version>
   ```

   Inspect `dist/release` and
   `dist/astrohacker-<version>-aarch64-apple-darwin.tar.gz`.

5. **Public source sync** (private monorepo → public checkout), then commit on
   public `main` so the tree is clean:

   ```sh
   scripts/sync-public-source.nu
   # commit in ~/dev/termsurf
   ```

   This step keeps the **public README** and **story screenshots** current:
   `docs/public-source/README-template.md` becomes public `README.md`, and
   allowlisted `assets/screenshots/story/*.webp` ship with the public tree.
   After regenerating story WebPs in the private monorepo
   (`shot-gallery/out/` → refresh `assets/screenshots/story/`), re-run this
   sync (or the next Homebrew release, which includes the same step) so
   github.com/astrohackerlabs/termsurf does not lag.

6. **Publish** (a direct helper invocation repackages; the canonical command
   instead uses the exact existing-package mode documented above):

   ```sh
   ASTROHACKER_TERMINAL_RELEASE_PUBLISH=1 scripts/release.nu <version>
   ```

   Creates/pushes `v<version>`, GitHub release asset, tap commit `v<version>`
   with authoritative SHA256.

### Product qualification (separate)

The following historical/active harnesses may be useful in an issue whose goal
is product qualification. They are not publication gates and are never invoked
by `scripts/release-termsurf.nu`:

| Script | Role |
| --- | --- |
| `scripts/test-issue-26062812000869-installed-homebrew-browser-smoke.sh` | installed three-engine browser smoke |
| `scripts/test-issue-26070112000882-installed-cold-start.sh` | cold-start + warmup |
| `scripts/test-issue-26062812000867-release-no-env-browser-discovery.sh` | useful discovery check |
| Older Surfari-named 871/872 harnesses | historical; not current gates until updated |

Example:

```sh
ASTROHACKER_TERMINAL_SMOKE_VERSION=<version> \
  scripts/test-issue-26062812000869-installed-homebrew-browser-smoke.sh
```

### Traps

- Dirty tap or public repo aborts publish mode.
- The canonical command packages once and requires the identical archive SHA in
  publish-existing mode. A direct lower-level publish may repackage instead.
- Partial publish: inspect tag/asset/tap; rerun same version; do not invent a
  new version just to recover.
  incremental dependency graph. Do not use `--clean` for a routine release.
- Do not add alternate or compatibility cask tokens.

### Agent checklist

1. Keep the private monorepo, public source, and tap clean and pushed.
2. Keep `patches/release-manifest.json` current whenever a released fork patch
   series changes.
3. Do not invoke the publishing command; hand it to the human release operator.
4. Do not add product qualification to the release transaction.
5. Preserve valid build outputs and caches; never turn release mode into an
   implicit clean build.

## Independent NuTorch distribution

The 0.3.24 repair below is historical. New publications use
`~/dev/homebrew-astrohacker`, with product-specific `termsurf-vX.Y.Z` tap tags.
Existing version tags remain unchanged in the old tap.

For the failed 0.3.24 installation, Issue 26091215074416 Experiment 8 prepares a
tap-only correction removing two obsolete ahsh postflight hooks. Keep the existing
0.3.24 archive, URL, checksum and tags. Ryan publishes the reviewed tap commit with
`git -C ~/dev/homebrew-termsurf push origin main`, then runs `brew update` and
retries installation. Do not rerun the full publisher to publish this correction.
The observed rollback restored 0.3.23, so the next consumer command is
`brew upgrade --cask termsurf`; if a later attempt records 0.3.24 as installed,
use `brew reinstall --cask termsurf`. Inspect current state before choosing.
No manual installed-tap or Caskroom edits are part of this repair.

The former ahtch component is retired from TermSurf source and packaging.
NuTorch is the independently installed shell dependency. An upgrade uses the old
cask's own uninstall artifacts to
remove its old ahtch links/payload; do not manually delete independent NuTorch
files, configuration or caches as part of that migration.

NuTorch has its own source repository, Homebrew formula, version and release
transaction. It is not added to the TermSurf payload lists above. Its source
checkout is `~/dev/nutorch` (`astrohackerlabs/nutorch`); its tap checkout is
`~/dev/homebrew-astrohacker` (`astrohackerlabs/homebrew-astrohacker`), with formula
`Formula/nutorch.rb`.

The human-operated publisher is `scripts/release-nutorch.nu`, with
`--dry-run`, optional `--version X.Y.Z`, and `--resume X.Y.Z`. It publishes
source and a prebuilt binary with bundled LibTorch without invoking Homebrew or
changing the installed environment. Installation follows publication as a separate
human action. Setup and confirmation are documented in
[NuTorch release](nutorch/release.md). Ryan accepted the corrected 2.0.2 binary
release and installation. TermSurf publication neither builds nor bumps NuTorch.

```nu
brew tap astrohackerlabs/astrohacker
brew trust astrohackerlabs/astrohacker
brew install astrohackerlabs/astrohacker/nutorch
```

The formula provides the `nutorch` shell and its runtime libraries/licenses.
Run `use torch` inside NuTorch for native tensor commands; there is no daemon or
external Nu module to configure. NuTorch itself does not require TermSurf or a
development checkout. Agents must not confirm publication.

## Installed smoke expectations

TermPlot is the renamed ahplt implementation and is becoming a separately
versioned Homebrew formula under Issue 26090715196587 Experiment 4. It still
requires an installed Astrohacker TermSurf runtime for plotting, but neither
`ahplt` nor `termplot` belongs in the TermSurf cask or archive. TermSurf releases
must not change TermPlot's package version. The preceding cask owns removal of
its bundled ahplt during upgrade; no migration cleanup may remove independent
TermPlot files. Publication and installed-upgrade qualification are pending.

After install, from inside Astrohacker TermSurf:

- `ahweb --browser chromium https://example.com`

Helpers resolve under `/opt/homebrew/opt/astrohacker-terminal-ah-*` without
browser path env overrides.

## Engine path environment variables

Primary product overrides (preferred):

- `ASTROHACKER_CHROMIUM_PATH`

Legacy dual-read aliases (deprecated; still accepted):

- `TERMSURF_ROAMIUM_PATH` / `TERMSURF_INSTALLED_ROAMIUM_PATH`

Values must be nonempty absolute paths. Protocol vars such as `TERMSURF_SOCKET`
and `TERMSURF_PANE_ID` are unchanged.
