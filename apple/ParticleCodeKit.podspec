# CocoaPods spec for Flutter plugins (or apps) that cannot use Swift Package Manager yet.
# SwiftPM (apple/ParticleCodeKit/Package.swift) is preferred; both build the same sources.
#
# Build the xcframework first: scripts/build-xcframework.sh (it also copies it to
# apple/ParticleCodeKit/Frameworks/, the path used below).
Pod::Spec.new do |s|
  s.name             = 'ParticleCodeKit'
  s.version          = '0.2.0'
  s.summary          = 'Particle Code scanner: camera frames to payload, decoded in Rust.'
  s.description      = <<-DESC
    Swift wrapper around the particlecode-ffi static library (a Rust decoder for the animated
    dot code). ParticleDecoder feeds CVPixelBuffer planes to Rust in place;
    ParticleCaptureScanner runs it on a serial queue behind AVCaptureVideoDataOutput.
  DESC
  s.homepage         = 'https://github.com/vaam-apps/qr-code'
  s.license          = { :type => 'MIT' }
  s.author           = { 'Stephane Segning' => 'stephane.segning@ssegning.com' }
  s.source           = { :path => '.' }

  s.ios.deployment_target = '13.0'
  s.swift_versions        = ['5.9']
  s.static_framework      = true

  s.source_files = 'ParticleCodeKit/Sources/ParticleCodeKit/**/*.swift'

  # Static-library xcframework with particlecode.h and module.modulemap in its Headers, so
  # Swift can `import ParticleCodeFFI`.
  s.vendored_frameworks = 'ParticleCodeKit/Frameworks/ParticleCodeFFI.xcframework'

  s.frameworks = 'AVFoundation', 'CoreVideo', 'CoreMedia'
  # Rust's std needs libiconv (libc, libm and libSystem are linked anyway).
  s.libraries  = 'iconv'
end
