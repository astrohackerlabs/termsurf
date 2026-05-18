+++
status = "closed"
opened = "2026-03-02"
closed = "2026-03-02"
+++

# Issue 697: Update Documentation

Review all living documentation for accuracy against Issues 600–696. Add a
comprehensive Features section to README.md. Update CLAUDE.md,
docs/keybindings.md, and other docs/\*.md files.

## Research

Reviewed all 97 issue docs (600–696) and all living docs. Findings below.

### README.md — Outdated

**Features section** is minimal (4 bullet points). Needs comprehensive expansion
covering all features added to Ghostty.

**Keyboard Modes table** is wrong:

- Says "Browse (default)" but Issue 649 changed startup to Control mode
- Only shows Browse/Control. Missing Edit mode (Issue 658), Command mode (Issue
  659), and their submodes
- Missing many keybindings: `i` (edit URL), `:` (command mode)

**"What works today" list** is incomplete. Missing:

- Dark mode / color scheme forwarding (Issue 680)
- Chrome DevTools in split panes (Issues 684, 690)
- Vim-style modes and commands (Issues 646, 658, 659, 665)
- Smart URL resolution (Issue 693)
- `file://` support (Issue 692)
- URL normalization — auto-prepend https:// (Issue 676)
- Configurable homepage (Issue 674)
- Active pane indicator with borders and desaturation (Issue 669)
- Click-to-focus without pass-through (Issue 670)
- Page title display (Issue 638)
- Tab lifecycle management (Issue 689)
- `:devtools`, `:colorscheme`, `:quitall` commands

**"Not yet started: In-process Chromium"** — explored in Issues 620–623 and
rejected due to JavaScript bottleneck. Should be removed or reworded.

**License section** references deleted directories: `ts5/`, `ts1/`, `ts3/`,
`vendor/cef-rs/`. These were archived in Issue 640.

**Getting Started** example `cargo run -- https://google.com` should be
`cargo run -- google.com` (smart resolve works now, Issue 693).

### CLAUDE.md — Missing Issues 666–696

**Current State** paragraph (line 117–126) stops at Issue 665. Missing 31
issues:

- Esc latency fix (Issue 666)
- Active pane indicator (Issues 667–669)
- Click-to-focus (Issue 670)
- App icon update (Issue 671)
- Border padding (Issue 672)
- Script consolidation (Issue 673)
- Configurable homepage (Issue 674)
- XPC hello message (Issue 675)
- URL normalization (Issue 676)
- Website deps/lint (Issues 677–678)
- License/trademark (Issue 679)
- Dark mode (Issue 680)
- Quit all / subsequence matching (Issue 681)
- DevTools (Issues 684, 687, 690, 691)
- Tab lifecycle (Issue 689)
- `web file` subcommand (Issue 692)
- Smart resolve (Issue 693)
- tab_id migration (Issue 694)
- Click protection fixes (Issues 695–696)

**Documentation index** (lines 163–221) is missing all docs from 634–696.
Currently jumps from 633 to 656, then stops at 665.

### docs/keybindings.md — Outdated

**TUI keybindings table** is incomplete:

- Missing: `e` (Control → Edit in normal mode), `:` (Control → Command mode)
- Missing: Command mode keys (`:q`, `:qa`, `:devtools`, `:colorscheme`)
- Missing: Edit mode submodes (Normal mode keys: `i`, `a`, `A`, `0`, `$`, etc.)

**GUI keybindings table** is wrong:

- Shows `Ctrl+Esc` for Browse→Control, but Issue 665 changed this to bare `Esc`

**Modes table** is outdated:

- Shows 3 modes but there are now 4: Browse, Control, Edit, Command
- Says "default on startup" for Browse, but Issue 649 changed to Control
- Missing Command mode (`:` prefix, Issue 659)

### docs/chromium.md — Current

Updated in Issue 694. Branch table and current state are accurate.

### docs/xdg.md — Current

No changes needed.

### docs/backlog.md — Current

Two items from Issue 655. No new items to add.

### docs/vendor.md — Current

Vendor repos are still used for reference. No changes needed.

### docs/ghostty.md — Current

Merge instructions are accurate.

### docs/early-prototypes.md — Current

Historical documentation, no changes needed.

### chromium/README.md — Current

Patch workflow documentation is accurate.

## Proposed Features Section (README.md)

Comprehensive list of all features TermSurf adds to Ghostty, organized by
category. This replaces both the current "Features" section and the "What works
today" list.

### Browser Integration

- Full Chromium browser in terminal panes via Content API (not CEF)
- Zero-copy CALayerHost compositing — Window Server composites directly from GPU
  VRAM, no per-frame IPC or texture copies
- 60fps Metal rendering at Retina resolution
- Dynamic resize — browser pane resizes with window/splits
- Multi-pane — multiple browser panes in one window
- Multi-profile isolation — separate cookies, sessions, storage per profile
- Dark mode — system color scheme forwarded to Chromium, overridable via
  `:colorscheme dark|light|system`
- Chrome DevTools — open in a split pane with `:devtools right|left|up|down`

### Mouse Input

- Click, drag, scroll forwarded to browser
- Cursor changes (pointer, text, crosshair, etc.)
- Text selection
- Click-to-focus — clicking an unfocused pane activates it without passing the
  click through (macOS-style)

### Keyboard Input

- Full keyboard forwarding to Chromium in browse mode
- Cmd+key bypass — Cmd+C/V/A/X/Z go to browser, not terminal
- Clipboard integration

### Navigation

- URL bar with vim-style editing (edtui widget)
- Smart URL resolution — `web google.com`, `web ./file.html`, `web :3000`,
  `web devtools` all resolve correctly
- URL normalization — bare domains get `https://` prefix automatically
- `file://` support — `web file <path>` or `web ./path`
- Browser navigation: Cmd+[ (back), Cmd+] (forward), Cmd+R (reload)
- Loading progress indicator
- Page title display in viewport border
- Links open in same tab (no popups)
- Configurable homepage — `web` without args opens default page

### Vim-Style Modes

- **Control** — terminal keybindings active (default on startup)
- **Browse** — keyboard/mouse goes to browser
- **Edit** — vim-style URL editing with Normal/Insert submodes
- **Command** — `:` prefix for commands (`:q`, `:devtools`, `:colorscheme`)
- Context-sensitive Esc — exits current mode appropriately
- Per-mode color indicators (LazyVim Tokyo Night palette)

### Commands

- `:q` / `:quit` — quit
- `:qa` / `:quitall` — quit all panes
- `:devtools [direction]` — open DevTools in split pane
- `:colorscheme dark|light|system` — set color scheme
- Vim-style subsequence matching — `:cs dark` works for `:colorscheme dark`

### UI

- Active pane indicator with colored borders and background desaturation
- Inner padding so borders don't cover content
- Purple border in Edit mode
- Tight title spacing

### Terminal

Based on [Ghostty](https://ghostty.org/). All Ghostty features, configuration,
and keybindings work out of the box. TermSurf adds browser integration on top.

## Changes Required

### 1. README.md

- Replace Features section with comprehensive version above
- Replace Keyboard Modes section with updated modes/keys
- Remove "Status" / "What works today" / "Not yet started" — replaced by
  Features
- Fix License section (remove deleted directory references)
- Update Getting Started example

### 2. CLAUDE.md

- Update Current State paragraph to include Issues 666–696
- Add all missing issue docs to Documentation index (634–696)

### 3. docs/keybindings.md

- Update TUI keybindings table (add `:`, Command mode keys)
- Fix GUI keybindings (Ctrl+Esc → bare Esc)
- Update Modes table (4 modes, Control is default)
- Add Command mode section

## Conclusion

Reviewed all 97 issue docs (600–696) against every living doc. Updated three
files:

1. **README.md** — Replaced the 4-bullet Features section with a comprehensive
   8-category breakdown (Browser Integration, Mouse Input, Keyboard Input,
   Navigation, Vim-Style Modes, Commands, UI, Terminal). Replaced the 2-mode
   keyboard table with all 4 modes and correct default (Control, not Browse).
   Removed the outdated "Status" / "What works today" / "Not yet started"
   sections. Fixed the License section (removed references to archived
   directories). Updated the Getting Started example to use smart resolve.

2. **CLAUDE.md** — Extended the Current State paragraph to cover Issues 666–696
   (14 lines of new feature summaries). Added 63 missing issue doc entries to
   the Documentation index (634–697), bringing it from 45 to 108 entries.

3. **docs/keybindings.md** — Fixed `Ctrl+Esc` → bare `Esc` (Issue 665). Added
   `:` to the TUI keybindings table. Added a Commands section with all four
   commands and subsequence matching. Updated the Modes table from 3 to 4 modes
   with Control as default.

Six other docs reviewed and confirmed current: docs/chromium.md, docs/xdg.md,
docs/vendor.md, docs/ghostty.md, docs/early-prototypes.md, chromium/README.md.
