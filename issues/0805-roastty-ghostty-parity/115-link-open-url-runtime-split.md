# Experiment 115: Link Open URL Runtime Split

## Description

`RUNTIME-012` currently combines several notification, bell, and link behaviors
into one broad `Gap`. Roastty already has focused runtime tests for a narrower
link/open-url slice: configured URL link matching, renderer link highlighting
rules, explicit open-url action dispatch, and copy-url-to-clipboard binding
behavior.

Pinned Ghostty's `link`, `link-url`, and `link-previews` fields define URL
matching, link actions, and preview policy. This experiment will split the
already-proven link/open-url runtime slice out of `RUNTIME-012` while keeping
bell, command-finish notifications, app notifications, hover/cursor UI, and
context/menu link flows unclosed.

The intended result is:

- `RUNTIME-012A`: `Oracle complete` for link URL matching, renderer
  link-highlight rules, explicit open-url runtime action dispatch, and
  copy-url-to-clipboard binding behavior.
- `RUNTIME-012B`: `Gap` for bell actions, command-finish notifications,
  app-notifications, link hover/cursor UI, link previews in the real app, and
  context/menu link flows.

## Changes

- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Replace broad `RUNTIME-012` with narrower `RUNTIME-012A` and `RUNTIME-012B`
    rows.
  - Update `EXPECTED_IDS` to require the new row split.
  - Mark `RUNTIME-012A` `Oracle complete` only with evidence from existing
    link/open-url guard families.
  - Keep `RUNTIME-012B` as `Gap` with explicit missing evidence for bell,
    notification, hover/cursor UI, preview UI, and context/menu behavior.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate via `config_runtime_inventory.py` so `CFG-223` reflects the new
    row counts.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning that deterministic link/open-url runtime behavior should be
    separated from GUI notification and hover/menu gaps.
  - Update the experiment index as the result is recorded.

## Verification

Pass criteria:

- The runtime inventory validates the new manifest and reports the expected row
  split:

  ```sh
  PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
    --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
    --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
  ```

- Focused link/open-url runtime tests pass:

  ```sh
  cargo test --manifest-path roastty/Cargo.toml surface_open_url
  cargo test --manifest-path roastty/Cargo.toml surface_binding_action_copy_url_to_clipboard
  cargo test --manifest-path roastty/Cargo.toml renderer_link
  cargo test --manifest-path roastty/Cargo.toml config_link_url_finalize
  ```

- A matrix assertion proves:
  - old `RUNTIME-012` is absent;
  - `RUNTIME-012A` is `Oracle complete`;
  - `RUNTIME-012A` evidence and guard cells name the open-url, copy-url, and
    renderer-link guard families;
  - `RUNTIME-012B` remains `Gap`;
  - `RUNTIME-012B` retains bell, command-finish notifications, app
    notifications, hover/cursor UI, link previews, and context/menu link flows;
  - `CFG-223` remains `Gap`.

  ```sh
  PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
  from pathlib import Path

  inventory = Path("issues/0805-roastty-ghostty-parity/config-runtime-inventory.md").read_text()
  matrix = Path("issues/0805-roastty-ghostty-parity/config-matrix.md").read_text()

  rows = {}
  for line in inventory.splitlines():
      if not line.startswith("| RUNTIME-"):
          continue
      cells = [cell.strip() for cell in line.strip("|").split("|")]
      rows[cells[0]] = cells

  assert "RUNTIME-012" not in rows, rows["RUNTIME-012"]
  assert len(rows) == 24, len(rows)
  assert rows["RUNTIME-012A"][5] == "Oracle complete", rows["RUNTIME-012A"]
  for term in ("surface_open_url", "copy_url_to_clipboard", "renderer_link"):
      assert term in rows["RUNTIME-012A"][6], (term, rows["RUNTIME-012A"])
      assert term in rows["RUNTIME-012A"][9], (term, rows["RUNTIME-012A"])
  assert rows["RUNTIME-012A"][7].startswith("None"), rows["RUNTIME-012A"]
  assert rows["RUNTIME-012B"][5] == "Gap", rows["RUNTIME-012B"]
  behavior = rows["RUNTIME-012B"][1]
  for term in ("bell", "command-finish", "app-notifications", "hover", "previews", "context/menu"):
      assert term in behavior, (term, rows["RUNTIME-012B"])
  cfg223 = next(line for line in matrix.splitlines() if line.startswith("| CFG-223 "))
  assert "| Gap " in cfg223, cfg223
  PY
  ```

- Markdown and diff hygiene pass:

  ```sh
  prettier --check issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/115-link-open-url-runtime-split.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md \
    issues/0805-roastty-ghostty-parity/config-runtime-inventory.md

  git diff --check
  ```

## Design Review

Fresh-context Codex adversarial reviewer `Galileo` returned **Approved** with no
required findings.

Optional note:

- The planned `RUNTIME-012A` wording is acceptable only if implementation keeps
  it narrow. Existing tests support default URL config finalization, renderer
  link highlighting, explicit open-url action dispatch, and OSC8
  copy-url-to-clipboard binding behavior. They do not prove
  click/context/menu/preview/hover UI flows, which must remain in the gap row.

Nit:

- The row-count assertion is hard-coded. This is acceptable for this matrix
  assertion, but it must be updated if nearby inventory rows change before
  implementation.

## Result

**Result:** Pass

Split the broad notification/link runtime row into two rows:

- `RUNTIME-012A` is `Oracle complete` for link URL matching, renderer
  highlighting, explicit open-url dispatch, and copy-url binding effects.
- `RUNTIME-012B` remains `Gap` for bell, command-finish notifications,
  app-notifications, hover/cursor UI, link previews, and context/menu link
  flows.

The regenerated runtime inventory now reports 24 runtime rows, 17
oracle-complete rows, 18 closed rows, and 6 gap rows. `CFG-223` remains `Gap`,
as intended.

Verification passed:

```text
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
# runtime_rows=24 oracle_complete=17 closed=18 audit_covered=0 incomplete=6 gap=6 cfg223=Gap

cargo test --manifest-path roastty/Cargo.toml surface_open_url
# 4 passed

cargo test --manifest-path roastty/Cargo.toml surface_binding_action_copy_url_to_clipboard
# 3 passed

cargo test --manifest-path roastty/Cargo.toml renderer_link
# 4 passed

cargo test --manifest-path roastty/Cargo.toml config_link_url_finalize
# 1 passed
```

The matrix assertion also passed, proving old `RUNTIME-012` is absent,
`RUNTIME-012A` names the open-url/copy-url/renderer-link guard families in its
evidence and guard cells, `RUNTIME-012B` remains `Gap`, and `CFG-223` remains
`Gap`.

## Conclusion

The deterministic link/open-url runtime slice is now guarded separately. The
remaining notification/link gap is narrower and explicitly limited to bell,
command-finish notifications, app notifications, hover/cursor UI, link previews,
and context/menu link flows.

## Completion Review

Fresh-context Codex reviewer `Pascal` initially returned **CHANGES REQUIRED**:

- **Required:** `RUNTIME-004G` still referenced old `RUNTIME-012` for
  link-specific context-menu behavior after the split.
- **Optional:** `RUNTIME-012A` named `link-previews` in its Ghostty reference,
  which could be misread as preview UI coverage even though preview UI remains
  in `RUNTIME-012B`.

Fix:

- Updated the `RUNTIME-004G` missing-evidence cross-reference to `RUNTIME-012B`.
- Removed `link-previews` from `RUNTIME-012A`'s Ghostty reference so the
  oracle-complete row stays limited to deterministic link/open-url behavior.
- Regenerated the runtime inventory and config matrix, and strengthened the
  matrix assertion to check the cross-reference and preview wording.

Re-review verdict: **Approved**. Fresh-context reviewer `James` confirmed the
`RUNTIME-004G` cross-reference now points to `RUNTIME-012B`, `RUNTIME-012A` no
longer names `link-previews`, preview ownership remains in `RUNTIME-012B`, and
no result commit had been made before approval.
