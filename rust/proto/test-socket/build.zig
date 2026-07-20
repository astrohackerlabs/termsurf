const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const exe = b.addExecutable(.{
        .name = "socket-server",
        .root_module = b.createModule(.{
            .root_source_file = b.path("server.zig"),
            .target = target,
            .optimize = optimize,
        }),
    });

    // Add the generated protobuf-c source file.
    exe.root_module.addCSourceFile(.{
        .file = b.path("termsurf.pb-c.c"),
    });

    // protobuf-c headers (for both the generated code and server.zig).
    exe.root_module.addIncludePath(b.path("."));
    exe.root_module.addSystemIncludePath(.{ .cwd_relative = "/opt/homebrew/include" });

    // Link against libprotobuf-c.
    exe.root_module.addLibraryPath(.{ .cwd_relative = "/opt/homebrew/lib" });
    exe.root_module.linkSystemLibrary("protobuf-c", .{});
    exe.root_module.link_libc = true;

    b.installArtifact(exe);

    const run_cmd = b.addRunArtifact(exe);
    run_cmd.step.dependOn(b.getInstallStep());

    const run_step = b.step("run", "Run the server");
    run_step.dependOn(&run_cmd.step);
}
