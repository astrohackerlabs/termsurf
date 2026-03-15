import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/manifesto")({
  head: () => ({ meta: [{ title: "Manifesto — TermSurf" }] }),
  component: ManifestoPage,
});

function ManifestoPage() {
  return (
    <section>
      <h2 className="text-sm font-bold text-foreground mb-4">
        ┌─ Manifesto ─┐
      </h2>
      <div className="text-sm space-y-4 text-foreground-dark">
        <p>We need total control.</p>
        <p>
          The terminal gave us that. Every process, every socket, every file —
          reachable from a single prompt. The terminal does not hide things. It
          does not decide what we are allowed to see. We are the admins. We are
          the operators. We are in charge.
        </p>
        <p>The browser took it away.</p>
        <p>
          Big tech brought the web to three billion people. That is a real
          achievement. But Chrome, Safari, Edge — they are built for civilians.
          They hide the network. They hide the DOM. They wrap everything in a
          GUI designed for people who do not know what a process is. You cannot
          pipe a browser. You cannot script it. You cannot embed it in your
          workflow. The browser is a walled garden on an open operating system.
          It is the last system on your machine that you do not control.
        </p>
        <p>
          We are not civilians. We are root. We need to inspect every request.
          Override every header. Pipe responses into scripts. Open DevTools in a
          split pane while tailing logs in another. We need the web the same way
          we need the filesystem — raw, fast, and under <em>our</em> complete
          control.
        </p>
        <p>
          The browser we need does not exist as a standalone app. The window is
          the wrong container. The right container is the terminal — where
          everything else already lives. The browser should be TUI. It should
          sit in the shell, next to the editor, next to the logs. It should
          resize with a keystroke. It should speak protobuf over a Unix socket.
          It should be a component in the system, not walled garden.
        </p>
        <p>
          So we built TermSurf. Full Chromium. Full GPU rendering. Zero-copy
          compositing. Type{" "}
          <code className="text-accent">web localhost:3000</code> and the page
          is there — right next to your shell. <em>In</em> your shell. No
          alt-tab. No context switch. No walled garden.
        </p>
        <p>We surf by day. We hack by night. We build TermSurf for us.</p>
        <p>
          TermSurfers are <em>root</em>.
        </p>
        <p className="pt-2">— Max Commits, San Tokyo, Hawaii</p>
        <p className="pt-2">
          <a
            href="https://github.com/termsurf/termsurf"
            target="_blank"
            rel="noopener noreferrer"
            className="text-accent hover:text-primary"
          >
            [fork the source]
          </a>
        </p>
      </div>
    </section>
  );
}
