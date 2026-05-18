+++
status = "open"
opened = "2026-05-18"
+++

# Issue 781: Chromium Upgrade Path

## Goal

Update Roamium to use the latest viable Chromium version while preserving the
ability to build and run Chromium, Roamium, and Wezboard together.

The issue is successful when Roamium builds and works against the latest
Chromium target selected by this issue, with a clear record of any upstream
versions skipped or rejected.

## Background

TermSurf currently tracks Chromium `146.0.7650.0`. The documented active branch
is `146.0.7650.0-issue-762`, based on `146.0.7650.0`.

The Chromium fork is expensive to rebuild, and TermSurf carries custom browser
embedding changes for Roamium. A blind jump to the newest Chromium tag could
waste time if several upstream changes break the build or invalidate the current
embedding API. At the same time, upgrading through every obsolete intermediate
version would also waste time.

As of 2026-05-18, Electron's release feed shows these relevant Chromium
versions:

- Stable Electron 42.1.0: Chromium `148.0.7778.97`
- Prerelease/nightly Electron 43/44: Chromium `150.0.7834.0`

TermSurf should use these as initial landmarks, then verify the actual Chromium
tags available in `chromium/src` before doing any migration work.

## Plan

Start by proving the current tree still builds. The baseline must include:

- `scripts/build.sh chromium`
- `scripts/build.sh roamium`
- `scripts/build.sh wezboard`

After the baseline is known, inspect the Chromium versions between
`146.0.7650.0` and the latest available target. For each serious candidate,
assess:

- whether the tag exists and syncs cleanly
- whether TermSurf's patch set applies cleanly
- whether Chromium's build files, Content API, embedding hooks, or macOS
  framework layout changed in relevant ways
- whether Roamium's FFI boundary still matches the Chromium library
- whether Wezboard still launches and talks to Roamium over the TermSurf socket
  protocol

Prefer the fewest migration steps that still preserve debuggability. If the
latest Chromium target is close enough to the current base, attempt a direct
upgrade. If the jump is too large, use major-version checkpoints rather than
every patch release.

## Candidate Upgrade Path

The likely path is:

1. Verify current `146.0.7650.0` baseline.
2. Assess a direct move to the latest stable Electron Chromium, currently
   `148.0.7778.97`.
3. Assess whether moving beyond stable to the latest prerelease/nightly Chromium
   is worth the extra risk.
4. If direct migration fails, bisect by Chromium major version or Electron major
   version until the first breaking point is isolated.
5. Land the newest working Chromium version with patches, documentation, and
   build instructions updated.

## Constraints

Every Chromium source change must happen on a new Chromium branch for this
issue. The branch should be named `{version}-issue-781`, based on the selected
Chromium target, and recorded in `chromium/README.md`.

Use `autoninja` for Chromium builds. Do not run `ninja` directly.

Do not delete or clean Chromium build outputs unless the issue explicitly
requires it. The build cache is valuable.

## Open Questions

- Should TermSurf track Electron stable Chromium, Chromium stable, or Chromium
  tip-of-tree for this upgrade?
- Is Roamium's current C ABI small enough to forward-port directly, or should
  the Chromium patch set be reorganized first?
- Does the current packaging layout still match newer Chromium app bundle and
  framework output?

## Experiments

### Experiment 1: Map Chromium releases since 146

#### Description

Before changing code or creating a Chromium branch, map the Chromium versions
released since TermSurf last updated Roamium's Chromium base. This experiment is
read-only discovery work. It should establish the available version landscape
and recommend the smallest sensible set of migration targets.

The current TermSurf base is `146.0.7650.0`. The mapping should start there and
cover every newer Chromium milestone relevant to this issue.

#### Changes

No code changes.

Collect version data from:

- Chromium release channels and milestone data
- Electron stable, beta, alpha, and nightly release metadata
- Chromium tags available to `chromium/src`
- TermSurf's current `chromium/README.md` branch and patch documentation

Record the findings in this issue as a table with:

- Chromium milestone
- representative Chromium version or tag
- release channel/source
- Electron version, if applicable
- whether the tag exists and is fetchable
- notes about why it is or is not a useful upgrade checkpoint

#### Verification

This experiment passes when the issue contains:

1. A table of Chromium versions newer than `146.0.7650.0`.
2. A shortlist of candidate upgrade targets.
3. A recommendation for Experiment 2.

Experiment 1 should not modify Chromium source, create a Chromium branch, or run
large builds. Its job is to choose where the migration should aim before we
spend build time.

**Result:** Pass

The current TermSurf Chromium checkout is on `146.0.7650.0-issue-762`.
`chromium/README.md` records `146.0.7650.0` as the base version. The local
Chromium checkout currently has 146-series tags, but not the newer candidate
tags. Remote tag checks against `chromium.googlesource.com/chromium/src.git`
confirmed that the candidate tags below exist.

Sources:

- Chromium Dash release data:
  `https://chromiumdash.appspot.com/fetch_releases?channel=Stable&platform=Mac&num=20`
- Chromium Dash milestone data:
  `https://chromiumdash.appspot.com/fetch_milestones?num=10`
- Electron release feed: `https://releases.electronjs.org/`
- Electron release JSON: `https://releases.electronjs.org/releases.json`

| Milestone | Representative version/tag | Source/channel                                         | Electron version            | Tag availability | Notes                                                                                                                        |
| --------- | -------------------------- | ------------------------------------------------------ | --------------------------- | ---------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| 146       | `146.0.7650.0`             | TermSurf base                                          | —                           | local            | Current Roamium base. Baseline for all comparisons.                                                                          |
| 146       | `146.0.7680.216`           | Electron stable 41.6.1                                 | 41.6.1                      | remote           | Newer 146 patch level. Useful only as a fallback if we want a low-risk patch refresh before crossing milestones.             |
| 147       | `147.0.7727.139`           | Chromium Dash stable history                           | —                           | remote           | Intermediate Chromium milestone. No current Electron stable target; use only as a checkpoint if 148 is too large.            |
| 148       | `148.0.7778.97`            | Electron stable 42.1.0                                 | 42.1.0                      | remote           | Best first real upgrade target because it matches current Electron stable.                                                   |
| 148       | `148.0.7778.168`           | Chromium Dash stable Mac                               | —                           | remote           | Newer Chromium stable patch in the same milestone. Consider after Electron-stable 148 works.                                 |
| 149       | `149.0.7827.0`             | Chromium Dash beta / Electron alpha-nightly transition | 43 alpha/nightly            | remote           | Useful checkpoint between stable 148 and prerelease 150. Not a final target unless 150 is blocked.                           |
| 150       | `150.0.7834.0`             | Electron alpha/nightly                                 | 43.0.0-alpha.3 / 44 nightly | remote           | Newest visible Electron prerelease/nightly Chromium. Highest risk; only target directly if 148 migration is straightforward. |

#### Candidate targets

1. **Primary target:** `148.0.7778.97`, matching Electron stable 42.1.0.
2. **Patch target after success:** `148.0.7778.168`, matching newer Chromium
   stable data from Chromium Dash.
3. **Checkpoint target:** `149.0.7827.0`, if moving from 148 to 150 needs an
   intermediate milestone.
4. **Stretch target:** `150.0.7834.0`, matching current Electron
   prerelease/nightly data.

#### Recommendation for Experiment 2

Experiment 2 should not start by upgrading. It should first prove the current
`146.0.7650.0` baseline still builds:

1. `scripts/build.sh chromium`
2. `scripts/build.sh roamium`
3. `scripts/build.sh wezboard`

If the baseline passes, Experiment 3 should attempt the first real migration to
`148.0.7778.97` on a new Chromium branch named `148.0.7778.97-issue-781`.

If the baseline fails, fix or document the baseline failure before attempting an
upgrade. Otherwise we will not be able to distinguish current breakage from
Chromium migration breakage.

#### Conclusion

The version landscape is small enough to avoid a long ladder of obsolete
upgrades. The first meaningful target is Electron-stable Chromium
`148.0.7778.97`. A direct jump from `146.0.7650.0` to `148.0.7778.97` is worth
trying after baseline builds pass. Keep `147.0.7727.139` and `149.0.7827.0` as
debug checkpoints, not planned migration stops.

### Experiment 2: Verify Current Baseline Builds

#### Description

Before changing Chromium versions, prove that the current `146.0.7650.0`
baseline still builds for Chromium, Roamium, and Wezboard. This isolates
existing build breakage from upgrade breakage.

#### Changes

No code changes are expected. Run the current build scripts in this order:

1. `scripts/build.sh chromium`
2. `scripts/build.sh roamium`
3. `scripts/build.sh wezboard`

Record the output paths and any failures. Do not clean Chromium output. Do not
create a Chromium branch.

#### Verification

This experiment passes when all three build commands complete successfully:

1. Chromium builds through `scripts/build.sh chromium`.
2. Roamium builds against the current Chromium output.
3. Wezboard builds its GUI and CLI targets.

If any command fails, record the failure, the relevant log or command output,
and whether Experiment 3 should fix the baseline before starting the Chromium
upgrade.

**Result:** Pass

All baseline builds completed successfully on 2026-05-18.

Commands:

1. `scripts/build.sh chromium`
2. `scripts/build.sh roamium`
3. `scripts/build.sh wezboard`

Build outputs:

- Chromium: `/Users/ryan/dev/termsurf/chromium/src/out/Default`
- Roamium: `/Users/ryan/dev/termsurf/chromium/src/out/Default/roamium`
- Wezboard GUI: `/Users/ryan/dev/termsurf/wezboard/target/debug/wezboard-gui`
- Wezboard CLI: `/Users/ryan/dev/termsurf/wezboard/target/debug/wezboard`

The first Roamium and Wezboard attempts failed because the sandbox blocked Cargo
from writing to caches under `~/.cargo`. Rerunning the same commands with
permission to use the normal Cargo cache completed successfully. No Chromium
clean was performed, and no Chromium branch was created.

#### Conclusion

The current `146.0.7650.0` baseline is buildable. Experiment 3 can start the
first Chromium migration attempt, targeting `148.0.7778.97` on a new Chromium
branch named `148.0.7778.97-issue-781`.

### Experiment 3: Attempt Chromium 148 Migration

#### Description

Attempt the first real Chromium upgrade by moving TermSurf's Chromium fork from
`146.0.7650.0` to `148.0.7778.97`, the current Electron-stable target identified
in Experiment 1. The purpose is to learn whether a direct jump from 146 to 148
is reasonable, or whether we need an intermediate checkpoint such as
`147.0.7727.139`.

#### Changes

1. In `chromium/src`, create a new Chromium branch named
   `148.0.7778.97-issue-781`.
2. Base the branch on the upstream `148.0.7778.97` tag.
3. Reapply the current TermSurf Chromium changes from the latest relevant
   `146.0.7650.0` issue branch.
4. Record whether the patches apply cleanly, partially, or fail.
5. If conflicts appear, classify them by area, especially:
   - Chromium embedding / profile server code
   - TermSurf protocol glue
   - build files and GN configuration
   - app bundle or packaging paths
6. Update `chromium/README.md` only if the new branch is created successfully
   and becomes the active migration branch.

Do not delete or clean `chromium/src/out/Default`. Do not use `ninja` directly;
only use `autoninja` through the project build scripts or Chromium-approved
commands.

#### Verification

This experiment passes if the new branch is created, the TermSurf patches are
reapplied, and `scripts/build.sh chromium` succeeds against `148.0.7778.97`.

It is a partial result if the branch is created but the migration exposes
conflicts or build failures that need separate follow-up experiments. Record the
exact conflict classes or failing build targets.

It fails if `148.0.7778.97` cannot be checked out, cannot be used as a branch
base, or the TermSurf changes are too tangled to classify. In that case,
Experiment 4 should try the intermediate checkpoint `147.0.7727.139`.
