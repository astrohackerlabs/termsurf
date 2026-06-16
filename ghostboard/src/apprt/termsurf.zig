const std = @import("std");
const internal_os = @import("../os/main.zig");

const c = @cImport({
    @cInclude("unistd.h");
    @cInclude("termsurf.pb-c.h");
});

const log = std.log.scoped(.termsurf);

const env_key: [:0]const u8 = "TERMSURF_SOCKET";
const max_frame_size: usize = 1024 * 1024;
const max_clients: usize = 128;
const max_panes: usize = 256;
const max_servers: usize = 64;
const max_tab_lookups: usize = 512;
const max_pane_id_len: usize = 128;
const max_profile_len: usize = 128;
const max_browser_len: usize = std.fs.max_path_bytes;
const max_url_len: usize = 2048;
const max_listen_socket_len: usize = std.fs.max_path_bytes;
const default_browser = "roamium";
const fallback_cell_width: u64 = 10;
const fallback_cell_height: u64 = 20;

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
    tab_id: i64 = 0,
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
var servers: [max_servers]ServerState = [_]ServerState{.{}} ** max_servers;
var tab_lookups: [max_tab_lookups]TabLookupState = [_]TabLookupState{.{}} ** max_tab_lookups;
var last_browser_pane: [max_pane_id_len]u8 = undefined;
var last_browser_pane_len: usize = 0;

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
        if (conn_type == .tui) clearPaneTuiFd(fd);
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
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_SERVER_REGISTER => {
                    handleServerRegister(fd, msg.*.unnamed_0.server_register);
                },
                c.TERMSURF__TERM_SURF_MESSAGE__MSG_TAB_READY => {
                    handleTabReady(msg.*.unnamed_0.tab_ready);
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

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_HELLO_REPLY;
    wrapper.unnamed_0.hello_reply = &reply;

    try sendProtobuf(fd, &wrapper);
    log.info("TermSurf HelloReply sent", .{});
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
        if (fillQueryDevtoolsSuccess(&reply, profile, browser, query.*.inspected_tab_id, &profile_buf, &profile_len, &browser_buf, &browser_len)) {
            break :blk "";
        }
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
    log.info("TermSurf QueryDevtoolsReply sent", .{});
}

fn fillQueryDevtoolsSuccess(
    reply: *c.Termsurf__QueryDevtoolsReply,
    profile: []const u8,
    browser: []const u8,
    inspected_tab_id: i64,
    profile_buf: []u8,
    profile_len: *usize,
    browser_buf: []u8,
    browser_len: *usize,
) bool {
    state_mutex.lock();
    defer state_mutex.unlock();

    if (findTabLookup(profile, browser, inspected_tab_id) == null) return false;
    if (!copyText(profile_buf, profile_len, profile)) return false;
    if (!copyText(browser_buf, browser_len, browser)) return false;

    reply.tab_id = inspected_tab_id;
    return true;
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
    log.info("ServerRegister: profile={s}", .{profile});

    state_mutex.lock();
    defer state_mutex.unlock();

    if (findAttachableServerByProfile(profile)) |index| {
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

    log.warn("ServerRegister: no matching server for profile={s}", .{profile});
}

fn serverRegisterProfile(req: ?*c.Termsurf__ServerRegister) []const u8 {
    if (req) |server| {
        if (server.*.profile) |profile| return std.mem.span(profile);
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
    var spawn_browser_buf: [max_browser_len]u8 = undefined;
    var spawn_browser_len: usize = 0;
    var spawn_listen_socket_buf: [max_listen_socket_len]u8 = undefined;
    var spawn_listen_socket_len: usize = 0;
    var resize_snapshot: ?ResizeSnapshot = null;

    state_mutex.lock();

    if (findPane(pane_id)) |pane_index| {
        if (!updatePane(&panes[pane_index], overlay, pane_id, profile, browser, url)) {
            state_mutex.unlock();
            return;
        }
        panes[pane_index].tui_fd = tui_fd;
        const server_index = findServer(profile, browser);
        const pane_count = if (server_index) |index| servers[index].pane_count else 0;
        resize_snapshot = snapshotResize(&panes[pane_index]);
        log.info(
            "SetOverlay: updated pane_id={s} profile={s} browser={s} pane_count={}",
            .{ pane_id, profile, browser, pane_count },
        );
        state_mutex.unlock();
        if (resize_snapshot) |snapshot| {
            sendResize(&snapshot) catch |err| {
                log.warn("Resize send failed pane_id={s} err={}", .{ snapshot.paneId(), err });
            };
        }
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

    if (findServer(profile, browser)) |server_index| {
        servers[server_index].pane_count += 1;
        log.info(
            "SetOverlay: reused pending server key={s}/{s} pane_count={} has_fd={}",
            .{ servers[server_index].profileName(), servers[server_index].browserName(), servers[server_index].pane_count, servers[server_index].attached_fd >= 0 },
        );
    } else {
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
            if (isAbsolutePath(browser)) {
                should_spawn =
                    copyText(&spawn_profile_buf, &spawn_profile_len, servers[server_index].profileName()) and
                    copyText(&spawn_browser_buf, &spawn_browser_len, servers[server_index].browserName()) and
                    copyText(&spawn_listen_socket_buf, &spawn_listen_socket_len, servers[server_index].listenSocket());
            } else {
                log.warn("SetOverlay: named browser launch not implemented browser={s}", .{browser});
            }
        } else {
            log.warn("SetOverlay: listen socket path too long profile={s} browser={s}", .{ profile, browser });
        }
        log.info(
            "SetOverlay: created pending server key={s}/{s} pane_count={} listen_socket={s}",
            .{ servers[server_index].profileName(), servers[server_index].browserName(), servers[server_index].pane_count, servers[server_index].listenSocket() },
        );
    }
    state_mutex.unlock();

    if (should_spawn) {
        const profile_z = spawn_profile_buf[0..spawn_profile_len :0];
        const browser_z = spawn_browser_buf[0..spawn_browser_len :0];
        const listen_socket_z = spawn_listen_socket_buf[0..spawn_listen_socket_len :0];
        if (spawnBrowserProcess(profile_z, browser_z, listen_socket_z)) |pid| {
            recordServerChild(profile_z, browser_z, pid);
        }
    }
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
    return true;
}

fn sendPendingCreateTabs(fd: std.posix.fd_t, server: *const ServerState) !void {
    for (&panes) |*pane| {
        if (!pane.in_use or pane.tab_id != 0) continue;
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
    create_tab.dark = 0;

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_CREATE_TAB;
    wrapper.unnamed_0.create_tab = &create_tab;

    try sendProtobuf(fd, &wrapper);
    log.info("sent CreateTab: pane_id={s} url={s}", .{ pane.paneId(), pane.url[0..pane.url_len] });
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
    log.info(
        "Resize: pane_id={s} tab_id={} pixel={}x{}",
        .{ snapshot.paneId(), snapshot.tab_id, snapshot.pixel_width, snapshot.pixel_height },
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
    _ = copyText(&last_browser_pane, &last_browser_pane_len, pane_id);
    browser_ready = snapshotBrowserReady(&panes[pane_index], tab_id);

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

fn clearPaneTuiFd(fd: std.posix.fd_t) void {
    state_mutex.lock();
    defer state_mutex.unlock();

    for (&panes) |*pane| {
        if (pane.in_use and pane.tui_fd == fd) {
            pane.tui_fd = -1;
        }
    }
}

fn spawnBrowserProcess(profile_z: [:0]const u8, browser_z: [:0]const u8, listen_socket_z: [:0]const u8) ?std.process.Child.Id {
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

    var user_data_dir_buf: [std.fs.max_path_bytes]u8 = undefined;
    const user_data_dir = buildUserDataDir(&user_data_dir_buf, profile_z) orelse return null;
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
        browser_z,
        ipc_arg,
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
    child.spawn() catch |err| {
        log.warn("browser spawn failed path={s} profile={s} err={}", .{ browser_z, profile_z, err });
        return null;
    };

    log.info(
        "spawned browser path={s} pid={} profile={s} listen_socket={s}",
        .{ browser_z, child.id, profile_z, listen_socket_z },
    );
    return child.id;
}

fn buildUserDataDir(buf: []u8, profile: []const u8) ?[:0]u8 {
    const home = std.posix.getenv("HOME") orelse {
        log.warn("browser spawn skipped: HOME is not set", .{});
        return null;
    };
    const data_home = std.posix.getenv("XDG_DATA_HOME");
    return if (data_home) |base|
        std.fmt.bufPrintZ(buf, "{s}/termsurf/chromium-profiles/{s}", .{ base, profile }) catch null
    else
        std.fmt.bufPrintZ(buf, "{s}/.local/share/termsurf/chromium-profiles/{s}", .{ home, profile }) catch null;
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

fn findAttachableServerByProfile(profile: []const u8) ?usize {
    for (&servers, 0..) |*server, i| {
        if (server.in_use and
            server.attached_fd < 0 and
            std.mem.eql(u8, server.profileName(), profile))
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
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_HELLO_REQUEST => "HelloRequest",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_HELLO_REPLY => "HelloReply",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_CREATE_TAB => "CreateTab",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_RESIZE => "Resize",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_TAB_READY => "TabReady",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_LAST_REQUEST => "QueryLastRequest",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_LAST_REPLY => "QueryLastReply",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_DEVTOOLS_REQUEST => "QueryDevtoolsRequest",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_DEVTOOLS_REPLY => "QueryDevtoolsReply",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_TABS_REQUEST => "QueryTabsRequest",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_QUERY_TABS_REPLY => "QueryTabsReply",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_SERVER_REGISTER => "ServerRegister",
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_SET_OVERLAY => "SetOverlay",
        else => "Other",
    };
}

fn classifyConnection(msg_case: c.Termsurf__TermSurfMessage__MsgCase) ConnType {
    return switch (msg_case) {
        c.TERMSURF__TERM_SURF_MESSAGE__MSG_SERVER_REGISTER => .browser,
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
