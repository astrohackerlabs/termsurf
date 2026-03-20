+++
status = "open"
opened = "2026-03-19"
+++

# Issue 762: Cookies and network state not persisted across restarts

## Goal

Cookies, HTTP cache, and other network state should persist to disk so that
users stay logged in across Roamium restarts.

## Background

### The problem

When Roamium exits and restarts, all cookies are gone. Users are logged out of
every site. localStorage may also be lost. The profile directory at
`~/.local/share/termsurf/chromium-profiles/{profile}/` exists and contains some
data (GPU cache, etc.), but cookies and network state are not among it.

### Root cause

Content shell's `ConfigureNetworkContextParamsForShell()` in
`shell_content_browser_client.cc` does not set `file_paths` on the
`NetworkContextParams`. Without `file_paths`, the network service uses
**in-memory storage** for everything — cookies, HTTP cache, HTTP server
properties, and all other network state. The data lives only in memory and is
lost when the process exits.

Chrome solves this in
`ProfileNetworkContextService::ConfigureNetworkContextParams()` by setting:

```cpp
network_context_params->file_paths =
    ::network::mojom::NetworkContextFilePaths::New();
network_context_params->file_paths->data_directory = path.Append("Network");
network_context_params->file_paths->unsandboxed_data_path = path;
network_context_params->file_paths->cookie_database_name =
    base::FilePath("Cookies");
network_context_params->file_paths->http_cache_directory =
    cache_path.Append("Cache");
network_context_params->file_paths->http_server_properties_file_name =
    base::FilePath("Network Persistent State");
```

Content shell never does this because it's a test harness, not a real browser.

### History

This was never solved in the Content API generation. Earlier generations used
CEF, which handles cookie persistence internally when given a `cache_path`. When
TermSurf switched from CEF to the Content API (ts5/ts6), nobody wired up the
network service file paths. The `--user-data-dir` flag correctly sets
`ShellBrowserContext::GetPath()`, but that path is only used by some subsystems
(GPU cache, etc.) — not the network service.

### Fix

Override `ConfigureNetworkContextParamsForShell()` in our content browser client
(or patch content shell's implementation) to set `file_paths` using the browser
context's path. The minimum viable change:

```cpp
auto* context_impl = static_cast<ShellBrowserContext*>(context);
base::FilePath path = context_impl->GetPath();

context_params->file_paths =
    network::mojom::NetworkContextFilePaths::New();
context_params->file_paths->data_directory = path.Append("Network");
context_params->file_paths->unsandboxed_data_path = path;
context_params->file_paths->cookie_database_name =
    base::FilePath("Cookies");
context_params->file_paths->http_cache_directory =
    path.Append("Cache");
context_params->file_paths->http_server_properties_file_name =
    base::FilePath("Network Persistent State");
```

### Scope

Chromium-only change. One function in `shell_content_browser_client.cc` (or a
TermSurf override). No protocol, Roamium, or TUI changes needed.

## Experiments

### Experiment 1: Override ConfigureNetworkContextParamsForShell in TsBrowserClient

#### Description

Override the virtual method `ConfigureNetworkContextParamsForShell()` in
`TsBrowserClient` (our `ShellContentBrowserClient` subclass) to set `file_paths`
on the network context params. This tells the network service to write cookies,
cache, and HTTP state to disk inside the profile directory.

The method is `virtual` and `protected` on `ShellContentBrowserClient` (line 227
of `shell_content_browser_client.h`), specifically designed for subclass
overrides. Our `TsBrowserClient` already overrides three other methods on this
class.

#### Chromium branch

Create `146.0.7650.0-issue-762` from `146.0.7650.0-issue-759` (the most recent
branch). After committing, generate patches with:

```bash
cd chromium/src
rm -rf ../../chromium/patches/issue-762/
git format-patch 146.0.7650.0..HEAD -o ../../chromium/patches/issue-762/
```

Add the new branch to the Branches table in `chromium/README.md` and update the
Current State section.

#### Changes

**1. Chromium: `content/libtermsurf_chromium/ts_browser_client.h`**

Add the override declaration (~line 34, after `OverrideWebPreferences`):

```cpp
void ConfigureNetworkContextParamsForShell(
    BrowserContext* context,
    network::mojom::NetworkContextParams* context_params,
    cert_verifier::mojom::CertVerifierCreationParams*
        cert_verifier_creation_params) override;
```

Add required forward declarations / includes if needed:

```cpp
#include "services/network/public/mojom/network_context.mojom.h"
```

**2. Chromium: `content/libtermsurf_chromium/ts_browser_client.cc`**

Add the include (~line 6):

```cpp
#include "content/shell/browser/shell_browser_context.h"
#include "services/network/public/mojom/network_context.mojom.h"
```

Add the override implementation (~after `OverrideWebPreferences`, line 82):

```cpp
void TsBrowserClient::ConfigureNetworkContextParamsForShell(
    BrowserContext* context,
    network::mojom::NetworkContextParams* context_params,
    cert_verifier::mojom::CertVerifierCreationParams*
        cert_verifier_creation_params) {
  // Call base class for user_agent, accept_language, zstd, etc.
  ShellContentBrowserClient::ConfigureNetworkContextParamsForShell(
      context, context_params, cert_verifier_creation_params);

  // Set file paths so the network service persists cookies, cache, etc.
  auto* shell_context = static_cast<ShellBrowserContext*>(context);
  base::FilePath path = shell_context->GetPath();

  context_params->file_paths =
      network::mojom::NetworkContextFilePaths::New();
  context_params->file_paths->data_directory = path.Append("Network");
  context_params->file_paths->unsandboxed_data_path = path;
  context_params->file_paths->cookie_database_name =
      base::FilePath("Cookies");
  context_params->file_paths->http_cache_directory = path.Append("Cache");
  context_params->file_paths->http_server_properties_file_name =
      base::FilePath("Network Persistent State");
}
```

Calls the base class first to preserve its setup (user agent, accept language,
zstd, CORS), then adds `file_paths`. The directory structure matches Chrome's
conventions: `Network/` for data, `Cache/` for HTTP cache, `Cookies` for the
cookie database.

#### Verification

```bash
scripts/build.sh chromium
```

| #   | Test                  | Steps                                                      | Expected                                           |
| --- | --------------------- | ---------------------------------------------------------- | -------------------------------------------------- |
| 1   | Cookies persist       | Log into a site, quit Roamium, reopen, check login state   | User remains logged in                             |
| 2   | Files created on disk | Check `~/.local/share/termsurf/chromium-profiles/default/` | `Network/`, `Cache/`, `Cookies` files exist        |
| 3   | Multiple profiles     | Log into different sites on two profiles, restart both     | Each profile retains its own cookies independently |
| 4   | No regression         | Browse normally, navigate, open DevTools                   | Everything works as before                         |

**Result:** Fail

Catastrophic regression. No web pages load at all — only a white screen. After
many minutes and multiple restarts, no content ever renders. Setting
`file_paths` on the network context params breaks the network service entirely.
The override needs to be reverted.

#### Conclusion

Setting `file_paths` in `ConfigureNetworkContextParamsForShell` broke page
loading completely. The issue is likely that content shell's network service
setup has additional requirements or constraints that our minimal `file_paths`
configuration doesn't satisfy. The next experiment should investigate what
Chrome does differently — there may be additional fields, initialization order,
or directory creation steps that are required for `file_paths` to work without
breaking the network stack.
