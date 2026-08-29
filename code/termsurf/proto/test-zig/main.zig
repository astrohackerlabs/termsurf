const std = @import("std");
const c = @cImport({
    @cInclude("termsurf.pb-c.h");
});
const print = std.debug.print;

pub fn main() !void {
    // Create and initialize a CreateTab message.
    var tab: c.Termsurf__CreateTab = undefined;
    c.termsurf__create_tab__init(&tab);
    tab.url = @constCast("https://termsurf.com");
    tab.pane_id = @constCast("pane-1");
    tab.pixel_width = 1920;
    tab.pixel_height = 1080;
    tab.dark = 1;

    // Create and initialize a TermSurfMessage wrapping the CreateTab.
    var msg: c.Termsurf__TermSurfMessage = undefined;
    c.termsurf__term_surf_message__init(&msg);
    msg.msg_case = c.TERMSURF__TERM_SURF_MESSAGE__MSG_CREATE_TAB;
    msg.unnamed_0.create_tab = &tab;

    // Serialize.
    const packed_size = c.termsurf__term_surf_message__get_packed_size(&msg);
    const buf = try std.heap.page_allocator.alloc(u8, packed_size);
    defer std.heap.page_allocator.free(buf);
    const written = c.termsurf__term_surf_message__pack(&msg, buf.ptr);
    std.debug.assert(written == packed_size);

    // Deserialize.
    const decoded = c.termsurf__term_surf_message__unpack(null, written, buf.ptr) orelse {
        print("Zig: FAIL (unpack returned null)\n", .{});
        return;
    };
    defer c.termsurf__term_surf_message__free_unpacked(decoded, null);

    // Verify the oneof round-trips correctly.
    const d = decoded.*;
    std.debug.assert(d.msg_case == c.TERMSURF__TERM_SURF_MESSAGE__MSG_CREATE_TAB);

    const ct_ptr = d.unnamed_0.create_tab orelse {
        print("Zig: FAIL (create_tab is null)\n", .{});
        return;
    };
    const ct = ct_ptr.*;
    std.debug.assert(std.mem.eql(u8, std.mem.span(ct.url), "https://termsurf.com"));
    std.debug.assert(std.mem.eql(u8, std.mem.span(ct.pane_id), "pane-1"));
    std.debug.assert(ct.pixel_width == 1920);
    std.debug.assert(ct.pixel_height == 1080);
    std.debug.assert(ct.dark == 1);

    print("Zig: pass ({d} bytes)\n", .{written});
}
