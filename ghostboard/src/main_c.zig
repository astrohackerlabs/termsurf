// This is the main file for the C API. The C API is used to embed Ghostty
// within other applications. Depending on the build settings some APIs
// may not be available (i.e. embedding into macOS exposes various Metal
// support).
//
// This currently isn't supported as a general purpose embedding API.
// This is currently used only to embed ghostty within a macOS app. However,
// it could be expanded to be general purpose in the future.

const std = @import("std");
const assert = @import("quirks.zig").inlineAssert;
const posix = std.posix;
const builtin = @import("builtin");
const build_config = @import("build_config.zig");
const main = @import("main_ghostty.zig");
const state = &@import("global.zig").state;
const apprt = @import("apprt.zig");
const termsurf = @import("apprt/termsurf.zig");
const internal_os = @import("os/main.zig");

// Some comptime assertions that our C API depends on.
comptime {
    // We allow tests to reference this file because we unit test
    // some of the C API. At runtime though we should never get these
    // functions unless we are building libghostty.
    if (!builtin.is_test) {
        assert(apprt.runtime == apprt.embedded);
    }
}

/// Global options so we can log. This is identical to main.
pub const std_options = main.std_options;

comptime {
    // These structs need to be referenced so the `export` functions
    // are truly exported by the C API lib.

    // Our config API
    _ = @import("config.zig").CApi;

    // Any apprt-specific C API, mainly libghostty for apprt.embedded.
    if (@hasDecl(apprt.runtime, "CAPI")) _ = apprt.runtime.CAPI;

    // Our benchmark API. We probably want to gate this on a build
    // config in the future but for now we always just export it.
    _ = @import("benchmark/main.zig").CApi;
}

/// ghostty_info_s
const Info = extern struct {
    mode: BuildMode,
    version: [*]const u8,
    version_len: usize,

    const BuildMode = enum(c_int) {
        debug,
        release_safe,
        release_fast,
        release_small,
    };
};

/// ghostty_string_s
pub const String = extern struct {
    ptr: ?[*]const u8,
    len: usize,
    sentinel: bool,

    pub const empty: String = .{
        .ptr = null,
        .len = 0,
        .sentinel = false,
    };

    pub fn fromSlice(slice: anytype) String {
        return .{
            .ptr = slice.ptr,
            .len = slice.len,
            .sentinel = sentinel: {
                const info = @typeInfo(@TypeOf(slice));
                switch (info) {
                    .pointer => |p| {
                        if (p.size != .slice) @compileError("only slices supported");
                        if (p.child != u8) @compileError("only u8 slices supported");
                        const sentinel_ = p.sentinel();
                        if (sentinel_) |sentinel| if (sentinel != 0) @compileError("only 0 is supported for sentinels");
                        break :sentinel sentinel_ != null;
                    },
                    else => @compileError("only []const u8 and [:0]const u8"),
                }
            },
        };
    }

    pub fn deinit(self: *const String) void {
        const ptr = self.ptr orelse return;
        if (self.sentinel) {
            state.alloc.free(ptr[0..self.len :0]);
        } else {
            state.alloc.free(ptr[0..self.len]);
        }
    }
};

/// Initialize ghostty global state.
pub export fn ghostty_init(argc: usize, argv: [*][*:0]u8) c_int {
    assert(builtin.link_libc);

    std.os.argv = argv[0..argc];
    state.init() catch |err| {
        std.log.err("failed to initialize ghostty error={}", .{err});
        return 1;
    };

    return 0;
}

/// Runs an action if it is specified. If there is no action this returns
/// false. If there is an action then this doesn't return.
pub export fn ghostty_cli_try_action() void {
    const action = state.action orelse return;
    std.log.info("executing CLI action={}", .{action});
    posix.exit(action.run(state.alloc) catch |err| {
        std.log.err("CLI action failed error={}", .{err});
        posix.exit(1);
    });

    posix.exit(0);
}

/// Return metadata about Ghostty, such as version, build mode, etc.
pub export fn ghostty_info() Info {
    return .{
        .mode = switch (builtin.mode) {
            .Debug => .debug,
            .ReleaseSafe => .release_safe,
            .ReleaseFast => .release_fast,
            .ReleaseSmall => .release_small,
        },
        .version = build_config.version_string.ptr,
        .version_len = build_config.version_string.len,
    };
}

/// Translate a string maintained by libghostty into the current
/// application language. This will return the same string (same pointer)
/// if no translation is found, so the pointer must be stable through
/// the function call.
///
/// This should only be used for singular strings maintained by Ghostty.
pub export fn ghostty_translate(msgid: [*:0]const u8) [*:0]const u8 {
    return internal_os.i18n._(msgid);
}

/// Free a string allocated by Ghostty.
pub export fn ghostty_string_free(str: String) void {
    str.deinit();
}

pub export fn termsurf_ipc_start() c_int {
    termsurf.start() catch |err| {
        std.log.err("failed to start TermSurf socket error={}", .{err});
        return 1;
    };
    return 0;
}

pub export fn termsurf_ipc_stop() void {
    termsurf.stop();
}

pub export fn termsurf_overlay_presented_pixels(
    pane_id: [*:0]const u8,
    pixel_width: u64,
    pixel_height: u64,
) void {
    termsurf.overlayPresentedPixels(std.mem.span(pane_id), pixel_width, pixel_height);
}

pub export fn termsurf_forward_key_event(
    pane_id: [*:0]const u8,
    event_type: [*:0]const u8,
    windows_key_code: i64,
    utf8: [*:0]const u8,
    modifiers: u64,
) c_int {
    return if (termsurf.forwardKeyEvent(
        std.mem.span(pane_id),
        std.mem.span(event_type),
        windows_key_code,
        std.mem.span(utf8),
        modifiers,
    )) 1 else 0;
}

pub export fn termsurf_forward_mouse_event(
    pane_id: [*:0]const u8,
    event_type: [*:0]const u8,
    button: [*:0]const u8,
    x: f64,
    y: f64,
    click_count: i64,
    modifiers: u64,
) c_int {
    return if (termsurf.forwardMouseEvent(
        std.mem.span(pane_id),
        std.mem.span(event_type),
        std.mem.span(button),
        x,
        y,
        click_count,
        modifiers,
    )) 1 else 0;
}

pub export fn termsurf_forward_mouse_move(
    pane_id: [*:0]const u8,
    x: f64,
    y: f64,
    modifiers: u64,
) c_int {
    return if (termsurf.forwardMouseMove(
        std.mem.span(pane_id),
        x,
        y,
        modifiers,
    )) 1 else 0;
}

pub export fn termsurf_forward_scroll_event(
    pane_id: [*:0]const u8,
    x: f64,
    y: f64,
    delta_x: f64,
    delta_y: f64,
    phase: u64,
    momentum_phase: u64,
    precise: bool,
    modifiers: u64,
) c_int {
    return if (termsurf.forwardScrollEvent(
        std.mem.span(pane_id),
        x,
        y,
        delta_x,
        delta_y,
        phase,
        momentum_phase,
        precise,
        modifiers,
    )) 1 else 0;
}

pub export fn termsurf_pane_closed(pane_id: [*:0]const u8) void {
    termsurf.paneClosed(std.mem.span(pane_id));
}

pub export fn termsurf_pane_focus_changed(pane_id: [*:0]const u8, focused: i32) void {
    termsurf.paneFocusChanged(std.mem.span(pane_id), focused != 0);
}

test "ghostty_string_s empty string" {
    const testing = std.testing;
    const empty_string = String.empty;
    defer empty_string.deinit();

    try testing.expect(empty_string.len == 0);
    try testing.expect(empty_string.sentinel == false);
}

test "ghostty_string_s c string" {
    const testing = std.testing;
    state.alloc = testing.allocator;

    const slice: [:0]const u8 = "hello";
    const allocated_slice = try testing.allocator.dupeZ(u8, slice);
    const c_null_string = String.fromSlice(allocated_slice);
    defer c_null_string.deinit();

    try testing.expect(allocated_slice[5] == 0);
    try testing.expect(@TypeOf(slice) == [:0]const u8);
    try testing.expect(@TypeOf(allocated_slice) == [:0]u8);
    try testing.expect(c_null_string.len == 5);
    try testing.expect(c_null_string.sentinel == true);
}

test "ghostty_string_s zig string" {
    const testing = std.testing;
    state.alloc = testing.allocator;

    const slice: []const u8 = "hello";
    const allocated_slice = try testing.allocator.dupe(u8, slice);
    const zig_string = String.fromSlice(allocated_slice);
    defer zig_string.deinit();

    try testing.expect(@TypeOf(slice) == []const u8);
    try testing.expect(@TypeOf(allocated_slice) == []u8);
    try testing.expect(zig_string.len == 5);
    try testing.expect(zig_string.sentinel == false);
}
