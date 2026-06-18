+++
status = "open"
opened = "2026-06-18"
+++

# Issue 821: Restore Roamium Install Resource Root

## Goal

Fix production Roamium installation so the installed binary runs from a complete
Chromium resource root and Ghostboard can launch web pages without hanging at
browser startup.

## Background

Production `web` currently starts Ghostboard and asks it to launch installed
Roamium from:

```text
/opt/homebrew/opt/termsurf-roamium/roamium
```

The binary starts, but Roamium cannot load required Chromium generated resource
packs from the install root. A direct `--help` run shows the installed binary
looking for files such as:

```text
/opt/homebrew/opt/termsurf-roamium/gen/chrome/pdf_resources.pak
/opt/homebrew/opt/termsurf-roamium/gen/components/components_resources.pak
/opt/homebrew/opt/termsurf-roamium/gen/components/strings/components_strings_en-US.pak
/opt/homebrew/opt/termsurf-roamium/gen/chrome/generated_resources_en-US.pak
/opt/homebrew/opt/termsurf-roamium/gen/chrome/common_resources.pak
/opt/homebrew/opt/termsurf-roamium/gen/extensions/extensions_renderer_resources.pak
```

Those files are missing from the installed layout, so Roamium crashes during
resource initialization and the TUI remains stuck waiting for Chromium.

This is a regression in the install packaging. We already solved this class of
problem in earlier issues:

- Issue 730 established that Roamium must be installed as a flat resource-root
  directory and launched by direct binary path. Symlinks and wrapper scripts are
  not acceptable because macOS/Chromium resource lookup resolves the wrong
  bundle/resource directory.
- Issue 707 established the development equivalent: copy the Cargo-built
  `roamium` binary into `chromium/src/out/Default/roamium` because that
  directory contains the Chromium runtime resources.
- Issue 808 Experiment 29 confirmed that launching `target/debug/roamium`
  directly fails with missing Chromium resources, and the correct runtime path
  is `chromium/src/out/Default/roamium`.
- Issue 792 added inline PDF support and introduced generated resource pack
  requirements under `gen/chrome`, `gen/components`, `gen/components/strings`,
  and `gen/extensions`.

## Analysis

The current `scripts/install.sh` installs Roamium to
`/opt/homebrew/opt/termsurf-roamium`, but it only copies top-level Chromium
assets:

```text
*.dylib
*.pak
icudtl.dat
v8_context_snapshot*.bin
```

That was enough for the older standalone install layout, but it is no longer
complete after the PDF resource work. The install script must preserve the
relative paths for the generated resource packs that Roamium loads from its
runtime asset directory.

The expected installed layout must include at least:

```text
/opt/homebrew/opt/termsurf-roamium/roamium
/opt/homebrew/opt/termsurf-roamium/*.dylib
/opt/homebrew/opt/termsurf-roamium/*.pak
/opt/homebrew/opt/termsurf-roamium/icudtl.dat
/opt/homebrew/opt/termsurf-roamium/v8_context_snapshot*.bin
/opt/homebrew/opt/termsurf-roamium/gen/chrome/*.pak
/opt/homebrew/opt/termsurf-roamium/gen/components/*.pak
/opt/homebrew/opt/termsurf-roamium/gen/components/strings/*.pak
/opt/homebrew/opt/termsurf-roamium/gen/extensions/*.pak
```

The fix should be durable: either copy the precise required generated packs or
copy the narrow generated resource subtrees that Roamium actually references.
The verification must prove the installed binary can start from the production
install directory and that `web` can load a page through Ghostboard using the
installed Roamium path.

## Constraints

- Do not make Roamium user-facing through a symlink or wrapper.
- Do not run Roamium from `target/debug` or `target/release` for production
  verification.
- Preserve direct launch from the installed resource-root binary:
  `/opt/homebrew/opt/termsurf-roamium/roamium`.
- Keep the fix scoped to install/release packaging unless an experiment proves a
  deeper runtime issue.

## Experiments

- [Experiment 1: Copy Generated Resource Packs](01-copy-generated-resource-packs.md)
  — **Designed**
