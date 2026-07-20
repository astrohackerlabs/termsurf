# WebKit Patch Archives

This directory stores Astrohacker Terminal WebKit patch sets generated from
`forks/webkit/src`.

Each issue that modifies WebKit source should get a subdirectory:

```text
patches/webkit/patches/issue-{N}/
```

Generate patches from the recorded upstream base commit to the issue branch tip:

```bash
rm -rf patches/webkit/patches/issue-{N}
mkdir -p patches/webkit/patches/issue-{N}
git -C forks/webkit/src format-patch {base-commit}..HEAD \
  -o ../../../patches/webkit/patches/issue-{N}
```

Apply patches from a fresh checkout with:

```bash
git -C forks/webkit/src switch -C webkit-{short-base}-issue-{N} {base-commit}
git -C forks/webkit/src am ../../../patches/webkit/patches/issue-{N}/*.patch
```

Issue 26031612000756 archives WebKit source patches in `issue-26031612000756/`. Experiment 12 added
the first patch, a macOS `PageClientImpl` cursor notification hook used by
Surfari cursor callbacks.
