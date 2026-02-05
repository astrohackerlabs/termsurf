# TermSurf 4.0: Architecture Reconsideration

## Problem

Issue 339 concluded that CEF cannot deliver the 60fps browser rendering TermSurf
requires. The only viable path forward is embedding Chromium directly, like
Electron and Steam have done.

This forces us to reconsider nearly every architectural choice made to date.

## Why This Changes Everything

### The Original Architecture (ts3)

TermSurf 3.0 was built on two Rust foundations:

| Component | Implementation | Language               |
| --------- | -------------- | ---------------------- |
| Terminal  | WezTerm (fork) | Rust                   |
| Browser   | CEF via cef-rs | Rust (bindings to C++) |

This worked because:

- WezTerm is a mature, full-featured terminal in Rust
- cef-rs provided Rust bindings to CEF
- Both could be "plugged together" in a unified Rust codebase

### The New Reality

Embedding Chromium directly means:

- **Chromium is C++** — No Rust bindings exist for direct embedding
- **Electron is C++** — Its OSR implementation is C++
- **The 240fps code path is C++** — `FrameSinkVideoCapturer`, `viz` layer, etc.

This breaks the Rust assumption.

## The Core Question

**What programming language and terminal should TermSurf use?**

### Option Space

| Approach | Terminal                  | Browser         | Language       |
| -------- | ------------------------- | --------------- | -------------- |
| A        | C++ terminal              | Chromium direct | C++            |
| B        | Rust terminal + C++ FFI   | Chromium direct | Rust + C++     |
| C        | Other language + bindings | Electron OSR    | ???            |
| D        | Electron-based terminal   | Electron        | TypeScript/C++ |

### Factors to Consider

1. **Terminal quality** — Must match or exceed current WezTerm functionality
2. **Browser integration** — Must achieve 60fps with GPU texture sharing
3. **Development velocity** — Team expertise, ecosystem, tooling
4. **Maintenance burden** — Long-term cost of each approach
5. **Cross-platform** — macOS now, Linux/Windows later

## What This Document Will Track

- [ ] Survey of C++ terminals (Alacritty's C++ predecessors, etc.)
- [ ] Feasibility of Rust + C++ FFI for Chromium embedding
- [ ] Electron as a platform (not just browser component)
- [ ] Alternative approaches (WebGPU terminals, etc.)
- [ ] Decision framework and recommendation

## Related Issues

- [Issue 338: Browser lag investigation](./338-lag.md) — Why CEF doesn't work
- [Issue 339: Electron study](./339-electron.md) — How Electron achieves 240fps
