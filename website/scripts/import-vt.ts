// Imports the Ghostty website's VT (Terminal API) MDX docs into TermSurf's
// content collection (https://github.com/ghostty-org/website, MIT). This is the
// MECHANICAL foundation (issue 834, Experiment 4): copy all pages, inject
// nested-nav frontmatter, adapt links/anchors, and apply only SAFE voice
// transforms — product/behavior claims stay upstream-attributed (true about
// Ghostty) until the TermSurf rebrand + per-claim fork verification of
// Experiment 5+. Output is committed; the website build needs no checkout.
//
// Usage (from website/):
//   bun run import:vt --in /path/to/ghostty-website/docs/vt
//   GHOSTTY_VT_DIR=/path/... bun run import:vt
//   bun run import:vt --check    # exit 1 if committed output is stale

import fs from "fs";
import path from "path";
import GithubSlugger from "github-slugger";

const __dirname = path.dirname(new URL(import.meta.url).pathname);
const websiteDir = path.resolve(__dirname, "..");
const outDir = path.resolve(websiteDir, "src/content/docs/vt");

const args = process.argv.slice(2);
const checkMode = args.includes("--check");
function argVal(flag: string): string | undefined {
  const i = args.indexOf(flag);
  return i >= 0 && i + 1 < args.length ? args[i + 1] : undefined;
}

const srcDir = path.resolve(
  argVal("--in") ?? process.env.GHOSTTY_VT_DIR ?? "/tmp/ghostty-website/docs/vt",
);

if (!fs.existsSync(srcDir)) {
  console.error(
    `import-vt: Ghostty VT docs not found at ${srcDir}\n` +
      `Clone github.com/ghostty-org/website and pass --in <repo>/docs/vt ` +
      `(or set GHOSTTY_VT_DIR).`,
  );
  process.exit(1);
}

// Directory → sidebar subsection label. Top-level files get no subsection.
const SUBSECTION: Record<string, string> = {
  concepts: "Concepts",
  control: "Control",
  csi: "CSI",
  esc: "ESC",
  osc: "OSC",
};

function listMdx(dir: string): string[] {
  const out: string[] = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...listMdx(full));
    else if (entry.name.endsWith(".mdx")) out.push(full);
  }
  return out;
}

interface Frontmatter {
  title: string;
  description: string;
}

// Minimal frontmatter reader for the simple `title`/`description` blocks the VT
// files use (description may be a `|-` folded block).
function splitFrontmatter(raw: string): { fm: Frontmatter; body: string } {
  const m = raw.match(/^---\n([\s\S]*?)\n---\n?([\s\S]*)$/);
  if (!m) return { fm: { title: "", description: "" }, body: raw };
  const fmLines = m[1].split("\n");
  const body = m[2];
  let title = "";
  let description = "";
  for (let i = 0; i < fmLines.length; i++) {
    const line = fmLines[i];
    const t = line.match(/^title:\s*(.*)$/);
    if (t) {
      title = stripQuotes(t[1].trim());
      continue;
    }
    const d = line.match(/^description:\s*(.*)$/);
    if (d) {
      let val = d[1].trim();
      if (val === "|-" || val === "|" || val === ">-" || val === ">") {
        // Folded/literal block: collect following more-indented lines.
        const parts: string[] = [];
        for (let j = i + 1; j < fmLines.length; j++) {
          if (/^\s+/.test(fmLines[j])) parts.push(fmLines[j].trim());
          else break;
        }
        val = parts.join(" ");
      }
      description = stripQuotes(val);
    }
  }
  return { fm: { title, description }, body };
}

function stripQuotes(s: string): string {
  if (
    (s.startsWith('"') && s.endsWith('"')) ||
    (s.startsWith("'") && s.endsWith("'"))
  ) {
    return s.slice(1, -1);
  }
  return s;
}

const slugger = new GithubSlugger();
function slug(s: string): string {
  slugger.reset();
  return slugger.slug(s);
}

// Adapt links/anchors mechanically (decision 3). No product-claim rewriting.
function adaptBody(body: string): string {
  let out = body;

  // [text](#TODO) and bare [text](#) placeholders → plain text.
  out = out.replace(/\[([^\]]+)\]\(#(?:TODO)?\)/g, "$1");

  // Ghostty config-reference links → TermSurf config reference (drop anchor).
  // Inline links: [text](/docs/config/reference#anything)
  out = out.replace(
    /\]\(\/docs\/config\/reference(?:#[^)]*)?\)/g,
    "](/docs/reference/config)",
  );
  // Reference-style defs: [label]: /docs/config/reference#anything
  out = out.replace(
    /^(\[[^\]]+\]:\s*)\/docs\/config\/reference(?:#\S*)?\s*$/gm,
    "$1/docs/reference/config",
  );

  // Normalize in-page fragments on /docs/vt links to match github-slugger ids.
  // Inline: ](/docs/vt/...#frag)
  out = out.replace(
    /\]\((\/docs\/vt\/[^)#]*)#([^)]+)\)/g,
    (_m, p, frag) => `](${p}#${slug(frag)})`,
  );
  // Reference-style: [label]: /docs/vt/...#frag
  out = out.replace(
    /^(\[[^\]]+\]:\s*\/docs\/vt\/\S*?)#(\S+)\s*$/gm,
    (_m, p, frag) => `${p}#${slug(frag)}`,
  );

  // Safe voice transform: rename the "Ghostty Status" heading only. Use
  // [ \t] (not \s) so the trailing newline / blank line after the heading is
  // preserved.
  out = out.replace(
    /^(#{1,6})[ \t]+Ghostty Status[ \t]*$/gm,
    "$1 Implementation Status",
  );

  return out.trimEnd() + "\n";
}

// Short sidebar label: the last parenthetical in the title (the mnemonic), else
// the title. e.g. "Cursor Position (CUP)" → "CUP".
function navLabelFor(id: string, title: string): string {
  if (id === "index") return "Overview";
  const paren = [...title.matchAll(/\(([^)]+)\)/g)].pop();
  return paren ? paren[1] : title;
}

const TOP_ORDER: Record<string, number> = { index: 1, reference: 2, external: 3 };

const ATTRIBUTION =
  "> The Terminal API documentation is adapted from " +
  "[Ghostty](https://ghostty.org)'s VT docs, used under the MIT license " +
  "(see the repo `NOTICE`). TermSurf's terminal (Ghostboard) is a Ghostty " +
  "fork and inherits its VT engine; product-specific details that still name " +
  "Ghostty are being verified against TermSurf.\n";

interface OutFile {
  rel: string;
  content: string;
}

const srcFiles = listMdx(srcDir).sort();
// Group by subsection to assign within-group order.
const bySub = new Map<string, string[]>();
for (const f of srcFiles) {
  const rel = path.relative(srcDir, f);
  const dir = rel.includes("/") ? rel.split("/")[0] : "";
  const key = SUBSECTION[dir] ?? "";
  if (!bySub.has(key)) bySub.set(key, []);
  bySub.get(key)!.push(rel);
}

const outputs: OutFile[] = [];
for (const f of srcFiles) {
  const rel = path.relative(srcDir, f);
  const id = rel.replace(/\.mdx$/, "");
  const baseName = path.basename(rel, ".mdx");
  const dir = rel.includes("/") ? rel.split("/")[0] : "";
  const subsection = SUBSECTION[dir] ?? "";

  const raw = fs.readFileSync(f, "utf8");
  const { fm, body } = splitFrontmatter(raw);

  // Order: top-level pages by explicit map; subsection pages alphabetically.
  let order: number;
  if (subsection === "") {
    order = TOP_ORDER[baseName] ?? 99;
  } else {
    order = bySub.get(subsection)!.indexOf(rel) + 1;
  }

  const fmOut: string[] = ["---"];
  fmOut.push(`title: ${yaml(fm.title)}`);
  fmOut.push(`navLabel: ${yaml(navLabelFor(id, fm.title))}`);
  // Drop placeholder descriptions (some upstream stub pages use "TODO").
  if (fm.description && fm.description !== "TODO") {
    fmOut.push(`description: ${yaml(fm.description)}`);
  }
  fmOut.push(`section: Terminal API`);
  if (subsection) fmOut.push(`subsection: ${subsection}`);
  fmOut.push(`order: ${order}`);
  fmOut.push("---");

  const adapted = adaptBody(body).replace(/^\n+/, "");
  // Ghostty's layout renders the frontmatter title as the page heading; our
  // DocPage does not, so emit it as an <h1> for parity with the other doc pages.
  let bodyOut = `# ${fm.title}\n\n`;
  // The overview page carries the section-level attribution / framing note.
  if (id === "index") bodyOut += `${ATTRIBUTION}\n`;
  bodyOut += adapted;

  outputs.push({
    rel,
    content: `${fmOut.join("\n")}\n\n${bodyOut}`,
  });
}

function yaml(s: string): string {
  // Quote if the value could be misparsed as YAML.
  return /[:#]/.test(s) ? JSON.stringify(s) : s;
}

if (checkMode) {
  let stale = false;
  for (const { rel, content } of outputs) {
    const dest = path.join(outDir, rel);
    const existing = fs.existsSync(dest) ? fs.readFileSync(dest, "utf8") : "";
    if (existing !== content) {
      console.error(`import-vt --check: stale or missing ${rel}`);
      stale = true;
    }
  }
  // Flag committed VT files with no source counterpart (removed upstream).
  const expected = new Set(outputs.map((o) => o.rel));
  for (const f of fs.existsSync(outDir) ? listMdx(outDir) : []) {
    const rel = path.relative(outDir, f);
    if (!expected.has(rel)) {
      console.error(`import-vt --check: orphaned ${rel}`);
      stale = true;
    }
  }
  if (stale) {
    console.error("Run `bun run import:vt` and commit the result.");
    process.exit(1);
  }
  console.log(`import-vt --check: ${outputs.length} VT pages up to date.`);
  process.exit(0);
}

for (const { rel, content } of outputs) {
  const dest = path.join(outDir, rel);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  fs.writeFileSync(dest, content);
}
console.log(`import-vt: wrote ${outputs.length} VT pages to src/content/docs/vt/`);
