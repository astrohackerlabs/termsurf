#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

function parseArgs(argv) {
  const args = {
    timeoutSeconds: 30,
    settleSeconds: 8,
    alwaysSettle: false,
  };

  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith("--")) {
      throw new Error(`unexpected positional argument: ${arg}`);
    }
    const [rawKey, inlineValue] = arg.slice(2).split("=", 2);
    const key = rawKey.replace(/-([a-z])/g, (_, ch) => ch.toUpperCase());
    if (key === "alwaysSettle") {
      args.alwaysSettle = true;
      continue;
    }
    const value = inlineValue ?? argv[++i];
    if (value === undefined) {
      throw new Error(`missing value for ${arg}`);
    }
    args[key] = value;
  }

  for (const key of ["devtoolsPort", "urlContains", "out"]) {
    if (!args[key]) {
      throw new Error(
        `missing required --${key.replace(/[A-Z]/g, (ch) => `-${ch.toLowerCase()}`)}`,
      );
    }
  }

  args.devtoolsPort = Number(args.devtoolsPort);
  args.timeoutSeconds = Number(args.timeoutSeconds);
  args.settleSeconds = Number(args.settleSeconds);

  if (!Number.isFinite(args.devtoolsPort) || args.devtoolsPort <= 0) {
    throw new Error(`invalid --devtools-port: ${args.devtoolsPort}`);
  }
  if (!Number.isFinite(args.timeoutSeconds) || args.timeoutSeconds <= 0) {
    throw new Error(`invalid --timeout-seconds: ${args.timeoutSeconds}`);
  }
  if (!Number.isFinite(args.settleSeconds) || args.settleSeconds < 0) {
    throw new Error(`invalid --settle-seconds: ${args.settleSeconds}`);
  }

  return args;
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function fetchJson(url) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`GET ${url} failed: HTTP ${response.status}`);
  }
  return await response.json();
}

async function pollTarget(args, sidecar) {
  const deadline = Date.now() + args.timeoutSeconds * 1000;
  const listUrl = `http://127.0.0.1:${args.devtoolsPort}/json/list`;
  let lastTargets = [];
  let lastError = null;

  while (Date.now() < deadline) {
    try {
      lastTargets = await fetchJson(listUrl);
      const matches = lastTargets.filter(
        (target) =>
          target.type === "page" &&
          typeof target.url === "string" &&
          target.url.includes(args.urlContains) &&
          target.webSocketDebuggerUrl,
      );
      if (matches.length > 0) {
        return matches[0];
      }
    } catch (error) {
      lastError = error;
    }
    await sleep(250);
  }

  sidecar.availableTargets = lastTargets.map((target) => ({
    id: target.id,
    type: target.type,
    url: target.url,
    title: target.title,
  }));
  if (lastError) {
    sidecar.lastError = String(lastError.stack || lastError);
  }
  throw new Error(
    `no page target contained ${JSON.stringify(args.urlContains)}`,
  );
}

function connectDevTools(wsUrl) {
  const socket = new WebSocket(wsUrl);
  let nextId = 1;
  const pending = new Map();
  const events = [];

  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const { resolve, reject } = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) {
        reject(
          new Error(
            `${message.error.message || "DevTools error"} (${message.error.code})`,
          ),
        );
      } else {
        resolve(message.result || {});
      }
      return;
    }
    if (message.method) {
      events.push(message);
    }
  });

  const open = new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });

  function send(method, params = {}) {
    const id = nextId;
    nextId += 1;
    const promise = new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
    });
    socket.send(JSON.stringify({ id, method, params }));
    return promise;
  }

  function waitForEvent(method, timeoutMs) {
    const existing = events.find((event) => event.method === method);
    if (existing) {
      return Promise.resolve(existing);
    }
    return new Promise((resolve) => {
      let settled = false;
      const finish = (message) => {
        if (settled) {
          return;
        }
        settled = true;
        clearTimeout(timer);
        socket.removeEventListener("message", onMessage);
        resolve(message);
      };
      const timer = setTimeout(() => {
        finish(null);
      }, timeoutMs);
      const onMessage = (event) => {
        const message = JSON.parse(event.data);
        if (message.method === method) {
          finish(message);
        }
      };
      socket.addEventListener("message", onMessage);
      const afterListener = events.find((event) => event.method === method);
      if (afterListener) {
        finish(afterListener);
      }
    });
  }

  return { socket, open, send, waitForEvent };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const outPath = path.resolve(args.out);
  const sidecarPath = `${outPath}.json`;
  const sidecar = {
    devtoolsPort: args.devtoolsPort,
    urlContains: args.urlContains,
    out: outPath,
    timeoutSeconds: args.timeoutSeconds,
    settleSeconds: args.settleSeconds,
    alwaysSettle: args.alwaysSettle,
  };
  let client = null;

  try {
    const target = await pollTarget(args, sidecar);
    sidecar.selectedTarget = {
      id: target.id,
      type: target.type,
      url: target.url,
      title: target.title,
    };

    client = connectDevTools(target.webSocketDebuggerUrl);
    await client.open;
    await client.send("Page.enable");
    try {
      await client.send("Page.bringToFront");
    } catch (error) {
      sidecar.bringToFrontError = String(error.message || error);
    }

    const settleMs = args.settleSeconds * 1000;
    if (args.alwaysSettle) {
      await sleep(settleMs);
      sidecar.waitMode = "fixed-settle";
    } else {
      const event = await client.waitForEvent("Page.loadEventFired", settleMs);
      sidecar.waitMode = event ? "load-event" : "settle-timeout";
    }

    const result = await client.send("Page.captureScreenshot", {
      format: "png",
      fromSurface: true,
    });
    if (!result.data) {
      throw new Error("Page.captureScreenshot returned no data");
    }

    const png = Buffer.from(result.data, "base64");
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, png);
    sidecar.screenshotBytes = png.length;
    fs.writeFileSync(sidecarPath, `${JSON.stringify(sidecar, null, 2)}\n`);
    client.socket.close();
  } catch (error) {
    if (client) {
      client.socket.close();
    }
    sidecar.error = String(error.stack || error);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(sidecarPath, `${JSON.stringify(sidecar, null, 2)}\n`);
    throw error;
  }
}

main().catch((error) => {
  console.error(error.stack || error);
  process.exit(1);
});
