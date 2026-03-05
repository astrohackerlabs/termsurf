// IPC communication for TermSurf (Issues 601–604, 698–701).
//
// All IPC event handlers run on a serial dispatch queue (`ipc_queue`).
// No mutexes needed for IPC state — serialization is guaranteed by GCD.
// The renderer's `draw_mutex` is separate and protects renderer state.
//
// Socket adapters build XPC dictionaries to reuse existing handleMessage()
// dispatch. Manual extern declarations for XPC dict API because @cImport
// may not handle the XPC header's C block types.

const std = @import("std");
const builtin = @import("builtin");
const CoreApp = @import("../App.zig");
const CoreSurface = @import("../Surface.zig");
const input = @import("../input.zig");

const internal_os = @import("../os/main.zig");
const log = std.log.scoped(.ipc);
const alloc = std.heap.page_allocator;

// Protobuf-c (Issue 699). Import generated types to force linking.
const pb = @cImport({
    @cInclude("termsurf.pb-c.h");
});

// -- XPC C API --

const xpc_object_t = ?*anyopaque;

extern "c" fn xpc_dictionary_create(keys: xpc_object_t, values: xpc_object_t, count: usize) xpc_object_t;
extern "c" fn xpc_dictionary_set_string(xdict: xpc_object_t, key: [*:0]const u8, string: [*:0]const u8) void;
extern "c" fn xpc_dictionary_get_string(xdict: xpc_object_t, key: [*:0]const u8) ?[*:0]const u8;
extern "c" fn xpc_dictionary_get_uint64(xdict: xpc_object_t, key: [*:0]const u8) u64;
extern "c" fn xpc_dictionary_get_bool(xdict: xpc_object_t, key: [*:0]const u8) bool;
extern "c" fn xpc_dictionary_set_uint64(xdict: xpc_object_t, key: [*:0]const u8, value: u64) void;
extern "c" fn xpc_dictionary_set_int64(xdict: xpc_object_t, key: [*:0]const u8, value: i64) void;
extern "c" fn xpc_dictionary_get_int64(xdict: xpc_object_t, key: [*:0]const u8) i64;
extern "c" fn xpc_dictionary_set_double(xdict: xpc_object_t, key: [*:0]const u8, value: f64) void;
extern "c" fn xpc_dictionary_set_bool(xdict: xpc_object_t, key: [*:0]const u8, value: bool) void;

// -- Dispatch queue C API --

extern "c" fn dispatch_queue_create(label: [*:0]const u8, attr: ?*anyopaque) ?*anyopaque;
extern "c" fn dispatch_async_f(queue: ?*anyopaque, context: ?*anyopaque, work: *const fn (?*anyopaque) callconv(.c) void) void;
extern const _dispatch_main_q: anyopaque;
// -- Dispatch source C API (Issue 700) --

extern "c" fn dispatch_source_create(source_type: *const anyopaque, handle: usize, mask: usize, queue: ?*anyopaque) ?*anyopaque;
extern "c" fn dispatch_source_set_event_handler_f(source: ?*anyopaque, handler: *const fn (?*anyopaque) callconv(.c) void) void;
extern "c" fn dispatch_set_context(object: ?*anyopaque, context: ?*anyopaque) void;
extern "c" fn dispatch_resume(object: ?*anyopaque) void;
extern "c" fn dispatch_source_cancel(source: ?*anyopaque) void;
extern const _dispatch_source_type_read: anyopaque;

// Embedded C API exports (Issue 690).
extern "c" fn termsurf_surface_split_with_input(ptr: *anyopaque, direction: c_int, input_ptr: [*:0]const u8) void;

// -- Data structures --

/// Per-pane state. No mutex — all access is on the serial `ipc_queue`.
const Pane = struct {
    overlay_surface: ?*CoreSurface = null,
    server: ?*Server = null,
    pane_id_key: []const u8 = "", // heap-allocated, also the key in `panes`
    pending_url_buf: [2048]u8 = undefined,
    pending_url_len: usize = 0,
    pending_pixel_w: u64 = 0,
    pending_pixel_h: u64 = 0,
    tab_sent: bool = false,
    browsing: bool = false,
    inspected_tab_id: i64 = 0, // Issue 684: nonzero = DevTools pane.
    tab_id: i64 = 0, // From tab_ready. 0 = DevTools or not yet assigned.
    web_fd: std.posix.fd_t = -1, // Issue 700: socket fd for TUI connections.
    browser: []const u8 = "", // Issue 704: browser name for this pane.
};

/// Per-(profile, browser) server state. Shared by all panes on the same profile+browser.
const Server = struct {
    process: ?std.process.Child = null,
    fd: std.posix.fd_t = -1, // Issue 701: socket fd.
    profile_key: []const u8 = "", // heap-allocated, also the key in `servers`
    browser: []const u8 = "", // Issue 704: resolved browser name.
    pane_count: usize = 0,
};

// -- Module state --

var app: *CoreApp = undefined;
var ipc_queue: ?*anyopaque = null;

/// Active panes, keyed by pane UUID string.
var panes: std.StringHashMap(*Pane) = undefined;

/// Active servers, keyed by "{profile}\x00{browser}" composite key.
var servers: std.StringHashMap(*Server) = undefined;

/// Browser registry: name → absolute path. Issue 704.
var browser_paths: std.StringHashMap([]const u8) = undefined;

/// Reverse lookup: CoreSurface pointer address → pane UUID string (Issue 606).
var surface_to_pane: std.AutoHashMap(usize, []const u8) = undefined;

/// The pane UUID that currently has Chromium focus (at most one). Issue 606.
var focused_pane: ?[]const u8 = null;

/// The most recently active browser pane — updated on tab creation and focus (Issue 684).
var last_browser_pane: ?[]const u8 = null;

/// Reverse lookup: Chromium tab_id → pane UUID string (Issue 694).
var tab_to_pane: std.AutoHashMap(i64, []const u8) = undefined;

// -- Socket state (Issue 700, multi-client Issue 701) --

const ConnType = enum { unknown, tui, chromium };

const ClientConn = struct {
    fd: std.posix.fd_t = -1,
    source: ?*anyopaque = null,
    buf: [65536]u8 = undefined,
    buf_len: usize = 0,
    conn_type: ConnType = .unknown,
    server: ?*Server = null, // set when conn_type == .chromium
};

var clients: std.ArrayList(*ClientConn) = undefined;
var sock_fd: std.posix.fd_t = -1;
var sock_source: ?*anyopaque = null;
var sock_path_buf: [256]u8 = undefined;
var sock_path_len: usize = 0;

// -- Public API --

pub fn init(core_app: *CoreApp) void {
    app = core_app;

    // Serial dispatch queue — all IPC handlers run here, no mutexes needed.
    ipc_queue = dispatch_queue_create("com.termsurf.ghost.ipc", null);

    // Initialize maps and client list.
    panes = std.StringHashMap(*Pane).init(alloc);
    servers = std.StringHashMap(*Server).init(alloc);
    browser_paths = std.StringHashMap([]const u8).init(alloc);
    surface_to_pane = std.AutoHashMap(usize, []const u8).init(alloc);
    tab_to_pane = std.AutoHashMap(i64, []const u8).init(alloc);
    clients = .{};

    // Populate browser registry (Issue 704).
    initBrowserRegistry();

    // Unix socket listener for TUI/Chromium connections (Issue 700/701).
    initSocket();

    // Tell child processes (the `web` TUI) where to find our socket.
    if (sock_fd >= 0) {
        _ = internal_os.setenv("TERMSURF_SOCKET", sock_path_buf[0..sock_path_len :0]);
    }

    log.info("ipc initialized", .{});
}

pub fn deinit() void {
    // Clean up all panes.
    var pane_it = panes.iterator();
    while (pane_it.next()) |entry| {
        const p = entry.value_ptr.*;
        if (p.overlay_surface) |surface| surface.clearOverlay();
        freeKey(p.pane_id_key);
        if (p.browser.len > 0) alloc.free(@constCast(p.browser));
        alloc.destroy(p);
    }
    panes.deinit();

    // Clean up all servers.
    var server_it = servers.iterator();
    while (server_it.next()) |entry| {
        const s = entry.value_ptr.*;
        killServer(s);
        freeKey(s.profile_key);
        if (s.browser.len > 0) alloc.free(s.browser);
        alloc.destroy(s);
    }
    servers.deinit();
    browser_paths.deinit();

    surface_to_pane.deinit();
    tab_to_pane.deinit();

    // Socket cleanup (Issue 700/701).
    for (clients.items) |c| {
        if (c.source) |src| dispatch_source_cancel(src);
        if (c.fd >= 0) std.posix.close(c.fd);
        alloc.destroy(c);
    }
    clients.deinit(alloc);
    if (sock_source) |src| {
        dispatch_source_cancel(src);
        sock_source = null;
    }
    if (sock_fd >= 0) {
        std.posix.close(sock_fd);
        sock_fd = -1;
    }
    if (sock_path_len > 0) {
        std.posix.unlink(sock_path_buf[0..sock_path_len]) catch {};
        sock_path_len = 0;
    }

    log.info("ipc connections closed", .{});
}

// -- Event handlers --

fn handleMessage(msg: xpc_object_t) void {
    const action = xpc_dictionary_get_string(msg, "action") orelse {
        log.warn("message missing 'action'", .{});
        return;
    };
    const action_str = std.mem.span(action);

    if (std.mem.eql(u8, action_str, "set_overlay")) {
        handleSetOverlay(msg);
    } else if (std.mem.eql(u8, action_str, "set_devtools_overlay")) {
        handleSetDevtoolsOverlay(msg);
    } else if (std.mem.eql(u8, action_str, "tab_ready")) {
        handleTabReady(msg);
    } else if (std.mem.eql(u8, action_str, "mode_changed")) {
        handleModeChanged(msg);
    } else if (std.mem.eql(u8, action_str, "ca_context")) {
        handleCAContext(msg);
    } else if (std.mem.eql(u8, action_str, "cursor_changed")) {
        handleCursorChanged(msg);
    } else if (std.mem.eql(u8, action_str, "loading_state")) {
        handleLoadingState(msg);
    } else if (std.mem.eql(u8, action_str, "url_changed")) {
        handleUrlChanged(msg);
    } else if (std.mem.eql(u8, action_str, "title_changed")) {
        handleTitleChanged(msg);
    } else if (std.mem.eql(u8, action_str, "navigate")) {
        handleNavigate(msg);
    } else if (std.mem.eql(u8, action_str, "set_color_scheme")) {
        handleSetColorScheme(msg);
    } else if (std.mem.eql(u8, action_str, "open_split")) {
        handleOpenSplit(msg);
    } else {
        log.warn("unknown action: {s}", .{action_str});
    }
}

fn handleSetOverlay(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const url = str(xpc_dictionary_get_string(msg, "url"));
    const profile = str(xpc_dictionary_get_string(msg, "profile"));
    const browser = str(xpc_dictionary_get_string(msg, "browser"));
    const col = xpc_dictionary_get_uint64(msg, "col");
    const row = xpc_dictionary_get_uint64(msg, "row");
    const width = xpc_dictionary_get_uint64(msg, "width");
    const height = xpc_dictionary_get_uint64(msg, "height");
    const browsing = xpc_dictionary_get_bool(msg, "browsing");

    std.debug.print("[DEBUG] handleSetOverlay: pane={s} profile={s} browser={s} url={s}\n", .{ pane_id, profile, browser, url });

    log.info("set_overlay pane={s} col={} row={} w={} h={} url={s} profile={s} browser={s} browsing={}", .{
        pane_id, col, row, width, height, url, profile, browser, browsing,
    });

    // Look up the surface by pane ID.
    const surface = app.findSurfaceByPaneId(pane_id) orelse {
        log.warn("no surface found for pane={s}", .{pane_id});
        return;
    };

    // Set overlay grid coordinates (thread-safe via draw_mutex internally).
    surface.core().setOverlay(
        @intCast(col),
        @intCast(row),
        @intCast(width),
        @intCast(height),
    );

    // Compute pixel dimensions from grid cells × cell size.
    const cell = surface.core().getCellSize();
    const new_pixel_w = width * @as(u64, cell.width);
    const new_pixel_h = height * @as(u64, cell.height);

    if (panes.get(pane_id)) |p| {
        // Existing pane — resize path.
        const old_w = p.pending_pixel_w;
        const old_h = p.pending_pixel_h;
        p.pending_pixel_w = new_pixel_w;
        p.pending_pixel_h = new_pixel_h;
        p.browsing = browsing;

        if (url.len > 0 and url.len <= p.pending_url_buf.len) {
            @memcpy(p.pending_url_buf[0..url.len], url);
            p.pending_url_len = url.len;
        }

        log.info("overlay updated pane={s} pixel={d}x{d}", .{
            pane_id, new_pixel_w, new_pixel_h,
        });

        // Send resize if tab is active and dimensions changed.
        if (p.tab_sent) {
            if (p.server) |server| {
                if (server.fd >= 0 and (new_pixel_w != old_w or new_pixel_h != old_h)) {
                    sendResize(p, server);
                }
            }
        }
    } else {
        // New pane.
        const p = alloc.create(Pane) catch {
            log.err("failed to allocate Pane", .{});
            return;
        };
        p.* = .{};

        // Owned copy of pane_id as HashMap key.
        const pane_id_key = alloc.dupe(u8, pane_id) catch {
            alloc.destroy(p);
            return;
        };
        p.pane_id_key = pane_id_key;
        panes.put(pane_id_key, p) catch {
            alloc.free(pane_id_key);
            alloc.destroy(p);
            return;
        };

        p.overlay_surface = surface.core();
        p.pending_pixel_w = new_pixel_w;
        p.pending_pixel_h = new_pixel_h;
        p.browsing = browsing;

        // Register surface → pane reverse lookup (Issue 606, mouse input).
        surface_to_pane.put(@intFromPtr(surface.core()), pane_id_key) catch {};

        // Store pending URL.
        if (url.len > 0 and url.len <= p.pending_url_buf.len) {
            @memcpy(p.pending_url_buf[0..url.len], url);
            p.pending_url_len = url.len;
        }

        log.info("new pane={s} pixel={d}x{d}", .{
            pane_id, new_pixel_w, new_pixel_h,
        });

        // Store browser on pane for DevTools auto-targeting (Issue 704).
        p.browser = alloc.dupe(u8, browser) catch "";

        // Get or create server for this (profile, browser) pair.
        if (getOrCreateServer(profile, browser)) |server| {
            p.server = server;
            server.pane_count += 1;

            if (server.fd >= 0) {
                // Server already registered — send create_tab now.
                sendCreateTab(p, server);
                if (p.browsing) {
                    sendFocusChanged(p.pane_id_key, true);
                }
            }
        }
    }
}

fn handleSetDevtoolsOverlay(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const profile = str(xpc_dictionary_get_string(msg, "profile"));
    const browser = str(xpc_dictionary_get_string(msg, "browser"));
    const col = xpc_dictionary_get_uint64(msg, "col");
    const row = xpc_dictionary_get_uint64(msg, "row");
    const width = xpc_dictionary_get_uint64(msg, "width");
    const height = xpc_dictionary_get_uint64(msg, "height");
    const browsing = xpc_dictionary_get_bool(msg, "browsing");
    const inspected_tab_id = xpc_dictionary_get_int64(msg, "inspected_tab_id");

    log.info("set_devtools_overlay pane={s} inspected_tab_id={d} profile={s} browser={s}", .{
        pane_id, inspected_tab_id, profile, browser,
    });

    // Look up the surface by pane ID.
    const surface = app.findSurfaceByPaneId(pane_id) orelse {
        log.warn("no surface found for pane={s}", .{pane_id});
        return;
    };

    // Set overlay grid coordinates.
    surface.core().setOverlay(
        @intCast(col),
        @intCast(row),
        @intCast(width),
        @intCast(height),
    );

    const cell = surface.core().getCellSize();
    const new_pixel_w = width * @as(u64, cell.width);
    const new_pixel_h = height * @as(u64, cell.height);

    if (panes.get(pane_id)) |p| {
        // Existing pane — resize path (same as set_overlay).
        const old_w = p.pending_pixel_w;
        const old_h = p.pending_pixel_h;
        p.pending_pixel_w = new_pixel_w;
        p.pending_pixel_h = new_pixel_h;
        p.browsing = browsing;

        if (p.tab_sent) {
            if (p.server) |server| {
                if (server.fd >= 0 and (new_pixel_w != old_w or new_pixel_h != old_h)) {
                    sendResize(p, server);
                }
            }
        }
    } else {
        // New DevTools pane.
        const p = alloc.create(Pane) catch {
            log.err("failed to allocate Pane", .{});
            return;
        };
        p.* = .{};

        const pane_id_key = alloc.dupe(u8, pane_id) catch {
            alloc.destroy(p);
            return;
        };
        p.pane_id_key = pane_id_key;
        panes.put(pane_id_key, p) catch {
            alloc.free(pane_id_key);
            alloc.destroy(p);
            return;
        };

        p.overlay_surface = surface.core();
        p.pending_pixel_w = new_pixel_w;
        p.pending_pixel_h = new_pixel_h;
        p.browsing = browsing;
        p.inspected_tab_id = inspected_tab_id;

        surface_to_pane.put(@intFromPtr(surface.core()), pane_id_key) catch {};

        log.info("new devtools pane={s} pixel={d}x{d} inspected_tab_id={d}", .{
            pane_id, new_pixel_w, new_pixel_h, p.inspected_tab_id,
        });

        // DevTools must run in the same Chromium process as the inspected tab.
        // Look up the inspected tab's pane and use its server (Issue 705 Exp 10).
        const inspected_pane_id = tab_to_pane.get(p.inspected_tab_id) orelse {
            log.err("devtools: inspected tab_id={d} not found in tab_to_pane", .{p.inspected_tab_id});
            cleanupPane(pane_id_key);
            return;
        };
        const inspected_pane = panes.get(inspected_pane_id) orelse {
            log.err("devtools: inspected pane {s} not found", .{inspected_pane_id});
            cleanupPane(pane_id_key);
            return;
        };
        if (inspected_pane.server) |target_server| {
            p.server = target_server;
            target_server.pane_count += 1;

            if (target_server.fd >= 0) {
                sendCreateDevToolsTab(p, target_server);
                if (p.browsing) {
                    sendFocusChanged(p.pane_id_key, true);
                }
            }
        } else {
            log.err("devtools: inspected pane {s} has no server", .{inspected_pane_id});
            cleanupPane(pane_id_key);
            return;
        }
    }
}

fn handleCAContext(msg: xpc_object_t) void {
    const tab_id = xpc_dictionary_get_int64(msg, "tab_id");
    const context_id: u32 = @intCast(xpc_dictionary_get_uint64(msg, "ca_context_id"));

    log.info("ca_context tab={d} context_id={}", .{ tab_id, context_id });

    // Guard against zero context ID (Issue 630, fix G3).
    if (context_id == 0) return;

    const pane_id = tab_to_pane.get(tab_id) orelse return;
    if (panes.get(pane_id)) |p| {
        if (p.overlay_surface) |surface| {
            // Dispatch CALayerHost creation to the main thread (Issue 630,
            // fix G1). Core Animation requires all layer-tree mutations on
            // the main thread.
            const Ctx = struct {
                surf: *CoreSurface,
                ctx_id: u32,

                fn dispatch(raw: ?*anyopaque) callconv(.c) void {
                    const self: *@This() = @ptrCast(@alignCast(raw));
                    self.surf.setCAContextId(self.ctx_id);
                    std.heap.c_allocator.destroy(self);
                }
            };
            const ctx = std.heap.c_allocator.create(Ctx) catch return;
            ctx.* = .{ .surf = surface, .ctx_id = context_id };
            dispatch_async_f(@constCast(&_dispatch_main_q), @ptrCast(ctx), &Ctx.dispatch);
        }
    }
}

fn handleTabReady(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const tab_id = xpc_dictionary_get_int64(msg, "tab_id");

    if (panes.get(pane_id)) |p| {
        p.tab_id = tab_id;
        if (p.inspected_tab_id == 0) {
            last_browser_pane = p.pane_id_key; // heap-allocated, stable
        }
        // Register reverse lookup (Issue 694).
        tab_to_pane.put(tab_id, p.pane_id_key) catch {};
    }

    log.info("tab_ready pane={s} tab_id={d}", .{ pane_id, tab_id });
}

fn handleModeChanged(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const browsing = xpc_dictionary_get_bool(msg, "browsing");

    log.info("mode_changed pane={s} browsing={}", .{ pane_id, browsing });

    if (panes.get(pane_id)) |p| {
        p.browsing = browsing;
        sendFocusChanged(p.pane_id_key, browsing);
    }
}

fn handleCursorChanged(msg: xpc_object_t) void {
    const tab_id = xpc_dictionary_get_int64(msg, "tab_id");
    const cursor_type = xpc_dictionary_get_int64(msg, "cursor_type");

    const pane_id = tab_to_pane.get(tab_id) orelse return;
    if (panes.get(pane_id)) |p| {
        if (p.overlay_surface) |surface| {
            surface.overlay_cursor_type = cursor_type;
        }
    }
}

fn handleLoadingState(msg: xpc_object_t) void {
    const tab_id = xpc_dictionary_get_int64(msg, "tab_id");
    const pane_id = tab_to_pane.get(tab_id) orelse return;
    const p = panes.get(pane_id) orelse return;
    if (p.web_fd < 0) return;

    const state_str = xpc_dictionary_get_string(msg, "state") orelse return;
    const progress = xpc_dictionary_get_uint64(msg, "progress");
    var ls: pb.Termsurf__LoadingState = undefined;
    pb.termsurf__loading_state__init(&ls);
    ls.tab_id = tab_id;
    ls.state = @ptrCast(@constCast(state_str));
    ls.progress = progress;
    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = @intCast(16); // LOADING_STATE
    wrapper.unnamed_0.loading_state = &ls;
    sendProtobuf(p.web_fd, &wrapper);
}

fn handleUrlChanged(msg: xpc_object_t) void {
    const tab_id = xpc_dictionary_get_int64(msg, "tab_id");
    const pane_id = tab_to_pane.get(tab_id) orelse return;
    const p = panes.get(pane_id) orelse return;
    if (p.web_fd < 0) return;

    const url_str = xpc_dictionary_get_string(msg, "url") orelse return;
    var uc: pb.Termsurf__UrlChanged = undefined;
    pb.termsurf__url_changed__init(&uc);
    uc.tab_id = tab_id;
    uc.url = @ptrCast(@constCast(url_str));
    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = @intCast(15); // URL_CHANGED
    wrapper.unnamed_0.url_changed = &uc;
    sendProtobuf(p.web_fd, &wrapper);
}

fn handleTitleChanged(msg: xpc_object_t) void {
    const tab_id = xpc_dictionary_get_int64(msg, "tab_id");
    const pane_id = tab_to_pane.get(tab_id) orelse return;
    const p = panes.get(pane_id) orelse return;
    if (p.web_fd < 0) return;

    const title_str = xpc_dictionary_get_string(msg, "title") orelse return;
    var tc: pb.Termsurf__TitleChanged = undefined;
    pb.termsurf__title_changed__init(&tc);
    tc.tab_id = tab_id;
    tc.title = @ptrCast(@constCast(title_str));
    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = @intCast(17); // TITLE_CHANGED
    wrapper.unnamed_0.title_changed = &tc;
    sendProtobuf(p.web_fd, &wrapper);
}

fn handleNavigate(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const url = str(xpc_dictionary_get_string(msg, "url"));

    log.info("navigate pane={s} url={s}", .{ pane_id, url });

    const p = panes.get(pane_id) orelse {
        log.warn("navigate: no pane for {s}", .{pane_id});
        return;
    };
    const server = p.server orelse return;
    if (server.fd < 0) return;

    var nav: pb.Termsurf__Navigate = undefined;
    pb.termsurf__navigate__init(&nav);
    nav.tab_id = p.tab_id;

    var url_z: [2049]u8 = undefined;
    if (url.len > 0 and url.len < url_z.len) {
        @memcpy(url_z[0..url.len], url);
        url_z[url.len] = 0;
        nav.url = @ptrCast(&url_z);
    }

    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = 5; // NAVIGATE
    wrapper.unnamed_0.navigate = &nav;
    sendProtobuf(server.fd, &wrapper);
}

fn handleSetColorScheme(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const scheme = str(xpc_dictionary_get_string(msg, "scheme"));

    log.info("set_color_scheme pane={s} scheme={s}", .{ pane_id, scheme });

    const p = panes.get(pane_id) orelse {
        log.warn("set_color_scheme: no pane for {s}", .{pane_id});
        return;
    };
    if (!p.tab_sent) return;
    const server = p.server orelse return;
    if (server.fd < 0) return;

    // Resolve scheme to dark bool.
    const dark: bool = if (std.mem.eql(u8, scheme, "dark"))
        true
    else if (std.mem.eql(u8, scheme, "light"))
        false
    else if (std.mem.eql(u8, scheme, "system"))
        // Read current system theme from the surface.
        if (p.overlay_surface) |surface|
            surface.config_conditional_state.theme == .dark
        else
            true // default to dark
    else
        return; // unknown scheme, ignore

    var sc: pb.Termsurf__SetColorScheme = undefined;
    pb.termsurf__set_color_scheme__init(&sc);
    sc.tab_id = p.tab_id;
    sc.dark = @intFromBool(dark);

    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = 11; // SET_COLOR_SCHEME
    wrapper.unnamed_0.set_color_scheme = &sc;
    sendProtobuf(server.fd, &wrapper);
    log.info("forwarded set_color_scheme pane={s} dark={}", .{ pane_id, dark });
}

// -- Focus lifecycle (Issue 606 Experiment 5) --

/// Send focus_changed to Chromium, enforcing single-pane focus.
fn sendFocusChanged(pane_id: []const u8, focused: bool) void {
    const p = panes.get(pane_id) orelse return;
    const server = p.server orelse return;
    if (server.fd < 0) return;

    // Single-pane enforcement: unfocus previous pane.
    if (focused) {
        if (focused_pane) |prev| {
            if (!std.mem.eql(u8, prev, pane_id)) {
                sendFocusMessage(prev, false);
            }
        }
        focused_pane = pane_id;

        // Track last focused browser (non-DevTools) pane for auto-targeting (Issue 684).
        if (p.inspected_tab_id == 0) {
            last_browser_pane = pane_id;
        }
    } else {
        if (focused_pane) |prev| {
            if (std.mem.eql(u8, prev, pane_id)) {
                focused_pane = null;
            }
        }
    }

    sendFocusMessage(pane_id, focused);
}

fn sendFocusMessage(pane_id: []const u8, focused: bool) void {
    const p = panes.get(pane_id) orelse return;
    const server = p.server orelse return;
    if (server.fd < 0) return;

    var fc: pb.Termsurf__FocusChanged = undefined;
    pb.termsurf__focus_changed__init(&fc);
    fc.tab_id = p.tab_id;
    fc.focused = @intFromBool(focused);

    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = 10; // FOCUS_CHANGED
    wrapper.unnamed_0.focus_changed = &fc;
    sendProtobuf(server.fd, &wrapper);
    log.info("focus_changed pane={s} focused={}", .{ pane_id, focused });
}

/// Called from Surface.paneFocusChanged (main thread). Dispatches to XPC queue.
pub fn handlePaneFocusChanged(surface: *CoreSurface, focused: bool) void {
    const ptr_val = @intFromPtr(surface);
    const dispatch_fn = struct {
        fn f(ctx: ?*anyopaque) callconv(.c) void {
            const addr = @intFromPtr(ctx);
            // focused encoded in low bit
            const surf_addr = addr & ~@as(usize, 1);
            const is_focused = (addr & 1) != 0;
            const pane_id = surface_to_pane.get(surf_addr) orelse return;
            const p = panes.get(pane_id) orelse return;
            if (is_focused) {
                // Track last browser pane regardless of mode (Issue 684 Exp 4).
                if (p.inspected_tab_id == 0 and p.tab_id > 0) {
                    last_browser_pane = p.pane_id_key;
                }
                // Only forward focus to Chromium if in browse mode.
                if (p.browsing) {
                    sendFocusChanged(pane_id, true);
                }
            } else {
                sendFocusChanged(pane_id, false);
            }
        }
    }.f;
    // Encode focused state in low bit of pointer (Surface is aligned).
    const encoded = ptr_val | @as(usize, if (focused) 1 else 0);
    dispatch_async_f(ipc_queue, @ptrFromInt(encoded), dispatch_fn);
}

/// Returns true if the surface's pane is in browse mode AND is the
/// focused pane — the only state where mouse events should forward
/// to Chromium. (Issue 606 Experiment 7.)
pub fn isOverlayForwarding(surface: *CoreSurface) bool {
    const pane_id = surface_to_pane.get(@intFromPtr(surface)) orelse return false;
    const p = panes.get(pane_id) orelse return false;
    if (!p.browsing) return false;
    const fp = focused_pane orelse return false;
    return std.mem.eql(u8, fp, pane_id);
}

/// Returns true if the surface has an overlay pane registered.
pub fn hasOverlayPane(surface: *CoreSurface) bool {
    return surface_to_pane.get(@intFromPtr(surface)) != null;
}

/// Returns true if the surface has an overlay in browse mode.
/// Unlike isOverlayForwarding, this does not check pane focus.
pub fn isOverlayBrowsing(surface: *CoreSurface) bool {
    const pane_id = surface_to_pane.get(@intFromPtr(surface)) orelse return false;
    const p = panes.get(pane_id) orelse return false;
    return p.browsing;
}

// -- Mouse-driven mode switching (Issue 606 Experiment 6) --

fn sendModeToWeb(p: *Pane, browsing: bool) void {
    if (p.web_fd < 0) return;

    var mc: pb.Termsurf__ModeChanged = undefined;
    pb.termsurf__mode_changed__init(&mc);
    mc.browsing = @intFromBool(browsing);
    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = @intCast(22); // MODE_CHANGED
    wrapper.unnamed_0.mode_changed = &mc;
    sendProtobuf(p.web_fd, &wrapper);
}

/// Called from mouseButtonCallback when a left-click hits the overlay.
/// If the pane is in control mode, switches to browse mode and focuses.
pub fn notifyOverlayClicked(surface: *CoreSurface) void {
    const pane_id_key = surface_to_pane.get(@intFromPtr(surface)) orelse return;
    const p = panes.get(pane_id_key) orelse return;
    if (p.browsing) return;

    p.browsing = true;
    sendModeToWeb(p, true);
    sendFocusChanged(pane_id_key, true);
}

/// Called when Esc is pressed. Always returns to control mode,
/// regardless of the current browsing state (Issue 665).
pub fn notifyEsc(surface: *CoreSurface) void {
    const pane_id_key = surface_to_pane.get(@intFromPtr(surface)) orelse return;
    const p = panes.get(pane_id_key) orelse return;

    if (p.browsing) {
        p.browsing = false;
        sendFocusChanged(pane_id_key, false);
    }
    sendModeToWeb(p, false);
}

/// Called from mouseButtonCallback when a left-click misses the overlay.
/// If the pane is in browse mode, switches to control mode and unfocuses.
pub fn notifyNonOverlayClicked(surface: *CoreSurface) void {
    const pane_id_key = surface_to_pane.get(@intFromPtr(surface)) orelse return;
    const p = panes.get(pane_id_key) orelse return;
    if (!p.browsing) return;

    p.browsing = false;
    sendModeToWeb(p, false);
    sendFocusChanged(pane_id_key, false);
}

/// Extract profile name from composite server key "{profile}\x00{browser}".
fn serverProfile(server: *const Server) []const u8 {
    const key = server.profile_key;
    for (key, 0..) |c, i| {
        if (c == 0) return key[0..i];
    }
    return key;
}

// -- Browser registry (Issue 704) --

fn initBrowserRegistry() void {
    const home = std.posix.getenv("HOME") orelse return;

    // Dev fallback paths for known browsers.
    const browsers = [_]struct { name: []const u8, suffix: []const u8 }{
        .{ .name = "chromium", .suffix = "/dev/termsurf/chromium/src/out/Default/Chromium Profile Server.app/Contents/MacOS/Chromium Profile Server" },
        .{ .name = "plusium", .suffix = "/dev/termsurf/chromium/src/out/Default/plusium" },
    };

    for (&browsers) |b| {
        var path_buf: [std.fs.max_path_bytes]u8 = undefined;
        const path = std.fmt.bufPrint(&path_buf, "{s}{s}", .{ home, b.suffix }) catch continue;

        // Check if binary exists.
        if (std.fs.accessAbsolute(path, .{})) {
            const name = alloc.dupe(u8, b.name) catch continue;
            const path_owned = alloc.dupe(u8, path) catch {
                alloc.free(name);
                continue;
            };
            browser_paths.put(name, path_owned) catch {
                alloc.free(name);
                alloc.free(path_owned);
                continue;
            };
            log.info("browser registered: {s} → {s}", .{ name, path_owned });
        } else |_| {}
    }
}

/// Build composite server key: "{profile}\x00{browser}".
fn buildServerKey(profile: []const u8, browser: []const u8) ?[]u8 {
    const key = alloc.alloc(u8, profile.len + 1 + browser.len) catch return null;
    @memcpy(key[0..profile.len], profile);
    key[profile.len] = 0;
    @memcpy(key[profile.len + 1 ..], browser);
    return key;
}

/// Resolve a browser specifier to an absolute path.
/// Empty → "chromium" (default). Starts with "/" → absolute path. Otherwise → registry lookup.
fn resolveBrowserPath(browser: []const u8) ?[]const u8 {
    const name = if (browser.len == 0) "chromium" else browser;
    if (name.len > 0 and name[0] == '/') return name;
    return browser_paths.get(name);
}

// -- Server lifecycle --

fn getOrCreateServer(profile: []const u8, browser: []const u8) ?*Server {
    const effective_browser: []const u8 = if (browser.len == 0) "chromium" else browser;

    std.debug.print("[DEBUG] getOrCreateServer: profile={s} browser={s} effective_browser={s}\n", .{ profile, browser, effective_browser });

    // Build composite key for lookup.
    var key_buf: [512]u8 = undefined;
    const key_len = profile.len + 1 + effective_browser.len;
    if (key_len > key_buf.len) return null;
    @memcpy(key_buf[0..profile.len], profile);
    key_buf[profile.len] = 0;
    @memcpy(key_buf[profile.len + 1 .. key_len], effective_browser);
    const lookup_key = key_buf[0..key_len];

    if (servers.get(lookup_key)) |server| {
        std.debug.print("[DEBUG] getOrCreateServer: found existing server fd={}\n", .{server.fd});
        return server;
    }

    std.debug.print("[DEBUG] getOrCreateServer: no existing server, creating new\n", .{});

    // Resolve browser path.
    const browser_path = resolveBrowserPath(effective_browser) orelse {
        std.debug.print("[DEBUG] getOrCreateServer: resolveBrowserPath FAILED for {s}\n", .{effective_browser});
        log.err("unknown browser: {s}", .{effective_browser});
        return null;
    };

    // Create new server for this (profile, browser) pair.
    const profile_key = buildServerKey(profile, effective_browser) orelse return null;
    const browser_owned = alloc.dupe(u8, effective_browser) catch {
        alloc.free(profile_key);
        return null;
    };
    const server = alloc.create(Server) catch {
        alloc.free(profile_key);
        alloc.free(browser_owned);
        return null;
    };
    server.* = .{ .profile_key = profile_key, .browser = browser_owned };
    servers.put(profile_key, server) catch {
        alloc.free(profile_key);
        alloc.free(browser_owned);
        alloc.destroy(server);
        return null;
    };

    spawnServerProcess(server, browser_path);
    return server;
}

fn spawnServerProcess(server: *Server, browser_path: []const u8) void {
    const home = std.posix.getenv("HOME") orelse {
        log.err("HOME not set, cannot spawn server", .{});
        return;
    };

    // XDG_DATA_HOME for browser profile data (default: ~/.local/share)
    var data_home_buf: [512]u8 = undefined;
    const data_home = std.posix.getenv("XDG_DATA_HOME") orelse std.fmt.bufPrintZ(
        &data_home_buf,
        "{s}/.local/share",
        .{home},
    ) catch {
        log.err("data home path too long", .{});
        return;
    };

    // Null-terminate the browser path for process spawn.
    var server_path_buf: [std.fs.max_path_bytes]u8 = undefined;
    const server_path = std.fmt.bufPrintZ(
        &server_path_buf,
        "{s}",
        .{browser_path},
    ) catch {
        log.err("server path too long", .{});
        return;
    };

    var ipc_arg_buf: [256]u8 = undefined;
    const ipc_arg = std.fmt.bufPrintZ(
        &ipc_arg_buf,
        "--ipc-socket={s}",
        .{sock_path_buf[0..sock_path_len]},
    ) catch return;

    var data_arg_buf: [512]u8 = undefined;
    const data_arg = std.fmt.bufPrintZ(
        &data_arg_buf,
        "--user-data-dir={s}/termsurf/" ++ (if (comptime builtin.mode == .Debug) "debug/" else "") ++ "chromium-profiles/{s}",
        .{ data_home, serverProfile(server) },
    ) catch {
        log.err("data dir path too long", .{});
        return;
    };

    var hidden_buf: [16]u8 = undefined;
    const hidden_arg = std.fmt.bufPrintZ(&hidden_buf, "--hidden", .{}) catch return;

    var nosandbox_buf: [16]u8 = undefined;
    const nosandbox_arg = std.fmt.bufPrintZ(&nosandbox_buf, "--no-sandbox", .{}) catch return;

    var logging_buf: [64]u8 = undefined;
    const logging_arg = std.fmt.bufPrintZ(
        &logging_buf,
        "--enable-logging",
        .{},
    ) catch return;

    // XDG_STATE_HOME for logs (default: ~/.local/state)
    var state_home_buf: [512]u8 = undefined;
    const state_home = std.posix.getenv("XDG_STATE_HOME") orelse std.fmt.bufPrintZ(
        &state_home_buf,
        "{s}/.local/state",
        .{home},
    ) catch {
        log.err("state home path too long", .{});
        return;
    };

    // Ensure log directory exists.
    var logdir_buf: [256]u8 = undefined;
    const logdir = std.fmt.bufPrintZ(
        &logdir_buf,
        "{s}/termsurf",
        .{state_home},
    ) catch null;
    if (logdir) |d| {
        std.fs.cwd().makePath(d) catch {};
    }

    var logfile_buf: [256]u8 = undefined;
    const logfile_arg = std.fmt.bufPrintZ(
        &logfile_buf,
        "--log-file={s}/termsurf/chromium-server.log",
        .{state_home},
    ) catch return;

    log.info("spawning server profile={s} browser={s}", .{ serverProfile(server), server.browser });

    std.debug.print("[DEBUG] spawnServerProcess: about to spawn profile={s} browser={s} path={s}\n", .{ serverProfile(server), server.browser, browser_path });

    var child = std.process.Child.init(
        &.{ server_path, ipc_arg, data_arg, hidden_arg, nosandbox_arg, logging_arg, logfile_arg },
        alloc,
    );
    child.spawn() catch |err| {
        std.debug.print("[DEBUG] spawnServerProcess: spawn FAILED err={}\n", .{err});
        log.err("failed to spawn server: {}", .{err});
        return;
    };

    server.process = child;
    std.debug.print("[DEBUG] spawnServerProcess: spawned pid={d} profile={s} browser={s}\n", .{ child.id, serverProfile(server), server.browser });
    log.info("server spawned pid={d} profile={s} browser={s}", .{ child.id, serverProfile(server), server.browser });
}

fn killServer(server: *Server) void {
    if (server.process) |*proc| {
        _ = proc.kill() catch {};
        _ = proc.wait() catch {};
        log.info("server killed profile={s}", .{server.profile_key});
    }
    server.process = null;
}

fn sendCreateTab(p: *Pane, server: *Server) void {
    // Color scheme (Issue 680).
    const dark: bool = if (p.overlay_surface) |surface|
        surface.config_conditional_state.theme == .dark
    else
        true; // default to dark

    var ct: pb.Termsurf__CreateTab = undefined;
    pb.termsurf__create_tab__init(&ct);

    var url_z: [2049]u8 = undefined;
    if (p.pending_url_len > 0 and p.pending_url_len < url_z.len) {
        @memcpy(url_z[0..p.pending_url_len], p.pending_url_buf[0..p.pending_url_len]);
        url_z[p.pending_url_len] = 0;
        ct.url = @ptrCast(&url_z);
    }

    var pane_z: [37]u8 = undefined;
    if (p.pane_id_key.len > 0 and p.pane_id_key.len <= 36) {
        @memcpy(pane_z[0..p.pane_id_key.len], p.pane_id_key);
        pane_z[p.pane_id_key.len] = 0;
        ct.pane_id = @ptrCast(&pane_z);
    }

    ct.pixel_width = p.pending_pixel_w;
    ct.pixel_height = p.pending_pixel_h;
    ct.dark = @intFromBool(dark);

    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = 1; // CREATE_TAB
    wrapper.unnamed_0.create_tab = &ct;
    sendProtobuf(server.fd, &wrapper);

    p.tab_sent = true;
    log.info("sent create_tab pane={s} pixel={d}x{d} dark={}", .{
        p.pane_id_key, p.pending_pixel_w, p.pending_pixel_h, dark,
    });
}

fn sendCreateDevToolsTab(p: *Pane, server: *Server) void {
    const dark: bool = if (p.overlay_surface) |surface|
        surface.config_conditional_state.theme == .dark
    else
        true; // default to dark

    var dt: pb.Termsurf__CreateDevtoolsTab = undefined;
    pb.termsurf__create_devtools_tab__init(&dt);

    var pane_z: [37]u8 = undefined;
    if (p.pane_id_key.len > 0 and p.pane_id_key.len <= 36) {
        @memcpy(pane_z[0..p.pane_id_key.len], p.pane_id_key);
        pane_z[p.pane_id_key.len] = 0;
        dt.pane_id = @ptrCast(&pane_z);
    }

    dt.inspected_tab_id = p.inspected_tab_id;
    dt.pixel_width = p.pending_pixel_w;
    dt.pixel_height = p.pending_pixel_h;
    dt.dark = @intFromBool(dark);

    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = 2; // CREATE_DEVTOOLS_TAB
    wrapper.unnamed_0.create_devtools_tab = &dt;
    sendProtobuf(server.fd, &wrapper);

    p.tab_sent = true;
    log.info("sent create_devtools_tab pane={s} inspected_tab_id={d} dark={}", .{
        p.pane_id_key, p.inspected_tab_id, dark,
    });
}

fn sendResize(p: *Pane, server: *Server) void {
    var r: pb.Termsurf__Resize = undefined;
    pb.termsurf__resize__init(&r);
    r.tab_id = p.tab_id;
    r.pixel_width = p.pending_pixel_w;
    r.pixel_height = p.pending_pixel_h;

    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = 3; // RESIZE
    wrapper.unnamed_0.resize = &r;
    sendProtobuf(server.fd, &wrapper);
    log.info("sent resize pane={s} pixel={d}x{d}", .{
        p.pane_id_key, p.pending_pixel_w, p.pending_pixel_h,
    });
}

// -- Color scheme (Issue 680) --

/// Called from Surface.colorSchemeCallback (main thread). Dispatches to XPC queue.
pub fn handleColorSchemeChanged(surface: *CoreSurface, dark: bool) void {
    const ptr_val = @intFromPtr(surface);
    const dispatch_fn = struct {
        fn f(ctx: ?*anyopaque) callconv(.c) void {
            const addr = @intFromPtr(ctx);
            const surf_addr = addr & ~@as(usize, 1);
            const is_dark = (addr & 1) != 0;
            const pane_id = surface_to_pane.get(surf_addr) orelse return;
            const p = panes.get(pane_id) orelse return;
            if (!p.tab_sent) return;
            const server = p.server orelse return;
            if (server.fd < 0) return;

            var sc: pb.Termsurf__SetColorScheme = undefined;
            pb.termsurf__set_color_scheme__init(&sc);
            sc.tab_id = p.tab_id;
            sc.dark = @intFromBool(is_dark);

            var wrapper: pb.Termsurf__TermSurfMessage = undefined;
            pb.termsurf__term_surf_message__init(&wrapper);
            wrapper.msg_case = 11; // SET_COLOR_SCHEME
            wrapper.unnamed_0.set_color_scheme = &sc;
            sendProtobuf(server.fd, &wrapper);
            log.info("sent set_color_scheme pane={s} dark={}", .{ pane_id, is_dark });
        }
    }.f;
    // Encode dark state in low bit of pointer (Surface is aligned).
    const encoded = ptr_val | @as(usize, if (dark) 1 else 0);
    dispatch_async_f(ipc_queue, @ptrFromInt(encoded), dispatch_fn);
}

// -- Mouse input (Issue 606) --

/// Called from Surface.mouseButtonCallback when a click hits the overlay.
/// Looks up the pane by CoreSurface pointer, constructs an XPC mouse_event
/// message, and sends it on the server's control connection.
pub fn sendMouseEvent(
    surface: *CoreSurface,
    action: input.MouseButtonState,
    button: input.MouseButton,
    mods: input.Mods,
    overlay_x: f64,
    overlay_y: f64,
    click_count: u8,
) void {
    const pane_id_key = surface_to_pane.get(@intFromPtr(surface)) orelse {
        log.warn("sendMouseEvent: no pane for surface", .{});
        return;
    };
    const p = panes.get(pane_id_key) orelse return;
    const server = p.server orelse return;
    if (server.fd < 0) return;

    // Modifier bitmask: shift=1, ctrl=2, alt=4, cmd=8.
    var modifiers: u64 = 0;
    if (mods.shift) modifiers |= 1;
    if (mods.ctrl) modifiers |= 2;
    if (mods.alt) modifiers |= 4;
    if (mods.super) modifiers |= 8;
    if (action == .press) {
        if (button == .left) modifiers |= 64; // 1 << 6
        if (button == .right) modifiers |= 256; // 1 << 8
    }

    var me: pb.Termsurf__MouseEvent = undefined;
    pb.termsurf__mouse_event__init(&me);
    me.tab_id = p.tab_id;
    me.type = @ptrCast(@constCast(switch (action) {
        .press => @as([*:0]const u8, "down"),
        .release => @as([*:0]const u8, "up"),
    }));
    me.button = @ptrCast(@constCast(switch (button) {
        .left => @as([*:0]const u8, "left"),
        .right => @as([*:0]const u8, "right"),
        .middle => @as([*:0]const u8, "middle"),
        else => @as([*:0]const u8, "left"),
    }));
    me.x = overlay_x;
    me.y = overlay_y;
    me.click_count = @intCast(click_count);
    me.modifiers = modifiers;

    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = 6; // MOUSE_EVENT
    wrapper.unnamed_0.mouse_event = &me;
    sendProtobuf(server.fd, &wrapper);
}

/// Called from Surface.scrollCallback when a scroll hits the overlay.
/// Reads raw scroll data from surface.raw_scroll (set by
/// termsurf_macos_surface_mouse_scroll) and forwards to Chromium.
pub fn sendScrollEvent(
    surface: *CoreSurface,
    overlay_x: f64,
    overlay_y: f64,
) void {
    const pane_id_key = surface_to_pane.get(@intFromPtr(surface)) orelse {
        log.warn("sendScrollEvent: no pane for surface", .{});
        return;
    };
    const p = panes.get(pane_id_key) orelse return;
    const server = p.server orelse return;
    if (server.fd < 0) return;

    const raw = surface.raw_scroll;

    var se: pb.Termsurf__ScrollEvent = undefined;
    pb.termsurf__scroll_event__init(&se);
    se.tab_id = p.tab_id;
    se.x = overlay_x;
    se.y = overlay_y;
    se.delta_x = raw.delta_x;
    se.delta_y = raw.delta_y;
    se.phase = raw.phase;
    se.momentum_phase = raw.momentum_phase;
    se.precise = @intFromBool(raw.precise);
    se.modifiers = 0;

    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = 8; // SCROLL_EVENT
    wrapper.unnamed_0.scroll_event = &se;
    sendProtobuf(server.fd, &wrapper);
}

/// Called from Surface.cursorPosCallback when the mouse is over the overlay.
/// Sends mouse_move with button-down flags derived from click_state.
pub fn sendMouseMove(
    surface: *CoreSurface,
    overlay_x: f64,
    overlay_y: f64,
) void {
    const pane_id_key = surface_to_pane.get(@intFromPtr(surface)) orelse {
        log.warn("sendMouseMove: no pane for surface", .{});
        return;
    };
    const p = panes.get(pane_id_key) orelse return;
    const server = p.server orelse return;
    if (server.fd < 0) return;

    // Button-down flags from click_state (for drag vs hover distinction).
    var modifiers: u64 = 0;
    const left_idx = @intFromEnum(input.MouseButton.left);
    const right_idx = @intFromEnum(input.MouseButton.right);
    if (surface.mouse.click_state[left_idx] == .press) modifiers |= 64;
    if (surface.mouse.click_state[right_idx] == .press) modifiers |= 256;

    var mm: pb.Termsurf__MouseMove = undefined;
    pb.termsurf__mouse_move__init(&mm);
    mm.tab_id = p.tab_id;
    mm.x = overlay_x;
    mm.y = overlay_y;
    mm.modifiers = modifiers;

    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = 7; // MOUSE_MOVE
    wrapper.unnamed_0.mouse_move = &mm;
    sendProtobuf(server.fd, &wrapper);
}

// -- Keyboard input (Issue 607) --

/// Map TermSurf input.Key to Windows virtual key code for Chromium.
fn keyToWindowsVK(key: input.Key) u32 {
    return switch (key) {
        .key_a => 0x41,
        .key_b => 0x42,
        .key_c => 0x43,
        .key_d => 0x44,
        .key_e => 0x45,
        .key_f => 0x46,
        .key_g => 0x47,
        .key_h => 0x48,
        .key_i => 0x49,
        .key_j => 0x4A,
        .key_k => 0x4B,
        .key_l => 0x4C,
        .key_m => 0x4D,
        .key_n => 0x4E,
        .key_o => 0x4F,
        .key_p => 0x50,
        .key_q => 0x51,
        .key_r => 0x52,
        .key_s => 0x53,
        .key_t => 0x54,
        .key_u => 0x55,
        .key_v => 0x56,
        .key_w => 0x57,
        .key_x => 0x58,
        .key_y => 0x59,
        .key_z => 0x5A,
        .digit_0 => 0x30,
        .digit_1 => 0x31,
        .digit_2 => 0x32,
        .digit_3 => 0x33,
        .digit_4 => 0x34,
        .digit_5 => 0x35,
        .digit_6 => 0x36,
        .digit_7 => 0x37,
        .digit_8 => 0x38,
        .digit_9 => 0x39,
        .enter => 0x0D,
        .tab => 0x09,
        .backspace => 0x08,
        .escape => 0x1B,
        .space => 0x20,
        .delete => 0x2E,
        .arrow_up => 0x26,
        .arrow_down => 0x28,
        .arrow_left => 0x25,
        .arrow_right => 0x27,
        .home => 0x24,
        .end => 0x23,
        .page_up => 0x21,
        .page_down => 0x22,
        .insert => 0x2D,
        .f1 => 0x70,
        .f2 => 0x71,
        .f3 => 0x72,
        .f4 => 0x73,
        .f5 => 0x74,
        .f6 => 0x75,
        .f7 => 0x76,
        .f8 => 0x77,
        .f9 => 0x78,
        .f10 => 0x79,
        .f11 => 0x7A,
        .f12 => 0x7B,
        .semicolon => 0xBA,
        .equal => 0xBB,
        .comma => 0xBC,
        .minus => 0xBD,
        .period => 0xBE,
        .slash => 0xBF,
        .backquote => 0xC0,
        .bracket_left => 0xDB,
        .backslash => 0xDC,
        .bracket_right => 0xDD,
        .quote => 0xDE,
        else => 0,
    };
}

/// Called from Surface.keyCallback when in browse mode.
/// Constructs an XPC key_event message and sends it to the Chromium server.
pub fn sendKeyEvent(
    surface: *CoreSurface,
    action: input.Action,
    key: input.Key,
    mods: input.Mods,
    utf8: []const u8,
) void {
    const pane_id_key = surface_to_pane.get(@intFromPtr(surface)) orelse {
        log.warn("sendKeyEvent: no pane for surface", .{});
        return;
    };
    const p = panes.get(pane_id_key) orelse return;
    const server = p.server orelse return;
    if (server.fd < 0) return;

    var modifiers: u64 = 0;
    if (mods.shift) modifiers |= 1;
    if (mods.ctrl) modifiers |= 2;
    if (mods.alt) modifiers |= 4;
    if (mods.super) modifiers |= 8;

    var ke: pb.Termsurf__KeyEvent = undefined;
    pb.termsurf__key_event__init(&ke);
    ke.tab_id = p.tab_id;
    ke.type = @ptrCast(@constCast(switch (action) {
        .press => @as([*:0]const u8, "down"),
        .release => @as([*:0]const u8, "up"),
        .repeat => @as([*:0]const u8, "repeat"),
    }));
    ke.windows_key_code = @intCast(keyToWindowsVK(key));
    ke.modifiers = modifiers;

    var utf8_z: [33]u8 = undefined;
    if (utf8.len > 0 and utf8.len <= 32) {
        @memcpy(utf8_z[0..utf8.len], utf8);
        utf8_z[utf8.len] = 0;
        ke.utf8 = @ptrCast(&utf8_z);
    }

    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = 9; // KEY_EVENT
    wrapper.unnamed_0.key_event = &ke;
    sendProtobuf(server.fd, &wrapper);
}

/// Handle open_split: create a split with initial input (Issue 690).
fn handleOpenSplit(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const direction_str = str(xpc_dictionary_get_string(msg, "direction"));
    const command = xpc_dictionary_get_string(msg, "command") orelse {
        log.warn("open_split: missing command", .{});
        return;
    };

    const p = panes.get(pane_id) orelse {
        log.warn("open_split: no pane for {s}", .{pane_id});
        return;
    };
    const surface = p.overlay_surface orelse {
        log.warn("open_split: no surface for {s}", .{pane_id});
        return;
    };

    const direction: c_int = if (std.mem.eql(u8, direction_str, "right"))
        0 // right
    else if (std.mem.eql(u8, direction_str, "down"))
        1 // down
    else if (std.mem.eql(u8, direction_str, "left"))
        2 // left
    else if (std.mem.eql(u8, direction_str, "up"))
        3 // up
    else {
        log.warn("open_split: unknown direction {s}", .{direction_str});
        return;
    };

    log.info("open_split pane={s} dir={s}", .{ pane_id, direction_str });

    // CoreSurface → embedded Surface via @fieldParentPtr, then cast to opaque.
    const Embedded = @import("embedded.zig");
    const surface_ptr: *Embedded.Surface = @fieldParentPtr("core_surface", surface);

    // Dispatch to main thread — the split call triggers NotificationCenter →
    // termsurfDidNewSplit → replaceSurfaceTree → NSView mutations, which MUST
    // run on the main thread (Issue 690 Exp 2).
    const cmd = std.mem.span(command);
    const SplitReq = struct {
        surface: *anyopaque,
        direction: c_int,
        command_buf: [512]u8,
        command_len: usize,

        fn dispatch(raw: ?*anyopaque) callconv(.c) void {
            const self: *@This() = @ptrCast(@alignCast(raw));
            defer std.heap.c_allocator.destroy(self);
            termsurf_surface_split_with_input(
                self.surface,
                self.direction,
                @ptrCast(&self.command_buf),
            );
        }
    };
    const req = std.heap.c_allocator.create(SplitReq) catch return;
    const copy_len = @min(cmd.len, req.command_buf.len - 1);
    @memcpy(req.command_buf[0..copy_len], cmd[0..copy_len]);
    req.command_buf[copy_len] = 0;
    req.command_len = copy_len;
    req.surface = @ptrCast(surface_ptr);
    req.direction = direction;

    dispatch_async_f(@constCast(&_dispatch_main_q), @ptrCast(req), &SplitReq.dispatch);
}

/// Remove a half-created pane from all maps and free it (auto-target failure cleanup).
fn cleanupPane(pane_id_key: []const u8) void {
    if (panes.get(pane_id_key)) |p| {
        if (p.overlay_surface) |surface| {
            surface.clearOverlay();
            _ = surface_to_pane.remove(@intFromPtr(surface));
        }
        if (p.browser.len > 0) alloc.free(@constCast(p.browser));
        _ = panes.remove(pane_id_key);
        alloc.destroy(p);
        alloc.free(pane_id_key);
    }
}

// -- Helpers --

/// Free a heap-allocated key ([]const u8 from alloc.dupe). No-op for empty.
fn freeKey(key: []const u8) void {
    if (key.len > 0) {
        alloc.free(@constCast(key));
    }
}

/// Convert a nullable C string to a Zig slice, defaulting to "(null)".
fn str(ptr: ?[*:0]const u8) []const u8 {
    return if (ptr) |p| std.mem.span(p) else "(null)";
}

// -- Socket listener (Issue 700) --

/// Cast a protobuf-c C string (char*) to the sentinel pointer XPC expects.
inline fn pbStr(ptr: [*c]u8) [*:0]const u8 {
    if (ptr) |p| return @ptrCast(p);
    return "";
}

fn initSocket() void {
    const tmpdir = std.posix.getenv("TMPDIR") orelse "/tmp/";

    const pid = std.c.getpid();
    var name_buf: [64]u8 = undefined;
    const sock_name = std.fmt.bufPrintZ(&name_buf, "gui-{d}.sock", .{pid}) catch return;

    const path = std.fmt.bufPrintZ(&sock_path_buf, "{s}termsurf/{s}", .{
        tmpdir, sock_name,
    }) catch {
        log.err("socket path too long", .{});
        return;
    };
    sock_path_len = path.len;

    // Ensure the directory exists.
    var dir_buf: [256]u8 = undefined;
    const dir_path = std.fmt.bufPrintZ(&dir_buf, "{s}termsurf", .{tmpdir}) catch return;
    std.fs.cwd().makePath(dir_path) catch {};

    // Remove stale socket.
    std.posix.unlink(path) catch {};

    // Create, bind, listen.
    sock_fd = std.posix.socket(std.posix.AF.UNIX, std.posix.SOCK.STREAM, 0) catch |err| {
        log.err("socket() failed: {}", .{err});
        return;
    };

    const addr = std.net.Address.initUnix(path) catch |err| {
        log.err("initUnix failed: {}", .{err});
        std.posix.close(sock_fd);
        sock_fd = -1;
        return;
    };
    std.posix.bind(sock_fd, &addr.any, addr.getOsSockLen()) catch |err| {
        log.err("bind() failed: {}", .{err});
        std.posix.close(sock_fd);
        sock_fd = -1;
        return;
    };
    std.posix.listen(sock_fd, 8) catch |err| {
        log.err("listen() failed: {}", .{err});
        std.posix.close(sock_fd);
        sock_fd = -1;
        return;
    };

    // dispatch_source for accept on ipc_queue.
    sock_source = dispatch_source_create(
        @ptrCast(&_dispatch_source_type_read),
        @intCast(sock_fd),
        0,
        ipc_queue,
    );
    if (sock_source) |src| {
        dispatch_source_set_event_handler_f(src, &socketAcceptHandler);
        dispatch_resume(src);
    }

    log.info("socket listener ready at {s}", .{path});
}

fn socketAcceptHandler(_: ?*anyopaque) callconv(.c) void {
    // Accept new connection.
    const client_fd = std.posix.accept(sock_fd, null, null, 0) catch |err| {
        log.err("accept() failed: {}", .{err});
        return;
    };

    // Heap-allocate a new connection.
    const conn = alloc.create(ClientConn) catch {
        log.err("OOM allocating client, rejecting fd={}", .{client_fd});
        std.posix.close(client_fd);
        return;
    };
    conn.* = .{ .fd = client_fd };

    clients.append(alloc, conn) catch {
        log.err("OOM tracking client, rejecting fd={}", .{client_fd});
        std.posix.close(client_fd);
        alloc.destroy(conn);
        return;
    };

    // dispatch_source for reading from client, with per-connection context.
    conn.source = dispatch_source_create(
        @ptrCast(&_dispatch_source_type_read),
        @intCast(client_fd),
        0,
        ipc_queue,
    );
    if (conn.source) |src| {
        dispatch_set_context(src, conn);
        dispatch_source_set_event_handler_f(src, &socketReadHandler);
        dispatch_resume(src);
    }

    log.info("client connected fd={}", .{client_fd});
}

fn socketReadHandler(ctx: ?*anyopaque) callconv(.c) void {
    const conn: *ClientConn = @ptrCast(@alignCast(ctx orelse return));
    if (conn.fd < 0) return;

    const n = std.posix.read(conn.fd, conn.buf[conn.buf_len..]) catch {
        handleClientDisconnect(conn);
        return;
    };
    if (n == 0) {
        handleClientDisconnect(conn);
        return;
    }
    conn.buf_len += n;

    // Extract complete length-prefixed messages.
    while (conn.buf_len >= 4) {
        const msg_len: usize = @as(u32, @bitCast([4]u8{ conn.buf[0], conn.buf[1], conn.buf[2], conn.buf[3] }));
        if (conn.buf_len < 4 + msg_len) break;

        const pb_msg = pb.termsurf__term_surf_message__unpack(
            null,
            msg_len,
            @ptrCast(conn.buf[4..].ptr),
        );
        if (pb_msg) |msg| {
            handleSocketMessage(conn, msg);
            pb.termsurf__term_surf_message__free_unpacked(msg, null);
        }

        // Shift buffer.
        const consumed = 4 + msg_len;
        if (consumed < conn.buf_len) {
            std.mem.copyForwards(u8, conn.buf[0 .. conn.buf_len - consumed], conn.buf[consumed..conn.buf_len]);
        }
        conn.buf_len -= consumed;
    }
}

fn handleClientDisconnect(conn: *ClientConn) void {
    log.info("client disconnected fd={} type={s}", .{ conn.fd, @tagName(conn.conn_type) });

    // Cancel read source.
    if (conn.source) |src| {
        dispatch_source_cancel(src);
        conn.source = null;
    }

    if (conn.conn_type == .tui) {
        // Find and clean up all panes belonging to this TUI connection.
        var keys_buf: [64][]const u8 = undefined;
        var count: usize = 0;

        var it = panes.iterator();
        while (it.next()) |entry| {
            const p = entry.value_ptr.*;
            if (p.web_fd >= 0 and p.web_fd == conn.fd and count < keys_buf.len) {
                keys_buf[count] = entry.key_ptr.*;
                count += 1;
            }
        }

        for (0..count) |i| {
            const pane_id_key = keys_buf[i];
            if (panes.get(pane_id_key)) |p| {
                if (p.overlay_surface) |surface| {
                    surface.clearOverlay();
                    _ = surface_to_pane.remove(@intFromPtr(surface));
                }

                // Clear focused/last pane refs.
                if (focused_pane) |fp| {
                    if (std.mem.eql(u8, fp, pane_id_key)) focused_pane = null;
                }
                if (last_browser_pane) |lp| {
                    if (std.mem.eql(u8, lp, pane_id_key)) last_browser_pane = null;
                }

                // Clean up tab_to_pane.
                if (p.tab_id != 0) _ = tab_to_pane.remove(p.tab_id);

                // Close Chromium tab.
                if (p.server) |server| {
                    if (p.tab_sent and server.fd >= 0) {
                        var ct: pb.Termsurf__CloseTab = undefined;
                        pb.termsurf__close_tab__init(&ct);
                        ct.tab_id = p.tab_id;

                        var wrapper: pb.Termsurf__TermSurfMessage = undefined;
                        pb.termsurf__term_surf_message__init(&wrapper);
                        wrapper.msg_case = 4; // CLOSE_TAB
                        wrapper.unnamed_0.close_tab = &ct;
                        sendProtobuf(server.fd, &wrapper);
                    }

                    if (server.pane_count > 0) server.pane_count -= 1;
                    if (server.pane_count == 0) {
                        killServer(server);

                        // Cancel the Chromium ClientConn's dispatch source
                        // before freeing the server to prevent use-after-free
                        // when the dead socket's dispatch source fires.
                        for (clients.items, 0..) |c, idx| {
                            if (c.conn_type == .chromium and c.server == server) {
                                if (c.source) |src| dispatch_source_cancel(src);
                                if (c.fd >= 0) std.posix.close(c.fd);
                                _ = clients.swapRemove(idx);
                                alloc.destroy(c);
                                break;
                            }
                        }

                        _ = servers.remove(server.profile_key);
                        freeKey(server.profile_key);
                        if (server.browser.len > 0) alloc.free(server.browser);
                        alloc.destroy(server);
                    }
                }

                _ = panes.remove(pane_id_key);
                freeKey(p.pane_id_key);
                if (p.browser.len > 0) alloc.free(@constCast(p.browser));
                alloc.destroy(p);
            }
        }
    } else if (conn.conn_type == .chromium) {
        // Clear the server's socket fd.
        if (conn.server) |server| {
            server.fd = -1;
            log.info("chromium server disconnected profile={s} browser={s}", .{ serverProfile(server), server.browser });
        }
    }

    if (conn.fd >= 0) {
        std.posix.close(conn.fd);
    }

    // Remove from list and free.
    for (clients.items, 0..) |c, idx| {
        if (c == conn) {
            _ = clients.swapRemove(idx);
            break;
        }
    }
    alloc.destroy(conn);
}

// -- Socket message dispatch (Issue 700) --

fn handleSocketMessage(conn: *ClientConn, pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const case = pb_msg.msg_case;

    std.debug.print("[DEBUG] handleSocketMessage: case={} fd={} conn_type={s}\n", .{ case, conn.fd, @tagName(conn.conn_type) });

    // Connection type tagging: first message determines the type (Issue 701).
    if (conn.conn_type == .unknown) {
        if (case == 12) {
            // server_register → chromium connection
            conn.conn_type = .chromium;
        } else {
            conn.conn_type = .tui;
        }
        log.info("client fd={} tagged as {s}", .{ conn.fd, @tagName(conn.conn_type) });
    }

    if (case == 12) {
        // server_register (Chromium → GUI via socket, Issue 701)
        handleSocketServerRegister(conn, pb_msg);
    } else if (case == 19) {
        // set_overlay
        handleSocketSetOverlay(conn.fd, pb_msg);
    } else if (case == 20) {
        // set_devtools_overlay
        handleSocketSetDevtoolsOverlay(conn.fd, pb_msg);
    } else if (case == 5) {
        // navigate
        handleSocketNavigate(pb_msg);
    } else if (case == 11) {
        // set_color_scheme
        handleSocketSetColorScheme(pb_msg);
    } else if (case == 22) {
        // mode_changed
        handleSocketModeChanged(pb_msg);
    } else if (case == 21) {
        // open_split
        handleSocketOpenSplit(pb_msg);
    } else if (case == 23) {
        // hello_request
        handleSocketHello(conn.fd, pb_msg);
    } else if (case == 25) {
        // query_last_request
        handleSocketQueryLast(conn.fd, pb_msg);
    } else if (case == 27) {
        // query_devtools_request
        handleSocketQueryDevtools(conn.fd, pb_msg);
    } else if (case == 29) {
        // query_tabs_request
        handleSocketQueryTabs(conn.fd, pb_msg);
    } else if (case == 13) {
        // tab_ready (Chromium → GUI, Issue 701)
        handleSocketTabReady(pb_msg);
    } else if (case == 14) {
        // ca_context (Chromium → GUI, Issue 701)
        handleSocketCaContext(pb_msg);
    } else if (case == 15) {
        // url_changed (Chromium → GUI, Issue 701)
        handleSocketUrlChanged(pb_msg);
    } else if (case == 16) {
        // loading_state (Chromium → GUI, Issue 701)
        handleSocketLoadingState(pb_msg);
    } else if (case == 17) {
        // title_changed (Chromium → GUI, Issue 701)
        handleSocketTitleChanged(pb_msg);
    } else if (case == 18) {
        // cursor_changed (Chromium → GUI, Issue 701)
        handleSocketCursorChanged(pb_msg);
    } else {
        log.warn("unknown socket message case={}", .{case});
    }
}

/// ServerRegister from a Chromium server via socket (Issue 701).
/// The server sends its profile name. We match it to a server that was spawned
/// but hasn't registered yet (fd == -1) with a matching profile (Issue 704).
fn handleSocketServerRegister(conn: *ClientConn, pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const m: *pb.Termsurf__ServerRegister = pb_msg.unnamed_0.server_register orelse {
        std.debug.print("[DEBUG] handleSocketServerRegister: server_register field is null\n", .{});
        return;
    };
    const profile = std.mem.span(pbStr(m.profile));
    std.debug.print("[DEBUG] handleSocketServerRegister: profile={s} fd={}\n", .{ profile, conn.fd });
    log.info("socket server_register profile={s} fd={}", .{ profile, conn.fd });

    // Find a server with matching profile that hasn't registered yet.
    var server: ?*Server = null;
    var sit = servers.iterator();
    while (sit.next()) |entry| {
        const s = entry.value_ptr.*;
        if (s.fd == -1 and std.mem.eql(u8, serverProfile(s), profile)) {
            server = s;
            break;
        }
    }
    const srv = server orelse {
        std.debug.print("[DEBUG] handleSocketServerRegister: NO matching server for profile={s}\n", .{profile});
        log.warn("socket server_register for unknown profile={s}", .{profile});
        return;
    };

    // Store the socket fd on the server.
    srv.fd = conn.fd;
    conn.server = srv;
    std.debug.print("[DEBUG] handleSocketServerRegister: matched server key={s} fd={}\n", .{ srv.profile_key, conn.fd });

    // Flush all pending tabs for this server (mirrors handleServerRegister).
    var pending_count: u32 = 0;
    var it = panes.iterator();
    while (it.next()) |entry| {
        const p = entry.value_ptr.*;
        if (p.server == srv and !p.tab_sent) {
            if (p.inspected_tab_id > 0) {
                std.debug.print("[DEBUG] handleSocketServerRegister: flushing devtools tab pane={s}\n", .{p.pane_id_key});
                sendCreateDevToolsTab(p, srv);
            } else if (p.pending_url_len > 0) {
                std.debug.print("[DEBUG] handleSocketServerRegister: flushing tab pane={s} url={s}\n", .{ p.pane_id_key, p.pending_url_buf[0..p.pending_url_len] });
                sendCreateTab(p, srv);
            } else {
                continue;
            }
            pending_count += 1;
            if (p.browsing) {
                sendFocusChanged(p.pane_id_key, true);
            }
        }
    }
    std.debug.print("[DEBUG] handleSocketServerRegister: flushed {} pending tabs\n", .{pending_count});
}

/// Fire-and-forget: set_overlay via XPC-dict adapter, then set web_fd.
fn handleSocketSetOverlay(client_fd: std.posix.fd_t, pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const m: *pb.Termsurf__SetOverlay = pb_msg.unnamed_0.set_overlay orelse return;
    const pane_id_z = pbStr(m.pane_id);

    const dict = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(dict, "action", "set_overlay");
    xpc_dictionary_set_string(dict, "pane_id", pane_id_z);
    xpc_dictionary_set_string(dict, "url", pbStr(m.url));
    xpc_dictionary_set_string(dict, "profile", pbStr(m.profile));
    xpc_dictionary_set_string(dict, "browser", pbStr(m.browser));
    xpc_dictionary_set_uint64(dict, "col", m.col);
    xpc_dictionary_set_uint64(dict, "row", m.row);
    xpc_dictionary_set_uint64(dict, "width", m.width);
    xpc_dictionary_set_uint64(dict, "height", m.height);
    xpc_dictionary_set_bool(dict, "browsing", m.browsing != 0);
    handleMessage(dict);

    // Set web_fd on the pane.
    const pane_id = std.mem.span(pane_id_z);
    if (panes.get(pane_id)) |p| {
        p.web_fd = client_fd;
    }
}

/// Fire-and-forget: set_devtools_overlay via XPC-dict adapter, then set web_fd.
fn handleSocketSetDevtoolsOverlay(client_fd: std.posix.fd_t, pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const m: *pb.Termsurf__SetDevtoolsOverlay = pb_msg.unnamed_0.set_devtools_overlay orelse return;
    const pane_id_z = pbStr(m.pane_id);

    const dict = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(dict, "action", "set_devtools_overlay");
    xpc_dictionary_set_string(dict, "pane_id", pane_id_z);
    xpc_dictionary_set_string(dict, "profile", pbStr(m.profile));
    xpc_dictionary_set_string(dict, "browser", pbStr(m.browser));
    xpc_dictionary_set_uint64(dict, "col", m.col);
    xpc_dictionary_set_uint64(dict, "row", m.row);
    xpc_dictionary_set_uint64(dict, "width", m.width);
    xpc_dictionary_set_uint64(dict, "height", m.height);
    xpc_dictionary_set_bool(dict, "browsing", m.browsing != 0);
    xpc_dictionary_set_int64(dict, "inspected_tab_id", m.inspected_tab_id);
    handleMessage(dict);

    // Set web_fd on the pane.
    const pane_id = std.mem.span(pane_id_z);
    if (panes.get(pane_id)) |p| {
        p.web_fd = client_fd;
    }
}

/// Fire-and-forget: navigate via XPC-dict adapter.
fn handleSocketNavigate(pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const m: *pb.Termsurf__Navigate = pb_msg.unnamed_0.navigate orelse return;

    const dict = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(dict, "action", "navigate");
    xpc_dictionary_set_string(dict, "pane_id", pbStr(m.pane_id));
    xpc_dictionary_set_string(dict, "url", pbStr(m.url));
    handleMessage(dict);
}

/// Fire-and-forget: set_color_scheme via XPC-dict adapter.
fn handleSocketSetColorScheme(pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const m: *pb.Termsurf__SetColorScheme = pb_msg.unnamed_0.set_color_scheme orelse return;

    // The TUI sends dark (bool). Convert back to scheme string for the XPC handler.
    const scheme: [*:0]const u8 = if (m.dark != 0) "dark" else "light";

    const dict = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(dict, "action", "set_color_scheme");
    xpc_dictionary_set_string(dict, "pane_id", pbStr(m.pane_id));
    xpc_dictionary_set_string(dict, "scheme", scheme);
    handleMessage(dict);
}

/// Fire-and-forget: mode_changed via XPC-dict adapter.
fn handleSocketModeChanged(pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const m: *pb.Termsurf__ModeChanged = pb_msg.unnamed_0.mode_changed orelse return;

    const dict = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(dict, "action", "mode_changed");
    xpc_dictionary_set_string(dict, "pane_id", pbStr(m.pane_id));
    xpc_dictionary_set_bool(dict, "browsing", m.browsing != 0);
    handleMessage(dict);
}

/// Fire-and-forget: open_split via XPC-dict adapter.
fn handleSocketOpenSplit(pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const m: *pb.Termsurf__OpenSplit = pb_msg.unnamed_0.open_split orelse return;

    const dict = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(dict, "action", "open_split");
    xpc_dictionary_set_string(dict, "pane_id", pbStr(m.pane_id));
    xpc_dictionary_set_string(dict, "direction", pbStr(m.direction));
    xpc_dictionary_set_string(dict, "command", pbStr(m.command));
    handleMessage(dict);
}

/// Sync query: hello — look up homepage, reply with protobuf.
fn handleSocketHello(client_fd: std.posix.fd_t, pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const req: *pb.Termsurf__HelloRequest = pb_msg.unnamed_0.hello_request orelse return;
    const pane_id = std.mem.span(pbStr(req.pane_id));
    log.info("socket hello pane={s}", .{pane_id});

    // Look up surface to read config.
    var homepage: [*:0]const u8 = "";
    if (app.findSurfaceByPaneId(pane_id)) |surface| {
        homepage = surface.core().config.homepage;
    }

    // Build browsers list from registry (Issue 704).
    // Stack-allocated array of pointers (max 16 browsers).
    var browser_ptrs_buf: [16]?[*:0]u8 = .{null} ** 16;
    var n_browsers: usize = 0;
    var bit = browser_paths.iterator();
    while (bit.next()) |entry| {
        if (n_browsers >= browser_ptrs_buf.len) break;
        const name = alloc.dupeZ(u8, entry.key_ptr.*) catch continue;
        browser_ptrs_buf[n_browsers] = name;
        n_browsers += 1;
    }

    // Build protobuf reply.
    var reply: pb.Termsurf__HelloReply = undefined;
    pb.termsurf__hello_reply__init(&reply);
    reply.homepage = @ptrCast(@constCast(homepage));
    reply.n_browsers = n_browsers;
    reply.browsers = @ptrCast(&browser_ptrs_buf);

    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = @intCast(24); // HELLO_REPLY
    wrapper.unnamed_0.hello_reply = &reply;

    sendProtobuf(client_fd, &wrapper);

    // Free temporary browser name copies.
    for (browser_ptrs_buf[0..n_browsers]) |p| {
        if (p) |ptr| {
            const s = std.mem.span(ptr);
            alloc.free(s.ptr[0 .. s.len + 1]);
        }
    }
}

/// Sync query: query_last — find last browser pane, reply with protobuf.
fn handleSocketQueryLast(client_fd: std.posix.fd_t, pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const req: *pb.Termsurf__QueryLastRequest = pb_msg.unnamed_0.query_last_request orelse return;
    const profile_filter = std.mem.span(pbStr(req.profile));
    log.info("socket query_last profile_filter={s}", .{profile_filter});

    var reply: pb.Termsurf__QueryLastReply = undefined;
    pb.termsurf__query_last_reply__init(&reply);

    // Same logic as handleQueryLast.
    var target_pane: ?*Pane = null;
    var target_pane_id: []const u8 = "";

    if (last_browser_pane) |lpid| {
        if (panes.get(lpid)) |p| {
            if (profile_filter.len > 0 and !std.mem.eql(u8, profile_filter, "(null)")) {
                if (p.server) |s| {
                    if (std.mem.eql(u8, s.profile_key, profile_filter)) {
                        target_pane = p;
                        target_pane_id = lpid;
                    }
                }
            } else {
                target_pane = p;
                target_pane_id = lpid;
            }
        }
    }

    // Null-terminated copies for protobuf-c.
    var pane_z: [128]u8 = undefined;
    var prof_z: [128]u8 = undefined;

    if (target_pane) |p| {
        if (target_pane_id.len < pane_z.len) {
            @memcpy(pane_z[0..target_pane_id.len], target_pane_id);
            pane_z[target_pane_id.len] = 0;
            reply.pane_id = @ptrCast(&pane_z);
        }
        reply.tab_id = p.tab_id;
        if (p.server) |s| {
            if (s.profile_key.len < prof_z.len) {
                @memcpy(prof_z[0..s.profile_key.len], s.profile_key);
                prof_z[s.profile_key.len] = 0;
                reply.profile = @ptrCast(&prof_z);
            }
        }
    }

    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = @intCast(26); // QUERY_LAST_REPLY
    wrapper.unnamed_0.query_last_reply = &reply;

    sendProtobuf(client_fd, &wrapper);
}

/// Sync query: query_devtools — validate DevTools request, reply with protobuf.
fn handleSocketQueryDevtools(client_fd: std.posix.fd_t, pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const req: *pb.Termsurf__QueryDevtoolsRequest = pb_msg.unnamed_0.query_devtools_request orelse return;
    log.info("socket query_devtools inspected_tab_id={d}", .{req.inspected_tab_id});

    var reply: pb.Termsurf__QueryDevtoolsReply = undefined;
    pb.termsurf__query_devtools_reply__init(&reply);

    var resolved_tab_id: i64 = req.inspected_tab_id;

    // Auto-target (inspected_tab_id == 0).
    if (resolved_tab_id == 0) {
        const target_pane_id = last_browser_pane orelse {
            reply.@"error" = @ptrCast(@constCast(@as([*:0]const u8, "No browser tab found")));
            var wrapper: pb.Termsurf__TermSurfMessage = undefined;
            pb.termsurf__term_surf_message__init(&wrapper);
            wrapper.msg_case = @intCast(28);
            wrapper.unnamed_0.query_devtools_reply = &reply;
            sendProtobuf(client_fd, &wrapper);
            return;
        };
        const target = panes.get(target_pane_id) orelse {
            reply.@"error" = @ptrCast(@constCast(@as([*:0]const u8, "No browser tab found")));
            var wrapper: pb.Termsurf__TermSurfMessage = undefined;
            pb.termsurf__term_surf_message__init(&wrapper);
            wrapper.msg_case = @intCast(28);
            wrapper.unnamed_0.query_devtools_reply = &reply;
            sendProtobuf(client_fd, &wrapper);
            return;
        };
        if (target.tab_id == 0) {
            reply.@"error" = @ptrCast(@constCast(@as([*:0]const u8, "No browser tab found")));
            var wrapper: pb.Termsurf__TermSurfMessage = undefined;
            pb.termsurf__term_surf_message__init(&wrapper);
            wrapper.msg_case = @intCast(28);
            wrapper.unnamed_0.query_devtools_reply = &reply;
            sendProtobuf(client_fd, &wrapper);
            return;
        }
        resolved_tab_id = target.tab_id;
    }

    // Check for duplicate.
    var dup_it = panes.iterator();
    while (dup_it.next()) |entry| {
        const p = entry.value_ptr.*;
        if (p.inspected_tab_id == resolved_tab_id) {
            var err_buf: [128]u8 = undefined;
            const err_msg = std.fmt.bufPrintZ(&err_buf, "Tab {d} already has DevTools open", .{resolved_tab_id}) catch "DevTools already open";
            reply.@"error" = @ptrCast(@constCast(@as([*:0]const u8, @ptrCast(err_msg.ptr))));
            var wrapper: pb.Termsurf__TermSurfMessage = undefined;
            pb.termsurf__term_surf_message__init(&wrapper);
            wrapper.msg_case = @intCast(28);
            wrapper.unnamed_0.query_devtools_reply = &reply;
            sendProtobuf(client_fd, &wrapper);
            return;
        }
    }

    // Success — populate browser and profile from the inspected tab's server.
    reply.tab_id = resolved_tab_id;
    var browser_z: [128]u8 = undefined;
    var profile_z: [128]u8 = undefined;
    if (tab_to_pane.get(resolved_tab_id)) |resolved_pane_id| {
        if (panes.get(resolved_pane_id)) |resolved_pane| {
            if (resolved_pane.server) |s| {
                if (s.browser.len < browser_z.len) {
                    @memcpy(browser_z[0..s.browser.len], s.browser);
                    browser_z[s.browser.len] = 0;
                    reply.browser = @ptrCast(&browser_z);
                }
                // Extract profile from profile_key ("{profile}\x00{browser}").
                const key = s.profile_key;
                const sep = std.mem.indexOfScalar(u8, key, 0) orelse key.len;
                const prof = key[0..sep];
                if (prof.len < profile_z.len) {
                    @memcpy(profile_z[0..prof.len], prof);
                    profile_z[prof.len] = 0;
                    reply.profile = @ptrCast(&profile_z);
                }
            }
        }
    }
    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = @intCast(28); // QUERY_DEVTOOLS_REPLY
    wrapper.unnamed_0.query_devtools_reply = &reply;
    sendProtobuf(client_fd, &wrapper);
}

/// Sync query: query_tabs — count GUI panes, forward to Chromium, reply with protobuf.
fn handleSocketQueryTabs(client_fd: std.posix.fd_t, pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const req: *pb.Termsurf__QueryTabsRequest = pb_msg.unnamed_0.query_tabs_request orelse return;
    const profile_str = std.mem.span(pbStr(req.profile));
    log.info("socket query_tabs profile={s}", .{profile_str});

    var reply: pb.Termsurf__QueryTabsReply = undefined;
    pb.termsurf__query_tabs_reply__init(&reply);

    // Count GUI panes.
    var gui_pane_count: i64 = 0;
    {
        var it = panes.iterator();
        while (it.next()) |entry| {
            const p = entry.value_ptr.*;
            if (p.server) |s| {
                if (std.mem.eql(u8, s.profile_key, profile_str)) {
                    gui_pane_count += 1;
                }
            }
        }
    }
    reply.gui_panes = gui_pane_count;

    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = @intCast(30); // QUERY_TABS_REPLY
    wrapper.unnamed_0.query_tabs_reply = &reply;
    sendProtobuf(client_fd, &wrapper);
}

// -- Chromium → GUI socket handlers (Issue 701) --
// XPC-dict adapter pattern: build an XPC dict and call handleMessage().

fn handleSocketTabReady(pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const m: *pb.Termsurf__TabReady = pb_msg.unnamed_0.tab_ready orelse return;
    const dict = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(dict, "action", "tab_ready");
    xpc_dictionary_set_string(dict, "pane_id", pbStr(m.pane_id));
    xpc_dictionary_set_int64(dict, "tab_id", m.tab_id);
    handleMessage(dict);
}

fn handleSocketCaContext(pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const m: *pb.Termsurf__CaContext = pb_msg.unnamed_0.ca_context orelse return;
    const dict = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(dict, "action", "ca_context");
    xpc_dictionary_set_int64(dict, "tab_id", m.tab_id);
    xpc_dictionary_set_uint64(dict, "ca_context_id", m.ca_context_id);
    xpc_dictionary_set_uint64(dict, "pixel_width", m.pixel_width);
    xpc_dictionary_set_uint64(dict, "pixel_height", m.pixel_height);
    handleMessage(dict);
}

fn handleSocketUrlChanged(pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const m: *pb.Termsurf__UrlChanged = pb_msg.unnamed_0.url_changed orelse return;
    const dict = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(dict, "action", "url_changed");
    xpc_dictionary_set_int64(dict, "tab_id", m.tab_id);
    xpc_dictionary_set_string(dict, "url", pbStr(m.url));
    handleMessage(dict);
}

fn handleSocketLoadingState(pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const m: *pb.Termsurf__LoadingState = pb_msg.unnamed_0.loading_state orelse return;
    const dict = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(dict, "action", "loading_state");
    xpc_dictionary_set_int64(dict, "tab_id", m.tab_id);
    xpc_dictionary_set_string(dict, "state", pbStr(m.state));
    xpc_dictionary_set_uint64(dict, "progress", m.progress);
    handleMessage(dict);
}

fn handleSocketTitleChanged(pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const m: *pb.Termsurf__TitleChanged = pb_msg.unnamed_0.title_changed orelse return;
    const dict = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(dict, "action", "title_changed");
    xpc_dictionary_set_int64(dict, "tab_id", m.tab_id);
    xpc_dictionary_set_string(dict, "title", pbStr(m.title));
    handleMessage(dict);
}

fn handleSocketCursorChanged(pb_msg: *pb.Termsurf__TermSurfMessage) void {
    const m: *pb.Termsurf__CursorChanged = pb_msg.unnamed_0.cursor_changed orelse return;
    const dict = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(dict, "action", "cursor_changed");
    xpc_dictionary_set_int64(dict, "tab_id", m.tab_id);
    xpc_dictionary_set_int64(dict, "cursor_type", m.cursor_type);
    handleMessage(dict);
}

/// Serialize and send a length-prefixed protobuf message over a socket.
fn sendProtobuf(fd: std.posix.fd_t, wrapper: *pb.Termsurf__TermSurfMessage) void {
    const size = pb.termsurf__term_surf_message__get_packed_size(wrapper);
    if (size > 4096) {
        log.warn("protobuf message too large: {}", .{size});
        return;
    }

    var buf: [4100]u8 = undefined;

    // 4-byte LE length prefix.
    const len_u32: u32 = @intCast(size);
    buf[0] = @intCast(len_u32 & 0xFF);
    buf[1] = @intCast((len_u32 >> 8) & 0xFF);
    buf[2] = @intCast((len_u32 >> 16) & 0xFF);
    buf[3] = @intCast((len_u32 >> 24) & 0xFF);

    // Pack the message.
    _ = pb.termsurf__term_surf_message__pack(wrapper, @ptrCast(buf[4..].ptr));

    // Write atomically.
    _ = std.posix.write(fd, buf[0 .. 4 + size]) catch {};
}

/// Issue 699: Force the linker to include protobuf-c objects.
pub fn testProtobuf() void {
    var msg: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&msg);
}
