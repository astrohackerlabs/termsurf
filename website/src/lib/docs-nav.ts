import { getCollection } from "astro:content";

export interface DocsNavItem {
  href: string;
  label: string;
}

export interface DocsNavGroup {
  /** Section heading, or null for the ungrouped items that lead the sidebar. */
  section: string | null;
  items: DocsNavItem[];
}

// Build the docs sidebar from the `docs` collection: drop drafts, sort by
// (section, order, title), and group by section with ungrouped entries first.
// Single source of truth for the sidebar (and later section indexes/sitemap).
export async function getDocsNav(): Promise<DocsNavGroup[]> {
  const entries = await getCollection("docs", ({ data }) => !data.draft);

  entries.sort((a, b) => {
    const sa = a.data.section ?? "";
    const sb = b.data.section ?? "";
    if (sa !== sb) {
      // Ungrouped (empty section) sorts before any named section.
      if (sa === "") return -1;
      if (sb === "") return 1;
      return sa.localeCompare(sb);
    }
    if (a.data.order !== b.data.order) return a.data.order - b.data.order;
    return a.data.title.localeCompare(b.data.title);
  });

  const groups: DocsNavGroup[] = [];
  for (const entry of entries) {
    const section = entry.data.section ?? null;
    const item: DocsNavItem = {
      href: `/docs/${entry.id}`,
      label: entry.data.navLabel ?? entry.data.title,
    };
    const last = groups[groups.length - 1];
    if (last && last.section === section) {
      last.items.push(item);
    } else {
      groups.push({ section, items: [item] });
    }
  }
  return groups;
}
