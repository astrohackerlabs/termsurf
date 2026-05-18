+++
status = "closed"
opened = "2026-03-22"
closed = "2026-03-22"
+++

# Issue 765: Wezboard pane crashes on XTGETTCAP (corrupted terminfo)

## Goal

Fix the panic in `term/src/terminalstate/mod.rs:41` caused by a corrupted
compiled terminfo database, so that programs like vim can query terminal
capabilities without crashing the pane.

## Background

### The problem

Running `vim` from a shell that sets `TERM=wezboard` crashes the Wezboard pane.
The panic is:

```
panic at term/src/terminalstate/mod.rs:41:42 - !?
called `Result::unwrap()` on an `Err` value: Parse
```

The crash occurs when vim sends an `XTGETTCAP` (DCS+q) escape sequence to query
terminal capabilities. Wezboard tries to answer by loading the compiled terminfo
database at `termwiz/data/wezboard`, which fails to parse.

### Root cause

The WezTerm → Wezboard rename script (`scripts/rename-wezterm.sh`) did a binary
find-and-replace of "wezterm" → "wezboard" inside the **compiled** terminfo
file. This made the terminal name 1 byte longer (7 → 8 characters) but did not
update the name section length field in the terminfo header.

- **Original (wezterm):** header says name section = 32 bytes (`20 00`), actual
  name "wezterm|Wez's terminal emulator\0" = 32 bytes. Correct.
- **After rename (wezboard):** header still says 32 bytes (`20 00`), but actual
  name "wezboard|Wez's terminal emulator\0" = 33 bytes. Off by 1.

Every field after the name section is shifted by 1 byte, corrupting the entire
database. The `terminfo` crate's parser returns `Err(Parse)`.

The same corruption affects `termwiz/data/w/wezboard` (a subdirectory copy).

### Fix

Recompile the terminfo database from the source file
`termwiz/data/wezboard.terminfo` (which already has the name "wezboard") using
`tic -x`. Replace both corrupted binary files with the fresh output.

## Experiments

### Experiment 1: Recompile terminfo from source

#### Description

Use `tic -x` to compile `wezboard.terminfo` into a fresh binary, then replace
the two corrupted files (`termwiz/data/wezboard` and `termwiz/data/w/wezboard`).
Verify the header's name section length is correct (33 = `21 00`).

#### Changes

**1. Recompile:**

```bash
cd wezboard
tic -x -o /tmp/wezboard-tic termwiz/data/wezboard.terminfo
```

macOS `tic` outputs to a hex hash directory (`77/wezboard`).

**2. Replace corrupted files:**

```bash
cp /tmp/wezboard-tic/77/wezboard termwiz/data/wezboard
cp /tmp/wezboard-tic/77/wezboard termwiz/data/w/wezboard
```

**3. Verify header:**

```bash
hexdump -C termwiz/data/wezboard | head -1
# Should show: 1a 01 21 00 ... (21 00 = 33, correct for "wezboard")
# NOT:         1a 01 20 00 ... (20 00 = 32, the corrupted value)
```

#### Verification

```bash
scripts/build.sh wezboard
```

| #   | Test                 | Steps                                         | Expected                        |
| --- | -------------------- | --------------------------------------------- | ------------------------------- |
| 1   | vim no longer panics | Run shannon, then run vim inside it           | vim opens normally              |
| 2   | XTGETTCAP works      | Run a program that queries terminal caps      | No panic, capabilities returned |
| 3   | No regression        | Use wezboard normally, open panes, browse web | Everything works as before      |

**Result:** Pass

vim opens normally from shannon without crashing the pane.

#### Conclusion

Recompiling the terminfo database with `tic -x` from the source file produced a
correct binary with the right name section length (33 bytes for "wezboard").

## Conclusion

The rename script's binary find-and-replace of "wezterm" → "wezboard" inside the
compiled terminfo file corrupted the header — the name grew by 1 byte but the
length field wasn't updated, shifting every subsequent field. The fix was
recompiling from the `.terminfo` source with `tic -x`. Lesson: never
find-and-replace inside compiled binary formats; recompile from source.
