// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "ParticleCodeKit",
    platforms: [
        .iOS(.v13),
        // macOS is for `swift build` / `swift test` and desktop (Tauri) hosts.
        .macOS(.v11),
    ],
    products: [
        .library(name: "ParticleCodeKit", targets: ["ParticleCodeKit"]),
    ],
    targets: [
        // The Rust static library plus particlecode.h and a module map, built by
        // `scripts/build-xcframework.sh` (add `--macos` for `swift build` / `swift test`).
        // The script copies the result to Frameworks/ because SwiftPM wants a local binary
        // target inside the package directory.
        //
        // To publish: zip the xcframework (the script prints the checksum), upload the zip
        // to a release, then replace this target with
        //
        //     .binaryTarget(
        //         name: "ParticleCodeFFI",
        //         url: "https://github.com/<org>/<repo>/releases/download/<tag>/ParticleCodeFFI.xcframework.zip",
        //         checksum: "<output of: swift package compute-checksum ParticleCodeFFI.xcframework.zip>"
        //     ),
        .binaryTarget(
            name: "ParticleCodeFFI",
            path: "Frameworks/ParticleCodeFFI.xcframework"
        ),
        .target(
            name: "ParticleCodeKit",
            dependencies: ["ParticleCodeFFI"],
            linkerSettings: [
                // Rust's std needs libiconv; a static library cannot carry linker flags.
                // (libc, libm and libSystem are linked by default.)
                .linkedLibrary("iconv"),
            ]
        ),
        .testTarget(
            name: "ParticleCodeKitTests",
            dependencies: ["ParticleCodeKit"]
        ),
    ]
)
