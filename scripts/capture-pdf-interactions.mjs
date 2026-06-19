#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";

const PDF_EXTENSION_ID = "mhjfbmdgcfjbbpaeojofohoefgiehjai";

function parseArgs(argv) {
  const args = {
    timeoutSeconds: 30,
    settleSeconds: 8,
    inputSettleMs: 350,
    resizeSettleMs: 500,
    mode: "full",
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

  for (const key of ["devtoolsPort", "urlContains", "outDir"]) {
    if (!args[key]) {
      throw new Error(
        `missing required --${key.replace(/[A-Z]/g, (ch) => `-${ch.toLowerCase()}`)}`,
      );
    }
  }

  args.devtoolsPort = Number(args.devtoolsPort);
  args.timeoutSeconds = Number(args.timeoutSeconds);
  args.settleSeconds = Number(args.settleSeconds);
  args.inputSettleMs = Number(args.inputSettleMs);
  args.resizeSettleMs = Number(args.resizeSettleMs);

  if (!Number.isFinite(args.devtoolsPort) || args.devtoolsPort <= 0) {
    throw new Error(`invalid --devtools-port: ${args.devtoolsPort}`);
  }
  if (!Number.isFinite(args.timeoutSeconds) || args.timeoutSeconds <= 0) {
    throw new Error(`invalid --timeout-seconds: ${args.timeoutSeconds}`);
  }
  if (!Number.isFinite(args.settleSeconds) || args.settleSeconds < 0) {
    throw new Error(`invalid --settle-seconds: ${args.settleSeconds}`);
  }
  if (!Number.isFinite(args.inputSettleMs) || args.inputSettleMs < 0) {
    throw new Error(`invalid --input-settle-ms: ${args.inputSettleMs}`);
  }
  if (!Number.isFinite(args.resizeSettleMs) || args.resizeSettleMs < 0) {
    throw new Error(`invalid --resize-settle-ms: ${args.resizeSettleMs}`);
  }
  if (!["probe", "full"].includes(args.mode)) {
    throw new Error(`invalid --mode: ${args.mode}`);
  }

  args.outDir = path.resolve(args.outDir);
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

async function pollTarget(args, summary) {
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

  summary.availableTargets = lastTargets.map((target) => ({
    id: target.id,
    type: target.type,
    url: target.url,
    title: target.title,
  }));
  if (lastError) {
    summary.lastTargetPollError = String(lastError.stack || lastError);
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

function writeJson(filePath, value) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`);
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
    {
      expression,
      awaitPromise: true,
      returnByValue: true,
    },
    sessionId,
  );
  if (!result.ok) {
    return { ok: false, error: result.error };
  }
  if (result.result.exceptionDetails) {
    return {
      ok: false,
      error: JSON.stringify(result.result.exceptionDetails),
    };
  }
  return {
    ok: true,
    value: result.result.result?.value ?? null,
  };
}

const STATE_EXPRESSION = `(() => {
  const scrolling = document.scrollingElement || document.documentElement || document.body;
  const rectOf = (el) => {
    const rect = el.getBoundingClientRect();
    return {
      tag: el.tagName,
      id: el.id || "",
      className: String(el.className || ""),
      type: el.getAttribute("type") || "",
      src: el.getAttribute("src") || "",
      width: rect.width,
      height: rect.height,
      x: rect.x,
      y: rect.y,
      top: rect.top,
      left: rect.left,
      right: rect.right,
      bottom: rect.bottom,
    };
  };
  const query = [
    "embed",
    "iframe",
    "canvas",
    "pdf-viewer",
    "viewer-toolbar",
    "viewer-page-indicator",
    "viewer-zoom-toolbar",
    "viewer-page-selector",
    "viewer-download-controls",
    "cr-icon-button",
    "#viewer",
    "#plugin",
    "#sizer",
    "#container",
    "#page-container",
    "#click-target",
    "#selection-target",
    ".page",
    ".page-container",
    "body",
    "main",
    "section",
    "article",
    "button",
    "input",
    "p",
  ].join(",");
  const elements = [];
  const visit = (root, depth = 0) => {
    if (!root || depth > 8 || elements.length > 160) {
      return;
    }
    for (const el of root.querySelectorAll(query)) {
      elements.push(rectOf(el));
      if (elements.length > 160) {
        return;
      }
    }
    for (const el of root.querySelectorAll("*")) {
      if (el.shadowRoot) {
        visit(el.shadowRoot, depth + 1);
      }
      if (elements.length > 160) {
        return;
      }
    }
  };
  visit(document);
  return {
    url: location.href,
    title: document.title,
    activeElement: document.activeElement ? {
      tag: document.activeElement.tagName,
      id: document.activeElement.id || "",
      className: String(document.activeElement.className || ""),
    } : null,
    viewport: {
      innerWidth,
      innerHeight,
      devicePixelRatio,
    },
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
    selection: String(getSelection ? getSelection() : ""),
    bodyTextSample: document.body ? document.body.innerText.slice(0, 1000) : "",
    elements,
  };
})()`;

function pickInteractiveRect(state) {
  const elements = allElements(state);
  const candidates = elements.filter(
    (el) =>
      el.width > 100 &&
      el.height > 100 &&
      ["EMBED", "IFRAME", "PDF-VIEWER"].includes(el.tag),
  );
  if (candidates.length > 0) {
    return candidates.sort(
      (a, b) => b.width * b.height - a.width * a.height,
    )[0];
  }
  const viewport = state?.value?.viewport ||
    state?.viewport || {
      innerWidth: 1024,
      innerHeight: 768,
    };
  return {
    x: 0,
    y: 0,
    left: 0,
    top: 0,
    right: viewport.innerWidth,
    bottom: viewport.innerHeight,
    width: viewport.innerWidth,
    height: viewport.innerHeight,
  };
}

function centerOf(rect) {
  return {
    x: Math.max(1, Math.round(rect.left + rect.width / 2)),
    y: Math.max(1, Math.round(rect.top + rect.height / 2)),
  };
}

function findElementById(state, id) {
  const elements = allElements(state);
  return elements.find((el) => el.id === id) || null;
}

function allElements(state) {
  const top = state?.value?.elements || state?.elements || [];
  const child = (state?.childStates || []).flatMap(
    (childState) => childState.state?.value?.elements || [],
  );
  return [...top, ...child];
}

function pickClickRect(state, interactiveRect) {
  return findElementById(state, "click-target") || interactiveRect;
}

function pickDragRect(state, interactiveRect) {
  return findElementById(state, "selection-target") || interactiveRect;
}

function dragPoints(rect) {
  const x1 = Math.max(
    5,
    Math.round(rect.left + Math.min(80, rect.width * 0.12)),
  );
  const x2 = Math.max(x1 + 20, Math.round(rect.left + rect.width * 0.85));
  const y1 = Math.max(
    5,
    Math.round(rect.top + Math.min(180, rect.height * 0.22)),
  );
  const y2 = Math.max(
    y1 + 20,
    Math.round(rect.top + Math.min(360, rect.height * 0.42)),
  );
  return { x1, y1, x2, y2 };
}

async function captureScreenshot(client, args, relativePath) {
  const result = await client.send("Page.captureScreenshot", {
    format: "png",
    fromSurface: true,
  });
  if (!result.data) {
    throw new Error("Page.captureScreenshot returned no data");
  }
  const png = Buffer.from(result.data, "base64");
  const filePath = path.join(args.outDir, relativePath);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, png);
  return { relativePath, bytes: png.length, base64: result.data };
}

function screenshotDiff(before, after) {
  return {
    beforeBytes: before.bytes,
    afterBytes: after.bytes,
    bytesChanged: before.bytes !== after.bytes,
    contentChanged: before.base64 !== after.base64,
  };
}

function stateDiff(before, after) {
  const b = before?.value || {};
  const a = after?.value || {};
  const bLargest = largestElement(allElements(before));
  const aLargest = largestElement(allElements(after));
  return {
    scrollTopDelta: (a.scroll?.top ?? 0) - (b.scroll?.top ?? 0),
    scrollYDelta: (a.scroll?.y ?? 0) - (b.scroll?.y ?? 0),
    innerWidthDelta:
      (a.viewport?.innerWidth ?? 0) - (b.viewport?.innerWidth ?? 0),
    innerHeightDelta:
      (a.viewport?.innerHeight ?? 0) - (b.viewport?.innerHeight ?? 0),
    largestElementWidthDelta: (aLargest?.width ?? 0) - (bLargest?.width ?? 0),
    largestElementHeightDelta:
      (aLargest?.height ?? 0) - (bLargest?.height ?? 0),
    selectionChanged: (a.selection || "") !== (b.selection || ""),
    selectionLength: (a.selection || "").length,
  };
}

function largestElement(elements) {
  if (!elements.length) {
    return null;
  }
  return [...elements].sort(
    (a, b) => b.width * b.height - a.width * a.height,
  )[0];
}

function resultStatusFromChange(diff) {
  if (
    diff.scrollTopDelta !== 0 ||
    diff.scrollYDelta !== 0 ||
    diff.selectionChanged ||
    diff.innerWidthDelta !== 0 ||
    diff.innerHeightDelta !== 0
  ) {
    return "pass";
  }
  return "inconclusive";
}

async function writeResult(args, check, status, evidence, notes = "") {
  const result = { check, status, evidence, notes };
  writeJson(path.join(args.outDir, `${check}.json`), result);
  return result;
}

async function snapshot(client, args, name, childSessions = []) {
  const state = await evaluate(client, STATE_EXPRESSION);
  state.childStates = [];
  for (const child of childSessions) {
    state.childStates.push({
      sessionId: child.sessionId,
      targetInfo: child.targetInfo,
      state: await evaluate(client, STATE_EXPRESSION, child.sessionId),
    });
  }
  const screenshot = await captureScreenshot(client, args, `${name}.png`);
  return { state, screenshot };
}

async function resetScroll(client) {
  await evaluate(
    client,
    `(() => {
      window.scrollTo(0, 0);
      const scrolling = document.scrollingElement || document.documentElement || document.body;
      if (scrolling) {
        scrolling.scrollTop = 0;
        scrolling.scrollLeft = 0;
      }
      return true;
    })()`,
  );
  await sleep(100);
}

async function dispatchMouse(client, type, x, y, params = {}) {
  return client.send("Input.dispatchMouseEvent", {
    type,
    x,
    y,
    button: params.button ?? "left",
    buttons: params.buttons ?? (type === "mouseReleased" ? 0 : 1),
    clickCount: params.clickCount ?? 1,
    deltaX: params.deltaX,
    deltaY: params.deltaY,
  });
}

async function dispatchKey(client, key) {
  const common = {
    key,
    code: key,
    windowsVirtualKeyCode: key === " " ? 32 : 34,
    nativeVirtualKeyCode: key === " " ? 32 : 34,
  };
  await client.send("Input.dispatchKeyEvent", {
    type: "keyDown",
    ...common,
  });
  await client.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    ...common,
  });
}

async function copySelection(client) {
  await client.send("Input.dispatchKeyEvent", {
    type: "keyDown",
    key: "Meta",
    code: "MetaLeft",
    windowsVirtualKeyCode: 91,
    nativeVirtualKeyCode: 91,
    modifiers: 4,
  });
  await client.send("Input.dispatchKeyEvent", {
    type: "keyDown",
    key: "c",
    code: "KeyC",
    windowsVirtualKeyCode: 67,
    nativeVirtualKeyCode: 67,
    modifiers: 4,
  });
  await client.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: "c",
    code: "KeyC",
    windowsVirtualKeyCode: 67,
    nativeVirtualKeyCode: 67,
    modifiers: 4,
  });
  await client.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: "Meta",
    code: "MetaLeft",
    windowsVirtualKeyCode: 91,
    nativeVirtualKeyCode: 91,
  });
}

async function runChecks(client, args, summary) {
  const results = [];
  const childSessions = (summary.attachAttempts || [])
    .filter((attempt) => attempt.state?.ok)
    .map((attempt) => ({
      sessionId: attempt.sessionId,
      targetInfo: attempt.targetInfo,
    }));
  const baseline = await snapshot(client, args, "baseline", childSessions);
  writeJson(path.join(args.outDir, "baseline.json"), {
    selectedTarget: summary.selectedTarget,
    state: baseline.state,
    screenshot: {
      path: baseline.screenshot.relativePath,
      bytes: baseline.screenshot.bytes,
    },
    childTargets: summary.childTargets || [],
    attachAttempts: summary.attachAttempts || [],
  });

  results.push(
    await writeResult(
      args,
      "render",
      baseline.screenshot.bytes > 1000 ? "pass" : "fail",
      {
        before: {},
        after: {
          screenshotBytes: baseline.screenshot.bytes,
        },
        diff: {},
        screenshots: [baseline.screenshot.relativePath],
      },
      baseline.screenshot.bytes > 1000
        ? "screenshot is non-empty"
        : "screenshot is too small to be credible",
    ),
  );

  if (args.mode === "probe") {
    return results;
  }

  const rect = pickInteractiveRect(baseline.state);
  const clickRect = pickClickRect(baseline.state, rect);
  const center = centerOf(clickRect);

  {
    const before = await snapshot(client, args, "wheel-before", childSessions);
    await client.send("Input.dispatchMouseEvent", {
      type: "mouseWheel",
      x: center.x,
      y: center.y,
      deltaX: 0,
      deltaY: 600,
    });
    await sleep(args.inputSettleMs);
    const after = await snapshot(client, args, "wheel-after", childSessions);
    const diff = {
      ...stateDiff(before.state, after.state),
      screenshot: screenshotDiff(before.screenshot, after.screenshot),
    };
    const changed =
      diff.scrollTopDelta !== 0 ||
      diff.scrollYDelta !== 0 ||
      diff.screenshot.contentChanged;
    results.push(
      await writeResult(
        args,
        "wheel-scroll",
        changed ? "pass" : "fail",
        {
          before: before.state,
          after: after.state,
          diff,
          screenshots: [
            before.screenshot.relativePath,
            after.screenshot.relativePath,
          ],
        },
        changed
          ? "wheel input changed scroll state or screenshot"
          : "wheel input produced no observable change",
      ),
    );
  }

  {
    const before = await snapshot(
      client,
      args,
      "keyboard-before",
      childSessions,
    );
    await dispatchKey(client, "PageDown");
    await sleep(args.inputSettleMs);
    const after = await snapshot(client, args, "keyboard-after", childSessions);
    const diff = {
      ...stateDiff(before.state, after.state),
      screenshot: screenshotDiff(before.screenshot, after.screenshot),
    };
    const changed =
      diff.scrollTopDelta !== 0 ||
      diff.scrollYDelta !== 0 ||
      diff.screenshot.contentChanged;
    results.push(
      await writeResult(
        args,
        "keyboard-scroll",
        changed ? "pass" : "fail",
        {
          before: before.state,
          after: after.state,
          diff,
          screenshots: [
            before.screenshot.relativePath,
            after.screenshot.relativePath,
          ],
        },
        changed
          ? "keyboard input changed scroll state or screenshot"
          : "keyboard input produced no observable change",
      ),
    );
  }

  {
    await resetScroll(client);
    const before = await snapshot(client, args, "click-before", childSessions);
    const currentRect = pickInteractiveRect(before.state);
    const currentClickRect = pickClickRect(before.state, currentRect);
    const currentCenter = centerOf(currentClickRect);
    await dispatchMouse(
      client,
      "mousePressed",
      currentCenter.x,
      currentCenter.y,
    );
    await dispatchMouse(
      client,
      "mouseReleased",
      currentCenter.x,
      currentCenter.y,
    );
    await sleep(100);
    const after = await snapshot(client, args, "click-after", childSessions);
    const diff = stateDiff(before.state, after.state);
    const focused =
      JSON.stringify(before.state.value?.activeElement || null) !==
        JSON.stringify(after.state.value?.activeElement || null) ||
      (before.state.value?.bodyTextSample || "") !==
        (after.state.value?.bodyTextSample || "");
    results.push(
      await writeResult(
        args,
        "click-focus",
        focused ? "pass" : "inconclusive",
        {
          before: before.state,
          after: after.state,
          diff: { ...diff, activeElementChanged: focused },
          screenshots: [
            before.screenshot.relativePath,
            after.screenshot.relativePath,
          ],
        },
        focused
          ? "click changed active element state"
          : "click produced no top-level active element change",
      ),
    );
  }

  {
    await resetScroll(client);
    const before = await snapshot(client, args, "drag-before", childSessions);
    const currentRect = pickInteractiveRect(before.state);
    const currentDragRect = pickDragRect(before.state, currentRect);
    const points = dragPoints(currentDragRect);
    await dispatchMouse(client, "mousePressed", points.x1, points.y1);
    const steps = 8;
    for (let i = 1; i <= steps; i += 1) {
      const t = i / steps;
      await dispatchMouse(
        client,
        "mouseMoved",
        Math.round(points.x1 + (points.x2 - points.x1) * t),
        Math.round(points.y1 + (points.y2 - points.y1) * t),
        { clickCount: 0 },
      );
      await sleep(20);
    }
    await dispatchMouse(client, "mouseReleased", points.x2, points.y2);
    await sleep(args.inputSettleMs);
    const grant = await safeSend(client, "Browser.grantPermissions", {
      permissions: ["clipboardReadWrite", "clipboardSanitizedWrite"],
    });
    let clipboardRead = { ok: false, error: "not attempted" };
    if (grant.ok) {
      await copySelection(client);
      await sleep(100);
      clipboardRead = await evaluate(
        client,
        `navigator.clipboard.readText().then((text) => ({ text, length: text.length }))`,
      );
    }
    const after = await snapshot(client, args, "drag-after", childSessions);
    const diff = {
      ...stateDiff(before.state, after.state),
      screenshot: screenshotDiff(before.screenshot, after.screenshot),
      dragPoints: points,
      clipboardGrant: grant,
      clipboardRead,
    };
    const selected =
      diff.selectionLength > 0 ||
      (clipboardRead.ok && (clipboardRead.value?.length || 0) > 0);
    results.push(
      await writeResult(
        args,
        "drag-select",
        selected ? "pass" : grant.ok ? "fail" : "unsupported-by-harness",
        {
          before: before.state,
          after: after.state,
          diff,
          screenshots: [
            before.screenshot.relativePath,
            after.screenshot.relativePath,
          ],
        },
        selected
          ? "drag produced selected or copied text"
          : grant.ok
            ? "drag/copy produced no selected text"
            : "clipboard permission could not be granted",
      ),
    );
  }

  {
    const before = await snapshot(client, args, "resize-before", childSessions);
    const viewport = before.state.value?.viewport || {
      innerWidth: 1024,
      innerHeight: 768,
      devicePixelRatio: 1,
    };
    const resize = await safeSend(
      client,
      "Emulation.setDeviceMetricsOverride",
      {
        width: Math.max(600, Math.round(viewport.innerWidth * 0.75)),
        height: Math.max(500, Math.round(viewport.innerHeight * 0.75)),
        deviceScaleFactor: viewport.devicePixelRatio || 1,
        mobile: false,
      },
    );
    await sleep(args.resizeSettleMs);
    const after = await snapshot(client, args, "resize-after", childSessions);
    await safeSend(client, "Emulation.clearDeviceMetricsOverride");
    const diff = {
      ...stateDiff(before.state, after.state),
      screenshot: screenshotDiff(before.screenshot, after.screenshot),
      resize,
    };
    results.push(
      await writeResult(
        args,
        "resize-state",
        resultStatusFromChange(diff),
        {
          before: before.state,
          after: after.state,
          diff,
          screenshots: [
            before.screenshot.relativePath,
            after.screenshot.relativePath,
          ],
        },
        resize.ok
          ? "DevTools viewport resize was applied"
          : "DevTools viewport resize failed",
      ),
    );
  }

  {
    const collectControlsExpression = `(() => {
      const controls = [];
      const query = "button,input,select,viewer-toolbar,viewer-page-selector,viewer-zoom-toolbar,cr-icon-button,*[role=button]";
      const rectOf = (el) => {
        const rect = el.getBoundingClientRect();
        return {
          tag: el.tagName,
          id: el.id || "",
          className: String(el.className || ""),
          role: el.getAttribute("role") || "",
          ariaLabel: el.getAttribute("aria-label") || "",
          title: el.getAttribute("title") || "",
          text: (el.innerText || el.value || "").slice(0, 80),
          width: rect.width,
          height: rect.height,
          x: rect.x,
          y: rect.y
        };
      };
      const visit = (root, depth = 0) => {
        if (!root || depth > 8 || controls.length > 160) {
          return;
        }
        for (const el of root.querySelectorAll(query)) {
          controls.push(rectOf(el));
          if (controls.length > 160) {
            return;
          }
        }
        for (const el of root.querySelectorAll("*")) {
          if (el.shadowRoot) {
            visit(el.shadowRoot, depth + 1);
          }
          if (controls.length > 160) {
            return;
          }
        }
      };
      visit(document);
      return controls;
    })()`;
    const state = await evaluate(client, collectControlsExpression);
    const childControlStates = [];
    for (const child of childSessions) {
      childControlStates.push({
        sessionId: child.sessionId,
        targetInfo: child.targetInfo,
        state: await evaluate(
          client,
          collectControlsExpression,
          child.sessionId,
        ),
      });
    }
    const controlCount =
      (state.value?.length || 0) +
      childControlStates.reduce(
        (total, child) => total + (child.state.value?.length || 0),
        0,
      );
    results.push(
      await writeResult(
        args,
        "toolbar-probe",
        controlCount > 0 ? "pass" : "unsupported-by-harness",
        {
          before: {},
          after: { state, childControlStates },
          diff: { controlCount },
          screenshots: [baseline.screenshot.relativePath],
        },
        "toolbar controls are only detected, not clicked",
      ),
    );
  }

  {
    const logText =
      args.guiLog && fs.existsSync(args.guiLog)
        ? fs.readFileSync(args.guiLog, "utf8")
        : "";
    const titleLines = logText
      .split("\n")
      .filter((line) => /title|Title|TitleChanged/.test(line))
      .slice(-20);
    results.push(
      await writeResult(
        args,
        "title-probe",
        baseline.state.value?.title ? "pass" : "inconclusive",
        {
          before: {},
          after: {
            targetTitle: baseline.state.value?.title || "",
            titleLines,
          },
          diff: {},
          screenshots: [baseline.screenshot.relativePath],
        },
        titleLines.length > 0
          ? "found title-like lines in GUI log"
          : "no title-like GUI log lines found",
      ),
    );
  }

  return results;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  fs.mkdirSync(args.outDir, { recursive: true });
  const summary = {
    devtoolsPort: args.devtoolsPort,
    urlContains: args.urlContains,
    outDir: args.outDir,
    timeoutSeconds: args.timeoutSeconds,
    settleSeconds: args.settleSeconds,
    inputSettleMs: args.inputSettleMs,
    resizeSettleMs: args.resizeSettleMs,
    mode: args.mode,
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
    for (const domain of ["Page", "Runtime", "DOM", "Input", "Target"]) {
      const enabled = await safeSend(client, `${domain}.enable`);
      summary[`${domain}Enabled`] = enabled.ok ? true : enabled.error;
    }
    summary.browserVersion = await safeSend(client, "Browser.getVersion");
    await safeSend(client, "Target.setAutoAttach", {
      autoAttach: true,
      waitForDebuggerOnStart: false,
      flatten: true,
    });
    await safeSend(client, "Page.bringToFront");
    await sleep(args.settleSeconds * 1000);

    summary.frameTree = await safeSend(client, "Page.getFrameTree");
    summary.childTargets = client.events
      .filter((event) => event.method === "Target.attachedToTarget")
      .map((event) => ({
        sessionId: event.params.sessionId,
        targetInfo: event.params.targetInfo,
      }));
    summary.attachAttempts = [];
    for (const child of summary.childTargets) {
      const sessionId = child.sessionId;
      const runtime = await safeSend(client, "Runtime.enable", {}, sessionId);
      const page = await safeSend(client, "Page.enable", {}, sessionId);
      const state = await evaluate(client, STATE_EXPRESSION, sessionId);
      summary.attachAttempts.push({
        sessionId,
        targetInfo: child.targetInfo,
        runtime,
        page,
        state,
        isPdfExtension:
          child.targetInfo?.url?.includes(PDF_EXTENSION_ID) ||
          state.value?.url?.includes(PDF_EXTENSION_ID),
      });
    }

    summary.results = await runChecks(client, args, summary);
    summary.status = "ok";
  } catch (error) {
    summary.status = "error";
    summary.error = String(error.stack || error);
    throw error;
  } finally {
    writeJson(path.join(args.outDir, "summary.json"), summary);
    if (client) {
      client.socket.close();
    }
  }
}

main().catch((error) => {
  console.error(error.stack || error);
  process.exit(1);
});
