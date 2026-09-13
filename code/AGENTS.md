# AGENTS.md — product source

Product code is grouped as `code/<product>/rs/` for Rust and
`code/<product>/ts/` for Bun/TypeScript. Root `Cargo.toml` and `package.json`
own the shared workspaces; names remain unprefixed crates or
`@astrohacker/<package>`.

The shell formerly at `code/termsurf/rs/ahsh` now belongs to NuTorch's isolated
workspace at `code/nutorch/rs/shell`. Build it with `scripts/build.nu nutorch`.
Fixtures
`code/termsurf/ts/test-html` and `code/termsurf/ts/slow-load` are private Bun
workspace members for shared formatting, linting and typechecking; they are
not shipping products.

NuTorch is also isolated at `code/nutorch/rs/`; run Cargo after changing into
that directory to load its LibTorch configuration. Its React Router site is the
root Bun workspace member `code/nutorch/ts/ntcom`. Tensor commands are built into
the native shell and exposed through `use torch`; the former Nu client module and daemon are retired in Issue
26091215074416 Experiment 3. Experiment 9 promotes the accepted port to this canonical
path and the existing Cloudflare Pages publisher. Experiment 11 adopts shared
UI, Austin Night and Space Rain; preserve content, interactions and the colorful
3D homepage hero. The page is dark-only; PNG favicons follow OS preference.
Issue 26090811168867 Experiment 2 makes `scripts/deploy-ntcom.nu` target AWS
(--aws is equivalent). Domain provisioning and DNS cutover were separately
approved and applied; human acceptance is recorded. Legacy Pages requires explicit --cloudflare and human
confirmation. See infra guidance; routine publishing never changes DNS.

## Product boundaries

WebBuf is integrated at `code/webbuf/rs/` and `code/webbuf/ts/`, in the root
Cargo and Bun workspaces. Public `@webbuf/*`, `webbuf` and `webbuf_*` names are
preserved exceptions to internal naming. Its `ts/wbcom` site uses React Router 8
with static prerendering and typed Link/href navigation. Experiment 12 adopts
shared UI, Austin Night, persistent SpaceRain and canonical logo assets.
See `code/webbuf/AGENTS.md` for commands.

TermPlot's single-page installation site is `code/termplot/ts/tpcom`, a separate
React Router SPA using shared UI. `bun run dev:tpcom` binds 127.0.0.1:3512.
Experiment 5 qualified localhost. Experiment 6 adds static S3/CloudFront
deployment via `scripts/deploy-tpcom.nu`, with separate infrastructure and DNS
approval gates. No executable release inclusion.

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

Internal TypeScript consumers declare WebBuf dependencies as `workspace:^`,
including dependencies referenced by exported types. Do not pin npm versions
or rely on hoisting. Run `bun run build:webbuf` from the root after local WebBuf
edits, then rebuild downstream libraries before apps; no publication or version
bump is needed. Entry points remain `dist/`, not TypeScript source aliases.

Use WebBuf types as product-domain types: `WebBuf` for variable bytes,
`FixedBuf<N>` for fixed material, `@webbuf/numbers` for fixed-width wire
integers, and `@webbuf/rw` for sequential I/O. Convert Node/mysql2
`Buffer`/`Uint8Array` once at external edges. mysql2 `Buffer` is allowed only
inside Drizzle custom-type conversion. Wipe fixed secret buffers when done.

Use `BufReader.readU16LE`/`readU32LE`/`readU64LE`/`readU128LE`/`readU256LE`
and matching BufWriter methods for sequential fixed-width little-endian fields.
`BufWriter.writeVarIntU64BE` is a big-endian CompactSize variant, not Bitcoin's
little-endian encoding or protobuf LEB128. Keep TermSurf/protobuf varints separate.

## TermSurf Bun binaries

`ahcalc`, independent `termplot`, and `ahebx` use a dedicated server process for UDS
and HTTP; foreground binaries are clients. `--version` and `--help` must exit
before requiring TermSurf. Compiled output is one `dist/<binary>`, not a
separate server executable. Socket names and product-specific behavior live in
package guidance.

## Verification and hygiene

Run Cargo workspace checks from the monorepo root, except for the nested
workspaces above. Bun scripts are authoritative in root and package manifests;
Prettier is installed per workspace. Root `format`, `format:check`, `lint`,
`lint:fix`, and `typecheck` first check/fix the shared code-level tooling,
then fan out to the corresponding workspace scripts. `code/tsconfig.json`
owns the shared TypeScript configs; it inherits the same strict base.
Use shared `skipLibCheck: true`: dependency declarations stay unpatched and
their internal consistency is not checked. This also skips first-party `.d.ts`
consistency checks; source implementations and consumers remain strictly checked.
Do not replace this policy with dependency patches or source suppressions.
Oxlint enables only common ES2022/runtime globals by default. App/UI browser
files, server/CLI/tool/test files, explicit browser tests and workers use
separate environment scopes; server-only overrides take precedence over app
paths. TypeScript compiler declarations own TS global checking; maintained
JS/MJS/CJS helpers also enforce no-undef. Legacy ESLint disable directives are
not honored. Do not reintroduce blanket runtime globals or suppression comments.
WebBuf and its migrated consumers expose `format:check` with the same targets
and ignore policy as `format`, without rewriting files. Keep generated build,
binding and embedded-version outputs out of formatting, not their handwritten
generators. Package-specific exclusions document provenance; do not exclude
maintained source or disable lint rules to obtain a passing check.

Issue 26091023458065 Experiment 1 has human-approved exceptions in the shared
Oxlint config: framework route Response/data throws, two existing BCH static
class APIs, BCH opcode aliases, and two deliberate non-Error negative-test files.
Keep them limited to the documented types/files. Do not generalize them to other
classes, routes' arbitrary thrown values, or entire test trees. Verify scope with
`nu scripts/lib/test-typescript-coverage.nu --exceptions`.

Do not commit dependencies, generated Vite assets, or secrets. Do not recreate
the deleted `code/website`; the company site is
`code/astrohacker/ts/ahcom`, while `termsurf.com` and its public `/docs`
live in `code/termsurf/ts/tscom`.
