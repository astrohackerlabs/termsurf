# AGENTS.md — Red patches

Red is the upstream-derived product fork for RedVim. Follow `../AGENTS.md`
and this directory's reconstruction README. Product checkout: `forks/red`.
This is independent of TermSurf and NuTorch; their publishers must not prepare
or package it.

Use `issue-{ISSUE_ID}-exp{N}-{slug}` branches. Archive the ordered cumulative
series from the recorded upstream base under `patches/issue-{ISSUE_ID}/`.
Update the reconstruction README, manifest and issue evidence together.
Never rewrite old issue archives. If Red's checkout has an `AGENTS.md`, it is
ignored with the clone. Record that file in the patch series; do not track
`forks/red/AGENTS.md` in the monorepo.

Keep upstream LICENSE, attribution and dependency notices. The current fork builds `redvim` with bundled Nushell support. Publication
and installed qualification require Experiment 5 evidence.
