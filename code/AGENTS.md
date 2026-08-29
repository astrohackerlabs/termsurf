# AGENTS.md

Guidance for coding agents working in the Astrohacker **product
source** tree (`code/`). Rust crates live in **`rs/`**; Bun/TS
packages live in **`ts/`**. Product folders: `termsurf`,
`earthbucks`, `astrohacker`, `keypears`, `compubutton`.
`AGENTS.md` and `oxlint.config.ts` stay at `code/`. Package, crate,
PATH, and npm names stay unprefixed.
Root `Cargo.toml` and `package.json` stay at the **monorepo
root**. Workspace `target/` is at the monorepo root. Fork trees
live under top-level `forks/` (root Cargo **excludes** `forks`).

Five **projects** with a **`ts/`** language folder:
`code/astrohacker/ts`, `code/termsurf/ts`, `code/earthbucks/ts`,
`code/keypears/ts`, `code/compubutton/ts`. Fixtures
`test-html` and `slow-load` live under `code/termsurf/ts` and
are not workspace members. Static `termsurf.com` homepage is
`code/termsurf/ts/tswebapp`. No extra `lib` / `bin` / `web` /
`node` folders.

Two **Rust** product buckets with **`rs/`**:
`code/termsurf/rs` (`ahweb`, `ah-chromiumd`, `ahnexus`, `ahsh`,
`ahtch`) and `code/earthbucks/rs` (`ebxlib`, `ebxfloat`,
`ebxpow5`, `ebxmine`). Proto and render-channel stay at
`code/termsurf/proto` and `code/termsurf/render-channel` (not
under `rs/`).

Package catalog (path, npm name, one-line role): **root
`AGENTS.md`**. Package traps: that package’s `AGENTS.md`. Do not
duplicate the full table here.

**Why projects plus `rs/` / `ts/`:** folder token stays
`@astrohacker/<token>` or the unprefixed crate name; the project
folder is the product. Languages sit side by side. One shared
Bun workspace at the repo root so `@astrohacker/ui` resolves
across projects. CSS `@source` to `ui` is cross-project; oxlint
`--config` is `../../../oxlint.config.ts` (see Shared UI).

Root Cargo members: `code/termsurf/rs/ahweb`,
`code/termsurf/rs/ah-chromiumd`, `code/termsurf/rs/ahnexus`,
`code/earthbucks/rs/ebxlib`, `code/earthbucks/rs/ebxfloat`,
`code/earthbucks/rs/ebxpow5`, `code/earthbucks/rs/ebxmine`.
`code/termsurf/rs/ahsh` is **excluded** (own lockfile). Default
interactive mode is **nu**; alt is **zsh** (not bash). Build
with `cargo build --manifest-path code/termsurf/rs/ahsh/Cargo.toml`.
See `code/termsurf/rs/ahsh/AGENTS.md`. `code/termsurf/rs/ahtch`
is **excluded** (own workspace + LibTorch pin in
`code/termsurf/rs/ahtch/.cargo/config.toml`). `cd
code/termsurf/rs/ahtch` before cargo so that config applies:

```nu
cd code/termsurf/rs/ahtch; cargo test
```

`ah-chromiumd` consumes optional host `--render-surface-service=<NAME>`
before entering Chromium (including launches by absolute executable
path). Chromium presentation still uses CAContext.

EarthBucks 2.0 crates are **root members** under
`code/earthbucks/rs`. PATH **`ahebx`** is
`code/earthbucks/ts/ahebx` (pool client of ebxwebapp), not
`ebxmine`. Production pool is `code/earthbucks/ts/ebxwebapp`.

```nu
cargo metadata --no-deps
cargo check -p ahweb
cargo test -p ebxlib -p ebxfloat -p ebxpow5
cargo check -p ebxmine
cargo build --manifest-path code/termsurf/rs/ahsh/Cargo.toml
```

**APIs are not interchangeable:**

| Surface | Where |
| --- | --- |
| oRPC `/api` | `ahwebapp` (product), `kpnode` (KeyPears contract from `kplib`), `ahkey` (local `/api` only) |
| tRPC `/trpc` | `ebxwebapp` + `ebxwebclient` |
| No RPC | `kpwebapp` (`code/keypears/ts/kpwebapp`; static well-known + landing), `tswebapp` (`code/termsurf/ts/tswebapp`; static homepage) |
| Local `/api/*` | `ahnexus` SPA → `code/termsurf/rs/ahnexus` (no Nexus wire in TS) |

**KeyPears crypto details** (salt strings, PBKDF2 rounds,
`vault-key-v2`, AES-GCM framing) live in **`code/keypears/ts/ahkey/AGENTS.md`**
and **`code/keypears/ts/kpnode/AGENTS.md`** (site reference:
`code/astrohacker/ts/ahwebapp/AGENTS.md`). Do **not** duplicate algorithm
parameters here.

Non-workspace fixtures: `code/termsurf/ts/test-html` (port 9616; prefer
over ad-hoc issue HTML) and `code/termsurf/ts/slow-load` (delayed HTML,
**127.0.0.1:3456**). Operator Chromium smoke: **root `AGENTS.md`**
(outer shell for server/`^open`; **`ahweb` only inside TermSurf**).

**ahwebapp agents:** RR8 + UI rules below + `code/astrohacker/ts/ahwebapp/AGENTS.md` (Drizzle,
**models + oRPC architecture**, auth, deploy, app-only CSS). All product DB
access goes through `app/server/models/`; oRPC contract under
`app/server/api/`; every product loader needs an oRPC read twin; product
writes use browser `getBrowserApi()` (no product RR actions).

**ahcalc agents:** RR8 + UI rules below + `skills/ahcalc` + `code/termsurf/ts/ahcalc/AGENTS.md`
(CLI / TermSurf / binary). **No Drizzle** / no `DATABASE_URL`.

**ahplt agents:** RR8 + UI rules below + `code/termsurf/ts/ahplt/AGENTS.md`. Plotly +
Austin Night template + TermSurf SetOverlay. **No Drizzle**. PATH
binary **`ahplt`**.

### Drizzle

Not every Bun package has a DB. Do **not** put Drizzle in the browser.

| Package | Store | Apply |
| --- | --- | --- |
| `ahwebapp` | MySQL | **Push-only** (`db:dev:push`). No committed kit SQL. See package AGENTS. |
| `ebxwebapp` / `ebxdb` | MySQL | EarthBucks schema; `drizzle-kit push` on ebxwebapp. |
| `kpnode` | PlanetScale MySQL | **Migrate-on-boot**. No production FKs. See `code/keypears/ts/kpnode/AGENTS.md`. |
| TermSurf apps, `kpwebapp`, ui catalog | none | No `DATABASE_URL`. |

**ui agents:** shared kit only—primitives and multi-app chrome. Do not put
app-specific pages (login form, calculator keypad) here. **Center modals** use
`@astrohacker/ui/dialog` (blurred overlay + panel chrome). Do not hand-roll
`fixed inset-0` scrims for product modals; the mobile nav drawer is a separate
pattern (not center Dialog).

**pow5 agents:** treat `code/keypears/ts/pow5` as an algorithm library (vendored). Prove
changes with `bun run --cwd code/keypears/ts/pow5 test` (Vitest browser / Playwright).

## React Router 8 (framework mode)

**Framework-mode** RR8 apps: ahwebapp, ebxwebapp, ahcalc, ahkey, ahplt,
ahebx, ui catalog (`code/astrohacker/ts/ui` `app/`), kpwebapp, kpnode landing. Use `@react-router/dev`, `react-router.config.ts`,
`app/routes.ts`, route modules under `app/routes/`. Do not use Data
mode (`createBrowserRouter` + `RouterProvider`) or Declarative mode
(`BrowserRouter` + JSX `<Routes>`) for those apps.

**`code/termsurf/ts/ahnexus` is not RR8 framework mode** (Vite SPA). Do not apply
this section there. **`ebxwebapp` is RR8** for the UI and **Express +
tRPC** for the product API — do not add ahwebapp oRPC there.

### Navigation

- **Internal** routes: `<Link>` or `<NavLink>` with **`to={href(...)}`**.
  Dynamic segments: `href("/blog/:slug", { slug })`. Paths must match
  `app/routes.ts` (typed).
- **External** URLs (`https://`, `mailto:`, stores): plain `<a href>`, not
  `Link`/`href`.
- Prefer declarative `Link` over `useNavigate` for ordinary UI navigation.
- Server redirects: `redirect(href("…"))` (same path discipline).

### Data (internal app I/O)

“Internal” = our app data plane (session, DB, auth, health, blog, account
commands)—not a public third-party API the browser is meant to call with
secrets.

**webapp (product):**

- **Page reads** → route **`loader`** (server) calling **models**. Prefer
  loaders for first paint + URL pagination. Loader-only **`useFetcher` GET**
  for “load more” is fine.
- **Mutations / commands** → browser **`getBrowserApi()`** (`app/lib/browser-api.ts`)
  in **event handlers** → oRPC `/api`. **No product route `action` exports.**
  Do not use mutation `useSubmit` / `<Form method="post">` for product writes.
- After oRPC success: **`navigate`** and/or **`useRevalidator().revalidate()`**.
  Local React state for errors (not `actionData`).

**ahcalc / generic RR8:**

- **Page reads** → route **`loader`**. Keep secrets in server-only modules.
- **Mutations** may use route **`action`** + **`<Form method="post">`** when
  the app has no oRPC surface (ahcalc).
- **Search/filter in the URL** → `<Form method="get">` or query links; parse
  search params in the **loader**.

Do **not** use `useEffect` + ad-hoc `fetch` as the primary path for product
reads when a loader can own them. Product writes use the typed oRPC client,
not raw `fetch` to route actions.

### Types

- In each route module: `import type { Route } from "./+types/<route>"`.
- Type **`loader`** with `Route.LoaderArgs`; default component with
  `Route.ComponentProps` (`loaderData`). Product webapp has **no** route
  actions; ahcalc may still type **`action`** with `Route.ActionArgs`.
- Typecheck: **`react-router typegen && tsc`**. Never hand-edit
  `.react-router/`. Keep `.react-router/` gitignored; `tsconfig` must include
  generated types + `rootDirs` as the app already does.

### Middleware, errors, UX

- Shared auth/session for a route tree: **`middleware`** + typed
  **`createContext`** / `context.set` / `context.get`—not copy-paste checks in
  every loader.
- Mutation validation failures: client local error state (webapp oRPC) or
  serializable **`actionData`** (ahcalc actions). Missing resource: throw/
  `data` with 404 + **`ErrorBoundary`**. Success that leaves the page:
  **`navigate`** / **`redirect`**.
- Pending UI: **`useNavigation`**, NavLink `isPending`, or local busy state.
  App owns spinners and disabled buttons.
- Prefer **`Link`** for navigation; progressive enhancement for product
  mutations is oRPC + JS (webapp).

### Runtime notes

- **webapp SSR:** `entry.server` uses **`renderToReadableStream`** (Bun). Do
  not use Node-only `renderToPipeableStream` on Bun.
- **ahcalc:** still RR8 framework source; product binary serves the built
  client—packaging does not change route-module conventions.
- After loader navigations, loaders revalidate by default; use
  `shouldRevalidate` only when deliberately skipping work.

## Shared UI (`@astrohacker/ui` / `code/astrohacker/ts/ui`)

RR8 product apps (ahwebapp, ebxwebapp, ahcalc, ahkey, ahplt, ahebx,
kpnode landing, kpwebapp) and the ui catalog share one UI package:

| Put in `@astrohacker/ui` | Keep in the app |
| --- | --- |
| shadcn-style primitives (`button`, `input`, `field`, `label`, `avatar`, …) | Page/feature shells (`login-page`, `login-form` logic, calculator, …) |
| Multi-app chrome (`GlyphRain`) | One-off layouts and product shells |
| Shared Austin Night **tokens + `@theme`** | App-only CSS (blog `.prose`, `.ahcalc-keypad`, …) |
| `cn()` | Route modules, loaders, business logic |

### Musts

**Default to the kit.** Use `@astrohacker/ui` whenever a primitive
exists. Native `<select>` / `<dialog>` / range inputs and hand-rolled
buttons or error rows need a **named** reason. Catalog of basics:
`code/astrohacker/ts/ui` catalog (`http://127.0.0.1:3464`, `bun run dev:ui`). Package rules:
`code/astrohacker/ts/ui/AGENTS.md`.

- Import primitives: `import { Button } from "@astrohacker/ui/button"` (same for
  `input`, `field`, `label`, `avatar`, `badge`, `card`, `dropdown-menu`,
  `select`, `separator`, `glyph-rain`, `utils`, and every freeze-list name).
- **Form dropdowns:** kit **`Select`** (`@astrohacker/ui/select`) — not native
  `<select>` and not **`NativeSelect`** (TermSurf Chromium panes do not host
  OS popup widgets). Action menus stay **`dropdown-menu`**.
- **Product forms** (sign-in, create/save account, and other labeled
  handle/password chrome): stack each control with kit **`Field`** /
  **`FieldLabel`** / **`Input`** / **`FieldFooter`** (+ **`FieldHint`** /
  **`FieldError`** as needed) and **Button** for actions — not ad-hoc raw
  `<input>` / `<button>` / hand-rolled under-field error rows. Hidden payload
  inputs (`type="hidden"`) may stay native.
- **`Field` vs bare `Input`:** use **`Field`** whenever the control has a
  visible label and may show helper, status, or error text under the box
  (default: one reserved footer line so content never reflows the form). Use
  bare **`Input`** only for compact/non-form chrome (search strips, dialogs
  without under-field copy, etc.). Do **not** teach `Input` itself to own
  error layout — keep `Input` a leaf control; compose with `Field`.
- Each app **`app/app.css`**: `@import "tailwindcss"`, then
  **`@source` the ui package source** (`@source "../../ui/src"` from
  `code/astrohacker/ts/ahwebapp`;
  `@source "../../../../astrohacker/ts/ui/src"` from other
  project `app/` CSS; ebxwebapp tailwind.css one extra `../`;
  ui catalog `@source "../src"`), then **`@import "@astrohacker/ui/styles.css"`**. Do **not** re-define
  the full shared token/`@theme` block in the app.
- Do **not** recreate `app/components/ui/` or fork glyph rain / tokens per app.
- New multi-app component or theme token → add to **`code/astrohacker/ts/ui`**, not both apps.
- App-only styling stays in that app’s CSS after the shared import.

## Webbuf (binary data + crypto)

Product binary data and cryptography use the **webbuf ecosystem** from
`~/dev/webbuf` / npm `@webbuf/*` — typed buffers that work
on Bun and the browser. Prefer this over Node **`Buffer`** and bare
**`Uint8Array`** as **domain** types.

| Type | Use for |
| --- | --- |
| **`WebBuf`** | Variable-length bytes (ciphertext, messages, frames, payloads) |
| **`FixedBuf<N>`** | Exact-length material — common: **16** UUID / AES-128, **32** SHA-256 / AES-256 key, **64** headers/sigs |
| **`@webbuf/numbers`** | Fixed-width integers on the wire (`U32LE`, `U64BE`, …) |
| **`@webbuf/rw`** | Sequential binary I/O (`BufWriter` / `BufReader`) |
| **`@webbuf/*` algos** | Hash, HMAC, PBKDF2, AES-GCM, ECC, PQ, … |

### Musts

- **App types:** export and store binary as WebBuf/FixedBuf—not `Buffer` or
  plain `Uint8Array` as domain/schema `data` types.
- **Edges (idiomatic):** when an API we do not control yields `Buffer` /
  `Uint8Array` (mysql2, Node `data` events), convert **once** with
  **`WebBuf.fromUint8Array(...)`** or **`WebBuf.view(...)`**. It is fine to type
  the callback as Node’s `Buffer | string` and convert on the next line. **Do
  not** rebuild buffers with DataView byte loops + `fromArray`.
- **Driver edge only (Drizzle):** mysql2 `Buffer` belongs **inside**
  `customType` `toDriver` / `fromDriver` only (`Buffer.from(fixed.buf)` out,
  `WebBuf.fromUint8Array(buf)` in). That is correct, not debt.
- **Drizzle binary columns:** UUID/id `BINARY(16)` → **`FixedBuf<16>`**;
  fixed key columns → `FixedBuf<N>`; variable blobs → **`WebBuf`**.
- **Crypto:** use `@webbuf/*` packages; keys/hashes as FixedBuf; call
  **`wipe()`** on FixedBuf secrets when done. Prefer package hex helpers
  (`toHex` / `fromHex`) over reinvented loops.
- **Sequential binary:** prefer `@webbuf/rw` + `numbers` for structured
  layouts. **Warning:** `BufWriter.writeVarIntU64BE` is **Bitcoin CompactSize**,
  **not** protobuf LEB128 — do **not** use it for TermSurf/protobuf field tags.
  Keep local protobuf LEB128 helpers when encoding protobuf.
- **ahcalc CLI:** TermSurf frames and tty scan domain types are **`WebBuf`**;
  assemble with `BufWriter.write(WebBuf)` when useful; stream edges convert as
  above.
- **Cross-runtime:** webbuf works on Bun and browser — do not put Node-only
  Buffer APIs into shared auth/crypto modules.

### Do not

- Type schema columns as `customType<{ data: Buffer }>`.
- Pass raw `Buffer` through loaders/actions/session user ids when FixedBuf is
  available.
- Use bare `Uint8Array` or `Buffer` as product encode/scan buffer types.
- Use Bitcoin CompactSize (`writeVarIntU64BE`) for protobuf/TermSurf varints.
- Add unused `@webbuf/*` packages without imports.

## TermSurf `bin` apps (ahcalc / ahkey / ahplt / ahebx)

Shared shape: dedicated **server** owns UDS + HTTP; each foreground
binary is a **client**. `--version` / `--help` exit before TermSurf.
`bun --compile` → `dist/<bin>` only (no `*-server` sibling).
Sockets: `/tmp/ahc-$UID/ahcalc.sock`, `/tmp/ahk-$UID/ahkeypears.sock`,
`/tmp/ahp-$UID/ahplt.sock`, `/tmp/ahe-$UID/ahebx.sock`. CLI frames
use **WebBuf**. Details: each package `AGENTS.md`.

## Commands

From monorepo root (Nushell for operator-facing lists; bash scripts still
invokable as commands):

```nu
bun install
bun run dev                 # ahwebapp
bun run dev:webapp          # same
bun run dev:ahcalc          # http://127.0.0.1:3460
bun run dev:ahkey           # http://127.0.0.1:3461
bun run dev:ahplt           # http://127.0.0.1:3462
bun run dev:ahebx           # http://127.0.0.1:3463
bun run dev:ui              # kit catalog http://127.0.0.1:3464
bun run dev:ahnexus         # http://127.0.0.1:3471
bun run dev:kpweb           # http://127.0.0.1:3500
bun run dev:tsweb           # http://127.0.0.1:3510
bun run dev:kpnode          # http://127.0.0.1:3750
bun run build:webapp
./infra/deploy-astrohacker.nu   # ahwebapp → ECS
./infra/deploy-earthbucks.nu    # ebxwebapp → ECS (not package.json "fly deploy")
./infra/deploy-kpweb.nu         # kpwebapp → S3 + CloudFront
./infra/deploy-kpnode.nu        # kpnode → ECS
bun run format              # prettier via workspaces
```

Prettier is installed **per subproject**. Root `format` only fans out via
`bun run --workspaces format`; each workspace must define its own `format`
script.

## Hygiene

- Do not commit `node_modules`, Vite `public/assets/` build products, or secrets.
- **`code/website` was deleted** (Issue 26072501135254 Exp 1). Do not recreate
  without an issue. Public site is `code/astrohacker/ts/ahwebapp`.
