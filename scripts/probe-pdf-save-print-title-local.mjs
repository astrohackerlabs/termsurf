#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

function parseArgs(argv) {
  const args = {
    timeoutSeconds: 30,
    settleSeconds: 8,
    actionSettleMs: 900,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith("--")) {
      throw new Error(`unexpected positional argument: ${arg}`);
    }
    const [rawKey, inlineValue] = arg.slice(2).split("=", 2);
    const key = rawKey.replace(/-([a-z])/g, (_, ch) => ch.toUpperCase());
    const value = inlineValue ?? argv[++i];
    if (value === undefined) {
      throw new Error(`missing value for ${arg}`);
    }
    args[key] = value;
  }
  for (const key of ["devtoolsPort", "urlContains", "outDir", "downloadsDir"]) {
    if (!args[key]) {
      throw new Error(
        `missing --${key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`,
      );
    }
  }
  args.devtoolsPort = Number(args.devtoolsPort);
  args.timeoutSeconds = Number(args.timeoutSeconds);
  args.settleSeconds = Number(args.settleSeconds);
  args.actionSettleMs = Number(args.actionSettleMs);
  args.outDir = path.resolve(args.outDir);
  args.downloadsDir = path.resolve(args.downloadsDir);
  if (args.traceFile) {
    args.traceFile = path.resolve(args.traceFile);
  }
  if (args.roamiumStderr) {
    args.roamiumStderr = path.resolve(args.roamiumStderr);
  }
  if (args.printInterceptFile) {
    args.printInterceptFile = path.resolve(args.printInterceptFile);
  }
  if (args.printBridgeTraceFile) {
    args.printBridgeTraceFile = path.resolve(args.printBridgeTraceFile);
  }
  if (args.nativePrintTraceFile) {
    args.nativePrintTraceFile = path.resolve(args.nativePrintTraceFile);
  }
  args.allowNativePrintClick =
    args.allowNativePrintClick === "1" || args.allowNativePrintClick === "true";
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
  if (!response.ok) {
    throw new Error(`GET ${url} failed: HTTP ${response.status}`);
  }
  return await response.json();
}

async function pollTarget(args, urlContains) {
  const deadline = Date.now() + args.timeoutSeconds * 1000;
  const listUrl = `http://127.0.0.1:${args.devtoolsPort}/json/list`;
  let lastTargets = [];
  while (Date.now() < deadline) {
    try {
      lastTargets = await fetchJson(listUrl);
      const match = lastTargets.find(
        (target) =>
          target.type === "page" &&
          typeof target.url === "string" &&
          target.url.includes(urlContains) &&
          target.webSocketDebuggerUrl,
      );
      if (match) {
        return match;
      }
    } catch {
      // Keep polling while Roamium starts DevTools.
    }
    await sleep(250);
  }
  throw new Error(
    `no page target contained ${JSON.stringify(urlContains)}; targets=${JSON.stringify(
      lastTargets.map((target) => ({
        type: target.type,
        url: target.url,
        title: target.title,
      })),
    )}`,
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

  function send(method, params = {}, sessionId = undefined) {
    const id = nextId;
    nextId += 1;
    const promise = new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
    });
    const message = { id, method, params };
    if (sessionId) {
      message.sessionId = sessionId;
    }
    socket.send(JSON.stringify(message));
    return promise;
  }

  return { socket, open, send, events };
}

async function safeSend(client, method, params = {}, sessionId = undefined) {
  try {
    return {
      ok: true,
      result: await sendWithTimeout(client, method, params, sessionId),
    };
  } catch (error) {
    return { ok: false, error: String(error.message || error) };
  }
}

async function sendWithTimeout(
  client,
  method,
  params = {},
  sessionId = undefined,
  timeoutMs = 10000,
) {
  return await Promise.race([
    client.send(method, params, sessionId),
    sleep(timeoutMs).then(() => {
      throw new Error(`${method} timed out after ${timeoutMs}ms`);
    }),
  ]);
}

async function evaluate(client, expression, sessionId = undefined) {
  const result = await safeSend(
    client,
    "Runtime.evaluate",
    { expression, awaitPromise: true, returnByValue: true },
    sessionId,
  );
  if (!result.ok) {
    return { ok: false, error: result.error };
  }
  if (result.result.exceptionDetails) {
    return { ok: false, error: JSON.stringify(result.result.exceptionDetails) };
  }
  return { ok: true, value: result.result.result?.value ?? null };
}

async function captureScreenshot(client, args, relativePath) {
  const result = await Promise.race([
    client.send("Page.captureScreenshot", {
      format: "png",
      fromSurface: true,
    }),
    sleep(5000).then(() => {
      throw new Error(`Page.captureScreenshot timed out for ${relativePath}`);
    }),
  ]);
  const png = Buffer.from(result.data || "", "base64");
  const filePath = path.join(args.outDir, relativePath);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, png);
  return { relativePath, bytes: png.length, sha256: await sha256(png) };
}

async function sha256(buffer) {
  const digest = await crypto.subtle.digest("SHA-256", buffer);
  return Array.from(new Uint8Array(digest))
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
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
  const visit = (root, depth = 0, scope = "document") => {
    if (!root || depth > 10 || controls.length > 260 || elements.length > 260) {
      return;
    }
    for (const el of root.querySelectorAll("button,input,select,cr-icon-button,*[role=button],viewer-toolbar,viewer-page-selector,viewer-download-controls")) {
      controls.push({...rectOf(el), scope, depth});
    }
    for (const el of root.querySelectorAll("embed,iframe,canvas,pdf-viewer,#viewer,#plugin,#sizer,#container,#page-container,.page,.page-container,viewer-toolbar,viewer-page-selector")) {
      elements.push({...rectOf(el), scope, depth});
    }
    for (const el of root.querySelectorAll("*")) {
      if (el.shadowRoot) {
        visit(el.shadowRoot, depth + 1, \`\${scope} > \${el.tagName.toLowerCase()}#\${el.id || ""}\`);
      }
    }
  };
  visit(document);
  const scrolling = document.scrollingElement || document.documentElement || document.body;
  const loadTimeDataObject = globalThis.loadTimeData || null;
  const loadTimeData = loadTimeDataObject?.data || loadTimeDataObject?.data_ || null;
  const pdfViewerPrivate = chrome?.pdfViewerPrivate || null;
  const resourcesPrivate = chrome?.resourcesPrivate || null;
  const lastErrorBefore = chrome?.runtime?.lastError?.message || null;
  const visibleElements = elements.filter((el) => !el.hidden && el.width > 0 && el.height > 0);
  const largest = [...visibleElements].sort((a, b) => b.width * b.height - a.width * a.height)[0] || null;
  return {
    url: location.href,
    title: document.title,
    viewport: {innerWidth, innerHeight, devicePixelRatio},
    scroll: scrolling ? {
      x: window.scrollX,
      y: window.scrollY,
      top: scrolling.scrollTop,
      left: scrolling.scrollLeft,
      height: scrolling.scrollHeight,
      width: scrolling.scrollWidth,
      clientHeight: scrolling.clientHeight,
      clientWidth: scrolling.clientWidth,
    } : null,
    controls,
    elements,
    largestElement: largest,
    bodyTextSample: document.body ? document.body.innerText.slice(0, 1000) : "",
    loadTimeFlags: loadTimeData ? {
      printingEnabled: loadTimeData.printingEnabled ?? null,
      pdfUseShowSaveFilePicker: loadTimeData.pdfUseShowSaveFilePicker ?? null,
      pdfGetSaveDataInBlocks: loadTimeData.pdfGetSaveDataInBlocks ?? null,
      pdfSaveToDrive: loadTimeData.pdfSaveToDrive ?? null,
      presetZoomFactors: loadTimeData.presetZoomFactors ?? null,
    } : null,
    api: {
      hasPdfViewerPrivate: !!pdfViewerPrivate,
      hasResourcesPrivate: !!resourcesPrivate,
      setPdfDocumentTitle: typeof pdfViewerPrivate?.setPdfDocumentTitle,
      isAllowedLocalFileAccess: typeof pdfViewerPrivate?.isAllowedLocalFileAccess,
      getStreamInfo: typeof pdfViewerPrivate?.getStreamInfo,
      saveToDrive: typeof pdfViewerPrivate?.saveToDrive,
    },
    lastErrorBefore,
  };
})()`;

const ACTIVATE_SOURCE = `((kind, activationId = "") => {
  window.__termsurfPdfPrintBridgeActivationId = activationId;
  const aliases = {
    zoomIn: ["zoom in", "zoom-in", "zoomin", "increase zoom"],
    pageNext: ["next page", "next"],
    download: ["download", "save"],
    print: ["print"],
    rotateCounterclockwise: ["rotate counterclockwise", "rotate"],
  };
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
      token: haystack(el),
    };
  };
  const candidates = [];
  const visit = (root, depth = 0, scope = "document") => {
    if (!root || depth > 10 || candidates.length > 240) {
      return;
    }
    for (const el of root.querySelectorAll("button,input,cr-icon-button,*[role=button]")) {
      const info = {...rectOf(el), scope, depth};
      const token = info.token;
      const score = (aliases[kind] || []).reduce((value, alias) => value + (token.includes(alias) ? alias.length : 0), 0);
      candidates.push({info, score, element: el});
    }
    for (const el of root.querySelectorAll("*")) {
      if (el.shadowRoot) {
        visit(el.shadowRoot, depth + 1, \`\${scope} > \${el.tagName.toLowerCase()}#\${el.id || ""}\`);
      }
    }
  };
  visit(document);
  if (kind === "pageNext") {
    const input = candidates.find((candidate) => candidate.info.id === "pageSelector" || candidate.info.token.includes("pageselector"));
    if (input) {
      const current = Number(String(input.element.value || "1").replace(/\\D+/g, "")) || 1;
      input.element.focus?.();
      input.element.value = String(current + 1);
      input.element.dispatchEvent(new Event("input", {bubbles: true, composed: true}));
      input.element.dispatchEvent(new Event("change", {bubbles: true, composed: true}));
      input.element.dispatchEvent(new KeyboardEvent("keydown", {key: "Enter", code: "Enter", bubbles: true, composed: true}));
      input.element.dispatchEvent(new KeyboardEvent("keyup", {key: "Enter", code: "Enter", bubbles: true, composed: true}));
      return {found: true, activated: true, method: "dom-page-selector", selected: input.info};
    }
  }
  const matches = candidates
    .filter((candidate) => candidate.score > 0)
    .sort((a, b) => b.score - a.score || Number(a.info.disabled) - Number(b.info.disabled));
  const selected = matches.find((candidate) => !candidate.info.disabled && !candidate.info.hidden) || matches[0];
  if (!selected) {
    return {found: false, candidates: candidates.map((candidate) => candidate.info).slice(0, 80)};
  }
  if (selected.info.disabled || selected.info.hidden) {
    return {
      found: true,
      activated: false,
      reason: selected.info.disabled ? "disabled" : "hidden",
      selected: selected.info,
      candidates: matches.map((candidate) => candidate.info).slice(0, 20),
    };
  }
  if (kind === "print") {
    const viewer = document.querySelector("pdf-viewer");
    const toolbar =
      viewer?.shadowRoot?.querySelector("#toolbar");
    if (toolbar) {
      toolbar.dispatchEvent(new CustomEvent("print"));
    } else {
      selected.element.click();
    }
    const diagnostics = {
      hasViewer: !!viewer,
      viewerKeys: viewer ? Object.keys(viewer).sort() : [],
      currentControllerPrintType: typeof viewer?.currentController?.print,
      pluginControllerType: typeof viewer?.pluginController_,
      pluginControllerPrintType: typeof viewer?.pluginController_?.print,
      pluginControllerIsActive: viewer?.pluginController_?.isActive ?? null,
      pluginElementType: typeof viewer?.pluginController_?.plugin_,
      pluginPostMessageType:
        typeof viewer?.pluginController_?.plugin_?.postMessage,
    };
    viewer?.currentController?.print?.();
    viewer?.pluginController_?.print?.();
    viewer?.pluginController_?.plugin_?.postMessage?.({
      type: "print",
      termsurfActivationId: activationId,
    });
    return {
      found: true,
      activated: true,
      method: toolbar
        ? "toolbar-print-event-controller-plugin-print"
        : "dom-click-controller-plugin-print",
      selected: selected.info,
      diagnostics,
      candidates: matches.map((candidate) => candidate.info).slice(0, 20),
    };
  }
  return {
    found: true,
    activated: false,
    method: "candidate-found",
    selected: selected.info,
    candidates: matches.map((candidate) => candidate.info).slice(0, 20),
  };
})`;

async function attachChildTargets(client, sinceEventIndex = 0) {
  await safeSend(client, "Target.setAutoAttach", {
    autoAttach: true,
    waitForDebuggerOnStart: false,
    flatten: true,
  });
  await sleep(1000);
  const children = client.events
    .slice(sinceEventIndex)
    .filter((event) => event.method === "Target.attachedToTarget")
    .map((event) => ({
      sessionId: event.params.sessionId,
      targetInfo: event.params.targetInfo,
    }));
  for (const child of children) {
    await safeSend(client, "Runtime.enable", {}, child.sessionId);
    await safeSend(client, "Page.enable", {}, child.sessionId);
  }
  return children;
}

async function collectState(client, children) {
  const top = await evaluate(client, STATE_SOURCE);
  const childStates = [];
  for (const child of children) {
    childStates.push({
      sessionId: child.sessionId,
      targetInfo: child.targetInfo,
      state: await evaluate(client, STATE_SOURCE, child.sessionId),
    });
  }
  return { top, children: childStates };
}

function stateValues(state) {
  const values = [];
  if (state.top?.ok && state.top.value) {
    values.push({ sessionId: null, targetInfo: null, value: state.top.value });
  }
  for (const child of state.children || []) {
    if (child.state?.ok && child.state.value) {
      values.push({
        sessionId: child.sessionId,
        targetInfo: child.targetInfo,
        value: child.state.value,
      });
    }
  }
  return values;
}

function flattenControls(state) {
  return stateValues(state).flatMap(({ sessionId, targetInfo, value }) =>
    (value.controls || []).map((control) => ({
      sessionId,
      targetUrl: targetInfo?.url || value.url,
      ...control,
    })),
  );
}

function hasControl(controls, kind) {
  const aliases =
    {
      download: ["download", "save"],
      print: ["print"],
    }[kind] || [];
  return controls.some((control) => {
    const token = String(control.token || "").toLowerCase();
    return aliases.some((alias) => token.includes(alias));
  });
}

function placeholderDiagnostics(controls) {
  const placeholders = controls.filter((control) =>
    /\$i18n\{[^}]+\}/.test(
      [
        control.token || "",
        control.ariaLabel || "",
        control.title || "",
        control.text || "",
      ].join(" "),
    ),
  );
  return {
    count: placeholders.length,
    examples: placeholders.slice(0, 20).map((control) => ({
      sessionId: control.sessionId,
      targetUrl: control.targetUrl,
      tag: control.tag,
      id: control.id,
      token: control.token,
      ariaLabel: control.ariaLabel,
      title: control.title,
      text: control.text,
    })),
  };
}

function missingStringErrors(events) {
  return events
    .filter((event) => {
      const text = JSON.stringify(event);
      return text.includes("Could not find value for");
    })
    .map((event) => ({
      method: event.method,
      params: event.params,
    }));
}

function readTitleTraceEvidence(args, startLine = 0) {
  if (!args.traceFile || !fs.existsSync(args.traceFile)) {
    return {
      traceFile: args.traceFile || null,
      startLine,
      lines: [],
      titles: [],
    };
  }
  const allLines = fs
    .readFileSync(args.traceFile, "utf8")
    .split(/\r?\n/)
    .filter(Boolean);
  const lines = allLines
    .slice(startLine)
    .filter((line) => line.includes("title-changed"));
  const titles = lines
    .map((line) => line.match(/\btitle=(.*)$/)?.[1] || "")
    .filter(Boolean);
  return {
    traceFile: args.traceFile,
    startLine,
    lineCount: allLines.length,
    lines,
    titles,
  };
}

function traceLineCount(args) {
  if (!args.traceFile || !fs.existsSync(args.traceFile)) {
    return 0;
  }
  return fs.readFileSync(args.traceFile, "utf8").split(/\r?\n/).filter(Boolean)
    .length;
}

function readRoamiumLogEvidence(args, startLine = 0, predicate = () => true) {
  if (!args.roamiumStderr || !fs.existsSync(args.roamiumStderr)) {
    return {
      logFile: args.roamiumStderr || null,
      startLine,
      lineCount: 0,
      lines: [],
    };
  }
  const allLines = fs
    .readFileSync(args.roamiumStderr, "utf8")
    .split(/\r?\n/)
    .filter(Boolean);
  return {
    logFile: args.roamiumStderr,
    startLine,
    lineCount: allLines.length,
    lines: allLines.slice(startLine).filter(predicate),
  };
}

function roamiumLogLineCount(args) {
  if (!args.roamiumStderr || !fs.existsSync(args.roamiumStderr)) {
    return 0;
  }
  return fs
    .readFileSync(args.roamiumStderr, "utf8")
    .split(/\r?\n/)
    .filter(Boolean).length;
}

async function probeLoadTimeString(client, state, children, key) {
  const sessionId = bestSessionForPdf(state, children);
  const expression = `(async () => {
    const module = await import("chrome://resources/js/load_time_data.js");
    return module.loadTimeData.getStringF(${JSON.stringify(key)}, 1);
  })()`;
  return await evaluate(client, expression, sessionId);
}

async function probeLoadTimeBoolean(client, state, children, key) {
  const sessionId = bestSessionForPdf(state, children);
  const expression = `(async () => {
    const module = await import("chrome://resources/js/load_time_data.js");
    return module.loadTimeData.getBoolean(${JSON.stringify(key)});
  })()`;
  return await evaluate(client, expression, sessionId);
}

function bestSessionForPdf(state, children) {
  const values = stateValues(state);
  const child = values.find(({ targetInfo, value }) => {
    return (
      targetInfo?.url?.startsWith("chrome-extension://") ||
      value.url?.startsWith("chrome-extension://")
    );
  });
  if (child) {
    return child.sessionId || undefined;
  }
  return children[0]?.sessionId;
}

async function clickControl(client, state, children, kind, activationId = "") {
  const expression = `${ACTIVATE_SOURCE}(${JSON.stringify(kind)}, ${JSON.stringify(activationId)})`;
  const sessions = [
    { sessionId: bestSessionForPdf(state, children) },
    { sessionId: undefined },
    ...children,
  ];
  const seen = new Set();
  const attempts = [];
  for (const session of sessions) {
    const key = session.sessionId || "top";
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);
    const result = await evaluate(client, expression, session.sessionId);
    attempts.push({ sessionId: session.sessionId || null, result });
    if (result.ok && result.value?.activated) {
      return { ok: true, attempts, activation: result.value };
    }
    if (result.ok && result.value?.found && result.value?.selected) {
      const selected = result.value.selected;
      const x = Math.round(selected.x + selected.width / 2);
      const y = Math.round(selected.y + selected.height / 2);
      await client.send(
        "Input.dispatchMouseEvent",
        { type: "mouseMoved", x, y, button: "none", buttons: 0 },
        session.sessionId,
      );
      await client.send(
        "Input.dispatchMouseEvent",
        {
          type: "mousePressed",
          x,
          y,
          button: "left",
          buttons: 1,
          clickCount: 1,
        },
        session.sessionId,
      );
      await client.send(
        "Input.dispatchMouseEvent",
        {
          type: "mouseReleased",
          x,
          y,
          button: "left",
          buttons: 0,
          clickCount: 1,
        },
        session.sessionId,
      );
      return {
        ok: true,
        attempts,
        activation: {
          ...result.value,
          activated: true,
          method: "cdp-mouse",
          x,
          y,
        },
      };
    }
  }
  return { ok: false, attempts };
}

function scrollYSignature(state) {
  return stateValues(state)
    .map(({ value }) => value.scroll?.top ?? value.scroll?.y ?? null)
    .join("|");
}

function pageSignature(state) {
  return JSON.stringify(
    stateValues(state).map(({ value }) =>
      (value.controls || [])
        .filter((control) => /page|pageselector/i.test(control.token || ""))
        .map((control) => ({
          id: control.id,
          text: control.text,
          token: control.token,
        })),
    ),
  );
}

function zoomSignature(state) {
  return JSON.stringify(
    stateValues(state).map(({ value }) =>
      (value.controls || [])
        .filter((control) => /%|zoom/i.test(control.token || ""))
        .map((control) => ({
          id: control.id,
          text: control.text,
          token: control.token,
        })),
    ),
  );
}

async function dispatchWheel(client, state, children) {
  const values = stateValues(state);
  const largest = values
    .flatMap(({ value }) => value.elements || [])
    .filter((element) => element.width > 0 && element.height > 0)
    .sort((a, b) => b.width * b.height - a.width * a.height)[0];
  const x = Math.round((largest?.x || 600) + (largest?.width || 1) / 2);
  const y = Math.round((largest?.y || 450) + (largest?.height || 1) / 2);
  await client.send("Input.dispatchMouseEvent", {
    type: "mouseWheel",
    x,
    y,
    deltaX: 0,
    deltaY: 650,
    button: "none",
    buttons: 0,
  });
  return { sessionId: null, x, y };
}

async function currentTopTargetTitle(args, targetId) {
  try {
    const targets = await fetchJson(
      `http://127.0.0.1:${args.devtoolsPort}/json/list`,
    );
    const target =
      targets.find((item) => item.id === targetId) ||
      targets.find(
        (item) =>
          item.type === "page" &&
          typeof item.url === "string" &&
          item.url.includes(args.urlContains),
      ) ||
      targets.find((item) => item.type === "page");
    return target
      ? { id: target.id, url: target.url, title: target.title || "" }
      : null;
  } catch (error) {
    return { error: String(error.message || error) };
  }
}

function classifyTitle(state, topTargetInfo, titleEvidence) {
  const values = stateValues(state).map(({ sessionId, targetInfo, value }) => ({
    sessionId,
    targetUrl: targetInfo?.url || "",
    url: value.url,
    documentTitle: value.title,
    api: value.api,
  }));
  const extensionTitle = values.find((item) =>
    item.url.startsWith("chrome-extension://"),
  )?.documentTitle;
  const topDocumentTitle = values.find(
    (item) => item.sessionId === null,
  )?.documentTitle;
  const topTargetTitle = topTargetInfo?.title || "";
  const traceTitles = titleEvidence?.titles || [];
  let classification = "title-unobserved";
  if (
    topTargetTitle &&
    extensionTitle &&
    topTargetTitle === extensionTitle &&
    traceTitles.includes(extensionTitle)
  ) {
    classification = "title-propagated";
  } else if (extensionTitle) {
    classification = "title-extension-only";
  } else if (
    values.some((item) => item.api?.setPdfDocumentTitle !== "function")
  ) {
    classification = "title-api-missing";
  }
  return {
    classification,
    topTargetTitle,
    topTargetInfo,
    topDocumentTitle,
    extensionTitle,
    titleEvidence,
    values,
  };
}

async function runUrlCase(client, args, targetId, label, url, children) {
  const caseDir = path.join(args.outDir, "local-parity", label);
  fs.mkdirSync(caseDir, { recursive: true });
  const eventStart = client.events.length;
  const navigate = await safeSend(client, "Page.navigate", { url });
  await sleep(args.settleSeconds * 1000);
  const freshChildren = await attachChildTargets(client, eventStart);
  const activeChildren = freshChildren.length > 0 ? freshChildren : children;
  const baseline = await collectState(client, activeChildren);
  const screenshot = await captureScreenshot(
    client,
    args,
    `local-parity/${label}/baseline.png`,
  );
  const beforeScroll = scrollYSignature(baseline);
  const beforePage = pageSignature(baseline);
  const beforeZoom = zoomSignature(baseline);
  const wheel = await dispatchWheel(client, baseline, activeChildren);
  await sleep(args.actionSettleMs);
  const afterWheel = await collectState(client, activeChildren);
  const afterWheelScreenshot = await captureScreenshot(
    client,
    args,
    `local-parity/${label}/after-wheel.png`,
  );
  const pageClick = await clickControl(
    client,
    afterWheel,
    activeChildren,
    "pageNext",
  );
  await sleep(args.actionSettleMs);
  const afterPage = await collectState(client, activeChildren);
  const zoomClick = await clickControl(
    client,
    afterPage,
    activeChildren,
    "zoomIn",
  );
  await sleep(args.actionSettleMs);
  const afterZoom = await collectState(client, activeChildren);
  const topTargetInfo = await currentTopTargetTitle(args, targetId);
  const titleEvidence = readTitleTraceEvidence(args);
  const controls = flattenControls(baseline);
  const result = {
    label,
    url,
    navigate,
    screenshot,
    render: screenshot.bytes > 1000 && stateValues(baseline).length > 0,
    scroll: {
      dispatch: wheel,
      before: beforeScroll,
      after: scrollYSignature(afterWheel),
      screenshotBefore: screenshot,
      screenshotAfter: afterWheelScreenshot,
      changed:
        beforeScroll !== scrollYSignature(afterWheel) ||
        screenshot.sha256 !== afterWheelScreenshot.sha256,
    },
    pageNavigation: {
      click: pageClick,
      before: beforePage,
      after: pageSignature(afterPage),
      changed: beforePage !== pageSignature(afterPage),
    },
    zoom: {
      click: zoomClick,
      before: beforeZoom,
      after: zoomSignature(afterZoom),
      changed: beforeZoom !== zoomSignature(afterZoom),
    },
    title: classifyTitle(afterZoom, topTargetInfo, titleEvidence),
    controlInventory: controls,
    loadTimeFlags: stateValues(baseline).map(({ sessionId, value }) => ({
      sessionId,
      url: value.url,
      loadTimeFlags: value.loadTimeFlags,
      api: value.api,
    })),
  };
  writeJson(path.join(caseDir, "summary.json"), result);
  return result;
}

async function probeEmbeddedTitle(client, args, targetId, url) {
  if (!url) {
    return { status: "not-run", reason: "no-embedded-html-url" };
  }
  const startTraceLine = traceLineCount(args);
  const startRoamiumLine = roamiumLogLineCount(args);
  const eventStart = client.events.length;
  const navigate = await safeSend(client, "Page.navigate", { url });
  await sleep(args.settleSeconds * 1000);
  const children = await attachChildTargets(client, eventStart);
  const state = await collectState(client, children);
  const screenshot = await captureScreenshot(
    client,
    args,
    "embedded-title/baseline.png",
  );
  const top = state.top?.ok ? state.top.value : null;
  const embedRects = (top?.elements || []).filter(
    (element) =>
      String(element.tag || "").toUpperCase() === "EMBED" &&
      !element.hidden &&
      element.width > 0 &&
      element.height > 0,
  );
  const childValues = stateValues({ top: null, children: state.children });
  const extensionChildren = childValues.filter(({ value, targetInfo }) =>
    String(value.url || targetInfo?.url || "").startsWith(
      "chrome-extension://",
    ),
  );
  const pluginSignals = childValues.flatMap(
    ({ value, sessionId, targetInfo }) =>
      (value.elements || [])
        .filter((element) => {
          const tag = String(element.tag || "").toUpperCase();
          const token = `${element.id || ""} ${element.className || ""} ${element.scope || ""}`;
          return (
            !element.hidden &&
            element.width > 0 &&
            element.height > 0 &&
            (tag === "EMBED" ||
              tag === "IFRAME" ||
              tag === "CANVAS" ||
              /pdf|plugin|viewer/i.test(token))
          );
        })
        .map((element) => ({
          sessionId,
          targetUrl: targetInfo?.url || value.url,
          ...element,
        })),
  );
  const topTargetInfo = await currentTopTargetTitle(args, targetId);
  const titleEvidence = readTitleTraceEvidence(args, startTraceLine);
  const embeddedLogEvidence = readRoamiumLogEvidence(
    args,
    startRoamiumLine,
    (line) =>
      line.includes("embedded-pdf.html") ||
      line.includes("url=http://127.0.0.1:9799/bitcoin.pdf") ||
      line.includes("renderer-override-create-plugin") ||
      line.includes("delegated_to_extensions=1"),
  );
  const embeddedPluginLogged = embeddedLogEvidence.lines.some(
    (line) =>
      line.includes("renderer-override-create-plugin") &&
      line.includes("mime_type=application/pdf") &&
      line.includes("delegated_to_extensions=1"),
  );
  const viewerPathExercised =
    embedRects.length > 0 &&
    (extensionChildren.length > 0 ||
      pluginSignals.length > 0 ||
      embeddedPluginLogged);
  const hostTitlePreserved =
    topTargetInfo?.title === "Embedded PDF Host" ||
    top?.title === "Embedded PDF Host";
  const overwrittenByPdf = [
    topTargetInfo?.title,
    top?.title,
    ...titleEvidence.titles,
  ]
    .filter(Boolean)
    .some(
      (title) => title !== "Embedded PDF Host" && /bitcoin|\.pdf/i.test(title),
    );
  const status =
    viewerPathExercised && hostTitlePreserved && !overwrittenByPdf
      ? "pass"
      : viewerPathExercised
        ? "fail"
        : "inconclusive";
  return {
    status,
    url,
    navigate,
    screenshot,
    topTargetInfo,
    topDocumentTitle: top?.title || "",
    embedRects,
    extensionChildren: extensionChildren.map(
      ({ sessionId, targetInfo, value }) => ({
        sessionId,
        targetUrl: targetInfo?.url || "",
        url: value.url,
        title: value.title,
        api: value.api,
      }),
    ),
    pluginSignals,
    titleEvidence,
    embeddedLogEvidence,
    embeddedPluginLogged,
    viewerPathExercised,
    hostTitlePreserved,
    overwrittenByPdf,
  };
}

function listFilesRecursive(root) {
  if (!fs.existsSync(root)) {
    return [];
  }
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

async function probeSave(client, args, baseline, children) {
  const controls = flattenControls(baseline);
  const controlFound = hasControl(controls, "download");
  fs.mkdirSync(args.downloadsDir, { recursive: true });
  const beforeFiles = listFilesRecursive(args.downloadsDir);
  const containment = await safeSend(client, "Browser.setDownloadBehavior", {
    behavior: "allow",
    downloadPath: args.downloadsDir,
    eventsEnabled: true,
  });
  if (!controlFound) {
    return {
      status: "download-control-missing",
      controlFound,
      clicked: false,
      containment,
    };
  }
  if (!containment.ok) {
    return {
      status: "download-not-contained",
      controlFound,
      clicked: false,
      containment,
      notes:
        "DevTools download containment failed and no replacement was active.",
    };
  }
  const click = await clickControl(client, baseline, children, "download");
  await sleep(args.actionSettleMs * 2);
  const afterFiles = listFilesRecursive(args.downloadsDir);
  const newFiles = afterFiles.filter(
    (after) =>
      !beforeFiles.some(
        (before) =>
          before.relativePath === after.relativePath &&
          before.bytes === after.bytes,
      ),
  );
  const downloadEvents = client.events.filter((event) =>
    String(event.method || "").startsWith("Browser.download"),
  );
  let status = "download-no-op";
  if (newFiles.length > 0) {
    status = "download-file-created";
  } else if (downloadEvents.length > 0) {
    status = "download-browser-callback-only";
  } else if (!click.ok) {
    status = "download-control-not-activated";
  }
  return {
    status,
    controlFound,
    clicked: click.ok,
    containment,
    click,
    beforeFiles,
    afterFiles,
    newFiles,
    downloadEvents,
  };
}

function readPrintInterceptLines(args) {
  if (!args.printInterceptFile || !fs.existsSync(args.printInterceptFile)) {
    return [];
  }
  return fs
    .readFileSync(args.printInterceptFile, "utf8")
    .split(/\r?\n/)
    .filter(Boolean);
}

function readPrintBridgeTraceLines(args) {
  if (!args.printBridgeTraceFile || !fs.existsSync(args.printBridgeTraceFile)) {
    return [];
  }
  return fs
    .readFileSync(args.printBridgeTraceFile, "utf8")
    .split(/\r?\n/)
    .filter(Boolean);
}

function readNativePrintTraceLines(args) {
  if (!args.nativePrintTraceFile || !fs.existsSync(args.nativePrintTraceFile)) {
    return [];
  }
  return fs
    .readFileSync(args.nativePrintTraceFile, "utf8")
    .split(/\r?\n/)
    .filter(Boolean);
}

function parseTraceFields(line) {
  const fields = {};
  for (const part of line.split(/\s+/)) {
    const index = part.indexOf("=");
    if (index <= 0) {
      continue;
    }
    fields[part.slice(0, index)] = part.slice(index + 1);
  }
  return fields;
}

async function readJsPrintBridgeTrace(client, state, children) {
  const sessionId = bestSessionForPdf(state, children);
  const result = await evaluate(
    client,
    "window.__termsurfPdfPrintBridgeTrace || []",
    sessionId,
  );
  return result.ok && Array.isArray(result.value) ? result.value : [];
}

function classifyPrintBridge({
  rotateNativeLines,
  printNativeLines,
  printJsRecords,
  freshInterceptLines,
}) {
  const rotateReachedPlugin = rotateNativeLines.some(
    (line) =>
      line.includes(" type=rotateCounterclockwise ") &&
      line.includes(" event=on-message "),
  );
  if (!rotateReachedPlugin) {
    return "comparison-rotate-plugin-receipt-missing";
  }

  if (!printJsRecords.some((record) => record.event === "viewer-on-print")) {
    return "print-stops-before-viewer-handler";
  }
  if (!printJsRecords.some((record) => record.event === "controller-print")) {
    return "print-stops-before-controller-print";
  }
  if (
    !printJsRecords.some(
      (record) => record.event === "post-message" && record.type === "print",
    )
  ) {
    return "print-stops-before-post-message";
  }
  if (
    printJsRecords.some(
      (record) =>
        record.event === "post-message" &&
        record.type === "print" &&
        record.delayedMessagesActive,
    )
  ) {
    return "print-stops-in-delayed-message-queue";
  }
  if (
    !printNativeLines.some(
      (line) =>
        line.includes(" type=print ") && line.includes(" event=on-message "),
    )
  ) {
    return "print-posted-but-not-received-by-plugin";
  }
  if (
    !printNativeLines.some(
      (line) =>
        line.includes(" type=print ") && line.includes(" event=handle-print "),
    )
  ) {
    return "print-received-by-plugin-but-not-dispatched";
  }
  const guardEnterLines = printNativeLines.filter(
    (line) =>
      line.includes("pdf-print-guard ") && line.includes(" event=print-enter "),
  );
  const guardAppendFailed = printNativeLines.some(
    (line) =>
      line.includes("pdf-print-guard ") &&
      line.includes(" event=append-failed "),
  );
  const guardPathMismatch = printNativeLines.some(
    (line) =>
      line.includes("pdf-print-guard ") &&
      line.includes(" event=intercept-path-mismatch "),
  );
  if (guardAppendFailed) {
    return "append-failed";
  }
  if (guardPathMismatch) {
    return "intercept-path-mismatch";
  }
  if (guardEnterLines.length === 0) {
    return "print-not-entered";
  }
  const guardState = parseTraceFields(guardEnterLines.at(-1));
  if (guardState.intercept_path_present !== "1") {
    if (
      guardState.has_intercept_switch !== "1" &&
      guardState.env_intercept !== "1"
    ) {
      return "intercept-switch-missing";
    }
    if (
      guardState.has_intercept_file_switch !== "1" &&
      guardState.env_intercept_file_present !== "1"
    ) {
      return "intercept-file-switch-missing";
    }
    if (guardState.intercept_switch_file_empty === "1") {
      return "intercept-file-empty";
    }
    if (guardState.env_intercept !== "1") {
      return "env-intercept-missing";
    }
    if (guardState.env_intercept_file_present !== "1") {
      return "env-intercept-file-missing";
    }
  }
  if (guardState.intercept_path_expected === "0") {
    return "intercept-path-mismatch";
  }
  if (freshInterceptLines.length === 0) {
    return "intercept-present-but-log-missing";
  }
  return "print-reaches-contained-intercept";
}

async function probePrint(client, args, baseline, children) {
  const controls = flattenControls(baseline);
  const controlFound = hasControl(controls, "print");
  const loadTimePrintingEnabled = await probeLoadTimeBoolean(
    client,
    baseline,
    children,
    "printingEnabled",
  );
  const flags = stateValues(baseline)
    .map(({ value }) => value.loadTimeFlags)
    .filter(Boolean);
  const printingEnabled =
    loadTimePrintingEnabled.ok && loadTimePrintingEnabled.value === true;
  const printingDisabled =
    loadTimePrintingEnabled.ok && loadTimePrintingEnabled.value === false;
  const beforeInterceptLines = readPrintInterceptLines(args);
  if (!controlFound) {
    return {
      status: "print-control-missing",
      controlFound,
      clicked: false,
      flags,
      loadTimePrintingEnabled,
    };
  }
  if (printingDisabled) {
    return {
      status: "print-ready-disabled-by-flags",
      controlFound,
      clicked: false,
      flags,
      loadTimePrintingEnabled,
      notes:
        "printingEnabled is not true, so the toolbar print control was not clicked.",
    };
  }
  if (!printingEnabled) {
    return {
      status: "print-not-contained",
      controlFound,
      clicked: false,
      flags,
      loadTimePrintingEnabled,
      notes:
        "printingEnabled was not observable as true or false, and no non-native/intercepted toolbar print path was proven.",
    };
  }
  if (!args.printInterceptFile) {
    if (args.allowNativePrintClick) {
      const beforeNativePrintLines = readNativePrintTraceLines(args);
      const beforePrintBridgeJsRecords = await readJsPrintBridgeTrace(
        client,
        baseline,
        children,
      );
      const click = await clickControl(
        client,
        baseline,
        children,
        "print",
        "native-print-1",
      );
      await sleep(args.actionSettleMs);
      return {
        status: click.ok
          ? "print-native-click-sent"
          : "print-native-click-failed",
        controlFound,
        clicked: click.ok,
        flags,
        loadTimePrintingEnabled,
        bridgeTraceFile: args.printBridgeTraceFile || null,
        nativePrintTraceFile: args.nativePrintTraceFile || null,
        printNativeLines: readNativePrintTraceLines(args).slice(
          beforeNativePrintLines.length,
        ),
        printJsRecords: (
          await readJsPrintBridgeTrace(client, baseline, children)
        ).slice(beforePrintBridgeJsRecords.length),
        click,
        notes:
          "Production print was clicked only because --allow-native-print-click was provided by the safety-gated native print harness.",
      };
    }
    return {
      status: "print-production-available-not-clicked",
      controlFound,
      clicked: false,
      flags,
      loadTimePrintingEnabled,
      notes:
        "printingEnabled is true, and the print control was intentionally not clicked because no contained print intercept file was configured.",
    };
  }
  const bridgeNativeBefore = readPrintBridgeTraceLines(args);
  const bridgeJsBefore = await readJsPrintBridgeTrace(
    client,
    baseline,
    children,
  );
  let rotateClick = null;
  let rotateNativeLines = [];
  let rotateJsRecords = [];
  if (args.printBridgeTraceFile) {
    rotateClick = await clickControl(
      client,
      baseline,
      children,
      "rotateCounterclockwise",
      "rotate-1",
    );
    await sleep(args.actionSettleMs);
    rotateNativeLines = readPrintBridgeTraceLines(args).slice(
      bridgeNativeBefore.length,
    );
    rotateJsRecords = (
      await readJsPrintBridgeTrace(client, baseline, children)
    ).slice(bridgeJsBefore.length);
  }

  const beforePrintBridgeNativeLines = readPrintBridgeTraceLines(args);
  const beforePrintBridgeJsRecords = await readJsPrintBridgeTrace(
    client,
    baseline,
    children,
  );
  const click = await clickControl(
    client,
    baseline,
    children,
    "print",
    "print-1",
  );
  await sleep(args.actionSettleMs);
  const afterInterceptLines = readPrintInterceptLines(args);
  const freshLines = afterInterceptLines.slice(beforeInterceptLines.length);
  const printNativeLines = readPrintBridgeTraceLines(args).slice(
    beforePrintBridgeNativeLines.length,
  );
  const printJsRecords = (
    await readJsPrintBridgeTrace(client, baseline, children)
  ).slice(beforePrintBridgeJsRecords.length);
  const printGuardLines = printNativeLines.filter((line) =>
    line.includes("pdf-print-guard "),
  );
  const printGuardStateLine = printGuardLines
    .filter((line) => line.includes(" event=print-enter "))
    .at(-1);
  const printGuardState = printGuardStateLine
    ? parseTraceFields(printGuardStateLine)
    : null;
  const bridgeClassification = args.printBridgeTraceFile
    ? classifyPrintBridge({
        rotateNativeLines,
        printNativeLines,
        printJsRecords,
        freshInterceptLines: freshLines,
      })
    : null;
  if (click.ok && freshLines.length > 0) {
    return {
      status: "print-contained-callback",
      controlFound,
      clicked: true,
      flags,
      loadTimePrintingEnabled,
      printInterceptFile: args.printInterceptFile,
      beforeInterceptLines,
      afterInterceptLines,
      freshLines,
      bridgeTraceFile: args.printBridgeTraceFile || null,
      rotateClick,
      rotateNativeLines,
      rotateJsRecords,
      printNativeLines,
      printJsRecords,
      printGuardLines,
      printGuardState,
      bridgeClassification,
      click,
    };
  }
  return {
    status: "print-intercept-missing",
    controlFound,
    clicked: click.ok,
    flags,
    loadTimePrintingEnabled,
    printInterceptFile: args.printInterceptFile,
    beforeInterceptLines,
    afterInterceptLines,
    freshLines,
    bridgeTraceFile: args.printBridgeTraceFile || null,
    rotateClick,
    rotateNativeLines,
    rotateJsRecords,
    printNativeLines,
    printJsRecords,
    printGuardLines,
    printGuardState,
    bridgeClassification,
    click,
    notes:
      "printing appears enabled, but no fresh contained print intercept line was observed.",
  };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  fs.mkdirSync(args.outDir, { recursive: true });
  fs.mkdirSync(args.downloadsDir, { recursive: true });
  const summary = {
    devtoolsPort: args.devtoolsPort,
    urlContains: args.urlContains,
    outDir: args.outDir,
    downloadsDir: args.downloadsDir,
  };
  let client = null;
  try {
    const target = await pollTarget(args, args.urlContains);
    summary.selectedTarget = {
      id: target.id,
      type: target.type,
      url: target.url,
      title: target.title,
    };
    client = connectDevTools(target.webSocketDebuggerUrl);
    await client.open;
    for (const domain of [
      "Page",
      "Runtime",
      "DOM",
      "Input",
      "Target",
      "Browser",
    ]) {
      summary[`${domain}Enabled`] = await safeSend(client, `${domain}.enable`);
    }
    await safeSend(client, "Page.bringToFront");
    const children = await attachChildTargets(client);
    summary.childTargets = children;
    await sleep(args.settleSeconds * 1000);
    const baseline = await collectState(client, children);
    const baselineTopTargetInfo = await currentTopTargetTitle(args, target.id);
    const baselineTitleEvidence = readTitleTraceEvidence(args);
    summary.baseline = {
      screenshot: await captureScreenshot(client, args, "baseline.png"),
      titles: classifyTitle(
        baseline,
        baselineTopTargetInfo,
        baselineTitleEvidence,
      ),
      controlInventory: flattenControls(baseline),
      loadTimeFlags: stateValues(baseline).map(({ sessionId, value }) => ({
        sessionId,
        url: value.url,
        loadTimeFlags: value.loadTimeFlags,
        api: value.api,
      })),
    };
    summary.baseline.placeholders = placeholderDiagnostics(
      summary.baseline.controlInventory,
    );
    summary.baseline.thumbnailPageAriaLabel = await probeLoadTimeString(
      client,
      baseline,
      children,
      "thumbnailPageAriaLabel",
    );
    writeJson(
      path.join(args.outDir, "control-inventory.json"),
      summary.baseline.controlInventory,
    );
    summary.saveDownload = await probeSave(client, args, baseline, children);
    summary.print = await probePrint(client, args, baseline, children);

    const urlCases = [
      ["http-pdf", args.httpPdfUrl],
      ["file-pdf", args.filePdfUrl],
      ["http-extensionless", args.httpExtensionlessUrl],
      ["file-extensionless", args.fileExtensionlessUrl],
      ["http-untitled", args.httpUntitledUrl],
      ["file-untitled", args.fileUntitledUrl],
    ].filter(([, url]) => url);
    summary.localParity = [];
    for (const [label, url] of urlCases) {
      summary.localParity.push(
        await runUrlCase(client, args, target.id, label, url, children),
      );
    }
    summary.embeddedTitle = await probeEmbeddedTitle(
      client,
      args,
      target.id,
      args.embeddedHtmlUrl,
    );
    summary.consoleMissingStringErrors = missingStringErrors(client.events);
    summary.localParityDiagnostics = {
      scrollChanged: summary.localParity.every((item) => item.scroll.changed),
      pageNavigationChanged: summary.localParity.every(
        (item) => item.pageNavigation.changed,
      ),
      note: "Local parity scroll/page navigation are diagnostics here. Dedicated protocol and toolbar-event harnesses own authoritative scroll/page-navigation coverage.",
    };
    const localPass = summary.localParity.every(
      (item) => item.render && item.zoom.changed,
    );
    const titlePass =
      summary.baseline.titles.classification === "title-propagated" &&
      summary.localParity.every(
        (item) => item.title.classification === "title-propagated",
      ) &&
      summary.embeddedTitle.status === "pass";
    summary.titlePropagationPass = titlePass;
    const printPass =
      summary.print.status === "print-contained-callback" ||
      summary.print.status === "print-production-available-not-clicked" ||
      summary.print.status === "print-ready-disabled-by-flags" ||
      summary.print.status === "print-restricted-by-document";
    summary.status =
      localPass &&
      titlePass &&
      printPass &&
      summary.saveDownload.status === "download-file-created" &&
      !["download-native-dialog", "print-native-dialog"].includes(
        summary.saveDownload.status,
      ) &&
      summary.print.status !== "print-native-dialog"
        ? "pass"
        : "partial";
  } catch (error) {
    summary.status = "error";
    summary.error = String(error.stack || error);
    throw error;
  } finally {
    writeJson(
      path.join(args.outDir, "save-print-title-local-summary.json"),
      summary,
    );
    if (client) {
      client.socket.close();
    }
  }
}

main().catch((error) => {
  console.error(error.stack || error);
  process.exit(1);
});
