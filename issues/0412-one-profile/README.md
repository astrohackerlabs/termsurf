+++
status = "closed"
opened = "2026-02-11"
closed = "2026-02-11"
+++

# Issue 412: One Profile — Isolate the 2fps Cause

## Goal

Starting from a minimal app that is nearly identical to Content Shell (60fps),
add changes one at a time toward a two-profile side-by-side layout. Each step is
a self-contained experiment. The step where fps drops from 60 to 2 identifies
the exact cause of the rendering degradation.

## Background

Content Shell runs at 60fps with a single profile. The Two Profiles app (Issue
407) runs at 2fps — both panes, including Shell A which uses the same
`Shell::CreateNewWindow` code path. Issues 410 and 411 spent five experiments
targeting throttling and visibility code paths that turned out to be irrelevant.
The actual cause remains unknown.

The Two Profiles app differs from Content Shell in several ways. Any one of them
could be the culprit:

1. Custom `TwoProfilesMainParts` subclass of `ShellBrowserMainParts`
2. `SHELL_DIR_USER_DATA` path override (changes the global profile path)
3. Second `ShellBrowserContext` with a different storage path
4. Second `WebContents` created and navigated
5. View hierarchy manipulation (adding a second NSView, resizing the first)

Rather than guessing, we will isolate the cause by adding these changes one at a
time.

## Branch

Create a new branch `146.0.7650.0-issue-412` in the `termsurf-chromium`
submodule, starting from the vanilla Chromium `146.0.7650.0` tag. Cherry-pick
the Two Profiles app commit to get the build scaffolding, then apply each step
as a commit on top.

## Steps

### Step 1: Baseline — Content Shell equivalent

Strip the Two Profiles app down to a single `Shell::CreateNewWindow` call with
no path overrides, no second BrowserContext, and no view manipulation. This
should be functionally identical to Content Shell.

```
InitializeBrowserContexts: default (inherited from ShellBrowserMainParts)
InitializeMessageLoopContext: Shell::CreateNewWindow(browser_context(), url, ...)
```

**Expected: 60fps.** If this is 2fps, the problem is in the app scaffolding
itself (BUILD.gn, delegates, plists) and not in any of our code changes.

### Step 2: Override SHELL_DIR_USER_DATA

Add the `SHELL_DIR_USER_DATA` override to point profile-a at
`~/.config/termsurf/poc/profile-a`.

```
InitializeBrowserContexts:
  PathService::Override(SHELL_DIR_USER_DATA, GetProfilePath("profile-a"))
  set_browser_context(new ShellBrowserContext(false))
```

**Expected: 60fps.** If this drops to 2fps, the path override is interfering
with the storage service or some other subsystem that depends on the default
path.

### Step 3: Add second BrowserContext

Create `browser_context_b_` with a path override to profile-b. Don't use it for
anything — just create and hold it.

```
InitializeBrowserContexts:
  PathService::Override(SHELL_DIR_USER_DATA, GetProfilePath("profile-a"))
  set_browser_context(new ShellBrowserContext(false))
  PathService::Override(SHELL_DIR_USER_DATA, GetProfilePath("profile-b"))
  browser_context_b_ = make_unique<ShellBrowserContext>(false)
```

**Expected: 60fps.** If this drops to 2fps, creating a second BrowserContext
interferes with Shell A's rendering — possibly through the global
`SHELL_DIR_USER_DATA` being left pointing at profile-b, or through the storage
service trying to serve both contexts from one root.

### Step 4: Add second WebContents (no view attachment)

Create a second `WebContents` with `browser_context_b_` and navigate it to the
test page, but do not add its view to any window.

```
InitializeMessageLoopContext:
  Shell::CreateNewWindow(browser_context(), url, ...)
  web_contents_b_ = WebContents::Create(CreateParams(browser_context_b_))
  web_contents_b_->GetController().LoadURLWithParams(url)
```

**Expected: 60fps.** If this drops to 2fps, the act of creating and navigating a
second WebContents (even without displaying it) triggers something that degrades
Shell A's rendering — possibly the storage service crash, renderer process
contention, or compositor interference.

### Step 5: Attach second view side by side

Add WebContents B's view to Shell A's window, side by side. This is the full Two
Profiles layout.

```
InitializeMessageLoopContext:
  ... (same as step 4)
  [container addSubview:view_b]
  view_a.frame = left half
  view_b.frame = right half
```

**Expected: Shell A 60fps, Shell B unknown.** If Shell A drops to 2fps here, the
view hierarchy manipulation is the cause. If Shell A stays at 60fps but Shell B
is at 2fps, the race condition from Issue 411 is confirmed as the cause for
Shell B specifically.

## Process

For each step:

1. Modify `two_profiles_main_parts.{h,mm}` to match the step's description.
2. Build with `autoninja -C out/Default two_profiles`.
3. Run the app and observe the fps in the test page.
4. Record the result (fps for each visible pane).
5. If fps dropped, stop — the cause is identified. Investigate further.
6. If fps is still 60, proceed to the next step.

## Experiments

### Experiment 1: Clone Content Shell

#### Branch setup

1. `cd ts4/termsurf-chromium/src`
2. `git checkout -b 146.0.7650.0-issue-412 146.0.7650.0`
3. Clone `content/shell/` to `content/one_profile/`. Rename classes, targets,
   and bundle names from "Content Shell" / "content_shell" to "One Profile" /
   "one_profile". The behavior is identical — same delegates, same `Shell`
   class, same `ShellBrowserMainParts`, same window management. The only
   difference is the name.
4. Add `//content/one_profile` to the root `BUILD.gn` `gn_all` group.
5. Build with `autoninja -C out/Default one_profile`.

#### Hypothesis

One Profile is a byte-for-byte clone of Content Shell with different names.
Content Shell runs at 60fps. One Profile should too. This establishes that our
build scaffolding, bundle structure, and renamed targets don't introduce any
regressions. It gives us a known-good starting point to iterate from.

#### Design

Copy the entire `content/shell/` directory to `content/one_profile/`. This
includes everything: browser/, renderer/, common/, utility/, gpu/, app/,
BUILD.gn, plists, resources. One Profile is a fully independent copy of Content
Shell — its own libraries, its own app target, its own bundle.

Find-and-replace rename across all copied files:

- `content/shell` → `content/one_profile` (include paths, GN source paths)
- `content_shell` → `one_profile` (target names, identifiers, bundle IDs)
- `ContentShell` → `OneProfile` (class names using this form)
- `Content Shell` → `One Profile` (strings, bundle names, plists)
- `CONTENT_SHELL_` → `CONTENT_ONE_PROFILE_` (include guards)
- `content.shell` → `content.one_profile` (Java package names, if any)
- `Shell` class references stay as-is (ShellBrowserMainParts,
  ShellContentBrowserClient, Shell, ShellBrowserContext, etc.)

GN deps that point to `//content/shell/...` within the copied BUILD.gn files
must be updated to `//content/one_profile/...` so One Profile builds against its
own copy. External deps (to `//content/public/...`, `//base/...`, etc.) stay
as-is.

No behavioral changes. One Profile is Content Shell under a different name.

#### Expected result

60fps. This must pass before any further steps. If this is not 60fps, something
is wrong with the clone and we fix it before proceeding.

#### Build issues

Three issues prevented a straight copy-and-rename from compiling.

**1. GN visibility restrictions.** Several Chromium components restrict who can
depend on them via `visibility` and `friend` lists that include
`//content/shell/*` but not `//content/one_profile/*`. Four files needed
patching:

- `components/cdm/renderer/BUILD.gn` — added `//content/one_profile/*` to
  visibility
- `components/cdm/browser/BUILD.gn` — same
- `components/cdm/common/BUILD.gn` — same
- `net/dns/BUILD.gn` — added `//content/one_profile:one_profile_lib` to friend
  list

**2. Duplicate resource output.** Both Content Shell and One Profile define a
`copy_shell_resources` target that outputs `shell_resources.pak` to
`$root_out_dir`. Renamed One Profile's target to `copy_one_profile_resources`
with output `one_profile_resources.pak`. Also added a grit resource ID entry for
`content/one_profile/shell_resources.grd` in
`tools/gritsettings/resource_ids.spec` (ID 8010, before Content Shell's 8020 to
maintain monotonic ordering).

**3. Web test class name collisions.** Content Shell serves double duty: it is
both a minimal Content API embedder and the host for Chromium's "web tests"
(formerly "layout tests") — the test suite that verifies web platform behavior
(HTML rendering, CSS, JavaScript APIs). Content Shell's `shell_main_delegate.cc`
conditionally includes `content/web_test/` headers, which subclass Content
Shell's `ShellContentBrowserClient` and `ShellContentRendererClient`. Because
our rename kept the `Shell*` class names unchanged (they're the Content API
classes, not Content Shell-specific), both copies define identically-named
classes in the `content` namespace. When `shell_main_delegate.cc` includes
`content/web_test/...` which transitively includes `content/shell/...`, the
compiler sees two definitions of `ShellContentBrowserClient`,
`ShellSpeechRecognitionManagerDelegate`, and `ShellContentRendererClient`.

Fixed by removing web test support from One Profile entirely:

- Removed `content/web_test` includes from `app/shell_main_delegate.cc`
- Removed web test conditional code in `BasicStartupComplete()`, `RunProcess()`,
  `CreateContentBrowserClient()`, and `CreateContentRendererClient()`
- Removed `WebTestBrowserMainRunner` member and `OsSettingsProvider` member from
  `app/shell_main_delegate.h`
- Removed `//content/web_test:*` deps from `BUILD.gn`

One Profile doesn't need web test support — it exists purely for our fps
isolation experiments.

#### Result: PASSED

One Profile runs at 60fps. The spinning blue square renders smoothly with the
same framerate as Content Shell. The baseline is established — our build
scaffolding, renamed targets, and independent copy of the Content Shell
libraries do not introduce any rendering regressions.

#### Conclusion

One Profile is a confirmed 60fps clone of Content Shell. The three build issues
were all mechanical (GN visibility, resource naming, web test collisions) and do
not affect runtime behavior. We now have a known-good starting point from which
to add changes incrementally toward two profiles. The next experiment adds the
`SHELL_DIR_USER_DATA` override (Step 2).

## Conclusion

Issue 412 accomplished its first goal: establish a known-good baseline. One
Profile is a fully independent clone of Content Shell — 218 files copied, six
find-and-replace renames applied, three build issues resolved — that runs at
60fps. This proves that the app scaffolding (BUILD.gn structure, bundle layout,
renamed targets, independent libraries) does not cause any rendering
degradation.

The remaining steps (2–5) have not been executed yet. The isolation approach is
sound — each step adds exactly one variable from the Two Profiles app — but the
next issue will continue from this baseline rather than staying within
Issue 412.

### What we have

- Branch `146.0.7650.0-issue-412` in the `termsurf-chromium` submodule with one
  commit on top of the vanilla `146.0.7650.0` tag
- `content/one_profile/` — a complete, buildable, 60fps Content Shell clone with
  its own libraries, delegates, and app bundle
- A proven recipe for cloning Content Shell: copy, rename, patch visibility
  lists, fix resource collisions, strip web test support

### What's next

Continue the isolation from Step 2 onward using the One Profile app as the base.
Each step modifies `content/one_profile/` to add one more element from the Two
Profiles app until the fps drops. The step where it drops identifies the root
cause of the 2fps degradation.
