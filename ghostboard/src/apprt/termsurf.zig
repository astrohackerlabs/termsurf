const std = @import("std");
const builtin = @import("builtin");
const build_config = @import("../build_config.zig");
const internal_os = @import("../os/main.zig");

const c = @cImport({
    @cInclude("stdlib.h");
    @cInclude("sys/socket.h");
    @cInclude("unistd.h");
    @cInclude("termsurf.pb-c.h");
});

const log = std.log.scoped(.termsurf);

const env_key: [:0]const u8 = "TERMSURF_SOCKET";
const max_frame_size: usize = 1024 * 1024;
const max_clients: usize = 128;
const max_panes: usize = 256;
const max_servers: usize = 64;
const max_apps: usize = 16;
const max_app_frontends: usize = 128;
const max_tab_lookups: usize = 512;
const max_devtools_reservations: usize = 256;
const max_closed_panes: usize = max_panes;
const max_hello_browsers: usize = 16;
const max_pane_id_len: usize = 128;
const max_profile_len: usize = 128;
const max_browser_len: usize = std.fs.max_path_bytes;
const max_url_len: usize = 2048;
const max_app_id_len: usize = 64;
const max_app_url_len: usize = max_url_len;
const max_app_frontend_id_len: usize = 64;
const max_homepage_len: usize = max_url_len;
const max_listen_socket_len: usize = std.fs.max_path_bytes;
const default_browser = "roamium";
const default_homepage = "https://termsurf.com/welcome";
const fallback_cell_width: u64 = 10;
const fallback_cell_height: u64 = 20;
const geometry_trace_env = "TERMSURF_GEOMETRY_TRACE";
const devtools_reservation_timeout_env = "TERMSURF_DEVTOOLS_RESERVATION_TIMEOUT_MS";
const default_devtools_reservation_timeout_ms: i64 = 15_000;
const roamium_path_env = "TERMSURF_ROAMIUM_PATH";
const surfari_path_env = "TERMSURF_SURFARI_PATH";
const installed_roamium_path_env = "TERMSURF_INSTALLED_ROAMIUM_PATH";
const installed_surfari_path_env = "TERMSURF_INSTALLED_SURFARI_PATH";
const installed_roamium_path = "/opt/homebrew/opt/termsurf-roamium/roamium";
const installed_surfari_path = "/opt/homebrew/opt/termsurf-surfari/surfari";
const gtui_app_id = "gtui";
const gtui_app_path_env = "TERMSURF_GTUI_APP_PATH";
const deno_path_env = "TERMSURF_DENO_PATH";

const AppRuntime = enum {
    deno,
};

const AppDescriptor = struct {
    app_id: []const u8,
    runtime: AppRuntime,
    app_path_env: [:0]const u8,
    runtime_path_env: [:0]const u8,
    default_runtime_path: []const u8,
    readiness_timeout_ms: u64,
};

const app_descriptors = [_]AppDescriptor{
    .{
        .app_id = gtui_app_id,
        .runtime = .deno,
        .app_path_env = gtui_app_path_env,
        .runtime_path_env = deno_path_env,
        .default_runtime_path = "deno",
        .readiness_timeout_ms = 15_000,
    },
};

const termsurf_open_split = if (builtin.is_test) testTermsurfOpenSplit else @extern(*const fn (
    pane_id: [*:0]const u8,
    direction: [*:0]const u8,
    command: [*:0]const u8,
) callconv(.c) void, .{ .name = "termsurf_open_split" });

const termsurf_present_overlay = if (builtin.is_test) testTermsurfPresentOverlay else @extern(*const fn (
    pane_id: [*:0]const u8,
    context_id: u64,
    col: u64,
    row: u64,
    width: u64,
    height: u64,
    pixel_width: u64,
    pixel_height: u64,
) callconv(.c) void, .{ .name = "termsurf_present_overlay" });

const termsurf_clear_overlay = if (builtin.is_test) testTermsurfClearOverlay else @extern(*const fn (
    pane_id: [*:0]const u8,
) callconv(.c) void, .{ .name = "termsurf_clear_overlay" });

const termsurf_set_cursor = if (builtin.is_test) testTermsurfSetCursor else @extern(*const fn (
    pane_id: [*:0]const u8,
    cursor_type: i64,
) callconv(.c) void, .{ .name = "termsurf_set_cursor" });

const termsurf_set_title = if (builtin.is_test) testTermsurfSetTitle else @extern(*const fn (
    pane_id: [*:0]const u8,
    title: [*:0]const u8,
) callconv(.c) void, .{ .name = "termsurf_set_title" });

fn testTermsurfOpenSplit(
    pane_id: [*:0]const u8,
    direction: [*:0]const u8,
    command: [*:0]const u8,
) callconv(.c) void {
    _ = pane_id;
    _ = direction;
    _ = command;
}

fn testTermsurfPresentOverlay(
    pane_id: [*:0]const u8,
    context_id: u64,
    col: u64,
    row: u64,
    width: u64,
    height: u64,
    pixel_width: u64,
    pixel_height: u64,
) callconv(.c) void {
    _ = pane_id;
    _ = context_id;
    _ = col;
    _ = row;
    _ = width;
    _ = height;
    _ = pixel_width;
    _ = pixel_height;
}

fn testTermsurfClearOverlay(pane_id: [*:0]const u8) callconv(.c) void {
    _ = pane_id;
}

fn testTermsurfSetCursor(pane_id: [*:0]const u8, cursor_type: i64) callconv(.c) void {
    _ = pane_id;
    _ = cursor_type;
}

fn testTermsurfSetTitle(pane_id: [*:0]const u8, title: [*:0]const u8) callconv(.c) void {
    _ = pane_id;
    _ = title;
}

const ConnType = enum {
    unknown,
    tui,
    browser,
};

const ClientSlot = struct {
    fd: std.posix.fd_t = -1,
    thread: ?std.Thread = null,
    done: bool = false,
};

const PaneState = struct {
    in_use: bool = false,
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,
    profile: [max_profile_len]u8 = undefined,
    profile_len: usize = 0,
    browser: [max_browser_len]u8 = undefined,
    browser_len: usize = 0,
    url: [max_url_len]u8 = undefined,
    url_len: usize = 0,
    col: u64 = 0,
    row: u64 = 0,
    width: u64 = 0,
    height: u64 = 0,
    browsing: bool = false,
    focused: bool = false,
    inspected_tab_id: i64 = 0,
    tab_id: i64 = 0,
    ca_context_id: u64 = 0,
    ca_pixel_width: u64 = 0,
    ca_pixel_height: u64 = 0,
    appkit_pixel_width: u64 = 0,
    appkit_pixel_height: u64 = 0,
    last_resize_pixel_width: u64 = 0,
    last_resize_pixel_height: u64 = 0,
    cursor_type: i64 = 0,
    tui_fd: std.posix.fd_t = -1,

    fn paneId(self: *const PaneState) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }

    fn profileName(self: *const PaneState) []const u8 {
        return self.profile[0..self.profile_len];
    }

    fn browserName(self: *const PaneState) []const u8 {
        return self.browser[0..self.browser_len];
    }
};

const ClosedPaneState = struct {
    in_use: bool = false,
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,
    tui_fd: std.posix.fd_t = -1,

    fn paneId(self: *const ClosedPaneState) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }
};

const ServerState = struct {
    in_use: bool = false,
    profile: [max_profile_len]u8 = undefined,
    profile_len: usize = 0,
    browser: [max_browser_len]u8 = undefined,
    browser_len: usize = 0,
    listen_socket: [max_listen_socket_len]u8 = undefined,
    listen_socket_len: usize = 0,
    pane_count: usize = 0,
    attached_fd: std.posix.fd_t = -1,
    child_pid: std.process.Child.Id = 0,

    fn profileName(self: *const ServerState) []const u8 {
        return self.profile[0..self.profile_len];
    }

    fn browserName(self: *const ServerState) []const u8 {
        return self.browser[0..self.browser_len];
    }

    fn listenSocket(self: *const ServerState) []const u8 {
        return self.listen_socket[0..self.listen_socket_len];
    }
};

const AppState = struct {
    in_use: bool = false,
    starting: bool = false,
    app_id: [max_app_id_len]u8 = undefined,
    app_id_len: usize = 0,
    url: [max_app_url_len]u8 = undefined,
    url_len: usize = 0,
    child_pid: std.process.Child.Id = 0,
    port: u16 = 0,

    fn appId(self: *const AppState) []const u8 {
        return self.app_id[0..self.app_id_len];
    }

    fn appUrl(self: *const AppState) []const u8 {
        return self.url[0..self.url_len];
    }
};

const AppFrontendState = struct {
    in_use: bool = false,
    app_id: [max_app_id_len]u8 = undefined,
    app_id_len: usize = 0,
    frontend_id: [max_app_frontend_id_len]u8 = undefined,
    frontend_id_len: usize = 0,
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,
    tui_fd: std.posix.fd_t = -1,

    fn appId(self: *const AppFrontendState) []const u8 {
        return self.app_id[0..self.app_id_len];
    }

    fn frontendId(self: *const AppFrontendState) []const u8 {
        return self.frontend_id[0..self.frontend_id_len];
    }

    fn paneId(self: *const AppFrontendState) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }
};

const AppShutdownSnapshot = struct {
    app_id: [max_app_id_len]u8 = undefined,
    app_id_len: usize = 0,
    child_pid: std.process.Child.Id = 0,

    fn appId(self: *const AppShutdownSnapshot) []const u8 {
        return self.app_id[0..self.app_id_len];
    }
};

const AppFrontendCloseSnapshot = struct {
    tui_fd: std.posix.fd_t = -1,
    app_id: [max_app_id_len]u8 = undefined,
    app_id_len: usize = 0,
    frontend_id: [max_app_frontend_id_len]u8 = undefined,
    frontend_id_len: usize = 0,
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,

    fn appId(self: *const AppFrontendCloseSnapshot) []const u8 {
        return self.app_id[0..self.app_id_len];
    }

    fn frontendId(self: *const AppFrontendCloseSnapshot) []const u8 {
        return self.frontend_id[0..self.frontend_id_len];
    }

    fn paneId(self: *const AppFrontendCloseSnapshot) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }
};

const AppReadyResult = enum {
    ready,
    exited,
    timeout,
};

const TabLookupState = struct {
    in_use: bool = false,
    profile: [max_profile_len]u8 = undefined,
    profile_len: usize = 0,
    browser: [max_browser_len]u8 = undefined,
    browser_len: usize = 0,
    tab_id: i64 = 0,
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,

    fn profileName(self: *const TabLookupState) []const u8 {
        return self.profile[0..self.profile_len];
    }

    fn browserName(self: *const TabLookupState) []const u8 {
        return self.browser[0..self.browser_len];
    }

    fn paneId(self: *const TabLookupState) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }
};

const DevtoolsReservationState = struct {
    in_use: bool = false,
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,
    profile: [max_profile_len]u8 = undefined,
    profile_len: usize = 0,
    browser: [max_browser_len]u8 = undefined,
    browser_len: usize = 0,
    inspected_tab_id: i64 = 0,
    created_ms: i64 = 0,
    handoff_used: bool = false,

    fn paneId(self: *const DevtoolsReservationState) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }

    fn profileName(self: *const DevtoolsReservationState) []const u8 {
        return self.profile[0..self.profile_len];
    }

    fn browserName(self: *const DevtoolsReservationState) []const u8 {
        return self.browser[0..self.browser_len];
    }
};

const QueryDevtoolsStatus = enum {
    success,
    not_found,
    duplicate,
    key_too_long,
    reservation_limit,
};

const BrowserReadySnapshot = struct {
    tui_fd: std.posix.fd_t = -1,
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,
    browser: [max_browser_len]u8 = undefined,
    browser_len: usize = 0,
    browser_socket: [max_listen_socket_len]u8 = undefined,
    browser_socket_len: usize = 0,
    tab_id: i64 = 0,

    fn paneId(self: *const BrowserReadySnapshot) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }

    fn browserName(self: *const BrowserReadySnapshot) []const u8 {
        return self.browser[0..self.browser_len];
    }

    fn browserSocket(self: *const BrowserReadySnapshot) []const u8 {
        return self.browser_socket[0..self.browser_socket_len];
    }
};

const ResizeSnapshot = struct {
    browser_fd: std.posix.fd_t = -1,
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,
    tab_id: i64 = 0,
    pixel_width: u64 = 0,
    pixel_height: u64 = 0,

    fn paneId(self: *const ResizeSnapshot) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }
};

const FocusChangedSnapshot = struct {
    browser_fd: std.posix.fd_t = -1,
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,
    tab_id: i64 = 0,
    focused: bool = false,

    fn paneId(self: *const FocusChangedSnapshot) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }
};

const ModeChangedSnapshot = struct {
    tui_fd: std.posix.fd_t = -1,
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,
    browsing: bool = false,

    fn paneId(self: *const ModeChangedSnapshot) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }
};

const GuiActiveTarget = struct {
    browser_fd: std.posix.fd_t = -1,
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,
    tab_id: i64 = 0,
    active: bool = false,
    has_pane: bool = false,

    fn paneId(self: *const GuiActiveTarget) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }
};

const BrowserInputSnapshot = struct {
    browser_fd: std.posix.fd_t = -1,
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,
    tab_id: i64 = 0,

    fn paneId(self: *const BrowserInputSnapshot) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }
};

const OverlaySnapshot = struct {
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,
    context_id: u64 = 0,
    col: u64 = 0,
    row: u64 = 0,
    width: u64 = 0,
    height: u64 = 0,
    pixel_width: u64 = 0,
    pixel_height: u64 = 0,

    fn paneId(self: *const OverlaySnapshot) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }
};

const CursorSnapshot = struct {
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,
    tab_id: i64 = 0,
    cursor_type: i64 = 0,

    fn paneId(self: *const CursorSnapshot) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }
};

const TitleSnapshot = struct {
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,
    title: [max_url_len]u8 = undefined,
    title_len: usize = 0,
    tab_id: i64 = 0,

    fn paneId(self: *const TitleSnapshot) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }

    fn titleText(self: *const TitleSnapshot) []const u8 {
        return self.title[0..self.title_len];
    }
};

fn geometryTraceEnabled() bool {
    const value = std.posix.getenv(geometry_trace_env) orelse return false;
    return !std.mem.eql(u8, value, "0") and !std.mem.eql(u8, value, "false");
}

fn geometryTracePane(event: []const u8, pane: *const PaneState, note: []const u8) void {
    if (!geometryTraceEnabled()) return;
    log.info(
        "TermSurf geometry layer=zig event={s} scenario={s} identity=window_id:unknown:appkit-only surface_id:unknown:appkit-only selected_tab_id:unknown:appkit-only pane_id:{s} browser_tab_id:{} grid={}x{}+{}+{} browser_pixel={}x{} context_id={} browsing={} visible={} note={s}",
        .{
            event,
            geometryScenario(),
            pane.paneId(),
            pane.tab_id,
            pane.width,
            pane.height,
            pane.col,
            pane.row,
            pane.ca_pixel_width,
            pane.ca_pixel_height,
            pane.ca_context_id,
            pane.browsing,
            pane.ca_context_id != 0 and pane.width != 0 and pane.height != 0,
            note,
        },
    );
}

fn geometryTraceOverlay(event: []const u8, snapshot: *const OverlaySnapshot, note: []const u8) void {
    if (!geometryTraceEnabled()) return;
    log.info(
        "TermSurf geometry layer=zig event={s} scenario={s} identity=window_id:unknown:appkit-only surface_id:unknown:appkit-only selected_tab_id:unknown:appkit-only pane_id:{s} browser_tab_id:unknown:see-tabready grid={}x{}+{}+{} browser_pixel={}x{} context_id={} visible=true note={s}",
        .{
            event,
            geometryScenario(),
            snapshot.paneId(),
            snapshot.width,
            snapshot.height,
            snapshot.col,
            snapshot.row,
            snapshot.pixel_width,
            snapshot.pixel_height,
            snapshot.context_id,
            note,
        },
    );
}

fn geometryTraceClear(event: []const u8, snapshot: *const ClearOverlaySnapshot, note: []const u8) void {
    if (!geometryTraceEnabled()) return;
    log.info(
        "TermSurf geometry layer=zig event={s} scenario={s} identity=window_id:unknown:appkit-only surface_id:unknown:appkit-only selected_tab_id:unknown:appkit-only pane_id:{s} browser_tab_id:unknown:clearing visible=false note={s}",
        .{ event, geometryScenario(), snapshot.paneId(), note },
    );
}

fn geometryTraceAppKitPixels(event: []const u8, snapshot: *const ResizeSnapshot, note: []const u8) void {
    if (!geometryTraceEnabled()) return;
    log.info(
        "TermSurf geometry layer=zig event={s} scenario={s} identity=window_id:unknown:appkit-only surface_id:unknown:appkit-only selected_tab_id:unknown:appkit-only pane_id:{s} browser_tab_id:{} appkit_pixel={}x{} visible=true note={s}",
        .{
            event,
            geometryScenario(),
            snapshot.paneId(),
            snapshot.tab_id,
            snapshot.pixel_width,
            snapshot.pixel_height,
            note,
        },
    );
}

fn geometryScenario() []const u8 {
    return std.posix.getenv("TERMSURF_GEOMETRY_SCENARIO") orelse "unknown";
}

const ClearOverlaySnapshot = struct {
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,

    fn paneId(self: *const ClearOverlaySnapshot) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }
};

const CloseTabSnapshot = struct {
    browser_fd: std.posix.fd_t = -1,
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,
    tab_id: i64 = 0,

    fn paneId(self: *const CloseTabSnapshot) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }
};

const ServerShutdownSnapshot = struct {
    browser_fd: std.posix.fd_t = -1,
    child_pid: std.process.Child.Id = 0,
    profile: [max_profile_len]u8 = undefined,
    profile_len: usize = 0,
    browser: [max_browser_len]u8 = undefined,
    browser_len: usize = 0,

    fn profileName(self: *const ServerShutdownSnapshot) []const u8 {
        return self.profile[0..self.profile_len];
    }

    fn browserName(self: *const ServerShutdownSnapshot) []const u8 {
        return self.browser[0..self.browser_len];
    }
};

const CreateDevtoolsTabSnapshot = struct {
    browser_fd: std.posix.fd_t = -1,
    pane_id: [max_pane_id_len]u8 = undefined,
    pane_id_len: usize = 0,
    inspected_tab_id: i64 = 0,
    pixel_width: u64 = 0,
    pixel_height: u64 = 0,
    dark: bool = false,

    fn paneId(self: *const CreateDevtoolsTabSnapshot) []const u8 {
        return self.pane_id[0..self.pane_id_len];
    }
};

var mutex: std.Thread.Mutex = .{};
var clients_mutex: std.Thread.Mutex = .{};
var state_mutex: std.Thread.Mutex = .{};
var listener_fd: std.posix.fd_t = -1;
var accept_thread: ?std.Thread = null;
var stopping = std.atomic.Value(bool).init(false);
var socket_path_buf: [std.fs.max_path_bytes]u8 = undefined;
var socket_path_len: usize = 0;
var clients: [max_clients]ClientSlot = [_]ClientSlot{.{}} ** max_clients;
var panes: [max_panes]PaneState = [_]PaneState{.{}} ** max_panes;
var closed_panes: [max_closed_panes]ClosedPaneState = [_]ClosedPaneState{.{}} ** max_closed_panes;
var servers: [max_servers]ServerState = [_]ServerState{.{}} ** max_servers;
var apps: [max_apps]AppState = [_]AppState{.{}} ** max_apps;
var app_frontends: [max_app_frontends]AppFrontendState = [_]AppFrontendState{.{}} ** max_app_frontends;
var tab_lookups: [max_tab_lookups]TabLookupState = [_]TabLookupState{.{}} ** max_tab_lookups;
var devtools_reservations: [max_devtools_reservations]DevtoolsReservationState = [_]DevtoolsReservationState{.{}} ** max_devtools_reservations;
var last_browser_pane: [max_pane_id_len]u8 = undefined;
var last_browser_pane_len: usize = 0;
var hello_homepage: [max_homepage_len:0]u8 = undefined;
var hello_homepage_len: usize = 0;
var hello_browsers: [max_hello_browsers][max_browser_len:0]u8 = undefined;
var hello_browser_lens: [max_hello_browsers]usize = undefined;
var hello_browser_count: usize = 0;
var gui_active: bool = false;
var pending_gui_activation: bool = false;
var current_color_scheme_dark: bool = false;

pub fn start() !void {
    mutex.lock();
    defer mutex.unlock();

    if (listener_fd >= 0) return error.AlreadyStarted;

    const tmpdir = std.posix.getenv("TMPDIR") orelse "/tmp";
    const sep = if (std.mem.endsWith(u8, tmpdir, "/")) "" else "/";

    var dir_buf: [std.fs.max_path_bytes]u8 = undefined;
    const dir_z = std.fmt.bufPrintZ(
        &dir_buf,
        "{s}{s}termsurf",
        .{ tmpdir, sep },
    ) catch return error.SocketPathTooLong;
    try std.fs.cwd().makePath(dir_z);

    const path_z = try socketPath(tmpdir, sep);
    std.posix.unlink(path_z) catch {};
    errdefer {
        std.posix.unlink(path_z) catch {};
        socket_path_len = 0;
        _ = internal_os.unsetenv(env_key);
    }

    const fd = try std.posix.socket(std.posix.AF.UNIX, std.posix.SOCK.STREAM, 0);
    errdefer std.posix.close(fd);

    const addr = try std.net.Address.initUnix(path_z);
    try std.posix.bind(fd, &addr.any, addr.getOsSockLen());
    try std.posix.listen(fd, 8);

    if (internal_os.setenv(env_key, path_z) != 0) return error.SetEnvFailed;

    stopping.store(false, .release);
    const thread = try std.Thread.spawn(.{}, acceptLoop, .{fd});

    listener_fd = fd;
    accept_thread = thread;

    log.info("TermSurf socket listening on {s}", .{path_z});
}

pub fn stop() void {
    mutex.lock();
    const fd = listener_fd;
    const thread = accept_thread;
    listener_fd = -1;
    accept_thread = null;
    stopping.store(true, .release);
    mutex.unlock();

    wakeAccept();

    if (fd >= 0) {
        std.posix.close(fd);
    }
    if (thread) |t| {
        t.join();
    }

    const client_threads = stopClients();
    for (client_threads) |maybe_thread| {
        if (maybe_thread) |t| t.join();
    }
    stopAllApps();

    if (socket_path_len > 0) {
        const path = socket_path_buf[0..socket_path_len];
        std.posix.unlink(path) catch {};
        socket_path_len = 0;
    }

    _ = internal_os.unsetenv(env_key);
}

fn acceptLoop(fd: std.posix.fd_t) void {
    while (!stopping.load(.acquire)) {
        const client_fd = std.posix.accept(fd, null, null, 0) catch |err| {
            if (!stopping.load(.acquire)) {
                log.warn("TermSurf socket accept failed err={}", .{err});
            }
            return;
        };
        if (stopping.load(.acquire)) {
            std.posix.close(client_fd);
            return;
        }
        log.info("TermSurf client connected fd={}", .{client_fd});

        joinClientThreads(reapDoneClients());
        const slot_index = reserveClient(client_fd) orelse {
            log.warn("TermSurf client limit reached fd={}", .{client_fd});
            std.posix.close(client_fd);
            continue;
        };

        const thread = std.Thread.spawn(.{}, handleClient, .{ client_fd, slot_index }) catch |err| {
            log.warn("TermSurf client thread failed fd={} err={}", .{ client_fd, err });
            clearClient(slot_index);
            std.posix.close(client_fd);
            continue;
        };

        if (activateClient(slot_index, thread)) |done_thread| {
            log.info("TermSurf client exited before thread registration fd={}", .{client_fd});
            done_thread.join();
        }
    }
}

fn handleClient(fd: std.posix.fd_t, slot_index: usize) void {
    var conn_type: ConnType = .unknown;
    defer {
        if (conn_type == .tui) {
            cleanupAppFrontendsForFd(fd);
            cleanupTuiPanes(fd);
        }
        if (conn_type == .browser) cleanupBrowserConnection(fd);
        markClientDone(slot_index);
        std.posix.close(fd);
    }

    const allocator = std.heap.c_allocator;
    while (!stopping.load(.acquire)) {
        const frame = readFrame(fd, allocator) catch |err| {
            log.warn("TermSurf client read failed fd={} err={}", .{ fd, err });
            return;
        } orelse return;

        {
            defer allocator.free(frame);

            const msg = c.termsurf__term_surf_message__unpack(null, frame.len, frame.ptr) orelse {
                log.warn("TermSurf protobuf decode failed fd={}", .{fd});
                return;
            };
            defer c.termsurf__term_surf_message__free_unpacked(msg, null);

            log.info("TermSurf message decoded type={s}", .{msgTypeName(msg.*.msg_case)});
            if (conn_type == .unknown) {
                conn_type = classifyConnection(msg.*.msg_case);
                log.info("TermSurf connection type={s} fd={}", .{ connTypeName(conn_type), fd });
            }

            switch (msg.*.msg_case) {
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_HELLO_REQUEST => {
                    sendHelloReply(fd) catch |err| {
                        log.warn("TermSurf HelloReply failed fd={} err={}", .{ fd, err });
                        return;
                    };
                },
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_LAST_REQUEST => {
                    const req = msg.*.unnamed_0.query_last_request;
                    if (req) |query| {
                        log.info(
                            "TermSurf QueryLastRequest pane_id={s} profile={s}",
                            .{ query.*.pane_id, query.*.profile },
                        );
                    }
                    sendQueryLastReply(fd, req) catch |err| {
                        log.warn("TermSurf QueryLastReply failed fd={} err={}", .{ fd, err });
                        return;
                    };
                },
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_DEVTOOLS_REQUEST => {
                    const req = msg.*.unnamed_0.query_devtools_request;
                    if (req) |query| {
                        log.info(
                            "TermSurf QueryDevtoolsRequest pane_id={s} inspected_tab_id={} profile={s} browser={s}",
                            .{ query.*.pane_id, query.*.inspected_tab_id, query.*.profile, query.*.browser },
                        );
                        sendQueryDevtoolsReply(fd, query) catch |err| {
                            log.warn("TermSurf QueryDevtoolsReply failed fd={} err={}", .{ fd, err });
                            return;
                        };
                    } else {
                        sendQueryDevtoolsReply(fd, null) catch |err| {
                            log.warn("TermSurf QueryDevtoolsReply failed fd={} err={}", .{ fd, err });
                            return;
                        };
                    }
                },
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_TABS_REQUEST => {
                    const req = msg.*.unnamed_0.query_tabs_request;
                    if (req) |query| {
                        log.info(
                            "TermSurf QueryTabsRequest pane_id={s} profile={s}",
                            .{ query.*.pane_id, query.*.profile },
                        );
                    }
                    sendQueryTabsReply(fd, req) catch |err| {
                        log.warn("TermSurf QueryTabsReply failed fd={} err={}", .{ fd, err });
                        return;
                    };
                },
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_SET_OVERLAY => {
                    handleSetOverlay(fd, msg.*.unnamed_0.set_overlay);
                },
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_SET_DEVTOOLS_OVERLAY => {
                    handleSetDevtoolsOverlay(fd, msg.*.unnamed_0.set_devtools_overlay);
                },
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_SERVER_REGISTER => {
                    handleServerRegister(fd, msg.*.unnamed_0.server_register);
                },
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_TAB_READY => {
                    handleTabReady(msg.*.unnamed_0.tab_ready);
                },
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_CA_CONTEXT => {
                    handleCaContext(fd, msg.*.unnamed_0.ca_context);
                },
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_CURSOR_CHANGED => {
                    handleCursorChanged(fd, msg.*.unnamed_0.cursor_changed);
                },
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_TITLE_CHANGED => {
                    handleTitleChanged(fd, msg.*.unnamed_0.title_changed);
                },
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_MODE_CHANGED => {
                    handleModeChanged(msg.*.unnamed_0.mode_changed);
                },
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_OPEN_SPLIT => {
                    handleOpenSplit(msg.*.unnamed_0.open_split);
                },
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_OPEN_APP => {
                    handleOpenApp(fd, msg.*.unnamed_0.open_app);
                },
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_CLOSE_APP_FRONTEND => {
                    handleCloseAppFrontend(msg.*.unnamed_0.close_app_frontend);
                },
                else => {
                    log.info("TermSurf message ignored type={s}", .{msgTypeName(msg.*.msg_case)});
                },
            }
        }
    }
}

fn reserveClient(fd: std.posix.fd_t) ?usize {
    clients_mutex.lock();
    defer clients_mutex.unlock();

    for (&clients, 0..) |*slot, i| {
        if (slot.thread == null and slot.fd < 0) {
            slot.* = .{ .fd = fd, .thread = null, .done = false };
            return i;
        }
    }

    return null;
}

fn activateClient(index: usize, thread: std.Thread) ?std.Thread {
    clients_mutex.lock();
    defer clients_mutex.unlock();

    if (clients[index].done) {
        clients[index] = .{};
        return thread;
    }

    clients[index].thread = thread;
    return null;
}

fn clearClient(index: usize) void {
    clients_mutex.lock();
    defer clients_mutex.unlock();
    clients[index] = .{};
}

fn markClientDone(index: usize) void {
    clients_mutex.lock();
    defer clients_mutex.unlock();

    clients[index].fd = -1;
    clients[index].done = true;
}

fn reapDoneClients() [max_clients]?std.Thread {
    var threads: [max_clients]?std.Thread = [_]?std.Thread{null} ** max_clients;

    clients_mutex.lock();
    for (&clients, 0..) |*slot, i| {
        if (slot.done) {
            threads[i] = slot.thread;
            slot.* = .{};
        }
    }
    clients_mutex.unlock();

    return threads;
}

fn joinClientThreads(threads: [max_clients]?std.Thread) void {
    for (threads) |maybe_thread| {
        if (maybe_thread) |thread| {
            thread.join();
        }
    }
}

fn stopClients() [max_clients]?std.Thread {
    var threads: [max_clients]?std.Thread = [_]?std.Thread{null} ** max_clients;

    clients_mutex.lock();
    for (&clients, 0..) |*slot, i| {
        if (slot.fd >= 0) {
            std.posix.shutdown(slot.fd, .both) catch {};
            slot.fd = -1;
        }
        threads[i] = slot.thread;
        slot.* = .{};
    }
    clients_mutex.unlock();

    return threads;
}

fn readFrame(fd: std.posix.fd_t, allocator: std.mem.Allocator) !?[]u8 {
    var len_buf: [4]u8 = undefined;
    if (!try readExactOrEof(fd, &len_buf)) return null;

    const frame_len = std.mem.readInt(u32, &len_buf, .little);
    if (frame_len > max_frame_size) {
        log.warn("TermSurf frame rejected len={} max={}", .{ frame_len, max_frame_size });
        return error.FrameTooLarge;
    }

    const frame = try allocator.alloc(u8, frame_len);
    errdefer allocator.free(frame);
    if (!try readExactOrEof(fd, frame)) return error.UnexpectedEof;
    return frame;
}

fn readExactOrEof(fd: std.posix.fd_t, buf: []u8) !bool {
    var offset: usize = 0;
    while (offset < buf.len) {
        const n = try std.posix.read(fd, buf[offset..]);
        if (n == 0) {
            if (offset == 0) return false;
            return error.UnexpectedEof;
        }
        offset += n;
    }
    return true;
}

fn sendHelloReply(fd: std.posix.fd_t) !void {
    var reply: c.Termsurf__HelloReply = undefined;
    c.termsurf__hello_reply__init(&reply);
    var homepage_buf: [max_homepage_len:0]u8 = undefined;
    var homepage_len: usize = 0;
    var browser_bufs: [max_hello_browsers][max_browser_len:0]u8 = undefined;
    var browser_lens: [max_hello_browsers]usize = undefined;
    var browser_ptrs: [max_hello_browsers][*:0]u8 = undefined;
    const homepage = currentHelloHomepage(&homepage_buf, &homepage_len);
    const browser_count = currentHelloBrowsers(&browser_bufs, &browser_lens, &browser_ptrs);
    reply.homepage = @constCast(homepage.ptr);
    reply.n_browsers = browser_count;
    reply.browsers = @ptrCast(&browser_ptrs);

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_HELLO_REPLY;
    wrapper.unnamed_0.hello_reply = &reply;

    try sendProtobuf(fd, &wrapper);
    var browser_log_buf: [1024]u8 = undefined;
    const browser_log = browserListForLog(browser_ptrs[0..browser_count], &browser_log_buf);
    log.info(
        "TermSurf HelloReply sent homepage={s} browsers={s}",
        .{ homepage, browser_log },
    );
}

pub fn helloConfigChanged(homepage: []const u8, browser_payload: []const u8) void {
    const next_homepage = if (homepage.len == 0) default_homepage else homepage;

    state_mutex.lock();
    defer state_mutex.unlock();

    if (next_homepage.len > hello_homepage.len) {
        @memcpy(hello_homepage[0..default_homepage.len], default_homepage);
        hello_homepage[default_homepage.len] = 0;
        hello_homepage_len = default_homepage.len;
        log.warn(
            "TermSurf Hello config homepage too long len={} max={} using_default={s}",
            .{ next_homepage.len, hello_homepage.len, default_homepage },
        );
    } else {
        @memcpy(hello_homepage[0..next_homepage.len], next_homepage);
        hello_homepage[next_homepage.len] = 0;
        hello_homepage_len = next_homepage.len;
    }

    setHelloBrowsersLocked(browser_payload);

    var browser_log_buf: [1024]u8 = undefined;
    const browser_log = currentHelloBrowsersForLogLocked(&browser_log_buf);
    log.info(
        "TermSurf Hello config homepage={s} browsers={s}",
        .{ currentHelloHomepageLocked(), browser_log },
    );
}

pub fn colorSchemeChanged(dark: bool) void {
    state_mutex.lock();
    defer state_mutex.unlock();

    current_color_scheme_dark = dark;
    log.info("TermSurf color scheme changed dark={}", .{dark});
}

fn currentHelloHomepage(buf: *[max_homepage_len:0]u8, len: *usize) [:0]const u8 {
    state_mutex.lock();
    defer state_mutex.unlock();

    const current = currentHelloHomepageLocked();
    @memcpy(buf[0..current.len], current);
    buf[current.len] = 0;
    len.* = current.len;
    return buf[0..len.* :0];
}

fn currentHelloHomepageLocked() []const u8 {
    return if (hello_homepage_len == 0) default_homepage else hello_homepage[0..hello_homepage_len];
}

fn setHelloBrowsersLocked(browser_payload: []const u8) void {
    var next_count: usize = 0;
    var it = std.mem.splitScalar(u8, browser_payload, '\n');
    while (it.next()) |raw| {
        const browser = std.mem.trim(u8, raw, " \t\r");
        if (browser.len == 0) continue;
        if (next_count >= hello_browsers.len or browser.len > hello_browsers[next_count].len) {
            setDefaultHelloBrowsersLocked();
            log.warn(
                "TermSurf Hello config browsers invalid count={} len={} using_default={s}",
                .{ next_count, browser.len, default_browser },
            );
            return;
        }

        @memcpy(hello_browsers[next_count][0..browser.len], browser);
        hello_browsers[next_count][browser.len] = 0;
        hello_browser_lens[next_count] = browser.len;
        next_count += 1;
    }

    if (next_count == 0) {
        setDefaultHelloBrowsersLocked();
        return;
    }

    hello_browser_count = next_count;
}

fn setDefaultHelloBrowsersLocked() void {
    @memcpy(hello_browsers[0][0..default_browser.len], default_browser);
    hello_browsers[0][default_browser.len] = 0;
    hello_browser_lens[0] = default_browser.len;
    hello_browser_count = 1;
}

fn currentHelloBrowsers(
    bufs: *[max_hello_browsers][max_browser_len:0]u8,
    lens: *[max_hello_browsers]usize,
    ptrs: *[max_hello_browsers][*:0]u8,
) usize {
    state_mutex.lock();
    defer state_mutex.unlock();

    if (hello_browser_count == 0) setDefaultHelloBrowsersLocked();

    for (0..hello_browser_count) |idx| {
        const len = hello_browser_lens[idx];
        @memcpy(bufs[idx][0..len], hello_browsers[idx][0..len]);
        bufs[idx][len] = 0;
        lens[idx] = len;
        ptrs[idx] = @ptrCast(bufs[idx][0..len :0].ptr);
    }

    return hello_browser_count;
}

fn currentHelloBrowsersForLogLocked(buf: []u8) []const u8 {
    if (hello_browser_count == 0) setDefaultHelloBrowsersLocked();
    var pos: usize = 0;
    for (0..hello_browser_count) |idx| {
        if (idx > 0 and pos < buf.len) {
            buf[pos] = ',';
            pos += 1;
        }
        const browser = hello_browsers[idx][0..hello_browser_lens[idx]];
        const writable = @min(browser.len, buf.len -| pos);
        @memcpy(buf[pos .. pos + writable], browser[0..writable]);
        pos += writable;
    }
    return buf[0..pos];
}

fn browserListForLog(browsers: [][*:0]u8, buf: []u8) []const u8 {
    var pos: usize = 0;
    for (browsers, 0..) |browser_ptr, idx| {
        if (idx > 0 and pos < buf.len) {
            buf[pos] = ',';
            pos += 1;
        }
        const browser = std.mem.span(browser_ptr);
        const writable = @min(browser.len, buf.len -| pos);
        @memcpy(buf[pos .. pos + writable], browser[0..writable]);
        pos += writable;
    }
    return buf[0..pos];
}

fn sendQueryLastReply(fd: std.posix.fd_t, req: ?*c.Termsurf__QueryLastRequest) !void {
    var reply: c.Termsurf__QueryLastReply = undefined;
    c.termsurf__query_last_reply__init(&reply);

    var pane_id_buf: [max_pane_id_len]u8 = undefined;
    var pane_id_len: usize = 0;
    var profile_buf: [max_profile_len]u8 = undefined;
    var profile_len: usize = 0;

    const requested_profile = if (req) |query| cString(query.*.profile) else "";
    const error_msg = fillQueryLastReply(
        &reply,
        requested_profile,
        &pane_id_buf,
        &pane_id_len,
        &profile_buf,
        &profile_len,
    );
    if (error_msg) |err| {
        reply.@"error" = @constCast(err.ptr);
    } else {
        reply.pane_id = @constCast(pane_id_buf[0..pane_id_len :0].ptr);
        reply.profile = @constCast(profile_buf[0..profile_len :0].ptr);
    }

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_LAST_REPLY;
    wrapper.unnamed_0.query_last_reply = &reply;

    try sendProtobuf(fd, &wrapper);
    log.info("TermSurf QueryLastReply sent", .{});
}

fn fillQueryLastReply(
    reply: *c.Termsurf__QueryLastReply,
    requested_profile: []const u8,
    pane_id_buf: []u8,
    pane_id_len: *usize,
    profile_buf: []u8,
    profile_len: *usize,
) ?[:0]const u8 {
    state_mutex.lock();
    defer state_mutex.unlock();

    if (last_browser_pane_len == 0) {
        return "No browser pane yet";
    }

    const pane = findPane(last_browser_pane[0..last_browser_pane_len]) orelse {
        return "Last pane no longer exists";
    };

    if (requested_profile.len > 0 and
        !std.mem.eql(u8, panes[pane].profileName(), requested_profile))
    {
        return "No matching pane for profile";
    }

    if (!copyText(pane_id_buf, pane_id_len, panes[pane].paneId())) {
        return "Last pane id is too long";
    }
    if (!copyText(profile_buf, profile_len, panes[pane].profileName())) {
        return "Last pane profile is too long";
    }
    reply.tab_id = panes[pane].tab_id;
    return null;
}

fn sendQueryDevtoolsReply(fd: std.posix.fd_t, req: ?*c.Termsurf__QueryDevtoolsRequest) !void {
    var reply: c.Termsurf__QueryDevtoolsReply = undefined;
    c.termsurf__query_devtools_reply__init(&reply);

    const allocator = std.heap.c_allocator;
    var allocated_error: ?[]u8 = null;
    defer if (allocated_error) |err| allocator.free(err);
    var browser_buf: [max_browser_len]u8 = undefined;
    var browser_len: usize = 0;
    var profile_buf: [max_profile_len]u8 = undefined;
    var profile_len: usize = 0;

    const error_msg: [:0]const u8 = if (req) |query| blk: {
        const browser = cString(query.*.browser);
        const profile = cString(query.*.profile);
        if (browser.len == 0) {
            break :blk "DevTools target browser is required";
        }
        if (profile.len == 0) {
            break :blk "DevTools target profile is required";
        }
        if (query.*.inspected_tab_id == 0) {
            break :blk "DevTools target tab id is required";
        }
        const pane_id = cString(query.*.pane_id);
        switch (fillQueryDevtoolsReply(&reply, pane_id, profile, browser, query.*.inspected_tab_id, &profile_buf, &profile_len, &browser_buf, &browser_len)) {
            .success => break :blk "",
            .not_found => {
                const error_len = std.fmt.count(
                    "Inspected tab {} not found in {s}/{s}",
                    .{ query.*.inspected_tab_id, browser, profile },
                );
                allocated_error = try allocator.alloc(u8, error_len + 1);
                break :blk std.fmt.bufPrintZ(
                    allocated_error.?,
                    "Inspected tab {} not found in {s}/{s}",
                    .{ query.*.inspected_tab_id, browser, profile },
                ) catch unreachable;
            },
            .duplicate => {
                const error_len = std.fmt.count(
                    "Tab {} already has DevTools open in {s}/{s}",
                    .{ query.*.inspected_tab_id, browser, profile },
                );
                allocated_error = try allocator.alloc(u8, error_len + 1);
                break :blk std.fmt.bufPrintZ(
                    allocated_error.?,
                    "Tab {} already has DevTools open in {s}/{s}",
                    .{ query.*.inspected_tab_id, browser, profile },
                ) catch unreachable;
            },
            .key_too_long => break :blk "DevTools target key is too long",
            .reservation_limit => break :blk "Too many pending DevTools launches",
        }
    } else "DevTools target browser is required";
    reply.@"error" = @constCast(error_msg.ptr);
    if (error_msg.len == 0) {
        reply.profile = @constCast(profile_buf[0..profile_len :0].ptr);
        reply.browser = @constCast(browser_buf[0..browser_len :0].ptr);
    }

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_DEVTOOLS_REPLY;
    wrapper.unnamed_0.query_devtools_reply = &reply;

    try sendProtobuf(fd, &wrapper);
    log.info("TermSurf QueryDevtoolsReply sent error={s}", .{error_msg});
}

fn fillQueryDevtoolsReply(
    reply: *c.Termsurf__QueryDevtoolsReply,
    pane_id: []const u8,
    profile: []const u8,
    browser: []const u8,
    inspected_tab_id: i64,
    profile_buf: []u8,
    profile_len: *usize,
    browser_buf: []u8,
    browser_len: *usize,
) QueryDevtoolsStatus {
    state_mutex.lock();
    defer state_mutex.unlock();

    const now_ms = std.time.milliTimestamp();
    const reservation_timeout_ms = devtoolsReservationTimeoutMs();
    expireDevtoolsReservations(now_ms, reservation_timeout_ms);

    if (findTabLookup(profile, browser, inspected_tab_id) == null) return .not_found;
    if (!copyText(profile_buf, profile_len, profile)) return .key_too_long;
    if (!copyText(browser_buf, browser_len, browser)) return .key_too_long;

    const claim_status = claimDevtoolsTarget(pane_id, profile, browser, inspected_tab_id, now_ms, reservation_timeout_ms);
    if (claim_status != .success) return claim_status;

    reply.tab_id = inspected_tab_id;
    return .success;
}

fn claimDevtoolsTarget(
    pane_id: []const u8,
    profile: []const u8,
    browser: []const u8,
    inspected_tab_id: i64,
    now_ms: i64,
    reservation_timeout_ms: i64,
) QueryDevtoolsStatus {
    if (hasLiveDevtoolsTarget(profile, browser, inspected_tab_id)) return .duplicate;

    if (findDevtoolsReservation(profile, browser, inspected_tab_id)) |index| {
        const reservation = &devtools_reservations[index];
        if (!reservation.handoff_used and !std.mem.eql(u8, reservation.paneId(), pane_id)) {
            reservation.handoff_used = true;
            reservation.created_ms = now_ms;
            log.info(
                "DevTools reservation handoff: profile={s} browser={s} inspected_tab_id={} from_pane={s} to_pane={s}",
                .{ profile, browser, inspected_tab_id, reservation.paneId(), pane_id },
            );
            return .success;
        }
        return .duplicate;
    }

    return reserveDevtoolsTarget(pane_id, profile, browser, inspected_tab_id, now_ms, reservation_timeout_ms);
}

fn hasLiveDevtoolsTarget(profile: []const u8, browser: []const u8, inspected_tab_id: i64) bool {
    return findLiveDevtoolsTarget(profile, browser, inspected_tab_id) != null;
}

fn findLiveDevtoolsTarget(profile: []const u8, browser: []const u8, inspected_tab_id: i64) ?usize {
    for (&panes, 0..) |*pane, i| {
        if (pane.in_use and
            pane.inspected_tab_id == inspected_tab_id and
            std.mem.eql(u8, pane.profileName(), profile) and
            std.mem.eql(u8, pane.browserName(), browser))
        {
            return i;
        }
    }

    return null;
}

fn findDevtoolsReservation(profile: []const u8, browser: []const u8, inspected_tab_id: i64) ?usize {
    for (&devtools_reservations, 0..) |*reservation, i| {
        if (reservation.in_use and
            reservation.inspected_tab_id == inspected_tab_id and
            std.mem.eql(u8, reservation.profileName(), profile) and
            std.mem.eql(u8, reservation.browserName(), browser))
        {
            return i;
        }
    }
    return null;
}

fn reserveDevtoolsTarget(pane_id: []const u8, profile: []const u8, browser: []const u8, inspected_tab_id: i64, now_ms: i64, reservation_timeout_ms: i64) QueryDevtoolsStatus {
    for (&devtools_reservations) |*reservation| {
        if (!reservation.in_use) {
            if (!copyText(&reservation.pane_id, &reservation.pane_id_len, pane_id)) return .key_too_long;
            if (!copyText(&reservation.profile, &reservation.profile_len, profile)) return .key_too_long;
            if (!copyText(&reservation.browser, &reservation.browser_len, browser)) return .key_too_long;
            reservation.inspected_tab_id = inspected_tab_id;
            reservation.created_ms = now_ms;
            reservation.handoff_used = false;
            reservation.in_use = true;
            log.info(
                "DevTools reservation: profile={s} browser={s} inspected_tab_id={} pane_id={s} timeout_ms={}",
                .{ profile, browser, inspected_tab_id, pane_id, reservation_timeout_ms },
            );
            return .success;
        }
    }

    return .reservation_limit;
}

fn clearDevtoolsReservation(profile: []const u8, browser: []const u8, inspected_tab_id: i64, reason: []const u8) void {
    if (findDevtoolsReservation(profile, browser, inspected_tab_id)) |index| {
        devtools_reservations[index] = .{};
        log.info(
            "DevTools reservation cleared: profile={s} browser={s} inspected_tab_id={} reason={s}",
            .{ profile, browser, inspected_tab_id, reason },
        );
    }
}

fn expireDevtoolsReservations(now_ms: i64, reservation_timeout_ms: i64) void {
    for (&devtools_reservations) |*reservation| {
        if (!reservation.in_use) continue;
        if (now_ms - reservation.created_ms < reservation_timeout_ms) continue;
        log.info(
            "DevTools reservation expired: profile={s} browser={s} inspected_tab_id={}",
            .{ reservation.profileName(), reservation.browserName(), reservation.inspected_tab_id },
        );
        reservation.* = .{};
    }
}

fn devtoolsReservationTimeoutMs() i64 {
    const value = std.posix.getenv(devtools_reservation_timeout_env) orelse return default_devtools_reservation_timeout_ms;
    return std.fmt.parseInt(i64, value, 10) catch default_devtools_reservation_timeout_ms;
}

fn sendQueryTabsReply(fd: std.posix.fd_t, req: ?*c.Termsurf__QueryTabsRequest) !void {
    var reply: c.Termsurf__QueryTabsReply = undefined;
    c.termsurf__query_tabs_reply__init(&reply);
    reply.gui_panes = countQueryTabsGuiPanes(req);

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_TABS_REPLY;
    wrapper.unnamed_0.query_tabs_reply = &reply;

    try sendProtobuf(fd, &wrapper);
    log.info("TermSurf QueryTabsReply sent", .{});
}

fn countQueryTabsGuiPanes(req: ?*c.Termsurf__QueryTabsRequest) i64 {
    const requested_profile = if (req) |query| cString(query.*.profile) else "";

    state_mutex.lock();
    defer state_mutex.unlock();

    var count: i64 = 0;
    for (&panes) |*pane| {
        if (!pane.in_use) continue;
        if (requested_profile.len > 0 and
            !std.mem.eql(u8, pane.profileName(), requested_profile))
        {
            continue;
        }
        count += 1;
    }
    return count;
}

fn handleServerRegister(fd: std.posix.fd_t, req: ?*c.Termsurf__ServerRegister) void {
    const profile = serverRegisterProfile(req);
    const browser = serverRegisterBrowser(req);
    log.info("ServerRegister: profile={s} browser={s}", .{ profile, browser });

    state_mutex.lock();
    defer state_mutex.unlock();

    if (findAttachableServer(profile, browser)) |index| {
        servers[index].attached_fd = fd;
        log.info(
            "ServerRegister: matched server key={s}/{s}",
            .{ servers[index].profileName(), servers[index].browserName() },
        );
        sendPendingCreateTabs(fd, &servers[index]) catch |err| {
            log.warn("ServerRegister: pending CreateTab flush failed err={}", .{err});
        };
        return;
    }

    log.warn("ServerRegister: no matching server for profile={s} browser={s}", .{ profile, browser });
}

fn serverRegisterProfile(req: ?*c.Termsurf__ServerRegister) []const u8 {
    if (req) |server| {
        if (server.*.profile) |profile| return std.mem.span(profile);
    }
    return "";
}

fn serverRegisterBrowser(req: ?*c.Termsurf__ServerRegister) []const u8 {
    if (req) |server| {
        if (server.*.browser) |browser| return std.mem.span(browser);
    }
    return "";
}

fn handleSetOverlay(tui_fd: std.posix.fd_t, req: ?*c.Termsurf__SetOverlay) void {
    const overlay = req orelse {
        log.warn("SetOverlay: missing payload", .{});
        return;
    };

    const pane_id = cString(overlay.*.pane_id);
    const profile = cString(overlay.*.profile);
    const requested_browser = cString(overlay.*.browser);
    const browser = if (requested_browser.len == 0) default_browser else requested_browser;
    const url = cString(overlay.*.url);

    log.info(
        "SetOverlay: pane_id={s} profile={s} browser={s} url={s}",
        .{ pane_id, profile, browser, url },
    );

    var should_spawn = false;
    var spawn_profile_buf: [max_profile_len]u8 = undefined;
    var spawn_profile_len: usize = 0;
    var spawn_browser_name_buf: [max_browser_len]u8 = undefined;
    var spawn_browser_name_len: usize = 0;
    var spawn_browser_executable_buf: [max_browser_len]u8 = undefined;
    var spawn_browser_executable_len: usize = 0;
    var spawn_listen_socket_buf: [max_listen_socket_len]u8 = undefined;
    var spawn_listen_socket_len: usize = 0;
    var should_record_child: bool = false;
    var overlay_snapshot: ?OverlaySnapshot = null;

    state_mutex.lock();

    if (findClosedPane(pane_id, tui_fd)) |_| {
        log.info("SetOverlay ignored for closed pane_id={s} fd={}", .{ pane_id, tui_fd });
        state_mutex.unlock();
        return;
    }

    if (findPane(pane_id)) |pane_index| {
        if (!updatePane(&panes[pane_index], overlay, pane_id, profile, browser, url)) {
            state_mutex.unlock();
            return;
        }
        panes[pane_index].tui_fd = tui_fd;
        const server_index = findServer(profile, browser);
        const pane_count = if (server_index) |index| servers[index].pane_count else 0;
        overlay_snapshot = snapshotOverlay(&panes[pane_index]);
        log.info(
            "SetOverlay: updated pane_id={s} profile={s} browser={s} pane_count={}",
            .{ pane_id, profile, browser, pane_count },
        );
        geometryTracePane("set_overlay_update", &panes[pane_index], "updated-existing-pane");
        state_mutex.unlock();
        if (overlay_snapshot) |snapshot| presentOverlay(&snapshot);
        return;
    }

    const pane_index = reservePane() orelse {
        log.warn("SetOverlay: pane limit reached pane_id={s}", .{pane_id});
        state_mutex.unlock();
        return;
    };
    if (!updatePane(&panes[pane_index], overlay, pane_id, profile, browser, url)) {
        state_mutex.unlock();
        return;
    }
    panes[pane_index].tui_fd = tui_fd;
    geometryTracePane("set_overlay_new", &panes[pane_index], "reserved-new-pane");

    if (findServer(profile, browser)) |server_index| {
        servers[server_index].pane_count += 1;
        log.info(
            "SetOverlay: reused pending server key={s}/{s} pane_count={} has_fd={}",
            .{ servers[server_index].profileName(), servers[server_index].browserName(), servers[server_index].pane_count, servers[server_index].attached_fd >= 0 },
        );
        if (servers[server_index].attached_fd >= 0) {
            sendCreateTab(servers[server_index].attached_fd, &panes[pane_index]) catch |err| {
                log.warn("SetOverlay: attached CreateTab send failed pane_id={s} err={}", .{ pane_id, err });
            };
        }
    } else {
        const browser_executable = resolveBrowserExecutable(browser) orelse {
            panes[pane_index] = .{};
            state_mutex.unlock();
            return;
        };
        const server_index = reserveServer() orelse {
            log.warn("SetOverlay: server limit reached profile={s} browser={s}", .{ profile, browser });
            panes[pane_index] = .{};
            state_mutex.unlock();
            return;
        };
        if (!setServer(&servers[server_index], profile, browser)) {
            panes[pane_index] = .{};
            state_mutex.unlock();
            return;
        }
        if (buildListenSocket(&servers[server_index])) {
            if (!copyText(&spawn_profile_buf, &spawn_profile_len, servers[server_index].profileName()) or
                !copyText(&spawn_browser_name_buf, &spawn_browser_name_len, servers[server_index].browserName()) or
                !copyText(&spawn_browser_executable_buf, &spawn_browser_executable_len, browser_executable) or
                !copyText(&spawn_listen_socket_buf, &spawn_listen_socket_len, servers[server_index].listenSocket()))
            {
                log.warn("SetOverlay: spawn arguments too long profile={s} browser={s}", .{ profile, browser });
                panes[pane_index] = .{};
                servers[server_index] = .{};
                state_mutex.unlock();
                return;
            }
            should_spawn = true;
            should_record_child = should_spawn;
        } else {
            log.warn("SetOverlay: listen socket path too long profile={s} browser={s}", .{ profile, browser });
            panes[pane_index] = .{};
            servers[server_index] = .{};
            state_mutex.unlock();
            return;
        }
        log.info(
            "SetOverlay: created pending server key={s}/{s} pane_count={} listen_socket={s}",
            .{ servers[server_index].profileName(), servers[server_index].browserName(), servers[server_index].pane_count, servers[server_index].listenSocket() },
        );
    }
    state_mutex.unlock();

    if (should_spawn) {
        const profile_z = spawn_profile_buf[0..spawn_profile_len :0];
        const browser_name_z = spawn_browser_name_buf[0..spawn_browser_name_len :0];
        const browser_executable_z = spawn_browser_executable_buf[0..spawn_browser_executable_len :0];
        const listen_socket_z = spawn_listen_socket_buf[0..spawn_listen_socket_len :0];
        if (spawnBrowserProcess(profile_z, browser_name_z, browser_executable_z, listen_socket_z)) |pid| {
            if (should_record_child) recordServerChild(profile_z, browser, pid);
        }
    }
}

fn resolveBrowserExecutable(browser: []const u8) ?[]const u8 {
    return resolveBrowserExecutableForBuild(browser, build_config.is_debug);
}

fn resolveBrowserExecutableForBuild(browser: []const u8, is_debug: bool) ?[]const u8 {
    if (isAbsolutePath(browser)) {
        log.info("SetOverlay: browser executable absolute path={s}", .{browser});
        return browser;
    }

    if (std.mem.eql(u8, browser, default_browser)) {
        if (std.posix.getenv(roamium_path_env)) |resolved| {
            if (resolved.len != 0 and isAbsolutePath(resolved)) {
                log.info(
                    "SetOverlay: named browser resolved browser={s} env={s} path={s}",
                    .{ browser, roamium_path_env, resolved },
                );
                return resolved;
            }
            log.warn(
                "SetOverlay: named browser unresolved browser={s} env={s} value={s}",
                .{ browser, roamium_path_env, resolved },
            );
        } else {
            log.warn(
                "SetOverlay: named browser unresolved browser={s} env={s}",
                .{ browser, roamium_path_env },
            );
        }

        if (is_debug) return null;

        if (std.posix.getenv(installed_roamium_path_env)) |resolved| {
            if (resolved.len != 0 and isAbsolutePath(resolved)) {
                log.info(
                    "SetOverlay: named browser resolved browser={s} env={s} path={s}",
                    .{ browser, installed_roamium_path_env, resolved },
                );
                return resolved;
            }
            log.warn(
                "SetOverlay: installed browser override ignored browser={s} env={s} value={s}",
                .{ browser, installed_roamium_path_env, resolved },
            );
        }

        log.info(
            "SetOverlay: named browser resolved browser={s} installed_path={s}",
            .{ browser, installed_roamium_path },
        );
        return installed_roamium_path;
    }

    if (std.mem.eql(u8, browser, "surfari")) {
        if (std.posix.getenv(surfari_path_env)) |resolved| {
            if (resolved.len != 0 and isAbsolutePath(resolved)) {
                log.info(
                    "SetOverlay: named browser resolved browser={s} env={s} path={s}",
                    .{ browser, surfari_path_env, resolved },
                );
                return resolved;
            }
            log.warn(
                "SetOverlay: named browser unresolved browser={s} env={s} value={s}",
                .{ browser, surfari_path_env, resolved },
            );
        }

        if (is_debug) {
            log.warn(
                "SetOverlay: named browser unresolved browser={s} env={s}",
                .{ browser, surfari_path_env },
            );
            return null;
        }

        if (std.posix.getenv(installed_surfari_path_env)) |resolved| {
            if (resolved.len != 0 and isAbsolutePath(resolved)) {
                log.info(
                    "SetOverlay: named browser resolved browser={s} env={s} path={s}",
                    .{ browser, installed_surfari_path_env, resolved },
                );
                return resolved;
            }
            log.warn(
                "SetOverlay: installed browser override ignored browser={s} env={s} value={s}",
                .{ browser, installed_surfari_path_env, resolved },
            );
        }

        log.info(
            "SetOverlay: named browser resolved browser={s} installed_path={s}",
            .{ browser, installed_surfari_path },
        );
        return installed_surfari_path;
    }

    log.warn("SetOverlay: named browser unsupported browser={s}", .{browser});
    return null;
}

fn handleSetDevtoolsOverlay(tui_fd: std.posix.fd_t, req: ?*c.Termsurf__SetDevtoolsOverlay) void {
    const overlay = req orelse {
        log.warn("SetDevtoolsOverlay: missing payload", .{});
        return;
    };

    const pane_id = cString(overlay.*.pane_id);
    const profile = cString(overlay.*.profile);
    const requested_browser = cString(overlay.*.browser);
    const browser = if (requested_browser.len == 0) default_browser else requested_browser;
    const inspected_tab_id = overlay.*.inspected_tab_id;
    var create_devtools: ?CreateDevtoolsTabSnapshot = null;
    var resize_snapshot: ?ResizeSnapshot = null;

    log.info(
        "SetDevtoolsOverlay: pane_id={s} profile={s} browser={s} inspected_tab_id={}",
        .{ pane_id, profile, browser, inspected_tab_id },
    );

    state_mutex.lock();

    if (findPane(pane_id)) |pane_index| {
        if (!updateDevtoolsPane(&panes[pane_index], overlay, pane_id, profile, browser)) {
            state_mutex.unlock();
            return;
        }
        clearDevtoolsReservation(profile, browser, inspected_tab_id, "set-devtools-overlay-update");
        panes[pane_index].tui_fd = tui_fd;
        resize_snapshot = snapshotResize(&panes[pane_index]);
        log.info(
            "SetDevtoolsOverlay: updated pane_id={s} profile={s} browser={s}",
            .{ pane_id, profile, browser },
        );
        state_mutex.unlock();
        if (resize_snapshot) |snapshot| {
            sendResize(&snapshot) catch |err| {
                log.warn("Resize send failed pane_id={s} err={}", .{ snapshot.paneId(), err });
            };
        }
        return;
    }

    if (findLiveDevtoolsTarget(profile, browser, inspected_tab_id)) |existing_index| {
        log.warn(
            "SetDevtoolsOverlay: duplicate target rejected pane_id={s} existing_pane_id={s} profile={s} browser={s} inspected_tab_id={}",
            .{ pane_id, panes[existing_index].paneId(), profile, browser, inspected_tab_id },
        );
        state_mutex.unlock();
        return;
    }

    const pane_index = reservePane() orelse {
        log.warn("SetDevtoolsOverlay: pane limit reached pane_id={s}", .{pane_id});
        state_mutex.unlock();
        return;
    };
    if (!updateDevtoolsPane(&panes[pane_index], overlay, pane_id, profile, browser)) {
        panes[pane_index] = .{};
        state_mutex.unlock();
        return;
    }
    clearDevtoolsReservation(profile, browser, inspected_tab_id, "set-devtools-overlay-create");
    panes[pane_index].tui_fd = tui_fd;

    const server_index = findServer(profile, browser) orelse {
        log.warn("SetDevtoolsOverlay: no server profile={s} browser={s}", .{ profile, browser });
        panes[pane_index] = .{};
        state_mutex.unlock();
        return;
    };
    if (servers[server_index].attached_fd < 0) {
        log.warn("SetDevtoolsOverlay: server not attached profile={s} browser={s}", .{ profile, browser });
        panes[pane_index] = .{};
        state_mutex.unlock();
        return;
    }

    servers[server_index].pane_count += 1;
    create_devtools = snapshotCreateDevtoolsTab(&panes[pane_index], servers[server_index].attached_fd);
    log.info(
        "SetDevtoolsOverlay: created pane_id={s} profile={s} browser={s} pane_count={}",
        .{ pane_id, profile, browser, servers[server_index].pane_count },
    );
    state_mutex.unlock();

    if (create_devtools) |snapshot| {
        sendCreateDevtoolsTab(&snapshot) catch |err| {
            log.warn("CreateDevtoolsTab send failed pane_id={s} err={}", .{ snapshot.paneId(), err });
        };
    }
}

fn handleOpenSplit(req: ?*c.Termsurf__OpenSplit) void {
    const open_split = req orelse {
        log.warn("OpenSplit: missing payload", .{});
        return;
    };
    const pane_id = open_split.*.pane_id orelse {
        log.warn("OpenSplit: missing pane_id", .{});
        return;
    };
    const direction = open_split.*.direction orelse {
        log.warn("OpenSplit: missing direction", .{});
        return;
    };
    const command = open_split.*.command orelse {
        log.warn("OpenSplit: missing command", .{});
        return;
    };

    log.info(
        "OpenSplit: pane_id={s} direction={s} command={s}",
        .{ pane_id, direction, command },
    );
    termsurf_open_split(pane_id, direction, command);
}

fn handleOpenApp(tui_fd: std.posix.fd_t, req: ?*c.Termsurf__OpenApp) void {
    const open_app = req orelse {
        sendOpenAppReply(tui_fd, "", "", "", "", "missing OpenApp payload") catch {};
        return;
    };
    const pane_id = cString(open_app.*.pane_id);
    const app_id = cString(open_app.*.app_id);
    const browser = cString(open_app.*.browser);
    const profile = cString(open_app.*.profile);
    const entrypoint = cString(open_app.*.entrypoint);

    log.info("OpenApp: pane_id={s} app_id={s}", .{ pane_id, app_id });

    const descriptor = findAppDescriptor(app_id) orelse {
        sendOpenAppReply(tui_fd, pane_id, app_id, "", "", "unsupported app_id") catch |err| {
            log.warn("OpenAppReply unsupported failed pane_id={s} app_id={s} err={}", .{ pane_id, app_id, err });
        };
        return;
    };

    var frontend_id_buf: [max_app_frontend_id_len]u8 = undefined;
    var frontend_id_len: usize = 0;
    var url_buf: [max_app_url_len]u8 = undefined;
    var url_len: usize = 0;
    var error_buf: [256]u8 = undefined;

    const result = openOrReuseApp(descriptor, tui_fd, pane_id, app_id, browser, profile, entrypoint, &frontend_id_buf, &frontend_id_len, &url_buf, &url_len, &error_buf);
    if (result) {
        sendOpenAppReply(
            tui_fd,
            pane_id,
            app_id,
            frontend_id_buf[0..frontend_id_len],
            url_buf[0..url_len],
            "",
        ) catch |err| {
            log.warn("OpenAppReply failed pane_id={s} app_id={s} err={}", .{ pane_id, app_id, err });
        };
    } else {
        sendOpenAppReply(tui_fd, pane_id, app_id, "", "", std.mem.sliceTo(&error_buf, 0)) catch |err| {
            log.warn("OpenAppReply error failed pane_id={s} app_id={s} err={}", .{ pane_id, app_id, err });
        };
    }
}

fn handleCloseAppFrontend(req: ?*c.Termsurf__CloseAppFrontend) void {
    const close_app = req orelse {
        log.warn("CloseAppFrontend: missing payload", .{});
        return;
    };
    const app_id = cString(close_app.*.app_id);
    const frontend_id = cString(close_app.*.frontend_id);

    var shutdown: ?AppShutdownSnapshot = null;
    state_mutex.lock();
    removeAppFrontendLocked(app_id, frontend_id, null, &shutdown);
    state_mutex.unlock();

    if (shutdown) |*snapshot| stopAppBackend(snapshot);
}

fn updatePane(
    pane: *PaneState,
    overlay: *c.Termsurf__SetOverlay,
    pane_id: []const u8,
    profile: []const u8,
    browser: []const u8,
    url: []const u8,
) bool {
    if (!copyText(&pane.pane_id, &pane.pane_id_len, pane_id)) {
        log.warn("SetOverlay: pane_id too long len={} max={}", .{ pane_id.len, max_pane_id_len });
        return false;
    }
    if (!copyText(&pane.profile, &pane.profile_len, profile)) {
        log.warn("SetOverlay: profile too long len={} max={}", .{ profile.len, max_profile_len });
        return false;
    }
    if (!copyText(&pane.browser, &pane.browser_len, browser)) {
        log.warn("SetOverlay: browser too long len={} max={}", .{ browser.len, max_browser_len });
        return false;
    }
    if (!copyText(&pane.url, &pane.url_len, url)) {
        log.warn("SetOverlay: url too long len={} max={}", .{ url.len, max_url_len });
        return false;
    }

    pane.in_use = true;
    pane.col = overlay.*.col;
    pane.row = overlay.*.row;
    pane.width = overlay.*.width;
    pane.height = overlay.*.height;
    pane.browsing = overlay.*.browsing != 0;
    pane.inspected_tab_id = 0;
    return true;
}

fn updateDevtoolsPane(
    pane: *PaneState,
    overlay: *c.Termsurf__SetDevtoolsOverlay,
    pane_id: []const u8,
    profile: []const u8,
    browser: []const u8,
) bool {
    if (!copyText(&pane.pane_id, &pane.pane_id_len, pane_id)) {
        log.warn("SetDevtoolsOverlay: pane_id too long len={} max={}", .{ pane_id.len, max_pane_id_len });
        return false;
    }
    if (!copyText(&pane.profile, &pane.profile_len, profile)) {
        log.warn("SetDevtoolsOverlay: profile too long len={} max={}", .{ profile.len, max_profile_len });
        return false;
    }
    if (!copyText(&pane.browser, &pane.browser_len, browser)) {
        log.warn("SetDevtoolsOverlay: browser too long len={} max={}", .{ browser.len, max_browser_len });
        return false;
    }
    if (!copyText(&pane.url, &pane.url_len, "")) {
        log.warn("SetDevtoolsOverlay: url clear failed", .{});
        return false;
    }

    pane.in_use = true;
    pane.col = overlay.*.col;
    pane.row = overlay.*.row;
    pane.width = overlay.*.width;
    pane.height = overlay.*.height;
    pane.browsing = overlay.*.browsing != 0;
    pane.inspected_tab_id = overlay.*.inspected_tab_id;
    return true;
}

fn sendPendingCreateTabs(fd: std.posix.fd_t, server: *const ServerState) !void {
    for (&panes) |*pane| {
        if (!pane.in_use or pane.tab_id != 0) continue;
        if (pane.inspected_tab_id != 0) continue;
        if (!std.mem.eql(u8, pane.profileName(), server.profileName())) continue;
        if (!std.mem.eql(u8, pane.browserName(), server.browserName())) continue;
        try sendCreateTab(fd, pane);
    }
}

fn sendCreateTab(fd: std.posix.fd_t, pane: *const PaneState) !void {
    var create_tab: c.Termsurf__CreateTab = undefined;
    c.termsurf__create_tab__init(&create_tab);
    create_tab.url = @constCast(pane.url[0..pane.url_len :0].ptr);
    create_tab.pane_id = @constCast(pane.pane_id[0..pane.pane_id_len :0].ptr);
    create_tab.pixel_width = pane.width * fallback_cell_width;
    create_tab.pixel_height = pane.height * fallback_cell_height;
    create_tab.dark = if (current_color_scheme_dark) 1 else 0;

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_CREATE_TAB;
    wrapper.unnamed_0.create_tab = &create_tab;

    try sendProtobuf(fd, &wrapper);
    log.info("sent CreateTab: pane_id={s} url={s} dark={}", .{
        pane.paneId(),
        pane.url[0..pane.url_len],
        current_color_scheme_dark,
    });
    geometryTracePane("create_tab", pane, "sent-create-tab");
}

fn snapshotCreateDevtoolsTab(pane: *const PaneState, browser_fd: std.posix.fd_t) ?CreateDevtoolsTabSnapshot {
    var snapshot: CreateDevtoolsTabSnapshot = .{
        .browser_fd = browser_fd,
        .inspected_tab_id = pane.inspected_tab_id,
        .pixel_width = pane.width * fallback_cell_width,
        .pixel_height = pane.height * fallback_cell_height,
        .dark = current_color_scheme_dark,
    };
    if (!copyText(&snapshot.pane_id, &snapshot.pane_id_len, pane.paneId())) return null;
    return snapshot;
}

fn sendCreateDevtoolsTab(snapshot: *const CreateDevtoolsTabSnapshot) !void {
    var create_devtools_tab: c.Termsurf__CreateDevtoolsTab = undefined;
    c.termsurf__create_devtools_tab__init(&create_devtools_tab);
    create_devtools_tab.pane_id = @constCast(snapshot.pane_id[0..snapshot.pane_id_len :0].ptr);
    create_devtools_tab.inspected_tab_id = snapshot.inspected_tab_id;
    create_devtools_tab.pixel_width = snapshot.pixel_width;
    create_devtools_tab.pixel_height = snapshot.pixel_height;
    create_devtools_tab.dark = if (snapshot.dark) 1 else 0;

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_CREATE_DEVTOOLS_TAB;
    wrapper.unnamed_0.create_devtools_tab = &create_devtools_tab;

    try sendProtobuf(snapshot.browser_fd, &wrapper);
    log.info(
        "CreateDevtoolsTab: pane_id={s} inspected_tab_id={} dark={}",
        .{ snapshot.paneId(), snapshot.inspected_tab_id, snapshot.dark },
    );
}

fn snapshotResize(pane: *const PaneState) ?ResizeSnapshot {
    if (pane.tab_id == 0) return null;
    const server_index = findServer(pane.profileName(), pane.browserName()) orelse return null;
    if (servers[server_index].attached_fd < 0) return null;

    var snapshot: ResizeSnapshot = .{
        .browser_fd = servers[server_index].attached_fd,
        .tab_id = pane.tab_id,
        .pixel_width = pane.width * fallback_cell_width,
        .pixel_height = pane.height * fallback_cell_height,
    };
    if (!copyText(&snapshot.pane_id, &snapshot.pane_id_len, pane.paneId())) return null;
    return snapshot;
}

fn sendResize(snapshot: *const ResizeSnapshot) !void {
    var resize: c.Termsurf__Resize = undefined;
    c.termsurf__resize__init(&resize);
    resize.tab_id = snapshot.tab_id;
    resize.pixel_width = snapshot.pixel_width;
    resize.pixel_height = snapshot.pixel_height;
    resize.screen_x = 0.0;
    resize.screen_y = 0.0;
    resize.screen_width = 0.0;
    resize.screen_height = 0.0;
    resize.screen_scale = 0.0;

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_RESIZE;
    wrapper.unnamed_0.resize = &resize;

    try sendProtobuf(snapshot.browser_fd, &wrapper);
    markResizeSent(snapshot);
    log.info(
        "Resize: pane_id={s} tab_id={} pixel={}x{}",
        .{ snapshot.paneId(), snapshot.tab_id, snapshot.pixel_width, snapshot.pixel_height },
    );
    if (geometryTraceEnabled()) {
        log.info(
            "TermSurf geometry layer=zig event=resize scenario={s} identity=window_id:unknown:appkit-only surface_id:unknown:appkit-only selected_tab_id:unknown:appkit-only pane_id:{s} browser_tab_id:{} browser_pixel={}x{} visible=true note=sent-browser-resize",
            .{ geometryScenario(), snapshot.paneId(), snapshot.tab_id, snapshot.pixel_width, snapshot.pixel_height },
        );
    }
}

fn markResizeSent(snapshot: *const ResizeSnapshot) void {
    state_mutex.lock();
    defer state_mutex.unlock();

    const pane_index = findPane(snapshot.paneId()) orelse return;
    panes[pane_index].last_resize_pixel_width = snapshot.pixel_width;
    panes[pane_index].last_resize_pixel_height = snapshot.pixel_height;
}

pub fn overlayPresentedPixels(pane_id: []const u8, pixel_width: u64, pixel_height: u64) void {
    var trace_snapshot: ?ResizeSnapshot = null;
    var resize_snapshot: ?ResizeSnapshot = null;

    state_mutex.lock();
    if (findPane(pane_id)) |pane_index| {
        var pane = &panes[pane_index];
        const unchanged_appkit =
            pane.appkit_pixel_width == pixel_width and pane.appkit_pixel_height == pixel_height;

        pane.appkit_pixel_width = pixel_width;
        pane.appkit_pixel_height = pixel_height;

        if (pixel_width != 0 and pixel_height != 0) {
            trace_snapshot = snapshotResizeForPixels(pane, pixel_width, pixel_height);

            const already_requested =
                pane.last_resize_pixel_width == pixel_width and pane.last_resize_pixel_height == pixel_height;

            // The CA context dimensions are the size reported when the context
            // was created. After pane layout changes, the browser's actual
            // size is the last resize we sent, so use that as the de-dupe key.
            if (!already_requested and (!unchanged_appkit or pane.ca_context_id != 0)) {
                resize_snapshot = trace_snapshot;
            }
        }
    } else {
        log.warn("AppKit overlay pixels ignored: no pane pane_id={s}", .{pane_id});
    }
    state_mutex.unlock();

    if (trace_snapshot) |snapshot| {
        geometryTraceAppKitPixels("appkit_presented_pixels", &snapshot, "received-appkit-presented-pixels");
    }
    if (resize_snapshot) |snapshot| {
        geometryTraceAppKitPixels("appkit_corrective_resize", &snapshot, "sending-browser-resize");
        sendResize(&snapshot) catch |err| {
            log.warn(
                "AppKit corrective resize failed pane_id={s} pixel={}x{} err={}",
                .{ snapshot.paneId(), snapshot.pixel_width, snapshot.pixel_height, err },
            );
        };
    }
}

fn snapshotResizeForPixels(pane: *const PaneState, pixel_width: u64, pixel_height: u64) ?ResizeSnapshot {
    if (pane.tab_id == 0) return null;
    const server_index = findServer(pane.profileName(), pane.browserName()) orelse return null;
    if (servers[server_index].attached_fd < 0) return null;

    var snapshot: ResizeSnapshot = .{
        .browser_fd = servers[server_index].attached_fd,
        .tab_id = pane.tab_id,
        .pixel_width = pixel_width,
        .pixel_height = pixel_height,
    };
    if (!copyText(&snapshot.pane_id, &snapshot.pane_id_len, pane.paneId())) return null;
    return snapshot;
}

fn handleModeChanged(req: ?*c.Termsurf__ModeChanged) void {
    const mode = req orelse {
        log.warn("ModeChanged: missing payload", .{});
        return;
    };
    const pane_id = cString(mode.*.pane_id);
    const browsing = mode.*.browsing != 0;
    var focus_changed: ?FocusChangedSnapshot = null;
    var gui_active_target: ?GuiActiveTarget = null;

    state_mutex.lock();
    const pane_index = findPane(pane_id) orelse {
        log.warn("ModeChanged: unknown pane_id={s} browsing={}", .{ pane_id, browsing });
        state_mutex.unlock();
        return;
    };

    panes[pane_index].browsing = browsing;
    if (browsing) {
        if (panes[pane_index].focused) {
            focus_changed = snapshotFocusChanged(&panes[pane_index], true);
            if (pending_gui_activation) {
                gui_active_target = snapshotGuiActiveTarget(&panes[pane_index]);
                if (gui_active_target != null) pending_gui_activation = false;
            }
        }
    } else {
        focus_changed = snapshotFocusChanged(&panes[pane_index], false);
    }
    log.info("ModeChanged: pane_id={s} browsing={}", .{ pane_id, browsing });
    state_mutex.unlock();

    if (focus_changed) |snapshot| {
        sendFocusChanged(&snapshot) catch |err| {
            log.warn("FocusChanged send failed pane_id={s} err={}", .{ snapshot.paneId(), err });
        };
    }
    if (gui_active_target) |target| {
        sendGuiActive(&target, "gui_activated") catch |err| {
            log.warn(
                "SetGuiActive send failed pane_id={s} tab_id={} active=true reason=gui_activated err={}",
                .{ target.paneId(), target.tab_id, err },
            );
        };
    }
}

pub fn paneFocusChanged(pane_id: []const u8, focused: bool) void {
    var focus_changed: ?FocusChangedSnapshot = null;
    var gui_active_target: ?GuiActiveTarget = null;

    state_mutex.lock();
    if (findPane(pane_id)) |pane_index| {
        panes[pane_index].focused = focused;
        if (focused) {
            if (panes[pane_index].browsing) {
                focus_changed = snapshotFocusChanged(&panes[pane_index], true);
                if (pending_gui_activation) {
                    gui_active_target = snapshotGuiActiveTarget(&panes[pane_index]);
                    if (gui_active_target != null) pending_gui_activation = false;
                }
            }
        } else {
            focus_changed = snapshotFocusChanged(&panes[pane_index], false);
        }
        log.info("Pane focus changed: pane_id={s} focused={}", .{ pane_id, focused });
    }
    state_mutex.unlock();

    if (focus_changed) |snapshot| {
        sendFocusChanged(&snapshot) catch |err| {
            log.warn("FocusChanged send failed pane_id={s} err={}", .{ snapshot.paneId(), err });
        };
    }
    if (gui_active_target) |target| {
        sendGuiActive(&target, "gui_activated") catch |err| {
            log.warn(
                "SetGuiActive send failed pane_id={s} tab_id={} active=true reason=gui_activated err={}",
                .{ target.paneId(), target.tab_id, err },
            );
        };
    }
}

pub fn copyCurrentUrl(pane_id: []const u8, out: []u8) usize {
    if (out.len == 0) return 0;

    state_mutex.lock();
    defer state_mutex.unlock();

    const pane_index = findPane(pane_id) orelse return 0;
    const pane = &panes[pane_index];
    if (!pane.in_use or !pane.focused or pane.browsing or pane.url_len == 0) return 0;

    const url = pane.url[0..pane.url_len];
    const copied_len = @min(url.len, out.len - 1);
    @memcpy(out[0..copied_len], url[0..copied_len]);
    out[copied_len] = 0;

    log.info("CopyCurrentUrl: pane_id={s} url={s}", .{ pane.paneId(), url });
    return copied_len;
}

pub fn guiActiveChanged(active: bool) void {
    var targets: [max_servers]GuiActiveTarget = [_]GuiActiveTarget{.{}} ** max_servers;
    var target_count: usize = 0;

    state_mutex.lock();
    gui_active = active;
    if (active) {
        for (&panes) |*pane| {
            if (!pane.in_use or !pane.focused or !pane.browsing or pane.tab_id == 0) continue;
            targets[target_count] = snapshotGuiActiveTarget(pane) orelse continue;
            target_count += 1;
            break;
        }
        pending_gui_activation = target_count == 0;
    } else {
        pending_gui_activation = false;
        for (&servers) |*server| {
            if (!server.in_use or server.attached_fd < 0) continue;
            targets[target_count] = .{
                .browser_fd = server.attached_fd,
                .tab_id = 0,
                .active = false,
                .has_pane = false,
            };
            target_count += 1;
        }
    }
    state_mutex.unlock();

    const reason: [:0]const u8 = if (active) "gui_activated" else "gui_deactivated";
    if (target_count == 0) {
        log.info(
            "SetGuiActive skipped active={} reason={s} targets=0 pending_activation={}",
            .{ active, reason, pending_gui_activation },
        );
        return;
    }

    for (targets[0..target_count]) |*target| {
        sendGuiActive(target, reason) catch |err| {
            if (target.has_pane) {
                log.warn(
                    "SetGuiActive send failed pane_id={s} tab_id={} active={} reason={s} err={}",
                    .{ target.paneId(), target.tab_id, active, reason, err },
                );
            } else {
                log.warn(
                    "SetGuiActive send failed tab_id={} active={} reason={s} err={}",
                    .{ target.tab_id, active, reason, err },
                );
            }
        };
    }
}

fn snapshotGuiActiveTarget(pane: *const PaneState) ?GuiActiveTarget {
    if (!gui_active or pane.tab_id == 0) return null;
    const server_index = findServer(pane.profileName(), pane.browserName()) orelse return null;
    if (servers[server_index].attached_fd < 0) return null;

    var target: GuiActiveTarget = .{
        .browser_fd = servers[server_index].attached_fd,
        .tab_id = pane.tab_id,
        .active = true,
        .has_pane = true,
    };
    if (!copyText(&target.pane_id, &target.pane_id_len, pane.paneId())) return null;
    return target;
}

fn snapshotFocusChanged(pane: *const PaneState, focused: bool) ?FocusChangedSnapshot {
    if (pane.tab_id == 0) return null;
    const server_index = findServer(pane.profileName(), pane.browserName()) orelse return null;
    if (servers[server_index].attached_fd < 0) return null;

    var snapshot: FocusChangedSnapshot = .{
        .browser_fd = servers[server_index].attached_fd,
        .tab_id = pane.tab_id,
        .focused = focused,
    };
    if (!copyText(&snapshot.pane_id, &snapshot.pane_id_len, pane.paneId())) return null;
    return snapshot;
}

fn sendFocusChanged(snapshot: *const FocusChangedSnapshot) !void {
    var focus: c.Termsurf__FocusChanged = undefined;
    c.termsurf__focus_changed__init(&focus);
    focus.tab_id = snapshot.tab_id;
    focus.focused = if (snapshot.focused) 1 else 0;

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_FOCUS_CHANGED;
    wrapper.unnamed_0.focus_changed = &focus;

    try sendProtobuf(snapshot.browser_fd, &wrapper);
    log.info(
        "FocusChanged: pane_id={s} tab_id={} focused={}",
        .{ snapshot.paneId(), snapshot.tab_id, snapshot.focused },
    );
}

fn sendGuiActive(target: *const GuiActiveTarget, reason: [:0]const u8) !void {
    var set_gui_active: c.Termsurf__SetGuiActive = undefined;
    c.termsurf__set_gui_active__init(&set_gui_active);
    set_gui_active.tab_id = target.tab_id;
    set_gui_active.active = if (target.active) 1 else 0;
    set_gui_active.reason = @constCast(reason.ptr);

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_SET_GUI_ACTIVE;
    wrapper.unnamed_0.set_gui_active = &set_gui_active;

    try sendProtobuf(target.browser_fd, &wrapper);
    if (target.has_pane) {
        log.info(
            "SetGuiActive: pane_id={s} tab_id={} active={} reason={s}",
            .{ target.paneId(), target.tab_id, target.active, reason },
        );
    } else {
        log.info(
            "SetGuiActive: tab_id={} active={} reason={s}",
            .{ target.tab_id, target.active, reason },
        );
    }
}

pub fn forwardKeyEvent(
    pane_id: []const u8,
    event_type: []const u8,
    windows_key_code: i64,
    utf8: []const u8,
    modifiers: u64,
) bool {
    if (isCtrlCDown(event_type, windows_key_code, modifiers)) {
        if (closeAppFrontendForPaneShortcut(pane_id)) return true;
    }

    const snapshot = snapshotBrowserInput(pane_id, true) orelse return false;

    var key_event: c.Termsurf__KeyEvent = undefined;
    c.termsurf__key_event__init(&key_event);
    key_event.tab_id = snapshot.tab_id;
    key_event.type = @constCast(event_type.ptr);
    key_event.windows_key_code = windows_key_code;
    key_event.utf8 = @constCast(utf8.ptr);
    key_event.modifiers = modifiers;

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_KEY_EVENT;
    wrapper.unnamed_0.key_event = &key_event;

    sendProtobuf(snapshot.browser_fd, &wrapper) catch |err| {
        log.warn("KeyEvent send failed pane_id={s} err={}", .{ snapshot.paneId(), err });
        return false;
    };
    log.info(
        "KeyEvent: pane_id={s} tab_id={} type={s} windows_key_code={} utf8_len={} modifiers={}",
        .{ snapshot.paneId(), snapshot.tab_id, event_type, windows_key_code, utf8.len, modifiers },
    );
    return true;
}

fn isCtrlCDown(event_type: []const u8, windows_key_code: i64, modifiers: u64) bool {
    return std.mem.eql(u8, event_type, "down") and windows_key_code == 67 and (modifiers & 2) != 0;
}

fn closeAppFrontendForPaneShortcut(pane_id: []const u8) bool {
    var close_snapshot: ?AppFrontendCloseSnapshot = null;
    var shutdown: ?AppShutdownSnapshot = null;

    state_mutex.lock();
    for (&app_frontends) |*frontend| {
        if (!frontend.in_use or !std.mem.eql(u8, frontend.paneId(), pane_id)) continue;
        var snapshot: AppFrontendCloseSnapshot = .{ .tui_fd = frontend.tui_fd };
        if (copyText(&snapshot.app_id, &snapshot.app_id_len, frontend.appId()) and
            copyText(&snapshot.frontend_id, &snapshot.frontend_id_len, frontend.frontendId()) and
            copyText(&snapshot.pane_id, &snapshot.pane_id_len, frontend.paneId()))
        {
            close_snapshot = snapshot;
            const app_id_copy = snapshot.appId();
            frontend.* = .{};
            shutdown = maybeSnapshotAppShutdownLocked(app_id_copy);
        }
        break;
    }
    state_mutex.unlock();

    if (close_snapshot) |*snapshot| {
        sendCloseAppFrontendToTui(snapshot) catch |err| {
            log.warn("App Ctrl+C close notify failed pane_id={s} err={}", .{ snapshot.paneId(), err });
        };
        if (shutdown) |*app_shutdown| stopAppBackend(app_shutdown);
        log.info("App Ctrl+C close pane_id={s} app_id={s}", .{ snapshot.paneId(), snapshot.appId() });
        return true;
    }
    return false;
}

pub fn forwardMouseEvent(
    pane_id: []const u8,
    event_type: []const u8,
    button: []const u8,
    x: f64,
    y: f64,
    click_count: i64,
    modifiers: u64,
) bool {
    if (std.mem.eql(u8, event_type, "down") and std.mem.eql(u8, button, "left")) {
        enterBrowseFromOverlayClick(pane_id);
    }

    const snapshot = snapshotBrowserInput(pane_id, false) orelse return false;

    var mouse_event: c.Termsurf__MouseEvent = undefined;
    c.termsurf__mouse_event__init(&mouse_event);
    mouse_event.tab_id = snapshot.tab_id;
    mouse_event.type = @constCast(event_type.ptr);
    mouse_event.button = @constCast(button.ptr);
    mouse_event.x = x;
    mouse_event.y = y;
    mouse_event.click_count = click_count;
    mouse_event.modifiers = modifiers;

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_MOUSE_EVENT;
    wrapper.unnamed_0.mouse_event = &mouse_event;

    sendProtobuf(snapshot.browser_fd, &wrapper) catch |err| {
        log.warn("MouseEvent send failed pane_id={s} err={}", .{ snapshot.paneId(), err });
        return false;
    };
    log.info(
        "MouseEvent: pane_id={s} tab_id={} type={s} button={s} x={d:.2} y={d:.2} click_count={} modifiers={}",
        .{ snapshot.paneId(), snapshot.tab_id, event_type, button, x, y, click_count, modifiers },
    );
    return true;
}

pub fn overlayMissedLeftClick(pane_id: []const u8) void {
    var mode_changed: ?ModeChangedSnapshot = null;
    var focus_changed: ?FocusChangedSnapshot = null;

    state_mutex.lock();
    if (findPane(pane_id)) |pane_index| {
        const pane = &panes[pane_index];
        if (pane.browsing) {
            pane.browsing = false;
            mode_changed = snapshotModeChanged(pane, false);
            focus_changed = snapshotFocusChanged(pane, false);
            log.info("ClickModeChanged: pane_id={s} browsing=false reason=overlay_miss", .{pane_id});
        }
    }
    state_mutex.unlock();

    sendClickModeSnapshots(mode_changed, focus_changed);
}

fn enterBrowseFromOverlayClick(pane_id: []const u8) void {
    var mode_changed: ?ModeChangedSnapshot = null;
    var focus_changed: ?FocusChangedSnapshot = null;

    state_mutex.lock();
    if (findPane(pane_id)) |pane_index| {
        const pane = &panes[pane_index];
        if (!pane.browsing) {
            pane.browsing = true;
            mode_changed = snapshotModeChanged(pane, true);
            if (pane.focused) {
                focus_changed = snapshotFocusChanged(pane, true);
            }
            log.info("ClickModeChanged: pane_id={s} browsing=true reason=overlay_click", .{pane_id});
        }
    }
    state_mutex.unlock();

    sendClickModeSnapshots(mode_changed, focus_changed);
}

fn sendClickModeSnapshots(mode_changed: ?ModeChangedSnapshot, focus_changed: ?FocusChangedSnapshot) void {
    if (mode_changed) |snapshot| {
        sendModeChangedToWeb(&snapshot) catch |err| {
            log.warn("ModeChanged send failed pane_id={s} browsing={} err={}", .{ snapshot.paneId(), snapshot.browsing, err });
        };
    }
    if (focus_changed) |snapshot| {
        sendFocusChanged(&snapshot) catch |err| {
            log.warn("FocusChanged send failed pane_id={s} err={}", .{ snapshot.paneId(), err });
        };
    }
}

pub fn forwardMouseMove(
    pane_id: []const u8,
    x: f64,
    y: f64,
    modifiers: u64,
) bool {
    const snapshot = snapshotBrowserInput(pane_id, false) orelse return false;

    var mouse_move: c.Termsurf__MouseMove = undefined;
    c.termsurf__mouse_move__init(&mouse_move);
    mouse_move.tab_id = snapshot.tab_id;
    mouse_move.x = x;
    mouse_move.y = y;
    mouse_move.modifiers = modifiers;

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_MOUSE_MOVE;
    wrapper.unnamed_0.mouse_move = &mouse_move;

    sendProtobuf(snapshot.browser_fd, &wrapper) catch |err| {
        log.warn("MouseMove send failed pane_id={s} err={}", .{ snapshot.paneId(), err });
        return false;
    };
    log.info(
        "MouseMove: pane_id={s} tab_id={} x={d:.2} y={d:.2} modifiers={}",
        .{ snapshot.paneId(), snapshot.tab_id, x, y, modifiers },
    );
    return true;
}

pub fn forwardScrollEvent(
    pane_id: []const u8,
    x: f64,
    y: f64,
    delta_x: f64,
    delta_y: f64,
    phase: u64,
    momentum_phase: u64,
    precise: bool,
    modifiers: u64,
) bool {
    const snapshot = snapshotBrowserInput(pane_id, false) orelse return false;

    var scroll_event: c.Termsurf__ScrollEvent = undefined;
    c.termsurf__scroll_event__init(&scroll_event);
    scroll_event.tab_id = snapshot.tab_id;
    scroll_event.x = x;
    scroll_event.y = y;
    scroll_event.delta_x = delta_x;
    scroll_event.delta_y = delta_y;
    scroll_event.phase = phase;
    scroll_event.momentum_phase = momentum_phase;
    scroll_event.precise = if (precise) 1 else 0;
    scroll_event.modifiers = modifiers;

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_SCROLL_EVENT;
    wrapper.unnamed_0.scroll_event = &scroll_event;

    sendProtobuf(snapshot.browser_fd, &wrapper) catch |err| {
        log.warn("ScrollEvent send failed pane_id={s} err={}", .{ snapshot.paneId(), err });
        return false;
    };
    log.info(
        "ScrollEvent: pane_id={s} tab_id={} x={d:.2} y={d:.2} delta_x={d:.2} delta_y={d:.2} phase={} momentum_phase={} precise={} modifiers={}",
        .{ snapshot.paneId(), snapshot.tab_id, x, y, delta_x, delta_y, phase, momentum_phase, precise, modifiers },
    );
    return true;
}

fn snapshotBrowserInput(pane_id: []const u8, require_browsing: bool) ?BrowserInputSnapshot {
    state_mutex.lock();
    defer state_mutex.unlock();

    const pane_index = findPane(pane_id) orelse return null;
    const pane = &panes[pane_index];
    if (require_browsing and !pane.browsing) return null;
    if (pane.tab_id == 0) return null;
    const server_index = findServer(pane.profileName(), pane.browserName()) orelse return null;
    if (servers[server_index].attached_fd < 0) return null;

    var snapshot: BrowserInputSnapshot = .{
        .browser_fd = servers[server_index].attached_fd,
        .tab_id = pane.tab_id,
    };
    if (!copyText(&snapshot.pane_id, &snapshot.pane_id_len, pane.paneId())) return null;
    return snapshot;
}

fn snapshotModeChanged(pane: *const PaneState, browsing: bool) ?ModeChangedSnapshot {
    if (pane.tui_fd < 0) return null;

    var snapshot: ModeChangedSnapshot = .{
        .tui_fd = pane.tui_fd,
        .browsing = browsing,
    };
    if (!copyText(&snapshot.pane_id, &snapshot.pane_id_len, pane.paneId())) return null;
    return snapshot;
}

fn sendModeChangedToWeb(snapshot: *const ModeChangedSnapshot) !void {
    var mode: c.Termsurf__ModeChanged = undefined;
    c.termsurf__mode_changed__init(&mode);
    mode.pane_id = @constCast(snapshot.pane_id[0..snapshot.pane_id_len :0].ptr);
    mode.browsing = if (snapshot.browsing) 1 else 0;

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_MODE_CHANGED;
    wrapper.unnamed_0.mode_changed = &mode;

    try sendProtobuf(snapshot.tui_fd, &wrapper);
    log.info("ModeChanged: pane_id={s} browsing={} source=gui", .{ snapshot.paneId(), snapshot.browsing });
}

fn sendCloseTab(snapshot: *const CloseTabSnapshot) !void {
    var close_tab: c.Termsurf__CloseTab = undefined;
    c.termsurf__close_tab__init(&close_tab);
    close_tab.tab_id = snapshot.tab_id;

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_CLOSE_TAB;
    wrapper.unnamed_0.close_tab = &close_tab;

    try sendProtobuf(snapshot.browser_fd, &wrapper);
    log.info("CloseTab: pane_id={s} tab_id={}", .{ snapshot.paneId(), snapshot.tab_id });
}

fn sendCloseTabForCleanup(snapshot: *const CloseTabSnapshot) bool {
    sendCloseTab(snapshot) catch |err| {
        log.info("CloseTab skipped: browser socket closed pane_id={s} err={}", .{ snapshot.paneId(), err });
        return false;
    };
    return true;
}

fn shutdownServer(snapshot: *const ServerShutdownSnapshot) void {
    shutdownBrowserSocket(snapshot);
    reapServerChild(snapshot);
}

fn shutdownBrowserSocket(snapshot: *const ServerShutdownSnapshot) void {
    if (snapshot.browser_fd < 0) {
        log.info(
            "Server shutdown skipped: browser socket detached profile={s} browser={s}",
            .{ snapshot.profileName(), snapshot.browserName() },
        );
        return;
    }

    const rc = c.shutdown(snapshot.browser_fd, c.SHUT_RDWR);
    switch (std.posix.errno(rc)) {
        .SUCCESS => {
            log.info(
                "Server shutdown: profile={s} browser={s}",
                .{ snapshot.profileName(), snapshot.browserName() },
            );
        },
        .BADF, .INVAL, .NOTCONN, .NOTSOCK => |err| {
            log.info(
                "Server shutdown skipped: browser socket already closed profile={s} browser={s} err={}",
                .{ snapshot.profileName(), snapshot.browserName(), err },
            );
        },
        else => |err| {
            log.warn(
                "Server shutdown failed profile={s} browser={s} err={}",
                .{ snapshot.profileName(), snapshot.browserName(), err },
            );
        },
    }
}

fn reapServerChild(snapshot: *const ServerShutdownSnapshot) void {
    if (snapshot.child_pid == 0) return;
    for (0..50) |_| {
        const res = std.posix.waitpid(snapshot.child_pid, std.c.W.NOHANG);
        if (res.pid != 0) {
            log.info(
                "Server child reaped: profile={s} browser={s} pid={}",
                .{ snapshot.profileName(), snapshot.browserName(), snapshot.child_pid },
            );
            return;
        }
        std.Thread.sleep(100 * std.time.ns_per_ms);
    }
    log.warn(
        "Server child reap timed out: profile={s} browser={s} pid={}",
        .{ snapshot.profileName(), snapshot.browserName(), snapshot.child_pid },
    );
}

fn handleTabReady(req: ?*c.Termsurf__TabReady) void {
    const ready = req orelse {
        log.warn("TabReady: missing payload", .{});
        return;
    };
    const pane_id = cString(ready.*.pane_id);
    const tab_id = ready.*.tab_id;
    var browser_ready: ?BrowserReadySnapshot = null;

    state_mutex.lock();

    const pane_index = findPane(pane_id) orelse {
        log.warn("TabReady: unknown pane_id={s}", .{pane_id});
        state_mutex.unlock();
        return;
    };

    panes[pane_index].tab_id = tab_id;
    const lookup_count = upsertTabLookup(&panes[pane_index], tab_id);
    if (panes[pane_index].inspected_tab_id == 0) {
        _ = copyText(&last_browser_pane, &last_browser_pane_len, pane_id);
    }
    browser_ready = snapshotBrowserReady(&panes[pane_index], tab_id);
    geometryTracePane("tab_ready", &panes[pane_index], "mapped-browser-tab-to-pane");

    log.info(
        "TabReady lookup: key={s}/{s} tab_id={} pane_id={s}",
        .{ panes[pane_index].profileName(), panes[pane_index].browserName(), tab_id, pane_id },
    );
    log.info("last_browser_pane={s}", .{last_browser_pane[0..last_browser_pane_len]});
    log.info("TabReady pending=false pane_id={s} tab_id={}", .{ pane_id, tab_id });
    log.info(
        "TabReady: pane_id={s} tab_id={} tab_to_pane_count={}",
        .{ pane_id, tab_id, lookup_count },
    );
    state_mutex.unlock();

    if (browser_ready) |snapshot| {
        sendBrowserReady(&snapshot) catch |err| {
            log.warn("BrowserReady send failed pane_id={s} err={}", .{ snapshot.paneId(), err });
        };
    }
}

fn snapshotBrowserReady(pane: *const PaneState, tab_id: i64) ?BrowserReadySnapshot {
    if (pane.tui_fd < 0) return null;
    const server_index = findServer(pane.profileName(), pane.browserName()) orelse return null;
    if (servers[server_index].listen_socket_len == 0) return null;

    var snapshot: BrowserReadySnapshot = .{
        .tui_fd = pane.tui_fd,
        .tab_id = tab_id,
    };
    if (!copyText(&snapshot.pane_id, &snapshot.pane_id_len, pane.paneId())) return null;
    if (!copyText(&snapshot.browser, &snapshot.browser_len, pane.browserName())) return null;
    if (!copyText(&snapshot.browser_socket, &snapshot.browser_socket_len, servers[server_index].listenSocket())) return null;
    return snapshot;
}

fn sendBrowserReady(snapshot: *const BrowserReadySnapshot) !void {
    var ready: c.Termsurf__BrowserReady = undefined;
    c.termsurf__browser_ready__init(&ready);
    ready.pane_id = @constCast(snapshot.pane_id[0..snapshot.pane_id_len :0].ptr);
    ready.tab_id = snapshot.tab_id;
    ready.browser_socket = @constCast(snapshot.browser_socket[0..snapshot.browser_socket_len :0].ptr);
    ready.browser = @constCast(snapshot.browser[0..snapshot.browser_len :0].ptr);

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_BROWSER_READY;
    wrapper.unnamed_0.browser_ready = &ready;

    try sendProtobuf(snapshot.tui_fd, &wrapper);
    log.info(
        "BrowserReady: pane_id={s} tab_id={} socket={s} browser={s}",
        .{ snapshot.paneId(), snapshot.tab_id, snapshot.browserSocket(), snapshot.browserName() },
    );
}

fn handleCaContext(fd: std.posix.fd_t, req: ?*c.Termsurf__CaContext) void {
    const ca_context = req orelse {
        log.warn("CaContext: missing payload", .{});
        return;
    };
    if (ca_context.*.ca_context_id == 0) {
        log.warn("CaContext: missing context id tab_id={}", .{ca_context.*.tab_id});
        return;
    }

    var overlay_snapshot: ?OverlaySnapshot = null;

    state_mutex.lock();

    const server_index = findServerByFd(fd) orelse {
        log.warn("CaContext: unknown browser fd={} tab_id={}", .{ fd, ca_context.*.tab_id });
        state_mutex.unlock();
        return;
    };
    const profile = servers[server_index].profileName();
    const browser = servers[server_index].browserName();
    const lookup_index = findTabLookup(profile, browser, ca_context.*.tab_id) orelse {
        log.warn(
            "CaContext: unknown tab key={s}/{s} tab_id={}",
            .{ profile, browser, ca_context.*.tab_id },
        );
        state_mutex.unlock();
        return;
    };
    const pane_id = tab_lookups[lookup_index].paneId();
    const pane_index = findPane(pane_id) orelse {
        log.warn("CaContext: tab maps to missing pane_id={s}", .{pane_id});
        state_mutex.unlock();
        return;
    };

    panes[pane_index].ca_context_id = ca_context.*.ca_context_id;
    panes[pane_index].ca_pixel_width = ca_context.*.pixel_width;
    panes[pane_index].ca_pixel_height = ca_context.*.pixel_height;
    overlay_snapshot = snapshotOverlay(&panes[pane_index]);
    geometryTracePane("ca_context", &panes[pane_index], "received-ca-context");

    log.info(
        "CaContext: tab_id={} pane_id={s} context_id={} pixel={}x{}",
        .{
            ca_context.*.tab_id,
            pane_id,
            ca_context.*.ca_context_id,
            ca_context.*.pixel_width,
            ca_context.*.pixel_height,
        },
    );
    state_mutex.unlock();

    if (overlay_snapshot) |snapshot| presentOverlay(&snapshot);
}

fn handleCursorChanged(fd: std.posix.fd_t, req: ?*c.Termsurf__CursorChanged) void {
    const cursor = req orelse {
        log.warn("CursorChanged: missing payload", .{});
        return;
    };

    var cursor_snapshot: ?CursorSnapshot = null;

    state_mutex.lock();

    const server_index = findServerByFd(fd) orelse {
        log.warn("CursorChanged: unknown browser fd={} tab_id={}", .{ fd, cursor.*.tab_id });
        state_mutex.unlock();
        return;
    };
    const profile = servers[server_index].profileName();
    const browser = servers[server_index].browserName();
    const lookup_index = findTabLookup(profile, browser, cursor.*.tab_id) orelse {
        log.warn(
            "CursorChanged: unknown tab key={s}/{s} tab_id={} cursor_type={}",
            .{ profile, browser, cursor.*.tab_id, cursor.*.cursor_type },
        );
        state_mutex.unlock();
        return;
    };
    const pane_id = tab_lookups[lookup_index].paneId();
    const pane_index = findPane(pane_id) orelse {
        log.warn("CursorChanged: tab maps to missing pane_id={s}", .{pane_id});
        state_mutex.unlock();
        return;
    };

    panes[pane_index].cursor_type = cursor.*.cursor_type;
    var snapshot: CursorSnapshot = .{
        .tab_id = cursor.*.tab_id,
        .cursor_type = cursor.*.cursor_type,
    };
    if (copyText(&snapshot.pane_id, &snapshot.pane_id_len, panes[pane_index].paneId())) {
        cursor_snapshot = snapshot;
    } else {
        log.warn("CursorChanged: pane_id too long pane_id={s}", .{panes[pane_index].paneId()});
    }

    log.info(
        "CursorChanged: pane_id={s} tab_id={} cursor_type={}",
        .{ panes[pane_index].paneId(), cursor.*.tab_id, cursor.*.cursor_type },
    );
    state_mutex.unlock();

    if (cursor_snapshot) |cursor_snapshot_value| setCursor(&cursor_snapshot_value);
}

fn handleTitleChanged(fd: std.posix.fd_t, req: ?*c.Termsurf__TitleChanged) void {
    const title_changed = req orelse {
        log.warn("TitleChanged: missing payload", .{});
        return;
    };
    const title = cString(title_changed.*.title);
    if (title.len == 0) {
        log.info("TitleChanged: empty title ignored tab_id={}", .{title_changed.*.tab_id});
        return;
    }

    var title_snapshot: ?TitleSnapshot = null;

    state_mutex.lock();

    const server_index = findServerByFd(fd) orelse {
        log.warn("TitleChanged: unknown browser fd={} tab_id={}", .{ fd, title_changed.*.tab_id });
        state_mutex.unlock();
        return;
    };
    const profile = servers[server_index].profileName();
    const browser = servers[server_index].browserName();
    const lookup_index = findTabLookup(profile, browser, title_changed.*.tab_id) orelse {
        log.warn(
            "TitleChanged: unknown tab key={s}/{s} tab_id={} title={s}",
            .{ profile, browser, title_changed.*.tab_id, title },
        );
        state_mutex.unlock();
        return;
    };
    const pane_id = tab_lookups[lookup_index].paneId();
    const pane_index = findPane(pane_id) orelse {
        log.warn("TitleChanged: tab maps to missing pane_id={s}", .{pane_id});
        state_mutex.unlock();
        return;
    };

    var snapshot: TitleSnapshot = .{
        .tab_id = title_changed.*.tab_id,
    };
    if (!copyText(&snapshot.pane_id, &snapshot.pane_id_len, panes[pane_index].paneId())) {
        log.warn("TitleChanged: pane_id too long pane_id={s}", .{panes[pane_index].paneId()});
    } else if (!copyText(&snapshot.title, &snapshot.title_len, title)) {
        log.warn("TitleChanged: title too long pane_id={s} tab_id={}", .{
            panes[pane_index].paneId(),
            title_changed.*.tab_id,
        });
    } else {
        title_snapshot = snapshot;
    }

    log.info(
        "TitleChanged: pane_id={s} tab_id={} title={s}",
        .{ panes[pane_index].paneId(), title_changed.*.tab_id, title },
    );
    state_mutex.unlock();

    if (title_snapshot) |title_snapshot_value| setTitle(&title_snapshot_value);
}

fn snapshotOverlay(pane: *const PaneState) ?OverlaySnapshot {
    if (pane.ca_context_id == 0) return null;
    if (pane.width == 0 or pane.height == 0) return null;

    var snapshot: OverlaySnapshot = .{
        .context_id = pane.ca_context_id,
        .col = pane.col,
        .row = pane.row,
        .width = pane.width,
        .height = pane.height,
        .pixel_width = pane.ca_pixel_width,
        .pixel_height = pane.ca_pixel_height,
    };
    if (!copyText(&snapshot.pane_id, &snapshot.pane_id_len, pane.paneId())) return null;
    return snapshot;
}

fn presentOverlay(snapshot: *const OverlaySnapshot) void {
    geometryTraceOverlay("present_overlay_call", snapshot, "calling-appkit-bridge");
    termsurf_present_overlay(
        snapshot.pane_id[0..snapshot.pane_id_len :0].ptr,
        snapshot.context_id,
        snapshot.col,
        snapshot.row,
        snapshot.width,
        snapshot.height,
        snapshot.pixel_width,
        snapshot.pixel_height,
    );
    log.info(
        "PresentOverlay: pane_id={s} context_id={} grid={}x{}+{}+{} pixel={}x{}",
        .{
            snapshot.paneId(),
            snapshot.context_id,
            snapshot.width,
            snapshot.height,
            snapshot.col,
            snapshot.row,
            snapshot.pixel_width,
            snapshot.pixel_height,
        },
    );
}

fn setCursor(snapshot: *const CursorSnapshot) void {
    termsurf_set_cursor(
        snapshot.pane_id[0..snapshot.pane_id_len :0].ptr,
        snapshot.cursor_type,
    );
    log.info(
        "SetCursor: pane_id={s} tab_id={} cursor_type={}",
        .{ snapshot.paneId(), snapshot.tab_id, snapshot.cursor_type },
    );
}

fn setTitle(snapshot: *const TitleSnapshot) void {
    termsurf_set_title(
        snapshot.pane_id[0..snapshot.pane_id_len :0].ptr,
        snapshot.title[0..snapshot.title_len :0].ptr,
    );
    log.info(
        "SetTitle: pane_id={s} tab_id={} title={s}",
        .{ snapshot.paneId(), snapshot.tab_id, snapshot.titleText() },
    );
}

fn upsertTabLookup(pane: *const PaneState, tab_id: i64) usize {
    if (findTabLookup(pane.profileName(), pane.browserName(), tab_id)) |index| {
        _ = copyText(&tab_lookups[index].pane_id, &tab_lookups[index].pane_id_len, pane.paneId());
        return countTabLookups();
    }

    const index = reserveTabLookup() orelse {
        log.warn(
            "TabReady: tab lookup limit reached key={s}/{s} tab_id={}",
            .{ pane.profileName(), pane.browserName(), tab_id },
        );
        return countTabLookups();
    };

    var lookup = &tab_lookups[index];
    _ = copyText(&lookup.profile, &lookup.profile_len, pane.profileName());
    _ = copyText(&lookup.browser, &lookup.browser_len, pane.browserName());
    _ = copyText(&lookup.pane_id, &lookup.pane_id_len, pane.paneId());
    lookup.tab_id = tab_id;
    lookup.in_use = true;
    return countTabLookups();
}

fn findTabLookup(profile: []const u8, browser: []const u8, tab_id: i64) ?usize {
    for (&tab_lookups, 0..) |*lookup, i| {
        if (lookup.in_use and
            lookup.tab_id == tab_id and
            std.mem.eql(u8, lookup.profileName(), profile) and
            std.mem.eql(u8, lookup.browserName(), browser))
        {
            return i;
        }
    }
    return null;
}

fn reserveTabLookup() ?usize {
    for (&tab_lookups, 0..) |*lookup, i| {
        if (!lookup.in_use) return i;
    }
    return null;
}

fn countTabLookups() usize {
    var count: usize = 0;
    for (&tab_lookups) |*lookup| {
        if (lookup.in_use) count += 1;
    }
    return count;
}

fn buildListenSocket(server: *ServerState) bool {
    const tmpdir = std.posix.getenv("TMPDIR") orelse "/tmp";
    const sep = if (std.mem.endsWith(u8, tmpdir, "/")) "" else "/";
    const browser_base = std.fs.path.basename(server.browserName());
    const pid = c.getpid();
    const socket = std.fmt.bufPrintZ(
        &server.listen_socket,
        "{s}{s}termsurf/{s}-{}-{s}.sock",
        .{ tmpdir, sep, browser_base, pid, server.profileName() },
    ) catch return false;
    server.listen_socket_len = socket.len;
    return true;
}

fn isAbsolutePath(path: []const u8) bool {
    return path.len > 0 and path[0] == '/';
}

fn recordServerChild(profile: []const u8, browser: []const u8, pid: std.process.Child.Id) void {
    state_mutex.lock();
    defer state_mutex.unlock();

    if (findServer(profile, browser)) |index| {
        servers[index].child_pid = pid;
    }
}

fn cleanupBrowserConnection(fd: std.posix.fd_t) void {
    var server_shutdown: ?ServerShutdownSnapshot = null;

    state_mutex.lock();
    if (findServerByFd(fd)) |server_index| {
        const server = &servers[server_index];
        var snapshot: ServerShutdownSnapshot = .{
            .browser_fd = -1,
            .child_pid = server.child_pid,
        };
        const copied =
            copyText(&snapshot.profile, &snapshot.profile_len, server.profileName()) and
            copyText(&snapshot.browser, &snapshot.browser_len, server.browserName());
        if (copied) {
            server_shutdown = snapshot;
        }
        server.attached_fd = -1;
        server.child_pid = 0;
    }
    state_mutex.unlock();

    if (server_shutdown) |*snapshot| {
        log.info(
            "Browser disconnect: detached browser server profile={s} browser={s} fd={}",
            .{ snapshot.profileName(), snapshot.browserName(), fd },
        );
        reapServerChild(snapshot);
    } else {
        log.info("Browser disconnect: no attached server fd={}", .{fd});
    }
}

fn containsFd(fds: []const std.posix.fd_t, fd: std.posix.fd_t) bool {
    for (fds) |candidate| {
        if (candidate == fd) return true;
    }
    return false;
}

fn cleanupTuiPanes(fd: std.posix.fd_t) void {
    var close_tabs: [max_panes]CloseTabSnapshot = undefined;
    var close_tab_count: usize = 0;
    var clear_overlays: [max_panes]ClearOverlaySnapshot = undefined;
    var clear_overlay_count: usize = 0;
    var server_shutdowns: [max_servers]ServerShutdownSnapshot = undefined;
    var server_shutdown_count: usize = 0;

    state_mutex.lock();

    for (&panes) |*pane| {
        if (!pane.in_use or pane.tui_fd != fd) continue;

        const pane_id = pane.paneId();
        const profile = pane.profileName();
        const browser = pane.browserName();
        const tab_id = pane.tab_id;

        if (last_browser_pane_len > 0 and
            std.mem.eql(u8, last_browser_pane[0..last_browser_pane_len], pane_id))
        {
            last_browser_pane_len = 0;
        }

        if (tab_id != 0) removeTabLookupForPane(profile, browser, tab_id, pane_id);

        if (findServer(profile, browser)) |server_index| {
            if (servers[server_index].pane_count > 0) {
                servers[server_index].pane_count -= 1;
            }

            if (tab_id != 0 and servers[server_index].attached_fd >= 0) {
                var snapshot: CloseTabSnapshot = .{
                    .browser_fd = servers[server_index].attached_fd,
                    .tab_id = tab_id,
                };
                if (copyText(&snapshot.pane_id, &snapshot.pane_id_len, pane_id)) {
                    close_tabs[close_tab_count] = snapshot;
                    close_tab_count += 1;
                }
            }

            if (servers[server_index].pane_count == 0) {
                if (snapshotServerShutdown(&servers[server_index])) |snapshot| {
                    server_shutdowns[server_shutdown_count] = snapshot;
                    server_shutdown_count += 1;
                }
                servers[server_index] = .{};
            }
        }

        if (pane.ca_context_id != 0) {
            var clear_snapshot: ClearOverlaySnapshot = .{};
            if (copyText(&clear_snapshot.pane_id, &clear_snapshot.pane_id_len, pane_id)) {
                clear_overlays[clear_overlay_count] = clear_snapshot;
                clear_overlay_count += 1;
            }
        }

        log.info("TUI disconnect cleanup: pane_id={s} tab_id={}", .{ pane_id, tab_id });
        pane.* = .{};
    }

    forgetClosedPanesForFd(fd);
    state_mutex.unlock();

    for (clear_overlays[0..clear_overlay_count]) |*snapshot| {
        clearOverlay(snapshot);
    }

    var detached_browser_fds: [max_panes]std.posix.fd_t = undefined;
    var detached_browser_fd_count: usize = 0;
    for (close_tabs[0..close_tab_count]) |*snapshot| {
        if (!sendCloseTabForCleanup(snapshot)) {
            detached_browser_fds[detached_browser_fd_count] = snapshot.browser_fd;
            detached_browser_fd_count += 1;
        }
    }

    for (server_shutdowns[0..server_shutdown_count]) |*snapshot| {
        if (containsFd(detached_browser_fds[0..detached_browser_fd_count], snapshot.browser_fd)) {
            snapshot.browser_fd = -1;
        }
        shutdownServer(snapshot);
    }
}

pub fn paneClosed(pane_id: []const u8) void {
    cleanupAppFrontendsForPane(pane_id);

    var close_tab: ?CloseTabSnapshot = null;
    var clear_overlay: ?ClearOverlaySnapshot = null;
    var server_shutdown: ?ServerShutdownSnapshot = null;

    state_mutex.lock();

    if (findPane(pane_id)) |pane_index| {
        const pane = &panes[pane_index];
        const profile = pane.profileName();
        const browser = pane.browserName();
        const tab_id = pane.tab_id;
        const tui_fd = pane.tui_fd;

        if (last_browser_pane_len > 0 and
            std.mem.eql(u8, last_browser_pane[0..last_browser_pane_len], pane_id))
        {
            last_browser_pane_len = 0;
        }

        if (tab_id != 0) removeTabLookupForPane(profile, browser, tab_id, pane_id);

        if (findServer(profile, browser)) |server_index| {
            if (servers[server_index].pane_count > 0) {
                servers[server_index].pane_count -= 1;
            }

            if (tab_id != 0 and servers[server_index].attached_fd >= 0) {
                var snapshot: CloseTabSnapshot = .{
                    .browser_fd = servers[server_index].attached_fd,
                    .tab_id = tab_id,
                };
                if (copyText(&snapshot.pane_id, &snapshot.pane_id_len, pane_id)) {
                    close_tab = snapshot;
                }
            }

            if (servers[server_index].pane_count == 0) {
                server_shutdown = snapshotServerShutdown(&servers[server_index]);
                servers[server_index] = .{};
            }
        }

        if (pane.ca_context_id != 0) {
            var snapshot: ClearOverlaySnapshot = .{};
            if (copyText(&snapshot.pane_id, &snapshot.pane_id_len, pane_id)) {
                clear_overlay = snapshot;
            }
        }

        rememberClosedPane(pane_id, tui_fd);
        log.info("Pane close cleanup: pane_id={s} tab_id={}", .{ pane_id, tab_id });
        pane.* = .{};
    } else {
        log.info("Pane close cleanup skipped: unknown pane_id={s}", .{pane_id});
    }

    state_mutex.unlock();

    if (clear_overlay) |*snapshot| {
        clearOverlay(snapshot);
    }

    if (close_tab) |*snapshot| {
        if (!sendCloseTabForCleanup(snapshot)) {
            if (server_shutdown) |*shutdown| {
                if (shutdown.browser_fd == snapshot.browser_fd) {
                    shutdown.browser_fd = -1;
                }
            }
        }
    }

    if (server_shutdown) |*snapshot| {
        shutdownServer(snapshot);
    }
}

fn clearOverlay(snapshot: *const ClearOverlaySnapshot) void {
    geometryTraceClear("clear_overlay_call", snapshot, "calling-appkit-bridge");
    termsurf_clear_overlay(snapshot.pane_id[0..snapshot.pane_id_len :0].ptr);
    log.info("ClearOverlay: pane_id={s}", .{snapshot.paneId()});
}

fn removeTabLookupForPane(profile: []const u8, browser: []const u8, tab_id: i64, pane_id: []const u8) void {
    for (&tab_lookups) |*lookup| {
        if (lookup.in_use and
            lookup.tab_id == tab_id and
            std.mem.eql(u8, lookup.profileName(), profile) and
            std.mem.eql(u8, lookup.browserName(), browser) and
            std.mem.eql(u8, lookup.paneId(), pane_id))
        {
            lookup.* = .{};
        }
    }
}

fn spawnBrowserProcess(
    profile_z: [:0]const u8,
    browser_name_z: [:0]const u8,
    browser_executable_z: [:0]const u8,
    listen_socket_z: [:0]const u8,
) ?std.process.Child.Id {
    const gui_socket = socket_path_buf[0..socket_path_len];
    if (gui_socket.len == 0) {
        log.warn("browser spawn skipped: GUI socket path is empty", .{});
        return null;
    }

    var ipc_arg_buf: [std.fs.max_path_bytes + 32]u8 = undefined;
    const ipc_arg = std.fmt.bufPrintZ(&ipc_arg_buf, "--ipc-socket={s}", .{gui_socket}) catch {
        log.warn("browser spawn skipped: ipc socket arg too long", .{});
        return null;
    };

    var listen_arg_buf: [std.fs.max_path_bytes + 32]u8 = undefined;
    const listen_arg = std.fmt.bufPrintZ(&listen_arg_buf, "--listen-socket={s}", .{listen_socket_z}) catch {
        log.warn("browser spawn skipped: listen socket arg too long", .{});
        return null;
    };

    var browser_name_arg_buf: [max_browser_len + 32]u8 = undefined;
    const browser_name_arg = std.fmt.bufPrintZ(&browser_name_arg_buf, "--browser-name={s}", .{browser_name_z}) catch {
        log.warn("browser spawn skipped: browser-name arg too long", .{});
        return null;
    };

    var user_data_dir_buf: [std.fs.max_path_bytes]u8 = undefined;
    const user_data_dir = buildUserDataDir(&user_data_dir_buf, browser_name_z, profile_z) orelse return null;
    var data_arg_buf: [std.fs.max_path_bytes + 32]u8 = undefined;
    const data_arg = std.fmt.bufPrintZ(&data_arg_buf, "--user-data-dir={s}", .{user_data_dir}) catch {
        log.warn("browser spawn skipped: user-data-dir arg too long", .{});
        return null;
    };

    var log_file_buf: [std.fs.max_path_bytes]u8 = undefined;
    const log_file = buildBrowserLogFile(&log_file_buf) orelse return null;
    var log_arg_buf: [std.fs.max_path_bytes + 32]u8 = undefined;
    const log_arg = std.fmt.bufPrintZ(&log_arg_buf, "--log-file={s}", .{log_file}) catch {
        log.warn("browser spawn skipped: log-file arg too long", .{});
        return null;
    };

    const argv = [_][]const u8{
        browser_executable_z,
        ipc_arg,
        browser_name_arg,
        data_arg,
        listen_arg,
        "--hidden",
        "--no-sandbox",
        "--enable-logging",
        log_arg,
    };

    var child = std.process.Child.init(&argv, std.heap.c_allocator);
    child.stdin_behavior = .Ignore;
    child.stdout_behavior = .Inherit;
    child.stderr_behavior = .Inherit;
    var child_env = std.process.getEnvMap(std.heap.c_allocator) catch |err| blk: {
        log.warn("browser spawn env map unavailable browser={s} err={}", .{ browser_name_z, err });
        break :blk null;
    };
    defer if (child_env) |*env| env.deinit();
    if (child_env) |*env| {
        const inherited_framework_path = env.get("DYLD_FRAMEWORK_PATH");
        if (inherited_framework_path) |framework_path| {
            if (std.mem.eql(u8, browser_name_z, "surfari")) {
                log.info(
                    "browser spawn runtime env browser={s} inherited DYLD_FRAMEWORK_PATH={s}",
                    .{ browser_name_z, framework_path },
                );
            }
        } else if (shouldSupplySurfariRuntimeFrameworkPath(browser_name_z, inherited_framework_path)) {
            const framework_path = surfariRuntimeFrameworkPath(browser_name_z, browser_executable_z).?;
            env.put("DYLD_FRAMEWORK_PATH", framework_path) catch |err| {
                log.warn(
                    "browser spawn runtime env skipped browser={s} path={s} err={}",
                    .{ browser_name_z, framework_path, err },
                );
            };
            log.info(
                "browser spawn runtime env browser={s} DYLD_FRAMEWORK_PATH={s}",
                .{ browser_name_z, framework_path },
            );
        }
        child.env_map = env;
    }
    child.spawn() catch |err| {
        log.warn("browser spawn failed path={s} profile={s} browser={s} err={}", .{ browser_executable_z, profile_z, browser_name_z, err });
        return null;
    };

    log.info(
        "spawned browser path={s} pid={} profile={s} browser={s} argv={s} {s} {s} {s} {s} --hidden --no-sandbox --enable-logging {s}",
        .{ browser_executable_z, child.id, profile_z, browser_name_z, browser_executable_z, ipc_arg, browser_name_arg, data_arg, listen_arg, log_arg },
    );
    return child.id;
}

fn shouldSupplySurfariRuntimeFrameworkPath(browser: []const u8, inherited_framework_path: ?[]const u8) bool {
    return std.mem.eql(u8, browser, "surfari") and inherited_framework_path == null;
}

fn surfariRuntimeFrameworkPath(browser: []const u8, browser_executable: []const u8) ?[]const u8 {
    if (!std.mem.eql(u8, browser, "surfari")) return null;
    return std.fs.path.dirname(browser_executable);
}

fn buildUserDataDir(buf: []u8, browser: []const u8, profile: []const u8) ?[:0]u8 {
    const home = std.posix.getenv("HOME") orelse {
        log.warn("browser spawn skipped: HOME is not set", .{});
        return null;
    };
    const data_home = std.posix.getenv("XDG_DATA_HOME");
    const profile_dir = browserProfileDirName(browser);
    return if (data_home) |base|
        std.fmt.bufPrintZ(buf, "{s}/termsurf/{s}/{s}", .{ base, profile_dir, profile }) catch null
    else
        std.fmt.bufPrintZ(buf, "{s}/.local/share/termsurf/{s}/{s}", .{ home, profile_dir, profile }) catch null;
}

fn browserProfileDirName(browser: []const u8) []const u8 {
    if (std.mem.eql(u8, browser, "surfari")) return "webkit-profiles";
    if (std.mem.eql(u8, browser, default_browser)) return "chromium-profiles";
    return "browser-profiles";
}

fn buildBrowserLogFile(buf: []u8) ?[:0]u8 {
    const home = std.posix.getenv("HOME") orelse {
        log.warn("browser spawn skipped: HOME is not set", .{});
        return null;
    };
    const state_home = std.posix.getenv("XDG_STATE_HOME");
    var dir_buf: [std.fs.max_path_bytes]u8 = undefined;
    const dir = if (state_home) |base|
        std.fmt.bufPrintZ(&dir_buf, "{s}/termsurf", .{base}) catch return null
    else
        std.fmt.bufPrintZ(&dir_buf, "{s}/.local/state/termsurf", .{home}) catch return null;
    std.fs.makeDirAbsolute(dir) catch |err| switch (err) {
        error.PathAlreadyExists => {},
        else => log.warn("browser log directory create failed path={s} err={}", .{ dir, err }),
    };
    return std.fmt.bufPrintZ(buf, "{s}/chromium-server.log", .{dir}) catch null;
}

fn setServer(server: *ServerState, profile: []const u8, browser: []const u8) bool {
    if (!copyText(&server.profile, &server.profile_len, profile)) {
        log.warn("SetOverlay: server profile too long len={} max={}", .{ profile.len, max_profile_len });
        return false;
    }
    if (!copyText(&server.browser, &server.browser_len, browser)) {
        log.warn("SetOverlay: server browser too long len={} max={}", .{ browser.len, max_browser_len });
        return false;
    }

    server.in_use = true;
    server.pane_count = 1;
    server.attached_fd = -1;
    server.listen_socket_len = 0;
    server.child_pid = 0;
    return true;
}

fn findPane(pane_id: []const u8) ?usize {
    for (&panes, 0..) |*pane, i| {
        if (pane.in_use and std.mem.eql(u8, pane.paneId(), pane_id)) return i;
    }
    return null;
}

fn findClosedPane(pane_id: []const u8, tui_fd: std.posix.fd_t) ?usize {
    for (&closed_panes, 0..) |*pane, i| {
        if (pane.in_use and
            pane.tui_fd == tui_fd and
            std.mem.eql(u8, pane.paneId(), pane_id))
        {
            return i;
        }
    }
    return null;
}

fn rememberClosedPane(pane_id: []const u8, tui_fd: std.posix.fd_t) void {
    if (tui_fd < 0) return;
    if (findClosedPane(pane_id, tui_fd) != null) return;

    for (&closed_panes) |*pane| {
        if (!pane.in_use) {
            if (!copyText(&pane.pane_id, &pane.pane_id_len, pane_id)) return;
            pane.tui_fd = tui_fd;
            pane.in_use = true;
            return;
        }
    }

    log.warn("Closed pane limit reached pane_id={s} fd={}", .{ pane_id, tui_fd });
}

fn forgetClosedPanesForFd(tui_fd: std.posix.fd_t) void {
    for (&closed_panes) |*pane| {
        if (pane.in_use and pane.tui_fd == tui_fd) {
            pane.* = .{};
        }
    }
}

fn snapshotServerShutdown(server: *const ServerState) ?ServerShutdownSnapshot {
    if (server.attached_fd < 0) return null;

    var snapshot: ServerShutdownSnapshot = .{
        .browser_fd = server.attached_fd,
        .child_pid = server.child_pid,
    };
    if (!copyText(&snapshot.profile, &snapshot.profile_len, server.profileName())) return null;
    if (!copyText(&snapshot.browser, &snapshot.browser_len, server.browserName())) return null;
    return snapshot;
}

fn findServer(profile: []const u8, browser: []const u8) ?usize {
    for (&servers, 0..) |*server, i| {
        if (server.in_use and
            std.mem.eql(u8, server.profileName(), profile) and
            std.mem.eql(u8, server.browserName(), browser))
        {
            return i;
        }
    }
    return null;
}

fn findServerByFd(fd: std.posix.fd_t) ?usize {
    for (&servers, 0..) |*server, i| {
        if (server.in_use and server.attached_fd == fd) return i;
    }
    return null;
}

fn findAttachableServer(profile: []const u8, browser: []const u8) ?usize {
    for (&servers, 0..) |*server, i| {
        if (server.in_use and
            server.attached_fd < 0 and
            std.mem.eql(u8, server.profileName(), profile) and
            std.mem.eql(u8, server.browserName(), browser))
        {
            return i;
        }
    }
    return null;
}

fn reservePane() ?usize {
    for (&panes, 0..) |*pane, i| {
        if (!pane.in_use) return i;
    }
    return null;
}

fn reserveServer() ?usize {
    for (&servers, 0..) |*server, i| {
        if (!server.in_use) return i;
    }
    return null;
}

fn reserveApp() ?usize {
    for (&apps, 0..) |*app, i| {
        if (!app.in_use) return i;
    }
    return null;
}

fn reserveAppFrontend() ?usize {
    for (&app_frontends, 0..) |*frontend, i| {
        if (!frontend.in_use) return i;
    }
    return null;
}

fn findApp(app_id: []const u8) ?usize {
    for (&apps, 0..) |*app, i| {
        if (app.in_use and std.mem.eql(u8, app.appId(), app_id)) return i;
    }
    return null;
}

fn findAppFrontendById(app_id: []const u8, frontend_id: []const u8) ?usize {
    for (&app_frontends, 0..) |*frontend, i| {
        if (frontend.in_use and
            std.mem.eql(u8, frontend.appId(), app_id) and
            std.mem.eql(u8, frontend.frontendId(), frontend_id))
        {
            return i;
        }
    }
    return null;
}

fn frontendCountForApp(app_id: []const u8) usize {
    var count: usize = 0;
    for (&app_frontends) |*frontend| {
        if (frontend.in_use and std.mem.eql(u8, frontend.appId(), app_id)) count += 1;
    }
    return count;
}

fn setApp(app: *AppState, app_id: []const u8, url: []const u8, port: u16, child_pid: std.process.Child.Id) bool {
    if (!copyText(&app.app_id, &app.app_id_len, app_id)) return false;
    if (!copyText(&app.url, &app.url_len, url)) return false;
    app.port = port;
    app.child_pid = child_pid;
    app.starting = false;
    app.in_use = true;
    return true;
}

fn setAppStarting(app: *AppState, app_id: []const u8) bool {
    app.* = .{};
    if (!copyText(&app.app_id, &app.app_id_len, app_id)) return false;
    app.in_use = true;
    app.starting = true;
    return true;
}

fn setAppFrontend(frontend: *AppFrontendState, app_id: []const u8, frontend_id: []const u8, pane_id: []const u8, tui_fd: std.posix.fd_t) bool {
    if (!copyText(&frontend.app_id, &frontend.app_id_len, app_id)) return false;
    if (!copyText(&frontend.frontend_id, &frontend.frontend_id_len, frontend_id)) return false;
    if (!copyText(&frontend.pane_id, &frontend.pane_id_len, pane_id)) return false;
    frontend.tui_fd = tui_fd;
    frontend.in_use = true;
    return true;
}

fn findAppDescriptor(app_id: []const u8) ?*const AppDescriptor {
    for (&app_descriptors) |*descriptor| {
        if (std.mem.eql(u8, descriptor.app_id, app_id)) return descriptor;
    }
    return null;
}

fn openOrReuseApp(
    descriptor: *const AppDescriptor,
    tui_fd: std.posix.fd_t,
    pane_id: []const u8,
    app_id: []const u8,
    browser: []const u8,
    profile: []const u8,
    entrypoint: []const u8,
    frontend_id_buf: *[max_app_frontend_id_len]u8,
    frontend_id_len: *usize,
    url_buf: *[max_app_url_len]u8,
    url_len: *usize,
    error_buf: *[256]u8,
) bool {
    _ = browser;
    _ = profile;
    error_buf[0] = 0;

    var app_url_copy: [max_app_url_len]u8 = undefined;
    var app_url_copy_len: usize = 0;
    var should_spawn = false;
    var app_index: usize = undefined;
    var port: u16 = 0;
    var url_z_buf: [max_app_url_len]u8 = undefined;
    var app_path_buf: [std.fs.max_path_bytes]u8 = undefined;
    var runtime_path_buf: [std.fs.max_path_bytes]u8 = undefined;

    const startup_deadline = std.time.milliTimestamp() + @as(i64, @intCast(descriptor.readiness_timeout_ms));
    while (true) {
        state_mutex.lock();
        if (findApp(app_id)) |existing| {
            if (apps[existing].starting) {
                state_mutex.unlock();
                if (std.time.milliTimestamp() >= startup_deadline) {
                    _ = copyError(error_buf, "app startup timed out");
                    return false;
                }
                std.Thread.sleep(50 * std.time.ns_per_ms);
                continue;
            } else if (appChildExited(apps[existing].child_pid)) {
                log.warn("OpenApp: stale app backend exited app_id={s} pid={}", .{ app_id, apps[existing].child_pid });
                apps[existing] = .{};
                for (&app_frontends) |*frontend| {
                    if (frontend.in_use and std.mem.eql(u8, frontend.appId(), app_id)) {
                        frontend.* = .{};
                    }
                }
                app_index = existing;
                if (!setAppStarting(&apps[app_index], app_id)) {
                    state_mutex.unlock();
                    _ = copyError(error_buf, "failed to reserve app state");
                    return false;
                }
                should_spawn = true;
            } else {
                app_index = existing;
                if (!copyText(&app_url_copy, &app_url_copy_len, apps[existing].appUrl())) {
                    state_mutex.unlock();
                    _ = copyError(error_buf, "app URL too long");
                    return false;
                }
            }
        } else {
            app_index = reserveApp() orelse {
                state_mutex.unlock();
                _ = copyError(error_buf, "app limit reached");
                return false;
            };
            if (!setAppStarting(&apps[app_index], app_id)) {
                state_mutex.unlock();
                _ = copyError(error_buf, "failed to reserve app state");
                return false;
            }
            should_spawn = true;
        }
        state_mutex.unlock();
        break;
    }

    if (should_spawn) {
        port = chooseLocalPort() orelse {
            state_mutex.lock();
            apps[app_index] = .{};
            state_mutex.unlock();
            _ = copyError(error_buf, "no local port available");
            return false;
        };
        const url_z = std.fmt.bufPrintZ(&url_z_buf, "http://127.0.0.1:{d}/", .{port}) catch {
            state_mutex.lock();
            apps[app_index] = .{};
            state_mutex.unlock();
            _ = copyError(error_buf, "app URL too long");
            return false;
        };
        const app_path_z = resolveAppPath(descriptor, entrypoint, &app_path_buf) orelse {
            state_mutex.lock();
            apps[app_index] = .{};
            state_mutex.unlock();
            _ = copyError(error_buf, "app entrypoint not found");
            return false;
        };
        const runtime_path_z = resolveRuntimePath(descriptor, &runtime_path_buf) orelse {
            state_mutex.lock();
            apps[app_index] = .{};
            state_mutex.unlock();
            _ = copyError(error_buf, "app runtime executable not found");
            return false;
        };
        const child_pid = spawnAppBackend(descriptor, runtime_path_z, app_path_z, port) orelse {
            state_mutex.lock();
            apps[app_index] = .{};
            state_mutex.unlock();
            _ = copyError(error_buf, "failed to spawn app backend");
            return false;
        };
        const ready_result = waitForAppReady(child_pid, port, descriptor.readiness_timeout_ms);
        if (ready_result != .ready) {
            state_mutex.lock();
            apps[app_index] = .{};
            state_mutex.unlock();
            if (ready_result == .timeout) {
                var snapshot: AppShutdownSnapshot = .{ .child_pid = child_pid };
                _ = copyText(&snapshot.app_id, &snapshot.app_id_len, app_id);
                stopAppBackend(&snapshot);
            }
            _ = copyError(error_buf, "app backend did not become ready");
            return false;
        }

        state_mutex.lock();
        if (!setApp(&apps[app_index], app_id, url_z, port, child_pid)) {
            state_mutex.unlock();
            var snapshot: AppShutdownSnapshot = .{ .child_pid = child_pid };
            _ = copyText(&snapshot.app_id, &snapshot.app_id_len, app_id);
            stopAppBackend(&snapshot);
            _ = copyError(error_buf, "failed to record app state");
            return false;
        }
        if (!copyText(&app_url_copy, &app_url_copy_len, apps[app_index].appUrl())) {
            const snapshot = snapshotAppShutdown(&apps[app_index]);
            apps[app_index] = .{};
            state_mutex.unlock();
            if (snapshot) |*s| stopAppBackend(s);
            _ = copyError(error_buf, "app URL too long");
            return false;
        }
        state_mutex.unlock();
    }

    const frontend_id_z = std.fmt.bufPrintZ(frontend_id_buf, "{s}-{d}-{d}", .{ app_id, std.c.getpid(), std.crypto.random.int(u32) }) catch {
        _ = copyError(error_buf, "frontend id too long");
        return false;
    };
    frontend_id_len.* = frontend_id_z.len;

    state_mutex.lock();
    const frontend_index = reserveAppFrontend() orelse {
        const shutdown = maybeSnapshotAppShutdownLocked(app_id);
        state_mutex.unlock();
        if (shutdown) |*snapshot| stopAppBackend(snapshot);
        _ = copyError(error_buf, "app frontend limit reached");
        return false;
    };
    if (!setAppFrontend(&app_frontends[frontend_index], app_id, frontend_id_z, pane_id, tui_fd)) {
        app_frontends[frontend_index] = .{};
        state_mutex.unlock();
        _ = copyError(error_buf, "failed to record app frontend");
        return false;
    }
    state_mutex.unlock();

    if (!copyText(url_buf, url_len, app_url_copy[0..app_url_copy_len])) {
        _ = copyError(error_buf, "app URL too long");
        return false;
    }
    return true;
}

fn copyError(buf: *[256]u8, message: []const u8) bool {
    var len: usize = 0;
    return copyText(buf, &len, message);
}

fn resolveRuntimePath(descriptor: *const AppDescriptor, buf: []u8) ?[:0]u8 {
    if (std.posix.getenv(descriptor.runtime_path_env)) |path| {
        return std.fmt.bufPrintZ(buf, "{s}", .{path}) catch null;
    }
    return std.fmt.bufPrintZ(buf, "{s}", .{descriptor.default_runtime_path}) catch null;
}

fn resolveAppPath(descriptor: *const AppDescriptor, entrypoint: []const u8, buf: []u8) ?[:0]u8 {
    if (entrypoint.len > 0 and std.fs.path.isAbsolute(entrypoint)) {
        return std.fmt.bufPrintZ(buf, "{s}", .{entrypoint}) catch null;
    }
    if (std.posix.getenv(descriptor.app_path_env)) |path| {
        return std.fmt.bufPrintZ(buf, "{s}", .{path}) catch null;
    }

    switch (descriptor.runtime) {
        .deno => {
            const candidates = [_][:0]const u8{
                "gtui/app/server.ts",
                "../gtui/app/server.ts",
            };
            return resolveFirstExistingPath(&candidates, buf);
        },
    }
}

fn resolveFirstExistingPath(candidates: []const [:0]const u8, buf: []u8) ?[:0]u8 {
    for (candidates) |candidate| {
        if (std.fs.cwd().realpathZ(candidate, buf)) |resolved| {
            if (resolved.len >= buf.len) return null;
            buf[resolved.len] = 0;
            return buf[0..resolved.len :0];
        } else |_| {}
    }
    return null;
}

fn chooseLocalPort() ?u16 {
    for (0..128) |_| {
        const port = std.crypto.random.intRangeAtMost(u16, 49152, 60999);
        if (!canConnectLocalPort(port)) return port;
    }
    return null;
}

fn canConnectLocalPort(port: u16) bool {
    const stream = std.net.tcpConnectToHost(std.heap.c_allocator, "127.0.0.1", port) catch return false;
    stream.close();
    return true;
}

fn waitForAppReady(child_pid: std.process.Child.Id, port: u16, timeout_ms: u64) AppReadyResult {
    const deadline = std.time.milliTimestamp() + @as(i64, @intCast(timeout_ms));
    while (std.time.milliTimestamp() < deadline) {
        if (appChildExited(child_pid)) return .exited;
        if (httpHealthzOk(port)) return .ready;
        std.Thread.sleep(100 * std.time.ns_per_ms);
    }
    return .timeout;
}

fn httpHealthzOk(port: u16) bool {
    const stream = std.net.tcpConnectToHost(std.heap.c_allocator, "127.0.0.1", port) catch return false;
    defer stream.close();

    const request = "GET /healthz HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    writeAll(stream.handle, request) catch return false;

    var response_buf: [512]u8 = undefined;
    const n = std.posix.read(stream.handle, &response_buf) catch return false;
    const response = response_buf[0..n];
    return std.mem.indexOf(u8, response, "200 OK") != null and
        std.mem.indexOf(u8, response, "\r\n\r\nok") != null;
}

fn appChildExited(pid: std.process.Child.Id) bool {
    if (pid == 0) return true;
    const res = std.posix.waitpid(pid, std.c.W.NOHANG);
    return res.pid != 0;
}

fn spawnAppBackend(descriptor: *const AppDescriptor, runtime_path_z: [:0]const u8, app_path_z: [:0]const u8, port: u16) ?std.process.Child.Id {
    var port_buf: [16]u8 = undefined;
    const port_arg = std.fmt.bufPrintZ(&port_buf, "{d}", .{port}) catch return null;
    switch (descriptor.runtime) {
        .deno => {
            const argv = [_][]const u8{
                runtime_path_z,
                "run",
                "--allow-net=127.0.0.1",
                "--no-check",
                app_path_z,
                "--host",
                "127.0.0.1",
                "--port",
                port_arg,
            };
            return spawnAppBackendArgv(descriptor, app_path_z, port, argv[0..]);
        },
    }
}

fn spawnAppBackendArgv(descriptor: *const AppDescriptor, app_path_z: [:0]const u8, port: u16, argv: []const []const u8) ?std.process.Child.Id {
    var child = std.process.Child.init(argv, std.heap.c_allocator);
    child.stdin_behavior = .Ignore;
    child.stdout_behavior = .Inherit;
    child.stderr_behavior = .Inherit;
    child.spawn() catch |err| {
        log.warn("app backend spawn failed app_id={s} app={s} err={}", .{ descriptor.app_id, app_path_z, err });
        return null;
    };
    log.info("app backend spawned app_id={s} pid={} port={}", .{ descriptor.app_id, child.id, port });
    return child.id;
}

fn stopAppBackend(snapshot: *const AppShutdownSnapshot) void {
    if (snapshot.child_pid == 0) return;
    std.posix.kill(snapshot.child_pid, std.posix.SIG.TERM) catch |err| {
        log.info("app backend kill skipped app={s} pid={} err={}", .{ snapshot.appId(), snapshot.child_pid, err });
    };
    for (0..50) |_| {
        const res = std.posix.waitpid(snapshot.child_pid, std.c.W.NOHANG);
        if (res.pid != 0) {
            log.info("app backend reaped app={s} pid={}", .{ snapshot.appId(), snapshot.child_pid });
            return;
        }
        std.Thread.sleep(100 * std.time.ns_per_ms);
    }
    log.warn("app backend reap timed out app={s} pid={} sending SIGKILL", .{ snapshot.appId(), snapshot.child_pid });
    std.posix.kill(snapshot.child_pid, std.posix.SIG.KILL) catch |err| {
        log.warn("app backend SIGKILL failed app={s} pid={} err={}", .{ snapshot.appId(), snapshot.child_pid, err });
        return;
    };
    _ = std.posix.waitpid(snapshot.child_pid, 0);
    log.info("app backend killed app={s} pid={}", .{ snapshot.appId(), snapshot.child_pid });
}

fn snapshotAppShutdown(app: *const AppState) ?AppShutdownSnapshot {
    if (app.child_pid == 0) return null;
    var snapshot: AppShutdownSnapshot = .{ .child_pid = app.child_pid };
    if (!copyText(&snapshot.app_id, &snapshot.app_id_len, app.appId())) return null;
    return snapshot;
}

fn maybeSnapshotAppShutdownLocked(app_id: []const u8) ?AppShutdownSnapshot {
    if (frontendCountForApp(app_id) != 0) return null;
    const app_index = findApp(app_id) orelse return null;
    const snapshot = snapshotAppShutdown(&apps[app_index]);
    apps[app_index] = .{};
    return snapshot;
}

fn removeAppFrontendLocked(app_id: []const u8, frontend_id: []const u8, pane_id: ?[]const u8, shutdown: *?AppShutdownSnapshot) void {
    var removed = false;
    if (frontend_id.len > 0) {
        if (findAppFrontendById(app_id, frontend_id)) |index| {
            app_frontends[index] = .{};
            removed = true;
        }
    } else if (pane_id) |pid| {
        for (&app_frontends) |*frontend| {
            if (frontend.in_use and
                std.mem.eql(u8, frontend.appId(), app_id) and
                std.mem.eql(u8, frontend.paneId(), pid))
            {
                frontend.* = .{};
                removed = true;
            }
        }
    }
    if (removed) {
        shutdown.* = maybeSnapshotAppShutdownLocked(app_id);
    }
}

fn cleanupAppFrontendsForFd(fd: std.posix.fd_t) void {
    var shutdowns: [max_apps]AppShutdownSnapshot = undefined;
    var shutdown_count: usize = 0;

    state_mutex.lock();
    for (&app_frontends) |*frontend| {
        if (!frontend.in_use or frontend.tui_fd != fd) continue;
        var app_id_buf: [max_app_id_len]u8 = undefined;
        var app_id_len: usize = 0;
        if (!copyText(&app_id_buf, &app_id_len, frontend.appId())) continue;
        frontend.* = .{};
        if (maybeSnapshotAppShutdownLocked(app_id_buf[0..app_id_len])) |snapshot| {
            shutdowns[shutdown_count] = snapshot;
            shutdown_count += 1;
        }
    }
    state_mutex.unlock();

    for (shutdowns[0..shutdown_count]) |*snapshot| stopAppBackend(snapshot);
}

fn cleanupAppFrontendsForPane(pane_id: []const u8) void {
    var shutdowns: [max_apps]AppShutdownSnapshot = undefined;
    var shutdown_count: usize = 0;

    state_mutex.lock();
    for (&app_frontends) |*frontend| {
        if (!frontend.in_use or !std.mem.eql(u8, frontend.paneId(), pane_id)) continue;
        var app_id_buf: [max_app_id_len]u8 = undefined;
        var app_id_len: usize = 0;
        if (!copyText(&app_id_buf, &app_id_len, frontend.appId())) continue;
        frontend.* = .{};
        if (maybeSnapshotAppShutdownLocked(app_id_buf[0..app_id_len])) |snapshot| {
            shutdowns[shutdown_count] = snapshot;
            shutdown_count += 1;
        }
    }
    state_mutex.unlock();

    for (shutdowns[0..shutdown_count]) |*snapshot| stopAppBackend(snapshot);
}

fn stopAllApps() void {
    var shutdowns: [max_apps]AppShutdownSnapshot = undefined;
    var shutdown_count: usize = 0;

    state_mutex.lock();
    for (&apps) |*app| {
        if (!app.in_use) continue;
        if (snapshotAppShutdown(app)) |snapshot| {
            shutdowns[shutdown_count] = snapshot;
            shutdown_count += 1;
        }
        app.* = .{};
    }
    app_frontends = [_]AppFrontendState{.{}} ** max_app_frontends;
    state_mutex.unlock();

    for (shutdowns[0..shutdown_count]) |*snapshot| stopAppBackend(snapshot);
}

fn sendOpenAppReply(fd: std.posix.fd_t, pane_id: []const u8, app_id: []const u8, frontend_id: []const u8, url: []const u8, err_text: []const u8) !void {
    var pane_buf: [max_pane_id_len]u8 = undefined;
    var pane_len: usize = 0;
    var app_buf: [max_app_id_len]u8 = undefined;
    var app_len: usize = 0;
    var frontend_buf: [max_app_frontend_id_len]u8 = undefined;
    var frontend_len: usize = 0;
    var url_buf: [max_app_url_len]u8 = undefined;
    var url_len: usize = 0;
    var error_buf: [256]u8 = undefined;
    var error_len: usize = 0;
    if (!copyText(&pane_buf, &pane_len, pane_id)) return error.PaneIdTooLong;
    if (!copyText(&app_buf, &app_len, app_id)) return error.AppIdTooLong;
    if (!copyText(&frontend_buf, &frontend_len, frontend_id)) return error.FrontendIdTooLong;
    if (!copyText(&url_buf, &url_len, url)) return error.UrlTooLong;
    if (!copyText(&error_buf, &error_len, err_text)) return error.AppErrorTooLong;

    var reply: c.Termsurf__OpenAppReply = undefined;
    c.termsurf__open_app_reply__init(&reply);
    reply.pane_id = @constCast(pane_buf[0..pane_len :0].ptr);
    reply.app_id = @constCast(app_buf[0..app_len :0].ptr);
    reply.frontend_id = @constCast(frontend_buf[0..frontend_len :0].ptr);
    reply.url = @constCast(url_buf[0..url_len :0].ptr);
    @field(reply, "error") = @constCast(error_buf[0..error_len :0].ptr);

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_OPEN_APP_REPLY;
    wrapper.unnamed_0.open_app_reply = &reply;
    try sendProtobuf(fd, &wrapper);
}

fn sendCloseAppFrontendToTui(snapshot: *const AppFrontendCloseSnapshot) !void {
    var close_app: c.Termsurf__CloseAppFrontend = undefined;
    c.termsurf__close_app_frontend__init(&close_app);
    close_app.pane_id = @constCast(snapshot.pane_id[0..snapshot.pane_id_len :0].ptr);
    close_app.app_id = @constCast(snapshot.app_id[0..snapshot.app_id_len :0].ptr);
    close_app.frontend_id = @constCast(snapshot.frontend_id[0..snapshot.frontend_id_len :0].ptr);

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_CLOSE_APP_FRONTEND;
    wrapper.unnamed_0.close_app_frontend = &close_app;
    try sendProtobuf(snapshot.tui_fd, &wrapper);
}

fn cString(ptr: [*c]u8) []const u8 {
    if (ptr) |value| return std.mem.span(value);
    return "";
}

fn copyText(buf: []u8, len: *usize, value: []const u8) bool {
    if (value.len >= buf.len) return false;
    @memcpy(buf[0..value.len], value);
    buf[value.len] = 0;
    len.* = value.len;
    return true;
}

fn sendProtobuf(fd: std.posix.fd_t, wrapper: *c.Termsurf__TermSurfMessage) !void {
    const size = c.termsurf__term_surf_message__get_packed_size(wrapper);
    if (size > max_frame_size) return error.FrameTooLarge;

    const allocator = std.heap.c_allocator;
    const payload = try allocator.alloc(u8, size);
    defer allocator.free(payload);

    const packed_size = c.termsurf__term_surf_message__pack(wrapper, payload.ptr);
    if (packed_size != size) return error.ProtobufPackFailed;

    var len_buf: [4]u8 = undefined;
    std.mem.writeInt(u32, &len_buf, @intCast(size), .little);
    try writeAll(fd, &len_buf);
    try writeAll(fd, payload);
}

fn writeAll(fd: std.posix.fd_t, bytes: []const u8) !void {
    var offset: usize = 0;
    while (offset < bytes.len) {
        const n = try std.posix.write(fd, bytes[offset..]);
        if (n == 0) return error.WriteZero;
        offset += n;
    }
}

fn msgTypeName(msg_case: c.Termsurf__TermSurfMessage__MsgCase) []const u8 {
    return switch (msg_case) {
        c.TERMSURF__TERM_SURF_MESSAGE__MSG__NOT_SET => "NotSet",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_HELLO_REQUEST => "HelloRequest",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_HELLO_REPLY => "HelloReply",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_CREATE_TAB => "CreateTab",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_CREATE_DEVTOOLS_TAB => "CreateDevtoolsTab",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_RESIZE => "Resize",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_CLOSE_TAB => "CloseTab",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_NAVIGATE => "Navigate",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_MOUSE_EVENT => "MouseEvent",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_MOUSE_MOVE => "MouseMove",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_SCROLL_EVENT => "ScrollEvent",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_KEY_EVENT => "KeyEvent",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_FOCUS_CHANGED => "FocusChanged",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_SET_COLOR_SCHEME => "SetColorScheme",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_SET_GUI_ACTIVE => "SetGuiActive",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_TAB_READY => "TabReady",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_CA_CONTEXT => "CaContext",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_URL_CHANGED => "UrlChanged",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_LOADING_STATE => "LoadingState",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_TITLE_CHANGED => "TitleChanged",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_CURSOR_CHANGED => "CursorChanged",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_TARGET_URL_CHANGED => "TargetUrlChanged",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_MODE_CHANGED => "ModeChanged",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_LAST_REQUEST => "QueryLastRequest",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_LAST_REPLY => "QueryLastReply",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_DEVTOOLS_REQUEST => "QueryDevtoolsRequest",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_DEVTOOLS_REPLY => "QueryDevtoolsReply",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_TABS_REQUEST => "QueryTabsRequest",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_TABS_REPLY => "QueryTabsReply",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_SERVER_REGISTER => "ServerRegister",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_SET_OVERLAY => "SetOverlay",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_SET_DEVTOOLS_OVERLAY => "SetDevtoolsOverlay",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_OPEN_SPLIT => "OpenSplit",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_OPEN_APP => "OpenApp",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_OPEN_APP_REPLY => "OpenAppReply",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_CLOSE_APP_FRONTEND => "CloseAppFrontend",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_BROWSER_READY => "BrowserReady",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_JAVASCRIPT_DIALOG_REQUEST => "JavascriptDialogRequest",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_JAVASCRIPT_DIALOG_REPLY => "JavascriptDialogReply",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_CONSOLE_MESSAGE => "ConsoleMessage",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_HTTP_AUTH_REQUEST => "HttpAuthRequest",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_HTTP_AUTH_REPLY => "HttpAuthReply",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_RENDERER_CRASHED => "RendererCrashed",
        else => "Other",
    };
}

fn classifyConnection(msg_case: c.Termsurf__TermSurfMessage__MsgCase) ConnType {
    return switch (msg_case) {
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_SERVER_REGISTER => .browser,
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_OPEN_APP => .tui,
        else => .tui,
    };
}

fn connTypeName(conn_type: ConnType) []const u8 {
    return switch (conn_type) {
        .unknown => "Unknown",
        .tui => "Tui",
        .browser => "Browser",
    };
}

fn wakeAccept() void {
    if (socket_path_len == 0) return;

    const fd = std.posix.socket(std.posix.AF.UNIX, std.posix.SOCK.STREAM, 0) catch return;
    defer std.posix.close(fd);

    const path = socket_path_buf[0..socket_path_len];
    const addr = std.net.Address.initUnix(path) catch return;
    std.posix.connect(fd, &addr.any, addr.getOsSockLen()) catch {};
}

fn socketPath(tmpdir: []const u8, sep: []const u8) ![:0]u8 {
    const pid = std.c.getpid();
    const path_z = std.fmt.bufPrintZ(
        &socket_path_buf,
        "{s}{s}termsurf/termsurf-ghostboard-{d}.sock",
        .{ tmpdir, sep, pid },
    ) catch return error.SocketPathTooLong;
    socket_path_len = path_z.len;
    return path_z;
}

test "termsurf server register matches profile and browser" {
    const testing = std.testing;

    resetTermsurfStateForTest();
    defer resetTermsurfStateForTest();

    const profile: [:0]const u8 = "default";
    const surfari: [:0]const u8 = "surfari";
    const roamium: [:0]const u8 = "roamium";

    try testBrowserResolutionWithRestoredEnv();

    var surfari_data_dir_buf: [std.fs.max_path_bytes]u8 = undefined;
    const surfari_data_dir = buildUserDataDir(&surfari_data_dir_buf, surfari, profile) orelse return error.UserDataDirFailed;
    try testing.expect(std.mem.indexOf(u8, surfari_data_dir, "webkit-profiles") != null);
    try testing.expect(std.mem.indexOf(u8, surfari_data_dir, "chromium-profiles") == null);

    var roamium_data_dir_buf: [std.fs.max_path_bytes]u8 = undefined;
    const roamium_data_dir = buildUserDataDir(&roamium_data_dir_buf, roamium, profile) orelse return error.UserDataDirFailed;
    try testing.expect(std.mem.indexOf(u8, roamium_data_dir, "chromium-profiles") != null);

    var surfari_browser = try testSocketPair();
    defer testCloseSocketPair(&surfari_browser);
    var roamium_browser = try testSocketPair();
    defer testCloseSocketPair(&roamium_browser);
    var surfari_tui = try testSocketPair();
    defer testCloseSocketPair(&surfari_tui);
    var roamium_tui = try testSocketPair();
    defer testCloseSocketPair(&roamium_tui);

    try testSetPendingServer(0, profile, surfari);
    try testSetPendingServer(1, profile, roamium);
    try testSetPane(0, "pane-surfari", profile, surfari, "https://example.test/surfari", surfari_tui[0]);
    try testSetPane(1, "pane-roamium", profile, roamium, "https://example.test/roamium", roamium_tui[0]);

    var roamium_register: c.Termsurf__ServerRegister = undefined;
    c.termsurf__server_register__init(&roamium_register);
    roamium_register.profile = @constCast(profile.ptr);
    roamium_register.browser = @constCast(roamium.ptr);
    handleServerRegister(roamium_browser[0], &roamium_register);

    try testing.expectEqual(@as(std.posix.fd_t, -1), servers[0].attached_fd);
    try testing.expectEqual(roamium_browser[0], servers[1].attached_fd);

    const roamium_create = try readTestMessage(roamium_browser[1], testing.allocator);
    defer c.termsurf__term_surf_message__free_unpacked(roamium_create, null);
    try testing.expectEqual(
        @as(c.Termsurf__TermSurfMessage__MsgCase, c.TERMSURF__TERM_SURF_MESSAGE__MSG_CREATE_TAB),
        roamium_create.*.msg_case,
    );
    try testing.expectEqualStrings("pane-roamium", cString(roamium_create.*.unnamed_0.create_tab.*.pane_id));

    var surfari_register: c.Termsurf__ServerRegister = undefined;
    c.termsurf__server_register__init(&surfari_register);
    surfari_register.profile = @constCast(profile.ptr);
    surfari_register.browser = @constCast(surfari.ptr);
    handleServerRegister(surfari_browser[0], &surfari_register);

    try testing.expectEqual(surfari_browser[0], servers[0].attached_fd);
    try testing.expectEqual(roamium_browser[0], servers[1].attached_fd);

    const surfari_create = try readTestMessage(surfari_browser[1], testing.allocator);
    defer c.termsurf__term_surf_message__free_unpacked(surfari_create, null);
    try testing.expectEqual(
        @as(c.Termsurf__TermSurfMessage__MsgCase, c.TERMSURF__TERM_SURF_MESSAGE__MSG_CREATE_TAB),
        surfari_create.*.msg_case,
    );
    try testing.expectEqualStrings("pane-surfari", cString(surfari_create.*.unnamed_0.create_tab.*.pane_id));

    var surfari_ready: c.Termsurf__TabReady = undefined;
    c.termsurf__tab_ready__init(&surfari_ready);
    surfari_ready.tab_id = 42;
    surfari_ready.pane_id = @constCast("pane-surfari");
    handleTabReady(&surfari_ready);

    const ready = try readTestMessage(surfari_tui[1], testing.allocator);
    defer c.termsurf__term_surf_message__free_unpacked(ready, null);
    try testing.expectEqual(
        @as(c.Termsurf__TermSurfMessage__MsgCase, c.TERMSURF__TERM_SURF_MESSAGE__MSG_BROWSER_READY),
        ready.*.msg_case,
    );
    try testing.expectEqualStrings("surfari", cString(ready.*.unnamed_0.browser_ready.*.browser));
    try testing.expectEqualStrings(servers[0].listenSocket(), cString(ready.*.unnamed_0.browser_ready.*.browser_socket));

    var surfari_context: c.Termsurf__CaContext = undefined;
    c.termsurf__ca_context__init(&surfari_context);
    surfari_context.tab_id = 42;
    surfari_context.ca_context_id = 1234;
    surfari_context.pixel_width = 800;
    surfari_context.pixel_height = 600;
    handleCaContext(surfari_browser[0], &surfari_context);

    try testing.expectEqual(@as(u64, 1234), panes[0].ca_context_id);
    try testing.expectEqual(@as(u64, 0), panes[1].ca_context_id);
}

fn testBrowserResolutionWithRestoredEnv() !void {
    try testWithRestoredEnv(roamium_path_env, testBrowserResolutionRestoreSurfari);
}

fn testBrowserResolutionRestoreSurfari() !void {
    try testWithRestoredEnv(surfari_path_env, testBrowserResolutionRestoreInstalledRoamium);
}

fn testBrowserResolutionRestoreInstalledRoamium() !void {
    try testWithRestoredEnv(installed_roamium_path_env, testBrowserResolutionRestoreInstalledSurfari);
}

fn testBrowserResolutionRestoreInstalledSurfari() !void {
    try testWithRestoredEnv(installed_surfari_path_env, testBrowserResolution);
}

fn testBrowserResolution() !void {
    const testing = std.testing;
    const surfari: [:0]const u8 = "surfari";
    const roamium: [:0]const u8 = "roamium";
    const fake_surfari_path: [:0]const u8 = "/tmp/termsurf-test-surfari";
    const fake_roamium_path: [:0]const u8 = "/tmp/termsurf-test-roamium";
    const installed_surfari_override: [:0]const u8 = "/tmp/termsurf-installed-surfari";
    const installed_roamium_override: [:0]const u8 = "/tmp/termsurf-installed-roamium";
    const invalid_override: [:0]const u8 = "relative-browser";

    _ = c.unsetenv(roamium_path_env);
    _ = c.unsetenv(surfari_path_env);
    _ = c.unsetenv(installed_roamium_path_env);
    _ = c.unsetenv(installed_surfari_path_env);

    try testing.expect(resolveBrowserExecutableForBuild(surfari, true) == null);
    try testing.expect(resolveBrowserExecutableForBuild(roamium, true) == null);
    try testing.expect(resolveBrowserExecutableForBuild("unknown-browser", true) == null);
    try testing.expectEqualStrings("/tmp/absolute-browser", resolveBrowserExecutableForBuild("/tmp/absolute-browser", true).?);

    try testing.expectEqualStrings(installed_surfari_path, resolveBrowserExecutableForBuild(surfari, false).?);
    try testing.expectEqualStrings(installed_roamium_path, resolveBrowserExecutableForBuild(roamium, false).?);

    if (c.setenv(surfari_path_env, fake_surfari_path, 1) != 0) return error.SetenvFailed;
    if (c.setenv(roamium_path_env, fake_roamium_path, 1) != 0) return error.SetenvFailed;
    try testing.expectEqualStrings(fake_surfari_path, resolveBrowserExecutableForBuild(surfari, true).?);
    try testing.expectEqualStrings(fake_surfari_path, resolveBrowserExecutableForBuild(surfari, false).?);
    try testing.expectEqualStrings(fake_roamium_path, resolveBrowserExecutableForBuild(roamium, true).?);
    try testing.expectEqualStrings(fake_roamium_path, resolveBrowserExecutableForBuild(roamium, false).?);

    if (c.setenv(surfari_path_env, invalid_override, 1) != 0) return error.SetenvFailed;
    if (c.setenv(roamium_path_env, invalid_override, 1) != 0) return error.SetenvFailed;
    try testing.expect(resolveBrowserExecutableForBuild(surfari, true) == null);
    try testing.expect(resolveBrowserExecutableForBuild(roamium, true) == null);
    try testing.expectEqualStrings(installed_surfari_path, resolveBrowserExecutableForBuild(surfari, false).?);
    try testing.expectEqualStrings(installed_roamium_path, resolveBrowserExecutableForBuild(roamium, false).?);

    _ = c.unsetenv(surfari_path_env);
    _ = c.unsetenv(roamium_path_env);
    if (c.setenv(installed_surfari_path_env, installed_surfari_override, 1) != 0) return error.SetenvFailed;
    if (c.setenv(installed_roamium_path_env, installed_roamium_override, 1) != 0) return error.SetenvFailed;
    try testing.expect(resolveBrowserExecutableForBuild(surfari, true) == null);
    try testing.expect(resolveBrowserExecutableForBuild(roamium, true) == null);
    try testing.expectEqualStrings(installed_surfari_override, resolveBrowserExecutableForBuild(surfari, false).?);
    try testing.expectEqualStrings(installed_roamium_override, resolveBrowserExecutableForBuild(roamium, false).?);
}

test "Surfari runtime framework path follows executable directory" {
    const testing = std.testing;
    try testing.expect(shouldSupplySurfariRuntimeFrameworkPath("surfari", null));
    try testing.expect(!shouldSupplySurfariRuntimeFrameworkPath("surfari", "/tmp/WebKitBuild/Debug"));
    try testing.expect(!shouldSupplySurfariRuntimeFrameworkPath("roamium", null));
    try testing.expectEqualStrings(
        "/opt/homebrew/opt/termsurf-surfari",
        surfariRuntimeFrameworkPath("surfari", "/opt/homebrew/opt/termsurf-surfari/surfari").?,
    );
    try testing.expect(surfariRuntimeFrameworkPath("roamium", "/opt/homebrew/opt/termsurf-roamium/roamium") == null);
}

fn testWithRestoredEnv(name: [:0]const u8, callback: *const fn () anyerror!void) !void {
    var previous_buf: [std.fs.max_path_bytes]u8 = undefined;
    var previous_len: usize = 0;
    const had_previous = if (std.posix.getenv(name)) |value|
        copyText(&previous_buf, &previous_len, value)
    else
        false;

    defer {
        if (had_previous) {
            const previous_z = previous_buf[0..previous_len :0];
            _ = c.setenv(name, previous_z, 1);
        } else {
            _ = c.unsetenv(name);
        }
    }

    try callback();
}

fn resetTermsurfStateForTest() void {
    clients = [_]ClientSlot{.{}} ** max_clients;
    panes = [_]PaneState{.{}} ** max_panes;
    closed_panes = [_]ClosedPaneState{.{}} ** max_closed_panes;
    servers = [_]ServerState{.{}} ** max_servers;
    apps = [_]AppState{.{}} ** max_apps;
    app_frontends = [_]AppFrontendState{.{}} ** max_app_frontends;
    tab_lookups = [_]TabLookupState{.{}} ** max_tab_lookups;
    devtools_reservations = [_]DevtoolsReservationState{.{}} ** max_devtools_reservations;
    last_browser_pane_len = 0;
    hello_homepage_len = 0;
    hello_browser_count = 0;
    gui_active = false;
    pending_gui_activation = false;
    current_color_scheme_dark = false;
}

test "termsurf create tab uses tracked color scheme" {
    const testing = std.testing;

    for ([_][]const u8{ "roamium", "surfari" }) |browser_name| {
        for ([_]bool{ false, true }) |dark| {
            resetTermsurfStateForTest();
            defer resetTermsurfStateForTest();

            var browser = try testSocketPair();
            defer testCloseSocketPair(&browser);
            var tui = try testSocketPair();
            defer testCloseSocketPair(&tui);

            colorSchemeChanged(dark);
            try testSetPane(0, "pane-color", "default", browser_name, "https://example.test/color", tui[0]);
            try sendCreateTab(browser[0], &panes[0]);

            const create = try readTestMessage(browser[1], testing.allocator);
            defer c.termsurf__term_surf_message__free_unpacked(create, null);
            try testing.expectEqual(
                @as(c.Termsurf__TermSurfMessage__MsgCase, c.TERMSURF__TERM_SURF_MESSAGE__MSG_CREATE_TAB),
                create.*.msg_case,
            );
            try testing.expectEqual(@as(c_int, if (dark) 1 else 0), create.*.unnamed_0.create_tab.*.dark);
        }
    }
}

test "termsurf create devtools tab snapshots tracked color scheme" {
    const testing = std.testing;

    for ([_]bool{ false, true }) |dark| {
        resetTermsurfStateForTest();
        defer resetTermsurfStateForTest();

        var browser = try testSocketPair();
        defer testCloseSocketPair(&browser);
        var tui = try testSocketPair();
        defer testCloseSocketPair(&tui);

        colorSchemeChanged(dark);
        try testSetPane(0, "pane-devtools", "default", "roamium", "", tui[0]);
        panes[0].inspected_tab_id = 42;
        const snapshot = snapshotCreateDevtoolsTab(&panes[0], browser[0]).?;
        colorSchemeChanged(!dark);
        try sendCreateDevtoolsTab(&snapshot);

        const create = try readTestMessage(browser[1], testing.allocator);
        defer c.termsurf__term_surf_message__free_unpacked(create, null);
        try testing.expectEqual(
            @as(c.Termsurf__TermSurfMessage__MsgCase, c.TERMSURF__TERM_SURF_MESSAGE__MSG_CREATE_DEVTOOLS_TAB),
            create.*.msg_case,
        );
        try testing.expectEqual(@as(c_int, if (dark) 1 else 0), create.*.unnamed_0.create_devtools_tab.*.dark);
    }
}

test "termsurf open app messages are tui connections" {
    const testing = std.testing;

    try testing.expectEqual(ConnType.tui, classifyConnection(c.TERMSURF__TERM_SURF_MESSAGE__MSG_OPEN_APP));
}

test "termsurf open app rejects unknown app id" {
    const testing = std.testing;

    resetTermsurfStateForTest();
    defer resetTermsurfStateForTest();

    var tui = try testSocketPair();
    defer testCloseSocketPair(&tui);

    var open_app: c.Termsurf__OpenApp = undefined;
    c.termsurf__open_app__init(&open_app);
    open_app.pane_id = @constCast("pane-app");
    open_app.app_id = @constCast("unknown");
    open_app.browser = @constCast("roamium");
    open_app.profile = @constCast("default");
    handleOpenApp(tui[0], &open_app);

    const reply = try readTestMessage(tui[1], testing.allocator);
    defer c.termsurf__term_surf_message__free_unpacked(reply, null);
    try testing.expectEqual(
        @as(c.Termsurf__TermSurfMessage__MsgCase, c.TERMSURF__TERM_SURF_MESSAGE__MSG_OPEN_APP_REPLY),
        reply.*.msg_case,
    );
    try testing.expectEqualStrings("pane-app", cString(reply.*.unnamed_0.open_app_reply.*.pane_id));
    try testing.expectEqualStrings("unknown", cString(reply.*.unnamed_0.open_app_reply.*.app_id));
    try testing.expectEqualStrings("unsupported app_id", cString(@field(reply.*.unnamed_0.open_app_reply.*, "error")));
}

test "termsurf app descriptors select trusted runtimes" {
    const testing = std.testing;

    const gtui = findAppDescriptor(gtui_app_id).?;
    try testing.expectEqual(AppRuntime.deno, gtui.runtime);
    try testing.expectEqualStrings(gtui_app_path_env, gtui.app_path_env);
    try testing.expectEqualStrings(deno_path_env, gtui.runtime_path_env);

    try testing.expect(findAppDescriptor("unknown") == null);
}

test "termsurf open app reuses existing backend for another frontend" {
    const testing = std.testing;

    resetTermsurfStateForTest();
    defer resetTermsurfStateForTest();

    const argv = [_][]const u8{ "/bin/sleep", "30" };
    var child = std.process.Child.init(&argv, testing.allocator);
    child.stdin_behavior = .Ignore;
    child.stdout_behavior = .Ignore;
    child.stderr_behavior = .Ignore;
    try child.spawn();
    defer {
        std.posix.kill(child.id, std.posix.SIG.TERM) catch {};
        _ = std.posix.waitpid(child.id, 0);
    }

    try testSetApp(0, gtui_app_id, "http://127.0.0.1:50000/", 50000, child.id);

    var frontend_id_buf: [max_app_frontend_id_len]u8 = undefined;
    var frontend_id_len: usize = 0;
    var url_buf: [max_app_url_len]u8 = undefined;
    var url_len: usize = 0;
    var error_buf: [256]u8 = undefined;

    try testing.expect(openOrReuseApp(
        findAppDescriptor(gtui_app_id).?,
        10,
        "pane-b",
        gtui_app_id,
        "roamium",
        "default",
        "",
        &frontend_id_buf,
        &frontend_id_len,
        &url_buf,
        &url_len,
        &error_buf,
    ));
    try testing.expectEqual(@as(usize, 1), frontendCountForApp(gtui_app_id));
    try testing.expectEqual(child.id, apps[0].child_pid);
    try testing.expectEqualStrings("http://127.0.0.1:50000/", url_buf[0..url_len]);
    try testing.expect(frontend_id_len > 0);
}

test "termsurf app frontend cleanup is idempotent and stops after last frontend" {
    const testing = std.testing;

    resetTermsurfStateForTest();
    defer resetTermsurfStateForTest();

    try testSetApp(0, gtui_app_id, "http://127.0.0.1:50000/", 50000, 0);
    try testSetAppFrontend(0, gtui_app_id, "gtui-a", "pane-a", 10);
    try testSetAppFrontend(1, gtui_app_id, "gtui-b", "pane-b", 11);

    var shutdown: ?AppShutdownSnapshot = null;
    removeAppFrontendLocked(gtui_app_id, "gtui-a", null, &shutdown);
    try testing.expect(shutdown == null);
    try testing.expect(apps[0].in_use);
    try testing.expect(!app_frontends[0].in_use);
    try testing.expect(app_frontends[1].in_use);

    shutdown = null;
    removeAppFrontendLocked(gtui_app_id, "gtui-a", null, &shutdown);
    try testing.expect(shutdown == null);
    try testing.expect(apps[0].in_use);
    try testing.expect(app_frontends[1].in_use);

    shutdown = null;
    removeAppFrontendLocked(gtui_app_id, "gtui-b", null, &shutdown);
    try testing.expect(shutdown == null);
    try testing.expect(!apps[0].in_use);
    try testing.expect(!app_frontends[1].in_use);
}

test "termsurf app cleanup by fd keeps shared backend until final frontend" {
    const testing = std.testing;

    resetTermsurfStateForTest();
    defer resetTermsurfStateForTest();

    try testSetApp(0, gtui_app_id, "http://127.0.0.1:50000/", 50000, 0);
    try testSetAppFrontend(0, gtui_app_id, "gtui-a", "pane-a", 10);
    try testSetAppFrontend(1, gtui_app_id, "gtui-b", "pane-b", 11);

    cleanupAppFrontendsForFd(10);
    try testing.expect(apps[0].in_use);
    try testing.expect(!app_frontends[0].in_use);
    try testing.expect(app_frontends[1].in_use);

    cleanupAppFrontendsForFd(10);
    try testing.expect(apps[0].in_use);
    try testing.expect(app_frontends[1].in_use);

    cleanupAppFrontendsForFd(11);
    try testing.expect(!apps[0].in_use);
    try testing.expect(!app_frontends[1].in_use);
}

test "termsurf app cleanup by pane removes the pane frontend only" {
    const testing = std.testing;

    resetTermsurfStateForTest();
    defer resetTermsurfStateForTest();

    try testSetApp(0, gtui_app_id, "http://127.0.0.1:50000/", 50000, 0);
    try testSetAppFrontend(0, gtui_app_id, "gtui-a", "pane-a", 10);
    try testSetAppFrontend(1, gtui_app_id, "gtui-b", "pane-b", 11);

    cleanupAppFrontendsForPane("pane-a");
    try testing.expect(apps[0].in_use);
    try testing.expect(!app_frontends[0].in_use);
    try testing.expect(app_frontends[1].in_use);

    cleanupAppFrontendsForPane("pane-b");
    try testing.expect(!apps[0].in_use);
    try testing.expect(!app_frontends[1].in_use);
}

test "termsurf app ctrl-c closes app frontend instead of forwarding to browser" {
    const testing = std.testing;

    resetTermsurfStateForTest();
    defer resetTermsurfStateForTest();

    var tui = try testSocketPair();
    defer testCloseSocketPair(&tui);

    try testSetApp(0, gtui_app_id, "http://127.0.0.1:50000/", 50000, 0);
    try testSetAppFrontend(0, gtui_app_id, "gtui-a", "pane-a", tui[0]);

    try testing.expect(closeAppFrontendForPaneShortcut("pane-a"));
    try testing.expect(!apps[0].in_use);
    try testing.expect(!app_frontends[0].in_use);

    const close = try readTestMessage(tui[1], testing.allocator);
    defer c.termsurf__term_surf_message__free_unpacked(close, null);
    try testing.expectEqual(
        @as(c.Termsurf__TermSurfMessage__MsgCase, c.TERMSURF__TERM_SURF_MESSAGE__MSG_CLOSE_APP_FRONTEND),
        close.*.msg_case,
    );
    try testing.expectEqualStrings("pane-a", cString(close.*.unnamed_0.close_app_frontend.*.pane_id));
    try testing.expectEqualStrings(gtui_app_id, cString(close.*.unnamed_0.close_app_frontend.*.app_id));
    try testing.expectEqualStrings("gtui-a", cString(close.*.unnamed_0.close_app_frontend.*.frontend_id));
}

test "termsurf app backend early exit returns failure without retained state" {
    resetTermsurfStateForTest();
    defer resetTermsurfStateForTest();
    try testWithRestoredEnv(deno_path_env, testOpenAppEarlyExitWithRestoredTsgtuiPath);
}

fn testOpenAppEarlyExitWithRestoredTsgtuiPath() !void {
    try testWithRestoredEnv(gtui_app_path_env, testOpenAppEarlyExitTsgtui);
}

fn testOpenAppEarlyExitTsgtui() !void {
    const testing = std.testing;

    if (c.setenv(deno_path_env, "/usr/bin/false", 1) != 0) return error.SetenvFailed;
    if (c.setenv(gtui_app_path_env, "/tmp/termsurf-gtui-test-server.ts", 1) != 0) return error.SetenvFailed;
    const descriptor = findAppDescriptor(gtui_app_id).?;

    var frontend_id_buf: [max_app_frontend_id_len]u8 = undefined;
    var frontend_id_len: usize = 0;
    var url_buf: [max_app_url_len]u8 = undefined;
    var url_len: usize = 0;
    var error_buf: [256]u8 = undefined;

    try testing.expect(!openOrReuseApp(
        descriptor,
        10,
        "pane-app",
        gtui_app_id,
        "roamium",
        "default",
        "/tmp/termsurf-gtui-test-server.ts",
        &frontend_id_buf,
        &frontend_id_len,
        &url_buf,
        &url_len,
        &error_buf,
    ));
    try testing.expectEqual(@as(usize, 0), frontend_id_len);
    try testing.expectEqual(@as(usize, 0), url_len);
    try testing.expect(findApp(gtui_app_id) == null);
    try testing.expectEqual(@as(usize, 0), frontendCountForApp(gtui_app_id));
    try testing.expect(std.mem.indexOf(u8, std.mem.sliceTo(error_buf[0..], 0), "ready") != null);
}

fn testSocketPair() ![2]std.posix.fd_t {
    var fds: [2]c_int = undefined;
    if (c.socketpair(c.AF_UNIX, c.SOCK_STREAM, 0, &fds) != 0) return error.SocketPairFailed;
    return .{ fds[0], fds[1] };
}

fn testCloseSocketPair(fds: *[2]std.posix.fd_t) void {
    if (fds[0] >= 0) {
        std.posix.close(fds[0]);
        fds[0] = -1;
    }
    if (fds[1] >= 0) {
        std.posix.close(fds[1]);
        fds[1] = -1;
    }
}

fn testSetPendingServer(index: usize, profile: []const u8, browser: []const u8) !void {
    servers[index] = .{};
    try testingCopyText(&servers[index].profile, &servers[index].profile_len, profile);
    try testingCopyText(&servers[index].browser, &servers[index].browser_len, browser);
    servers[index].in_use = true;
    servers[index].pane_count = 1;
    servers[index].attached_fd = -1;
    if (!buildListenSocket(&servers[index])) return error.ListenSocketTooLong;
}

fn testSetPane(
    index: usize,
    pane_id: []const u8,
    profile: []const u8,
    browser: []const u8,
    url: []const u8,
    tui_fd: std.posix.fd_t,
) !void {
    panes[index] = .{};
    try testingCopyText(&panes[index].pane_id, &panes[index].pane_id_len, pane_id);
    try testingCopyText(&panes[index].profile, &panes[index].profile_len, profile);
    try testingCopyText(&panes[index].browser, &panes[index].browser_len, browser);
    try testingCopyText(&panes[index].url, &panes[index].url_len, url);
    panes[index].in_use = true;
    panes[index].width = 80;
    panes[index].height = 24;
    panes[index].browsing = true;
    panes[index].tui_fd = tui_fd;
}

fn testSetApp(index: usize, app_id: []const u8, url: []const u8, port: u16, child_pid: std.process.Child.Id) !void {
    apps[index] = .{};
    if (!setApp(&apps[index], app_id, url, port, child_pid)) return error.TestValueTooLong;
}

fn testSetAppFrontend(index: usize, app_id: []const u8, frontend_id: []const u8, pane_id: []const u8, tui_fd: std.posix.fd_t) !void {
    app_frontends[index] = .{};
    if (!setAppFrontend(&app_frontends[index], app_id, frontend_id, pane_id, tui_fd)) return error.TestValueTooLong;
}

fn testingCopyText(buf: []u8, len: *usize, value: []const u8) !void {
    if (!copyText(buf, len, value)) return error.TestValueTooLong;
}

fn readTestMessage(fd: std.posix.fd_t, allocator: std.mem.Allocator) !*c.Termsurf__TermSurfMessage {
    var len_buf: [4]u8 = undefined;
    try readExactTest(fd, &len_buf);
    const len = std.mem.readInt(u32, &len_buf, .little);
    if (len > max_frame_size) return error.FrameTooLarge;

    const payload = try allocator.alloc(u8, len);
    defer allocator.free(payload);
    try readExactTest(fd, payload);

    return c.termsurf__term_surf_message__unpack(null, len, payload.ptr) orelse error.ProtobufUnpackFailed;
}

fn readExactTest(fd: std.posix.fd_t, buf: []u8) !void {
    var offset: usize = 0;
    while (offset < buf.len) {
        const n = try std.posix.read(fd, buf[offset..]);
        if (n == 0) return error.EndOfStream;
        offset += n;
    }
}
