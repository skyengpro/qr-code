import 'dart:io';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show ExternalLibrary;

/// File name of the Rust library for [os] (a `Platform.operatingSystem` value).
String hostLibraryName(String os) {
  switch (os) {
    case 'macos':
    case 'ios':
      return 'libparticlecode_ffi.dylib';
    case 'windows':
      return 'particlecode_ffi.dll';
    default:
      return 'libparticlecode_ffi.so';
  }
}

/// Where `flutter test` (and `flutter run` on a desktop host) leaves the library that the
/// plugin's Native Assets hook built: `build/native_assets/<os>/` under [root], which
/// defaults to the working directory (the package being tested).
File nativeAssetsLibrary({Directory? root, String? os}) {
  final system = os ?? Platform.operatingSystem;
  final sep = Platform.pathSeparator;
  final base = (root ?? Directory.current).path;
  return File(
    '$base${sep}build${sep}native_assets$sep$system$sep${hostLibraryName(system)}',
  );
}

/// The library to hand to `RustLib.init`, or `null` to let flutter_rust_bridge's default
/// loader find it.
///
/// Apps need no help: Native Assets bundles `libparticlecode_ffi.so` into the APK,
/// `particlecode_ffi.framework` into the iOS app and the library next to a desktop
/// executable, which is where the default loader looks. Only `flutter test` keeps the
/// library outside any bundle, in `build/native_assets/<os>/`; this opens it from there.
ExternalLibrary? openParticleLibrary({Directory? root}) {
  if (Platform.isAndroid || Platform.isIOS) return null;
  final file = nativeAssetsLibrary(root: root);
  return file.existsSync() ? ExternalLibrary.open(file.path) : null;
}
