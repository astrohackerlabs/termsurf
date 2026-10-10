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
| Product branch | `issue-26093013295774-exp1-default-the-release-panel-off` |
| Product HEAD | `aec754fffe5f13b8f83812369b54967fa6d0ef6c` |
| Product tree | `ce49f13cdb843cd298b995e2ececcc1d779a759e` |
| Checkout | `forks/red` |
| Package / executable | internal Rust package `red` / `redvim` 0.7.13 |
| License | Upstream `LICENSE`, MIT, copyright 2024 Felipe Coury; preserved unchanged |
| Product commits / patch count | 46 / 46 |
| Archive SHA-256 | `f0ac77fc42d6fd5bbb8a870e4edd5169d166502cee6f7ce9a3623e480ec17254` |

Issue 26092809575864 Experiment 1 appends
`patches/issue-26092809575864/0001-Keep-formatting-and-navigation-alive-without-LSP.patch`.
It adds Prettier defaults from the built-in 3.9.9 filename matrix, independent of
syntax grammars, and nonfatal LSP handling for actions, startup, background work
and document synchronization. Failed definition requests release pending
navigation. A second, test-only patch adds missing-tool and invalid-formatter-input
coverage: `0002-Cover-missing-tools-and-rejected-formatter-input.patch`, SHA-256
`79ec334a48597e96d2fd2a583f15860cd9a9666f40141eff8e1f53412450b99f`.
The 41-patch series independently reconstructs the recorded tree at replay HEAD
`8ab5b3bdc110fcdb98e5d7283554de4938d46d06`.
Patch SHA-256: `cf7748e5f319479c0ee212a25044058b97d7639c9e080c9a1726255a9c7527dd`.
This remains a local 0.7.10 candidate; human experiment acceptance and any later
publication are separate.

Issue 26092711590255 Experiment 1 appends
`patches/issue-26092711590255/0001-Default-Markdown-formatting-to-Prettier.patch`.
It embeds the requested 80-column Markdown formatter settings and preserves
language-pack capabilities beneath explicit user definitions. Focused checks
exercise real Prettier, undo/save behavior and configuration precedence.
Patch SHA-256: `0ea9ebf14e9f4ce59a9ff75c79033c54c70f826a25c932b254499a618f3bd827`.
The initial export reconstructed 37 patches. A second patch updates reload-test
expectations for the newly configured Markdown entry:
`0002-Account-for-Markdown-defaults-when-reloading-languag.patch`, SHA-256
`542f5390c039ca70802fbcd63bac04a15b96238a55489f72a742458a7592d194`.
Independent replay of all 38 patches produced tree
`e4f32d7ce79d8fcd1b760ff7eb32ce72f1c91e53`, matching the final tested tip.
Version remains 0.7.9 for local qualification; the publisher will select and
archive the release version. Publication and installed acceptance remain pending.

Issue 26092609387958 Experiment 1 appends
`patches/issue-26092609387958/0001-Bundle-pinned-Typst-syntax-support.patch`.
It bundles the pinned SeniorMars Typst parser, adapted highlighting and raw-block
injection queries, language/default registration, standalone checks, and MIT
provenance. `ah patch export` independently reconstructed all 35 patches to
the recorded product tree. Patch SHA-256:
`837ad59d0090041a6078ba2b264504fcf9f8e278a9df3c005cb7efdab3becf36`;
stable patch ID: `50a00085929cd651e40e85f3b5ee9528f1e7df06`.
This is a local 0.7.8 candidate, not a new publication. Human visual acceptance
and the experiment's completion review remain pending; see its evidence record.

The first, unchanged patch is
[`0001-Add-Astrohacker-Red-fork-guidance.patch`](patches/issue-26091823140848/0001-Add-Astrohacker-Red-fork-guidance.patch).
It replaces upstream root guidance with the Astrohacker overlay.

Issue 26091918020609 Experiment 3 appends patches 0003 and 0004 in that
issue's archive: Austin Night as the embedded normal/recovery default, retained
Enkia attribution and palette provenance, default/override regression tests,
and the PTY-driven search/breadcrumb contrast correction. The complete 28-patch
series independently replays at `c950c4576b13534679d7838ae6552c295a355b3e`
to the accepted Austin Night candidate tree. Local source projection and binary packaging
retain theme notices. See the issue's `austin-night-evidence.md` for candidate
hashes and Ryan's manual visual acceptance (“perfect”). His copy-only follow-up
adds patch 0005, changing the intro/release-card tagline to “the editor that
respects your time”. All 29 patches replay at
`cadb2916c2bbf337c4feeffd85ca722f6157f9f3` to the recorded product tree;
25 relevant UI tests pass. Version remains 0.7.5;
this source pin is not a new publication.

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

Issue 26091914238740 Experiment 6 appends
`patches/issue-26091914238740/0009-Bundle-WGSL-syntax-support.patch`, retaining
the reproducibly generated official WGSL parser, scanner corrections, adapted
queries, tests and required notices. Patch SHA-256:
`b3f9e01e10e8dc8faae88a06b5307ae5eb2ceaecd3b6acf167045be4fb487939`.
That 21-patch series independently replays to WGSL tree
`3fe419faad01277a6eea55068b4742e8ff9d8662`;
replay HEAD is `b0212b4ebf54af204a9891c64176ee8e41a47c02`.
Source remains version 0.7.4 until the normal publisher selects a new version.
See the automated language issue's WGSL evidence for the qualification boundary.

Issue 26091914238740 Experiment 7 appends
`patches/issue-26091914238740/0010-Bundle-Caddyfile-syntax-support.patch`, with
the pinned official native grammar, corrected heredoc scanner, matched queries,
tab defaults, tests and MIT notice. Patch SHA-256:
`d2c60865504c7cd276b1ab41b8e42c33e3e3ca8f38cf59b5525c5c047ea359e7`.
The 22-patch series replays to tree `eee76f1341422e55a4399e41c0a7207ca7dc6da0`
at reconstruction HEAD `3b2886f5bf474f8c5c28884eb37cbe03247c7458`.
Source remains 0.7.4 for local qualification; this pin is not a new publication.

Issue 26091914238740 Experiment 8 appends
`patches/issue-26091914238740/0011-Bundle-AppleScript-syntax-support.patch`, with
the unchanged pinned native parser/scanner, complete Rust wrapper, adapted
queries, tests and MIT notice. Patch SHA-256:
`c8c659bfeb8dce78293a2b0b0805ac695d0390e711437ac635d9a8c7ae9672c4`.
The 23-patch series replays to tree `c4af0b7b2dd2e2638586fd3020bfe5d1dca2a064`
at reconstruction HEAD `cb4cd9f8c50b2f46d64b1f58e2f18116dbc384a7`.
That experiment's pin was a local 0.7.4 candidate pending publication.

Issue 26091914238740 Experiment 9 published and installed RedVim 0.7.5.
The publisher appended `0024-Release-RedVim-0.7.5.patch` (SHA-256
`1a272949e6493a4d35f7f6d6ed270d1628ee0c401c67227871540917ab223753`).
All 24 patches independently reconstruct the recorded shipping tree at replay
HEAD `8eecb55c3f03f8929777b92be70ebc347d0c2f8e`; earlier patch bytes and the
ordered stable patch IDs are unchanged. See the automated issue's
[installed release evidence](../../docs/issues/26091914238740-complete-redvim-language-support/installed-release-evidence.md)
for publication, Homebrew and all eight installed language checks.

## Reconstruct from upstream

The current local 0.7.5 candidate adds Issue 26091918020609 Experiment 1's
`redvim` intro artwork, retaining the red dot and adapting compact layout.
Its patch is `patches/issue-26091918020609/0001-Extend-intro-wordmark-to-redvim.patch`,
SHA-256 `b06b20b6bc0966cd220b965a9fc3e96916bb1d0b63bdfcf2561f749b4d83ed7f`.
`ah patch export` verified reconstruction of all 25 patches and ordered patch
identities against the recorded tree. Ryan visually accepted this candidate in
Experiment 1; it has not been published or installed through Homebrew.

Experiment 2 appends
`patches/issue-26091918020609/0002-Use-RedVim-branding-across-product-surfaces.patch`
(SHA-256 `a3c4f2968e3bab421c3064066821928851ebb40e3fdcb8216c87b12ee86872c1`).
It renames the 74 audited product locations, shares Rust identity labels, and
preserves the legacy palette alias and Git commit-context reader. The full
26-patch series independently replays at
`84d216639b2d4fc98e1d242baead7b22a0efeed8` to the pinned tree, with matching
ordered stable patch IDs. All preceding patch bytes are unchanged. This is an
unpublished 0.7.5 candidate accepted by Ryan in Experiment 2.

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
let manifest = (open patches/release-manifest.json | get forks | where name == red | first)
git -C $destination switch -c $manifest.branch $base
let series = ($manifest.patch_directories | each {|dir| glob ($root | path join $dir '*.patch') | sort } | flatten)
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

Issue 26091914238740 Experiment 1 appends its HCL integration delta in
`patches/issue-26091914238740/0001-Bundle-Terraform-HCL-highlighting.patch`.
Its SHA-256 is `5824ab120e41f480c3d9be41096c40c9b2b11a573b61749157d4a3fc774d6b59`.
The twelve predecessor patches remain unchanged. `ah patch export` independently
reconstructed the complete thirteen-patch series and verified the pinned tree.
This is local HCL qualification; the final multi-language release remains pending.
Result-review corrections append `0002-Check-HCL-template-and-splat-colors.patch`
(SHA-256 `b073d0399d65a7a2dc54e722f54cfa1565d84b87b6d29d6c784dde46d2fc079a`).
It strengthens exact template/collection/traversal color assertions and fixes
property colors following splats. The fourteen-patch reconstruction produced
HEAD `e7d09521170e09b3d34a549809f1d16d1eb736f8` and matched the current tree.

Experiment 2 adds Objective-C in patches `0003-Bundle-Objective-C-highlighting.patch`
and `0004-Restore-the-complete-Objective-C-query.patch` of that same issue directory.
Their SHA-256 values are `6fcb57a158629a4f4bca07aacb97ee36e1fd19d97aefc5c7634bcc2a76b4eea5`
and `3bdc43daf11474b308fe4838c6fd6687b033fb80da05c38871d975d3a43da781`.
The second preserves the complete query after a staging-time streaming trim
error; no incomplete query is qualified. The full sixteen-patch reconstruction
at `f443171af0eee594a0590b700cc1831b18045cb0` matches that experiment's pinned tree.
Earlier archives are unchanged. Objective-C++ is not claimed by this grammar.

Experiment 3 appends `0005-Bundle-Protocol-Buffers-highlighting.patch` and
`0006-Distinguish-named-protobuf-types-in-checks.patch` in the same directory.
SHA-256 values: `d78faf503e99854c0534486ec718e07f59def909821f3975b15e6ddde166a417`
and `06cb964b1f8c5a1f13fb0c8ecc263d3ff209d3b0cd2a79c5e44ec0bfd3e0ac48`.
The eighteen-patch replay at `cc9c81b4a82a931cbb33da428ccd9b06bbf9821a`
matches that experiment's tree and stable patch IDs. Proto2/proto3 and an edition-2023
syntax fixture are checked; no protobuf semantic validation is claimed.

Experiment 4 appends `0007-Bundle-Dockerfile-highlighting.patch`, SHA-256
`47aa8ed42474eee0f7d255dc5dac19c8b26e6939248458ff472da6b6b96d8e9a`.
The nineteen-patch replay at `810e731061bcca485fb687552088984032def5f2`
matches tree `227d27e5f969d1f69953621e49dd33a95b062248` and stable patch IDs.
It bundles the parser/scanner, adapted query and both MIT notices. Dockerfile
host highlighting includes flags and heredocs; embedded shell execution and
language injections are outside scope. Publication remains pending.

Experiment 5 appends `0008-Bundle-GN-highlighting-and-native-grammar.patch`,
SHA-256 `a693db42d5f9073f6718cdad3921e9fc570dfdd962036a88f26c61523dd3835e`.
The twenty-patch replay at `de2e54b51b03186eafa4f80398edb8e8279fe52b`
matches tree `a2e68bd9deac0d48a3cd3885711d8cb81189e3e5` and stable patch IDs.
The local grammar crate retains pinned generated sources, adds LanguageFn
bindings and corrects the scanner's byte-escape lookahead with regression proof.
No GN build is executed. Publication remains separate from this local integration.

Select a new issue/experiment branch before edits. For a new issue directory,
use `ah patch export red --issue <id> --experiment <n> --from <previous-tip>`
to append a verified delta, then adopt its proposed manifest entry and refresh
this README. Preserve the selected preceding directories in manifest order.
For subsequent commits within an existing issue archive, append only the new
commits with the next unused filename number; never overwrite an old patch:

```nu
let archive = ($env.PWD | path join patches/red/patches/issue-ISSUE_ID)
mkdir $archive
git -C forks/red format-patch --start-number NEXT_NUMBER --output-directory $archive PREVIOUS_TIP..HEAD
git -C forks/red rev-parse HEAD 'HEAD^{tree}'
```

Replace `ISSUE_ID`, `NEXT_NUMBER` and `PREVIOUS_TIP` with the current issue,
next unused number in that directory and last archived tip. Recompute the
complete manifest digest and independently reconstruct before recording success.
Record provenance explicitly for
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

Experiment 8 appends `0009-Bundle-common-languages-in-RedVim.patch`, adding C,
C++, Python, HTML, CSS, Ruby, Zig, Swift, XML/SVG and Makefile parsers, embedded
queries, defaults, provenance/licenses and release self-checks. Patch SHA-256:
`9e2a2d0a8e8fb3b7f239141ddaf623284a00fa1a4f486e5acaa5341a596a6bca`.
The source still reports 0.7.2 until the normal publisher prepares the next
patch version; this pin does not claim a new publication. See Experiment 8's
language-release evidence for reconstruction, package verification and the
remaining human publication/installed-use boundary.

Release 0.7.3 appends `patches/red/patches/issue-26091823140848/0010-Release-RedVim-0.7.3.patch`; patch SHA-256 `d700b187a5acaa031bba638317576570f1a2b49aa990da1a0b7e26f13ff884a4`.

Experiment 9 appends `0011-Bundle-SQL-highlighting-in-RedVim.patch`. It bundles
the pinned SQL parser, adapted queries, documented MySQL/SQLite recovery colors,
default metadata, tests and MIT attribution. Patch SHA-256:
`69084268fe8d1f538cc3102a80c3a184ad1f54a82d97e40e9e29c9d266fa8b4e`.
Independent reconstruction HEAD `386a2cb56dce61510a5336c33d23489c218d7527`
matches tree `04cca965877e7a402621eea6843b7df9a0e1022e`. Source still reports
0.7.3 until the automatic publisher prepares the next version. This is a local
SQL release candidate, not replacement bytes for the published 0.7.3 release.
See the issue's SQL release evidence for automated and human qualification.

Release 0.7.4 appends `patches/red/patches/issue-26091823140848/0012-Release-RedVim-0.7.4.patch`; patch SHA-256 `dbe6fb2bfaf779424619bb97cdfde2326be2443ae5e80a5f3478cd98515a1af7`.

Release 0.7.5 appends `patches/red/patches/issue-26091914238740/0024-Release-RedVim-0.7.5.patch`; patch SHA-256 `1a272949e6493a4d35f7f6d6ed270d1628ee0c401c67227871540917ab223753`.

Release 0.7.6 appends `patches/red/patches/issue-26091918020609/0030-Release-RedVim-0.7.6.patch`; patch SHA-256 `9eadd86bfd8437b0ddfe7ca2794a0c3425f2f55e5a9b958e7a5cbff12cb60244`.

Issue 26092310328880 Experiment 1 appends
`patches/issue-26092310328880/0001-Color-Markdown-inline-spans.patch`.
It parses each Markdown `(inline)` range with tree-sitter-md's inline grammar
and maps those captures onto Austin Night's existing italic, bold, inline-code,
and link styles. Patch SHA-256
`e4537a76599b3e1d64648a923aae20cf58ba96adf8c92716d5d7da27cacb9b9f`;
stable patch ID `2cd8df14162564c9fa5b48407c5cfc801c4db4d9`.
`ah patch export` reconstructed all 31 patches to tree
`b5ad91fa7c34d0a59fc0dbee4896f9ad712b35be` at fork HEAD
`ee105f18c3df92d0685fc48efd03a6553c992996`. Source remains 0.7.6; this pin
is not a new publication. Ryan accepted the running editor.

Release 0.7.7 appends `patches/red/patches/issue-26092310328880/0032-Release-RedVim-0.7.7.patch`; patch SHA-256 `1dccd14d7a240bc106995fbc85bd3362cdefeddeb4d16aaebc2d6ee4dfaf7126`.

Issue 26092311060249 Experiment 1 appends
`patches/issue-26092311060249/0001-Color-each-Markdown-heading-level.patch`.
It gives each ATX heading marker and title a distinct bold Austin Night color,
and uses the level 1 and level 2 colors for setext headings. Patch SHA-256
`9733189150840e337fee4312bbd294a49f544c5ddb7c76183a49153eb00b3613`;
stable patch ID `4292240356cd79e7608de457111d990a365ba560`.
`ah patch export` reconstructed all 33 patches to tree
`f6cb29dae297f27b74ecdb7c454d7e322b6d0c0a` at fork HEAD
`fcd8f89292f06d00745714eb1b4def7d08368d22`. Source remains 0.7.7; this pin
is not a new publication. Ryan accepted the running editor.

Release 0.7.8 appends `patches/red/patches/issue-26092311060249/0034-Release-RedVim-0.7.8.patch`; patch SHA-256 `293aa6b77d762d68a11131da32831abdd5800314aa7d53b9036fd79c81b05526`.

Release 0.7.9 appends `patches/red/patches/issue-26092609387958/0036-Release-RedVim-0.7.9.patch`; patch SHA-256 `69b35069078372119b80279daba4e612d8a48620965171f36a18c9ff386a212f`.

Release 0.7.10 appends `patches/red/patches/issue-26092711590255/0039-Release-RedVim-0.7.10.patch`; patch SHA-256 `0d5e7cdfda9d91b130b99da7f2a20cbc9ceafcea2f21e8943486f31b740da936`.

Release 0.7.11 appends `patches/red/patches/issue-26092809575864/0042-Release-RedVim-0.7.11.patch`; patch SHA-256 `bedccf045af285b45718021640197c58d8a8757dec4d6625a943a08a2914cbb1`.

Issue 26093012262975 Experiment 1 appends
`patches/issue-26093012262975/0001-Rescan-the-file-list-when-it-opens.patch`.
Opening the file list scans every expanded directory, so files added or
removed while the list was closed show up without Shift+R. Patch SHA-256
`e1afe27e89668d010baa8069e81d02aca7ba809264ead6be8cc5020119f1a65c`;
stable patch ID `3d1bf80c4f03d5a9c961b95e6050317316203a7d`.
`ah patch export` reconstructed all 43 patches to tree
`bd32e0412d2d89c473ceee3ef6893fb7e50b9c1c` at fork HEAD
`88567486064adfcacfec1b6e75c753a3f5b29b46`. Source remains 0.7.11; this pin
is not a new publication. Ryan accepted the running editor.

Release 0.7.12 appends `patches/red/patches/issue-26093012262975/0044-Release-RedVim-0.7.12.patch`; patch SHA-256 `2ff6cd5dbaf09bbc2dc5b79830a411bb730c075aeb1b522e49e2cf5912eac8b1`.

Issue 26093013295774 Experiment 1 appends
`patches/issue-26093013295774/0001-Leave-the-release-panel-closed-on-startup.patch`.
Startup leaves the What's new panel closed unless `show_whats_new` is true.
`:whats-new` and `:changelog` still open it. Patch SHA-256
`4ee0d615d9603d90b73b0837a5a8bc9be938c6589b95903c82a930e382a9e667`;
stable patch ID `2e451dab822b5e11ffa6ca84f0baeaa7e184e1db`.
`ah patch export` reconstructed all 45 patches to tree
`1cb36e6ea0c6517e349394039b9cf7e236d628d8` at fork HEAD
`3e1c44d2578a4eda654343a31610a3db5087dc57`. Source remains 0.7.12; this pin
is not a new publication. Ryan accepted the startup look on 2026-09-30.

Release 0.7.13 appends `patches/red/patches/issue-26093013295774/0046-Release-RedVim-0.7.13.patch`; patch SHA-256 `175fc90c8dc142c7285b796ac99079a12dc0f4a5451cae702c1b7ed80a687de2`.
