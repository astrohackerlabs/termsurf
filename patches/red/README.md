# Red fork for RedVim

Maintained local fork of [codersauce/red](https://github.com/codersauce/red).
Issue 26091823140848 Experiment 1 adopted upstream source and Astrohacker
guidance. Experiment 4 adds bundled Nushell highlighting using the parser/query
pair proven in Experiment 3. Experiment 5 adds the `redvim` 0.7.0 target, isolated configuration,
and standalone runtime checks. Ryan accepted publication and installed use of 0.7.0 and the automatic 0.7.1 release in Experiments 5 and 6. Ryan accepted Experiment 4's isolated TermSurf build as
“worked perfectly”.
TermSurf and NuTorch do not prepare or package this fork.

## Recorded source

| Field | Value |
| --- | --- |
| Upstream URL / remote | `https://github.com/codersauce/red.git` / `upstream` |
| Default branch observed | `main`, 2026-09-18 23:25:07 UTC |
| Base | `9e0952767d35d5b8dddd3fa3bb90087938692c52` |
| Base tree | `40bc8a92d19f9b9fd10145d6cc002a3a04de2fd9` |
| Product branch | `issue-26091823140848-exp7-storage-paths` |
| Product HEAD | `b56e93d5d809d3e77b6a0eb24889d7304fb00afc` |
| Product tree | `1ca2868dc0507ba7868de5bcc935354efef5fe1f` |
| Checkout | `forks/red` |
| Package / executable | internal Rust package `red` / `redvim` 0.7.2 |
| License | Upstream `LICENSE`, MIT, copyright 2024 Felipe Coury; preserved unchanged |
| Product commits / patch count | 8 / 8 |
| Archive SHA-256 | `5c0ab7f14ba970a4b899d189c24af3a4a8b945438e8a6844af6ca29120f3db00` |

The first, unchanged patch is
[`0001-Add-Astrohacker-Red-fork-guidance.patch`](patches/issue-26091823140848/0001-Add-Astrohacker-Red-fork-guidance.patch).
It replaces upstream root guidance with the Astrohacker overlay.

- Patch SHA-256: `d26a595a6b48e4d6684491c1568c56f6309c289a17a6b0d8e2632e2f7b08d8b5`
- Stable patch ID: `8a82fe4a42c8e5cee6cf54b64b8cbb5fa2e98e98`

The second patch is
[`0002-Bundle-Nushell-syntax-support-by-default.patch`](patches/issue-26091823140848/0002-Bundle-Nushell-syntax-support-by-default.patch).
It pins `tree-sitter-nu` at `bb3f533e5792260291945e1f329e1f0a779def6e`,
embeds the unchanged query from nvim-treesitter
`4d9466677a5ceadef104eaa0fe08d60d91c4e9a7`, registers language defaults,
adds theme compatibility fallbacks and six regression tests, and retains
MIT/Apache-2.0 notices in `docs/licenses/`. See fork `docs/NUSHELL.md`.

- Patch SHA-256: `5b448b9cb7a179676782e890c88433aa6e77f81e5b481527aaa9264c9a084b17`
- Stable patch ID: `1b2876cddb588f1d8f5e30f07e45d2b961882a26`

Experiment 5 appends two patches without changing the first two:

| Patch | SHA-256 | Stable patch ID |
| --- | --- | --- |
| `0003-Prepare-RedVim-identity-for-Homebrew.patch` | `29fb816161a1ab6bf61bd6645fe7fb1a64cbf792531a61c22f27ec526d97c386` | `96fc11bedfc264cec00367974023469a84d2ca98` |
| `0004-Embed-the-standalone-self-check-theme.patch` | `9b166f43f5353bdf3d7f5e640de5cea1a4f93144ea6c046bf8c2a0dc27a2343c` | `a05af791b9d385bae57307243623e7e1a7ef8385` |

They add the real `redvim` executable, independent configuration/runtime override,
RedVim welcome/release notes and identity regression coverage. The standalone
self-check embeds its theme and exercises bundled Nushell highlighting outside
the source checkout. Experiment 5 reconstruction HEAD is
`09c95a1397515a519e93e01f8bfa0fe7f7bcb599`, with its Experiment 5 tree
`2fd1fc74c4334e4e6e6dfd000e8d97533e1890a1`.

Experiment 6 appends `0005-Prepare-RedVim-for-automatic-version-updates.patch`.
It derives identity-test expectations from the Cargo version and makes fork
guidance compatible with automatic version bumps. Patch SHA-256:
`7d1611fac42c3af57cf8564ae7631a2625145206c310ca34f0fdd02f4b1d1782`;
stable patch ID: `31b39b9a48020eee90a08561c51df49ca2143cdf`.
Independent reconstruction HEAD `b5006aab6084aa694b4e5a4b2409ede01b0f5e45`
matched the Experiment 6 implementation tree before the 0.7.1 version bump. The publisher appends a separate version-bump
patch and commits refreshed pins for each selected release; it preserves all
existing patches. The accepted 0.7.1 release added patch 6.

The archive digest uses the shared manifest convention: SHA-256 of ordered
`<patch-sha256>  <repo-relative-path>\n` rows. `patches/release-manifest.json`
is the machine-readable source authority. It is a source pin, not evidence of
a shipped RedVim release.

## Reconstruct from upstream

Run from the monorepo root with an absent destination. Existing checkouts and
caches must be inspected and preserved, never reset or removed to follow this
recipe. The monorepo's tracked overlay may already occupy `forks/red`; use a
fresh ignored reconstruction path in that case, then deliberately provision
the product checkout after inspecting it.

```nu
let root = $env.PWD
let destination = ($root | path join dist/redvim/reconstruction)
if ($destination | path exists) { error make {msg: 'Destination already exists'} }
let base = '9e0952767d35d5b8dddd3fa3bb90087938692c52'
git clone --origin upstream https://github.com/codersauce/red.git $destination
git -C $destination fetch upstream $base
git -C $destination switch -c issue-26091823140848-exp7-storage-paths $base
let series = (glob ($root | path join patches/red/patches/issue-26091823140848/*.patch) | sort)
git -C $destination am ...$series

use scripts/lib/archive-digest.nu [patch-files compute-archive-digest]
use scripts/lib/release_forks.nu [verify-series]
let entry = (open patches/release-manifest.json | get forks | where name == red | first)
let files = (patch-files $root $entry.patch_directories $entry.patch_count)
if (compute-archive-digest $root $files) != $entry.archive_sha256 {
  error make {msg: 'Red archive digest mismatch'}
}
verify-series $entry $destination $files HEAD
git -C $destination status --porcelain
```

`git am` may produce a different commit ID due to committer metadata. The
expected tree and ordered stable patch IDs must match; record reconstructed
HEAD separately instead of changing the canonical HEAD pin. Experiment 4’s earlier
independent reconstruction had HEAD `bf93e931ee39fdfb76d3f8da264467a9b9675d43`
and tree `94140959c0a961419b75c330f8324e9ff22f2d5f`. See the current experiment evidence for later reconstruction.

## Build and smoke

Upstream requires recent stable Rust and Git. Its workspace includes Rust 2024
crates; native C/C++ tools compile the bundled parsers. Experiment 1 uses
Rust/Cargo 1.98.1 and Apple clang 21.0.0 on `aarch64-apple-darwin`.
Keep the upstream lockfile and normal Cargo cache; do not clean existing builds.

```nu
cd $destination
cargo build --locked --release --bin redvim
./target/release/redvim --version
./target/release/redvim --help
```

This builds locally without installation. Upstream developer checks for later
Rust changes include `cargo test --all-targets --all-features` and
`cargo clippy --all-targets --all-features -- -D warnings`.

Experiment 4 qualifies Apple Silicon macOS. Its isolated launcher is
`nu dist/redvim/exp4/manual-smoke.nu`; generated staging is not part of the
patch archive. Distribute the retained license/attribution files with the
executable. Nushell needs no runtime parser/query files or trust action.

## Maintain the series

Select a new issue/experiment branch before edits. Generate a new cumulative
archive from the recorded base into that issue's directory, leaving prior
archives intact, then update README and manifest identities/digests together:

```nu
let archive = ($env.PWD | path join patches/red/patches/issue-ISSUE_ID)
mkdir $archive
git -C forks/red format-patch --output-directory $archive 9e0952767d35d5b8dddd3fa3bb90087938692c52..HEAD
git -C forks/red rev-parse HEAD 'HEAD^{tree}'
```

Replace `ISSUE_ID` with the current issue ID. Record provenance explicitly for
future upstream updates; never silently advance this base to a moving tip.

Release 0.7.1 appends `patches/red/patches/issue-26091823140848/0006-Release-RedVim-0.7.1.patch`; patch SHA-256 `a2d520efc6e7309dc2e61db221f923e43fbf836a787e98e28e06a3822293a370`.

Experiment 7 appends `0007-Namespace-RedVim-configuration-and-storage.patch`.
It moves managed storage beneath `astrohacker/redvim`, including fallback
logging, with isolated configuration/component/IPC regression tests. Default
logging uses `redvim.log`; previous user directories are left untouched.

- Patch SHA-256: `6fa654e6979508c69e726c3d429dd6cdcccf5ce4c531a318e8d2199654b99c51`
- Stable patch ID: `2e8243bc3a688f6c91c09a9868b10fa7eb0102a6`
- Independent reconstruction HEAD: `62ed206311bb25d1020d9b995caeb98032a061bb`
- Matching tree: `f4ca7fb48792d5ea296d8b368c9c1aa6be0072e4`

Experiment 7's package is local qualification output, still reporting 0.7.1;
it does not replace the published 0.7.1 bytes. The publisher selects a new patch
version for the next actual release. See the issue's storage-path evidence for
local validation and Ryan's accepted TermSurf use.

Release 0.7.2 appends `patches/red/patches/issue-26091823140848/0008-Release-RedVim-0.7.2.patch`; patch SHA-256 `c3fcc01bdeb23936ed8d67d177cf47fb43583152a3427ac68f408faaa882d606`.
