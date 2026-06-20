import { getCollection } from "astro:content";

export interface DocsNavItem {
  href: string;
  label: string;
}

export interface DocsNavSubgroup {
  subsection: string;
  items: DocsNavItem[];
}

export interface DocsNavGroup {
  /** Section heading, or null for the ungrouped items that lead the sidebar. */
  section: string | null;
  /** Items in this section with no subsection (rendered flat, before groups). */
  items: DocsNavItem[];
  /** Second-level subsection groups (e.g. VT: CSI/OSC/...), in explicit order. */
  subgroups: DocsNavSubgroup[];
}

// Explicit section order (replaces the old alphabetical reliance). Ungrouped
// entries always lead; unknown sections sort after these, alphabetically.
const SECTION_ORDER = ["Components", "Protocol", "Reference", "Terminal API"];
// Explicit subsection order within "Terminal API".
const SUBSECTION_ORDER = ["Concepts", "Control", "CSI", "ESC", "OSC"];

function rank(value: string, order: string[]): number {
  const i = order.indexOf(value);
  return i === -1 ? order.length : i;
}

function itemOf(entry: { id: string; data: { navLabel?: string; title: string } }): DocsNavItem {
  return {
    href: `/docs/${entry.id}`,
    label: entry.data.navLabel ?? entry.data.title,
  };
}

// Build the docs sidebar from the `docs` collection: drop drafts, then group by
// section and (within a section) by subsection, with explicit ordering and
// (order, title) within each leaf list. Single source of truth for the sidebar.
export async function getDocsNav(): Promise<DocsNavGroup[]> {
  const entries = await getCollection("docs", ({ data }) => !data.draft);

  const within = (
    a: { data: { order: number; title: string } },
    b: { data: { order: number; title: string } },
  ) =>
    a.data.order !== b.data.order
      ? a.data.order - b.data.order
      : a.data.title.localeCompare(b.data.title);

  // Bucket by section.
  const sections = new Map<string | null, typeof entries>();
  for (const entry of entries) {
    const key = entry.data.section ?? null;
    if (!sections.has(key)) sections.set(key, []);
    sections.get(key)!.push(entry);
  }

  const groups: DocsNavGroup[] = [];
  for (const [section, sectionEntries] of sections) {
    const flat = sectionEntries.filter((e) => !e.data.subsection).sort(within);

    const subMap = new Map<string, typeof entries>();
    for (const e of sectionEntries) {
      if (!e.data.subsection) continue;
      if (!subMap.has(e.data.subsection)) subMap.set(e.data.subsection, []);
      subMap.get(e.data.subsection)!.push(e);
    }
    const subgroups: DocsNavSubgroup[] = [...subMap.entries()]
      .sort((a, b) => rank(a[0], SUBSECTION_ORDER) - rank(b[0], SUBSECTION_ORDER) || a[0].localeCompare(b[0]))
      .map(([subsection, es]) => ({ subsection, items: es.sort(within).map(itemOf) }));

    groups.push({ section, items: flat.map(itemOf), subgroups });
  }

  // Order sections: ungrouped (null) first, then explicit order, then unknown.
  groups.sort((a, b) => {
    if (a.section === null) return -1;
    if (b.section === null) return 1;
    const ra = rank(a.section, SECTION_ORDER);
    const rb = rank(b.section, SECTION_ORDER);
    return ra - rb || a.section.localeCompare(b.section);
  });

  return groups;
}
