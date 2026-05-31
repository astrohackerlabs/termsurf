# Experiment 10: Add Explicit Default-Deny Permissions

## Description

Experiment 1 placed permission/API default-deny hardening after renderer crash
UX. Experiment 9 is now complete, so this experiment starts that queue item with
the deterministic surfaces that are already covered by the Issue 799 harness.

The current full harness evidence shows a concrete policy mismatch:

```text
logs/issue-799-browser-api-audit/20260531-021630-171874
permissions-query:
  geolocation: granted
  notifications: denied
  camera: denied
  microphone: denied
geolocation-deny:
  rejected code=3 message="Timeout expired"
file-system-access:
  blocked_user_activation before the picker policy is reached
service-worker-basic:
  resolved
webauthn-create:
  blocked_needs_virtual_authenticator
```

The `geolocation: granted` result is inherited from Content Shell's
`ShellPermissionManager`, which allowlists geolocation and several test-focused
permissions by default. That policy is wrong for TermSurf. TermSurf should not
pretend a page has geolocation permission when there is no TermSurf geolocation
provider or user permission UX. The result should be explicit denial, not a
timeout.

This experiment adds a TermSurf-owned default-deny permission policy for common
web permissions and a contained deny path for File System Access pickers. It
does not implement real permission prompts, camera/mic capture, native file
pickers, WebAuthn virtual authenticators, OS notifications, or service worker
feature expansion. Those are separate experiments or future issues.

## Changes

1. Create a new Chromium branch.

   In `chromium/src`, fork from:

   ```text
   148.0.7778.97-issue-799-exp9
   ```

   Name the new branch:

   ```text
   148.0.7778.97-issue-799-exp10
   ```

   Add it to `chromium/README.md` with a description such as:

   ```text
   Add explicit default-deny permissions.
   ```

2. Add a TermSurf permission manager.

   Add `TsPermissionManager` under `chromium/src/content/libtermsurf_chromium/`.

   It should implement `content::PermissionControllerDelegate` directly or
   subclass the shell manager only if subclassing does not inherit the shell
   allowlist. The TermSurf policy for this experiment is:
   - deny `GEOLOCATION` and `GEOLOCATION_APPROXIMATE`;
   - deny `NOTIFICATIONS`;
   - deny `AUDIO_CAPTURE`, `VIDEO_CAPTURE`, and `CAMERA_PAN_TILT_ZOOM`;
   - deny `DISPLAY_CAPTURE`;
   - deny `LOCAL_FONTS`;
   - deny every permission type not explicitly needed by an already-working
     TermSurf feature;
   - do not grant anything merely because Content Shell grants it for browser
     tests.

   If a permission descriptor maps to an unknown/new permission type, return
   `DENIED` and log a concise `[termsurf-permission] denied type=...` line.

   Match Chromium 148's method signatures:
   - return one denied `PermissionResult` per requested permission from
     `RequestPermissionsFromCurrentDocument`;
   - return `blink::mojom::PermissionStatus::DENIED` from `GetPermissionStatus`;
   - return `PermissionResult(blink::mojom::PermissionStatus::DENIED)` from
     `GetPermissionResultForOriginWithoutContext`;
   - return `PermissionResult(blink::mojom::PermissionStatus::DENIED)` from
     `GetPermissionResultForCurrentDocument`;
   - return `PermissionResult(blink::mojom::PermissionStatus::DENIED)` from
     `GetPermissionResultForWorker`;
   - return `PermissionResult(blink::mojom::PermissionStatus::DENIED)` from
     `GetPermissionResultForEmbeddedRequester`.

   This is a default-deny safety layer, not a prompt UI. Do not add a
   request/reply protobuf, webtui prompt, native prompt, allowlist UI, or
   persistent permission storage in this experiment.

3. Install the TermSurf permission manager in browser contexts.

   Avoid editing Content Shell globally.

   Preferred implementation:
   - add a small `TsBrowserContext` subclass of `ShellBrowserContext`;
   - override `GetPermissionControllerDelegate()` to lazily allocate
     `TsPermissionManager`;
   - use `TsBrowserContext` for both regular and off-the-record contexts in
     `TsBrowserMainParts::InitializeBrowserContexts()`.

   If subclassing `ShellBrowserContext` is awkward, use the smallest local
   alternative that keeps the policy inside `libtermsurf_chromium` and does not
   change `content/shell/` behavior for other embedders.

4. Block File System Access native pickers.

   Override `ContentBrowserClient::IsFileSystemAccessApiFilePickerAllowed(...)`
   in `TsBrowserClient` and return `false`.

   This should make `showOpenFilePicker()` reject with a deterministic
   permission-denied error after user activation, without opening a native
   picker. Do not implement a real file picker or auto-select a file in this
   experiment.

5. Extend the harness verification.

   Update existing probes or add focused companion probes so automation proves:
   - `permissions-query` reports `denied` for geolocation, notifications,
     camera, and microphone;
   - `geolocation-deny` rejects with `PERMISSION_DENIED` / code `1`, not timeout
     code `3`;
   - `notification-permission` remains `denied`;
   - File System Access reaches the picker policy after synthetic user
     activation and rejects deterministically without native UI;
   - `service-worker-basic` still resolves;
   - no bad-Mojo, missing-binder, or unexpected crash signature appears.

   Add a specific File System Access activation path instead of accepting the
   current `blocked_user_activation` result. The probe can render a button or
   install a click handler, the harness can send the same contained mouse/key
   activation style used by JavaScript dialog tests, and the page can then call
   `showOpenFilePicker()`. A pass requires reaching the TermSurf denial path
   after activation.

   The File System Access result must distinguish activation failure from
   TermSurf's intended denial. Record explicit evidence:
   - `activation_sent`;
   - `activation_observed`;
   - `picker_call_started_after_activation`;
   - the rejection `errorName` and message;
   - a distinct success classification such as `file_system_access_denied`.

   `blocked_user_activation` remains a failure for this experiment.

6. Leave WebAuthn virtual-authenticator coverage for the next experiment.

   `webauthn-create` currently reports `blocked_needs_virtual_authenticator`.
   That is a different setup problem from default-deny permission policy. Do not
   broaden this experiment into DevTools virtual authenticator plumbing.

7. Run formatting and builds.
   - Run Chromium `clang-format` on edited C++ headers/sources.
   - Run `cargo fmt` after any Rust edits and accept all output.
   - Build Chromium with `autoninja -C out/Default libtermsurf_chromium`.
   - Build debug `roamium`, `webtui`, and `wezboard` if protobuf/Rust code
     changes. If this experiment only changes Chromium and the Python harness,
     build Roamium at minimum so the linked Chromium dylib is exercised.

8. Regenerate the Chromium patch archive after a passing implementation.

   Use the standard Issue 799 patch archive:

   ```text
   chromium/patches/issue-799/
   ```

## Verification

1. Run focused probes:

   ```bash
   python3 scripts/test-issue-799-browser-api-audit.py \
     --probe permissions-query \
     --probe geolocation-deny \
     --probe notification-permission \
     --probe file-system-access \
     --probe service-worker-basic
   ```

   Pass criteria:
   - `permissions-query` classifies as the chosen default-deny success
     classification and records `denied` for geolocation, notifications, camera,
     and microphone;
   - `geolocation-deny` rejects with code `1` / permission denied, not timeout;
   - `notification-permission` returns `denied`;
   - `file-system-access` reaches post-activation denial with
     `activation_sent=true`, `activation_observed=true`,
     `picker_call_started_after_activation=true`, classification
     `file_system_access_denied`, and no native picker appears;
   - `service-worker-basic` still resolves;
   - no missing binder, bad Mojo, or unexpected crash appears.

2. Run the full Issue 799 harness:

   ```bash
   python3 scripts/test-issue-799-browser-api-audit.py
   ```

   Pass criteria:
   - all previous completed feature probes remain green;
   - `renderer-crash-recovery` still reports `renderer_crash_recovered`;
   - `missing_interfaces` is empty;
   - `empty_interfaces` is empty;
   - overall status is `completed`;
   - WebAuthn may remain `blocked_needs_virtual_authenticator`, because this
     experiment explicitly leaves virtual-authenticator setup for the next
     experiment.

3. Inspect artifacts:
   - `probe-results.json`
   - `coverage-map.md`
   - `reference-coverage-map.md`
   - `roamium.stderr`

   The coverage maps must not describe geolocation as granted or File System
   Access as merely blocked by missing activation.

## Failure Criteria

The experiment fails if:

- geolocation still reports `granted`;
- geolocation still times out instead of rejecting with permission denied;
- File System Access is still classified only as `blocked_user_activation`;
- any native file picker or permission prompt opens;
- the implementation modifies Content Shell globally instead of TermSurf's
  embedder layer;
- the implementation adds prompt UI or persistent permission storage;
- service worker registration regresses;
- prior Issue 799 probes regress;
- bad-Mojo, missing-binder, or unexpected crash evidence appears.

## Non-Negotiable Invariants

- TermSurf's default permission posture is deny unless a feature-specific
  experiment explicitly implements safe behavior.
- Do not claim camera/mic, real geolocation, real notifications, WebAuthn, or
  File System Access support from this experiment.
- Do not keep Content Shell's test allowlist for background sync/fetch, sensors,
  NFC, idle detection, or other permissions. `service-worker-basic` is only a
  registration regression check; it is not proof that every
  service-worker-adjacent browser service is complete.
- Do not add native UI.
- Do not add manual verification requirements unless automation proves
  insufficient.
- Do not use `ninja`; Chromium builds must use `autoninja`.
- Run `cargo fmt` after Rust edits and accept its output.
