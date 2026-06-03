+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"
+++

# Experiment 336: codepoint overrides

## Description

The resolver's `get_index` has one deferred placeholder left:
`// Codepoint overrides are deferred here`. Upstream's
`getIndexCodepointOverride` lets a config map **force a specific font** for
specific codepoint ranges (e.g. "use Fira Code for U+2190..U+2193"). It runs
**first** in `getIndex`, looks up the codepoint in a [`CodepointMap`], discovers
the mapped descriptor's font (caching the result), adds it to the collection,
and returns it — overriding the normal resolution. This experiment ports it,
completing `get_index`.

## Upstream behavior (`CodepointResolver.zig` `getIndexCodepointOverride`)

```zig
fn getIndexCodepointOverride(self, alloc, cp) !?Index {
    if (comptime font.Discover == void) return null;       // needs discovery
    const map = self.codepoint_map orelse return null;     // needs a map
    const cp_u21 = cast(u21, cp) orelse return null;
    const desc = map.get(cp_u21) orelse return null;       // map this codepoint?

    // Fast path: this descriptor was already discovered (or known-absent).
    const idx_ = self.descriptor_cache.get(desc) orelse idx: {
        // Slow path: discover the descriptor's font.
        const face = (try self.discover.?.discover(alloc, desc)).next() orelse {
            try self.descriptor_cache.put(alloc, desc, null);  // negative cache
            return null;
        };
        const idx = try self.collection.addDeferred(alloc, face, .{
            .style = .regular, .fallback = false,
            .size_adjustment = default_fallback_adjustment });
        try self.descriptor_cache.put(alloc, desc, idx);
        break :idx idx;
    };

    const idx = idx_ orelse return null;                   // known-absent
    // Discovery ignores presentation, so verify the glyph exists (any).
    if (self.collection.hasCodepoint(idx, cp, .{ .any = {} })) return idx;
    return null;
}
```

The `descriptor_cache` (a `Descriptor → ?Index` map) makes repeat lookups cheap
and — crucially — prevents re-discovering/re-adding the same font on every call.
A negative entry (`null`) records "this descriptor found nothing", avoiding
repeat failed searches. The added face is a **non-fallback** regular face with
the `ic_width` size adjustment. The final `hasCodepoint(..any)` check confirms
the discovered font actually has the glyph.

## Rust mapping (`roastty/src/font/codepoint_resolver.rs`)

- `CodepointResolver` gains `codepoint_map: Option<CodepointMap>` (with
  `set_codepoint_map`) and `descriptor_cache: HashMap<u64, Option<Index>>`
  (keyed by a descriptor hash; `Index` is `Copy`).
- `Descriptor::hashcode(&self) -> u64` (`discovery.rs`) — a `DefaultHasher` over
  the fields (the `f32` size via `to_bits`, variations via
  `(id, value.to_bits())`). The cache keys on this; the exact hash value is
  internal (not serialized), so a consistent Rust hash suffices (a faithful
  analog of upstream's `hashcode`).
- `fn get_index_codepoint_override(&mut self, cp: u32) -> Option<Index>`:
  - Gate: `self.discover_enabled` and a `codepoint_map`. `cp <= 0x10_FFFF`
    (always true for a scalar; the `u21` cast is implicit).
  - `let desc = self.codepoint_map.as_ref()?.get(cp)?.clone();` (clone to drop
    the map borrow before mutating the collection/cache).
  - `let key = desc.hashcode();`
  - Look up `self.descriptor_cache.get(&key).copied()`:
    - Hit ⇒ the cached `Option<Index>`.
    - Miss ⇒ discover: `let face = desc.discover_faces().next()`; on `None`
      cache `None` and return `None`; else
      `add_with_adjustment(face, Regular, false, IcWidth)`, cache `Some(idx)`,
      use it.
  - `let idx = cached?;` (negative ⇒ `None`).
  - `if self.collection.has_codepoint(idx, cp, PresentationMode::Any) { Some(idx) } else { None }`.
- `get_index`: call `self.get_index_codepoint_override(cp)` **first** (replacing
  the `// (Codepoint overrides are deferred here.)` comment); `return` it when
  `Some`.

## Scope / faithfulness notes

- **Ported**: the codepoint override — the map lookup, the descriptor discovery
  with the positive/negative `descriptor_cache`, the non-fallback collection
  insertion, and the `hasCodepoint(any)` verification — run first in
  `get_index`.
- **Faithful deviations**: the override uses the **general** `discover_faces`
  (consistent with Experiment 333's resolver fallback); a dedicated
  `discoverFallback` for the override is not needed (the map descriptor names a
  family, so the general match resolves it). The cache keys on a `u64`
  descriptor hash rather than the descriptor value (upstream keys on the
  descriptor with its `hashcode`); collisions are negligible and the behavior is
  identical.
- **Deferred**: the variation-axis score and variations application.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/font/discovery.rs`: add `Descriptor::hashcode`.
2. `roastty/src/font/codepoint_resolver.rs`: add `codepoint_map` +
   `set_codepoint_map`, `descriptor_cache`, `get_index_codepoint_override`; call
   it first in `get_index`. Import `CodepointMap`, `HashMap`.
3. Tests (in `codepoint_resolver.rs`):
   - `codepoint_override_forces_font`: a Menlo-only resolver with discovery
     enabled and a `CodepointMap` mapping `'A'..='Z'` to a different family
     (e.g. `"Helvetica"`). `get_index('A', Regular, Some(Text))` returns an
     `Index` that is **not** the Menlo primary `(Regular, 0)` — the override
     added the mapped font and returned it; without the map, `'A'` resolves to
     `(Regular, 0)`.
   - `codepoint_override_caches`: a second override lookup of another codepoint
     in the same range (`'B'`) returns the **same** index and does **not** grow
     the collection again (the `descriptor_cache` hit).
   - `codepoint_override_unmapped`: a codepoint outside the map (`'0'`) is not
     overridden (resolves normally to Menlo).
   - `hashcode_consistent` (`discovery.rs`): equal descriptors hash equal;
     differing ones (family, size, bold) hash differently.
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `get_index_codepoint_override` reproduces upstream's override (the map lookup,
  the cached discovery, the non-fallback insertion, the `any` verification), run
  first in `get_index`;
- the forces-font, caches, unmapped, and hashcode tests pass, and the existing
  resolver tests still pass;
- the variation-axis score and variations stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment is **partial** if the override's discovery cannot be made
host-deterministic (the cache/lookup logic is still exercised).

The experiment **fails** if the map lookup, the cache, the insertion, or the
verification diverges from upstream, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and found **no Required
findings**. It confirmed the design matches upstream on the important behavior:
the override lookup runs in the right position (after disabled-style
normalization, **before** the sprite handling, the presentation logic, the exact
collection lookup, and the fallback discovery); the positive/negative cache
semantics are faithful (`Some(idx)` reuses the loaded override face, `None`
records a failed descriptor discovery and avoids repeating it); the override
face is added as `Style::Regular`, `fallback = false`, with
`SizeAdjustment::IcWidth` (matching upstream `addDeferred`); the final
`collection.has_codepoint(idx, cp, PresentationMode::Any)` check is correct
(upstream's "discovery ignores presentation, verify glyph presence"); gating on
`discover_enabled` is the right Rust analog of upstream's optional
`self.discover`; cloning the descriptor before mutating resolver state is the
right way to end the map borrow; and the `u64` cache key is acceptable
(upstream's descriptor cache equality also effectively compares descriptor
hashcodes, so the collision risk is not a new semantic gap).

Review artifacts:

- Prompt: `logs/codex-review/20260603-124144-442921-prompt.md`
- Result: `logs/codex-review/20260603-124144-442921-last-message.md`

## Result

**Result:** Pass

Codepoint overrides land — `get_index` is now a complete port of upstream
`getIndex`.

- `roastty/src/font/discovery.rs`: `Descriptor::hashcode(&self) -> u64` (a
  `DefaultHasher` over all fields, floats by bit pattern) — the descriptor-cache
  key.
- `roastty/src/font/codepoint_resolver.rs`: the resolver gained
  `codepoint_map: Option<CodepointMap>` (`set_codepoint_map`) and
  `descriptor_cache: HashMap<u64, Option<Index>>`;
  `get_index_codepoint_override` runs **first** in `get_index` (after the
  disabled-style normalization, before the sprite check): it looks up and clones
  the mapped descriptor, consults the cache (`.get(&key) .copied()`
  distinguishes a miss from a negative-cache hit), discovers + adds the font on
  a miss (`Style::Regular`, non-fallback, `IcWidth`), caches the result
  (positive or negative), and returns the index only if it
  `has_codepoint(.., Any)`. The module/`get_index` docs note the override and
  the now-complete port.

Tests: `codepoint_override_forces_font` (an `'A'..='Z'` → `"Helvetica"` map
makes `get_index('A')` return the override face, not the Menlo primary
`(Regular, 0)`; without the map it is `(Regular, 0)`),
`codepoint_override_caches` (`'B'` in the same range is a cache hit — the same
index, no new face), `codepoint_override_unmapped` (`'0'` outside the map
resolves to Menlo), `hashcode_consistent` (equal descriptors hash equal;
family/size/bold differences hash differently).

Gate results:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty` → 2735 passed, 0 failed (+4, no regressions).
- `cargo build -p roastty` → no warnings.
- No-`ghostty`-name gates clean; `git diff --check` clean.

## Conclusion

The resolver's `get_index` is a **complete** port of upstream `getIndex`:
codepoint overrides → sprite check → UCD presentation default → exact collection
lookup → regular-style retry → discovery fallback (`discoverFallback`, CJK
included) → last-resort `Any`. The codepoint resolution path — sprite rendering,
color detection, discovery, and the resolver — is now functionally whole.

The remaining font work is the deferred **variation-axis** score refinement and
**variations** application (for variable fonts), and the **shaper** (shape
calls, glyph placement) — the largest remaining subsystem.

## Completion Review

Codex reviewed the completed implementation and result and **approved** with
**no Required findings**. It verified: the override placement matches upstream
(after disabled-style normalization, before sprite handling); the cache
semantics are correct (`HashMap<u64, Option<Index>>` + `.get(&key).copied()`
cleanly distinguishes a miss from a negative-cache hit, positive hits reuse the
existing index without re-adding, negative hits avoid rediscovery); the
insertion matches upstream (`Style::Regular`, `fallback = false`,
`SizeAdjustment::IcWidth`); the final
`has_codepoint(idx, cp, PresentationMode::Any)` matches upstream's
presentation-agnostic verification; the descriptor cloning is sound (it ends the
map borrow before mutating the resolver); and `hashcode` is total/reflexive
(floats hashed by bit pattern). The `u64` cache key has the same practical
collision profile already accepted in the design review (upstream's
descriptor-cache equality also reduces to hash equality). No Optional findings.

Review artifacts:

- Result review: `logs/codex-review/20260603-124525-355071-last-message.md`
