const std = @import("std");
const c = @cImport({
    @cInclude("hello.pb-c.h");
});
const print = std.debug.print;

pub fn main() !void {
    // Create and initialize a Hello message.
    var msg: c.Termsurf__Hello = undefined;
    c.termsurf__hello__init(&msg);
    msg.name = @constCast("TermSurf");
    msg.id = -42;
    msg.size = 1024;
    msg.x = 3.14;
    msg.active = 1;

    // Serialize.
    const packed_size = c.termsurf__hello__get_packed_size(&msg);
    const buf = try std.heap.page_allocator.alloc(u8, packed_size);
    defer std.heap.page_allocator.free(buf);
    const written = c.termsurf__hello__pack(&msg, buf.ptr);
    std.debug.assert(written == packed_size);

    // Deserialize.
    const decoded = c.termsurf__hello__unpack(null, written, buf.ptr) orelse {
        print("Zig: FAIL (unpack returned null)\n", .{});
        return;
    };
    defer c.termsurf__hello__free_unpacked(decoded, null);

    // Verify fields (dereference [*c] pointer).
    const d = decoded.*;
    const name = std.mem.span(d.name);
    std.debug.assert(std.mem.eql(u8, name, "TermSurf"));
    std.debug.assert(d.id == -42);
    std.debug.assert(d.size == 1024);
    std.debug.assert(d.x == 3.14);
    std.debug.assert(d.active == 1);

    print("Zig: pass ({d} bytes)\n", .{written});
}
