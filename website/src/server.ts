import {
  createStartHandler,
  defaultStreamHandler,
} from "@tanstack/react-start/server";
import { join } from "node:path";
import { existsSync } from "node:fs";

const handler = createStartHandler(defaultStreamHandler);

const CLIENT_DIR = join(import.meta.dirname, "..", "client");

export default {
  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);

    // Serve static assets from dist/client
    if (url.pathname.startsWith("/assets/") || url.pathname.startsWith("/_build/")) {
      const filePath = join(CLIENT_DIR, url.pathname);
      const file = Bun.file(filePath);
      if (await file.exists()) {
        return new Response(file);
      }
    }

    // Serve public files (images, feeds, etc.)
    const publicPath = join(CLIENT_DIR, url.pathname);
    if (url.pathname !== "/" && existsSync(publicPath)) {
      const file = Bun.file(publicPath);
      const stat = await file.exists();
      if (stat) {
        return new Response(file);
      }
    }

    // SSR for everything else
    return handler(request);
  },
};
