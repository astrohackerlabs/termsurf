+++
status = "open"
opened = "2026-06-17"
+++

# Issue 819: Ghostboard Packaging and Identity Hardening

## Goal

Audit and harden Ghostboard packaging, app identity, config locations, release
packaging, and normal launch environment.

## Background

Issue 810 grouped packaging and identity as a `Maybe` finding. This overlaps
with launch workflow but is specifically about user-visible and distributable
app identity: app bundle naming, config paths, release packaging, normal launch
environment, and debug-vs-installed binary selection.

## Analysis

The work should first define which names intentionally remain Ghostty upstream
names and which names must be Ghostboard or TermSurf names. It should then audit
the app bundle, menus, config paths, binary names, generated artifacts, release
scripts, and docs against that decision.

Verification should include:

- app bundle identity is deliberate;
- config location is documented and loaded;
- release packaging includes the intended binaries and assets;
- debug testing cannot accidentally mask installed-app behavior;
- docs and scripts agree on the supported launch paths.

## Experiments

- [Experiment 1: Audit packaging identity contract](01-audit-packaging-identity-contract.md)
  — **Pass**
- [Experiment 2: Decide public macOS app identity](02-decide-public-macos-app-identity.md)
  — **Pass**
- [Experiment 3: Implement macOS bundle identity](03-implement-macos-bundle-identity.md)
  — **Pass**
- [Experiment 4: Clean user-visible Ghostty identity](04-clean-user-visible-ghostty-identity.md)
  — **Pass**
- [Experiment 5: Prove config loading paths](05-prove-config-loading-paths.md) —
  **Pass**
- [Experiment 6: Align config path documentation](06-align-config-path-documentation.md)
  — **Pass**
