import { defineCollection } from "astro:content";
import { glob } from "astro/loaders";
import { z } from "zod";

// Docs content collection. Entry IDs are the path relative to the `base`
// directory (e.g. `components/webtui`), which maps 1:1 to the doc URL
// `/docs/components/webtui`.
const docs = defineCollection({
  loader: glob({ base: "./src/content/docs", pattern: "**/*.{md,mdx}" }),
  schema: z.object({
    // Page <title> (and accessible name); the MDX body carries its own <h1>.
    title: z.string(),
    // Optional shorter label for the sidebar; falls back to `title`.
    navLabel: z.string().optional(),
    // Used for <meta name="description"> and section indexes.
    description: z.string().optional(),
    // Sidebar group heading. Ungrouped entries render above the first group.
    section: z.string().optional(),
    // Sort order within a section (lower first); unset sorts last.
    order: z.number().default(1000),
    // Excluded from the build and from navigation.
    draft: z.boolean().default(false),
  }),
});

export const collections = { docs };
