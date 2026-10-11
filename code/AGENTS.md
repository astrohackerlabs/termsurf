# AGENTS.md — product source

Product code is grouped as `code/<product>/rs/` for Rust and
`code/<product>/ts/` for Bun/TypeScript. Root `Cargo.toml` and `package.json`
own the shared workspaces; names remain unprefixed crates or
`@astrohacker/<package>`.

The shell formerly at `code/termsurf/rs/ahsh` now belongs to NuTorch's isolated
workspace at `code/nutorch/rs/shell`. Build it with `scripts/build.nu nutorch`.
The browser client formerly at `code/termsurf/rs/ahweb` now belongs to Roamari
at `code/roamari/rs/roamari`. Build it with `scripts/build.nu roamari`. The
engine helper is `roamari-chromiumd` at `code/roamari/rs/roamari-chromiumd`.
Fixtures
`code/termsurf/ts/test-html` and `code/termsurf/ts/slow-load` are private Bun
workspace members for shared formatting, linting and typechecking; they are
not shipping products.

NuTorch is also isolated at `code/nutorch/rs/`; run Cargo after changing into
that directory to load its LibTorch configuration. Its React Router site is the
root Bun workspace member `code/nutorch/ts/nutorch-com`. Tensor commands are built into
the native shell and exposed through `use torch`; the former Nu client module and daemon are retired in Issue
26091215074416 Experiment 3. Experiment 9 promotes the accepted port to this canonical
path and the existing Cloudflare Pages publisher. Experiment 11 adopts shared
UI, Austin Night and Space Rain; preserve content, interactions and the colorful
3D homepage hero. The page is dark-only; PNG favicons follow OS preference.
Issue 26090811168867 Experiment 2 makes `scripts/deploy-nutorch-com.nu` target AWS
(--aws is equivalent). Domain provisioning and DNS cutover were separately
approved and applied; human acceptance is recorded. Legacy Pages requires explicit --cloudflare and human
confirmation. See infra guidance; routine publishing never changes DNS.

## Product boundaries

Ryan's personal homepage and blog are at `code/rxc/ts/ryanxcharles-com`, a root Bun
workspace using shared UI and SpaceRain. `bun run dev:ryanxcharles-com` serves port 3530.
It retains Cloudflare Pages project `rxc`; `scripts/deploy-ryanxcharles-com.nu` is its
separate publisher. CV and personal agent skills were not imported.

WebBuf is integrated at `code/webbuf/rs/` and `code/webbuf/ts/`, in the root
Cargo and Bun workspaces. Public `@webbuf/*`, `webbuf` and `webbuf_*` names are
preserved exceptions to internal naming. Its `ts/webbuf-com` site uses React Router 8
with static prerendering and typed Link/href navigation. Experiment 12 adopts
shared UI, Austin Night, persistent SpaceRain and canonical logo assets.
See `code/webbuf/AGENTS.md` for commands.

TermPlot's single-page installation site is `code/termplot/ts/termplot-com`, a separate
React Router SPA using shared UI. `bun run dev:termplot-com` binds 127.0.0.1:3512.
Experiment 5 qualified localhost. Experiment 6 adds static S3/CloudFront
deployment via `scripts/deploy-termplot-com.nu`, with separate infrastructure and DNS
approval gates. No executable release inclusion.

RedVim's site is `code/redvim/ts/redvim-com`, a React Router SPA using shared
UI, Austin Night and SpaceRain. Use `bun run dev:redvim-com` at 127.0.0.1:3518
and `bun run build:redvim-com`. Local HTTPS is `https://redvim.test`. Its
production preview uses port 3519. Issue 26091919387750 qualifies the page,
AWS hosting at redvim.com, boxed logo and live astrohacker-com product card
through Experiments 2–5.

Compubutton's unpublished website is `code/compubutton/ts/compubutton-com`, a one-route
React Router SPA using shared UI. `bun run dev:compubutton-com` binds 127.0.0.1:3516.
Local HTTPS is `https://compubutton.test`. It is a test canvas for later 3D
work; publication, DNS and analytics are not part of the current site.

Sarcasmotron's unpublished website is `code/sarcasmotron/ts/sarcasmotron-com`, a one-route
React Router SPA using shared UI. `bun run dev:sarcasmotron-com` binds 127.0.0.1:3520.
Local HTTPS is `https://sarcasmotron.test`. Publication, DNS, analytics, product
mark and classifier UI are not part of the current blank landing.

Roamari's one-page installation site is `code/roamari/ts/roamari-com`, a React Router
SPA using shared UI. `bun run dev:roamari-com` binds 127.0.0.1:3522. Local HTTPS is
`https://roamari.test`. Production is S3/CloudFront via `scripts/deploy-roamari-com.nu`
(`ah deploy roamari-com`).

QuikOpen's one-page install site is `code/quikopen/ts/quikopen-com`, a React Router
SPA using shared UI. `bun run dev:quikopen-com` / `ah dev quikopen-com` binds 127.0.0.1:3524.
Local HTTPS is `https://quikopen.test`. Production is `https://quikopen.com`
via `ah deploy quikopen-com`. The opener app remains `code/quikopen/ts/quik` on 3465.
The astrohacker-com product catalog includes its image viewer card. Umami is not part of
the current site.

Discalc's one-page research-log site is `code/discalc/ts/discalc-com`, a React
Router SPA using shared UI. `bun run dev:discalc-com` / `ah dev discalc-com` binds
127.0.0.1:3526. Local HTTPS is `https://discalc.test`. It is hosted on Cloudflare Pages
(project `discalc`) via `ah deploy discalc-com`. The production domain is to be
`https://discalc.com`, with discretecalculus.com redirecting there; DNS is on Cloudflare.

OpenSPV is a BCH wallet at `code/openspv/ts/openspv-com` (Issue 26100812550120): React
Router, Express, oRPC at `/api/v1` and a local MySQL database `openspv` (`setup:local`).
`ah dev openspv-com` binds 127.0.0.1:3528, with Vite HMR on 3529. Local HTTPS is
`https://openspv.test`. It is not deployed yet.

- Browser miners are EBX1 pool clients; `ebxmine` is the EBX2 validating
  mine; `earthbucks-com` is the pool/app.
- astrohacker-com exposes oRPC `/api`; earthbucks-com exposes the EarthBucks
  v1 oRPC API at `/api/v1`. Its tRPC `/trpc` API was removed in Issue
  26100317465696.
  kpnode/kpcom/ahkey source is archived off HEAD.
- ahnexus is a Vite SPA backed by Rust `ahnexus`; Nexus wire code stays in
  Rust. Other React apps use React Router framework mode as configured.
- astrohacker-com owns its MySQL schema with push-only Drizzle. earthbucks-com/ebxdb own the
  EarthBucks MySQL schema. TermSurf apps, UI, and static sites have no DB.
- `roamari-chromiumd` consumes optional
  `--render-surface-service=<NAME>` before Chromium argument handling.

Package-specific commands and traps belong in the nearest package guidance.

## React Router and UI

**OS motion preference:** Only SpaceRain may consult or respond to
`prefers-reduced-motion`. No other component, animation, transition, glow or
interaction may use the operating system's motion preference, whether through
CSS media queries, JavaScript or shared helpers. Outside SpaceRain, System must
not inherit the OS setting. Explicit app-level choices are separate and do not
authorize consulting the OS. Do not add OS-motion gating as an automatic
accessibility enhancement.

Framework-mode apps use their existing `@react-router/dev` configuration,
typed route modules, and generated `+types`; never hand-edit `.react-router/`.
Internal links use typed `href(...)`; external links use plain anchors.

astrohacker-com page reads use loaders backed by models, and writes use its browser
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

`ahcalc` and independent `termplot` use a dedicated server process for UDS
and HTTP; foreground binaries are clients. Independent `quikopen` is one
process per overlay: it binds its own loopback HTTP port and has no UDS.
`--version` and `--help` must exit
before requiring TermSurf. Compiled output is one `dist/<binary>`, not a
separate server executable. Socket names and product-specific behavior live in
package guidance.

## Verification and hygiene

Run Cargo workspace checks from the monorepo root, except for the nested
workspaces above. Bun scripts are authoritative in root and package manifests;
Oxfmt 0.72.0 is installed per workspace through its npm package; Node.js
^20.19.0 or >=22.12.0 is required, including bundled HTML formatting. Markdown
uses Oxfmt's native formatter.
`code/oxfmt.config.mjs` owns shared style, with per-app configs for Tailwind
stylesheets and `cn`/`cva`. Scripts pass configs explicitly. Existing
`.prettierignore` files remain active Oxfmt exclusions. Do not enable import,
package.json or JSDoc sorting or add new file types without an explicit change.
Scripts use one Oxfmt thread per process because Bun already runs workspace
packages concurrently. Root `format` skips packages without a write script; `format:check` still checks
every package. The `.ts.txt` fixtures use `scripts/format-typescript-fixture.nu`.
Root `format`, `format:check`, `lint`,
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
`code/astrohacker/ts/astrohacker-com`, while `termsurf.com` and its public `/docs`
live in `code/termsurf/ts/termsurf-com`.
