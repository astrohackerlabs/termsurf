# Release Procedure (TermSurf 1.x)

> **Scope:** This document applies to TermSurf 1.x releases.
> TermSurf 2.0 will have a different release process based on Rust/Cargo tooling.

This document describes how to make a new release of TermSurf.

## Prerequisites

- All changes committed to `main` branch
- Working Xcode installation

## Steps

### 1. Update Version Numbers

Update the version in **two places**:

#### a) build.zig.zon

Edit `build.zig.zon` and update the version field:

```zig
.version = "X.Y.Z",
```

**Important:** The version here must match the git tag you'll create (without the `v` prefix). The build system enforces this—if they don't match, `zig build` will fail with "tagged releases must be in vX.Y.Z format matching build.zig".

#### b) Xcode Project (MARKETING_VERSION)

Update `MARKETING_VERSION` in the Xcode project. This controls the version shown in the About box.

**Option 1 - In Xcode:**
1. Open `termsurf-macos/TermSurf.xcodeproj`
2. Select the TermSurf target → Build Settings → Search for "marketing"
3. Update `MARKETING_VERSION` to `X.Y.Z`

**Option 2 - Via command line:**
```bash
sed -i '' 's/MARKETING_VERSION = [0-9]*\.[0-9]*\.[0-9]*/MARKETING_VERSION = X.Y.Z/g' \
  termsurf-macos/TermSurf.xcodeproj/project.pbxproj
```

### 2. Update CHANGELOG.md

Add a new section for the release version with a summary of changes.

### 3. Commit Version Bump

```bash
git add build.zig.zon CHANGELOG.md
git commit -m "Bump version to X.Y.Z"
```

### 4. Verify Build

Build the app to ensure it compiles without errors:

```bash
zig build
cd termsurf-macos && xcodebuild -project TermSurf.xcodeproj -scheme TermSurf -configuration Release build
```

### 5. Tag the Release

Create an annotated tag for the new version:

```bash
git tag -a vX.Y.Z -m "Release vX.Y.Z"
```

### 6. Push to GitHub

Push the main branch and the new tag:

```bash
git push origin main
git push origin vX.Y.Z
```

Or push all tags at once:

```bash
git push origin main --tags
```

### 7. Deploy Website

Update the commit log and version on termsurf.com:

```bash
cd website
bun run build:data
bun run deploy
```

This rebuilds the commit and version data from git history and deploys the updated website to Fly.io.

## Version Numbering

We use semantic versioning (MAJOR.MINOR.PATCH):

- **PATCH** (0.0.x): Bug fixes, small improvements
- **MINOR** (0.x.0): New features, backward compatible
- **MAJOR** (x.0.0): Breaking changes

## Future

When we publish builds (e.g., Homebrew, GitHub Releases with binaries), this
document will be expanded with:

- Building signed release binaries
- Creating GitHub Release with release notes
- Publishing to package managers
