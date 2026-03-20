+++
status = "closed"
opened = "2026-03-20"
closed = "2026-03-20"
+++

# Issue 764: Move profile label to bottom-right with engine name

## Goal

The viewport bottom-right should show `[avatar] profile/engine` (e.g.,
`👤 default/roamium`). Remove the profile from the top-right.

## Background

### Current layout

- **Top-left:** Page title (or "Viewport")
- **Top-right:** `👤 default` (profile icon + name)
- **Bottom-left:** Hover URL (when hovering a link)
- **Bottom-right:** `roamium` (engine name, dimmed)

### Target layout

- **Top-left:** Page title (or "Viewport")
- **Top-right:** _(empty)_
- **Bottom-left:** Hover URL (when hovering a link)
- **Bottom-right:** `👤 default/roamium` (profile icon + name + slash + engine)

### Scope

TUI-only change. One section in the `ui()` function in `webtui/src/main.rs`.

## Experiments

### Experiment 1: Combine profile and engine in bottom-right

#### Description

Replace the separate `profile_title` (top-right) and `engine_label`
(bottom-right) with a single combined label in the bottom-right:
`[avatar] profile/engine`.

#### Changes

**1. TUI: `webtui/src/main.rs`**

In the viewport block construction (~line 949), replace:

```rust
let profile_title = Line::from(vec![
    Span::raw("\u{F007} ").style(Style::default().fg(COMMENT)),
    Span::raw(profile).style(Style::default().fg(FG)),
]);
```

and:

```rust
let engine_label = Line::from(Span::raw(browser_label).style(Style::default().fg(DIM)));
```

with a single combined label:

```rust
let engine_label = Line::from(vec![
    Span::raw("\u{F007} ").style(Style::default().fg(COMMENT)),
    Span::raw(format!("{}/{}", profile, browser_label)).style(Style::default().fg(DIM)),
]);
```

Remove the `.title_top(profile_title.alignment(Alignment::Right))` line from the
viewport block builder. The `.title_bottom(engine_label...)` line stays.

#### Verification

```bash
cd webtui && cargo build
```

| #   | Test                  | Steps                             | Expected                                       |
| --- | --------------------- | --------------------------------- | ---------------------------------------------- |
| 1   | Combined label shows  | Open `web -p work localhost:9616` | Bottom-right shows `👤 work/roamium`           |
| 2   | Top-right is empty    | Check viewport top-right          | No profile text there                          |
| 3   | Default profile       | Open `web localhost:9616`         | Bottom-right shows `👤 default/roamium`        |
| 4   | Hover URL still works | Hover a link                      | Bottom-left shows URL, bottom-right unaffected |

**Result:** Pass

Combined label shows correctly in the bottom-right, top-right is empty.

#### Conclusion

Removed `profile_title` and its `.title_top()` call, replaced `engine_label`
with a combined `👤 profile/engine` label. Three lines removed, four added.

## Conclusion

The viewport bottom-right now shows `👤 profile/engine` (e.g.,
`👤 default/roamium`), combining the profile name and browser engine into a
single label. The top-right is freed up for future use.
