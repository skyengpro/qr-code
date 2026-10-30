#
# CocoaPods spec of the particle_code Flutter plugin (iOS). See ios/README.md.
#
# Before `flutter build ios` / `pod install`, build the Rust library and copy it here:
#
#   scripts/build-xcframework.sh --flutter     # from the repository root, on a Mac
#
# which leaves ios/Frameworks/ParticleCodeFFI.xcframework (git-ignored).
#
Pod::Spec.new do |s|
  s.name             = 'particle_code'
  s.version          = '0.1.0'
  s.summary          = 'Particle Code for Flutter: show payloads as animated dots, scan them with the camera.'
  s.description      = <<-DESC
    Native iOS side of the particle_code plugin: an AVFoundation scanner that feeds camera
    frames to the Rust decoder in place (ParticleCodeKit), a camera preview platform view and
    the brightness/idle-timer control for showing a code. The sender (encoder) is reached from
    Dart through dart:ffi.
  DESC
  s.homepage         = 'https://github.com/vaam-apps/qr-code'
  s.license          = { :type => 'Proprietary' }
  s.author           = { 'Stephane Segning' => 'stephane.segning@ssegning.com' }
  s.source           = { :path => '.' }

  s.platform = :ios, '13.0'
  s.swift_version = '5.9'
  s.dependency 'Flutter'

  # Everything ends up in the Runner executable, even when the app uses `use_frameworks!`.
  # dart:ffi looks the encoder functions up with DynamicLibrary.process(), and the
  # force_load below must apply to the same link step that contains the Swift code.
  s.static_framework = true

  # The plugin itself plus ParticleCodeKit's sources. Pod sources must live inside the pod
  # directory, so Classes/ParticleCodeKit is a real directory holding one relative symlink per
  # file of apple/ParticleCodeKit/Sources/ParticleCodeKit: one set of Swift sources serves
  # SwiftPM, the ParticleCodeKit pod and this plugin. Not a symlinked *directory*: CocoaPods
  # lists pod files with a `**` glob, which does not descend into symlinked directories, so
  # those files would silently be left out (tool/check-kit-links.sh guards the links).
  s.source_files = 'Classes/*.swift', 'Classes/ParticleCodeKit/*.swift'

  # Static-library xcframework (scripts/build-xcframework.sh) with particlecode.h and a
  # module.modulemap in its Headers, which is what makes `import ParticleCodeFFI` work.
  s.vendored_frameworks = 'Frameworks/ParticleCodeFFI.xcframework'

  s.frameworks = 'AVFoundation', 'CoreVideo', 'CoreMedia', 'UIKit'
  # Rust's std needs libiconv (libc, libm and libSystem are linked anyway).
  s.libraries  = 'iconv'

  # Flutter.framework does not contain an i386 slice.
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386',
  }

  # Keep the pc_* C functions that only Dart calls.
  #
  # CocoaPods (1.11+) copies the matching slice of a vendored static xcframework to
  # ${PODS_XCFRAMEWORKS_BUILD_DIR}/<pod name>/ (libparticlecode_ffi.a and Headers/) during
  # the build and links it with -lparticlecode_ffi, which only loads archive members that
  # something references. -force_load loads all of them, so the encoder is in the app.
  # ${PODS_XCFRAMEWORKS_BUILD_DIR} is defined in the Pods-Runner xcconfig, which Runner
  # includes, so the variable also expands in the user target. Because that directory is
  # per-SDK, one setting covers device and simulator.
  #
  # -force_load alone does not survive -dead_strip in release builds; the real guarantee is
  # Classes/SymbolKeeper.swift, which references every pc_* function from live code. If
  # the force_load path ever breaks (a CocoaPods layout change fails the link with "file not
  # found"), delete this line: the keeper still holds.
  s.user_target_xcconfig = {
    'OTHER_LDFLAGS' => '$(inherited) -force_load "${PODS_XCFRAMEWORKS_BUILD_DIR}/particle_code/libparticlecode_ffi.a"',
    # dart:ffi finds the pc_* functions with dlsym (DynamicLibrary.process()), so release
    # builds must keep global symbols: strip only local ones.
    'STRIP_STYLE' => 'non-global',
  }
end
