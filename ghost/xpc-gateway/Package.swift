// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "XPCGateway",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(name: "xpc-gateway",
                          path: "Sources")
    ]
)
