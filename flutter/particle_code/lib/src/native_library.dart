import 'dart:io';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show ExternalLibrary;

import 'exception.dart';

/// Environment variable naming the native library on desktop hosts and in tests.
const String libraryEnvVar = 'PARTICLECODE_LIB';

/// Opens the native Particle Code library for the current platform, for
/// `RustLib.init(externalLibrary: ...)`.
///
/// * Android: `libparticlecode_jni.so` (built into the plugin's `jniLibs`; it exports the
///   flutter_rust_bridge symbols as well as the JNI scanner entry points).
/// * iOS: the symbols linked into the app by `ParticleCodeFFI.xcframework` (the process
///   image).
/// * Other hosts (tests, desktop): the file named by `$PARTICLECODE_LIB`, otherwise the
///   newest `target/{debug,release}/libparticlecode_ffi.{so,dylib}` found by searching
///   upwards from the working directory.
///
/// Throws [ParticleCodeException] when nothing can be opened.
ExternalLibrary openParticleLibrary() {
  try {
    if (Platform.isAndroid) {
      return ExternalLibrary.open('libparticlecode_jni.so');
    }
    if (Platform.isIOS) {
      return ExternalLibrary.process(iKnowHowToUseIt: true);
    }
    final path = findHostLibrary();
    if (path == null) {
      throw const ParticleCodeException(
        'particlecode native library not found. Set \$$libraryEnvVar or run '
        '`cargo build -p particlecode-ffi` in the workspace.',
      );
    }
    return ExternalLibrary.open(path);
  } on ArgumentError catch (e) {
    throw ParticleCodeException(
      'cannot open particlecode library: ${e.message}',
    );
  } on UnsupportedError catch (e) {
    throw ParticleCodeException(
      'cannot open particlecode library: ${e.message}',
    );
  }
}

/// File name of the host library for [os] (a `Platform.operatingSystem` value).
String hostLibraryName(String os) {
  switch (os) {
    case 'macos':
      return 'libparticlecode_ffi.dylib';
    case 'windows':
      return 'particlecode_ffi.dll';
    default:
      return 'libparticlecode_ffi.so';
  }
}

/// Finds the host library via `$PARTICLECODE_LIB` or the workspace `target` directory.
///
/// Returns `null` if neither exists. [start] defaults to the working directory.
String? findHostLibrary({Directory? start, Map<String, String>? environment}) {
  final env = environment ?? Platform.environment;
  final fromEnv = env[libraryEnvVar];
  if (fromEnv != null && fromEnv.isNotEmpty) {
    return fromEnv;
  }
  final name = hostLibraryName(Platform.operatingSystem);
  var dir = (start ?? Directory.current).absolute;
  while (true) {
    File? best;
    DateTime? bestTime;
    for (final profile in const ['debug', 'release']) {
      final file = File(
        '${dir.path}${Platform.pathSeparator}target'
        '${Platform.pathSeparator}$profile${Platform.pathSeparator}$name',
      );
      if (file.existsSync()) {
        final modified = file.statSync().modified;
        if (bestTime == null || modified.isAfter(bestTime)) {
          best = file;
          bestTime = modified;
        }
      }
    }
    if (best != null) return best.path;
    final parent = dir.parent;
    if (parent.path == dir.path) return null;
    dir = parent;
  }
}
