#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

function parseArgs(argv) {
  const args = { timeoutSeconds: 30, settleSeconds: 4, actionSettleMs: 1200 };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith("--")) throw new Error(`unexpected argument: ${arg}`);
    const [rawKey, inlineValue] = arg.slice(2).split("=", 2);
    const key = rawKey.replace(/-([a-z])/g, (_, ch) => ch.toUpperCase());
    const value = inlineValue ?? argv[++i];
    if (value === undefined) throw new Error(`missing value for ${arg}`);
    args[key] = value;
  }
  for (const key of ["devtoolsPort", "urlContains", "outDir", "downloadsDir"]) {
    if (!args[key]) throw new Error(`missing ${key}`);
  }
  args.devtoolsPort = Number(args.devtoolsPort);
  args.timeoutSeconds = Number(args.timeoutSeconds);
  args.settleSeconds = Number(args.settleSeconds);
  args.actionSettleMs = Number(args.actionSettleMs);
  args.outDir = path.resolve(args.outDir);
  args.downloadsDir = path.resolve(args.downloadsDir);
  return args;
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function writeJson(filePath, value) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
}

async function fetchJson(url) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`GET ${url} failed: HTTP ${response.status}`);
  return await response.json();
}

async function pollTarget(args, summary) {
  const deadline = Date.now() + args.timeoutSeconds * 1000;
  const listUrl = `http://127.0.0.1:${args.devtoolsPort}/json/list`;
  let lastTargets = [];
  while (Date.now() < deadline) {
    lastTargets = await fetchJson(listUrl);
    const target = lastTargets.find(
      (item) =>
        item.type === "page" &&
        typeof item.url === "string" &&
        item.url.includes(args.urlContains) &&
        item.webSocketDebuggerUrl,
    );
    if (target) return target;
    await sleep(250);
  }
  summary.availableTargets = lastTargets.map((item) => ({
    id: item.id,
    type: item.type,
    url: item.url,
    title: item.title,
  }));
  throw new Error(`no page target contained ${JSON.stringify(args.urlContains)}`);
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
        reject(new Error(`${message.error.message || "DevTools error"} (${message.error.code})`));
      } else {
        resolve(message.result || {});
      }
      return;
    }
    if (message.method) events.push(message);
  });

  const open = new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });

  function send(method, params = {}, sessionId = undefined) {
    const id = nextId;
    nextId += 1;
    const promise = new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
    });
    const message = { id, method, params };
    if (sessionId) message.sessionId = sessionId;
    socket.send(JSON.stringify(message));
    return promise;
  }

  return { socket, open, send, events };
}

async function safeSend(client, method, params = {}, sessionId = undefined) {
  try {
    return { ok: true, result: await client.send(method, params, sessionId) };
  } catch (error) {
    return { ok: false, error: String(error.message || error) };
  }
}

async function evaluate(client, expression, sessionId = undefined) {
  const result = await safeSend(
    client,
    "Runtime.evaluate",
    { expression, awaitPromise: true, returnByValue: true },
    sessionId,
  );
  if (!result.ok) return { ok: false, error: result.error };
  if (result.result.exceptionDetails) {
    return { ok: false, error: JSON.stringify(result.result.exceptionDetails) };
  }
  return { ok: true, value: result.result.result?.value ?? null };
}

function listFilesRecursive(root) {
  if (!fs.existsSync(root)) return [];
  const files = [];
  const visit = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const fullPath = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        visit(fullPath);
      } else {
        files.push({
          path: fullPath,
          relativePath: path.relative(root, fullPath),
          bytes: fs.statSync(fullPath).size,
        });
      }
    }
  };
  visit(root);
  return files;
}

const STATE_SOURCE = `(() => {
  const textOf = (el) => [
    el.getAttribute("aria-label") || "",
    el.getAttribute("title") || "",
    el.id || "",
    String(el.className || ""),
    el.getAttribute("role") || "",
    el.innerText || "",
    el.value || "",
  ].join(" ").replace(/\\s+/g, " ").trim();
  const rectOf = (el) => {
    const rect = el.getBoundingClientRect();
    return {
      tag: el.tagName,
      id: el.id || "",
      className: String(el.className || ""),
      role: el.getAttribute("role") || "",
      ariaLabel: el.getAttribute("aria-label") || "",
      title: el.getAttribute("title") || "",
      text: (el.innerText || el.value || "").slice(0, 120),
      disabled: !!el.disabled || el.getAttribute("aria-disabled") === "true",
      hidden: !!el.hidden || getComputedStyle(el).visibility === "hidden" || getComputedStyle(el).display === "none",
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
      token: textOf(el),
    };
  };
  const controls = [];
  const elements = [];
  const props = (obj) => {
    const out = {};
    if (!obj) return out;
    let current = obj;
    let depth = 0;
    const seen = new Set();
    while (current && depth < 4) {
      for (const name of Object.getOwnPropertyNames(current).sort()) {
        if (seen.has(name) || name === "constructor") continue;
        seen.add(name);
        try {
          const descriptor = Object.getOwnPropertyDescriptor(current, name);
          if (!descriptor) continue;
          const value = obj[name];
          if (value === null || ["string", "number", "boolean"].includes(typeof value)) {
            out[name] = {depth, value, accessor: !!descriptor.get};
          } else if (Array.isArray(value)) {
            out[name] = {depth, value: {type: "array", length: value.length}, accessor: !!descriptor.get};
          }
        } catch (error) {
          out[name] = {depth, error: String(error)};
        }
      }
      current = Object.getPrototypeOf(current);
      depth += 1;
    }
    return out;
  };
  const visit = (root, depth = 0, scope = "document") => {
    if (!root || depth > 10 || controls.length > 320 || elements.length > 320) return;
    for (const el of root.querySelectorAll("button,input,select,cr-icon-button,*[role=button],viewer-toolbar,viewer-download-controls,viewer-page-selector")) {
      controls.push({...rectOf(el), scope, depth});
    }
    for (const el of root.querySelectorAll("embed,iframe,canvas,pdf-viewer,#viewer,#plugin,viewer-toolbar,viewer-download-controls")) {
      elements.push({...rectOf(el), scope, depth});
    }
    for (const el of root.querySelectorAll("*")) {
      if (el.shadowRoot) visit(el.shadowRoot, depth + 1, \`\${scope} > \${el.tagName.toLowerCase()}#\${el.id || ""}\`);
    }
  };
  visit(document);
  const viewer = document.querySelector("pdf-viewer#viewer") || document.querySelector("pdf-viewer");
  const plugin = viewer?.shadowRoot?.querySelector("#plugin") || document.querySelector("embed") || null;
  const loadTimeDataObject = globalThis.loadTimeData || null;
  const loadTimeData = loadTimeDataObject?.data || loadTimeDataObject?.data_ || null;
  return {
    url: location.href,
    title: document.title,
    bodyTextSample: document.body?.innerText?.slice(0, 1000) || "",
    viewport: {innerWidth, innerHeight, devicePixelRatio},
    controls,
    elements,
    pluginRect: plugin ? rectOf(plugin) : null,
    viewerProps: props(viewer),
    controllerProps: props(viewer?.currentController),
    loadTimeFlags: loadTimeData ? {
      printingEnabled: loadTimeData.printingEnabled ?? null,
      pdfUseShowSaveFilePicker: loadTimeData.pdfUseShowSaveFilePicker ?? null,
      pdfGetSaveDataInBlocks: loadTimeData.pdfGetSaveDataInBlocks ?? null,
      pdfSaveToDrive: loadTimeData.pdfSaveToDrive ?? null,
    } : null,
  };
})()`;

function controlMatches(control, aliases) {
  const token = String(control.token || "").toLowerCase();
  return aliases.some((alias) => token.includes(alias));
}

function findDownloadControls(states) {
  return states.flatMap((state) =>
    (state.value?.controls || [])
      .filter((control) => controlMatches(control, ["download", "save"]))
      .map((control) => ({
        sessionId: state.sessionId,
        targetUrl: state.targetInfo?.url || state.value?.url || "",
        ...control,
      })),
  );
}

const CLICK_DOWNLOAD_SOURCE = `(() => {
  const haystack = (el) => [
    el.getAttribute("aria-label") || "",
    el.getAttribute("title") || "",
    el.id || "",
    String(el.className || ""),
    el.getAttribute("role") || "",
    el.innerText || "",
    el.value || "",
  ].join(" ").replace(/\\s+/g, " ").trim().toLowerCase();
  const rectOf = (el) => {
    const rect = el.getBoundingClientRect();
    return {
      tag: el.tagName,
      id: el.id || "",
      ariaLabel: el.getAttribute("aria-label") || "",
      title: el.getAttribute("title") || "",
      text: (el.innerText || el.value || "").slice(0, 120),
      disabled: !!el.disabled || el.getAttribute("aria-disabled") === "true",
      hidden: !!el.hidden || getComputedStyle(el).visibility === "hidden" || getComputedStyle(el).display === "none",
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
      token: haystack(el),
    };
  };
  const candidates = [];
  const visit = (root, depth = 0) => {
    if (!root || depth > 10 || candidates.length > 260) return;
    for (const el of root.querySelectorAll("button,input,cr-icon-button,*[role=button]")) {
      const info = rectOf(el);
      if (info.token.includes("download") || info.token.includes("save")) {
        candidates.push({info, element: el});
      }
    }
    for (const el of root.querySelectorAll("*")) {
      if (el.shadowRoot) visit(el.shadowRoot, depth + 1);
    }
  };
  visit(document);
  const selected = candidates.find((item) => !item.info.disabled && !item.info.hidden) || candidates[0];
  if (!selected) return {found: false, candidates: candidates.map((item) => item.info)};
  if (selected.info.disabled || selected.info.hidden) {
    return {found: true, activated: false, reason: selected.info.disabled ? "disabled" : "hidden", selected: selected.info, candidates: candidates.map((item) => item.info)};
  }
  selected.element.click();
  return {found: true, activated: true, method: "dom-click", selected: selected.info, candidates: candidates.map((item) => item.info)};
})()`;

async function collectStates(client) {
  const eventStart = client.events.length;
  await safeSend(client, "Target.setAutoAttach", {
    autoAttach: true,
    waitForDebuggerOnStart: false,
    flatten: true,
  });
  await sleep(1000);
  const children = client.events
    .slice(eventStart)
    .filter((event) => event.method === "Target.attachedToTarget")
    .map((event) => ({
      sessionId: event.params.sessionId,
      targetInfo: event.params.targetInfo,
    }));
  for (const child of children) {
    await safeSend(client, "Runtime.enable", {}, child.sessionId);
    await safeSend(client, "Page.enable", {}, child.sessionId);
    await safeSend(client, "DOM.enable", {}, child.sessionId);
  }
  const states = [{ sessionId: null, targetInfo: null, state: await evaluate(client, STATE_SOURCE) }];
  for (const child of children) {
    states.push({
      sessionId: child.sessionId,
      targetInfo: child.targetInfo,
      state: await evaluate(client, STATE_SOURCE, child.sessionId),
    });
  }
  return {
    children,
    states,
    values: states
      .filter((state) => state.state.ok && state.state.value)
      .map((state) => ({
        sessionId: state.sessionId,
        targetInfo: state.targetInfo,
        value: state.state.value,
      })),
  };
}

async function probeDownload(client, args, values) {
  fs.mkdirSync(args.downloadsDir, { recursive: true });
  const beforeFiles = listFilesRecursive(args.downloadsDir);
  const containment = await safeSend(client, "Browser.setDownloadBehavior", {
    behavior: "allow",
    downloadPath: args.downloadsDir,
    eventsEnabled: true,
  });
  const sessions = [
    ...values.filter((item) => String(item.value.url || "").startsWith("chrome-extension://")),
    ...values,
  ];
  const attempts = [];
  for (const item of sessions) {
    const result = await evaluate(client, CLICK_DOWNLOAD_SOURCE, item.sessionId || undefined);
    attempts.push({
      sessionId: item.sessionId,
      targetUrl: item.targetInfo?.url || item.value.url,
      result,
    });
    if (result.ok && result.value?.activated) break;
    if (result.ok && result.value?.found && result.value?.selected) break;
  }
  await sleep(args.actionSettleMs * 2);
  const afterFiles = listFilesRecursive(args.downloadsDir);
  const newFiles = afterFiles.filter(
    (after) =>
      !beforeFiles.some(
        (before) => before.relativePath === after.relativePath && before.bytes === after.bytes,
      ),
  );
  const downloadEvents = client.events.filter((event) =>
    String(event.method || "").startsWith("Browser.download"),
  );
  return {
    status: newFiles.length > 0
      ? "download-file-created"
      : downloadEvents.length > 0
        ? "download-browser-callback-only"
        : attempts.some((attempt) => attempt.result.ok && attempt.result.value?.found && !attempt.result.value?.activated)
          ? "download-control-not-activated"
          : "download-no-op",
    containment,
    beforeFiles,
    afterFiles,
    newFiles,
    downloadEvents,
    attempts,
  };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  fs.mkdirSync(args.outDir, { recursive: true });
  const summary = { devtoolsPort: args.devtoolsPort, urlContains: args.urlContains };
  let client = null;
  try {
    const target = await pollTarget(args, summary);
    summary.selectedTarget = { id: target.id, type: target.type, url: target.url, title: target.title };
    client = connectDevTools(target.webSocketDebuggerUrl);
    await client.open;
    for (const domain of ["Page", "Runtime", "DOM", "Target", "Browser"]) {
      summary[`${domain}Enabled`] = await safeSend(client, `${domain}.enable`);
    }
    await safeSend(client, "Page.bringToFront");
    await sleep(args.settleSeconds * 1000);
    const collected = await collectStates(client);
    summary.children = collected.children;
    summary.states = collected.states;
    summary.downloadControls = findDownloadControls(collected.values);
    summary.download = await probeDownload(client, args, collected.values);
    summary.status = "pass";
    summary.firstFailingHop = "no-failure-observed";
  } catch (error) {
    summary.status = "error";
    summary.firstFailingHop = "devtools-probe-error";
    summary.error = String(error.stack || error);
    throw error;
  } finally {
    writeJson(path.join(args.outDir, "pdf-restrictions-devtools-summary.json"), summary);
    client?.socket?.close();
  }
}

main().catch((error) => {
  console.error(error.stack || error);
  process.exit(1);
});
