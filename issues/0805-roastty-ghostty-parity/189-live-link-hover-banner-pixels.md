# Experiment 189: Live link hover banner pixels

## Description

Experiment 188 proved live regular-link hover dispatch in the real debug macOS
app: Command-modified mouse movement over a deterministic URL now reaches the
Rust link-hover path, emits the link cursor-shape request, and routes the exact
hovered URL to `Roastty.App.setMouseOverLink`.

The remaining `RUNTIME-012B2B2B2B2B3C` gap still includes native link preview
display and real OS cursor pixels. This experiment will target the deterministic
part of that gap: the copied macOS SwiftUI `URLHoverBanner` display that appears
when `surfaceView.hoverUrl` is non-nil. It will not claim the OS cursor image,
Quick Look/native preview UI, or external URL handler delivery.

The expected outcome is a new Oracle-complete runtime row for live
`URLHoverBanner` display, or a documented failure explaining why screenshot
evidence is not deterministic in this VM.

## Changes

- Add a focused guard, tentatively
  `issues/0805-roastty-ghostty-parity/macos_live_link_hover_banner_pixels.py`.
  - Reuse the Experiment 188 live app setup: isolated config, deterministic URL
    at a known terminal row/column, Command-modified CGEvent mouse movement, and
    exact-window CGWindowID capture.
  - Require the same trace proof from Experiment 188:
    `cursorShape raw=3 pointerStyle=link` and
    `mouseOverLink url=https://example.com/issue805-exp189-link-banner`.
  - Capture the exact focused window with `screencapture -l`.
  - Sample the bottom-left banner region where `URLHoverBanner` renders by
    default. Use a Swift or Python sampler to compare against a pre-hover
    baseline from the same exact window.
  - Require a deterministic non-background delta in the expected banner area,
    enough to prove a visible overlay exists after hover. Prefer text/shape
    evidence if reliable; otherwise require a bounded pixel delta in the
    banner's expected bottom band and prove the rest of the terminal did not
    change enough to explain the delta.
  - Store debug screenshot/evidence artifacts outside the repo, following the
    existing issue guard pattern.
  - Check for new Roastty crash reports.
- Update `config_runtime_inventory.py` according to the outcome:
  - If the screenshot proof passes, split a new Oracle-complete row from
    `RUNTIME-012B2B2B2B2B3C` for live URL hover banner display.
  - Keep `RUNTIME-012B2B2B2B2B3C` as a `Gap` for actual OS notification
    delivery/banner/sound, audible bell output, measurable dock-attention state,
    bell border/title visible effects, real OS cursor pixels, Quick Look/native
    link preview display if not proven by this guard, and external Launch
    Services handler delivery.
  - Do not overclaim the OS cursor image just because `pointerStyle=link` was
    requested.
- Update residual guards and stale CFG-223 counts if a new runtime row is split.
- Regenerate `config-runtime-inventory.md` and `config-matrix.md`.
- Update Issue 805 `README.md` Learnings and Experiments index after the result
  is known.

## Verification

Pass criteria:

- The guard proves exact debug-app launch, isolated config/defaults, focused
  window-to-CGWindowID mapping, terminal marker evidence, and no new Roastty
  crash report.
- The guard proves the live hover state with trace evidence for the expected URL
  and link cursor request.
- The screenshot oracle captures the exact Roastty window before and after hover
  and proves a bounded, visible bottom-banner pixel delta attributable to
  `URLHoverBanner`.
- The experiment result does not claim real OS cursor pixels, external URL
  delivery, audible bell output, dock attention, or OS notification delivery.
- Inventory counts and remaining gap IDs are updated exactly and asserted by
  guards.

Commands:

```bash
(cd roastty && macos/build.nu --action build)
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/macos_live_link_hover_banner_pixels.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/notification_link_bell_gui_residual_parity.py
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
for guard in issues/0805-roastty-ghostty-parity/*_parity.py issues/0805-roastty-ghostty-parity/*_residual_audit.py issues/0805-roastty-ghostty-parity/macos_*_runtime.py; do
  PYTHONDONTWRITEBYTECODE=1 python3 "$guard" || exit 1
done
python3 -m py_compile issues/0805-roastty-ghostty-parity/*.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --check issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/189-live-link-hover-banner-pixels.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

The result must state the exact runtime row count, Oracle-complete count, closed
count, incomplete count, gap count, CFG-223 status, and remaining gap IDs.

## Design Review

Fresh-context Codex adversarial reviewer `Zeno the 3rd` reviewed the design
against the issue workflow, the remaining CFG-223 gap, Experiment 188's live
hover guard, and the copied SwiftUI `URLHoverBanner` source.

Verdict: **Approved**.

Findings: none.

## Result

**Result:** Partial

The focused guard passed and split the deterministic SwiftUI URL-hover banner
display out of the remaining residual GUI gap.

Evidence:

- `macos_live_link_hover_banner_pixels.py` launches the built debug app with an
  isolated config, creates a real terminal surface, prints
  `https://example.com/issue805-exp189-link-banner`, and captures the exact
  focused CGWindowID before hover.
- The guard injects Command-modified mouse movement until the live trace records
  `cursorShape raw=3 pointerStyle=link` and
  `mouseOverLink url=https://example.com/issue805-exp189-link-banner`.
- The guard captures the same exact window after hover and compares the PNGs
  with an embedded Swift sampler.
- The recorded passing run measured 32674 changed pixels in the expected
  bottom-left banner band, 373 changed pixels in the upper-left control band,
  and 1086 changed pixels in the bottom-right control band.
- No new Roastty crash report was written during the workflow.

Updated inventory counts:

- `runtime_rows=93`
- `oracle_complete=89`
- `closed=92`
- `audit_covered=0`
- `incomplete=1`
- `gap=1`
- `cfg223=Gap`
- Remaining gap ID: `RUNTIME-012B2B2B2B2B3C`

Verification logs:

- `logs/issue805-exp189-build-1.log`
- `logs/issue805-exp189-live-link-banner-3.log`
- `logs/issue805-exp189-config-runtime-inventory-2.log`
- `logs/issue805-exp189-residual-guard-1.log`
- `logs/issue805-exp189-py-compile-2.log`
- `logs/issue805-exp189-prettier-check-3.log`
- `logs/issue805-exp189-diff-check-1.log`
- `logs/issue805-exp189-broad-guard-sweep-2.log`

This result does not claim real OS cursor pixels, external URL delivery, audible
bell output, dock attention, OS notification delivery, or Quick Look/native link
preview display beyond the copied SwiftUI `URLHoverBanner`.

## Conclusion

Roastty now has live app proof for the copied SwiftUI URL hover banner display.
`RUNTIME-012B2B2B2B2B3C4` is Oracle complete, while `RUNTIME-012B2B2B2B2B3C`
remains a `Gap` for OS-controlled notification, bell, real cursor-pixel, Quick
Look/native preview, and external URL-handler effects that still lack
deterministic VM evidence.

## Completion Review

Fresh-context Codex adversarial reviewer `Rawls the 3rd` reviewed the completed
experiment, working-tree diff, relevant Swift/app sources, and verification
logs.

Verdict: **Approved**.

Required findings: none.

Optional finding resolved: the reviewer noted that the result originally cited
`logs/issue805-exp189-config-runtime-inventory-1.log` while the canonical final
verification run was `logs/issue805-exp189-config-runtime-inventory-2.log`. The
verification log list now cites the final run and the other completion-gate
logs.
