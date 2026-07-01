const args = new Map<string, string>();
for (let i = 0; i < Deno.args.length; i += 1) {
  const arg = Deno.args[i];
  if (!arg.startsWith("--")) continue;
  const key = arg.slice(2);
  const value = Deno.args[i + 1];
  if (value && !value.startsWith("--")) {
    args.set(key, value);
    i += 1;
  } else {
    args.set(key, "1");
  }
}

const hostname = args.get("host") ?? "127.0.0.1";
const port = Number(args.get("port") ?? "0");

if (!Number.isInteger(port) || port <= 0) {
  console.error("TermSurf server requires --port <port>");
  Deno.exit(1);
}

const html = `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>TermSurf</title>
  <style>
    :root {
      color-scheme: dark;
      font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: #111418;
      color: #edf0f2;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      min-height: 100vh;
      background: #111418;
    }
    main {
      display: grid;
      grid-template-columns: 220px minmax(0, 1fr);
      min-height: 100vh;
    }
    aside {
      border-right: 1px solid #2e353d;
      background: #171b21;
      padding: 16px;
      overflow: auto;
    }
    section {
      padding: 18px;
      overflow: auto;
    }
    .panel { display: none; }
    .panel.active { display: block; }
    h1 {
      margin: 0 0 8px;
      font-size: 22px;
      font-weight: 650;
    }
    h2 {
      margin: 0 0 10px;
      font-size: 18px;
      font-weight: 650;
    }
    p {
      color: #c6ced6;
      line-height: 1.5;
      margin: 0 0 14px;
      max-width: 740px;
    }
    .warning {
      margin: 0 0 16px;
      color: #ffcc7a;
      font-size: 13px;
      line-height: 1.4;
    }
    .nav {
      display: grid;
      gap: 8px;
      margin-top: 18px;
    }
    .toolbar {
      display: flex;
      gap: 8px;
      margin-bottom: 12px;
    }
    button, input {
      border: 1px solid #3a424c;
      border-radius: 6px;
      background: #20262d;
      color: #edf0f2;
      font: inherit;
      min-height: 34px;
    }
    button {
      padding: 0 12px;
      cursor: pointer;
    }
    button:hover { background: #2a313a; }
    button.nav-item {
      text-align: left;
      width: 100%;
    }
    button.nav-item.active {
      background: #2c6e6a;
      border-color: #3a8e88;
    }
    button.primary {
      background: #2c6e6a;
      border-color: #3a8e88;
    }
    button.danger {
      background: #553036;
      border-color: #79434c;
    }
    input {
      width: 100%;
      padding: 0 10px;
    }
    label {
      display: grid;
      gap: 6px;
      margin-bottom: 12px;
      color: #aab4bf;
      font-size: 13px;
    }
    .list {
      display: grid;
      gap: 8px;
      margin-top: 12px;
    }
    .item {
      width: 100%;
      text-align: left;
      padding: 10px;
      min-height: 58px;
    }
    .item strong {
      display: block;
      color: #ffffff;
      font-size: 14px;
    }
    .item span {
      display: block;
      color: #aab4bf;
      font-size: 12px;
      margin-top: 3px;
    }
    .empty {
      color: #8f9ba6;
      border: 1px dashed #3a424c;
      border-radius: 8px;
      padding: 16px;
    }
    form {
      max-width: 560px;
    }
    .home-grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
      gap: 10px;
      max-width: 860px;
      margin-top: 18px;
    }
    .feature {
      border: 1px solid #313943;
      border-radius: 8px;
      background: #171b21;
      padding: 12px;
    }
    .feature strong {
      display: block;
      margin-bottom: 6px;
    }
    .feature span {
      color: #aab4bf;
      font-size: 13px;
      line-height: 1.4;
    }
    .password-layout {
      display: grid;
      grid-template-columns: minmax(240px, 340px) minmax(0, 1fr);
      gap: 18px;
      max-width: 980px;
    }
    .actions {
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
    }
    @media (max-width: 760px) {
      main { grid-template-columns: 1fr; }
      aside { border-right: 0; border-bottom: 1px solid #2e353d; }
      .password-layout { grid-template-columns: 1fr; }
    }
  </style>
</head>
<body>
  <main>
    <aside>
      <h1>TermSurf</h1>
      <p class="warning">Alpha prototype. The password demo is in-memory only and is not encrypted, synced, or saved.</p>
      <nav class="nav" aria-label="TermSurf sections">
        <button class="nav-item active" type="button" data-panel="home">Home</button>
        <button class="nav-item" type="button" data-panel="passwords">Passwords PoC</button>
      </nav>
    </aside>
    <section class="panel active" id="panel-home">
      <h2>TermSurf</h2>
      <p>This alpha app proves that TermSurf can host a full graphical terminal user interface backed by a local Deno server.</p>
      <p>It will grow into the built-in place for TermSurf help, bookmarks, passwords, sync, and account features. For now, it ships only a fake password manager to validate the app architecture.</p>
      <div class="home-grid">
        <div class="feature"><strong>Help</strong><span>Context for using TermSurf without leaving the terminal.</span></div>
        <div class="feature"><strong>Bookmarks</strong><span>Future local-first browser bookmarks and sync.</span></div>
        <div class="feature"><strong>Passwords</strong><span>Future secure vault integration. The current demo is not secure.</span></div>
        <div class="feature"><strong>Sync</strong><span>Future configuration and account synchronization across machines.</span></div>
      </div>
    </section>
    <section class="panel" id="panel-passwords">
      <h2>Passwords PoC</h2>
      <p class="warning">Demo only. Records live in memory and are intentionally not real secrets.</p>
      <div class="password-layout">
        <div>
          <div class="toolbar">
            <button class="primary" id="new">New</button>
          </div>
          <div class="list" id="list"></div>
        </div>
        <form id="form">
          <label>Site
            <input id="site" autocomplete="off" placeholder="example.com">
          </label>
          <label>Username
            <input id="username" autocomplete="off" placeholder="user@example.com">
          </label>
          <label>Password
            <input id="password" autocomplete="off" placeholder="fake-password-for-poc">
          </label>
          <div class="actions">
            <button class="primary" type="submit">Save</button>
            <button class="danger" type="button" id="delete">Delete</button>
          </div>
        </form>
      </div>
    </section>
  </main>
  <script>
    let records = [
      { id: crypto.randomUUID(), site: "example.com", username: "demo", password: "not-a-real-secret" }
    ];
    let selected = records[0]?.id ?? null;
    const list = document.getElementById("list");
    const form = document.getElementById("form");
    const site = document.getElementById("site");
    const username = document.getElementById("username");
    const password = document.getElementById("password");
    const del = document.getElementById("delete");
    const navItems = Array.from(document.querySelectorAll(".nav-item"));
    const panels = Array.from(document.querySelectorAll(".panel"));

    for (const item of navItems) {
      item.addEventListener("click", () => {
        const target = item.dataset.panel;
        for (const navItem of navItems) navItem.classList.toggle("active", navItem === item);
        for (const panel of panels) panel.classList.toggle("active", panel.id === "panel-" + target);
      });
    }

    function current() {
      return records.find((record) => record.id === selected) ?? null;
    }

    function renderList() {
      list.innerHTML = "";
      if (records.length === 0) {
        const empty = document.createElement("div");
        empty.className = "empty";
        empty.textContent = "No fake password records yet.";
        list.append(empty);
        return;
      }
      for (const record of records) {
        const item = document.createElement("button");
        item.className = "item";
        item.type = "button";
        item.innerHTML = "<strong></strong><span></span>";
        item.querySelector("strong").textContent = record.site || "Untitled";
        item.querySelector("span").textContent = record.username || "No username";
        item.addEventListener("click", () => {
          selected = record.id;
          renderForm();
        });
        list.append(item);
      }
    }

    function renderForm() {
      const record = current();
      site.value = record?.site ?? "";
      username.value = record?.username ?? "";
      password.value = record?.password ?? "";
      del.disabled = !record;
      renderList();
    }

    document.getElementById("new").addEventListener("click", () => {
      const record = { id: crypto.randomUUID(), site: "", username: "", password: "" };
      records.unshift(record);
      selected = record.id;
      renderForm();
      site.focus();
    });

    del.addEventListener("click", () => {
      if (!selected) return;
      records = records.filter((record) => record.id !== selected);
      selected = records[0]?.id ?? null;
      renderForm();
    });

    form.addEventListener("submit", (event) => {
      event.preventDefault();
      let record = current();
      if (!record) {
        record = { id: crypto.randomUUID(), site: "", username: "", password: "" };
        records.unshift(record);
        selected = record.id;
      }
      record.site = site.value;
      record.username = username.value;
      record.password = password.value;
      renderForm();
    });

    renderForm();
  </script>
</body>
</html>`;

const abortController = new AbortController();

for (const signal of ["SIGTERM", "SIGINT"] as const) {
  Deno.addSignalListener(signal, () => {
    abortController.abort();
    Deno.exit(0);
  });
}

Deno.serve({ hostname, port, signal: abortController.signal }, (request) => {
  const url = new URL(request.url);
  if (url.pathname === "/healthz") {
    return new Response("ok", { headers: { "content-type": "text/plain" } });
  }
  return new Response(html, {
    headers: { "content-type": "text/html; charset=utf-8" },
  });
});
