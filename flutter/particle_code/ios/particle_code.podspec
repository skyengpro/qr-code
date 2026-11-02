#
# CocoaPods spec of the particle_code Flutter plugin (iOS). See ios/README.md.
#
# Before `flutter build ios` / `pod install`, build the Rust library and copy it here:
#
#   scripts/build-xcframework.sh --flutter     # from the repository root, on a Mac
#
# which leaves ios/particle_code/Frameworks/ParticleCodeFFI.xcframework (git-ignored). The same
# sources and xcframework also form the Swift package ios/particle_code/Package.swift, which
# Flutter uses instead of this pod when Swift Package Manager is enabled.
#
Pod::Spec.new do |s|
  s.name             = 'particle_code'
  s.version          = '0.2.0'
  s.summary          = 'Particle Code for Flutter: show payloads as animated dots, scan them with the camera.'
  s.description      = <<-DESC
    Native iOS side of the particle_code plugin: an AVFoundation scanner that feeds camera
    frames to the Rust decoder in place (ParticleCodeKit), a camera preview platform view and
    the brightness/idle-timer control for showing a code. The sender (encoder) is reached from
    Dart through dart:ffi.
  DESC
  s.homepage         = 'https://github.com/vaam-apps/qr-code'
  s.license          = { :type => 'MIT' }
  s.author           = { 'Stephane Segning' => 'stephane.segning@ssegning.com' }
  s.source           = { :path => '.' }

  s.platform = :ios, '15.0'
  s.swift_version = '5.9'
  s.dependency 'Flutter'

  # Everything ends up in the Runner executable, even when the app uses `use_frameworks!`.
  # dart:ffi looks the encoder functions up with DynamicLibrary.process(), so they must be
  # linked into the executable itself, next to the Swift code that keeps them.
  s.static_framework = true

  # The plugin itself plus ParticleCodeKit's sources, in the SwiftPM layout so the pod and the
  # Swift package compile the same files. Pod sources must live inside the pod directory, so
  # Sources/particle_code/ParticleCodeKit is a real directory holding one relative symlink per
  # file of apple/ParticleCodeKit/Sources/ParticleCodeKit: one set of Swift sources serves
  # SwiftPM, the ParticleCodeKit pod and this plugin. Not a symlinked *directory*: CocoaPods
  # lists pod files with a `**` glob, which does not descend into symlinked directories, so
  # those files would silently be left out (tool/check-kit-links.sh guards the links).
  s.source_files = 'particle_code/Sources/particle_code/**/*.swift'
  s.resource_bundles = {
    'particle_code_privacy' => ['particle_code/Sources/particle_code/PrivacyInfo.xcprivacy'],
  }

  # Static-library xcframework (scripts/build-xcframework.sh) with particlecode.h and a
  # module.modulemap in its Headers, which is what makes `import ParticleCodeFFI` work.
  s.vendored_frameworks = 'particle_code/Frameworks/ParticleCodeFFI.xcframework'

  s.frameworks = 'AVFoundation', 'CoreVideo', 'CoreMedia', 'UIKit'
  # Rust's std needs libiconv (libc, libm and libSystem are linked anyway).
  s.libraries  = 'iconv'

  # Flutter.framework does not contain an i386 slice.
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386',
  }

  # Keep the pc_* C functions that only Dart calls. CocoaPods links the vendored static
  # xcframework lazily (only archive members something references are loaded), and release
  # builds dead-strip. Sources/particle_code/SymbolKeeper.swift references every pc_* function from live
  # code, which keeps them through both; tool/check-symbols.sh keeps it in step with the
  # header, and CI checks the built app's symbol table. (A -force_load of the archive is not
  # used: CocoaPods does not expose the copied slice as a declared build input, so Xcode
  # rejects the path, and the keeper makes it unnecessary.)
  s.user_target_xcconfig = {
    # dart:ffi finds the pc_* functions with dlsym (DynamicLibrary.process()), so release
    # builds must keep global symbols: strip only local ones.
    'STRIP_STYLE' => 'non-global',
  }
end
