import { createServerFn } from "@tanstack/react-start";
import * as fs from "node:fs";
import * as path from "node:path";
import toml from "toml";

const BLOG_DIR = path.resolve(process.cwd(), "blog-posts");

export const getBlogPost = createServerFn({ method: "GET" })
  .inputValidator((slug: string) => slug)
  .handler(async ({ data: slug }) => {
    const raw = await fs.promises.readFile(
      path.join(BLOG_DIR, `${slug}.md`),
      "utf-8",
    );
    const parts = raw.split("+++");
    const meta = toml.parse(parts[1].trim());
    const content = parts.slice(2).join("+++").trim();
    return {
      slug,
      title: meta.title as string,
      author: meta.author as string,
      date: meta.date as string,
      content,
    };
  });
