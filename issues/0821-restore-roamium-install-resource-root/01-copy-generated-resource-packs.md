# Experiment 1: Copy Generated Resource Packs

## Description

Restore the installed Roamium resource root so production Roamium can load the
generated Chromium resource packs introduced by the inline PDF work.

The current installed binary runs from:

```text
/opt/homebrew/opt/termsurf-roamium/roamium
```

That direct binary path is correct, but the install directory is incomplete.
`LoadTsPdfResourceBundle()` loads generated packs relative to Chromium
`DIR_ASSETS`, and the production install root does not contain the required
`gen/...` files. Roamium therefore logs `found=0` for the generated packs and
crashes while initializing extension/PDF resources.

This experiment keeps the known-good direct resource-root layout from Issue 730
and fixes both production packaging paths so the Roamium resource root preserves
the generated pack paths it now requires:

- `scripts/install.sh`, which installs directly to
  `/opt/homebrew/opt/termsurf-roamium`;
- `scripts/release.sh`, which stages `dist/release/roamium` for the Homebrew
  cask artifact that is later installed to `/opt/homebrew/opt/termsurf-roamium`.

## Changes

1. Update `scripts/install.sh`.

   Add an explicit required-resource copy path inside `install_roamium()` for
   the generated packs that `LoadTsPdfResourceBundle()` loads:

   ```text
   gen/chrome/pdf_resources.pak
   gen/chrome/generated_resources_en-US.pak
   gen/chrome/common_resources.pak
   gen/components/components_resources.pak
   gen/components/strings/components_strings_en-US.pak
   gen/extensions/extensions_renderer_resources.pak
   ```

2. Update `scripts/release.sh`.

   Add the same required-resource copy path when staging `dist/release/roamium`.
   The Homebrew cask installs that staged `roamium` directory to
   `/opt/homebrew/opt/termsurf-roamium`, so release packaging must preserve the
   same relative `gen/...` paths as direct install.

3. Preserve exact relative paths under each Roamium resource root.

   Each source file under `chromium/src/out/Default/` must land at the same
   relative path under both Roamium resource roots, for example:

   ```text
   chromium/src/out/Default/gen/chrome/pdf_resources.pak
   -> /opt/homebrew/opt/termsurf-roamium/gen/chrome/pdf_resources.pak

   chromium/src/out/Default/gen/chrome/pdf_resources.pak
   -> dist/release/roamium/gen/chrome/pdf_resources.pak
   ```

4. Fail loudly on missing required packs.

   The scripts must not silently ship a partial Roamium resource root. If any
   required generated pack is missing from `chromium/src/out/Default`, both
   direct install and release packaging should fail with a clear error naming
   the missing file and telling the user to rebuild Chromium/Roamium.

5. Keep the rest of the install/release behavior unchanged.

   Do not add a symlink or wrapper. Do not change Ghostboard or webtui launch
   resolution. Do not modify Chromium code for this packaging regression. Do not
   publish a real release during this experiment.

## Verification

1. Static shell syntax:

   ```bash
   bash -n scripts/install.sh scripts/release.sh
   ```

2. Confirm required source packs exist:

   ```bash
   for path in \
     gen/chrome/pdf_resources.pak \
     gen/chrome/generated_resources_en-US.pak \
     gen/chrome/common_resources.pak \
     gen/components/components_resources.pak \
     gen/components/strings/components_strings_en-US.pak \
     gen/extensions/extensions_renderer_resources.pak
   do
     test -f "chromium/src/out/Default/$path"
   done
   ```

3. Reinstall Roamium:

   ```bash
   ./scripts/install.sh roamium
   ```

4. Confirm the installed resource root contains the required generated packs:

   ```bash
   for path in \
     gen/chrome/pdf_resources.pak \
     gen/chrome/generated_resources_en-US.pak \
     gen/chrome/common_resources.pak \
     gen/components/components_resources.pak \
     gen/components/strings/components_strings_en-US.pak \
     gen/extensions/extensions_renderer_resources.pak
   do
     test -f "/opt/homebrew/opt/termsurf-roamium/$path"
   done
   ```

5. Confirm release staging contains the required generated packs without
   publishing a release.

   Use local-only release staging or a testable equivalent that exercises the
   same Roamium packaging copy path without running the GitHub/Homebrew publish
   steps, then confirm:

   ```bash
   for path in \
     gen/chrome/pdf_resources.pak \
     gen/chrome/generated_resources_en-US.pak \
     gen/chrome/common_resources.pak \
     gen/components/components_resources.pak \
     gen/components/strings/components_strings_en-US.pak \
     gen/extensions/extensions_renderer_resources.pak
   do
     test -f "dist/release/roamium/$path"
   done
   ```

   If the implementation does not introduce a local-only release staging mode,
   record why and verify the release packaging copy helper directly against a
   temporary staging directory instead. The verification must still prove that
   the Homebrew cask artifact path gets the six generated packs at
   `roamium/gen/...`.

6. Confirm installed Roamium can initialize its resource packs:

   ```bash
   /opt/homebrew/opt/termsurf-roamium/roamium --help
   ```

   Pass criteria:

   - no `found=0` log lines for the six generated packs;
   - no `Unable to find resource` fatal;
   - no `icudtl.dat not found` fatal.

7. Confirm production browser startup:

   Launch TermSurf Ghostboard from the installed app, then run:

   ```bash
   /usr/local/bin/web \
     --browser /opt/homebrew/opt/termsurf-roamium/roamium \
     https://example.com
   ```

   Pass criteria:

   - Roamium connects back to Ghostboard;
   - the TUI leaves the `Waiting for Chromium` state;
   - the page reaches a visible loaded state.

8. Hygiene checks:

   ```bash
   git diff --check
   ```

## Design Review

Fresh-context adversarial review returned **CHANGES REQUIRED**.

Required findings:

- The original design fixed only `scripts/install.sh`, but `scripts/release.sh`
  stages the Homebrew `roamium` artifact and had the same missing-resource
  behavior.
- The original verification checked only direct install, not the release/cask
  artifact path users receive.

Fixes applied:

- Expanded the design to update both `scripts/install.sh` and
  `scripts/release.sh`.
- Added release-staging verification for the required
  `dist/release/roamium/gen/...` files.
- Added syntax checks for both touched shell scripts and `git diff --check`.

Re-review verdict: **APPROVED**. The reviewer confirmed the revised design
covers both production packaging paths, verifies the Homebrew cask staging
artifact, and includes syntax/whitespace hygiene checks. No Required findings
remain.
