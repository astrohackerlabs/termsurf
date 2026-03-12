# Issue 745: Research self-hosted Git with database storage

## Goal

Determine whether we can self-host Git repositories — including the main
TermSurf repo, releases, and the Chromium fork — using a database backend
instead of the filesystem, while remaining fully compatible with standard Git
operations (`git push`, `git pull`, `git clone`).

## Background

The TermSurf website (`website/`) is evolving into a hub. The next step is
self-hosting our Git repositories so that the website serves as the central
source of truth — code hosting, releases, user accounts, and project management
in one place.

### Why not filesystem-based Git hosting?

Traditional Git hosting (Gitea, GitLab, Gogs) stores repositories as bare Git
repos on the filesystem. This works, but:

- **Operational complexity** — Backups require filesystem snapshots or rsync.
  Scaling means managing disk I/O, NFS mounts, or distributed filesystems.
- **Chromium is enormous** — The Chromium repo is ~110 GB. Filesystem-based
  hosting for a repo this size requires careful disk management, and operations
  like garbage collection become expensive.
- **Database advantages** — A database gives us transactions, replication,
  point-in-time recovery, and the ability to query Git objects (commits, trees,
  blobs) directly. It also simplifies the deployment model — one backing store
  for everything.

### What we need

1. **Full Git protocol compatibility** — Users must be able to `git clone`,
   `git push`, `git pull`, and `git fetch` using standard Git clients. No custom
   tooling on the client side.
2. **Database-backed object storage** — Git objects (blobs, trees, commits,
   tags) stored in a database rather than as loose objects or packfiles on disk.
3. **Scale to Chromium** — Must handle repositories with millions of objects and
   tens of gigabytes of data.
4. **Self-hostable** — Runs on our infrastructure, not a SaaS dependency.

### Research questions

1. **What existing tools or libraries implement a Git backend on top of a
   database?** Examples might include custom Git object storage backends,
   virtual filesystems, or Git protocol servers that translate operations into
   database queries.

2. **What databases are suitable?** PostgreSQL with large object support? Object
   storage (S3) with a metadata database? A key-value store like TiKV or
   FoundationDB? Something purpose-built?

3. **How does the Git smart HTTP/SSH protocol work at the transport level?**
   Specifically, what does a server need to implement to handle `git push` and
   `git pull`? Understanding this helps evaluate whether we can intercept at the
   protocol level and route to a database.

4. **How do existing large-scale Git hosts handle this?** GitHub, GitLab.com,
   and Bitbucket all operate at scales far beyond filesystem-based hosting.
   What's known about their architectures? (GitHub's Spokes/DGit, GitLab's
   Gitaly, etc.)

5. **Are there Git libraries (in Rust, Go, or TypeScript) that abstract object
   storage?** For example, `gitoxide` (Rust) or `go-git` (Go) — do they support
   pluggable backends?

6. **What about partial/shallow clones and Git LFS?** Chromium uses neither LFS
   nor shallow clones by default, but supporting these could reduce bandwidth
   and storage requirements.

7. **Is there a hybrid approach?** For example, serving the Git protocol from a
   lightweight process that reads/writes to a database, while using a CDN or
   object store for large blobs.

### Known projects to investigate

- **Gitaly** (GitLab) — Git RPC service. Abstracts Git operations behind gRPC.
  Filesystem-backed but interesting architecture.
- **Spokes / DGit** (GitHub) — GitHub's distributed Git storage. Not
  open-source, but architecture is documented in blog posts.
- **go-git** — Pure Go Git implementation with pluggable storage backends
  (memory, filesystem, custom). Could potentially back onto a database.
- **gitoxide** — Pure Rust Git implementation. May support custom object
  databases.
- **Jujutsu (jj)** — Git-compatible VCS with a different internal model. Uses
  its own storage but interops with Git.
- **Dulwich** — Pure Python Git implementation with pluggable backends.
- **git-lfs-s3** / **Git LFS** — Large file storage over S3. Relevant for
  handling Chromium's size.
- **Noms / Dolt** — Version-controlled databases. Opposite direction (database
  with Git semantics) but architecturally interesting.
- **Soft Serve** — Self-hosted Git server (Go, Charm). Filesystem-backed but
  minimal and hackable.
- **Forgejo / Gitea** — Self-hosted Git forges. Filesystem-backed, but worth
  understanding their protocol handling.

## Experiments

### Experiment 1: First-pass internet research

#### Description

Survey the landscape of database-backed Git hosting. For each of the research
questions above, search the internet for existing solutions, blog posts,
architecture docs, and open-source projects. Summarize findings in a structured
report below.

#### Research plan

1. **go-git pluggable backends** — Search for go-git's storage interface, what
   backends exist (PostgreSQL, S3, etc.), and whether anyone has built a
   database-backed Git server with it.

2. **gitoxide custom backends** — Search for gitoxide's object database
   abstraction, whether it supports pluggable storage, and its maturity.

3. **GitHub architecture** — Search for blog posts about Spokes, DGit, and
   GitHub's move away from filesystem-based storage. How do they handle the Git
   protocol?

4. **GitLab Gitaly** — Search for Gitaly's architecture. It's gRPC over
   filesystem, but understanding its abstraction layer matters.

5. **Database-backed Git implementations** — Search specifically for projects
   that store Git objects in PostgreSQL, SQLite, S3, or key-value stores. Look
   for proof-of-concept projects, academic papers, and production systems.

6. **Git smart protocol** — Search for documentation on the Git smart HTTP and
   SSH transport protocols. What RPCs does a server need to implement?

7. **Chromium-scale considerations** — Search for how large monorepos are
   hosted. How does Google host Chromium? What about Microsoft's VFSForGit (now
   Scalar)?

8. **Jujutsu and alternative VCS** — Search for Jujutsu's storage model and
   whether its Git interop could serve as a Git hosting layer.

#### Findings

##### 1. go-git pluggable backends

go-git has a fully pluggable `Storer` interface with six sub-interfaces
(EncodedObjectStorer, ReferenceStorer, ShallowStorer, IndexStorer, ConfigStorer,
ModuleStorer). Two built-in backends: filesystem and memory. The repo ships an
Aerospike database example at `_examples/storage/`.

go-git also has a **built-in server transport** at `plumbing/transport/server` —
implements `git-upload-pack` (pull/fetch) and `git-receive-pack` (push). A
`MapLoader` maps endpoint strings to custom `Storer` implementations. A new
`backend/http` package provides Smart HTTP handlers returning `http.Handler`.

**Limitations:** ~8x memory overhead vs native git, ~4x slower cloning for large
repos, no pack protocol v2, no partial clone, single-threaded packfile
processing. No maintained PostgreSQL or S3 backend exists — the Aerospike
example is the only database reference.

**Chromium scale:** Problematic. The memory overhead and lack of partial clone
make it unsuitable for Chromium-sized repos without significant work. Could work
for smaller repos.

##### 2. gitoxide custom backends

gitoxide does **not** support pluggable object storage backends. The ODB
(`gix_odb::Store`) is hardcoded to Git's on-disk format (loose objects +
packfiles). There is no trait to implement a custom backend.

gitoxide also has **no server-side protocol implementation** — `gix-transport`
and `gix-protocol` are client-only. The maintainer recommends using lower-level
crates (`gix-pack`, `gix-object`, `gix-revwalk`) as building blocks but
implementing the server yourself.

By contrast, **libgit2** (`git2` crate) has an explicit `Odb` backend API where
you can register custom backends via `OdbBackend`.

**Chromium scale:** Not applicable — no pluggable backend, no server support.

##### 3. GitHub architecture

GitHub uses **Spokes** (formerly DGit) — application-layer replication of bare
Git repos on local disk. Three replicas per repo, three-phase commit for writes,
any replica can serve reads. MySQL for all metadata (routing, auth, repo
ownership). Standard Git on file servers, not a proprietary fork.

330+ million repos, ~19 PB of data. GitHub invested in upstream Git features:
cruft packs (52-92% size reduction), geometric repacking, MIDX improvements,
"Project Cyclops" for monorepo push performance.

**Chromium scale:** GitHub handles it, but with filesystem storage + Spokes
replication, not a database backend.

##### 4. GitLab Gitaly

Gitaly is a Go gRPC service that wraps all Git operations behind protobuf RPCs.
**Strictly filesystem-backed** — spawns `git` CLI processes and uses libgit2 via
`gitaly-git2go`. NFS is explicitly unsupported.

Gitaly Cluster (Praefect) adds replication across 3+ nodes with PostgreSQL for
cluster metadata only. Scales to ~2,000+ active users per cluster. Hugging Face
and Scalingo use it.

The gRPC interface is clean — you could theoretically reimplement it with a
database backend, but Gitaly itself is deeply coupled to the filesystem.

**Chromium scale:** Production-proven at GitLab.com scale, but filesystem-only.

##### 5. Database-backed Git implementations

**Gitgres** (Feb 2026) — Stores Git objects and refs in PostgreSQL. ~2,000 lines
of C implementing `git_odb_backend` and `git_refdb_backend` via libpq. Standard
`git push`/`git clone` work through a `git-remote-gitgres` helper. Missing full
`upload-pack`/`receive-pack` over SSH/HTTP. Being explored as a Forgejo backend.
[github.com/andrew/gitgres](https://github.com/andrew/gitgres)

**libgit2-backends** — Official standalone ODB backends for libgit2 targeting
SQLite, MySQL, Redis, and Memcached. Reference implementations, not widely
deployed.
[github.com/libgit2/libgit2-backends](https://github.com/libgit2/libgit2-backends)

**git-remote-s3 (AWS Labs)** — S3 as a serverless Git remote. Also supports Git
LFS to the same bucket.
[github.com/awslabs/git-remote-s3](https://github.com/awslabs/git-remote-s3)

**git-remote-s3 (Rust)** — Independent Rust implementation using gitoxide and
rust-s3.
[github.com/josephvoss/git-remote-s3](https://github.com/josephvoss/git-remote-s3)

**JGit** — Java Git implementation with built-in S3 support
(`jgit clone amazon-s3://...`).

No existing projects use **FoundationDB** or **TiKV** as Git backends, though
both are architecturally well-suited (ordered KV with transactions).

**Chromium scale:** Gitgres is a PoC — no scale testing. S3 backends could
handle blob storage at scale but add latency. FoundationDB/TiKV would need
greenfield implementation.

##### 6. Git smart protocol

A smart HTTP server needs **4 endpoints**:

| Endpoint                                   | Method | Purpose        |
| ------------------------------------------ | ------ | -------------- |
| `$REPO/info/refs?service=git-upload-pack`  | GET    | Ref discovery  |
| `$REPO/git-upload-pack`                    | POST   | Fetch/clone    |
| `$REPO/info/refs?service=git-receive-pack` | GET    | Push ref disc. |
| `$REPO/git-receive-pack`                   | POST   | Push           |

All data uses pkt-line framing. The protocol is \~30% ref management, \~60%
object transfer (packfile negotiation + streaming), \~10% capabilities/framing.

**Key insight:** The wire protocol doesn't care about on-disk format. A server
needs to: enumerate refs, resolve object graphs, generate packfiles, and unpack
received packfiles. Internal storage is an implementation detail.

The simplest servers shell out to `git upload-pack --stateless-rpc` and
`git receive-pack --stateless-rpc` and pipe stdin/stdout through HTTP. Grack
(Ruby) does this in ~200 lines.

Protocol v2 improves ref advertisement (client requests only needed refs instead
of receiving all), but push still uses v1's `receive-pack`.

##### 7. Chromium-scale considerations

Chromium: 35M+ lines of code, 1M+ lifetime commits, 100K+ commits/year, 15-20+
GB clone. Google hosts it with **Gerrit** (code review + push) and **Gitiles**
(read-only browser), both open source, both built on JGit.

**Microsoft Scalar** (now built into Git since 2.38): configures partial clone,
sparse checkout, background maintenance, commit graphs, and fsmonitor. Replaced
VFSForGit (which used a kernel-level virtual filesystem).

Key server-side techniques for large repos:

- **Partial clone** (`--filter=blob:none`) — fetch blobs on demand
- **Reachability bitmaps** — per-commit bitsets, 50%+ clone speedup
- **Commit graph** — precomputed graph file for fast log/merge-base
- **Multi-pack index (MIDX)** — index across multiple packfiles
- **Cruft packs** — compact unreachable objects (52-92% size reduction)

##### 8. Jujutsu

Jujutsu is a **client-side tool** with no server component. It has a clean
pluggable `Backend` trait (Google uses it internally with a custom cloud
backend), but the only production-ready backend is `GitBackend` which stores
data in a real Git repo via gitoxide.

No Git protocol server. Network operations (`jj git fetch/push`) shell out to
the Git CLI. A server/daemon architecture is on the roadmap but not implemented.

**Chromium scale:** Not applicable for hosting.

#### Candidate approaches

**Approach A: go-git + PostgreSQL.** Use go-git's pluggable `Storer` interface
with a PostgreSQL backend, and go-git's built-in server transport for smart
HTTP. This is the most direct path — go-git provides both the storage
abstraction and the protocol implementation. Main risk is performance at
Chromium scale (8x memory overhead, no partial clone). Could work for TermSurf's
own repos; may need a hybrid approach for Chromium (e.g., native git for
transport, database for storage).

**Approach B: libgit2 + PostgreSQL (Gitgres model).** Use libgit2's pluggable
ODB with a PostgreSQL backend (as Gitgres demonstrates). Implement
upload-pack/receive-pack over HTTP using libgit2's pack generation. libgit2 is
more mature than go-git for object manipulation and has better memory
characteristics. Could be called from Rust via the `git2` crate or from Go via
`git2go`. Main risk is that Gitgres is early-stage and libgit2's server-side
story is less complete than go-git's.

**Approach C: Hybrid — native git transport + database object store.** Use
standard `git upload-pack --stateless-rpc` / `git receive-pack --stateless-rpc`
for the protocol layer (shelling out like Grack/Gitea), but intercept object
storage at the filesystem level using FUSE or a custom git alternate. Objects
are read from / written to a database, but git thinks it's talking to a
filesystem. This avoids reimplementing the protocol but adds the complexity of a
virtual filesystem layer. Microsoft's VFSForGit proves the concept (though they
abandoned it for Scalar).

**Approach D: S3/object storage + metadata database.** Store packfiles and large
blobs in S3/GCS, with PostgreSQL for refs, metadata, and small objects. Use
native git or go-git for protocol handling. This is a pragmatic split: the
database handles queryable metadata, object storage handles bulk data. AWS's
git-remote-s3 and JGit's S3 support are precedents. Scales well for Chromium
blob storage but adds latency for small object access.

#### Verification

1. Each research question has at least one substantive finding.
2. At least 3 concrete candidate approaches are identified for further
   evaluation.
3. Chromium-scale feasibility is addressed for each candidate.

### Experiment 2: Storage architecture and hosting decision

#### Description

Based on experiment 1 findings, decide between database-backed Git and
filesystem-based Git, and choose a hosting provider. The key factors are:
Chromium + WebKit + Firefox + Ladybird repos (each 10-40 GB), plus TermSurf's
own repos and a website.

#### Findings

##### Database-backed vs. filesystem-based Git

Every major Git host (GitHub, GitLab, Google) uses **filesystem-based storage**.
No one runs database-backed Git in production at scale. The reasons:

- Git's scale features (reachability bitmaps, commit graphs, MIDX, cruft packs,
  partial clone) all assume the filesystem format. A database backend would need
  to reimplement each one.
- The Git wire protocol is storage-agnostic — a thin HTTP layer shells out to
  `git upload-pack --stateless-rpc` / `git receive-pack --stateless-rpc` and
  pipes stdin/stdout. This is how Gitea, Gogs, and Grack work (~200 lines).
- Hosting 4-5 giant repos (Chromium, WebKit, Firefox, Ladybird) through a
  database is uncharted territory with multiplicative risk.

**Decision: Filesystem-based Git (bare repos on NVMe SSD).** PostgreSQL for
everything else — users, permissions, repo metadata, issues. This is the
Gitea/Forgejo model.

##### Hosting provider comparison

**OVHcloud dedicated server (Dallas, TX):**

- AMD EPYC 4245P (6c/12t), 32-256 GB RAM, NVMe SSD
- ~$134/month + $134 setup
- 1-5 Gbps public bandwidth, unmetered
- Full root access, bare metal
- Perfect for git hosting: fast NVMe, unmetered bandwidth, full disk control
- Chromium clones (20 GB) cost nothing in bandwidth

**Fly.io (Dallas, TX — `dfw` region):**

- Managed micro-VMs (Firecracker), up to 8 cores / 16 GB RAM
- Volumes: local NVMe, priced per GB, single-region
- Bandwidth: ~$0.02/GB egress after free tier
- ~$30-60/month for a small web app
- Poor fit for git hosting: metered bandwidth, limited disk, no
  `git maintenance` cron jobs, volumes not designed for 100+ GB repos

**Other Texas options investigated:**

- Hetzner: US datacenter in Ashburn, VA (not Texas). ~$50-80/month.
- Limestone Networks: Dallas, TX. Downtown Dallas datacenter. Contact for
  pricing.
- SpinServers: Dallas, TX. Aggressive pricing on high-spec hardware.
- Austin colocation: Thin-nology (~$110/month colo fee + hardware cost). More
  expensive than renting a dedicated server.

| Concern                      | OVHcloud     | Fly.io        |
| ---------------------------- | ------------ | ------------- |
| Git hosting (Chromium-scale) | Excellent    | Poor fit      |
| Website hosting              | Fine         | Excellent     |
| Bandwidth for large clones   | Unmetered    | Metered ($$$) |
| Disk I/O control             | Full         | Limited       |
| Ops burden                   | Self-managed | Managed       |
| Scaling web traffic          | Manual       | Automatic     |

**Decision: OVHcloud dedicated server in Dallas.** One box handles the website,
git hosting, and PostgreSQL. Unmetered bandwidth is critical for hosting
Chromium-scale repos. Fly.io is currently used for the website
(`"deploy": "fly deploy"` in package.json) but should be consolidated onto the
dedicated server once git hosting is added.

#### Verification

1. Architecture decision documented: filesystem-based Git + PostgreSQL for
   metadata.
2. Hosting decision documented: OVHcloud dedicated in Dallas, TX.
3. Trade-offs between OVH and Fly.io analyzed for both website and git hosting.
