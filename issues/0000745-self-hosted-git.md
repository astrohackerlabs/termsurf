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

_{to be filled in after research}_

#### Verification

1. Each research question has at least one substantive finding.
2. At least 3 concrete candidate approaches are identified for further
   evaluation.
3. Chromium-scale feasibility is addressed for each candidate.
