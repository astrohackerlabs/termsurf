# Docs

Operational monorepo documentation for Astrohacker Terminal: install/release,
environment naming, legal package sources, and public-source templates.

**User-facing product docs** live on the website
([astrohacker.com/docs](https://astrohacker.com/docs) via
`bun/website/app/ui/docs/`). Craft rules:
[`docs/marketing/docs-writing.md`](./marketing/docs-writing.md).

| Path | Role |
| --- | --- |
| [`homebrew.md`](./homebrew.md) | Canonical install + release (Apple silicon cask) |
| [`environment.md`](./environment.md) | Process env taxonomy |
| `astrohacker-terminal-license` / `-notice` / `-trademarks.md` | Legal package sources |
| [`public-source/`](./public-source/) | Templates for public source mirror |

## Business records (under this tree)

| Path | Role |
| --- | --- |
| [`issues/`](./issues/) | Issues and experiments (catalog: gitignored `issues/INDEX.md`) |
| [`epics/`](./epics/) | Epics (catalog: gitignored `epics/INDEX.md`) |
| [`marketing/`](./marketing/) | Brand, voice, blog, docs-writing canon |

Agent workflows: `skills/issues-and-experiments`, `skills/epics`. Create issues with
`scripts/create-issue.sh` (defaults to `docs/issues/`).

Recover deleted monorepo notes (e.g. historical XDG/keybindings/Ghostty essays)
from git history if needed.
