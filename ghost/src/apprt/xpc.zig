// XPC communication for TermSurf Ghost (Issues 601–604).
//
// All XPC event handlers run on a serial dispatch queue (`xpc_queue`).
// No mutexes needed for XPC state — serialization is guaranteed by GCD.
// The renderer's `draw_mutex` is separate and protects renderer state.
//
// Manual extern declarations instead of @cImport because the XPC header uses
// C block types that Zig's translate-c may not handle.

const std = @import("std");
const objc = @import("objc");
const CoreApp = @import("../App.zig");
const CoreSurface = @import("../Surface.zig");
const input = @import("../input.zig");

const log = std.log.scoped(.xpc);
const alloc = std.heap.page_allocator;

// -- XPC C API --

const xpc_object_t = ?*anyopaque;

extern "c" fn xpc_connection_create_mach_service(name: [*:0]const u8, targetq: xpc_object_t, flags: u64) xpc_object_t;
extern "c" fn xpc_connection_set_event_handler(connection: xpc_object_t, handler: xpc_object_t) void;
extern "c" fn xpc_connection_resume(connection: xpc_object_t) void;
extern "c" fn xpc_connection_cancel(connection: xpc_object_t) void;
extern "c" fn xpc_connection_send_message(connection: xpc_object_t, message: xpc_object_t) void;
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
extern "c" fn xpc_dictionary_set_double(xdict: xpc_object_t, key: [*:0]const u8, value: f64) void;
extern "c" fn xpc_dictionary_set_bool(xdict: xpc_object_t, key: [*:0]const u8, value: bool) void;
extern "c" fn xpc_dictionary_get_remote_connection(msg: xpc_object_t) xpc_object_t;
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
extern "c" fn xpc_connection_set_target_queue(connection: xpc_object_t, queue: ?*anyopaque) void;

// -- Mach port / IOSurface C API --

extern "c" fn xpc_dictionary_copy_mach_send(xdict: xpc_object_t, key: [*:0]const u8) u32;
extern "c" fn IOSurfaceLookupFromMachPort(port: u32) ?*anyopaque;
extern "c" fn mach_port_deallocate(task: u32, name: u32) i32;
extern const mach_task_self_: u32;
extern "c" fn CFRelease(cf: *anyopaque) void;

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
    gateway = xpc_connection_create_mach_service("com.termsurf.xpc-gateway", null, 0);
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
    } else if (std.mem.eql(u8, action_str, "server_register")) {
        handleServerRegister(msg);
    } else if (std.mem.eql(u8, action_str, "display_surface")) {
        handleDisplaySurface(msg);
    } else if (std.mem.eql(u8, action_str, "tab_ready")) {
        handleTabReady(msg);
    } else if (std.mem.eql(u8, action_str, "mode_changed")) {
        handleModeChanged(msg);
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
        if (p.server == server and !p.tab_sent and p.pending_url_len > 0) {
            sendCreateTab(p, server);
        }
    }
}

fn handleDisplaySurface(msg: xpc_object_t) void {
    const port = xpc_dictionary_copy_mach_send(msg, "iosurface_port");
    if (port == 0) return;

    const iosurface = IOSurfaceLookupFromMachPort(port) orelse {
        _ = mach_port_deallocate(mach_task_self_, port);
        return;
    };
    _ = mach_port_deallocate(mach_task_self_, port);

    // Route by pane_id.
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    if (panes.get(pane_id)) |p| {
        if (p.overlay_surface) |surface| {
            surface.setOverlayIOSurface(iosurface);
        }
    }

    // IOSurfaceLookupFromMachPort returns +1 ref; setOverlayIOSurface
    // CFRetains, so we CFRelease our lookup reference.
    CFRelease(iosurface);
}

fn handleTabReady(msg: xpc_object_t) void {
    const tab_id = str(xpc_dictionary_get_string(msg, "tab_id"));
    log.info("tab_ready tab_id={s}", .{tab_id});
}

fn handleModeChanged(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const browsing = xpc_dictionary_get_bool(msg, "browsing");

    log.info("mode_changed pane={s} browsing={}", .{ pane_id, browsing });
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

    var path_buf: [512]u8 = undefined;
    const server_path = std.fmt.bufPrintZ(
        &path_buf,
        "{s}/dev/termsurf/chromium/src/out/Default/Chromium Profile Server.app/Contents/MacOS/Chromium Profile Server",
        .{home},
    ) catch {
        log.err("server path too long", .{});
        return;
    };

    var xpc_arg_buf: [128]u8 = undefined;
    const xpc_arg = std.fmt.bufPrintZ(
        &xpc_arg_buf,
        "--xpc-service=com.termsurf.xpc-gateway",
        .{},
    ) catch return;

    var data_arg_buf: [512]u8 = undefined;
    const data_arg = std.fmt.bufPrintZ(
        &data_arg_buf,
        "--user-data-dir={s}/.config/termsurf/chromium-profiles/{s}",
        .{ home, server.profile_key },
    ) catch {
        log.err("data dir path too long", .{});
        return;
    };

    var hidden_buf: [16]u8 = undefined;
    const hidden_arg = std.fmt.bufPrintZ(&hidden_buf, "--hidden", .{}) catch return;

    log.info("spawning server profile={s}", .{server.profile_key});

    var child = std.process.Child.init(
        &.{ server_path, xpc_arg, data_arg, hidden_arg },
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

    xpc_connection_send_message(server.peer, msg);
    p.tab_sent = true;

    log.info("sent create_tab pane={s} pixel={d}x{d}", .{
        p.pane_id_key, p.pending_pixel_w, p.pending_pixel_h,
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

    // Click count (1 for now — double-click support in a later experiment).
    xpc_dictionary_set_int64(msg, "click_count", 1);

    // Modifier bitmask: shift=1, ctrl=2, alt=4, cmd=8.
    // For mouse down, also set button-down flags: left=64, right=256.
    var modifiers: i64 = 0;
    if (mods.shift) modifiers |= 1;
    if (mods.ctrl) modifiers |= 2;
    if (mods.alt) modifiers |= 4;
    if (mods.super) modifiers |= 8;
    if (action == .press) {
        if (button == .left) modifiers |= 64; // 1 << 6
        if (button == .right) modifiers |= 256; // 1 << 8
    }
    xpc_dictionary_set_int64(msg, "modifiers", modifiers);

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
