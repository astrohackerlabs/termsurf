import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/about")({
  head: () => ({ meta: [{ title: "About — TermSurf" }] }),
  component: AboutPage,
});

function AboutPage() {
  return (
    <section>
      <h2 className="text-sm font-bold text-foreground mb-4">
        ┌─ About ─┐
      </h2>
      <div className="text-sm space-y-3">
        <p className="text-foreground-dark">
          TermSurf is a protocol for embedding web browsers inside terminal
          emulators. Any terminal, any browser engine, any TUI — connected by a
          protobuf/Unix socket protocol.
        </p>
        <p>
          <a
            href="https://github.com/termsurf/termsurf"
            target="_blank"
            rel="noopener noreferrer"
            className="text-accent hover:text-primary"
          >
            [GitHub]
          </a>
        </p>
      </div>
    </section>
  );
}
