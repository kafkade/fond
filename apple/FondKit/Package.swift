// swift-tools-version:5.9
import PackageDescription

// FondKit wraps the UniFFI-generated Swift bindings (`fond_ffi.swift`) together
// with the prebuilt `Fond.xcframework`. Both are produced by
// `apple/build-xcframework.sh` and are git-ignored — run that script before
// building anything that depends on this package.
let package = Package(
    name: "FondKit",
    platforms: [
        .iOS(.v17),
        .macOS(.v14),
    ],
    products: [
        .library(name: "FondKit", targets: ["FondKit"]),
    ],
    targets: [
        // The compiled Rust core + C headers + module map for every Apple slice.
        .binaryTarget(name: "FondFFI", path: "xcframework/Fond.xcframework"),
        // The generated Swift bindings plus small SwiftUI conveniences.
        .target(
            name: "FondKit",
            dependencies: ["FondFFI"],
            path: "Sources/FondKit"
        ),
    ]
)
