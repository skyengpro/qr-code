// swift-tools-version: 5.9
//
// Swift Package Manager manifest of the particle_code plugin's iOS side. Flutter uses it when
// SwiftPM is enabled (the default); CocoaPods users get the same sources through
// ../particle_code.podspec.
//
// The plugin is pure Swift over AVFoundation: it contains no Rust and links no Rust library.
// The Rust library is built and bundled by the Native Assets hook (hook/build.dart).

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
        .target(
            name: "particle_code",
            dependencies: [
                .product(name: "FlutterFramework", package: "FlutterFramework")
            ],
            resources: [
                .process("PrivacyInfo.xcprivacy")
            ],
            linkerSettings: [
                .linkedFramework("AVFoundation"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("CoreVideo"),
                .linkedFramework("UIKit"),
            ]
        )
    ]
)
