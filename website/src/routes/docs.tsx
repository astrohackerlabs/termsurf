import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/docs")({
  head: () => ({ meta: [{ title: "Docs — TermSurf" }] }),
  component: DocsPage,
});

function DocsPage() {
  return (
    <section>
      <h2 className="text-sm font-bold text-foreground mb-4">
        ┌─ Docs ─┐
      </h2>
      <p className="text-muted text-sm">Coming soon.</p>
    </section>
  );
}
