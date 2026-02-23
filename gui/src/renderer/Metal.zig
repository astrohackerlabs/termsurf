//! Graphics API wrapper for Metal.
pub const Metal = @This();

const std = @import("std");
const assert = @import("../quirks.zig").inlineAssert;
const Allocator = std.mem.Allocator;
const builtin = @import("builtin");
const objc = @import("objc");
const macos = @import("macos");
const graphics = macos.graphics;
const apprt = @import("../apprt.zig");
const font = @import("../font/main.zig");
const configpkg = @import("../config.zig");
const rendererpkg = @import("../renderer.zig");
const Renderer = rendererpkg.GenericRenderer(Metal);
const shadertoy = @import("shadertoy.zig");

const mtl = @import("metal/api.zig");
const IOSurfaceLayer = @import("metal/IOSurfaceLayer.zig");

pub const GraphicsAPI = Metal;
pub const Target = @import("metal/Target.zig");
pub const Frame = @import("metal/Frame.zig");
pub const RenderPass = @import("metal/RenderPass.zig");
pub const Pipeline = @import("metal/Pipeline.zig");
const bufferpkg = @import("metal/buffer.zig");
pub const Buffer = bufferpkg.Buffer;
pub const Sampler = @import("metal/Sampler.zig");
pub const Texture = @import("metal/Texture.zig");
pub const shaders = @import("metal/shaders.zig");

pub const custom_shader_target: shadertoy.Target = .msl;
// The fragCoord for Metal shaders is +Y = down.
pub const custom_shader_y_is_down = true;

/// Triple buffering.
pub const swap_chain_count = 3;

const log = std.log.scoped(.metal);

layer: IOSurfaceLayer,

/// MTLDevice
device: objc.Object,
/// MTLCommandQueue
queue: objc.Object,

/// Alpha blending mode
blending: configpkg.Config.AlphaBlending,

/// The default storage mode to use for resources created with our device.
///
/// This is based on whether the device is a discrete GPU or not, since
/// discrete GPUs do not have unified memory and therefore do not support
/// the "shared" storage mode, instead we have to use the "managed" mode.
default_storage_mode: mtl.MTLResourceOptions.StorageMode,

/// The maximum 2D texture width and height supported by the device.
max_texture_size: u32,

/// We start an AutoreleasePool before `drawFrame` and end it afterwards.
autorelease_pool: ?*objc.AutoreleasePool = null,

pub fn init(alloc: Allocator, opts: rendererpkg.Options) !Metal {
    comptime switch (builtin.os.tag) {
        .macos, .ios => {},
        else => @compileError("unsupported platform for Metal"),
    };

    _ = alloc;

    // Choose our MTLDevice and create a MTLCommandQueue for that device.
    const device = try chooseDevice();
    errdefer device.release();
    const queue = device.msgSend(objc.Object, objc.sel("newCommandQueue"), .{});
    errdefer queue.release();

    // Grab metadata about the device.
    const default_storage_mode: mtl.MTLResourceOptions.StorageMode = switch (comptime builtin.os.tag) {
        // manage mode is not supported by iOS
        .ios => .shared,
        else => if (device.getProperty(bool, "hasUnifiedMemory")) .shared else .managed,
    };
    const max_texture_size = queryMaxTextureSize(device);
    log.debug(
        "device properties default_storage_mode={} max_texture_size={}",
        .{ default_storage_mode, max_texture_size },
    );

    const ViewInfo = struct {
        view: objc.Object,
        scaleFactor: f64,
    };

    // Get the metadata about our underlying view that we'll be rendering to.
    const info: ViewInfo = switch (apprt.runtime) {
        apprt.embedded => .{
            .scaleFactor = @floatCast(opts.rt_surface.content_scale.x),
            .view = switch (opts.rt_surface.platform) {
                .macos => |v| v.nsview,
                .ios => |v| v.uiview,
            },
        },

        else => @compileError("unsupported apprt for metal"),
    };

    // Create an IOSurfaceLayer which we can assign to the view to make
    // it in to a "layer-hosting view", so that we can manually control
    // the layer contents.
    var layer = try IOSurfaceLayer.init();
    errdefer layer.release();

    // Add our layer to the view.
    //
    // On macOS we do this by making the view "layer-hosting"
    // by assigning it to the view's `layer` property BEFORE
    // setting `wantsLayer` to `true`.
    //
    // On iOS, views are always layer-backed, and `layer`
    // is readonly, so instead we add it as a sublayer.
    switch (comptime builtin.os.tag) {
        .macos => {
            info.view.setProperty("layer", layer.layer.value);
            info.view.setProperty("wantsLayer", true);
        },

        .ios => {
            const view_layer = objc.Object.fromId(info.view.getProperty(?*anyopaque, "layer"));
            view_layer.msgSend(void, objc.sel("addSublayer:"), .{layer.layer.value});
        },

        else => @compileError("unsupported target for Metal"),
    }

    // Ensure that if our layer is oversized it
    // does not overflow the bounds of the view.
    info.view.setProperty("clipsToBounds", true);

    // Ensure that our layer has a content scale set to
    // match the scale factor of the window. This avoids
    // magnification issues leading to blurry rendering.
    layer.layer.setProperty("contentsScale", info.scaleFactor);

    // This makes it so that our display callback will actually be called.
    layer.layer.setProperty("needsDisplayOnBoundsChange", true);

    return .{
        .layer = layer,
        .device = device,
        .queue = queue,
        .blending = opts.config.blending,
        .default_storage_mode = default_storage_mode,
        .max_texture_size = max_texture_size,
    };
}

pub fn deinit(self: *Metal) void {
    self.queue.release();
    self.device.release();
    self.layer.release();
}

/// Create or update a CALayerHost for browser overlay (Issue 625/626/627).
/// The CALayerHost displays the remote CAContext from Chromium's GPU process.
/// Window Server composites directly from GPU VRAM — zero per-frame IPC.
///
/// Layer tree (Issue 627 Experiment 2):
///   IOSurfaceLayer → flipped_layer (geometryFlipped, auto-fills parent)
///     → positioning_layer (explicit frame, top-origin Y)
///       → CALayerHost (at origin)
pub fn setCALayerHostContextId(
    self: *Metal,
    context_id: u32,
    ca_layer_host_ptr: *?*anyopaque,
    ca_layer_flipped_ptr: *?*anyopaque,
    ca_layer_positioning_ptr: *?*anyopaque,
) void {
    const CALayerHost = objc.getClass("CALayerHost") orelse {
        log.warn("CALayerHost class not found", .{});
        return;
    };
    const CALayer = objc.getClass("CALayer") orelse {
        log.warn("CALayer class not found", .{});
        return;
    };

    if (ca_layer_host_ptr.*) |existing| {
        // Replace existing CALayerHost with a new one (Issue 628).
        // Chromium's DisplayCALayerTree always creates a new CALayerHost
        // when the ca_context_id changes — updating contextId on an
        // existing host may not rebind Window Server compositing.
        const old_host = objc.Object.fromId(existing);
        old_host.msgSend(void, objc.sel("removeFromSuperlayer"), .{});
        old_host.release();

        const kCALayerMaxXMargin: c_uint = 1 << 2; // 4
        const kCALayerMaxYMargin: c_uint = 1 << 5; // 32

        const new_host_id = CALayerHost.msgSend(objc.c.id, objc.sel("layer"), .{});
        const new_host = objc.Object.fromId(new_host_id).retain();
        new_host.setProperty("contextId", @as(u32, context_id));
        new_host.setProperty("anchorPoint", macos.graphics.Point{ .x = 0, .y = 0 });
        new_host.setProperty("autoresizingMask", kCALayerMaxXMargin | kCALayerMaxYMargin);

        // Add to positioning_layer (must exist if we had an existing host).
        if (ca_layer_positioning_ptr.*) |pos_ptr| {
            const pos = objc.Object.fromId(pos_ptr);
            pos.msgSend(void, objc.sel("addSublayer:"), .{new_host.value});
        }
        ca_layer_host_ptr.* = new_host.value;

        log.info("replaced CALayerHost contextId={}", .{context_id});
    } else {
        // CAAutoresizingMask constants from QuartzCore.
        const kCALayerWidthSizable: c_uint = 1 << 1; // 2
        const kCALayerMaxXMargin: c_uint = 1 << 2; // 4
        const kCALayerHeightSizable: c_uint = 1 << 4; // 16
        const kCALayerMaxYMargin: c_uint = 1 << 5; // 32

        // Create intermediate flipped layer (matches Chromium's maybe_flipped_layer_).
        // Auto-fills the parent IOSurfaceLayer. geometryFlipped=YES gives
        // sublayers a top-left-origin coordinate system.
        const flipped_id = CALayer.msgSend(objc.c.id, objc.sel("layer"), .{});
        const flipped = objc.Object.fromId(flipped_id).retain();
        flipped.setProperty("geometryFlipped", true);
        flipped.setProperty("anchorPoint", macos.graphics.Point{ .x = 0, .y = 0 });
        // Set initial frame to parent bounds — autoresizingMask only handles
        // subsequent size changes, not initial sizing.
        const parent_bounds = self.layer.layer.getProperty(macos.graphics.Rect, "bounds");
        flipped.setProperty("frame", parent_bounds);
        flipped.setProperty("autoresizingMask", kCALayerWidthSizable | kCALayerHeightSizable);
        self.layer.layer.msgSend(void, objc.sel("addSublayer:"), .{flipped.value});
        ca_layer_flipped_ptr.* = flipped.value;

        // Create positioning layer inside the flipped layer.
        // This layer gets the explicit frame at the overlay grid rectangle.
        // Its Y coordinate is top-origin (because the parent is flipped).
        const pos_id = CALayer.msgSend(objc.c.id, objc.sel("layer"), .{});
        const pos = objc.Object.fromId(pos_id).retain();
        pos.setProperty("anchorPoint", macos.graphics.Point{ .x = 0, .y = 0 });
        flipped.msgSend(void, objc.sel("addSublayer:"), .{pos.value});
        ca_layer_positioning_ptr.* = pos.value;

        // Create CALayerHost as sublayer of the positioning layer.
        const host_id = CALayerHost.msgSend(objc.c.id, objc.sel("layer"), .{});
        const host = objc.Object.fromId(host_id).retain();
        host.setProperty("contextId", @as(u32, context_id));
        host.setProperty("anchorPoint", macos.graphics.Point{ .x = 0, .y = 0 });
        host.setProperty("autoresizingMask", kCALayerMaxXMargin | kCALayerMaxYMargin);
        pos.msgSend(void, objc.sel("addSublayer:"), .{host.value});
        ca_layer_host_ptr.* = host.value;

        log.info("created CALayerHost contextId={} with flipped + positioning layers", .{context_id});
    }
}

/// Update the positioning layer frame to match overlay grid coordinates.
/// Cell dimensions and padding are in physical pixels; CALayer frames use
/// logical points. The positioning layer sits inside the flipped layer
/// (geometryFlipped=YES), so Y=0 is at the top — no Y flip needed.
/// Padding is added so the overlay aligns with the grid, not the surface edge.
pub fn updateCALayerHostFrame(
    self: *Metal,
    positioning_ptr: *anyopaque,
    grid_col: f32,
    grid_row: f32,
    grid_width: f32,
    grid_height: f32,
    cell_width: u32,
    cell_height: u32,
    padding_top: u32,
    padding_left: u32,
) void {
    const positioning = objc.Object.fromId(positioning_ptr);
    const scale = self.layer.layer.getProperty(f64, "contentsScale");
    const cw: f64 = @floatFromInt(cell_width);
    const ch: f64 = @floatFromInt(cell_height);
    const pt: f64 = @floatFromInt(padding_top);
    const pl: f64 = @floatFromInt(padding_left);

    // Convert physical pixels to logical points. Add padding so the overlay
    // aligns with the terminal grid (which starts at padding offset).
    // Y is top-origin — the parent flipped_layer has geometryFlipped=YES.
    const x: f64 = @as(f64, grid_col) * cw / scale + pl / scale;
    const y: f64 = @as(f64, grid_row) * ch / scale + pt / scale;
    const w: f64 = @as(f64, grid_width) * cw / scale;
    const h: f64 = @as(f64, grid_height) * ch / scale;

    const frame = macos.graphics.Rect{
        .origin = .{ .x = x, .y = y },
        .size = .{ .width = w, .height = h },
    };
    positioning.setProperty("frame", frame);
}

/// Remove and release the CALayerHost, positioning layer, and flipped layer.
pub fn removeCALayerHost(self: *Metal, host_ptr: ?*anyopaque, positioning_ptr: ?*anyopaque, flipped_ptr: ?*anyopaque) void {
    _ = self;
    if (host_ptr) |ptr| {
        const host = objc.Object.fromId(ptr);
        host.msgSend(void, objc.sel("removeFromSuperlayer"), .{});
        host.release();
    }
    if (positioning_ptr) |ptr| {
        const pos = objc.Object.fromId(ptr);
        pos.msgSend(void, objc.sel("removeFromSuperlayer"), .{});
        pos.release();
    }
    if (flipped_ptr) |ptr| {
        const flipped = objc.Object.fromId(ptr);
        flipped.msgSend(void, objc.sel("removeFromSuperlayer"), .{});
        flipped.release();
    }
}

pub fn loopEnter(self: *Metal) void {
    const renderer: *align(1) Renderer = @fieldParentPtr("api", self);
    self.layer.setDisplayCallback(
        @ptrCast(&displayCallback),
        @ptrCast(renderer),
    );
}

fn displayCallback(renderer: *Renderer) align(8) void {
    renderer.drawFrame(true) catch |err| {
        log.warn("Error drawing frame in display callback, err={}", .{err});
    };
}

/// Actions taken before doing anything in `drawFrame`.
///
/// Right now we use this to start an AutoreleasePool.
pub fn drawFrameStart(self: *Metal) void {
    assert(self.autorelease_pool == null);
    self.autorelease_pool = .init();
}

/// Actions taken after `drawFrame` is done.
///
/// Right now we use this to end our AutoreleasePool.
pub fn drawFrameEnd(self: *Metal) void {
    assert(self.autorelease_pool != null);
    self.autorelease_pool.?.deinit();
    self.autorelease_pool = null;
}

pub fn initShaders(
    self: *const Metal,
    alloc: Allocator,
    custom_shaders: []const [:0]const u8,
) !shaders.Shaders {
    return try shaders.Shaders.init(
        alloc,
        self.device,
        custom_shaders,
        // Using an `*_srgb` pixel format makes Metal gamma encode
        // the pixels written to it *after* blending, which means
        // we get linear alpha blending rather than gamma-incorrect
        // blending.
        if (self.blending.isLinear())
            mtl.MTLPixelFormat.bgra8unorm_srgb
        else
            mtl.MTLPixelFormat.bgra8unorm,
    );
}

/// Get the current size of the runtime surface.
pub fn surfaceSize(self: *const Metal) !struct { width: u32, height: u32 } {
    const bounds = self.layer.layer.getProperty(graphics.Rect, "bounds");
    const scale = self.layer.layer.getProperty(f64, "contentsScale");

    // We need to clamp our runtime surface size to the maximum
    // possible texture size since we can't create a screen buffer (texture)
    // larger than that.
    return .{
        .width = @min(
            @as(u32, @intFromFloat(bounds.size.width * scale)),
            self.max_texture_size,
        ),
        .height = @min(
            @as(u32, @intFromFloat(bounds.size.height * scale)),
            self.max_texture_size,
        ),
    };
}

/// Initialize a new render target which can be presented by this API.
pub fn initTarget(self: *const Metal, width: usize, height: usize) !Target {
    return Target.init(.{
        .device = self.device,
        // Using an `*_srgb` pixel format makes Metal gamma encode the pixels
        // written to it *after* blending, which means we get linear alpha
        // blending rather than gamma-incorrect blending.
        .pixel_format = if (self.blending.isLinear())
            .bgra8unorm_srgb
        else
            .bgra8unorm,
        .storage_mode = self.default_storage_mode,
        .width = width,
        .height = height,
    });
}

/// Present the provided target.
pub inline fn present(self: *Metal, target: Target, sync: bool) !void {
    if (sync) {
        self.layer.setSurfaceSync(target.surface);
    } else {
        try self.layer.setSurface(target.surface);
    }
}

/// Present the last presented target again. (noop for Metal)
pub inline fn presentLastTarget(self: *Metal) !void {
    _ = self;
}

/// Returns the options to use when constructing buffers.
pub inline fn bufferOptions(self: Metal) bufferpkg.Options {
    return .{
        .device = self.device,
        .resource_options = .{
            // Indicate that the CPU writes to this resource but never reads it.
            .cpu_cache_mode = .write_combined,
            .storage_mode = self.default_storage_mode,
        },
    };
}

pub const instanceBufferOptions = bufferOptions;
pub const uniformBufferOptions = bufferOptions;
pub const fgBufferOptions = bufferOptions;
pub const bgBufferOptions = bufferOptions;
pub const imageBufferOptions = bufferOptions;
pub const bgImageBufferOptions = bufferOptions;

/// Returns the options to use when constructing textures.
pub inline fn textureOptions(self: Metal) Texture.Options {
    return .{
        .device = self.device,
        // Using an `*_srgb` pixel format makes Metal gamma encode the pixels
        // written to it *after* blending, which means we get linear alpha
        // blending rather than gamma-incorrect blending.
        .pixel_format = if (self.blending.isLinear())
            .bgra8unorm_srgb
        else
            .bgra8unorm,
        .resource_options = .{
            // Indicate that the CPU writes to this resource but never reads it.
            .cpu_cache_mode = .write_combined,
            .storage_mode = self.default_storage_mode,
        },
        .usage = .{
            // textureOptions is currently only used for custom shaders,
            // which require both the shader read (for when multiple shaders
            // are chained) and render target (for the final output) usage.
            // Disabling either of these will lead to metal validation
            // errors in Xcode.
            .shader_read = true,
            .render_target = true,
        },
    };
}

pub inline fn samplerOptions(self: Metal) Sampler.Options {
    return .{
        .device = self.device,

        // These parameters match Shadertoy behaviors.
        .min_filter = .linear,
        .mag_filter = .linear,
        .s_address_mode = .clamp_to_edge,
        .t_address_mode = .clamp_to_edge,
    };
}

/// Pixel format for image texture options.
pub const ImageTextureFormat = enum {
    /// 1 byte per pixel grayscale.
    gray,
    /// 4 bytes per pixel RGBA.
    rgba,
    /// 4 bytes per pixel BGRA.
    bgra,

    fn toPixelFormat(
        self: ImageTextureFormat,
        srgb: bool,
    ) mtl.MTLPixelFormat {
        return switch (self) {
            .gray => if (srgb) .r8unorm_srgb else .r8unorm,
            .rgba => if (srgb) .rgba8unorm_srgb else .rgba8unorm,
            .bgra => if (srgb) .bgra8unorm_srgb else .bgra8unorm,
        };
    }
};

/// Returns the options to use when constructing textures for images.
pub inline fn imageTextureOptions(
    self: Metal,
    format: ImageTextureFormat,
    srgb: bool,
) Texture.Options {
    return .{
        .device = self.device,
        .pixel_format = format.toPixelFormat(srgb),
        .resource_options = .{
            // Indicate that the CPU writes to this resource but never reads it.
            .cpu_cache_mode = .write_combined,
            .storage_mode = self.default_storage_mode,
        },
        .usage = .{
            // We only need to read from this texture from a shader.
            .shader_read = true,
        },
    };
}

/// Initializes a Texture suitable for the provided font atlas.
pub fn initAtlasTexture(
    self: *const Metal,
    atlas: *const font.Atlas,
) Texture.Error!Texture {
    const pixel_format: mtl.MTLPixelFormat = switch (atlas.format) {
        .grayscale => .r8unorm,
        .bgra => .bgra8unorm_srgb,
        else => @panic("unsupported atlas format for Metal texture"),
    };

    return try Texture.init(
        .{
            .device = self.device,
            .pixel_format = pixel_format,
            .resource_options = .{
                // Indicate that the CPU writes to this resource but never reads it.
                .cpu_cache_mode = .write_combined,
                .storage_mode = self.default_storage_mode,
            },
            .usage = .{
                // We only need to read from this texture from a shader.
                .shader_read = true,
            },
        },
        atlas.size,
        atlas.size,
        null,
    );
}

/// Begin a frame.
pub inline fn beginFrame(
    self: *const Metal,
    /// Once the frame has been completed, the `frameCompleted` method
    /// on the renderer is called with the health status of the frame.
    renderer: *Renderer,
    /// The target is presented via the provided renderer's API when completed.
    target: *Target,
) !Frame {
    return try Frame.begin(.{ .queue = self.queue }, renderer, target);
}

fn chooseDevice() error{NoMetalDevice}!objc.Object {
    var chosen_device: ?objc.Object = null;

    switch (comptime builtin.os.tag) {
        .macos => {
            const devices = objc.Object.fromId(mtl.MTLCopyAllDevices());
            defer devices.release();

            var iter = devices.iterate();
            while (iter.next()) |device| {
                // We want a GPU that’s connected to a display.
                if (device.getProperty(bool, "isHeadless")) continue;
                chosen_device = device;
                // If the user has an eGPU plugged in, they probably want
                // to use it. Otherwise, integrated GPUs are better for
                // battery life and thermals.
                if (device.getProperty(bool, "isRemovable") or
                    device.getProperty(bool, "isLowPower")) break;
            }
        },
        .ios => {
            chosen_device = objc.Object.fromId(mtl.MTLCreateSystemDefaultDevice());
        },
        else => @compileError("unsupported target for Metal"),
    }

    const device = chosen_device orelse return error.NoMetalDevice;
    return device.retain();
}

/// Determines the maximum 2D texture size supported by the device.
/// We need to clamp our frame size to this if it's larger.
fn queryMaxTextureSize(device: objc.Object) u32 {
    // https://developer.apple.com/metal/Metal-Feature-Set-Tables.pdf

    if (device.msgSend(
        bool,
        objc.sel("supportsFamily:"),
        .{mtl.MTLGPUFamily.apple10},
    )) return 32768;

    if (device.msgSend(
        bool,
        objc.sel("supportsFamily:"),
        .{mtl.MTLGPUFamily.apple3},
    )) return 16384;

    return 8192;
}
