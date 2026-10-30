// swift-tools-version: 5.9
//
// Swift Package Manager manifest of the particle_code plugin's iOS side. Flutter uses it when
// SwiftPM is enabled (the default); CocoaPods users get the same sources through
// ../particle_code.podspec.
//
// The ParticleCodeFFI binary target is the Rust C ABI (crates/particlecode-ffi) as a static
// xcframework. It is not committed: `scripts/build-xcframework.sh --flutter` copies it into
// Frameworks/ next to this file. Sources/particle_code/ParticleCodeKit/ holds per-file symlinks
// to apple/ParticleCodeKit/Sources/ParticleCodeKit, compiled into this target.

import PackageDescription

let package = Package(
    name: "particle_code",
    platforms: [
        .iOS("15.0")
    ],
    products: [
        .library(name: "particle-code", targets: ["particle_code"])
    ],
    dependencies: [
        .package(name: "FlutterFramework", path: "../FlutterFramework")
    ],
    targets: [
        .binaryTarget(
            name: "ParticleCodeFFI",
            path: "Frameworks/ParticleCodeFFI.xcframework"
        ),
        .target(
            name: "particle_code",
            dependencies: [
                .product(name: "FlutterFramework", package: "FlutterFramework"),
                "ParticleCodeFFI",
            ],
            resources: [
                .process("PrivacyInfo.xcprivacy")
            ],
            linkerSettings: [
                // The Rust standard library needs libiconv on Apple platforms.
                .linkedLibrary("iconv"),
                .linkedFramework("AVFoundation"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("CoreVideo"),
                .linkedFramework("UIKit"),
            ]
        ),
    ]
)
