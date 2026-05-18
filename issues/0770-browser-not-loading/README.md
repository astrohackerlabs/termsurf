+++
status = "closed"
opened = "2026-04-05"
closed = "2026-04-05"
+++

# Issue 770: Browser does not load

## Goal

Diagnose and fix why `web ryanxcharles.com` no longer opens a browser in
Wezboard.

## Background

Running `web ryanxcharles.com` in Wezboard does not display a browser. The
browser was working prior to recent development.

## Experiments

### Experiment 1: Run Roamium manually and diagnose

Run Roamium with the same arguments the GUI passes to it and capture its output.

#### Diagnostic sequence

1. **Wezboard logs** showed: Roamium process spawned but never connected to the
   GUI. Only one connection (the TUI) appeared. `has_tx=false` confirmed
   `ServerRegister` was never received.

2. **Kernel logs** (`log show --predicate 'process == "kernel"'`) showed:
   ```
   proc XXXXX: load code signature error 2 for file "roamium"
   ```
   macOS was killing the binary before it could start due to an invalid code
   signature. The `install.sh` script copies binaries with `cp` under `sudo`,
   which strips the code signature.

3. **Re-signing** with `codesign --force --sign -` fixed the `Killed: 9` error.
   The main Roamium process then started successfully:
   ```
   [libtermsurf_chromium] Initialized, firing callback
   [Roamium] connect failed: No such file or directory (os error 2)
   ```
   But all Chromium **child processes** (GPU, network, renderer) crashed:
   ```
   FATAL:content/app/content_main_runner_impl.cc:1002]
   Check failed: sandbox::Seatbelt::IsSandboxed().
   ```
   This happened even with `--no-sandbox`.

4. **Same crash from the Chromium build directory** — not an install issue.

5. **Root cause identified: macOS updated from 26.3.1 to 26.4 on March 29.**
   The Chromium build was compiled against the 26.3.1 SDK. macOS 26.4 changed
   sandbox behavior, causing child processes to fail the `IsSandboxed()` check.
   The old installed binary (from March 19, before the OS update) had continued
   working because macOS maintains backward compatibility for existing signed
   binaries. Running `install.sh` today replaced the binary with a fresh
   unsigned copy, which triggered the new OS version's stricter enforcement.

6. **Fix: full Chromium rebuild** with `scripts/build.sh chromium --clean`,
   then `scripts/build.sh all --release` and `scripts/install.sh all`. The
   rebuild compiled Chromium against the macOS 26.4 SDK, producing binaries
   compatible with the new sandbox behavior.

**Result:** Pass

`web ryanxcharles.com` loads successfully after the full rebuild and reinstall.

## Conclusion

The browser failure was caused by a **macOS SDK mismatch**. Chromium was built
against macOS 26.3.1, then macOS updated to 26.4 which changed Seatbelt sandbox
initialization for child processes. The previously installed binary continued
working (backward compatibility), but reinstalling today replaced it with a
fresh unsigned copy that exposed the incompatibility.

### Symptoms

- `web` TUI connects to Wezboard, sends `SetOverlay`, Roamium spawns — but
  never connects back to the GUI.
- Wezboard logs show `has_tx=false` and only one connection (TUI, no browser).
- Kernel log: `load code signature error 2 for file "roamium"` (before
  re-signing).
- After re-signing: child processes crash with
  `FATAL:content/app/content_main_runner_impl.cc:1002]
  Check failed: sandbox::Seatbelt::IsSandboxed()`.

### Diagnostic commands

```bash
# Check kernel log for code signature errors
log show --predicate 'process == "kernel" AND eventMessage CONTAINS "roamium"' --last 5m

# Check macOS version
sw_vers

# Check macOS update history
softwareupdate --history | head -10

# Run Roamium manually to see crash output
/usr/local/roamium/roamium --ipc-socket=/tmp/dummy.sock \
  --user-data-dir=~/.local/share/termsurf/chromium-profiles/default \
  --listen-socket=/tmp/test.sock --hidden --no-sandbox 2>&1 | head -20

# Re-sign binaries after install (fixes code signature errors)
sudo codesign --force --sign - /usr/local/roamium/*.dylib /usr/local/roamium/roamium
```

### Fix

Full Chromium rebuild against the current macOS SDK:

```bash
scripts/build.sh chromium --clean
scripts/build.sh all --release
sudo scripts/install.sh all
```

### Prevention

**After any macOS update, rebuild Chromium.** The Seatbelt sandbox APIs are
tightly coupled to the OS version. A Chromium binary built against an older SDK
may fail to initialize sandboxes on a newer OS, causing child processes (GPU,
network, renderer) to crash.

The install script should ideally re-sign binaries after copying, but the
deeper issue is the SDK mismatch — re-signing alone won't fix sandbox
incompatibilities.
