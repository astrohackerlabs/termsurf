# Experiment 2: Rerun the Roamium PDF Baseline

## Description

Experiment 1 found that Roamium has strong historical PDF evidence, but the
specific log directories from the prior PDF issues are not present in this
checkout. Before adding new PDF features or starting Surfari PDF work, we need
fresh current-tree proof for the existing Roamium baseline.

This experiment reruns the existing Roamium PDF probes without changing product
behavior. The goal is to convert current `Weak evidence` matrix rows to `Proven`
only where current logs directly support that status, and to identify the first
failing layer if any baseline regresses.

## Changes

1. Prepare a clean baseline run.

   Capture the current state before running probes:

   ```bash
   git status --short
   git -C chromium/src status --short
   git -C chromium/src rev-parse --abbrev-ref HEAD
   git -C chromium/src rev-parse HEAD
   ```

   Do not modify Chromium, Roamium, Ghostboard, Surfari, WebKit, protocol,
   fixtures, or harness behavior in this experiment unless a baseline probe
   cannot run because of an obvious harness environment issue. Any such change
   must be narrowly documented and reviewed before being included in the result.

2. Run current Roamium PDF baseline probes.

   Use fresh log directories under `logs/issue-834-exp2-*`:

   ```bash
   python3 scripts/test-issue-794-pdf-toolbar.py \
     --log-dir logs/issue-834-exp2-save-title-local \
     --serve-bitcoin-pdf \
     --probe save-print-title-local \
     --enable-pdf-print-intercept
   python3 scripts/test-issue-794-pdf-toolbar.py \
     --log-dir logs/issue-834-exp2-toolbar-events \
     --serve-bitcoin-pdf \
     --probe events
   python3 scripts/test-issue-794-protocol-scroll.py \
     --log-dir logs/issue-834-exp2-protocol-scroll \
     --serve-bitcoin-pdf
   python3 scripts/test-issue-794-protocol-resize.py \
     --log-dir logs/issue-834-exp2-protocol-resize \
     --serve-bitcoin-pdf
   python3 scripts/test-issue-794-protocol-mouse.py \
     --log-dir logs/issue-834-exp2-protocol-mouse-click \
     --serve-bitcoin-pdf \
     --action click
   python3 scripts/test-issue-794-protocol-mouse.py \
     --log-dir logs/issue-834-exp2-protocol-select-copy \
     --serve-bitcoin-pdf \
     --action key-select-copy
   python3 scripts/test-issue-796-pdf-security.py \
     --log-dir logs/issue-834-exp2-security
   ```

   Also run one current non-PDF Roamium smoke using the existing interaction
   fixture:

   ```bash
   mkdir -p logs/issue-834-exp2-non-pdf-html
   python3 -m http.server 9791 \
     --bind 127.0.0.1 \
     --directory test-html/public \
     > logs/issue-834-exp2-non-pdf-html/http-server.log 2>&1 &
   HTML_SERVER_PID=$!
   python3 scripts/test-issue-794-protocol-mouse.py \
     http://127.0.0.1:9791/test-interactions.html \
     --log-dir logs/issue-834-exp2-non-pdf-html \
     --action click \
     --url-contains test-interactions.html
   kill "$HTML_SERVER_PID"
   ```

3. Record probe summaries.

   For each run, record:

   - command;
   - exit status;
   - log directory;
   - summary file path;
   - summary status or `first_failing_hop`;
   - which matrix rows the run proves or fails to prove.

4. Update the Experiment 1 matrix conservatively.

   In Experiment 2's result, list matrix status updates driven by the fresh
   runs. Do not edit Experiment 1's historical matrix in place; instead, record
   the current baseline delta in this experiment and update the Issue 834 README
   checklist only when the Roamium baseline is genuinely current.

5. Stop on baseline failure.

   If any baseline probe fails, do not continue into new feature work. Record
   the first failing layer, classify the result as `Partial` or `Fail`, and
   recommend the next experiment around that failure.

## Verification

Verification for the completed result is:

- all intended baseline probe commands are run, or skipped only with a concrete
  blocker recorded;
- every produced summary file is cited;
- passing rows include concrete evidence, not just zero exit status;
- failing rows record the first failing layer when available;
- no product code changes are made unless explicitly justified as a harness
  environment fix;
- no Chromium source changes are made unless a fresh Chromium branch and patch
  archive are created according to `chromium/AGENTS.md`;
- native print is only exercised through the contained intercept path and no
  real print job is submitted;
- README experiment status is updated from `Designed` to the final result;
- completion review is recorded before the result commit;
- markdown is formatted with Prettier;
- `git diff --check` passes.

## Design Review

Fresh-context adversarial review by Codex subagent `Laplace`: **Changes
required**, then **Approved** after fixes.

Required finding:

- The initial design omitted the non-PDF baseline smoke required by Experiment
  1's matrix and fast-smoke tier.

Fix:

- Added a concrete non-PDF Roamium smoke using
  `test-html/public/test-interactions.html`, a local `python3 -m http.server`,
  and `scripts/test-issue-794-protocol-mouse.py --action click`.

Re-review verdict: **Approved**.

## Pass Criteria

This experiment passes if the current Roamium baseline probes pass and produce
fresh evidence for the already-working core PDF rows: rendering, embedded/local
parity, scroll, resize, click, selection/copy, toolbar zoom/fit/rotate,
save/download, title propagation, contained print safety, security, and non-PDF
regression where covered by the selected probes.

## Partial Criteria

This experiment is partial if at least one baseline area still works and is
recorded with fresh evidence, but one or more baseline probes fail or cannot
run.

## Failure Criteria

This experiment fails if the baseline cannot be run at all, if it changes
product behavior instead of measuring the current baseline, if it clicks
production native print without containment, or if it claims current proof
without fresh probe evidence.
