# termsurf.com Website

Project page for TermSurf showing commit history and project info.

## Tech Stack

- **Runtime:** Bun
- **Framework:** TanStack Start (React)
- **Styling:** Tailwind CSS v4 + shadcn/ui (Tokyo Night theme, dark mode only)

## Dependencies

| Package                  | Version  | Purpose                                 |
| ------------------------ | -------- | --------------------------------------- |
| `@tanstack/react-start`  | ^1.147.0 | Full-stack React framework              |
| `@tanstack/react-router` | ^1.147.0 | Type-safe routing                       |
| `vite`                   | ^6.0.0   | Build tool                              |
| `react`                  | ^19.0.0  | UI library                              |
| `react-dom`              | ^19.0.0  | React DOM bindings                      |
| `tailwindcss`            | ^4.0.0   | Utility-first CSS framework             |
| `@tailwindcss/vite`      | ^4.0.0   | Tailwind Vite plugin                    |
| `clsx`                   | ^2.1.0   | Class name utility                      |
| `tailwind-merge`         | ^2.6.0   | Merge Tailwind classes                  |

Note: shadcn/ui components are installed on-demand via CLI, not as a package dependency.

## Directory Structure

```
website/
├── package.json
├── vite.config.ts          # Vite + TanStack Start configuration
├── tsconfig.json
├── src/
│   ├── router.tsx          # Router configuration
│   ├── globals.css         # Tailwind imports + Tokyo Night theme
│   ├── vite-env.d.ts       # Vite type declarations
│   ├── routes/
│   │   ├── __root.tsx      # Root layout
│   │   └── index.tsx       # Home page (commit log)
│   ├── components/
│   │   ├── ui/             # shadcn/ui components (future)
│   │   ├── Header.tsx      # Site header
│   │   └── CommitLog.tsx   # Commit list display
│   └── lib/
│       └── utils.ts        # Utility functions (cn helper)
├── data/
│   └── commits.json        # Pre-built commit data
├── scripts/
│   └── build-commits.ts    # Script to fetch/build commit data
└── public/
    └── (static assets)
```

## Scripts

| Command                 | Description                                              |
| ----------------------- | -------------------------------------------------------- |
| `bun run dev`           | Start development server with hot reload                 |
| `bun run build`         | Production build                                         |
| `bun run start`         | Start production server                                  |
| `bun run build:commits` | Fetch commits from git and write to data/commits.json    |

## MVP Checklist

### Phase 1: Project Setup

- [x] Create `website/` directory
- [x] Initialize TanStack Start project manually
- [x] Configure vite.config.ts with TanStack Start plugin
- [x] Install dependencies with `bun install`
- [x] Verify dev server runs with `bun run dev`

### Phase 2: Tailwind Setup

- [x] Install Tailwind CSS v4 and Vite plugin
- [x] Create `globals.css` with Tailwind imports and Tokyo Night theme
- [x] Configure Vite to use Tailwind plugin
- [ ] Initialize shadcn/ui (`bunx shadcn@latest init`) - deferred
- [ ] Install any needed shadcn components - deferred

### Phase 3: Commit Data Pipeline

- [x] Create `scripts/build-commits.ts` to fetch commits from git
- [x] Output commit data to `data/commits.json`
- [x] Add `build:commits` script to package.json
- [x] Test: `bun run build:commits` generates valid JSON

### Phase 4: Components

- [x] Create `Header.tsx` with TermSurf branding
- [x] Create `CommitLog.tsx` to render commit list
- [x] Each commit shows: short hash, message, author, relative date
- [x] Use Tailwind classes for styling

### Phase 5: Home Page

- [x] Build `index.tsx` route
- [x] Load commits from `data/commits.json`
- [x] Render Header and CommitLog components
- [x] Verify SSR works correctly

### Phase 6: Polish & Deploy Prep

- [ ] Add meta tags (title, description, og:image)
- [ ] Test production build: `bun run build && bun run start`
- [ ] Document deployment options

## Commit Data Format

`data/commits.json` structure:

```json
{
  "generatedAt": "2025-01-10T12:00:00Z",
  "commits": [
    {
      "hash": "abc1234",
      "message": "Add feature X",
      "author": "ryan",
      "date": "2025-01-10T10:30:00Z"
    }
  ]
}
```

## Tokyo Night Theme

Dark mode only. Colors defined in `src/globals.css` using Tailwind v4 `@theme` directive:

```css
@import "tailwindcss";

@theme {
  --color-background: #1a1b26;
  --color-background-dark: #16161e;
  --color-background-highlight: #292e42;
  --color-foreground: #c0caf5;
  --color-foreground-dark: #a9b1d6;
  --color-primary: #7aa2f7;      /* blue */
  --color-secondary: #bb9af7;    /* magenta */
  --color-accent: #7dcfff;       /* cyan */
  --color-success: #9ece6a;      /* green */
  --color-warning: #e0af68;      /* yellow */
  --color-danger: #f7768e;       /* red */
  --color-muted: #565f89;        /* comment */
  --color-border: #3b4261;
}
```

## Future Enhancements (Post-MVP)

- Pagination / infinite scroll for commits
- Filter by author or date range
- Release notes section
- Download links for latest release
- Project stats (stars, contributors)
- Blog / changelog section
