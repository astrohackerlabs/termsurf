import { join } from "path";

const publicDir = join(import.meta.dir, "public");

Bun.serve({
  port: 9616,
  async fetch(req) {
    const url = new URL(req.url);

    // Slow-load route: streams chunked HTML over N seconds.
    if (url.pathname === "/slow") {
      const seconds = Math.min(
        Math.max(parseInt(url.searchParams.get("seconds") || "10"), 1),
        120,
      );

      const stream = new ReadableStream({
        async start(controller) {
          const encoder = new TextEncoder();

          // Send the initial HTML shell.
          controller.enqueue(
            encoder.encode(`<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Slow Load Test (${seconds}s)</title>
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body {
  background: #1a1b26;
  color: #c0caf5;
  font-family: system-ui, -apple-system, sans-serif;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100vh;
  gap: 24px;
}
h1 { color: #7aa2f7; font-size: 28px; }
.bar-container {
  width: 400px;
  height: 32px;
  background: #24283b;
  border-radius: 16px;
  overflow: hidden;
  border: 1px solid #565f89;
}
.bar-fill {
  height: 100%;
  width: 0%;
  background: linear-gradient(90deg, #7aa2f7, #7dcfff);
  border-radius: 16px;
  transition: width 0.3s ease;
}
.pct { font-size: 48px; color: #7dcfff; font-weight: bold; }
.status { color: #565f89; font-size: 16px; }
.done { color: #9ece6a; font-size: 24px; display: none; }
</style>
</head>
<body>
<h1>Slow Load Test</h1>
<div class="bar-container"><div class="bar-fill" id="bar"></div></div>
<div class="pct" id="pct">0%</div>
<div class="status" id="status">Loading... 0 / ${seconds}s</div>
<div class="done" id="done"></div>
<script>
function update(p, elapsed, total) {
  document.getElementById('bar').style.width = p + '%';
  document.getElementById('pct').textContent = p + '%';
  document.getElementById('status').textContent = 'Loading... ' + elapsed + ' / ' + total + 's';
}
function finish(total) {
  document.getElementById('bar').style.width = '100%';
  document.getElementById('pct').textContent = '100%';
  document.getElementById('status').style.display = 'none';
  document.getElementById('done').style.display = 'block';
  document.getElementById('done').textContent = 'Done! Loaded in ' + total + 's';
}
</script>
`),
          );

          // Send a progress chunk every second.
          for (let i = 1; i <= seconds; i++) {
            await Bun.sleep(1000);
            const pct = Math.round((i / seconds) * 100);
            controller.enqueue(
              encoder.encode(
                `<script>update(${pct}, ${i}, ${seconds});</script>\n`,
              ),
            );
          }

          // Send the final "done" chunk and close.
          controller.enqueue(
            encoder.encode(
              `<script>finish(${seconds});</script>\n</body></html>`,
            ),
          );
          controller.close();
        },
      });

      return new Response(stream, {
        headers: {
          "Content-Type": "text/html; charset=utf-8",
          "Transfer-Encoding": "chunked",
        },
      });
    }

    // Static file serving.
    const path = url.pathname === "/" ? "/index.html" : url.pathname;
    const file = Bun.file(join(publicDir, path));

    if (await file.exists()) {
      return new Response(file);
    }

    return new Response("Not Found", { status: 404 });
  },
});

console.log("http://localhost:9616");
