# Vendor Repos

Cloned repos for reading source code and learning. These are gitignored and not
committed to the TermSurf repo.

## vendor/

| Repo                | URL                                         | Why                                                                                                              |
| ------------------- | ------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| `vendor/ghostty/`   | https://github.com/ghostty-org/ghostty      | TermSurf GUI forks Ghostty. Reference for understanding upstream behavior, diffing changes, and planning merges. |
| `vendor/wezterm/`   | https://github.com/wezterm/wezterm          | Terminal emulator evaluated in ts2–ts3. Reference for terminal internals and IPC patterns.                       |
| `vendor/electron/`  | https://github.com/electron/electron        | Reference for Chromium embedding patterns, patch sets, and Content API usage.                                    |
| `vendor/alacritty/` | https://github.com/alacritty/alacritty      | Terminal emulator evaluated in ts4. Reference for Rust terminal architecture.                                    |
| `vendor/ladybird/`  | https://github.com/LadybirdBrowser/ladybird | Reference for Girlbat/Ladybird engine research, LibWebView embedding, and IOSurface/Metal rendering.             |

## libghostty dependency sources (Roastty proof-of-concept)

Upstream sources for the third-party libraries Ghostty depends on, cloned for
**reading and study only**. They were gathered for the Roastty Rust rewrite
proof-of-concept, not for the production frontend direction. Ghostboard is the
primary TermSurf frontend.

Pinned tags (`z2d`, `uucode`) are cloned at the exact version Roastty is
studying; the rest are the default branch. Ghostty's exact pins live in
`vendor/ghostty/build.zig.zon` (Zig libs) and `vendor/ghostty/pkg/<name>/` (C
libs). These sources are never vendored, linked, or built in TermSurf; they are
gitignored (`vendor/.gitignore`) like the analysis repos above.

| Repo                    | Origin | URL                                              | Provides (reimplementation role)                                                      |
| ----------------------- | ------ | ------------------------------------------------ | ------------------------------------------------------------------------------------- |
| `vendor/uucode/`        | Zig    | https://github.com/jacobsandlund/uucode (v0.2.0) | Unicode property / grapheme-break / width tables — core terminal + font text handling |
| `vendor/libxev/`        | Zig    | https://github.com/mitchellh/libxev              | Async event loop — PTY/IO read-write loops and timers (macOS kqueue path)             |
| `vendor/z2d/`           | Zig    | https://github.com/vancluever/z2d (v0.10.0)      | 2D vector rasterization — sprite-font AA path glyphs + CPU debug overlay              |
| `vendor/zf/`            | Zig    | https://github.com/natecraddock/zf               | Fuzzy matching for list/command filtering                                             |
| `vendor/zig-objc/`      | Zig    | https://github.com/mitchellh/zig-objc            | Objective-C runtime bindings — already satisfied by `objc2`                           |
| `vendor/vaxis/`         | Zig    | https://github.com/rockorager/libvaxis           | TUI toolkit — used only by Ghostty's `+list-*` CLI tools                              |
| `vendor/zig-js/`        | Zig    | https://github.com/mitchellh/zig-js              | WASM/JS interop — out of scope for the macOS library                                  |
| `vendor/harfbuzz/`      | C      | https://github.com/harfbuzz/harfbuzz             | Text shaping — superseded by CoreText on macOS (reference only)                       |
| `vendor/freetype/`      | C      | https://github.com/freetype/freetype             | Glyph rasterization — superseded by CoreText on macOS (reference only)                |
| `vendor/wuffs/`         | C      | https://github.com/google/wuffs                  | Image decoding (Kitty graphics PNG)                                                   |
| `vendor/libpng/`        | C      | https://github.com/pnggroup/libpng               | PNG decode/encode                                                                     |
| `vendor/zlib/`          | C      | https://github.com/madler/zlib                   | DEFLATE / inflate                                                                     |
| `vendor/oniguruma/`     | C      | https://github.com/kkos/oniguruma                | Regular expressions (link/URL detection)                                              |
| `vendor/simdutf/`       | C      | https://github.com/simdutf/simdutf               | Fast UTF-8 validation / transcoding                                                   |
| `vendor/highway/`       | C      | https://github.com/google/highway                | SIMD primitives used by the above                                                     |
| `vendor/sentry-native/` | C      | https://github.com/getsentry/sentry-native       | Crash reporting (app-level, optional)                                                 |
| `vendor/imgui/`         | C      | https://github.com/ocornut/imgui                 | Dear ImGui — inspector UI                                                             |
| `vendor/dear_bindings/` | C      | https://github.com/dearimgui/dear_bindings       | C-API generator that produces `dcimgui` from Dear ImGui                               |
| `vendor/glslang/`       | C      | https://github.com/KhronosGroup/glslang          | GLSL→SPIR-V — shader translation (deferred; Metal uses precompiled shaders)           |
| `vendor/spirv-cross/`   | C      | https://github.com/KhronosGroup/SPIRV-Cross      | SPIR-V→MSL — shader translation (deferred)                                            |

Not cloned (out of scope per the issue's macOS-only constraint): `fontconfig`,
`gobject`/GTK, `gtk4-layer-shell`, `wayland` (+ protocols), `opengl`, `libintl`,
and the font/theme/SDK asset packages.

## Chromium (special case)

Chromium lives at `chromium/src/` (not in `vendor/`). The repo is too large to
have two clones, so `chromium/src/` serves double duty: it is both the build
workspace for TermSurf's Chromium fork and the source code reference. When
studying Chromium internals (e.g., `WebContentsObserver`, Content API,
compositor pipeline), read from `chromium/src/` directly.

| Path            | Upstream                                       |
| --------------- | ---------------------------------------------- |
| `chromium/src/` | https://chromium.googlesource.com/chromium/src |
