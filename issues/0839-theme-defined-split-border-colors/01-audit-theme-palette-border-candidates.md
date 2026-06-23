# Experiment 1: Audit theme palette border candidates

## Description

Audit whether the proposed palette-derived split border defaults are a
defensible first heuristic before changing runtime behavior.

The issue goal is to avoid modifying bundled theme files while still making
unset split border colors feel theme-native. The proposed first heuristic is:

- focused split border: palette index 6;
- unfocused split border: palette index 8.

This experiment will inspect the currently bundled/generated theme data and
produce an audit table that answers:

- whether every bundled theme has usable palette entries at indices 6 and 8;
- whether Tokyo Night derives the required colors from those entries;
- whether palette 6 and palette 8 are visible against each theme background;
- whether any obvious outliers need a different fallback before runtime code is
  changed.

No app runtime behavior changes in this experiment. If the audit supports the
heuristic, the next experiment will implement the Swift/Zig/doc changes and will
include targeted tests for nullability and override behavior.

## Changes

- `issues/0839-theme-defined-split-border-colors/README.md`
  - Link this audit experiment as the first experiment.
- `issues/0839-theme-defined-split-border-colors/01-audit-theme-palette-border-candidates.md`
  - Record the audit design.
  - After running the audit, append the audit command, summary table, result,
    conclusion, and design-review notes.

No source code, theme files, generated theme output, vendoring metadata, or
website docs should be changed in this experiment.

## Verification

1. Confirm the experiment is documentation-only:

   ```bash
   git diff --name-only | rg -v '^issues/0839-theme-defined-split-border-colors/|^issues/README.md$'
   ```

   Pass: no output.

2. Confirm no bundled/generated theme files or theme dependency metadata were
   changed:

   ```bash
   git status --short -- ghostboard/zig-out ghostboard/build.zig.zon
   git diff --name-only | rg '(^ghostboard/zig-out/|ghostboard/build.zig.zon|themes/)'
   ```

   Pass: no output.

3. Run an audit over the available bundled theme files, using generated theme
   output or the downloaded `iterm2_themes` package only as read-only data:

   ```bash
   find ghostboard/zig-out/share/ghostty/themes -type f -maxdepth 1 | wc -l
   ```

   Then parse each theme's `background`, `palette = 6=...`, and
   `palette = 8=...`, calculate WCAG-style contrast between each candidate and
   the background, and record:

   - theme count;
   - count missing palette 6;
   - count missing palette 8;
   - Tokyo Night palette 6 and 8 values;
   - lowest focused contrast sample;
   - lowest unfocused contrast sample;
   - number of candidate outliers needing manual review.

   Pass: all bundled themes expose palette 6 and 8; Tokyo Night exposes focused
   `#7dcfff` and unfocused `#414868`; any low-contrast outliers are identified
   explicitly so the next experiment can either accept the simple heuristic or
   add a fallback.

4. Format markdown and check whitespace:

   ```bash
   prettier --check issues/0839-theme-defined-split-border-colors/README.md \
     issues/0839-theme-defined-split-border-colors/01-audit-theme-palette-border-candidates.md \
     issues/README.md
   git diff --check
   ```

   Pass: all checks succeed.

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Initial verdict: **Changes Required**.

- Required: the first experiment changed runtime behavior before performing the
  heuristic audit required by the issue.
- Required: the Zig/C nullability verification claim was too indirect because it
  did not require targeted tests for the two border color keys.

Fixes:

- Rewrote Experiment 1 as a documentation-only audit gate.
- Deferred Swift, Zig, and website documentation changes to the next experiment.
- Removed the indirect Zig/C nullability verification from this experiment.

Final verdict: **Approved**.

## Result

**Result:** Pass

The read-only audit inspected all generated bundled theme files in
`ghostboard/zig-out/share/ghostty/themes`.

Audit command:

```bash
find ghostboard/zig-out/share/ghostty/themes -maxdepth 1 -type f | wc -l
node <<'NODE'
const fs = require("fs");
const path = require("path");
const dir = "ghostboard/zig-out/share/ghostty/themes";
const files = fs
  .readdirSync(dir)
  .filter((name) => fs.statSync(path.join(dir, name)).isFile())
  .sort();

function hexToRgb(hex) {
  const match = String(hex).trim().match(/^#?([0-9a-fA-F]{6})$/);
  if (!match) return null;
  const n = parseInt(match[1], 16);
  return {
    r: (n >> 16) & 255,
    g: (n >> 8) & 255,
    b: n & 255,
    hex: "#" + match[1].toLowerCase(),
  };
}

function luminance(color) {
  const channel = (value) => {
    const s = value / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  };
  return (
    0.2126 * channel(color.r) +
    0.7152 * channel(color.g) +
    0.0722 * channel(color.b)
  );
}

function contrast(a, b) {
  const high = Math.max(luminance(a), luminance(b));
  const low = Math.min(luminance(a), luminance(b));
  return (high + 0.05) / (low + 0.05);
}

function parseTheme(file) {
  const text = fs.readFileSync(path.join(dir, file), "utf8");
  const result = { name: file, palette: {} };
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    let match = line.match(/^background\s*=\s*(#?[0-9a-fA-F]{6})\s*$/);
    if (match) result.background = hexToRgb(match[1]);
    match = line.match(
      /^palette\s*=\s*(\d+)\s*=\s*(#?[0-9a-fA-F]{6})\s*$/,
    );
    if (match) result.palette[Number(match[1])] = hexToRgb(match[2]);
  }
  return result;
}

const focusedFallbacks = [6, 14, 4, 12];
const rows = files.map(parseTheme);
const samples = rows.filter(
  (row) => row.background && row.palette[6] && row.palette[8],
);
const enriched = samples.map((row) => {
  const candidates = focusedFallbacks.map((index) => ({
    index,
    color: row.palette[index],
    contrast: row.palette[index]
      ? contrast(row.background, row.palette[index])
      : 0,
  }));
  const chosen =
    candidates.find(
      (candidate) => candidate.index === 6 && candidate.contrast >= 2,
    ) ||
    candidates
      .filter((candidate) => candidate.contrast >= 2)
      .sort((a, b) => b.contrast - a.contrast)[0] ||
    candidates.sort((a, b) => b.contrast - a.contrast)[0];
  return {
    name: row.name,
    background: row.background.hex,
    p6: row.palette[6].hex,
    p8: row.palette[8].hex,
    p14: row.palette[14]?.hex,
    p6Contrast: contrast(row.background, row.palette[6]),
    p8Contrast: contrast(row.background, row.palette[8]),
    chosenIndex: chosen.index,
    chosen: chosen.color?.hex,
    chosenContrast: chosen.contrast,
  };
});

console.log(
  JSON.stringify(
    {
      themeCount: files.length,
      parsedWithAllCandidates: samples.length,
      missingBackgroundCount: rows.filter((row) => !row.background).length,
      missingPalette6Count: rows.filter((row) => !row.palette[6]).length,
      missingPalette8Count: rows.filter((row) => !row.palette[8]).length,
      tokyoNight: enriched.filter((row) => row.name.startsWith("TokyoNight")),
      focusedPalette6Below15: enriched.filter((row) => row.p6Contrast < 1.5)
        .length,
      focusedPalette6Below20: enriched.filter((row) => row.p6Contrast < 2)
        .length,
      unfocusedPalette8Below15: enriched.filter((row) => row.p8Contrast < 1.5)
        .length,
      unfocusedPalette8Below20: enriched.filter((row) => row.p8Contrast < 2)
        .length,
      focusedBelow20AfterFallback: enriched.filter(
        (row) => row.chosenContrast < 2,
      ).length,
      focusedBelow15AfterFallback: enriched.filter(
        (row) => row.chosenContrast < 1.5,
      ).length,
      lowestFocusedPalette6: [...enriched]
        .sort((a, b) => a.p6Contrast - b.p6Contrast)
        .slice(0, 5),
      lowestUnfocusedPalette8: [...enriched]
        .sort((a, b) => a.p8Contrast - b.p8Contrast)
        .slice(0, 5),
      focusedFallbackExamples: enriched
        .filter((row) => row.chosenIndex !== 6)
        .slice(0, 5),
    },
    null,
    2,
  ),
);
NODE
```

Summary:

| Check                                             | Result |
| ------------------------------------------------- | -----: |
| Theme files audited                               |    534 |
| Themes with background, palette 6, and palette 8  |    534 |
| Missing background                                |      0 |
| Missing palette 6                                 |      0 |
| Missing palette 8                                 |      0 |
| Focused palette-6 candidates below 1.5 contrast   |      0 |
| Focused palette-6 candidates below 2.0 contrast   |     16 |
| Unfocused palette-8 candidates below 1.5 contrast |      0 |
| Unfocused palette-8 candidates below 2.0 contrast |    112 |

Tokyo Night samples:

| Theme            | Background | Palette 6 | Palette 8 | Palette 14 | Contrast 6 | Contrast 8 |
| ---------------- | ---------- | --------- | --------- | ---------- | ---------: | ---------: |
| TokyoNight       | `#1a1b26`  | `#7dcfff` | `#414868` | `#7dcfff`  |       9.96 |       1.91 |
| TokyoNight Day   | `#e1e2e7`  | `#007197` | `#a1a6c5` | `#007197`  |       4.26 |       1.85 |
| TokyoNight Moon  | `#222436`  | `#86e1fc` | `#444a73` | `#86e1fc`  |      10.33 |       1.80 |
| TokyoNight Night | `#1a1b26`  | `#7dcfff` | `#414868` | `#7dcfff`  |       9.96 |       1.91 |
| TokyoNight Storm | `#24283b`  | `#7dcfff` | `#4e5575` | `#7dcfff`  |       8.49 |       2.00 |

Lowest focused palette-6 contrast samples:

| Theme                | Background | Palette 6 | Contrast |
| -------------------- | ---------- | --------- | -------: |
| Everforest Light Med | `#efebd4`  | `#83c092` |     1.76 |
| GitHub               | `#f4f4f4`  | `#7cc4df` |     1.76 |
| Belafonte Day        | `#d5ccba`  | `#989a9c` |     1.77 |
| Horizon Bright       | `#fdf0ed`  | `#00cdcb` |     1.78 |
| Unikitty             | `#ff8cd9`  | `#9effef` |     1.80 |

Lowest unfocused palette-8 contrast samples:

| Theme               | Background | Palette 8 | Contrast |
| ------------------- | ---------- | --------- | -------: |
| Wryan               | `#101010`  | `#3d3d3d` |     1.75 |
| SeedFlip Canopy     | `#0f1714`  | `#2d453c` |     1.76 |
| Lab Fox             | `#2e2e2e`  | `#535353` |     1.77 |
| Apple System Colors | `#1e1e1e`  | `#464646` |     1.77 |
| Twilight            | `#141414`  | `#404040` |     1.78 |

The audit also tested the focused fallback candidates named by the issue:
palette 14, palette 4, and palette 12. Choosing palette 6 when it has at least
2.0 contrast, otherwise choosing the highest-contrast entry among 14, 4, and 12,
reduced focused candidates below 2.0 contrast from 16 themes to 2 themes. No
focused candidate fell below 1.5 contrast after fallback.

Example focused fallback changes:

| Theme                | Palette 6 | Palette 6 Contrast | Chosen Index | Chosen Color | Chosen Contrast |
| -------------------- | --------- | -----------------: | -----------: | ------------ | --------------: |
| GitHub               | `#7cc4df` |               1.76 |            4 | `#003e8a`    |            9.29 |
| Belafonte Day        | `#989a9c` |               1.77 |            4 | `#426a79`    |            3.69 |
| Everforest Light Med | `#83c092` |               1.76 |           12 | `#3a94c5`    |            2.81 |
| Unikitty             | `#9effef` |               1.80 |            4 | `#145fcd`    |            2.82 |
| Pro Light            | `#4ed2de` |               1.81 |            4 | `#3b75ff`    |            4.06 |

Verification:

- Documentation-only diff check passed: only the Issue 839 README and experiment
  result were dirty after the plan commit.
- Theme mutation check passed: `ghostboard/zig-out`, `ghostboard/build.zig.zon`,
  and theme paths were not changed.
- `git diff --check` passed.

## Completion Review

Reviewed by a fresh-context Codex adversarial subagent.

Initial verdict: **Changes Required**.

- Required: the audit verification was not reproducible because the result
  recorded only a command shape with comments, not the actual parser and
  contrast calculation.
- Required: the documentation-only verification note inaccurately said
  `issues/README.md` was dirty after the plan commit.

Fixes:

- Recorded the actual Node audit script used to parse themes and calculate
  contrast.
- Corrected the verification note to say only the Issue 839 README and
  experiment result were dirty after the plan commit.

Final verdict: **Approved**.

## Conclusion

The next experiment should implement theme-derived defaults without modifying
theme files:

- focused split border: use palette 6 when its contrast against the background
  is at least 2.0; otherwise choose the highest-contrast candidate from palette
  14, 4, and 12;
- unfocused split border: use palette 8 directly, because every theme defines it
  and every audited palette-8 candidate has at least 1.5 contrast while staying
  appropriately muted;
- preserve TokyoNight exactly: focused `#7dcfff`, unfocused `#414868`;
- preserve nullable config and explicit override behavior with targeted tests.
