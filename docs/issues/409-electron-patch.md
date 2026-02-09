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

## Fork Structure

Our `termsurf-chromium` submodule (at `ts4/termsurf-chromium/src/`) is a
Chromium fork with a linear commit history:

```
Chromium 146.0.7650.0 (vanilla)
  + Electron's 147 patches (applied as commits)
  ← electron-base tag (marks where Electron patches end)
  + TermSurf's commits (content/two_profiles/, BUILD.gn, etc.)
  ← main branch HEAD
```

Electron does not maintain a Chromium fork — it applies patches at build time.
We take a different approach: the patches become permanent commits in our fork.
TermSurf's own modifications are regular commits on top.

### The `electron-base` tag

A tag called `electron-base` marks the boundary between Electron's patches and
TermSurf's commits. This serves two purposes:

1. **Visibility.** `git log electron-base..HEAD` shows exactly what TermSurf
   has changed on top of Electron's patches.
2. **Rebase target.** When the Electron patch set updates, TermSurf's commits
   are rebased onto the new `electron-base`.

### Rebase workflow

TermSurf's modifications are maintained as regular commits — not a separate
patch set. When Electron updates (new Chromium version or new patches), we
rebase our commits on top:

```
1. Check out vanilla Chromium at Electron's new version
2. Apply the updated Electron patch set → new electron-base
3. Rebase TermSurf's commits onto the new electron-base
4. Rebuild and test
```

This works cleanly because TermSurf's changes don't overlap with Electron's
patches — our files (`content/two_profiles/`) are entirely new, and our
`BUILD.gn` change is a single line in a section Electron doesn't touch.
Rebasing should be conflict-free or nearly so.

If a conflict does arise, we resolve it during the rebase. This is the same
workflow any fork uses to stay current with upstream.

## Steps

### Step 1: Verify Chromium version

Check what version our fork is currently on and whether it matches Electron's
target (146.0.7650.0). If not, check out the matching version first.

```bash
cd ts4/termsurf-chromium/src
git log --oneline -1  # Check current HEAD
```

Electron's DEPS file pins `chromium_version: '146.0.7650.0'`. Our fork must be
on the same version for the patches to apply cleanly.

### Step 2: Create a branch for TermSurf's commits

Save our existing modifications (from Issue 407) so they can be rebased later:

```bash
# Tag the current state so we can cherry-pick/rebase later
git tag termsurf-pre-electron HEAD

# Reset to vanilla Chromium at the correct version
git checkout <chromium-146.0.7650.0-commit>
git checkout -b electron-patches
```

### Step 3: Apply the patch set

The patches are at `electron/patches/chromium/` and the ordered list is in
`electron/patches/chromium/.patches`. Apply them in order:

```bash
cd ts4/termsurf-chromium/src

while IFS= read -r patch; do
  git am --3way "../../electron/patches/chromium/$patch" || {
    echo "FAILED: $patch"
    break
  }
done < ../../electron/patches/chromium/.patches
```

If a patch fails, our Chromium version doesn't match Electron's. Fix by
checking out the correct version.

### Step 4: Tag the electron-base

```bash
git tag electron-base HEAD
```

### Step 5: Rebase TermSurf's commits on top

```bash
# Rebase our modifications onto the electron-base
git rebase --onto electron-base <vanilla-chromium-commit> termsurf-pre-electron

# Fast-forward main to the result
git checkout main
git reset --hard HEAD  # now at TermSurf commits on top of electron-base
```

Or more simply, since our changes don't overlap:

```bash
git cherry-pick termsurf-pre-electron
```

### Step 6: Rebuild and test

```bash
gn gen out/Default --args='is_debug=false symbol_level=0 is_component_build=true'
autoninja -C out/Default content/two_profiles:two_profiles
```

### Step 7: Wire up the throttling bypass

With the Electron patches applied, the three throttling bypass APIs are now
available. Modify `two_profiles_main_parts.mm` to use them:

```cpp
// After creating each WebContents:
auto* rwh = RenderWidgetHostImpl::From(
    web_contents->GetRenderWidgetHostView()->GetRenderWidgetHost());
rwh->disable_hidden_ = true;  // Layer 1: prevent WasHidden()

// Layer 2: disable Blink scheduler throttling
web_contents->GetRenderViewHost()->SetSchedulerThrottling(false);

// Layer 3 is handled by the compositor patch automatically
```

### Step 8: Verify 60fps

Launch the Two Profiles app with the Bun test server running. Both panes should
now render the spinning blue square at 60fps with different localStorage
identity strings.

## Success Criteria

- All 147 Electron patches apply cleanly to our Chromium fork.
- `electron-base` tag marks the boundary.
- TermSurf's commits rebase cleanly on top.
- The Two Profiles app builds and runs.
- Both panes render at 60fps (up from 2-3fps).
- Profile isolation still works (different localStorage strings).
- content_shell still builds and runs independently.

## Future: Staying in Sync with Electron

When Electron bumps its Chromium version:

```bash
# 1. Fetch the new vanilla Chromium version
git fetch upstream
git checkout <new-chromium-version>

# 2. Apply the updated Electron patch set
while IFS= read -r patch; do
  git am --3way "../../electron/patches/chromium/$patch"
done < ../../electron/patches/chromium/.patches

# 3. Move the electron-base tag
git tag -f electron-base HEAD

# 4. Rebase TermSurf's commits
git rebase electron-base main

# 5. Rebuild and test
autoninja -C out/Default content/two_profiles:two_profiles
```

This keeps us on a well-tested Chromium version with well-tested patches. We
never independently track Chromium releases — we follow Electron's lead.

## Relationship to Other Issues

| Issue | Relationship                                                      |
| ----- | ----------------------------------------------------------------- |
| 407   | Proved multi-profile works; identified 2-3fps throttling          |
| 408   | Traced throttling to three layers; discovered Electron's solution |
| 409   | This issue — applies the patch set to our fork                    |
