# Experiment 2: Document Launch Discovery Contract

## Description

Experiment 1 made named/default `roamium` launch deterministic for debug runs by
resolving it only through `TERMSURF_ROAMIUM_PATH`. Issue 814 still needs the
ordinary launch and discovery workflow to be explicit enough that developers can
tell which binary Ghostboard should spawn, which socket webtui should use, and
which behavior remains deferred to packaging work.

This experiment will document the current Ghostboard launch/discovery contract
and add lightweight script checks for that contract. It will not add broad
installed-app discovery. That belongs to Issue 819 unless this experiment proves
Issue 814 cannot be closed without it.

## Changes

Planned documentation changes:

- `docs/ghostboard-launch-discovery.md`
  - Document how debug Ghostboard is launched directly from
    `ghostboard/macos/build/Debug/TermSurf.app/Contents/MacOS/termsurf`.
  - Document how the app exposes `TERMSURF_SOCKET` to shell commands running
    inside Ghostboard.
  - Document browser selection rules:
    - an explicit absolute `--browser` path is spawned exactly as provided;
    - omitted/default webtui browser becomes named `roamium`;
    - named `roamium` resolves only through absolute `TERMSURF_ROAMIUM_PATH`;
    - missing, empty, or relative `TERMSURF_ROAMIUM_PATH` fails clearly;
    - Ghostboard must not silently fall through to stale installed paths during
      debug testing.
  - Document the current boundary between Issue 814 and Issue 819: Issue 814
    covers deterministic debug launch and explicit failure, while Issue 819
    covers app/package identity and normal installed distribution paths.

- `issues/0814-ghostboard-launch-discovery-workflow/README.md`
  - Link the new launch/discovery documentation from the issue analysis.
  - Record the experiment result and conclusion after verification.

Planned harness changes:

- `scripts/ghostboard-geometry-matrix.sh`
  - Add a lightweight contract mode or scenario that inspects the generated
    command and app environment for the existing absolute-path, named-debug, and
    invalid-env scenarios without launching a full GUI when possible.
  - If a no-launch check is awkward in the existing harness, keep the runtime
    scenarios unchanged and add focused log assertions to the existing
    `named-roamium-debug-launch` and `named-roamium-invalid-env` paths.
  - The checks must prove that the documented rules match the actual harness
    behavior, especially that default/named launch omits `--browser` and that
    invalid relative `TERMSURF_ROAMIUM_PATH` cannot create a pending browser
    server.

## Verification

Static checks:

1. `prettier --write --prose-wrap always --print-width 80 docs/ghostboard-launch-discovery.md issues/0814-ghostboard-launch-discovery-workflow/README.md issues/0814-ghostboard-launch-discovery-workflow/02-document-launch-discovery-contract.md`
2. `bash -n scripts/ghostboard-geometry-matrix.sh`
3. `shellcheck scripts/ghostboard-geometry-matrix.sh` if available.
4. `git diff --check`

Runtime or contract checks:

1. Run the new contract check if one is added.
2. Run `scripts/ghostboard-geometry-matrix.sh named-roamium-debug-launch` if the
   harness changed assertions on that scenario.
3. Run `scripts/ghostboard-geometry-matrix.sh named-roamium-invalid-env` if the
   harness changed assertions on that scenario.
4. Inspect the resulting logs and confirm the documentation claims match the
   observed behavior.

Pass criteria:

- The docs describe exactly the current Ghostboard debug launch/discovery
  behavior, including the explicit boundary with Issue 819.
- The harness or contract check proves the documented debug/default browser
  selection rules.
- The experiment does not introduce broad installed-app discovery or packaging
  behavior that belongs to Issue 819.
- All edited markdown is formatted, shell syntax checks pass, and
  `git diff --check` is clean.

Partial criteria:

- Documentation is accurate, but an additional runtime assertion needs a later
  experiment because it would require restructuring the GUI harness.

Fail criteria:

- Documentation contradicts the implementation or final Experiment 1 logs.
- The harness can no longer prove that debug default/named browser launch avoids
  stale installed paths.
- The experiment expands into packaging identity work that should stay in
  Issue 819.

## Design Review

Fresh-context adversarial review by Codex subagent `Erdos`:

- **Verdict:** Approved.
- **Findings:** None required.
- **Optional finding:** Add `shellcheck scripts/ghostboard-geometry-matrix.sh`
  if available because the experiment may edit the shell harness.
- **Resolution:** Accepted the optional finding and added the shellcheck check
  to the static verification list.

## Result

**Result:** Pass

Implemented the launch/discovery contract documentation and added a no-GUI
contract check to the Ghostboard geometry harness.

Changes:

- Added `docs/ghostboard-launch-discovery.md` describing:
  - direct debug app launch from
    `ghostboard/macos/build/Debug/TermSurf.app/Contents/MacOS/termsurf`;
  - `TERMSURF_SOCKET` discovery by webtui;
  - absolute `--browser` spawn behavior;
  - default/named `roamium` resolution through absolute `TERMSURF_ROAMIUM_PATH`;
  - clear failure for missing, empty, or relative named Roamium paths;
  - the debug-vs-installed boundary between Issue 814 and Issue 819.
- Linked that document from the Issue 814 README analysis.
- Added `scripts/ghostboard-geometry-matrix.sh launch-discovery-contract`, which
  exits before GUI launch and proves:
  - explicit absolute-path launch uses `--browser`;
  - named/default launch omits `--browser`;
  - the debug named Roamium environment path is absolute;
  - the invalid-env sentinel remains relative.

Verification commands:

1. `prettier --write --prose-wrap always --print-width 80 docs/ghostboard-launch-discovery.md issues/0814-ghostboard-launch-discovery-workflow/README.md issues/0814-ghostboard-launch-discovery-workflow/02-document-launch-discovery-contract.md`
2. `bash -n scripts/ghostboard-geometry-matrix.sh`
3. `shellcheck scripts/ghostboard-geometry-matrix.sh` if available.
4. `scripts/ghostboard-geometry-matrix.sh launch-discovery-contract`
5. `git diff --check`

Notes:

- `shellcheck` is not installed on this VM, so that optional check was skipped.
- The existing `named-roamium-debug-launch` and `named-roamium-invalid-env`
  runtime assertions were not changed in this experiment. Experiment 1 remains
  the runtime evidence for those full GUI paths.

Final contract evidence:

- Harness:
  `logs/ghostboard-geometry-launch-discovery-contract-harness-20260617-211011.log`

The contract run proved:

- the absolute launch command includes `--browser` with the debug Roamium path;
- the named/default launch command omits `--browser`;
- the named Roamium debug environment is absolute; and
- the invalid named Roamium environment sentinel is relative.

## Conclusion

The Issue 814 launch/discovery contract is now explicit and covered by a cheap
regression check. Debug runs have documented rules for socket discovery,
absolute browser paths, named/default Roamium resolution, invalid named-browser
failure, and stale installed-path avoidance.

Installed distribution identity and normal installed Roamium discovery remain
out of scope for Issue 814 and belong to Issue 819.

## Completion Review

Fresh-context adversarial review by Codex subagent `Newton`:

- **Verdict:** Approved.
- **Findings:** None.
- **Verification reviewed:** `bash -n scripts/ghostboard-geometry-matrix.sh`,
  `git diff --check`, the unavailable `shellcheck` note, the uncommitted result
  state, the `Pass` README status, the Result/Conclusion sections, and the
  no-GUI `launch-discovery-contract` scenario.
- **Evidence:** The reviewer confirmed the new contract scenario exits before
  GUI launch and checks the documented command/environment contract. The
  reviewer also confirmed the Ghostboard resolver behavior matches the docs and
  that Experiment 1 remains the runtime evidence for socket discovery, named
  Roamium resolution, invalid-env failure, no pending server, and stale
  installed-path avoidance.
