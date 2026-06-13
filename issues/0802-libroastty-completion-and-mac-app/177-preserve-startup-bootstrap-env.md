# Experiment 177: Phase C — preserve startup bootstrap env

## Description

Diagnose and fix the live smoke-marker gap found by Experiment 176.

Experiment 176 proved that the copied `Roastty.app` selects the real CoreVideo
display-link present driver and exits cleanly, but both live smoke captures
showed only the shell prompt instead of the harness marker. The harness writes a
temporary zsh bootstrap and launches each app with `ZDOTDIR` and
`XDG_CONFIG_HOME` in the app process environment. Ghostty preserves that
environment through shell-integration setup, so its zsh integration restores the
temporary `ZDOTDIR` and sources the bootstrap. Roastty currently starts termio
with only `Surface.env_vars` in `TermioSpawnOptions.env`; `PtyCommand` would
inherit the process environment, but shell-integration setup overwrites
`ZDOTDIR` before the child launches and cannot preserve the inherited value
because it is not visible in the explicit env vector. That loses the harness
bootstrap.

This experiment should make the termio child environment explicit and
upstream-shaped: start from the current app process environment, apply terminal
identity and shell-integration edits to that base, and then apply surface/config
env overrides last. That ordering lets zsh setup preserve the harness-provided
process `ZDOTDIR` as `ROASTTY_ZSH_ZDOTDIR` while preserving upstream's rule that
explicit surface env overrides win after integration. Then rerun the live A/B
smoke proof with strict evidence that the Roastty screenshot contains the
marker.

## Changes

- `roastty/src/termio.rs`
  - Treat `TermioSpawnOptions.env` as explicit env overrides, matching upstream
    embedded `env_override` behavior.
  - Add a small helper that builds the base spawn environment from the current
    process environment.
  - Apply terminal identity/features and shell-integration setup to the base
    inherited environment, then apply `TermioSpawnOptions.env` last.
  - Add focused termio tests proving inherited env reaches the child, explicit
    env overrides win after integration, and forced zsh integration preserves
    and sources an inherited bootstrap `ZDOTDIR`.

- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Link this experiment as `Designed`.
  - After the run, mark it `Pass`, `Partial`, or `Fail`.
  - If the live marker renders in Roastty and the app still selects
    `present-driver=display-link reason=core-video`, update the Experiment 176
    dependency in the roadmap/experiment notes as appropriate. Do not check the
    broader render-thread item unless cursor-blink timer parity is also proven.

- `issues/0802-libroastty-completion-and-mac-app/177-preserve-startup-bootstrap-env.md`
  - Record the exact implementation, verification commands, live screenshot/log
    paths, result, conclusion, and AI completion review.

## Verification

Before implementation:

- Codex-native adversarial design review approves this experiment.
- Commit the reviewed plan separately from the result.

Focused tests:

- `cargo test -p roastty termio_env -- --test-threads=1`
- `cargo test -p roastty zsh_integration -- --test-threads=1`

Regression checks:

- `cargo test -p roastty --test abi_harness`
- `cargo test -p roastty -- --test-threads=1`
- `cargo fmt --check -p roastty`
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/177-preserve-startup-bootstrap-env.md issues/0802-libroastty-completion-and-mac-app/README.md`
- `git diff --check`

Live proof:

- Rebuild the copied app:

  ```bash
  cd roastty && macos/build.nu --action build
  ```

- Run the smoke harness with display-link logging:

  ```bash
  scripts/roastty-app/stop-app.sh
  TERMSURF_AB_HOLD_SECONDS=10 \
  ROASTTY_PRESENT_DRIVER_LOG=1 \
    scripts/roastty-app/live-ab-smoke.sh \
      --recipe smoke \
      --comparison-region content \
      --max-mismatch-ratio 1 \
      --max-mean-channel-delta 255
  ```

- Assert the Roastty stderr log contains:

  ```text
  present-driver=display-link reason=core-video
  ```

- Assert a stronger marker oracle than the harness JSON. Prefer a machine check
  when feasible: OCR the new
  `/Users/ryan/.cache/termsurf/shots/roastty-ab-content-<stamp>.png`, or compare
  a marker row crop against Ghostty's marker-containing crop with a much tighter
  region. If machine OCR/comparison is unavailable, record a saved inspection
  note with the exact screenshot path, marker string, and visual observation.
  The harness JSON alone is not sufficient evidence because Experiment 176
  showed permissive thresholds can pass while the marker is absent.

- Prove no debug Roastty app PID remains:

  ```bash
  if pgrep -f 'roastty/macos/build/.*Roastty.app/Contents/MacOS/roastty'; then
    exit 1
  fi
  ```

**Pass** = focused tests prove inherited env and zsh bootstrap preservation, the
copied app rebuilds, the live Roastty content screenshot visibly contains the
smoke marker, stderr proves the CoreVideo display-link driver was selected, and
no debug Roastty app PID remains.

**Partial** = env/unit behavior is fixed but live capture still cannot prove the
marker, or the marker renders but display-link selection/cleanup cannot be
proved. Record the exact blocker and artifact paths.

**Fail** = the env fix breaks termio spawning, zsh integration, app build,
launch, live rendering, or cleanup.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Euler`, fresh context.

**Initial verdict:** Changes required.

Findings and fixes:

- Required: the first design applied `TermioSpawnOptions.env` before terminal
  identity and shell-integration setup, which was not faithful to upstream.
  Upstream treats embedded surface env as a final `env_override` after shell
  integration. Fixed by splitting inherited process env from explicit overrides:
  inherited env is the base, terminal identity/features and shell integration
  mutate that base, then `TermioSpawnOptions.env` is applied last.
- Optional: marker proof still depended on subjective visual inspection. Fixed
  by requiring a stronger oracle than the harness JSON: prefer OCR or a tighter
  marker-row comparison, and if that is unavailable record an explicit saved
  inspection note with screenshot path, marker string, and observation.

**Final verdict:** Approved.

Final findings: None.
