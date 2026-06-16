# Experiment 2: Build the Pristine Ghostty Import

## Description

Build the freshly imported upstream Ghostty `v1.3.1` tree before making any
Ghostboard-specific branding, config-path, icon, CLI, build-system, or protocol
changes. The goal is to prove the imported baseline builds in this VM and to
record any toolchain or source-build blocker before we start modifying the tree.

This experiment deliberately does not rename the app, change user-facing text,
change the config path, copy the Wezboard icon, implement TermSurf protocol
messages, launch the app, or modify `webtui` or `roamium`.

## Changes

No source changes are planned for this experiment. The only repository changes
should be this experiment's result documentation after the build attempt.

The build flow follows the imported Ghostty macOS instructions:

1. Verify the build tools are available:

   ```bash
   command -v zig
   zig version
   command -v nu
   xcode-select -p
   xcodebuild -version
   ```

2. Verify the working tree is clean before building:

   ```bash
   git status --short
   ```

3. Build the underlying library from the Ghostty subtree root:

   ```bash
   cd ghostboard
   zig build -Demit-macos-app=false
   ```

4. Build the macOS app using Ghostty's wrapper script, as required by
   `ghostboard/macos/AGENTS.md`:

   ```bash
   cd ghostboard
   macos/build.nu --configuration Debug --action build
   ```

5. Record the result in this experiment file.

Do not edit files under `ghostboard/` in this experiment. If the build fails,
record the error and conclusion without changing source.

## Verification

Pass criteria:

- `command -v zig` succeeds.
- `zig version` satisfies the imported Ghostty minimum Zig version in
  `ghostboard/build.zig.zon`.
- `command -v nu` succeeds because `ghostboard/macos/build.nu` requires Nushell.
- `xcode-select -p` points at full Xcode, not only Command Line Tools.
- `xcodebuild -version` reports the selected Xcode version.
- `git status --short` is clean before building.
- `cd ghostboard && zig build -Demit-macos-app=false` succeeds.
- `cd ghostboard && macos/build.nu --configuration Debug --action build`
  succeeds.
- The debug app bundle exists at `ghostboard/macos/build/Debug/Ghostty.app`.
- Any generated build output is ignored or otherwise does not leave source
  changes in `git status --short`.
- The issue README lists this experiment as `Pass` only after the result is
  recorded.

Fail criteria:

- Required tooling is missing.
- Xcode selection is invalid for building Ghostty.
- The underlying `zig build -Demit-macos-app=false` fails.
- The macOS `build.nu` app build fails.
- The build modifies tracked source files.
- The experiment makes branding, config-path, icon, CLI, build-system, protocol,
  `webtui`, or `roamium` changes.

## Notes

If the build fails for environmental reasons, the result should be `Fail` or
`Partial` with the exact blocker and the next experiment should address that
blocker. Do not silently turn a build failure into a source edit inside this
experiment.

## Design Review

Fresh-context adversarial review returned `APPROVED`.

- Optional: the original design required `command -v zig` to report a
  Homebrew-installed executable, but imported Ghostty only requires a compatible
  Zig version. Fixed by adding `zig version` to the tool checks and changing the
  pass criterion to require a version satisfying `ghostboard/build.zig.zon`.
- The reviewer confirmed the README links Experiment 2 as `Designed`, the
  experiment has the required sections, the scope is a clean build baseline, the
  macOS commands match `ghostboard/macos/AGENTS.md`, and the expected app output
  path matches `ghostboard/macos/build/Debug/Ghostty.app`.
- No required findings were reported.

## Result

**Result:** Partial

Tooling and preflight checks passed:

- `command -v zig` returned `/opt/homebrew/bin/zig`.
- `zig version` returned `0.15.2`, matching `ghostboard/build.zig.zon`'s
  `.minimum_zig_version = "0.15.2"`.
- `command -v nu` returned `/opt/homebrew/bin/nu`.
- `xcode-select -p` returned `/Applications/Xcode.app/Contents/Developer`.
- `xcodebuild -version` returned:

  ```text
  Xcode 26.6
  Build version 17F109
  ```

- `git status --short` was clean before building.

The underlying Ghostty library build succeeded:

```bash
cd ghostboard
zig build -Demit-macos-app=false
```

The build emitted `libtool` alignment warnings from archive members, but exited
with status `0`.

The macOS Debug app build failed:

```bash
cd ghostboard
macos/build.nu --configuration Debug --action build
```

The failure was captured in:

```text
logs/ghostboard-exp2-macos-build.log
```

The log shows the link step for `ghostty.debug.dylib` failed:

```text
Ld /Users/astrohacker/dev/termsurf/ghostboard/macos/build/Debug/Ghostty.app/Contents/MacOS/ghostty.debug.dylib normal (in target 'Ghostty' from project 'Ghostty')
Undefined symbols for architecture arm64:
ld: symbol(s) not found for architecture arm64
/Users/astrohacker/dev/termsurf/ghostboard/macos/Ghostty.xcodeproj: Ghostty: clang: error: linker command failed with exit code 1 (use -v to see invocation)
** BUILD FAILED **
```

Representative unresolved symbol groups in the log include:

- `spirv_cross::CompilerMSL::*` and `spirv_cross::CompilerGLSL::*`;
- `glslang::*`;
- `ImGuiStorage::GetInt`;
- `spv::SpvBuildLogger::getAllMessages`;
- `_libintl_*`;
- `_sentry_*`;
- `_ghostty_simd_*`;
- `___extenddftf2` and `___extendxftf2`.

`ghostboard/macos/build/Debug/Ghostty.app` was created, but it is incomplete:

- `ghostboard/macos/build/Debug/Ghostty.app/Contents/MacOS/ghostty` is missing.
- `ghostboard/macos/build/Debug/Ghostty.app/Contents/MacOS/ghostty.debug.dylib`
  is missing.

Generated build outputs are ignored. `git status --short` remained clean after
the build attempts. `git status --ignored --short ghostboard logs` showed only
ignored generated outputs:

```text
!! ghostboard/.zig-cache/
!! ghostboard/macos/GhosttyKit.xcframework/
!! ghostboard/macos/build/
!! ghostboard/zig-out/
!! logs/
```

No branding, config-path, icon, CLI, build-system, protocol, `webtui`, or
`roamium` changes were made in this experiment.

## Completion Review

Fresh-context adversarial completion review returned `APPROVED`.

- The reviewer confirmed `git status --short` showed only the two issue
  documentation edits.
- The reviewer confirmed the README marks Experiment 2 as `Partial`.
- The reviewer confirmed this experiment records the partial result, successful
  preflight and library build, failed macOS app link, incomplete app bundle,
  ignored generated outputs, and no source changes.
- The reviewer confirmed the build log contains the `ghostty.debug.dylib` link
  failure, undefined symbols, and `** BUILD FAILED **`.
- The reviewer confirmed both `Contents/MacOS/ghostty` and `ghostty.debug.dylib`
  are missing from the generated app bundle.
- The reviewer confirmed `git status --ignored --short ghostboard logs` shows
  only ignored generated outputs.
- No required findings were reported.

## Conclusion

The pristine import can build the underlying Ghostty library, but the macOS app
does not yet link in this environment. The next experiment should investigate
and fix the Debug app link failure, focusing on why the Xcode target links
`libghostty.a` without resolving the required Spirv-Cross, glslang, libintl,
Sentry, SIMD, and compiler-rt symbols.
