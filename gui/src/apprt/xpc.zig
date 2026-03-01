// XPC communication for TermSurf Ghost (Issues 601–604).
//
// All XPC event handlers run on a serial dispatch queue (`xpc_queue`).
// No mutexes needed for XPC state — serialization is guaranteed by GCD.
// The renderer's `draw_mutex` is separate and protects renderer state.
//
// Manual extern declarations instead of @cImport because the XPC header uses
// C block types that Zig's translate-c may not handle.

const std = @import("std");
const builtin = @import("builtin");
const objc = @import("objc");
const CoreApp = @import("../App.zig");
const CoreSurface = @import("../Surface.zig");
const input = @import("../input.zig");

const internal_os = @import("../os/main.zig");
const log = std.log.scoped(.xpc);
const alloc = std.heap.page_allocator;

// -- XPC C API --

const xpc_object_t = ?*anyopaque;

extern "c" fn xpc_connection_create_mach_service(name: [*:0]const u8, targetq: xpc_object_t, flags: u64) xpc_object_t;
extern "c" fn xpc_connection_set_event_handler(connection: xpc_object_t, handler: xpc_object_t) void;
extern "c" fn xpc_connection_resume(connection: xpc_object_t) void;
extern "c" fn xpc_connection_cancel(connection: xpc_object_t) void;
extern "c" fn xpc_connection_send_message(connection: xpc_object_t, message: xpc_object_t) void;
extern "c" fn xpc_connection_send_message_with_reply_sync(connection: xpc_object_t, message: xpc_object_t) xpc_object_t;
extern "c" fn xpc_connection_create(name: ?[*:0]const u8, targetq: xpc_object_t) xpc_object_t;
extern "c" fn xpc_endpoint_create(connection: xpc_object_t) xpc_object_t;
extern "c" fn xpc_dictionary_create(keys: xpc_object_t, values: xpc_object_t, count: usize) xpc_object_t;
extern "c" fn xpc_dictionary_set_string(xdict: xpc_object_t, key: [*:0]const u8, string: [*:0]const u8) void;
extern "c" fn xpc_dictionary_set_value(xdict: xpc_object_t, key: [*:0]const u8, value: xpc_object_t) void;
extern "c" fn xpc_dictionary_get_string(xdict: xpc_object_t, key: [*:0]const u8) ?[*:0]const u8;
extern "c" fn xpc_dictionary_get_uint64(xdict: xpc_object_t, key: [*:0]const u8) u64;
extern "c" fn xpc_dictionary_get_bool(xdict: xpc_object_t, key: [*:0]const u8) bool;
extern "c" fn xpc_dictionary_set_uint64(xdict: xpc_object_t, key: [*:0]const u8, value: u64) void;
extern "c" fn xpc_dictionary_set_int64(xdict: xpc_object_t, key: [*:0]const u8, value: i64) void;
extern "c" fn xpc_dictionary_get_int64(xdict: xpc_object_t, key: [*:0]const u8) i64;
extern "c" fn xpc_dictionary_set_double(xdict: xpc_object_t, key: [*:0]const u8, value: f64) void;
extern "c" fn xpc_dictionary_set_bool(xdict: xpc_object_t, key: [*:0]const u8, value: bool) void;
extern "c" fn xpc_dictionary_get_remote_connection(msg: xpc_object_t) xpc_object_t;
extern "c" fn xpc_dictionary_create_reply(original: xpc_object_t) xpc_object_t;
extern "c" fn xpc_get_type(object: xpc_object_t) xpc_object_t;
extern "c" fn xpc_retain(object: xpc_object_t) xpc_object_t;
extern "c" fn xpc_release(object: xpc_object_t) void;

// XPC type/error constants (compared by address identity).
extern const _xpc_type_connection: anyopaque;
extern const _xpc_type_error: anyopaque;
extern const _xpc_type_dictionary: anyopaque;
extern const _xpc_error_connection_invalid: anyopaque;

// -- Dispatch queue C API --

extern "c" fn dispatch_queue_create(label: [*:0]const u8, attr: ?*anyopaque) ?*anyopaque;
extern "c" fn dispatch_async_f(queue: ?*anyopaque, context: ?*anyopaque, work: *const fn (?*anyopaque) callconv(.c) void) void;
extern const _dispatch_main_q: anyopaque;
extern "c" fn xpc_connection_set_target_queue(connection: xpc_object_t, queue: ?*anyopaque) void;

/// Cast a const extern symbol address to xpc_object_t for identity comparison.
inline fn xpcPtr(ptr: *const anyopaque) xpc_object_t {
    return @constCast(ptr);
}

// -- Data structures --

/// Per-pane state. No mutex — all access is on the serial `xpc_queue`.
const Pane = struct {
    web_peer: xpc_object_t = null,
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
};

/// Per-profile server state. Shared by all panes on the same profile.
const Server = struct {
    process: ?std.process.Child = null,
    peer: xpc_object_t = null,
    profile_key: []const u8 = "", // heap-allocated, also the key in `servers`
    pane_count: usize = 0,
};

// -- Module state --

var app: *CoreApp = undefined;
var gateway: xpc_object_t = null;
var listener: xpc_object_t = null;
var xpc_queue: ?*anyopaque = null;

/// Active panes, keyed by pane UUID string.
var panes: std.StringHashMap(*Pane) = undefined;

/// Active servers, keyed by profile name.
var servers: std.StringHashMap(*Server) = undefined;

/// Reverse lookup: connection address (usize) → pane UUID string.
var peer_to_pane: std.AutoHashMap(usize, []const u8) = undefined;

/// Reverse lookup: connection address (usize) → profile name.
var peer_to_profile: std.AutoHashMap(usize, []const u8) = undefined;

/// Reverse lookup: CoreSurface pointer address → pane UUID string (Issue 606).
var surface_to_pane: std.AutoHashMap(usize, []const u8) = undefined;

/// The pane UUID that currently has Chromium focus (at most one). Issue 606.
var focused_pane: ?[]const u8 = null;

/// The most recently active browser pane — updated on tab creation and focus (Issue 684).
var last_browser_pane: ?[]const u8 = null;

// -- Block types --

/// Block with no captures (gateway, listener handlers).
const EventBlock = objc.Block(struct {}, .{xpc_object_t}, void);

/// Block with captured peer address (per-peer handler for disconnect ID).
const PeerContext = struct { peer_addr: usize };
const PeerBlock = objc.Block(PeerContext, .{xpc_object_t}, void);

// -- Public API --

pub fn init(core_app: *CoreApp) void {
    app = core_app;
    log.info("connecting to xpc-gateway", .{});

    // Serial dispatch queue — all XPC handlers run here, no mutexes needed.
    xpc_queue = dispatch_queue_create("com.termsurf.ghost.xpc", null);

    // Initialize maps.
    panes = std.StringHashMap(*Pane).init(alloc);
    servers = std.StringHashMap(*Server).init(alloc);
    peer_to_pane = std.AutoHashMap(usize, []const u8).init(alloc);
    peer_to_profile = std.AutoHashMap(usize, []const u8).init(alloc);
    surface_to_pane = std.AutoHashMap(usize, []const u8).init(alloc);

    // Connect to the xpc-gateway Mach service.
    // Debug and release builds use separate gateways so they don't interfere (Issue 653).
    const xpc_service_name = if (comptime builtin.mode == .Debug)
        "com.termsurf.debug.xpc-gateway"
    else
        "com.termsurf.xpc-gateway";
    gateway = xpc_connection_create_mach_service(xpc_service_name, null, 0);
    xpc_connection_set_target_queue(gateway, xpc_queue);
    var gw_block = EventBlock.init(.{}, &gatewayHandler);
    xpc_connection_set_event_handler(gateway, @ptrCast(&gw_block));
    xpc_connection_resume(gateway);

    // Create anonymous listener for direct peer connections.
    listener = xpc_connection_create(null, null);
    xpc_connection_set_target_queue(listener, xpc_queue);
    var listener_block = EventBlock.init(.{}, &listenerHandler);
    xpc_connection_set_event_handler(listener, @ptrCast(&listener_block));
    xpc_connection_resume(listener);

    // Register endpoint with the gateway.
    const endpoint = xpc_endpoint_create(listener);
    const msg = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(msg, "action", "register_app");
    xpc_dictionary_set_value(msg, "endpoint", endpoint);
    xpc_connection_send_message(gateway, msg);

    log.info("registered endpoint with xpc-gateway (serial queue)", .{});

    // Debug builds set TERMSURF_XPC_SERVICE so child terminal sessions
    // (and the `web` TUI) know which gateway to connect to (Issue 653).
    if (comptime builtin.mode == .Debug) {
        _ = internal_os.setenv("TERMSURF_XPC_SERVICE", "com.termsurf.debug.xpc-gateway");
    }
}

pub fn deinit() void {
    // Clean up all panes.
    var pane_it = panes.iterator();
    while (pane_it.next()) |entry| {
        const p = entry.value_ptr.*;
        if (p.overlay_surface) |surface| surface.clearOverlay();
        if (p.web_peer) |peer| xpc_release(peer);
        freeKey(p.pane_id_key);
        alloc.destroy(p);
    }
    panes.deinit();

    // Clean up all servers.
    var server_it = servers.iterator();
    while (server_it.next()) |entry| {
        const s = entry.value_ptr.*;
        killServer(s);
        if (s.peer) |peer| xpc_release(peer);
        freeKey(s.profile_key);
        alloc.destroy(s);
    }
    servers.deinit();

    peer_to_pane.deinit();
    peer_to_profile.deinit();
    surface_to_pane.deinit();

    if (listener != null) {
        xpc_connection_cancel(listener);
        listener = null;
    }
    if (gateway != null) {
        xpc_connection_cancel(gateway);
        gateway = null;
    }
    log.info("xpc connections closed", .{});
}

// -- Event handlers --

fn gatewayHandler(_: *const EventBlock.Context, event: xpc_object_t) callconv(.c) void {
    if (xpc_get_type(event) == xpcPtr(&_xpc_type_error)) {
        log.err("gateway connection error", .{});
    }
}

fn listenerHandler(_: *const EventBlock.Context, event: xpc_object_t) callconv(.c) void {
    if (xpc_get_type(event) == xpcPtr(&_xpc_type_connection)) {
        log.info("peer connected", .{});

        // Each peer gets a block that captures its connection address,
        // so handleDisconnect can identify which peer disconnected.
        const peer_addr = @intFromPtr(event.?);
        var peer_block = PeerBlock.init(.{ .peer_addr = peer_addr }, &peerHandler);
        xpc_connection_set_event_handler(event, @ptrCast(&peer_block));
        xpc_connection_set_target_queue(event, xpc_queue);
        xpc_connection_resume(event);
    }
}

fn peerHandler(ctx: *const PeerBlock.Context, event: xpc_object_t) callconv(.c) void {
    if (xpc_get_type(event) == xpcPtr(&_xpc_type_dictionary)) {
        handleMessage(event);
    } else if (xpc_get_type(event) == xpcPtr(&_xpc_type_error)) {
        if (event == xpcPtr(&_xpc_error_connection_invalid)) {
            handleDisconnect(ctx.peer_addr);
        }
    }
}

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
    } else if (std.mem.eql(u8, action_str, "server_register")) {
        handleServerRegister(msg);
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
    } else if (std.mem.eql(u8, action_str, "hello")) {
        handleHello(msg);
    } else if (std.mem.eql(u8, action_str, "query_last")) {
        handleQueryLast(msg);
    } else if (std.mem.eql(u8, action_str, "query_devtools")) {
        handleQueryDevtools(msg);
    } else if (std.mem.eql(u8, action_str, "query_tabs")) {
        handleQueryTabs(msg);
    } else {
        log.warn("unknown action: {s}", .{action_str});
    }
}

fn handleSetOverlay(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const url = str(xpc_dictionary_get_string(msg, "url"));
    const profile = str(xpc_dictionary_get_string(msg, "profile"));
    const col = xpc_dictionary_get_uint64(msg, "col");
    const row = xpc_dictionary_get_uint64(msg, "row");
    const width = xpc_dictionary_get_uint64(msg, "width");
    const height = xpc_dictionary_get_uint64(msg, "height");
    const browsing = xpc_dictionary_get_bool(msg, "browsing");

    log.info("set_overlay pane={s} col={} row={} w={} h={} url={s} profile={s} browsing={}", .{
        pane_id, col, row, width, height, url, profile, browsing,
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
                if (server.peer != null and (new_pixel_w != old_w or new_pixel_h != old_h)) {
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

        // Retain and register web peer for disconnect identification.
        const conn = xpc_dictionary_get_remote_connection(msg);
        if (conn != null) {
            p.web_peer = xpc_retain(conn);
            peer_to_pane.put(@intFromPtr(conn.?), pane_id_key) catch {};
        }

        log.info("new pane={s} pixel={d}x{d}", .{
            pane_id, new_pixel_w, new_pixel_h,
        });

        // Get or create server for this profile.
        if (getOrCreateServer(profile)) |server| {
            p.server = server;
            server.pane_count += 1;

            if (server.peer != null) {
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
    const col = xpc_dictionary_get_uint64(msg, "col");
    const row = xpc_dictionary_get_uint64(msg, "row");
    const width = xpc_dictionary_get_uint64(msg, "width");
    const height = xpc_dictionary_get_uint64(msg, "height");
    const browsing = xpc_dictionary_get_bool(msg, "browsing");
    const inspected_tab_id = xpc_dictionary_get_int64(msg, "inspected_tab_id");

    log.info("set_devtools_overlay pane={s} inspected_tab_id={d} profile={s}", .{
        pane_id, inspected_tab_id, profile,
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
                if (server.peer != null and (new_pixel_w != old_w or new_pixel_h != old_h)) {
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

        const conn = xpc_dictionary_get_remote_connection(msg);
        if (conn != null) {
            p.web_peer = xpc_retain(conn);
            peer_to_pane.put(@intFromPtr(conn.?), pane_id_key) catch {};
        }

        // Auto-target: resolve inspected_tab_id from last focused browser pane (Issue 684 Exp 3).
        if (p.inspected_tab_id == 0) {
            const target_pane_id = last_browser_pane orelse {
                log.err("devtools auto-target: no browser pane has been focused", .{});
                cleanupPane(pane_id_key);
                return;
            };
            const target = panes.get(target_pane_id) orelse {
                log.err("devtools auto-target: pane {s} not found", .{target_pane_id});
                cleanupPane(pane_id_key);
                return;
            };
            if (target.tab_id == 0) {
                log.err("devtools auto-target: target pane {s} has no tab_id yet", .{target_pane_id});
                cleanupPane(pane_id_key);
                return;
            }
            p.inspected_tab_id = target.tab_id;
            log.info("devtools auto-target: resolved pane={s} tab_id={d}", .{
                target_pane_id, target.tab_id,
            });
        }

        log.info("new devtools pane={s} pixel={d}x{d} inspected_tab_id={d}", .{
            pane_id, new_pixel_w, new_pixel_h, p.inspected_tab_id,
        });

        // Use the target's server (profile) when auto-targeting, not the --profile argument.
        if (p.inspected_tab_id != inspected_tab_id) {
            // Auto-targeted — use the target pane's server.
            const target_pane_id = last_browser_pane.?;
            const target = panes.get(target_pane_id).?;
            if (target.server) |target_server| {
                p.server = target_server;
                target_server.pane_count += 1;

                if (target_server.peer != null) {
                    sendCreateDevToolsTab(p, target_server);
                    if (p.browsing) {
                        sendFocusChanged(p.pane_id_key, true);
                    }
                }
            }
        } else if (getOrCreateServer(profile)) |server| {
            p.server = server;
            server.pane_count += 1;

            if (server.peer != null) {
                sendCreateDevToolsTab(p, server);
                if (p.browsing) {
                    sendFocusChanged(p.pane_id_key, true);
                }
            }
        }
    }
}

fn handleServerRegister(msg: xpc_object_t) void {
    const profile = str(xpc_dictionary_get_string(msg, "profile"));
    log.info("server_register profile={s}", .{profile});

    const server = servers.get(profile) orelse {
        log.warn("server_register for unknown profile={s}", .{profile});
        return;
    };

    // Retain and store server peer.
    const conn = xpc_dictionary_get_remote_connection(msg);
    if (conn != null) {
        server.peer = xpc_retain(conn);
        peer_to_profile.put(@intFromPtr(conn.?), server.profile_key) catch {};
    }

    // Flush all pending tabs for this server.
    var it = panes.iterator();
    while (it.next()) |entry| {
        const p = entry.value_ptr.*;
        if (p.server == server and !p.tab_sent) {
            if (p.inspected_tab_id > 0) {
                // DevTools pane (Issue 684).
                sendCreateDevToolsTab(p, server);
            } else if (p.pending_url_len > 0) {
                sendCreateTab(p, server);
            } else {
                continue;
            }
            if (p.browsing) {
                sendFocusChanged(p.pane_id_key, true);
            }
        }
    }
}

fn handleCAContext(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const context_id: u32 = @intCast(xpc_dictionary_get_uint64(msg, "ca_context_id"));

    log.info("ca_context pane={s} context_id={}", .{ pane_id, context_id });

    // Guard against zero context ID (Issue 630, fix G3).
    if (context_id == 0) return;

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
        if (tab_id > 0) {
            last_browser_pane = p.pane_id_key; // heap-allocated, stable
        }
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
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const cursor_type = xpc_dictionary_get_int64(msg, "cursor_type");

    if (panes.get(pane_id)) |p| {
        if (p.overlay_surface) |surface| {
            surface.overlay_cursor_type = cursor_type;
        }
    }
}

fn handleLoadingState(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const p = panes.get(pane_id) orelse return;
    if (p.web_peer == null) return;

    const state = xpc_dictionary_get_string(msg, "state") orelse return;
    const progress = xpc_dictionary_get_uint64(msg, "progress");

    const fwd = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(fwd, "action", "loading_state");
    xpc_dictionary_set_string(fwd, "state", state);
    xpc_dictionary_set_uint64(fwd, "progress", progress);
    xpc_connection_send_message(p.web_peer, fwd);
}

fn handleUrlChanged(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const p = panes.get(pane_id) orelse return;
    if (p.web_peer == null) return;

    const url = xpc_dictionary_get_string(msg, "url") orelse return;

    const fwd = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(fwd, "action", "url_changed");
    xpc_dictionary_set_string(fwd, "url", url);
    xpc_connection_send_message(p.web_peer, fwd);
}

fn handleTitleChanged(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const p = panes.get(pane_id) orelse return;
    if (p.web_peer == null) return;

    const title = xpc_dictionary_get_string(msg, "title") orelse return;

    const fwd = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(fwd, "action", "title_changed");
    xpc_dictionary_set_string(fwd, "title", title);
    xpc_connection_send_message(p.web_peer, fwd);
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
    if (server.peer == null) return;

    const fwd = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(fwd, "action", "navigate");

    // Null-terminate pane_id.
    var pane_z: [37]u8 = undefined;
    if (p.pane_id_key.len > 0 and p.pane_id_key.len <= 36) {
        @memcpy(pane_z[0..p.pane_id_key.len], p.pane_id_key);
        pane_z[p.pane_id_key.len] = 0;
        xpc_dictionary_set_string(fwd, "pane_id", @ptrCast(&pane_z));
    }

    // Null-terminate URL.
    var url_z: [2049]u8 = undefined;
    if (url.len > 0 and url.len < url_z.len) {
        @memcpy(url_z[0..url.len], url);
        url_z[url.len] = 0;
        xpc_dictionary_set_string(fwd, "url", @ptrCast(&url_z));
    }

    xpc_connection_send_message(server.peer, fwd);
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
    if (server.peer == null) return;

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

    // Forward to Chromium.
    const fwd = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(fwd, "action", "set_color_scheme");

    var pane_z: [37]u8 = undefined;
    if (p.pane_id_key.len > 0 and p.pane_id_key.len <= 36) {
        @memcpy(pane_z[0..p.pane_id_key.len], p.pane_id_key);
        pane_z[p.pane_id_key.len] = 0;
        xpc_dictionary_set_string(fwd, "pane_id", @ptrCast(&pane_z));
    }

    xpc_dictionary_set_bool(fwd, "dark", dark);
    xpc_connection_send_message(server.peer, fwd);
    log.info("forwarded set_color_scheme pane={s} dark={}", .{ pane_id, dark });
}

fn handleHello(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    log.info("hello pane={s}", .{pane_id});

    const reply = xpc_dictionary_create_reply(msg);
    if (reply == null) return;

    // Look up surface to read its config.
    if (app.findSurfaceByPaneId(pane_id)) |surface| {
        const homepage = surface.core().config.homepage;
        xpc_dictionary_set_string(reply, "homepage", homepage);
    }

    const conn = xpc_dictionary_get_remote_connection(msg);
    if (conn != null) {
        xpc_connection_send_message(conn, reply);
    }
}

fn handleQueryLast(msg: xpc_object_t) void {
    const profile_filter = str(xpc_dictionary_get_string(msg, "profile"));
    log.info("query_last profile_filter={s}", .{profile_filter});

    const reply = xpc_dictionary_create_reply(msg);
    if (reply == null) return;

    // If a profile filter is given, check that last_browser_pane matches.
    // Otherwise use the global last_browser_pane.
    var target_pane: ?*Pane = null;
    var target_pane_id: []const u8 = "";

    if (last_browser_pane) |lpid| {
        if (panes.get(lpid)) |p| {
            if (profile_filter.len > 0 and !std.mem.eql(u8, profile_filter, "(null)")) {
                // Profile-filtered: check the pane's server matches.
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

    if (target_pane) |p| {
        // Null-terminate the pane_id and profile for XPC string fields.
        var pane_id_z: [128]u8 = undefined;
        if (target_pane_id.len < pane_id_z.len) {
            @memcpy(pane_id_z[0..target_pane_id.len], target_pane_id);
            pane_id_z[target_pane_id.len] = 0;
            xpc_dictionary_set_string(reply, "pane_id", @ptrCast(&pane_id_z));
        }
        xpc_dictionary_set_int64(reply, "tab_id", p.tab_id);
        if (p.server) |s| {
            var prof_z: [128]u8 = undefined;
            if (s.profile_key.len < prof_z.len) {
                @memcpy(prof_z[0..s.profile_key.len], s.profile_key);
                prof_z[s.profile_key.len] = 0;
                xpc_dictionary_set_string(reply, "profile", @ptrCast(&prof_z));
            }
        }
        log.info("query_last result: pane={s} tab_id={d}", .{ target_pane_id, p.tab_id });
    } else {
        log.info("query_last result: no matching browser pane", .{});
    }

    const conn = xpc_dictionary_get_remote_connection(msg);
    if (conn != null) {
        xpc_connection_send_message(conn, reply);
    }
}

/// Synchronous query: validate a DevTools request before the TUI launches (Issue 687).
/// Resolves auto-target, checks for duplicate DevTools on the same tab, and
/// replies with the resolved tab_id or an error string.
fn handleQueryDevtools(msg: xpc_object_t) void {
    const inspected_tab_id = xpc_dictionary_get_int64(msg, "inspected_tab_id");
    const profile_str = str(xpc_dictionary_get_string(msg, "profile"));
    log.info("query_devtools inspected_tab_id={d} profile={s}", .{ inspected_tab_id, profile_str });

    const reply = xpc_dictionary_create_reply(msg);
    if (reply == null) return;

    var resolved_tab_id: i64 = inspected_tab_id;

    // Resolve auto-target (inspected_tab_id == 0).
    if (resolved_tab_id == 0) {
        const target_pane_id = last_browser_pane orelse {
            xpc_dictionary_set_string(reply, "error", "No browser tab found");
            const conn = xpc_dictionary_get_remote_connection(msg);
            if (conn != null) xpc_connection_send_message(conn, reply);
            log.info("query_devtools: no last_browser_pane", .{});
            return;
        };
        const target = panes.get(target_pane_id) orelse {
            xpc_dictionary_set_string(reply, "error", "No browser tab found");
            const conn = xpc_dictionary_get_remote_connection(msg);
            if (conn != null) xpc_connection_send_message(conn, reply);
            log.info("query_devtools: target pane not found", .{});
            return;
        };
        if (target.tab_id == 0) {
            xpc_dictionary_set_string(reply, "error", "No browser tab found");
            const conn = xpc_dictionary_get_remote_connection(msg);
            if (conn != null) xpc_connection_send_message(conn, reply);
            log.info("query_devtools: target pane has no tab_id", .{});
            return;
        }
        resolved_tab_id = target.tab_id;
    }

    // Check for duplicate: any existing pane already inspecting this tab?
    var it = panes.iterator();
    while (it.next()) |entry| {
        const p = entry.value_ptr.*;
        if (p.inspected_tab_id == resolved_tab_id) {
            // Format error message with the tab ID.
            var err_buf: [64]u8 = undefined;
            const err_msg = std.fmt.bufPrint(&err_buf, "Tab {d} already has DevTools open", .{resolved_tab_id}) catch "DevTools already open for this tab";
            // Null-terminate for XPC.
            var err_z: [128]u8 = undefined;
            if (err_msg.len < err_z.len) {
                @memcpy(err_z[0..err_msg.len], err_msg);
                err_z[err_msg.len] = 0;
                xpc_dictionary_set_string(reply, "error", @ptrCast(&err_z));
            }
            const conn = xpc_dictionary_get_remote_connection(msg);
            if (conn != null) xpc_connection_send_message(conn, reply);
            log.info("query_devtools: duplicate — tab {d} already has devtools", .{resolved_tab_id});
            return;
        }
    }

    // Success: reply with resolved tab_id.
    xpc_dictionary_set_int64(reply, "tab_id", resolved_tab_id);
    const conn = xpc_dictionary_get_remote_connection(msg);
    if (conn != null) xpc_connection_send_message(conn, reply);
    log.info("query_devtools: ok tab_id={d}", .{resolved_tab_id});
}

/// Synchronous query: return Chromium tab inventory for a profile (Issue 689).
/// Counts GUI panes, forwards query to the Chromium profile server, and
/// combines both into the reply sent back to the TUI.
fn handleQueryTabs(msg: xpc_object_t) void {
    const profile_str = str(xpc_dictionary_get_string(msg, "profile"));
    log.info("query_tabs profile={s}", .{profile_str});

    const reply = xpc_dictionary_create_reply(msg);
    if (reply == null) return;

    // Count GUI panes for this profile.
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
    xpc_dictionary_set_int64(reply, "gui_panes", gui_pane_count);

    // Look up the profile server.
    const server = servers.get(profile_str);
    if (server == null or server.?.peer == null) {
        // No server running for this profile — return zeros.
        xpc_dictionary_set_int64(reply, "chromium_tabs", 0);
        xpc_dictionary_set_int64(reply, "chromium_browser", 0);
        xpc_dictionary_set_int64(reply, "chromium_devtools", 0);
        const conn = xpc_dictionary_get_remote_connection(msg);
        if (conn != null) xpc_connection_send_message(conn, reply);
        log.info("query_tabs: no server for profile={s}", .{profile_str});
        return;
    }

    // Forward synchronous query to Chromium.
    const fwd = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(fwd, "action", "query_tabs");
    const chromium_reply = xpc_connection_send_message_with_reply_sync(server.?.peer, fwd);
    xpc_release(fwd);

    if (chromium_reply != null) {
        // Copy Chromium's reply fields into the TUI reply.
        const chromium_tabs = xpc_dictionary_get_int64(chromium_reply, "chromium_tabs");
        const chromium_browser = xpc_dictionary_get_int64(chromium_reply, "chromium_browser");
        const chromium_devtools = xpc_dictionary_get_int64(chromium_reply, "chromium_devtools");
        xpc_dictionary_set_int64(reply, "chromium_tabs", chromium_tabs);
        xpc_dictionary_set_int64(reply, "chromium_browser", chromium_browser);
        xpc_dictionary_set_int64(reply, "chromium_devtools", chromium_devtools);

        // Copy per-tab summary strings (tab_0, tab_1, ...).
        var i: i64 = 0;
        while (i < chromium_tabs) : (i += 1) {
            var key_buf: [16]u8 = undefined;
            const key = std.fmt.bufPrint(&key_buf, "tab_{d}", .{i}) catch continue;
            // Null-terminate the key.
            if (key.len < key_buf.len) {
                key_buf[key.len] = 0;
                const key_z: [*:0]const u8 = @ptrCast(&key_buf);
                const val = xpc_dictionary_get_string(chromium_reply, key_z);
                if (val != null) {
                    xpc_dictionary_set_string(reply, key_z, val.?);
                }
            }
        }
        xpc_release(chromium_reply);
    } else {
        xpc_dictionary_set_int64(reply, "chromium_tabs", 0);
        xpc_dictionary_set_int64(reply, "chromium_browser", 0);
        xpc_dictionary_set_int64(reply, "chromium_devtools", 0);
    }

    const conn = xpc_dictionary_get_remote_connection(msg);
    if (conn != null) xpc_connection_send_message(conn, reply);
    log.info("query_tabs: replied gui_panes={d}", .{gui_pane_count});
}

// -- Focus lifecycle (Issue 606 Experiment 5) --

/// Send focus_changed to Chromium, enforcing single-pane focus.
fn sendFocusChanged(pane_id: []const u8, focused: bool) void {
    const p = panes.get(pane_id) orelse return;
    const server = p.server orelse return;
    if (server.peer == null) return;

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
    if (server.peer == null) return;

    const msg = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(msg, "action", "focus_changed");

    if (pane_id.len > 0 and pane_id.len <= 36) {
        var pane_z: [37]u8 = undefined;
        @memcpy(pane_z[0..pane_id.len], pane_id);
        pane_z[pane_id.len] = 0;
        xpc_dictionary_set_string(msg, "pane_id", @ptrCast(&pane_z));
    }

    xpc_dictionary_set_bool(msg, "focused", focused);
    xpc_connection_send_message(server.peer, msg);
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
    dispatch_async_f(xpc_queue, @ptrFromInt(encoded), dispatch_fn);
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
    if (p.web_peer == null) return;
    const msg = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(msg, "action", "mode_changed");
    xpc_dictionary_set_bool(msg, "browsing", browsing);
    xpc_connection_send_message(p.web_peer, msg);
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

// -- Server lifecycle --

fn getOrCreateServer(profile: []const u8) ?*Server {
    if (servers.get(profile)) |server| {
        return server;
    }

    // Create new server for this profile.
    const profile_key = alloc.dupe(u8, profile) catch return null;
    const server = alloc.create(Server) catch {
        alloc.free(profile_key);
        return null;
    };
    server.* = .{ .profile_key = profile_key };
    servers.put(profile_key, server) catch {
        alloc.free(profile_key);
        alloc.destroy(server);
        return null;
    };

    spawnServerProcess(server);
    return server;
}

fn spawnServerProcess(server: *Server) void {
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

    // Resolution order: bundle → env var → dev fallback.
    var path_buf: [std.fs.max_path_bytes]u8 = undefined;
    const server_path = blk: {
        // 1. Check inside app bundle (release/install builds).
        var exe_buf: [std.fs.max_path_bytes]u8 = undefined;
        if (std.fs.selfExePath(&exe_buf)) |exe| {
            // Walk up 3 components: termsurf → MacOS → Contents → bundle root.
            var dir: []const u8 = exe;
            var i: usize = 0;
            while (i < 3) : (i += 1) {
                dir = std.fs.path.dirname(dir) orelse break;
            }
            if (i == 3) {
                const helpers_path = std.fmt.bufPrintZ(
                    &path_buf,
                    "{s}/Contents/Chromium/Chromium Profile Server.app/Contents/MacOS/Chromium Profile Server",
                    .{dir},
                ) catch null;
                if (helpers_path) |p| {
                    if (std.fs.accessAbsolute(p[0..p.len], .{})) {
                        break :blk helpers_path;
                    } else |_| {}
                }
            }
        } else |_| {}
        // 2. Check environment variable override.
        if (std.posix.getenv("TERMSURF_CHROMIUM_SERVER")) |p| {
            break :blk std.fmt.bufPrintZ(&path_buf, "{s}", .{p}) catch null;
        }
        // 3. Dev fallback.
        break :blk std.fmt.bufPrintZ(
            &path_buf,
            "{s}/dev/termsurf/chromium/src/out/Default/Chromium Profile Server.app/Contents/MacOS/Chromium Profile Server",
            .{home},
        ) catch null;
    } orelse {
        log.err("server path too long", .{});
        return;
    };

    var xpc_arg_buf: [128]u8 = undefined;
    const xpc_arg = std.fmt.bufPrintZ(
        &xpc_arg_buf,
        "--xpc-service=" ++ (if (comptime builtin.mode == .Debug) "com.termsurf.debug.xpc-gateway" else "com.termsurf.xpc-gateway"),
        .{},
    ) catch return;

    var data_arg_buf: [512]u8 = undefined;
    const data_arg = std.fmt.bufPrintZ(
        &data_arg_buf,
        "--user-data-dir={s}/termsurf/" ++ (if (comptime builtin.mode == .Debug) "debug/" else "") ++ "chromium-profiles/{s}",
        .{ data_home, server.profile_key },
    ) catch {
        log.err("data dir path too long", .{});
        return;
    };

    var hidden_buf: [16]u8 = undefined;
    const hidden_arg = std.fmt.bufPrintZ(&hidden_buf, "--hidden", .{}) catch return;

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

    log.info("spawning server profile={s}", .{server.profile_key});

    var child = std.process.Child.init(
        &.{ server_path, xpc_arg, data_arg, hidden_arg, logging_arg, logfile_arg },
        alloc,
    );
    child.spawn() catch |err| {
        log.err("failed to spawn server: {}", .{err});
        return;
    };

    server.process = child;
    log.info("server spawned pid={d} profile={s}", .{ child.id, server.profile_key });
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
    const msg = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(msg, "action", "create_tab");

    // URL (null-terminated).
    var url_z: [2049]u8 = undefined;
    if (p.pending_url_len > 0 and p.pending_url_len < url_z.len) {
        @memcpy(url_z[0..p.pending_url_len], p.pending_url_buf[0..p.pending_url_len]);
        url_z[p.pending_url_len] = 0;
        xpc_dictionary_set_string(msg, "url", @ptrCast(&url_z));
    }

    // Pane ID (null-terminated).
    if (p.pane_id_key.len > 0 and p.pane_id_key.len <= 36) {
        var pane_z: [37]u8 = undefined;
        @memcpy(pane_z[0..p.pane_id_key.len], p.pane_id_key);
        pane_z[p.pane_id_key.len] = 0;
        xpc_dictionary_set_string(msg, "pane_id", @ptrCast(&pane_z));
    }

    xpc_dictionary_set_uint64(msg, "pixel_width", p.pending_pixel_w);
    xpc_dictionary_set_uint64(msg, "pixel_height", p.pending_pixel_h);

    // Color scheme (Issue 680).
    const dark: bool = if (p.overlay_surface) |surface|
        surface.config_conditional_state.theme == .dark
    else
        true; // default to dark
    xpc_dictionary_set_bool(msg, "dark", dark);

    xpc_connection_send_message(server.peer, msg);
    p.tab_sent = true;

    log.info("sent create_tab pane={s} pixel={d}x{d} dark={}", .{
        p.pane_id_key, p.pending_pixel_w, p.pending_pixel_h, dark,
    });
}

fn sendCreateDevToolsTab(p: *Pane, server: *Server) void {
    const msg = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(msg, "action", "create_devtools_tab");

    // Pane ID (null-terminated).
    if (p.pane_id_key.len > 0 and p.pane_id_key.len <= 36) {
        var pane_z: [37]u8 = undefined;
        @memcpy(pane_z[0..p.pane_id_key.len], p.pane_id_key);
        pane_z[p.pane_id_key.len] = 0;
        xpc_dictionary_set_string(msg, "pane_id", @ptrCast(&pane_z));
    }

    xpc_dictionary_set_int64(msg, "inspected_tab_id", p.inspected_tab_id);
    xpc_dictionary_set_uint64(msg, "pixel_width", p.pending_pixel_w);
    xpc_dictionary_set_uint64(msg, "pixel_height", p.pending_pixel_h);

    // Color scheme (Issue 680).
    const dark: bool = if (p.overlay_surface) |surface|
        surface.config_conditional_state.theme == .dark
    else
        true; // default to dark
    xpc_dictionary_set_bool(msg, "dark", dark);

    xpc_connection_send_message(server.peer, msg);
    p.tab_sent = true;

    log.info("sent create_devtools_tab pane={s} inspected_tab_id={d} dark={}", .{
        p.pane_id_key, p.inspected_tab_id, dark,
    });
}

fn sendResize(p: *Pane, server: *Server) void {
    const msg = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(msg, "action", "resize");

    if (p.pane_id_key.len > 0 and p.pane_id_key.len <= 36) {
        var pane_z: [37]u8 = undefined;
        @memcpy(pane_z[0..p.pane_id_key.len], p.pane_id_key);
        pane_z[p.pane_id_key.len] = 0;
        xpc_dictionary_set_string(msg, "pane_id", @ptrCast(&pane_z));
    }

    xpc_dictionary_set_uint64(msg, "pixel_width", p.pending_pixel_w);
    xpc_dictionary_set_uint64(msg, "pixel_height", p.pending_pixel_h);

    xpc_connection_send_message(server.peer, msg);
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
            if (server.peer == null) return;

            const msg = xpc_dictionary_create(null, null, 0);
            xpc_dictionary_set_string(msg, "action", "set_color_scheme");

            var pane_z: [37]u8 = undefined;
            if (p.pane_id_key.len > 0 and p.pane_id_key.len <= 36) {
                @memcpy(pane_z[0..p.pane_id_key.len], p.pane_id_key);
                pane_z[p.pane_id_key.len] = 0;
                xpc_dictionary_set_string(msg, "pane_id", @ptrCast(&pane_z));
            }

            xpc_dictionary_set_bool(msg, "dark", is_dark);
            xpc_connection_send_message(server.peer, msg);
            log.info("sent set_color_scheme pane={s} dark={}", .{ pane_id, is_dark });
        }
    }.f;
    // Encode dark state in low bit of pointer (Surface is aligned).
    const encoded = ptr_val | @as(usize, if (dark) 1 else 0);
    dispatch_async_f(xpc_queue, @ptrFromInt(encoded), dispatch_fn);
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
    if (server.peer == null) return;

    const msg = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(msg, "action", "mouse_event");

    // Pane ID (null-terminated).
    if (pane_id_key.len > 0 and pane_id_key.len <= 36) {
        var pane_z: [37]u8 = undefined;
        @memcpy(pane_z[0..pane_id_key.len], pane_id_key);
        pane_z[pane_id_key.len] = 0;
        xpc_dictionary_set_string(msg, "pane_id", @ptrCast(&pane_z));
    }

    // Event type: down or up.
    xpc_dictionary_set_string(msg, "type", switch (action) {
        .press => "down",
        .release => "up",
    });

    // Button name.
    xpc_dictionary_set_string(msg, "button", switch (button) {
        .left => "left",
        .right => "right",
        .middle => "middle",
        else => "left",
    });

    // Overlay-relative logical coordinates.
    xpc_dictionary_set_double(msg, "x", overlay_x);
    xpc_dictionary_set_double(msg, "y", overlay_y);

    // Click count (1=single, 2=double/word select, 3=triple/line select).
    xpc_dictionary_set_int64(msg, "click_count", @intCast(click_count));

    // Modifier bitmask: shift=1, ctrl=2, alt=4, cmd=8.
    // For mouse down, also set button-down flags: left=64, right=256.
    var modifiers: u64 = 0;
    if (mods.shift) modifiers |= 1;
    if (mods.ctrl) modifiers |= 2;
    if (mods.alt) modifiers |= 4;
    if (mods.super) modifiers |= 8;
    if (action == .press) {
        if (button == .left) modifiers |= 64; // 1 << 6
        if (button == .right) modifiers |= 256; // 1 << 8
    }
    xpc_dictionary_set_uint64(msg, "modifiers", modifiers);

    xpc_connection_send_message(server.peer, msg);
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
    if (server.peer == null) return;

    const raw = surface.raw_scroll;

    const msg = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(msg, "action", "scroll_event");

    // Pane ID (null-terminated).
    if (pane_id_key.len > 0 and pane_id_key.len <= 36) {
        var pane_z: [37]u8 = undefined;
        @memcpy(pane_z[0..pane_id_key.len], pane_id_key);
        pane_z[pane_id_key.len] = 0;
        xpc_dictionary_set_string(msg, "pane_id", @ptrCast(&pane_z));
    }

    // Overlay-relative logical coordinates.
    xpc_dictionary_set_double(msg, "x", overlay_x);
    xpc_dictionary_set_double(msg, "y", overlay_y);

    // Raw NSEvent scroll deltas (unmodified).
    xpc_dictionary_set_double(msg, "delta_x", raw.delta_x);
    xpc_dictionary_set_double(msg, "delta_y", raw.delta_y);

    // Raw NSEvent phase bitmasks.
    xpc_dictionary_set_uint64(msg, "phase", raw.phase);
    xpc_dictionary_set_uint64(msg, "momentum_phase", raw.momentum_phase);

    // Precision flag (trackpad vs mouse wheel).
    xpc_dictionary_set_bool(msg, "precise", raw.precise);

    // Modifiers (not typically used for scroll events).
    xpc_dictionary_set_uint64(msg, "modifiers", 0);

    xpc_connection_send_message(server.peer, msg);
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
    if (server.peer == null) return;

    const msg = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(msg, "action", "mouse_move");

    // Pane ID (null-terminated).
    if (pane_id_key.len > 0 and pane_id_key.len <= 36) {
        var pane_z: [37]u8 = undefined;
        @memcpy(pane_z[0..pane_id_key.len], pane_id_key);
        pane_z[pane_id_key.len] = 0;
        xpc_dictionary_set_string(msg, "pane_id", @ptrCast(&pane_z));
    }

    // Overlay-relative logical coordinates.
    xpc_dictionary_set_double(msg, "x", overlay_x);
    xpc_dictionary_set_double(msg, "y", overlay_y);

    // Button-down flags from click_state (for drag vs hover distinction).
    var modifiers: u64 = 0;
    const left_idx = @intFromEnum(input.MouseButton.left);
    const right_idx = @intFromEnum(input.MouseButton.right);
    if (surface.mouse.click_state[left_idx] == .press) modifiers |= 64; // kLeftButtonDown (1 << 6)
    if (surface.mouse.click_state[right_idx] == .press) modifiers |= 256; // kRightButtonDown (1 << 8)
    xpc_dictionary_set_uint64(msg, "modifiers", modifiers);

    xpc_connection_send_message(server.peer, msg);
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
    if (server.peer == null) return;

    const msg = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(msg, "action", "key_event");

    // Pane ID (null-terminated).
    if (pane_id_key.len > 0 and pane_id_key.len <= 36) {
        var pane_z: [37]u8 = undefined;
        @memcpy(pane_z[0..pane_id_key.len], pane_id_key);
        pane_z[pane_id_key.len] = 0;
        xpc_dictionary_set_string(msg, "pane_id", @ptrCast(&pane_z));
    }

    // Key action type.
    xpc_dictionary_set_string(msg, "type", switch (action) {
        .press => "down",
        .release => "up",
        .repeat => "repeat",
    });

    // Windows virtual key code.
    xpc_dictionary_set_int64(msg, "windows_key_code", @intCast(keyToWindowsVK(key)));

    // UTF-8 text generated by the keystroke (null-terminated).
    if (utf8.len > 0 and utf8.len <= 32) {
        var utf8_z: [33]u8 = undefined;
        @memcpy(utf8_z[0..utf8.len], utf8);
        utf8_z[utf8.len] = 0;
        xpc_dictionary_set_string(msg, "utf8", @ptrCast(&utf8_z));
    } else {
        xpc_dictionary_set_string(msg, "utf8", "");
    }

    // Modifier bitmask: shift=1, ctrl=2, alt=4, meta=8.
    var modifiers: u64 = 0;
    if (mods.shift) modifiers |= 1;
    if (mods.ctrl) modifiers |= 2;
    if (mods.alt) modifiers |= 4;
    if (mods.super) modifiers |= 8;
    xpc_dictionary_set_uint64(msg, "modifiers", modifiers);

    xpc_connection_send_message(server.peer, msg);
}

/// Remove a half-created pane from all maps and free it (auto-target failure cleanup).
fn cleanupPane(pane_id_key: []const u8) void {
    if (panes.get(pane_id_key)) |p| {
        if (p.overlay_surface) |surface| {
            surface.clearOverlay();
            _ = surface_to_pane.remove(@intFromPtr(surface));
        }
        if (p.web_peer) |peer| {
            _ = peer_to_pane.remove(@intFromPtr(peer));
            xpc_release(peer);
        }
        _ = panes.remove(pane_id_key);
        alloc.destroy(p);
        alloc.free(pane_id_key);
    }
}

// -- Disconnect handling --

fn handleDisconnect(peer_addr: usize) void {
    // Check if this is a web peer.
    if (peer_to_pane.get(peer_addr)) |pane_id_key| {
        log.info("web peer disconnected pane={s}", .{pane_id_key});

        if (panes.get(pane_id_key)) |p| {
            if (p.overlay_surface) |surface| {
                surface.clearOverlay();
                _ = surface_to_pane.remove(@intFromPtr(surface));
            }

            // Clear focused_pane if this pane had focus (Issue 606).
            if (focused_pane) |fp| {
                if (std.mem.eql(u8, fp, pane_id_key)) {
                    focused_pane = null;
                }
            }

            // Clear last_browser_pane if this pane was it (Issue 684).
            if (last_browser_pane) |lp| {
                if (std.mem.eql(u8, lp, pane_id_key)) {
                    last_browser_pane = null;
                }
            }

            // Decrement server pane count; kill if last.
            if (p.server) |server| {
                if (server.pane_count > 0) server.pane_count -= 1;

                if (server.pane_count == 0) {
                    killServer(server);
                    if (server.peer) |sp| {
                        _ = peer_to_profile.remove(@intFromPtr(sp));
                        xpc_release(sp);
                    }
                    _ = servers.remove(server.profile_key);
                    freeKey(server.profile_key);
                    alloc.destroy(server);
                }
            }

            if (p.web_peer) |peer| xpc_release(peer);
            _ = panes.remove(pane_id_key);
            _ = peer_to_pane.remove(peer_addr);
            freeKey(p.pane_id_key);
            alloc.destroy(p);
        } else {
            _ = peer_to_pane.remove(peer_addr);
        }
        return;
    }

    // Check if this is a server peer.
    if (peer_to_profile.get(peer_addr)) |profile_key| {
        log.info("server peer disconnected profile={s}", .{profile_key});

        if (servers.get(profile_key)) |server| {
            // Collect pane keys to remove (can't mutate map during iteration).
            var keys_buf: [64][]const u8 = undefined;
            var addrs_buf: [64]usize = undefined;
            var count: usize = 0;

            var it = panes.iterator();
            while (it.next()) |entry| {
                const p = entry.value_ptr.*;
                if (p.server == server and count < keys_buf.len) {
                    if (p.overlay_surface) |surface| {
                        surface.clearOverlay();
                        _ = surface_to_pane.remove(@intFromPtr(surface));
                    }
                    addrs_buf[count] = if (p.web_peer) |wp| @intFromPtr(wp) else 0;
                    if (p.web_peer) |wp| xpc_release(wp);
                    keys_buf[count] = entry.key_ptr.*;
                    count += 1;
                    alloc.destroy(p);
                }
            }

            for (0..count) |i| {
                _ = panes.remove(keys_buf[i]);
                if (addrs_buf[i] != 0) _ = peer_to_pane.remove(addrs_buf[i]);
                freeKey(keys_buf[i]);
            }

            if (server.peer) |sp| xpc_release(sp);
            _ = servers.remove(profile_key);
            _ = peer_to_profile.remove(peer_addr);
            freeKey(server.profile_key);
            alloc.destroy(server);
        } else {
            _ = peer_to_profile.remove(peer_addr);
        }
        return;
    }

    log.info("unknown peer disconnected", .{});
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
