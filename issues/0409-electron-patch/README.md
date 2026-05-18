+++
status = "closed"
opened = "2026-02-09"
closed = "2026-02-09"
+++

# Issue 409: Apply Electron's Chromium Patch Set

## Goal

Apply Electron's full 147-patch set to our `termsurf-chromium` submodule so that
Two Profiles (and future TermSurf browser panes) render at 60fps. Track the same
Chromium version as Electron (146.0.7650.0) and apply the exact same patches
with no modifications.

## Background

Issue 407 proved that multiple `BrowserContext` instances coexist in one
Chromium process with full profile isolation, but rendering was throttled to
2-3fps. Issue 408 traced the problem to three independent throttling systems in
Chromium's rendering pipeline and discovered that Electron solves this with a
well-tested set of patches. Rather than cherry-picking a subset, we adopt the
full patch set — it's simpler, tested, and future-proof.

## Relationship to Other Issues

| Issue | Relationship                                                      |
| ----- | ----------------------------------------------------------------- |
| 407   | Proved multi-profile works; identified 2-3fps throttling          |
| 408   | Traced throttling to three layers; discovered Electron's solution |
| 409   | This issue — applies the patch set to our fork                    |

## Fork Structure

Our `termsurf-chromium` submodule (at `ts4/termsurf-chromium/src/`) is a
Chromium fork with a linear commit history:

```
146.0.7650.0           tag    ← vanilla Chromium
146.0.7650.0-electron  branch ← + Electron's 147 patches (applied as commits)
146.0.7650.0-termsurf  branch ← + TermSurf's commits (submodule points here)
```

Electron does not maintain a Chromium fork — it applies patches at build time.
We take a different approach: the patches become permanent commits in our fork.
TermSurf's own modifications are regular commits on top.

### Branch and tag convention

Each Chromium version produces three references:

| Reference                   | Type   | Purpose                                    |
| --------------------------- | ------ | ------------------------------------------ |
| `146.0.7650.0`              | tag    | Vanilla Chromium (from upstream)            |
| `146.0.7650.0-electron`     | branch | Electron's patches applied on top           |
| `146.0.7650.0-termsurf`     | branch | TermSurf's commits on top of Electron       |

The submodule points to the `-termsurf` branch. This makes it easy to diff
between layers:

```bash
git log 146.0.7650.0..146.0.7650.0-electron          # what Electron changed
git log 146.0.7650.0-electron..146.0.7650.0-termsurf  # what TermSurf changed
```

When Electron bumps Chromium versions, new branches are created with the new
version number. Old branches stay around as history.

### Rebase workflow

TermSurf's modifications are maintained as regular commits — not a separate
patch set. When Electron updates its patches (same Chromium version), we rebase:

```
1. Re-apply the updated Electron patches → update 146.0.7650.0-electron
2. Rebase 146.0.7650.0-termsurf onto the updated -electron branch
3. Rebuild and test
```

When Electron bumps to a new Chromium version entirely:

```
1. Check out the new vanilla Chromium tag
2. Apply the Electron patches → create <new-version>-electron
3. Cherry-pick or rebase TermSurf's commits → create <new-version>-termsurf
4. Update the submodule to point to the new -termsurf branch
5. Rebuild and test
```

This works cleanly because TermSurf's changes don't overlap with Electron's
patches — our files (`content/two_profiles/`) are entirely new, and our
`BUILD.gn` change is a single line in a section Electron doesn't touch. Rebasing
should be conflict-free or nearly so.

If a conflict does arise, we resolve it during the rebase. This is the same
workflow any fork uses to stay current with upstream.

## Staying in Sync with Electron

When Electron bumps its Chromium version (e.g., from 146.0.7650.0 to
147.0.xxxx.0):

```bash
cd ts4/termsurf-chromium/src

# 1. Fetch the new vanilla Chromium version
git fetch upstream
git checkout <new-version>

# 2. Create the new Electron branch and apply patches
git checkout -b <new-version>-electron
while IFS= read -r patch; do
  git am --3way "../../electron/patches/chromium/$patch"
done < ../../electron/patches/chromium/.patches

# 3. Create the new TermSurf branch and rebase our commits
git checkout -b <new-version>-termsurf
git cherry-pick <old-version>-electron..<old-version>-termsurf

# 4. Push both branches upstream
git push origin <new-version>-electron <new-version>-termsurf

# 5. Rebuild and test
autoninja -C out/Default content/two_profiles:two_profiles
```

This keeps us on a well-tested Chromium version with well-tested patches. We
never independently track Chromium releases — we follow Electron's lead.

## Implementation Plan

### Phase 1: Clean slate

Delete the Two Profiles app from the fork. It was built before the decision to
use the Electron patch set and will be rewritten from scratch using the new
APIs. This returns the fork to a clean vanilla Chromium state.

- [x] Delete `content/two_profiles/` directory
- [x] Revert the `//content/two_profiles` line in `BUILD.gn`
- [x] Commit the deletion

### Phase 2: Match Chromium version

Electron targets Chromium 146.0.7650.0. Our fork was created with
`fetch chromium`, which pulled whatever was HEAD at that time. We need to check
out the correct version before applying patches.

#### Repository topology

```
~/dev/termsurf-chromium/src/          ← "upstream" (full history)
  origin:   git@github.com:termsurf/termsurf-chromium.git
  upstream: https://chromium.googlesource.com/chromium/src.git

ts4/termsurf-chromium/src/            ← submodule (shallow clone)
  origin:   ~/dev/termsurf-chromium/src
```

The full-history repo at `~/dev/termsurf-chromium/src/` is the source of truth.
It will eventually live only on GitHub, but for now it lives locally. The
submodule is a shallow clone that fetches from it. All work (version checkout,
patch application, TermSurf commits) happens in the submodule, then gets pushed
back to the full-history repo. This simulates the future GitHub-based flow.

#### Workflow

1. **Check out the target version upstream:**

   ```bash
   cd ~/dev/termsurf-chromium/src
   git checkout 146.0.7650.0
   ```

2. **Fetch and check out in the submodule:**

   ```bash
   cd ts4/termsurf-chromium/src
   git fetch origin
   git checkout 146.0.7650.0
   ```

3. **Verify the version matches:**

   ```bash
   git log --oneline -1
   git tag -l '146.0.7650.0'   # tag should exist and match HEAD
   ```

The `-electron` and `-termsurf` branches are created in later phases. After all
work is done, push from the submodule back upstream:

```bash
cd ts4/termsurf-chromium/src
git push origin 146.0.7650.0-electron 146.0.7650.0-termsurf
```

- [x] Upstream repo checked out at 146.0.7650.0
- [x] Submodule fetched and on the same commit
- [x] Version verified — both at `8a0719fc70ef6`

### Phase 3: Apply the Electron patch set

Create the `-electron` branch and apply all 147 patches in order:

```bash
cd ts4/termsurf-chromium/src
git checkout -b 146.0.7650.0-electron

while IFS= read -r patch; do
  git am --3way "../../electron/patches/chromium/$patch" || {
    echo "FAILED: $patch"
    break
  }
done < ../../electron/patches/chromium/.patches
```

If a patch fails to apply, our Chromium version doesn't match Electron's. Fix by
checking out the correct version in Phase 2.

After all patches apply, push the branch upstream:

```bash
git push origin 146.0.7650.0-electron
```

- [x] `146.0.7650.0-electron` branch created
- [x] All 147 patches applied cleanly
- [x] Branch pushed upstream

### Phase 4: Verify Content Shell (baseline)

Content Shell is our baseline — it must still build and run at 60fps after the
Electron patches. If Content Shell breaks, the patches are the problem, not our
code.

```bash
gn gen out/Default --args='is_debug=false symbol_level=0 is_component_build=true'
autoninja -C out/Default content_shell
```

Launch Content Shell with the test page:

```bash
cd /Users/ryan/dev/termsurf/ts4/box-demo && bun run server.ts &
./out/Default/Content\ Shell.app/Contents/MacOS/Content\ Shell http://localhost:9407
```

Verify: spinning blue square at 60fps, localStorage string persists across
restarts. This confirms the Electron patches don't break vanilla Chromium
windowed rendering.

- [ ] Content Shell builds successfully
- [ ] Content Shell renders test page at 60fps

#### Conclusion

Phase 4 failed. The 147 Chromium patches cannot be built in isolation — they
are not standalone modifications to Chromium. They introduce hard dependencies
on Electron's build infrastructure (`//electron/build/config`,
`//third_party/electron_node/`, and others) that require the full Electron
source tree and its dependencies to be present. See the document conclusion for
full details.

### Phase 5: Rewrite Two Profiles

Create the `-termsurf` branch from the `-electron` branch and rebuild the Two
Profiles app from scratch, using the APIs that the Electron patches provide:

```bash
cd ts4/termsurf-chromium/src
git checkout -b 146.0.7650.0-termsurf 146.0.7650.0-electron
```

- Create `content/two_profiles/` with the same macOS bundle structure as before
- Use the three-layer throttling bypass on each WebContents:
  ```cpp
  rwh_impl->disable_hidden_ = true;                                    // Layer 1
  web_contents->GetRenderViewHost()->SetSchedulerThrottling(false);    // Layer 2
  // Layer 3 handled by compositor patch automatically
  ```
- Consider using Chromium's `views` framework (`views::Widget` +
  `views::WebView`) for view composition instead of raw NSView manipulation
- Register the target in `BUILD.gn`
- Update the parent repo's submodule to point to `146.0.7650.0-termsurf`

- [ ] `146.0.7650.0-termsurf` branch created
- [ ] Two Profiles app created with throttling bypass
- [ ] App builds successfully
- [ ] Branch pushed upstream

### Phase 6: Verify Two Profiles at 60fps

Launch the Two Profiles app with the test server. Both panes should render the
spinning blue square at 60fps with different localStorage identity strings.

```bash
autoninja -C out/Default content/two_profiles:two_profiles
./out/Default/Two\ Profiles.app/Contents/MacOS/Two\ Profiles
```

- [ ] Both panes render at 60fps
- [ ] Different localStorage strings (profile isolation works)
- [ ] Strings persist across app restarts

## Conclusion

### What happened

Phases 1–3 succeeded: we deleted the old Two Profiles app, pinned both repos to
Chromium 146.0.7650.0, applied all 147 Electron patches cleanly, created the
`146.0.7650.0-electron` branch, and pushed it upstream. The patches applied
without a single conflict, confirming our version match.

Phase 4 failed at the build step. Content Shell could not be compiled because
the 147 Chromium patches are not standalone — they are one piece of a much
larger build system.

### Why the full Electron patch set cannot be built in isolation

Electron's Chromium patches assume the presence of the full Electron build
environment. Specifically:

1. **Node.js dependency.** The patched `.gn` file (Chromium's root build
   config) references 21 files under `//third_party/electron_node/` — Node.js
   v24.13.0 and its 17 sub-dependencies (ada, brotli, cares, llhttp, nghttp2,
   openssl, uv, etc.). These files must exist for GN to generate build files.
   TermSurf has no use for Node.js.

2. **`is_electron_build` flag.** The patches expect the GN arg
   `is_electron_build=true`, which triggers `root_extra_deps=["//electron"]`.
   This pulls in Electron's full BUILD.gn, which imports
   `//third_party/electron_node/node.gni` — back to the Node.js dependency.
   Without this flag, the build hits missing framework links (e.g.,
   `CFRelease` undefined in `libcomponents_os_crypt_sync.dylib`) because
   Electron's build infrastructure handles macOS framework linking that vanilla
   Chromium doesn't need.

3. **232 patches, not 147.** The 147 Chromium patches are just one of 13 patch
   targets. Electron also patches boringssl (2), devtools (1), ffmpeg (1),
   v8 (1), node (48), nan (10), perfetto (1), webrtc (1), sqlite (1),
   squirrel.mac (12), ReactiveObjC (2), and reclient-configs (5). Some of
   the Chromium patches assume these other patches have been applied.

4. **Additional third-party checkouts.** Beyond `electron_node`, Electron's
   DEPS file fetches `third_party/nan/`, `third_party/squirrel.mac/`,
   `third_party/engflow-reclient-configs/`, and specific versions of
   boringssl, devtools-frontend, and others. These are managed by
   `gclient sync` reading Electron's DEPS, not Chromium's.

5. **The `generate_mas_config` dependency.** Dozens of Chromium BUILD.gn files
   were patched to depend on `//electron/build/config:generate_mas_config`,
   which generates a Mac App Store header. This target requires the Electron
   source tree and its `script/generate-mas-config.py`. We worked around this
   with a symlink, but it's symptomatic of the deeper coupling.

In short: the "just apply the patch set" approach assumed the patches were
self-contained modifications to Chromium. They are not. They are one component
of a tightly integrated build system that requires Electron, Node.js, and
several other dependencies to function. The simplicity we hoped for does not
exist at this level.

### What we learned

- Electron's patches apply cleanly to the correct Chromium version. The
  version-matching and branch workflow (Phases 1–3) is sound and reusable.
- The `146.0.7650.0-electron` branch is a valid artifact — it contains all
  147 patches as commits and can be used as a reference for reading and
  extracting specific patches.
- The three throttling patches identified in Issue 408 are still the right
  solution. The problem is not the patches themselves but the 144 other
  patches that come bundled with them.

### Paths forward

#### Option A: Build a TermSurf-specific patch set (recommended)

Instead of adopting all 147 Electron patches, build a minimal patch set
containing only the patches TermSurf actually needs. Start with the three
throttling patches from Issue 408:

1. `disable_hidden.patch` — adds `disable_hidden_` flag to
   `RenderWidgetHostImpl` (Layer 1)
2. `allow_disabling_blink_scheduler_throttling_per_renderview.patch` — adds
   `SetSchedulerThrottling()` to `WebViewImpl` (Layer 2)
3. `fix_disabling_background_throttling_in_compositor.patch` — bypasses
   `ui::Compositor::SetVisible()` check (Layer 3)

These three patches have no dependency on Node.js, `is_electron_build`, or
any Electron build infrastructure. They modify core Chromium rendering code
and should apply and build cleanly on vanilla Chromium.

The `146.0.7650.0-electron` branch serves as the reference — we can read
each patch, understand what it changes, and extract only what we need. Over
time, additional patches can be adopted individually as the need arises.

#### Option B: Use Electron's full build system

Set up the complete Electron build environment using `e init` and `e sync`
from `@electron/build-tools`. This would fetch `electron_node`, all
third-party dependencies, apply all 232 patches, and build with
`is_electron_build=true`. This gives us everything Electron has, but at the
cost of pulling in Node.js and dozens of dependencies TermSurf doesn't need.
The build tree would be ~60GB and tightly coupled to Electron's release cycle.

#### Option C: Stub out the Electron dependencies

Create minimal stub files for the missing dependencies
(`third_party/electron_node/*.gni`, `electron/build/config/BUILD.gn`, etc.)
so the build can proceed with `is_electron_build=false`. This is fragile —
each Electron version update could introduce new dependencies that need new
stubs. Not recommended for ongoing maintenance.

### Next step

Option A — create Issue 410 to build a minimal TermSurf patch set starting
with the three throttling patches. Use the `146.0.7650.0-electron` branch as
a reference. Return the submodule to vanilla Chromium 146.0.7650.0 and apply
only TermSurf's own patches on top.
