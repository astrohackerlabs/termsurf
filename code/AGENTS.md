# AGENTS.md — product source

Product code is grouped as `code/<product>/rs/` for Rust and
`code/<product>/ts/` for Bun/TypeScript. Root `Cargo.toml` and `package.json`
own the shared workspaces; names remain unprefixed crates or
`@astrohacker/<package>`.

`code/termsurf/rs/ahsh` and `ahtch` are excluded nested Cargo workspaces.
Build `ahsh` with its manifest. Run Cargo for `ahtch` after `cd
code/termsurf/rs/ahtch` so its LibTorch `.cargo/config.toml` applies. Fixtures
`code/termsurf/ts/test-html` and `code/termsurf/ts/slow-load` are not Bun
workspace members.

## Product boundaries

- `ahebx` is the TermSurf EBX1 pool client; `ebxmine` is the EBX2 validating
  mine; `ebxcom` is the pool/app.
- ahcom exposes oRPC `/api`; ebxcom exposes tRPC `/trpc`.
  kpnode/kpcom/ahkey source is archived off HEAD.
- ahnexus is a Vite SPA backed by Rust `ahnexus`; Nexus wire code stays in
  Rust. Other React apps use React Router framework mode as configured.
- ahcom owns its MySQL schema with push-only Drizzle. ebxcom/ebxdb own the
  EarthBucks MySQL schema. TermSurf apps, UI, and static sites have no DB.
- `ah-chromiumd` consumes optional
  `--render-surface-service=<NAME>` before Chromium argument handling.

Package-specific commands and traps belong in the nearest package guidance.

## React Router and UI

Framework-mode apps use their existing `@react-router/dev` configuration,
typed route modules, and generated `+types`; never hand-edit `.react-router/`.
Internal links use typed `href(...)`; external links use plain anchors.

ahcom page reads use loaders backed by models, and writes use its browser
oRPC client—never product route actions. Apps without an RPC mutation surface
may use RR actions. Keep server-only dependencies and environment values out of
browser graphs. Bun SSR uses `renderToReadableStream`.

Use `@astrohacker/ui` primitives and shared Austin Night tokens instead of
forking controls or theme blocks into apps. Form dropdowns use kit `Select`,
not native select widgets, because TermSurf Chromium panes do not host OS popup
widgets. Centered modals use kit `Dialog`; app-specific pages and CSS remain in
the app. Each app must include the UI source in Tailwind scanning before
importing `@astrohacker/ui/styles.css`; preserve its existing relative
`@source` path.

## Binary data

Use WebBuf types as product-domain types: `WebBuf` for variable bytes,
`FixedBuf<N>` for fixed material, `@webbuf/numbers` for fixed-width wire
integers, and `@webbuf/rw` for sequential I/O. Convert Node/mysql2
`Buffer`/`Uint8Array` once at external edges. mysql2 `Buffer` is allowed only
inside Drizzle custom-type conversion. Wipe fixed secret buffers when done.

`BufWriter.writeVarIntU64BE` is Bitcoin CompactSize, not protobuf LEB128; keep
TermSurf/protobuf varint encoding separate.

## TermSurf Bun binaries

`ahcalc`, `ahplt`, and `ahebx` use a dedicated server process for UDS
and HTTP; foreground binaries are clients. `--version` and `--help` must exit
before requiring TermSurf. Compiled output is one `dist/<binary>`, not a
separate server executable. Socket names and product-specific behavior live in
package guidance.

## Verification and hygiene

Run Cargo workspace checks from the monorepo root, except for the nested
workspaces above. Bun scripts are authoritative in root and package manifests;
Prettier is installed per workspace, and root `format` only fans out to
workspace `format` scripts.

Do not commit dependencies, generated Vite assets, or secrets. Do not recreate
the deleted `code/website`; the company site is
`code/astrohacker/ts/ahcom`, while `termsurf.com` and its public `/docs`
live in `code/termsurf/ts/tscom`.
