#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

function parseArgs(argv) {
  const args = {
    timeoutSeconds: 30,
    settleSeconds: 3,
    navigationsJson: "[]",
    initialLabel: "initial",
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith("--")) throw new Error(`unexpected argument: ${arg}`);
    const [rawKey, inlineValue] = arg.slice(2).split("=", 2);
    const key = rawKey.replace(/-([a-z])/g, (_, ch) => ch.toUpperCase());
    const value = inlineValue ?? argv[++i];
    if (value === undefined) throw new Error(`missing value for ${arg}`);
    args[key] = value;
  }
  for (const key of ["devtoolsPort", "urlContains", "outDir", "probe"]) {
    if (!args[key]) throw new Error(`missing ${key}`);
  }
  args.devtoolsPort = Number(args.devtoolsPort);
  args.timeoutSeconds = Number(args.timeoutSeconds);
  args.settleSeconds = Number(args.settleSeconds);
  args.outDir = path.resolve(args.outDir);
  args.navigations = JSON.parse(args.navigationsJson);
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

const STATE_SOURCE = `(() => {
  const rectOf = (el) => {
    const rect = el?.getBoundingClientRect?.();
    if (!rect) return null;
    return {
      tag: el.tagName,
      id: el.id || "",
      className: String(el.className || ""),
      role: el.getAttribute("role") || "",
      ariaLabel: el.getAttribute("aria-label") || "",
      title: el.getAttribute("title") || "",
      text: (el.innerText || "").slice(0, 240),
      x: rect.x,
      y: rect.y,
      width: rect.width,
      height: rect.height,
    };
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
  const deepText = [];
  const errorElements = [];
  const isVisible = (el) => {
    const rect = el.getBoundingClientRect?.();
    if (!rect || rect.width <= 0 || rect.height <= 0) return false;
    const style = getComputedStyle(el);
    return style.visibility !== "hidden" && style.display !== "none";
  };
  const visit = (root, depth = 0, scope = "document") => {
    if (!root || depth > 12 || deepText.length > 500) return;
    for (const el of root.querySelectorAll?.("*") || []) {
      if (["SCRIPT", "STYLE", "TEMPLATE", "SVG", "G", "PATH"].includes(el.tagName)) {
        continue;
      }
      const text = [
        el.id || "",
        String(el.className || ""),
        el.getAttribute("aria-label") || "",
        el.getAttribute("title") || "",
        el.innerText || "",
      ].join(" ").replace(/\\s+/g, " ").trim();
      if (text) {
        deepText.push({scope, tag: el.tagName, text: text.slice(0, 240)});
        if (/error|fail|invalid|couldn't|could not|can't|cannot|damaged|corrupt/i.test(text) && isVisible(el)) {
          errorElements.push({...rectOf(el), scope, text: text.slice(0, 240)});
        }
      }
      if (el.shadowRoot) visit(el.shadowRoot, depth + 1, \`\${scope} > \${el.tagName.toLowerCase()}#\${el.id || ""}\`);
    }
  };
  visit(document);
  const viewer = document.querySelector("pdf-viewer#viewer") || document.querySelector("pdf-viewer");
  const viewerRoot = viewer?.shadowRoot || null;
  const plugin = viewerRoot?.querySelector("#plugin") || document.querySelector("embed") || null;
  const bodyText = document.body?.innerText || "";
  return {
    url: location.href,
    title: document.title,
    readyState: document.readyState,
    bodyText: bodyText.slice(0, 2000),
    bodyTextHasError: /error|fail|invalid|couldn't|could not|can't|cannot|damaged|corrupt/i.test(bodyText),
    viewport: {innerWidth, innerHeight, devicePixelRatio},
    viewerPresent: !!viewer,
    pluginPresent: !!plugin,
    pluginRect: rectOf(plugin),
    viewerProps: props(viewer),
    controllerProps: props(viewer?.currentController),
    errorElements: errorElements.slice(0, 40),
    deepText: deepText.slice(0, 80),
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
  for (const child of children) {
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
  for (const child of children) {
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
  const pluginStates = values.map((item) => ({
    sessionId: item.sessionId,
    targetUrl: item.targetInfo?.url || item.value.url,
    loadState: item.value.viewerProps?.loadState_?.value ?? null,
    loadProgress: item.value.viewerProps?.loadProgress_?.value ?? null,
    docLength: item.value.viewerProps?.docLength_?.value ?? null,
    pluginPresent: item.value.pluginPresent,
    pluginRect: item.value.pluginRect,
    viewerPresent: item.value.viewerPresent,
    bodyTextHasError: item.value.bodyTextHasError,
    errorElementCount: item.value.errorElements?.length ?? 0,
  }));
  return { children, states, values, pluginStates };
}

function pluginLoaded(snapshot) {
  return (snapshot.pluginStates || []).some((item) => {
    const rect = item.pluginRect || {};
    return (
      item.loadState === "success" &&
      item.docLength &&
      rect.width > 0 &&
      rect.height > 0
    );
  });
}

function hasErrorEvidence(snapshot) {
  return (snapshot.values || []).some((item) => {
    const value = item.value || {};
    const loadStates = Object.values(value.viewerProps || {})
      .map((prop) => prop?.value)
      .filter((prop) => prop !== null && prop !== undefined)
      .join(" ");
    return (
      value.bodyTextHasError ||
      (value.errorElements || []).length > 0 ||
      /error|fail|invalid|couldn't|could not|can't|cannot|damaged|corrupt/i.test(loadStates)
    );
  });
}

async function snapshot(client, label) {
  const collected = await collectStates(client);
  return {
    label,
    children: collected.children,
    states: collected.states,
    values: collected.values,
    pluginStates: collected.pluginStates,
    pluginLoaded: pluginLoaded(collected),
    errorEvidence: hasErrorEvidence(collected),
    malformedOutcome: malformedOutcome(collected),
  };
}

function malformedOutcome(snapshot) {
  if (pluginLoaded(snapshot)) return "loaded-plugin-success";
  if (hasErrorEvidence(snapshot)) return "visible-error-evidence";
  const values = snapshot.values || [];
  if (
    values.length > 0 &&
    values.every((item) => !item.value?.viewerPresent && !item.value?.pluginPresent)
  ) {
    return "no-viewer-no-plugin";
  }
  return "unclassified";
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  fs.mkdirSync(args.outDir, { recursive: true });
  const summary = {
    devtoolsPort: args.devtoolsPort,
    urlContains: args.urlContains,
    probe: args.probe,
    navigations: args.navigations,
    snapshots: [],
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
    summary.snapshots.push(await snapshot(client, args.initialLabel));
    for (const navigation of args.navigations) {
      const result = await safeSend(client, "Page.navigate", { url: navigation.url });
      summary.snapshots.push({
        label: `${navigation.label}-navigate-result`,
        navigation,
        navigateResult: result,
      });
      await sleep(args.settleSeconds * 1000);
      summary.snapshots.push(await snapshot(client, navigation.label));
    }
    summary.status = "pass";
    summary.firstFailingHop = "no-failure-observed";
  } catch (error) {
    summary.status = "error";
    summary.firstFailingHop = "devtools-probe-error";
    summary.error = String(error.stack || error);
    throw error;
  } finally {
    writeJson(path.join(args.outDir, "pdf-error-devtools-summary.json"), summary);
    client?.socket?.close();
  }
}

main().catch((error) => {
  console.error(error.stack || error);
  process.exit(1);
});
