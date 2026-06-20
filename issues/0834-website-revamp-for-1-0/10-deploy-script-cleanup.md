# Experiment 10: Deploy script cleanup (Cloudflare Pages) (Phase 1)

## Description

A Phase-1 deliverable: clean up the stale website deploy path. The issue
background and the Exp-1 audit both flagged that `scripts/deploy.sh`'s
`deploy_website()` is broken — it references things that don't exist:

- `bun run build:data` — **no such script** in `website/package.json` (scripts
  are `dev`, `build`, `build:icons`, `gen:references`, `import:vt`, `deploy`).
- `fly deploy --config website/fly.toml --dockerfile website/Dockerfile` —
  **neither `website/fly.toml` nor `website/Dockerfile` exists** (confirmed).

The **actual** deploy is Cloudflare Pages, defined canonically in
`website/package.json`:

```
"deploy": "bun run build && wrangler pages deploy dist"
```

So `scripts/deploy.sh website` currently fails immediately (`build:data` not
found). This experiment makes it run the real Cloudflare flow.

## Changes

1. **`scripts/deploy.sh`** — rewrite `deploy_website()` to delegate to the
   canonical package script (single source of truth), removing the `build:data`
   / Fly.io / `fly.toml` / `Dockerfile` references:

   ```sh
   deploy_website() {
     echo "==> Building and deploying website to Cloudflare Pages..."
     cd "$REPO_DIR/website"
     bun run deploy
   }
   ```

   The rest of the script (arg parsing, component dispatch, usage) is unchanged.

2. **`scripts/build.sh` / root `CLAUDE.md`** — only touched if they restate the
   stale Fly.io flow; the root `CLAUDE.md` deploy line
   ("`scripts/deploy.sh <comp>` Deploy a component. Components: website.") is
   already accurate and needs no change. (Verified during implementation.)

No website source, content, or fork changes. This is a build-tooling fix.

## Verification

1. **No stale references.** `scripts/deploy.sh` no longer mentions `build:data`,
   `fly`, `fly.toml`, or `Dockerfile`; `grep` confirms zero hits.
2. **Delegates to the real flow.** `deploy_website()` runs `bun run deploy` in
   `website/`, which is `bun run build && wrangler pages deploy dist`.
3. **Script is valid + dry-runnable to the deploy step.**
   `bash -n scripts/deploy.sh` passes (syntax); `scripts/deploy.sh` with
   no/unknown args prints usage and exits non-zero as before;
   `scripts/deploy.sh website` reaches the build step (an actual Cloudflare
   deploy needs `wrangler` auth and is not run in CI/here — verification stops
   at "invokes the correct command").
4. **No regressions.** `website/package.json`'s `deploy` script is unchanged and
   still the canonical path; other components' behavior in `deploy.sh` (only
   `website` exists) is unaffected.

A full pass removes the last broken/stale deploy path, leaving Cloudflare Pages
as the single documented deploy mechanism — one of Phase 1's loose ends from the
Exp-1 audit. Remaining Phase 1: search (Pagefind), versioning posture, the full
IA/sitemap.

## Design Review

Independent `adversarial-reviewer`. **Verdict: APPROVE** (no Required findings).
Confirmed: the current script is genuinely dead (`build:data` script +
`fly.toml`

- `Dockerfile` all absent; `set -euo pipefail` aborts on the missing script);
  `website/package.json:10` `deploy` is the canonical Cloudflare flow;
  delegating to `bun run deploy` (not re-implementing build+wrangler) is right.
  **Critical check passed:** the only `build:data` occurrence repo-wide is the
  line being removed — zero commit-data/blog/changelog consumers in
  `website/src` or `website/scripts`, no pre/postbuild hooks — so dropping it
  cannot ship a broken site. No other live stale Fly.io references (`build.sh`,
  both `CLAUDE.md`s are clean; remaining Fly.io hits are in closed/immutable
  issues, correctly left untouched). One cosmetic nit (wording) — non-blocking.
