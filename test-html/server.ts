import { join } from "path";

const publicDir = join(import.meta.dir, "public");

Bun.serve({
  port: 9616,
  async fetch(req) {
    const url = new URL(req.url);
    const path = url.pathname === "/" ? "/index.html" : url.pathname;
    const file = Bun.file(join(publicDir, path));

    if (await file.exists()) {
      return new Response(file);
    }

    return new Response("Not Found", { status: 404 });
  },
});

console.log("http://localhost:9616");
