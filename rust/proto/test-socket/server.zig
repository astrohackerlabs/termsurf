const std = @import("std");
const c = @cImport({
    @cInclude("sys/socket.h");
    @cInclude("sys/un.h");
    @cInclude("unistd.h");
    @cInclude("termsurf.pb-c.h");
});
const print = std.debug.print;

const SOCKET_NAME = "termsurf-test.sock";

pub fn main() !void {
    const allocator = std.heap.page_allocator;

    // Build socket path from $TMPDIR.
    const tmpdir: []const u8 = std.posix.getenv("TMPDIR") orelse "/tmp/";
    var path_buf: [256]u8 = [_]u8{0} ** 256;
    var off: usize = 0;
    @memcpy(path_buf[0..tmpdir.len], tmpdir);
    off = tmpdir.len;
    if (tmpdir[tmpdir.len - 1] != '/') {
        path_buf[off] = '/';
        off += 1;
    }
    @memcpy(path_buf[off .. off + SOCKET_NAME.len], SOCKET_NAME);
    const path_len = off + SOCKET_NAME.len;

    // Null-terminated copy for C functions.
    const c_path: [*c]const u8 = @ptrCast(&path_buf);

    // Remove stale socket.
    _ = c.unlink(c_path);

    // Create socket.
    const sock = c.socket(c.AF_UNIX, c.SOCK_STREAM, 0);
    if (sock < 0) return error.SocketFailed;
    defer _ = c.close(sock);

    // Bind.
    var addr = std.mem.zeroes(c.struct_sockaddr_un);
    addr.sun_family = c.AF_UNIX;
    const sun_path_ptr: [*]u8 = @ptrCast(&addr.sun_path);
    @memcpy(sun_path_ptr[0..path_len], path_buf[0..path_len]);

    if (c.bind(sock, @ptrCast(&addr), @sizeOf(c.struct_sockaddr_un)) < 0)
        return error.BindFailed;
    if (c.listen(sock, 1) < 0)
        return error.ListenFailed;

    print("Zig server: listening\n", .{});

    // Accept one connection.
    const client = c.accept(sock, null, null);
    if (client < 0) return error.AcceptFailed;
    defer _ = c.close(client);

    // Read length prefix (4 bytes LE).
    var len_buf: [4]u8 = undefined;
    try readExact(client, &len_buf);
    const msg_len: usize = @intCast(std.mem.readInt(u32, &len_buf, .little));

    // Read protobuf message.
    const msg_buf = try allocator.alloc(u8, msg_len);
    defer allocator.free(msg_buf);
    try readExact(client, msg_buf);

    // Deserialize.
    const decoded = c.termsurf__term_surf_message__unpack(
        null,
        msg_len,
        msg_buf.ptr,
    ) orelse {
        print("Zig server: FAIL (unpack returned null)\n", .{});
        return;
    };
    defer c.termsurf__term_surf_message__free_unpacked(decoded, null);

    // Verify it's a HelloRequest with pane_id = "pane-1".
    const d = decoded.*;
    std.debug.assert(d.msg_case == c.TERMSURF__TERM_SURF_MESSAGE__MSG_HELLO_REQUEST);
    const req_ptr = d.unnamed_0.hello_request orelse {
        print("Zig server: FAIL (hello_request is null)\n", .{});
        return;
    };
    const req = req_ptr.*;
    std.debug.assert(std.mem.eql(u8, std.mem.span(req.pane_id), "pane-1"));

    // Build HelloReply.
    var reply: c.Termsurf__HelloReply = undefined;
    c.termsurf__hello_reply__init(&reply);
    reply.homepage = @constCast("https://termsurf.com");

    var wrapper: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_HELLO_REPLY;
    wrapper.unnamed_0.hello_reply = &reply;

    // Serialize.
    const reply_size = c.termsurf__term_surf_message__get_packed_size(&wrapper);
    const reply_buf = try allocator.alloc(u8, reply_size);
    defer allocator.free(reply_buf);
    const written = c.termsurf__term_surf_message__pack(&wrapper, reply_buf.ptr);
    std.debug.assert(written == reply_size);

    // Write length prefix + message.
    var reply_len_buf: [4]u8 = undefined;
    std.mem.writeInt(u32, &reply_len_buf, @intCast(written), .little);
    try writeAll(client, &reply_len_buf);
    try writeAll(client, reply_buf[0..written]);

    // Clean up socket file.
    _ = c.unlink(c_path);

    print("Zig server: pass\n", .{});
}

fn readExact(fd: c_int, buf: []u8) !void {
    var total: usize = 0;
    while (total < buf.len) {
        const n = c.read(fd, @ptrCast(buf[total..].ptr), buf.len - total);
        if (n <= 0) return error.ReadFailed;
        total += @intCast(@as(usize, @intCast(n)));
    }
}

fn writeAll(fd: c_int, buf: []const u8) !void {
    var total: usize = 0;
    while (total < buf.len) {
        const n = c.write(fd, @ptrCast(buf[total..].ptr), buf.len - total);
        if (n <= 0) return error.WriteFailed;
        total += @intCast(@as(usize, @intCast(n)));
    }
}
