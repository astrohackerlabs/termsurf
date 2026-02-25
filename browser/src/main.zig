// Zig launcher for the Chromium Profile Server.
//
// dlopen's the framework, resolves C API symbols, registers lifecycle
// callbacks, and calls ContentMain. Creates WebContents directly (no Shell
// window) and receives the CAContext ID via callback.

const std = @import("std");

// -- C API (manual extern declarations, following gui/src/apprt/xpc.zig pattern) --

const RTLD_LAZY: c_int = 0x1;
const RTLD_LOCAL: c_int = 0x4;
const RTLD_FIRST: c_int = 0x100;

extern "c" fn dlopen(path: [*:0]const u8, mode: c_int) ?*anyopaque;
extern "c" fn dlsym(handle: *anyopaque, symbol: [*:0]const u8) ?*anyopaque;
extern "c" fn dlerror() ?[*:0]const u8;
extern "c" fn getenv(name: [*:0]const u8) ?[*:0]const u8;

// _NSGetExecutablePath: first call with buf_size=0 returns -1 and sets buf_size.
extern "c" fn _NSGetExecutablePath(buf: ?[*]u8, buf_size: *u32) c_int;

// dirname modifies the input buffer and returns a pointer into it.
extern "c" fn dirname(path: [*:0]u8) ?[*:0]u8;

// -- Function pointer types matching content_api_shim.h --

const ContentMainFn = *const fn (c_int, [*]const [*:0]const u8) callconv(.c) c_int;
const CallbackFn = *const fn () callconv(.c) void;
const SetCallbackFn = *const fn (CallbackFn) callconv(.c) void;
const CreateContextFn = *const fn ([*:0]const u8) callconv(.c) ?*anyopaque;
const DestroyContextFn = *const fn (?*anyopaque) callconv(.c) void;
const CreateTabFn = *const fn (?*anyopaque, [*:0]const u8) callconv(.c) void;
const CreateWebContentsFn = *const fn (?*anyopaque, [*:0]const u8, c_int, c_int) callconv(.c) ?*anyopaque;
const DestroyWebContentsFn = *const fn (?*anyopaque) callconv(.c) void;
const SetViewSizeFn = *const fn (?*anyopaque, c_int, c_int) callconv(.c) void;
const CAContextCallbackFn = *const fn (?*anyopaque, u32) callconv(.c) void;
const SetOnCAContextChangedFn = *const fn (CAContextCallbackFn) callconv(.c) void;

// -- dlsym'd function pointers (set in main, used by callbacks) --

var g_create_ctx: ?CreateContextFn = null;
var g_destroy_ctx: ?DestroyContextFn = null;
var g_create_tab: ?CreateTabFn = null;
var g_create_web_contents: ?CreateWebContentsFn = null;
var g_destroy_web_contents: ?DestroyWebContentsFn = null;

// -- Context and WebContents handles --

var ctx_a: ?*anyopaque = null;
var wc_a: ?*anyopaque = null;

// -- Lifecycle callbacks --

fn onInitialized() callconv(.c) void {
    const home = getenv("HOME") orelse {
        std.debug.print("HOME not set\n", .{});
        std.process.abort();
    };

    var path_buf: [1024]u8 = undefined;
    const path = std.fmt.bufPrintZ(&path_buf, "{s}/.config/termsurf/zig-profile-server/default", .{home}) catch {
        std.debug.print("path too long\n", .{});
        std.process.abort();
    };

    ctx_a = g_create_ctx.?(path.ptr);
    wc_a = g_create_web_contents.?(ctx_a, "https://google.com", 1280, 720);
}

fn onShutdown() callconv(.c) void {
    if (wc_a != null) {
        g_destroy_web_contents.?(wc_a);
        wc_a = null;
    }
    g_destroy_ctx.?(ctx_a);
    ctx_a = null;
}

fn onCAContextChanged(wc: ?*anyopaque, ca_context_id: u32) callconv(.c) void {
    _ = wc;
    std.debug.print("ca_context_id={d}\n", .{ca_context_id});
}

// -- Symbol resolution helper --

fn resolveSymbol(comptime T: type, handle: *anyopaque, name: [*:0]const u8) T {
    const ptr = dlsym(handle, name) orelse {
        const err = dlerror() orelse "unknown error";
        std.debug.print("dlsym {s}: {s}\n", .{ name, err });
        std.process.abort();
    };
    return @ptrCast(@alignCast(ptr));
}

// -- Entry point --

pub fn main() void {
    // Find the executable path.
    var exec_path_size: u32 = 0;
    _ = _NSGetExecutablePath(null, &exec_path_size);

    const exec_path = std.heap.page_allocator.allocSentinel(u8, exec_path_size, 0) catch {
        std.debug.print("alloc failed\n", .{});
        std.process.abort();
    };

    if (_NSGetExecutablePath(exec_path.ptr, &exec_path_size) != 0) {
        std.debug.print("_NSGetExecutablePath failed\n", .{});
        std.process.abort();
    }

    // Framework path relative to the executable:
    //   <MacOS>/../Frameworks/Zig Profile Server Framework.framework/
    //   Zig Profile Server Framework
    const parent_dir = dirname(exec_path.ptr) orelse {
        std.debug.print("dirname failed\n", .{});
        std.process.abort();
    };

    var fw_path_buf: [2048]u8 = undefined;
    const fw_path = std.fmt.bufPrintZ(
        &fw_path_buf,
        "{s}/../Frameworks/Zig Profile Server Framework.framework/Zig Profile Server Framework",
        .{parent_dir},
    ) catch {
        std.debug.print("framework path too long\n", .{});
        std.process.abort();
    };

    // Load the framework.
    const library = dlopen(fw_path.ptr, RTLD_LAZY | RTLD_LOCAL | RTLD_FIRST) orelse {
        const err = dlerror() orelse "unknown error";
        std.debug.print("dlopen: {s}\n", .{err});
        std.process.abort();
    };

    // Resolve all C API symbols.
    const content_main = resolveSymbol(ContentMainFn, library, "ContentMain");
    const set_on_initialized = resolveSymbol(SetCallbackFn, library, "ts_set_on_initialized");
    const set_on_shutdown = resolveSymbol(SetCallbackFn, library, "ts_set_on_shutdown");
    g_create_ctx = resolveSymbol(CreateContextFn, library, "ts_create_browser_context");
    g_destroy_ctx = resolveSymbol(DestroyContextFn, library, "ts_destroy_browser_context");
    g_create_tab = resolveSymbol(CreateTabFn, library, "ts_create_tab");
    g_create_web_contents = resolveSymbol(CreateWebContentsFn, library, "ts_create_web_contents");
    g_destroy_web_contents = resolveSymbol(DestroyWebContentsFn, library, "ts_destroy_web_contents");
    _ = resolveSymbol(SetViewSizeFn, library, "ts_set_view_size");
    const set_on_ca_context_changed = resolveSymbol(SetOnCAContextChangedFn, library, "ts_set_on_ca_context_id_changed");

    // Register callbacks and run.
    set_on_ca_context_changed(&onCAContextChanged);
    set_on_initialized(&onInitialized);
    set_on_shutdown(&onShutdown);

    const argc: c_int = @intCast(std.os.argv.len);
    const argv: [*]const [*:0]const u8 = @ptrCast(std.os.argv.ptr);
    const rv = content_main(argc, argv);

    std.process.exit(@intCast(rv));
}
