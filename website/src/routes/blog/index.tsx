import { createFileRoute, Link } from "@tanstack/react-router";
import blogData from "../../../data/blog.json";

export const Route = createFileRoute("/blog/")({
  head: () => ({ meta: [{ title: "Blog — TermSurf" }] }),
  component: BlogIndex,
});

function BlogIndex() {
  return (
    <section>
      <h2 className="text-sm font-bold text-foreground mb-4">
        ┌─ Blog ─┐
      </h2>
      <ul>
        {blogData.posts.map((post) => (
          <li key={post.slug} className="py-1">
            <Link
              to="/blog/$slug"
              params={{ slug: post.slug }}
              className="flex gap-3 items-baseline text-sm hover:bg-background-highlight/30"
            >
              <span className="text-muted">{post.date}</span>
              <span className="text-accent hover:text-primary flex-1">
                {post.title}
              </span>
              <span className="text-success">{post.author}</span>
            </Link>
          </li>
        ))}
      </ul>
    </section>
  );
}
