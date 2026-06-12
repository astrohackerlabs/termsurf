# Experiment 147: Phase I — shell-integration setup

## Description

Port upstream `termio/shell_integration.zig` into Roastty and wire automatic
shell-integration setup into PTY startup.

Roastty already parses `shell-integration` and `shell-integration-features`, and
its terminal already understands the OSC 133 semantic prompt sequences that the
shell integration scripts emit. The missing Phase I piece is the startup
injection layer: when Roastty launches a supported interactive shell, it should
prepare the command and environment so the copied/renamed integration scripts
load automatically.

This experiment is bounded to setup and packaging:

- detect/force supported shells (`bash`, `elvish`, `fish`, `nu`, `zsh`);
- write the `ROASTTY_SHELL_FEATURES` environment variable from finalized config;
- rewrite shell commands and environment variables following upstream behavior;
- copy the pinned upstream `src/shell-integration` resource tree into Roastty
  with mechanical `ghostty`/`Ghostty`/`GHOSTTY` renames;
- make the macOS bundle and test environment able to find those resources;
- prove the setup by unit tests and a small startup/integration test.

Out of scope:

- implementing runtime behaviors that depend on shell integration after the OSC
  sequences arrive, such as command-finished notifications or prompt-based close
  confirmation refinements;
- SSH helper subcommands and terminfo installation behavior beyond preserving
  the feature flags/scripts this setup exposes;
- changing the copied macOS app logic.

## Changes

- `roastty/src/termio/shell_integration.rs`
  - Add a Rust port of upstream `termio/shell_integration.zig`'s setup layer.
  - Define a crate-private `Shell` enum and setup result type.
  - Detect shells from the command executable basename, including the upstream
    macOS `/bin/bash` exclusion.
  - Map `config::ShellIntegration::{Detect,None,Bash,Elvish,Fish,Nushell,Zsh}`
    to detection/force behavior.
  - Implement `setup_features` using Roastty names: `ROASTTY_SHELL_FEATURES`,
    sorted exactly like upstream `cursor,path,ssh-env,ssh-terminfo,sudo,title`,
    with `cursor:blink` or `cursor:steady` based on the finalized cursor blink
    setting.
  - Implement upstream's setup rules with renamed env vars and resource paths:
    - bash: start in POSIX mode, reject `--posix` and `-c`, preserve `ENV` in
      `ROASTTY_BASH_ENV`, set `ENV` to
      `<resources>/shell-integration/bash/roastty.bash`, set
      `ROASTTY_BASH_INJECT`, optional `ROASTTY_BASH_RCFILE`, and
      `ROASTTY_BASH_UNEXPORT_HISTFILE` when it supplies `.bash_history`;
    - zsh: set `ZDOTDIR` to `<resources>/shell-integration/zsh`, preserve the
      old value in `ROASTTY_ZSH_ZDOTDIR`;
    - fish/elvish: prepend `<resources>/shell-integration` to `XDG_DATA_DIRS`,
      set `ROASTTY_SHELL_INTEGRATION_XDG_DIR`, and preserve the freedesktop
      default fallback when `XDG_DATA_DIRS` is unset;
    - nushell: do the XDG setup and add `--execute 'use roastty *'`, rejecting
      `--command`, `--lsp`, and `-c` forms just like upstream.
  - Treat missing resource files/directories as non-fatal setup failure: startup
    continues with the original command/environment, matching upstream's "return
    null" behavior for failed automatic integration.
- `roastty/src/termio.rs`
  - Add shell-integration fields to `TermioSpawnOptions`: resource directory,
    integration mode, enabled features, and cursor blink.
  - Before spawning every PTY child, set `ROASTTY_SHELL_FEATURES` from finalized
    config features even when `shell-integration = none`. Upstream sets the
    feature environment variable before the automatic-integration gate so manual
    shell integration can still read it; Roastty must preserve that behavior.
  - After feature setup, call the automatic command/env rewrite helper only when
    `shell-integration != none`; use the rewritten command/env on success and
    the original command/env on non-fatal setup miss.
  - Preserve direct/env/cwd behavior already covered by `Termio` tests.
- `roastty/src/lib.rs`
  - Thread the finalized app config's shell-integration fields and
    `os::resources_dir::resources_dir().host()` into the `start_termio` spawn
    options.
  - Keep explicit surface command startup behavior intact; default shell startup
    is the primary automatic-integration path for this slice.
- `roastty/src/os/resources_dir.rs`
  - Add only minimal test/support helpers if needed to make resource lookup
    deterministic in unit and hosted tests.
- `roastty/resources/shell-integration/...`
  - Copy the pinned upstream `vendor/ghostty/src/shell-integration` files and
    mechanically rename file names/content from Ghostty to Roastty:
    `ghostty.bash` -> `roastty.bash`, `ghostty-integration` references to
    `roastty-integration`, `ghostty.nu` module references to `roastty.nu`, and
    env vars from `GHOSTTY_*` to `ROASTTY_*`.
  - Do not otherwise rewrite feature logic.
- `roastty/macos/Roastty.xcodeproj/project.pbxproj` or existing resource build
  plumbing
  - Include the renamed `shell-integration` resource tree under
    `Roastty.app/Contents/Resources/roastty/shell-integration/...`, matching
    `ResourcesDir::host()` / `RESOURCE_SUBDIR = "roastty"`.
  - The built app must contain these exact paths:
    - `Contents/Resources/roastty/shell-integration/bash/roastty.bash`
    - `Contents/Resources/roastty/shell-integration/bash/bash-preexec.sh`
    - `Contents/Resources/roastty/shell-integration/zsh/.zshenv`
    - `Contents/Resources/roastty/shell-integration/zsh/roastty-integration`
    - `Contents/Resources/roastty/shell-integration/fish/vendor_conf.d/roastty-shell-integration.fish`
    - `Contents/Resources/roastty/shell-integration/elvish/lib/roastty-integration.elv`
    - `Contents/Resources/roastty/shell-integration/nushell/vendor/autoload/roastty.nu`
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Link this experiment as `Designed`.
  - After the result, update the Phase I shell-integration checklist item.

## Verification

- Format markdown:
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/147-shell-integration-setup.md issues/0802-libroastty-completion-and-mac-app/README.md`
- Format Rust:
  - `cargo fmt`
- Run focused setup tests:
  - `cargo test -p roastty shell_integration -- --test-threads=1`
  - `cargo test -p roastty termio -- --test-threads=1`
  - `cargo test -p roastty resources_dir -- --test-threads=1`
- Run ABI harness:
  - `cargo test -p roastty --test abi_harness`
- Run full Roastty Rust coverage:
  - `cargo test -p roastty -- --test-threads=1`
- Run hosted app coverage:
  - `cd roastty && macos/build.nu --action test`
- Verify built app resource packaging:
  - `test -f roastty/macos/build/Debug/Roastty.app/Contents/Resources/roastty/shell-integration/bash/roastty.bash`
  - `test -f roastty/macos/build/Debug/Roastty.app/Contents/Resources/roastty/shell-integration/zsh/.zshenv`
  - `test -f roastty/macos/build/Debug/Roastty.app/Contents/Resources/roastty/shell-integration/zsh/roastty-integration`
  - `test -f roastty/macos/build/Debug/Roastty.app/Contents/Resources/roastty/shell-integration/fish/vendor_conf.d/roastty-shell-integration.fish`
  - `test -f roastty/macos/build/Debug/Roastty.app/Contents/Resources/roastty/shell-integration/elvish/lib/roastty-integration.elv`
  - `test -f roastty/macos/build/Debug/Roastty.app/Contents/Resources/roastty/shell-integration/nushell/vendor/autoload/roastty.nu`
  - Add or update a hosted macOS test that resolves `ResourcesDir::host()` from
    the built app context and asserts the bundled shell-integration files are
    visible through that exact directory.
- Check renamed resource hygiene:
  - `rg -n "ghostty|Ghostty|GHOSTTY" roastty/resources/shell-integration roastty/macos/Roastty.xcodeproj/project.pbxproj`
    must return no unintentional leftovers; if any intentional upstream
    compatibility string remains, document it in the result.
- Run general hygiene:
  - `cargo fmt --check`
  - `git diff --check`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/147-shell-integration-setup.md issues/0802-libroastty-completion-and-mac-app/README.md`

**Pass** = supported-shell detection and forced setup match upstream behavior
with Roastty env/resource names; missing resources fall back without breaking
PTY startup; feature flags are deterministic and include cursor blink/steady;
the renamed resource tree is bundled and free of unintended Ghostty names; a PTY
startup test proves setup reaches the child environment; full Rust and hosted
macOS tests pass; and the Phase I shell-integration checklist item can be marked
complete.

**Partial** = the pure setup helper is faithful and tested, but resource
packaging or live PTY startup integration needs a follow-up.

**Fail** = the current `Termio`/surface startup model cannot represent
upstream-style command/environment rewriting without a broader exec/config
refactor.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Ramanujan`, fresh
context.

**Initial verdict:** Changes required.

**Findings and fixes:**

- **Required:** the initial plan risked skipping `ROASTTY_SHELL_FEATURES` when
  `shell-integration = none`, diverging from upstream manual-integration
  behavior. Fixed by requiring feature env setup before every PTY spawn and
  gating only the automatic command/env rewrite on `shell-integration != none`.
- **Required:** the initial resource-bundle destination was underspecified.
  Fixed by requiring the copied tree under
  `Roastty.app/Contents/Resources/roastty/shell-integration/...` and listing
  exact expected built-app file paths for every supported shell.
- **Required:** verification did not directly prove macOS resource packaging.
  Fixed by adding built-app `test -f` checks and a hosted macOS test requirement
  that proves `ResourcesDir::host()` sees the bundled shell-integration files.

**Final verdict:** Approved.
