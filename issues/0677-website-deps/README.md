+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

# Issue 677: Update Website Dependencies

Update all outdated dependencies in `website/package.json` and fix any breaking
changes.

## Background

The website dependencies are significantly out of date. Several packages have
major version bumps available.

### Current state (`bun outdated`)

| Package                 | Current | Latest  | Notes     |
| ----------------------- | ------- | ------- | --------- |
| @tanstack/react-router  | 1.147.1 | 1.163.3 | minor     |
| @tanstack/react-start   | 1.147.1 | 1.163.3 | minor     |
| @tanstack/router-plugin | 1.147.1 | 1.163.3 | minor     |
| react                   | 19.2.3  | 19.2.4  | patch     |
| react-dom               | 19.2.3  | 19.2.4  | patch     |
| tailwind-merge          | 2.6.0   | 3.5.0   | **major** |
| @tailwindcss/vite       | 4.1.18  | 4.2.1   | minor     |
| @types/bun              | 1.3.5   | 1.3.9   | patch     |
| @types/react            | 19.2.8  | 19.2.14 | patch     |
| @vitejs/plugin-react    | 4.7.0   | 5.1.4   | **major** |
| tailwindcss             | 4.1.18  | 4.2.1   | minor     |
| vite                    | 6.4.1   | 7.3.1   | **major** |
| vite-tsconfig-paths     | 5.1.4   | 6.1.1   | **major** |

Four major version bumps: `tailwind-merge` (2→3), `@vitejs/plugin-react` (4→5),
`vite` (6→7), and `vite-tsconfig-paths` (5→6). These may have breaking changes.

## Experiment 1: Update all dependencies

### Hypothesis

Updating all dependencies at once with `bun update` and fixing any build errors
will bring the website up to date.

### Changes

#### 1. Update all dependencies

```bash
cd website && bun update
```

#### 2. Fix breaking changes

Check changelogs for the four major bumps and fix any issues:

- **tailwind-merge 2→3**: May have API changes to `twMerge` or `twJoin`
- **@vitejs/plugin-react 4→5**: May require Vite 7
- **vite 6→7**: May have config changes
- **vite-tsconfig-paths 5→6**: May require Vite 7

#### 3. Build and test

```bash
bun run build
bun run dev  # verify dev server works
```

### Test

1. `bun run build` — compiles without errors
2. `bun run dev` — dev server starts and pages render correctly
3. `bun outdated` — no outdated packages

### Result: PASS

All 13 packages updated. Four major version bumps (tailwind-merge 3, vite 7,
@vitejs/plugin-react 5, vite-tsconfig-paths 6) were drop-in replacements — no
code changes needed. `bun run build` succeeds, `bun outdated` shows zero
outdated packages.

Note: `bun update` only bumps within semver `^` ranges. The four major versions
required explicit `bun add pkg@latest` to cross the major boundary.

## Conclusion

All website dependencies updated to latest. No breaking changes encountered.

- `website/package.json` — all 13 packages bumped to latest
- `website/bun.lock` — regenerated
