+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"
+++

# Experiment 7: Phase B — copy + rename the Ghostty macOS app; first build against RoasttyKit

## Description

With `RoasttyKit.xcframework` built (Exp 6), copy Ghostty's macOS app into the
roastty project, mechanically rename `Ghostty`→`Roastty` / `ghostty`→`roastty`,
point it at `RoasttyKit`, and attempt the **first build** — which **confirms the
Exp-6 worklist against the real app** (the Swift compiler + linker will name
exactly the missing/divergent symbols). The app is otherwise **unaltered**
(workstream 2's whole point: an unmodified app is the conformance oracle).

This is expected to **not fully build** — the 3 by-value ABI divergences (Exp 6)
mean the Swift code constructs types (`roastty_input_key_s`, point-based
`roastty_selection_s`, the `roastty_action_s` union) that `roastty.h` doesn't
yet expose in the embedded shape, and 6 functions are missing. The deliverable
is the **renamed app source (committed) + the exact build-error list** that
drives Exp 8 (the by-value embedded ABI in `libroastty`).

## Approach

1. **Copy** `vendor/ghostty/macos` → `roastty/macos`, **excluding build
   artifacts** (`build/`, `*.xcframework`) — only the source (~8 MB: `Sources`,
   `Assets.xcassets`, `Tests`, `GhosttyUITests`, `Ghostty.xcodeproj`,
   entitlements, `Ghostty-Info.plist`, `Ghostty.sdef`, `Ghostty.xctestplan`,
   `build.nu`, `AGENTS.md`). `RoasttyKit.xcframework` is already in
   `roastty/macos/` (Exp 6, gitignored).
2. **Rename file/dir names** `Ghostty*`→`Roastty*` — the `.xcodeproj`,
   entitlements, Info plist, sdef, xctestplan, `Sources/Ghostty/` + its
   `Ghostty.*.swift`, `GhosttyUITests/`
   - tests, `Tests/Ghostty`, and **explicitly the shared scheme
     `xcshareddata/xcschemes/Ghostty.xcscheme`→`Roastty.xcscheme`** (xcodebuild
     resolves a scheme by filename — miss this and the build never reaches Swift
     compilation).
3. **Content find/replace** (case-sensitive, text files only — skip
   `Assets.xcassets` binaries): `Ghostty`→`Roastty`, `ghostty`→`roastty`,
   `GHOSTTY`→`ROASTTY`. One sweep: C symbols (`ghostty_*`→`roastty_*`),
   module/type (`GhosttyKit`→`RoasttyKit`, `import GhosttyKit`), the header
   (`ghostty.h`→`roastty.h`), bundle ids
   (`com.mitchellh.ghostty`→`com.mitchellh.roastty`), `.pbxproj` refs, and the
   scheme's internal `Blueprint/BuildableName`. The app reaches nothing past the
   C ABI (verified: all 36 lib accesses go through `import GhosttyKit`;
   `@_silgen_name` uses are private Apple symbols), so the rename within
   `roastty/macos/` is build-safe. **Known runtime breakage (out of build
   scope):** the sweep also rewrites the Sparkle appcast feeds
   (`*.files.ghostty.org/appcast.xml`) and doc/help URLs (`ghostty.org`,
   `github.com/ghostty-org`) to nonexistent `roastty.org` domains — the
   auto-updater and help links are knowingly dead on the conformance host; not a
   build concern.
4. **Strip the out-of-tree resource inputs.** The app's Resources build phases
   reference resources **outside** `macos/`:
   `../zig-out/share/{ghostty,bat,fish,vim,nvim,man,locale, terminfo,zsh,bash-completion}`
   (Zig build outputs) and `../images/Ghostty.icon`. A `macos/`-only copy lacks
   these, so the build would fail on **missing inputs unrelated to the ABI**.
   Remove those build-file/Resources-phase entries from the renamed `.pbxproj`
   so the build reaches Swift compilation. **The resource bundle (terminfo,
   shell-integration, syntax, icon) is a documented deferred item** (Roadmap
   Phase I / resource bundling) — not part of confirming the ABI worklist.
5. **Point the project at RoasttyKit** — the `.pbxproj`'s
   `GhosttyKit.xcframework` ref is a relative `<group>` path (verified) →
   becomes `RoasttyKit.xcframework`, satisfied by
   `roastty/macos/RoasttyKit.xcframework` (Exp 6). No Zig build phase exists
   (the only shell phase is SwiftLint, gated off locally).
6. **First build** via the renamed `build.nu` (Debug) under Xcode 26.4 (the lib
   is prebuilt in RoasttyKit). Collect every Swift-compile + linker error.

The work is captured in a re-runnable `scripts/roastty-app/rename-app.sh`
(copy + rename + replace) so the app can be regenerated from the pinned
upstream.

## Changes / Deliverables

- `roastty/macos/` — the **copied, renamed app source** (committed; ~8 MB).
  `RoasttyKit.xcframework` stays gitignored.
- `scripts/roastty-app/rename-app.sh` — reproducible copy+rename+replace from
  `vendor/ghostty/macos`.
- The **first-build error list** (this doc's Result), classified against the
  Exp-6 worklist (expected vs surprises).
- Lessons update (the rename recipe; the build entrypoint).

## Verification

1. `roastty/macos` exists, renamed, no `Ghostty`/`ghostty` tokens remain
   (`grep -rl` is empty over text files) and no `build/`/`*.xcframework` source
   copied.
2. The renamed `build.nu` runs and the build **reaches Swift compilation**
   (project/scheme resolve; RoasttyKit is found).
3. The build errors are collected and **each maps to the Exp-6 worklist** (the 6
   missing fns + the 3 by-value types + the enum-name drifts) — or any
   **surprise** error is recorded as a worklist addition.

**Gating precondition (checked before classifying ABI errors):** the build must
**reach Swift compilation cleanly of non-ABI causes** — zero residual `ghostty`
tokens, the `Roastty` scheme resolves, `RoasttyKit` links, and the out-of-tree
resource inputs are stripped (no missing-input errors). Only then are the
remaining errors judged against the worklist.

**Pass** = precondition met, and the remaining first-build errors **match the
Exp-6 worklist** (the 6 missing fns + the 3 by-value types
`input_key_s`/`selection_s`/`action_s`

- the enum-name drifts) with **no surprises** — the worklist is confirmed
  complete against the real app. (A clean build is not expected yet; that's Exp
  8.)

**Partial** = precondition met and it surfaces errors, but **new** ones appear
(an un-audited ABI corner) that widen the worklist — still a go.

**Fail** = the rename/project is structurally broken (the `.pbxproj` won't open,
the scheme won't resolve, or the build can't reach Swift compilation) with no
reasonable fix — documented.

## Design Review

**Reviewer:** `adversarial-reviewer` subagent (Claude Opus, fresh context,
read-only). **Verdict: CHANGES REQUIRED → addressed.** It confirmed the safe
parts (no Zig build phase — only a locally-disabled SwiftLint phase;
`GhosttyKit.xcframework` is a relative `<group>` path that RoasttyKit satisfies;
entitlements have no bundle-id-coupled app-groups/keychain; ad-hoc signing
`DEVELOPMENT_TEAM=""`/`CODE_SIGN_IDENTITY="-"`; all 36 lib accesses go through
`import GhosttyKit`, so the C-ABI conformance assumption holds). Findings,
addressed:

- **Required — out-of-tree resource inputs.** The Resources phases reference
  `../zig-out/share/{ghostty,bat,fish,vim,nvim,man,locale,terminfo,zsh,bash-completion}`
  and `../images/Ghostty.icon` — outside `macos/`; a `macos/`-only copy fails on
  missing inputs unrelated to the ABI, making the Pass bar unreachable.
  **Fixed:** added step 4 to **strip those entries** from the renamed
  `.pbxproj`; the resource bundle is a documented deferred item.
- **Required — Pass-bar falsifiability.** **Fixed:** added a **gating
  precondition** (build reaches Swift compilation with zero non-ABI errors)
  before classifying remaining errors against the worklist.
- **Optional — `.xcscheme` rename.** xcodebuild resolves a scheme by
  **filename**. **Fixed:** step 2 now renames
  `Ghostty.xcscheme`→`Roastty.xcscheme` explicitly.
- **Optional — URL rename breaks the updater/docs at runtime.** **Fixed:** step
  3 now states the Sparkle appcast + doc URLs become dead `roastty.org` domains
  (knowingly broken on the conformance host; out of build scope) instead of
  claiming blanket safety.

## Result

_(to be added after the run — the first-build error list.)_

## Conclusion

_(to be added after the run.)_
