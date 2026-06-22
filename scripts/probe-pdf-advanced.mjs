#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";

function parseArgs(argv) {
  const args = { timeoutSeconds: 30, settleSeconds: 3, probe: "forms" };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith("--")) throw new Error(`unexpected argument: ${arg}`);
    const [rawKey, inlineValue] = arg.slice(2).split("=", 2);
    const key = rawKey.replace(/-([a-z])/g, (_, ch) => ch.toUpperCase());
    const value = inlineValue ?? argv[++i];
    if (value === undefined) throw new Error(`missing value for ${arg}`);
    args[key] = value;
  }
  for (const key of ["devtoolsPort", "urlContains", "outDir"]) {
    if (!args[key]) throw new Error(`missing ${key}`);
  }
  args.devtoolsPort = Number(args.devtoolsPort);
  args.timeoutSeconds = Number(args.timeoutSeconds);
  args.settleSeconds = Number(args.settleSeconds);
  args.outDir = path.resolve(args.outDir);
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
  if (!response.ok)
    throw new Error(`GET ${url} failed: HTTP ${response.status}`);
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
  throw new Error(
    `no page target contained ${JSON.stringify(args.urlContains)}`,
  );
}

function connectDevTools(wsUrl) {
  const socket = new WebSocket(wsUrl);
  let nextId = 1;
  const pending = new Map();
  const events = [];
  const attachedTargets = new Map();

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
      if (message.method === "Target.attachedToTarget") {
        attachedTargets.set(message.params.sessionId, {
          sessionId: message.params.sessionId,
          targetInfo: message.params.targetInfo,
        });
      } else if (message.method === "Target.detachedFromTarget") {
        attachedTargets.delete(message.params.sessionId);
      }
    }
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

  return { socket, open, send, events, attachedTargets };
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

async function captureScreenshot(client, args, relativePath) {
  const result = await client.send("Page.captureScreenshot", {
    format: "png",
    fromSurface: true,
  });
  const bytes = Buffer.from(result.data || "", "base64");
  const filePath = path.join(args.outDir, relativePath);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, bytes);
  return {
    relativePath,
    bytes: bytes.length,
    sha256: crypto.createHash("sha256").update(bytes).digest("hex"),
  };
}

const STATE_SOURCE = `(() => {
  const deepQueryAll = (root, selector, depth = 0) => {
    if (!root || depth > 12) return [];
    const out = [...(root.querySelectorAll?.(selector) || [])];
    for (const el of root.querySelectorAll?.("*") || []) {
      if (el.shadowRoot) out.push(...deepQueryAll(el.shadowRoot, selector, depth + 1));
    }
    return out;
  };
  const rect = (el) => {
    const box = el?.getBoundingClientRect?.();
    return box ? {x: box.x, y: box.y, width: box.width, height: box.height} : null;
  };
  const primitive = (value) => {
    if (value === null || ["string", "number", "boolean"].includes(typeof value)) return value;
    if (Array.isArray(value)) return {type: "array", length: value.length};
    return undefined;
  };
  const props = (obj) => {
    const out = {};
    if (!obj) return out;
    let current = obj;
    let depth = 0;
    const seen = new Set();
    while (current && depth < 5) {
      for (const name of Object.getOwnPropertyNames(current).sort()) {
        if (seen.has(name) || name === "constructor") continue;
        seen.add(name);
        try {
          const descriptor = Object.getOwnPropertyDescriptor(current, name);
          if (!descriptor) continue;
          const value = primitive(obj[name]);
          if (value !== undefined) out[name] = {depth, value, accessor: !!descriptor.get};
        } catch (error) {
          out[name] = {depth, error: String(error)};
        }
      }
      current = Object.getPrototypeOf(current);
      depth += 1;
    }
    return out;
  };
  const loadFlag = (name) => {
    try {
      if (globalThis.loadTimeData?.valueExists?.(name)) {
        const value = globalThis.loadTimeData.getValue(name);
        if (["string", "number", "boolean"].includes(typeof value)) return value;
      }
    } catch (error) {
      return {error: String(error)};
    }
    return null;
  };
  const viewer = document.querySelector("pdf-viewer#viewer") || document.querySelector("pdf-viewer");
  const viewerRoot = viewer?.shadowRoot || null;
  const toolbar = viewerRoot?.querySelector("viewer-toolbar#toolbar") || viewerRoot?.querySelector("viewer-toolbar") || null;
  const plugin = viewerRoot?.querySelector("#plugin") || document.querySelector("embed") || null;
  const controls = deepQueryAll(document, "button,cr-icon-button,*[role=button],input,textarea,select,viewer-toolbar,viewer-bottom-toolbar,viewer-side-panel,ink-text-box")
    .slice(0, 360)
    .map((el) => ({
      tag: el.tagName,
      id: el.id || "",
      className: String(el.className || ""),
      role: el.getAttribute("role") || "",
      ariaLabel: el.getAttribute("aria-label") || "",
      title: el.getAttribute("title") || "",
      text: (el.innerText || el.value || "").slice(0, 160),
      disabled: !!el.disabled || el.getAttribute("aria-disabled") === "true",
      hidden: !!el.hidden || getComputedStyle(el).visibility === "hidden" || getComputedStyle(el).display === "none",
      rect: rect(el),
    }));
  const annotationControls = controls.filter((control) =>
    /annot|ink|draw|pen|highlight|text/i.test([
      control.id,
      control.className,
      control.ariaLabel,
      control.title,
      control.text,
    ].join(" ")));
  const searchifyProgress = viewerRoot?.querySelector("#searchifyProgress") || null;
  return {
    url: location.href,
    title: document.title,
    activeElement: {
      tag: document.activeElement?.tagName || "",
      id: document.activeElement?.id || "",
      className: String(document.activeElement?.className || ""),
    },
    viewport: {innerWidth, innerHeight, devicePixelRatio},
    viewerPresent: !!viewer,
    pluginPresent: !!plugin,
    pluginRect: rect(plugin),
    toolbarRect: rect(toolbar),
    viewerProps: props(viewer),
    toolbarProps: props(toolbar),
    controllerProps: props(viewer?.currentController),
    viewportProps: props(viewer?.viewport_),
    controls,
    annotationControls,
    searchifyProgress: searchifyProgress ? {
      rect: rect(searchifyProgress),
      hidden: !!searchifyProgress.hidden || getComputedStyle(searchifyProgress).visibility === "hidden" || getComputedStyle(searchifyProgress).display === "none",
      text: searchifyProgress.innerText || "",
    } : null,
    loadTimeFlags: {
      pdfInk2Enabled: loadFlag("pdfInk2Enabled"),
      pdfTextAnnotationsEnabled: loadFlag("pdfTextAnnotationsEnabled"),
      pdfSearchifySaveEnabled: loadFlag("pdfSearchifySaveEnabled"),
      printingEnabled: loadFlag("printingEnabled"),
      pdfSaveToDrive: loadFlag("pdfSaveToDrive"),
    },
  };
})()`;

async function collectStates(client) {
  const eventStart = client.events.length;
  await safeSend(client, "Target.setAutoAttach", {
    autoAttach: true,
    waitForDebuggerOnStart: false,
    flatten: true,
  });
  await sleep(500);
  const children = client.events
    .slice(eventStart)
    .filter((event) => event.method === "Target.attachedToTarget")
    .map((event) => ({
      sessionId: event.params.sessionId,
      targetInfo: event.params.targetInfo,
    }));
  const attachedChildren = [...client.attachedTargets.values()];
  for (const child of children) {
    if (!client.attachedTargets.has(child.sessionId)) {
      client.attachedTargets.set(child.sessionId, child);
    }
  }
  for (const child of attachedChildren) {
    await safeSend(client, "Runtime.enable", {}, child.sessionId);
    await safeSend(client, "Page.enable", {}, child.sessionId);
    await safeSend(client, "DOM.enable", {}, child.sessionId);
  }
  const states = [
    {
      sessionId: null,
      targetInfo: null,
      state: await evaluate(client, STATE_SOURCE),
    },
  ];
  for (const child of attachedChildren) {
    states.push({
      sessionId: child.sessionId,
      targetInfo: child.targetInfo,
      state: await evaluate(client, STATE_SOURCE, child.sessionId),
    });
  }
  const values = states
    .filter((state) => state.state.ok && state.state.value)
    .map((state) => ({
      sessionId: state.sessionId,
      targetInfo: state.targetInfo,
      value: state.state.value,
    }));
  return { children: attachedChildren, states, values };
}

function pluginLoaded(collected) {
  return (collected.values || []).some((item) => {
    const props = item.value.viewerProps || {};
    const rect = item.value.pluginRect || {};
    return (
      props.loadState_?.value === "success" &&
      props.docLength_?.value &&
      rect.width > 0 &&
      rect.height > 0
    );
  });
}

function pdfValue(collected) {
  return (
    (collected.values || []).find(
      (item) => item.value?.viewerPresent || item.value?.pluginPresent,
    )?.value || null
  );
}

async function waitForPlugin(client, expectedUrlPart, timeoutSeconds) {
  const deadline = Date.now() + timeoutSeconds * 1000;
  let lastCollected = null;
  while (Date.now() < deadline) {
    const collected = await collectStates(client);
    lastCollected = collected;
    const value = pdfValue(collected);
    if (
      pluginLoaded(collected) &&
      (!expectedUrlPart ||
        value?.url?.includes(expectedUrlPart) ||
        value?.title?.includes(expectedUrlPart))
    ) {
      return collected;
    }
    await sleep(250);
  }
  return lastCollected;
}

async function captureAnnotationComparison(client, args, summary) {
  const control = await waitForPlugin(
    client,
    args.urlContains,
    args.timeoutSeconds,
  );
  summary.annotationControl = {
    pluginLoaded: pluginLoaded(control),
    value: pdfValue(control),
    screenshot: await captureScreenshot(client, args, "annotation-control.png"),
  };

  if (!args.annotationUrl) {
    throw new Error("annotation probe missing --annotation-url");
  }

  summary.annotationNavigate = await safeSend(client, "Page.navigate", {
    url: args.annotationUrl,
  });
  const annotatedName = path.basename(new URL(args.annotationUrl).pathname);
  await sleep(args.settleSeconds * 1000);
  const annotated = await waitForPlugin(
    client,
    annotatedName,
    args.timeoutSeconds,
  );
  summary.annotationAnnotated = {
    pluginLoaded: pluginLoaded(annotated),
    value: pdfValue(annotated),
    screenshot: await captureScreenshot(
      client,
      args,
      "annotation-annotated.png",
    ),
  };
  summary.children = annotated.children;
  summary.states = annotated.states;
  summary.values = annotated.values;
  summary.pluginLoaded = pluginLoaded(annotated);
  summary.screenshot = await captureScreenshot(
    client,
    args,
    "advanced-state.png",
  );
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  fs.mkdirSync(args.outDir, { recursive: true });
  const summary = {
    devtoolsPort: args.devtoolsPort,
    urlContains: args.urlContains,
    probe: args.probe,
  };
  let client = null;
  try {
    const target = await pollTarget(args, summary);
    summary.selectedTarget = {
      id: target.id,
      type: target.type,
      url: target.url,
      title: target.title,
    };
    client = connectDevTools(target.webSocketDebuggerUrl);
    await client.open;
    for (const domain of ["Page", "Runtime", "DOM", "Target"]) {
      summary[`${domain}Enabled`] = await safeSend(client, `${domain}.enable`);
    }
    await safeSend(client, "Page.bringToFront");
    await sleep(args.settleSeconds * 1000);
    if (args.probe === "annotations") {
      await captureAnnotationComparison(client, args, summary);
    } else {
      const collected = await collectStates(client);
      summary.children = collected.children;
      summary.states = collected.states;
      summary.values = collected.values;
      summary.pluginLoaded = pluginLoaded(collected);
      summary.screenshot = await captureScreenshot(
        client,
        args,
        "advanced-state.png",
      );
    }
    summary.status = "pass";
    summary.firstFailingHop = "no-failure-observed";
  } catch (error) {
    summary.status = "error";
    summary.firstFailingHop = "devtools-probe-error";
    summary.error = String(error.stack || error);
    throw error;
  } finally {
    writeJson(
      path.join(args.outDir, "pdf-advanced-devtools-summary.json"),
      summary,
    );
    client?.socket?.close();
  }
}

main().catch((error) => {
  console.error(error.stack || error);
  process.exit(1);
});
