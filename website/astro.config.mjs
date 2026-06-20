import { defineConfig } from "astro/config";
import tailwindcss from "@tailwindcss/vite";
import react from "@astrojs/react";
import mdx from "@astrojs/mdx";
import { remarkAlert } from "remark-github-blockquote-alert";

export default defineConfig({
  output: "static",
  trailingSlash: "never",
  // mdx() after react() so MDX inherits the JSX renderer config. mdx() MUST stay
  // option-free: @astrojs/mdx defaults extendMarkdownConfig:true, which is how
  // MDX pages inherit markdown.remarkPlugins below (e.g. remarkAlert). If mdx()
  // is ever given explicit remarkPlugins, re-list remarkAlert there too.
  integrations: [react(), mdx()],
  // Docs code blocks are hand-highlighted with <span> + Tokyo Night CSS classes
  // (see src/styles/style.css). Disable the default Shiki highlighter so fenced
  // code blocks fall back to plain .prose-termsurf pre styling instead of
  // emitting inline per-token styles that would override the theme.
  markdown: {
    syntaxHighlight: false,
    // GitHub-style callouts (> [!NOTE] / [!WARNING] / ...) → .markdown-alert
    // divs, styled in src/styles/style.css. Inherited by MDX (see above).
    remarkPlugins: [remarkAlert],
  },
  vite: {
    plugins: [tailwindcss()],
  },
});
