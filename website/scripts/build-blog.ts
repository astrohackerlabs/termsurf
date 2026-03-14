/**
 * Reads markdown blog posts from docs/blog/, parses TOML front matter,
 * and generates data/blog.json + feed files in public/blog/.
 * Run with: bun run build:blog
 */

import { readFileSync, writeFileSync, readdirSync, mkdirSync } from "fs";
import { join } from "path";
import toml from "toml";
import { Feed } from "feed";
import type { BlogPost, BlogData } from "../src/blog";

const DOCS_DIR = join(import.meta.dir, "../../docs/blog");
const DATA_DIR = join(import.meta.dir, "../data");
const PUBLIC_DIR = join(import.meta.dir, "../public/blog");
const SITE_URL = "https://termsurf.com";

function parseFrontMatter(raw: string): { meta: Record<string, string>; content: string } {
  const parts = raw.split("+++");
  if (parts.length < 3) {
    throw new Error("Missing +++ front matter delimiters");
  }
  const meta = toml.parse(parts[1].trim());
  const content = parts.slice(2).join("+++").trim();
  return { meta, content };
}

function buildBlog() {
  const files = readdirSync(DOCS_DIR)
    .filter((f) => f.endsWith(".md"))
    .sort()
    .reverse();

  const posts: BlogPost[] = [];

  for (const file of files) {
    const raw = readFileSync(join(DOCS_DIR, file), "utf-8");
    const { meta, content } = parseFrontMatter(raw);
    const slug = file.replace(/\.md$/, "");

    posts.push({
      slug,
      title: meta.title,
      author: meta.author,
      date: meta.date,
      content,
    });
  }

  // Write blog.json
  mkdirSync(DATA_DIR, { recursive: true });
  const blogData: BlogData = { posts };
  writeFileSync(
    join(DATA_DIR, "blog.json"),
    JSON.stringify(blogData, null, 2) + "\n",
  );
  console.log(`  blog.json: ${posts.length} posts`);

  // Generate feeds
  mkdirSync(PUBLIC_DIR, { recursive: true });

  const feed = new Feed({
    title: "TermSurf Blog",
    description: "TermSurf — Terminal + Browser",
    id: `${SITE_URL}/blog`,
    link: `${SITE_URL}/blog`,
    language: "en",
    favicon: `${SITE_URL}/favicon.ico`,
    copyright: `Copyright (C) ${new Date().getFullYear()} TermSurf`,
    updated: posts.length > 0 ? new Date(posts[0].date) : new Date(),
    feedLinks: {
      json: `${SITE_URL}/blog/feed.json`,
      atom: `${SITE_URL}/blog/feed.atom.xml`,
      rss: `${SITE_URL}/blog/feed.rss.xml`,
    },
    author: {
      name: "Ryan X. Charles",
      link: SITE_URL,
    },
  });

  const recentPosts = posts.slice(0, 20);
  for (const post of recentPosts) {
    feed.addItem({
      title: post.title,
      id: `${SITE_URL}/blog/${post.slug}`,
      link: `${SITE_URL}/blog/${post.slug}`,
      date: new Date(post.date),
      description: post.title,
      content: post.content,
    });
  }

  writeFileSync(join(PUBLIC_DIR, "feed.json"), feed.json1());
  writeFileSync(join(PUBLIC_DIR, "feed.atom.xml"), feed.atom1());
  writeFileSync(join(PUBLIC_DIR, "feed.rss.xml"), feed.rss2());
  console.log("  feeds: feed.json, feed.atom.xml, feed.rss.xml");
}

buildBlog();
