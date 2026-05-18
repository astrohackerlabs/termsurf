+++
status = "closed"
opened = "2026-03-15"
closed = "2026-03-15"
+++

# Issue 754: Screenshot pipeline and homepage hero

## Goal

Add `assets/screenshot2.png` to the README (for GitHub) and to the website
homepage as a hero image. Create a reproducible pipeline to convert screenshots
to lossless WebP for the website. This pipeline will be reused for all future
screenshots on the website, docs, and blog.

## Background

### The screenshot

`assets/screenshot2.png` is a new screenshot of TermSurf in action. It should
appear in two places:

1. **README.md** — PNG, referenced directly from `assets/`. This is what shows
   up on GitHub.
2. **Website homepage** — Lossless WebP, served from the website's public
   assets.

### Why WebP

PNG screenshots from macOS Retina displays are large (often 2–5 MB). Lossless
WebP produces identical quality at ~30–50% smaller file sizes. Every screenshot
on the website should be WebP for faster page loads.

### Reproducible conversion

We need a script in `scripts/` that converts PNG screenshots to lossless WebP.
This script will be used every time a new screenshot is added. The website
already uses `sharp` for icon processing (`website/scripts/process-icons.ts`),
so we can use the same library — or use `cwebp` from the command line if
available.

### What needs to happen

1. Replace the old screenshot in `README.md` with the new one
2. Create a script to convert PNG screenshots to lossless WebP
3. Convert `assets/screenshot2.png` to WebP and place it in
   `website/public/images/`
4. Add the screenshot to the website homepage (`website/src/routes/index.tsx`)

## Experiments

### Experiment 1: Update README screenshot

#### Description

Replace the existing `assets/screenshot.png` reference in `README.md` with
`assets/screenshot2.png`.

#### Changes

**`README.md`**

Change the image reference on line 12 from `assets/screenshot.png` to
`assets/screenshot2.png`. Update the alt text if needed.

#### Verification

View the README on GitHub (or locally) and confirm the new screenshot appears.

### Experiment 2: PNG to WebP conversion script

#### Description

Create `scripts/png-to-webp.sh` that converts any PNG file to lossless WebP. The
script takes an input path and an output path. It uses a small TypeScript helper
that calls `sharp` (already a dependency in `website/`).

#### Changes

**`scripts/png-to-webp.sh`**

```bash
#!/usr/bin/env bash
set -euo pipefail

if [ $# -lt 2 ]; then
  echo "Usage: $0 <input.png> <output.webp>"
  exit 1
fi

INPUT="$1"
OUTPUT="$2"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"

bun "$REPO_DIR/website/scripts/png-to-webp.ts" "$INPUT" "$OUTPUT"
```

**`website/scripts/png-to-webp.ts`**

```typescript
import sharp from "sharp";

const input = process.argv[2];
const output = process.argv[3];

if (!input || !output) {
  console.error("Usage: bun png-to-webp.ts <input.png> <output.webp>");
  process.exit(1);
}

await sharp(input).webp({ lossless: true }).toFile(output);
console.log(`  ${input} → ${output}`);
```

#### Verification

```bash
scripts/png-to-webp.sh assets/screenshot2.png website/public/images/screenshot2.webp
```

Verify the output file exists and is a valid WebP image. Compare file sizes —
WebP should be smaller than the PNG.

### Experiment 3: Add screenshot to homepage

#### Description

Add the WebP screenshot to the homepage as a hero image, right after the tagline
and divider, before the "Latest Post" section. Full content width. No caption,
no border-radius, no effects. Just the image.

#### Changes

**`website/src/routes/index.tsx`**

After the divider `<div>` (~line 50–52), add a new section with the screenshot:

```tsx
<section className="mb-8">
  <img
    src="/images/screenshot2.webp"
    alt="TermSurf — a browser pane alongside terminal panes"
    className="w-full"
  />
</section>
```

#### Verification

```bash
cd website && bun run dev
```

Visit `http://localhost:3000`. The screenshot should appear below the tagline
and above the latest post. Full width, no distortion.
