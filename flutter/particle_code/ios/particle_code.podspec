#
# CocoaPods spec of the particle_code Flutter plugin (iOS). See ios/README.md.
#
# Pure Swift over AVFoundation: no Rust, no vendored framework. The Rust library is built and
# bundled by the Native Assets hook (hook/build.dart). The same sources also form the Swift
# package ios/particle_code/Package.swift, which Flutter uses instead of this pod when Swift
# Package Manager is enabled.
#
Pod::Spec.new do |s|
  s.name             = 'particle_code'
  s.version          = '0.2.0'
  s.summary          = 'Particle Code for Flutter: show payloads as animated dots, scan them with the camera.'
  s.description      = <<-DESC
    Native iOS side of the particle_code plugin: an AVFoundation camera that hands frames to
    Dart by address, a camera preview platform view and the brightness/idle-timer control for
    showing a code. Decoding and encoding run in Rust, reached from Dart through
    flutter_rust_bridge.
  DESC
  s.homepage         = 'https://github.com/vaam-apps/qr-code'
  s.license          = { :type => 'MIT' }
  s.author           = { 'Stephane Segning' => 'stephane.segning@ssegning.com' }
  s.source           = { :path => '.' }

  s.platform = :ios, '15.0'
  s.swift_version = '5.9'
  s.dependency 'Flutter'

  # The SwiftPM layout, so the pod and the Swift package compile the same files.
  s.source_files = 'particle_code/Sources/particle_code/**/*.swift'
  s.resource_bundles = {
    'particle_code_privacy' => ['particle_code/Sources/particle_code/PrivacyInfo.xcprivacy'],
  }

  s.frameworks = 'AVFoundation', 'CoreVideo', 'CoreMedia', 'UIKit'

  # Flutter.framework does not contain an i386 slice.
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386',
  }
end
