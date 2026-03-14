import { createFileRoute, Link } from "@tanstack/react-router";
import { CommitLog } from "../components/CommitLog";
import commitsData from "../../data/commits.json";
import blogData from "../../data/blog.json";

export const Route = createFileRoute("/")({
  component: HomePage,
  head: () => ({
    links: [
      {
        rel: "alternate",
        type: "application/json",
        title: "JSON Feed",
        href: "/blog/feed.json",
      },
      {
        rel: "alternate",
        type: "application/atom+xml",
        title: "Atom Feed",
        href: "/blog/feed.atom.xml",
      },
      {
        rel: "alternate",
        type: "application/rss+xml",
        title: "RSS Feed",
        href: "/blog/feed.rss.xml",
      },
    ],
  }),
});

function HomePage() {
  const latestPost = blogData.posts[0];

  return (
    <>
      <section className="mb-8">
        <h1 className="text-lg font-bold text-primary">
          Root access to the 'net.
        </h1>
        <p className="text-sm text-muted mt-3">
          Hack the web from your terminal.{" "}
          <span className="bg-accent text-background">
            TermSurf is a protocol that overlays web browser engines, like
            Chromium and WebKit, in your terminal emulator.
          </span>{" "}
          Unlimited power to control every web page from your keyboard.{" "}
          <a
            href="https://github.com/termsurf/termsurf"
            className="text-accent hover:text-primary"
          >
            [fork the source on GitHub]
          </a>
        </p>
        <div className="mt-4 text-muted text-xs">
          ──────────────────────────────────────────────────────────
        </div>
      </section>
      <section className="mb-8">
        <h2 className="text-sm font-bold text-foreground mb-4">
          ┌─ Latest Post ─┐
        </h2>
        {latestPost ? (
          <Link
            to="/blog/$slug"
            params={{ slug: latestPost.slug }}
            className="block hover:bg-background-highlight/30 text-sm"
          >
            <span className="text-accent hover:text-primary">
              {latestPost.title}
            </span>
            <span className="text-muted ml-3">{latestPost.date}</span>
            <span className="text-success ml-3">{latestPost.author}</span>
          </Link>
        ) : (
          <p className="text-muted text-sm">No posts yet.</p>
        )}
        <Link to="/blog" className="text-sm text-muted hover:text-accent mt-3 inline-block">
          [view all posts]
        </Link>
      </section>
      <CommitLog commits={commitsData.commits.slice(0, 10)} />
      <Link to="/commits" className="text-sm text-muted hover:text-accent mt-3 inline-block">
        [view all commits]
      </Link>
    </>
  );
}
