import { defineConfig } from "astro/config";
import tailwindcss from "@tailwindcss/vite";
import react from "@astrojs/react";
import mdx from "@astrojs/mdx";

export default defineConfig({
  output: "static",
  trailingSlash: "never",
  // mdx() after react() so MDX inherits the JSX renderer config.
  integrations: [react(), mdx()],
  // Docs code blocks are hand-highlighted with <span> + Tokyo Night CSS classes
  // (see src/styles/style.css). Disable the default Shiki highlighter so fenced
  // code blocks fall back to plain .prose-termsurf pre styling instead of
  // emitting inline per-token styles that would override the theme.
  markdown: {
    syntaxHighlight: false,
  },
  vite: {
    plugins: [tailwindcss()],
  },
});
