# Experiment 3: Stress hosted WebKit surface lifecycle

## Description

Experiment 2 proved the central compositor boundary: a WebKit-owning process can
export a rendered `WKWebView` layer through a Core Animation context ID, and a
separate host process can display it with `CALayerHost` without creating its own
`WKWebView`.

This experiment turns that one-shot proof into a lifecycle stress proof. Before
Surfari or `libtermsurf_webkit` depend on this compositor path, the hosted
surface must demonstrate the behaviors Ghostboard will require in real panes:
resize, dynamic rendering, scroll, post-export navigation, repeated hide/show,
and stable shutdown.

This experiment should extend the existing `surfari-proofs/hosting-context/`
harness. It should not create Surfari, modify Ghostboard, modify
`termsurf.proto`, or patch WebKit source.

## Changes

- Extend `surfari-proofs/hosting-context/WebKitHostingProof.m` with a
  deterministic lifecycle/stress mode or deterministic owner/host timers.
- Keep the two-process architecture from Experiment 2:
  - owner process creates and owns the `WKWebView`;
  - host process creates a `CALayerHost` for the exported context ID;
  - host process does not create a `WKWebView`.
- Add explicit logs for each lifecycle milestone:
  - owner initial load finished;
  - context exported;
  - host ready;
  - JavaScript dynamic update observed;
  - scroll observed;
  - owner resize applied;
  - host resize applied;
  - post-export navigation finished;
  - host hide/show cycle 1 complete;
  - host hide/show cycle 2 complete;
  - final hosted content still visible;
  - owner and host terminate cleanly.
- If necessary, add simple IPC from owner to host so the owner can request host
  resize and hide/show cycles after the host starts.
- Add deterministic screenshots under `logs/` for the important visual states:
  dynamic update/scroll, resize, navigation, and final post-hide/show state.
- Update `surfari-proofs/hosting-context/README.md` with the stress-mode build
  and run command.
- Update this experiment's result with the exact command output, screenshot
  paths, and what each screenshot proves.
- Do not modify `webkit/src` in this experiment. If a WebKit source patch seems
  necessary, record **Partial** and design a later WebKit-branch experiment.

## Verification

Start from a clean TermSurf repo root:

```bash
git status --short
git -C webkit/src status --short
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --abbrev-ref HEAD
surfari-proofs/hosting-context/build.sh
```

Then run the lifecycle proof with logs in the repo `logs/` directory. The exact
command may change during implementation, but it must be recorded in the result.

The proof must demonstrate all of the following:

- The host process is separate from the owner process.
- The host process does not create a `WKWebView`.
- The hosted WebKit surface visibly updates from JavaScript after export.
- The hosted WebKit surface visibly scrolls after export.
- Owner-side WebKit resize changes are reflected in the hosted surface.
- Host-side window/layer resize does not break the hosted surface.
- Post-export navigation is visible in the hosted surface.
- At least two host hide/show cycles complete without losing the hosted surface.
- Final screenshot after hide/show still displays WebKit content in the host
  process.
- Owner and host processes terminate cleanly after the proof.
- `webkit/src` remains clean and unchanged.

**Pass** = the lifecycle harness builds, runs, logs every required milestone,
captures screenshots proving dynamic update/scroll, resize, navigation, and
final post-hide/show visibility, terminates cleanly, and leaves `webkit/src`
unchanged.

**Partial** = the harness builds and some lifecycle steps work, but one or more
required visual/lifecycle guarantees fail or require a WebKit source patch. The
result must identify the failing step and the next experiment needed.

**Fail** = the harness cannot be built or cannot reproduce the two-process
hosted WebKit surface from Experiment 2.

Before recording the result, capture:

```bash
git status --short
git -C webkit/src status --short
git -C webkit/src rev-parse HEAD
git -C webkit/src rev-parse --abbrev-ref HEAD
```

The TermSurf worktree must contain only the intended harness/docs/issue changes
plus ignored `logs/` and harness build output.

## Design Review

An adversarial Codex subagent reviewed the design with fresh context.

**Verdict:** Approved.

Findings: none.

## Result

**Result:** Pass

The hosted WebKit surface lifecycle stress proof succeeded without modifying
`webkit/src`.

Implemented changes:

- Extended `surfari-proofs/hosting-context/WebKitHostingProof.m` with
  `--owner --stress` and `--host <context-id> --stress` modes.
- Kept the two-process architecture:
  - the owner process creates and owns the `WKWebView`;
  - the host process creates only a `CALayerHost` for the exported context ID;
  - the host process logs `host_has_no_wkwebview=1`.
- Added host-side stress timers for host resize, two hide/show cycles, final
  visibility logging, and clean host termination.
- Added owner-side observation of host termination and clean owner termination
  after the host exits.
- Updated `surfari-proofs/hosting-context/README.md` with the stress-mode run
  command.

Build command:

```text
$ surfari-proofs/hosting-context/build.sh
built surfari-proofs/hosting-context/build/WebKitHostingProof
```

Stress command:

```bash
rm -f logs/issue756-exp3-lifecycle.log \
  logs/issue756-exp3-dynamic-scroll.png \
  logs/issue756-exp3-resize.png \
  logs/issue756-exp3-navigation.png \
  logs/issue756-exp3-final.png

surfari-proofs/hosting-context/build/WebKitHostingProof --owner --stress \
  > logs/issue756-exp3-lifecycle.log 2>&1 &
owner=$!
sleep 4
screencapture -x logs/issue756-exp3-dynamic-scroll.png
sleep 2
screencapture -x logs/issue756-exp3-resize.png
sleep 2
screencapture -x logs/issue756-exp3-navigation.png
sleep 2
screencapture -x logs/issue756-exp3-final.png
wait "$owner"
rc=$?
echo "OWNER_WAIT_STATUS=$rc" >> logs/issue756-exp3-lifecycle.log
```

Lifecycle log evidence from `logs/issue756-exp3-lifecycle.log`:

```text
OWNER_LOADING pid=62766 url=/Users/astrohacker/dev/termsurf/surfari-proofs/hosting-context/test-content/index.html
OWNER_NAVIGATION_FINISHED pid=62766 url=file:///Users/astrohacker/dev/termsurf/surfari-proofs/hosting-context/test-content/index.html
OWNER_EXPORTED_CONTEXT pid=62766 context_id=2543693841 webview_layer=0x99730a730
OWNER_LAUNCHED_HOST host_pid=62773 context_id=2543693841
HOST_READY pid=62773 context_id=2543693841 host_has_no_wkwebview=1
OWNER_SCRIPT_MESSAGE pid=62766 name=proof body={
    event = scrolled;
    scrollY = 720;
    status = "Owner page updated by JavaScript animation tick.";
}
OWNER_RESIZED_WEBVIEW pid=62766 size=620x388
HOST_RESIZED pid=62773 size=820x588
OWNER_NAVIGATING_AFTER_EXPORT pid=62766 url=/Users/astrohacker/dev/termsurf/surfari-proofs/hosting-context/test-content/navigation.html
OWNER_NAVIGATION_FINISHED pid=62766 url=file:///Users/astrohacker/dev/termsurf/surfari-proofs/hosting-context/test-content/navigation.html
HOST_HIDDEN pid=62773 cycle=1
HOST_HIDE_SHOW_CYCLE_1_COMPLETE pid=62773 visible=1
HOST_HIDDEN pid=62773 cycle=2
HOST_HIDE_SHOW_CYCLE_2_COMPLETE pid=62773 visible=1
HOST_FINAL_VISIBLE pid=62773 context_id=2543693841 visible=1
HOST_TERMINATING pid=62773 context_id=2543693841
OWNER_OBSERVED_HOST_TERMINATION host_pid=62773 status=0
OWNER_TERMINATING pid=62766
OWNER_WAIT_STATUS=0
```

Screenshot evidence:

- `logs/issue756-exp3-dynamic-scroll.png` shows the hosted surface after the
  JavaScript dynamic update and scroll. The hosted content is red and the
  scrollbar is moved down.
- `logs/issue756-exp3-resize.png` shows the larger host window still displaying
  hosted WebKit content after owner-side and host-side resize events.
- `logs/issue756-exp3-navigation.png` shows the hosted surface after post-export
  navigation to the navigation page.
- `logs/issue756-exp3-final.png` shows the hosted WebKit content still visible
  after two host hide/show cycles.

Final checks:

```text
$ pgrep -af WebKitHostingProof || true
<no running proof process>

$ git -C webkit/src status --short
<clean>

$ git -C webkit/src rev-parse HEAD
1452a43959523449099b2616793fd2c5b6a6487e

$ git -C webkit/src rev-parse --abbrev-ref HEAD
main
```

The first attempted stress run exposed a harness lifecycle issue: hiding the
host window caused the host app to terminate after the first hide. The fix was
to keep the host app alive when the last window is hidden during stress mode:

```objc
- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication *)sender
{
    (void)sender;
    return !self.stressMode;
}
```

The rerun passed with two complete hide/show cycles and clean host/owner
termination.

## Conclusion

The hosted WebKit surface is stable enough for the next Surfari step. It
survives dynamic JavaScript updates, scroll, owner resize, host resize,
post-export navigation, two hide/show cycles, and clean owner/host shutdown
without modifying WebKit source.

The next experiment should establish WebKit branch and patch management
analogous to Chromium before introducing any WebKit source changes for
`libtermsurf_webkit`.

## Completion Review

An adversarial Codex subagent reviewed the completed experiment with fresh
context.

**Verdict:** Approved.

Findings: none.
