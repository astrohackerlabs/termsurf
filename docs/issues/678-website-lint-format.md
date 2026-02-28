# Issue 678: Website Linting and Formatting

Add `oxlint` and `oxfmt` to the website for linting and formatting.

## Background

The website has no linter or formatter configured. Adding oxc's tools gives us
fast, consistent code quality checks.

- **oxlint** — 650+ rules, 50-100x faster than ESLint
- **oxfmt** — 30x faster than Prettier, 3x faster than Biome

## Experiment 1: Set up oxlint and oxfmt

### Hypothesis

Installing both tools, generating default configs, and adding package.json
scripts will give us working lint and format commands.

### Changes

#### 1. Install dependencies

```bash
cd website && bun add -D oxlint oxfmt
```

#### 2. Generate configs

```bash
oxlint --init   # creates .oxlintrc.json
oxfmt --init    # creates .oxfmtrc.json
```

#### 3. Add scripts to package.json

```json
"lint": "oxlint",
"lint:fix": "oxlint --fix",
"fmt": "oxfmt",
"fmt:check": "oxfmt --check"
```

#### 4. Run and fix

Run `bun run lint` and `bun run fmt` to check the current codebase. Fix any lint
errors and format all files.

### Test

1. `bun run lint` — no errors
2. `bun run fmt:check` — all files formatted
3. `bun run build` — still compiles after formatting
