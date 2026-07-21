# Fork Patches

Read this before any work under `patches/` or ignored `forks/`.

## Fork change contract (MUST)

These rules apply to **every** product fork (Chromium, WebKit,
Ghostty, Gecko, Nushell, and Reedline when source is edited). There is no
"local-only" fork edit. Per-fork `AGENTS.md` files restate this contract and
add local paths and branch prefixes only.

1. **Never modify a fork without logging a monorepo patch record.**
   Ignored `forks/` trees are not the durable product record. Tracked
   archives under `patches/<fork>/patches/` are. Log a patch whenever the
   fork is intentionally changed — whether or not that change ships in the
   next Homebrew release.

2. **Branch names include issue id and experiment number.**
   Create (or switch to) a branch that encodes both before committing.
   Generic pattern:

   ```text
   issue-{ISSUE_ID}-exp{N}-{short-slug}
   ```

   Forks may prefix platform or version tokens (for example Chromium
   `{version}-issue-{ID}-exp{N}-…`, Gecko `{short8}-issue-{ID}-exp{N}-…`)
   but **must not omit** the issue id or `exp{N}`.

3. **One experiment → branch name for that experiment's work.**
   Do not pile unrelated experiments onto an issue-only branch name.
   Cumulative archives under `issue-{ID}/` may still hold ordered
   `0001…NNNN` patches from multiple experiments; the **branch name** still
   names the experiment you are working on.

4. **After each intentional commit on that branch, update the monorepo in
   the same work unit:**
   - `git format-patch` into `patches/<fork>/patches/issue-{ISSUE_ID}/`
     (next ordered `NNNN-….patch`, or regenerate the full series when that
     fork's `README.md` says the archive is cumulative-from-base).
   - Update that fork's `README.md` Active/Current pin (HEAD, tree, count,
     digests as applicable).
   - Update `patches/release-manifest.json` for that fork (head/tree/count/
     archive digest) whenever the shipped series changes.
   - Record branch, base, HEAD/tree, patch path(s), and digests in the
     **current** issue experiment file.

5. **Incomplete work definition.** A fork change is **not done** if any of
   these are still true:
   - branch lacks issue id or `exp{N}`;
   - new commits exist in `forks/` with no matching tracked `.patch`;
   - manifest/README pin is stale vs fork tip;
   - patch files are only untracked or uncommitted in the monorepo.

6. **Pin-only forks (e.g. Reedline default).** A tip pin with zero product
   patches is allowed only when there is **no** intentional Astrohacker
   source edit. The moment you edit source, rules 1–5 apply in full. Do not
   invent empty no-op `.patch` files for pin-only state.

## Navigation

- Shared policy and merge-upstream portfolio notes: [`README.md`](./README.md).
- **Release authority:** [`release-manifest.json`](./release-manifest.json).
  Do not invent the cumulative shipped series from “latest” issue folder
  names.
- Per-fork reconstruction detail (bases, archives, apply/generate/verify):
  each fork’s `README.md`.
- Local hygiene and fork-specific hazards: each fork’s `AGENTS.md` when
  present. Those files **obey** this contract; they do not replace it.

## Forks with patch (or pin) workspaces

| Fork | Notes |
| --- | --- |
| `chromium/` | Engine; large cumulative archives |
| `webkit/` | Engine |
| `ghostty/` | Host terminal (`ahterm`) |
| `gecko/` | Optional engine; not in Homebrew ship set |
| `nushell/` | Shell product fork |
| `reedline/` | Tip pin only (no product patch unless source is edited) |

Working trees stay under ignored `forks/`; only archives and docs live here.
