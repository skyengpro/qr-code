import 'dart:async';

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show ExternalLibrary;

import 'exception.dart';
import 'native_library.dart';
import 'rust/frb_generated.dart';

/// Start-up of the Rust library behind the sender.
abstract final class ParticleCode {
  static Future<void>? _pending;

  /// Loads the native library and starts flutter_rust_bridge. Call it once, before
  /// anything else of the sender (`ParticleEncoder`, `livenessVerify`,
  /// `ParticleCodeView`), typically in `main()`:
  ///
  /// ```dart
  /// WidgetsFlutterBinding.ensureInitialized();
  /// await ParticleCode.init();
  /// runApp(const MyApp());
  /// ```
  ///
  /// Idempotent: further calls (also concurrent ones) return once the first one is done.
  /// [library] overrides how the native library is opened; by default Android uses
  /// `libparticlecode_jni.so`, iOS the process image, and desktop hosts and tests
  /// `$PARTICLECODE_LIB` or `target/{debug,release}/libparticlecode_ffi.*`.
  ///
  /// Throws [ParticleCodeException] if the library cannot be loaded or does not match
  /// this package version. A failed call can be retried.
  static Future<void> init({ExternalLibrary? library}) {
    if (isInitialized) return Future<void>.value();
    final pending = _pending;
    if (pending != null) return pending;
    final started = _start(library);
    _pending = started;
    started.then<void>(
      (_) {},
      onError: (Object _) {
        if (identical(_pending, started)) _pending = null;
      },
    );
    return started;
  }

  /// Whether [init] has completed.
  static bool get isInitialized =>
      // ignore: invalid_use_of_internal_member
      RustLib.instance.initialized;

  static Future<void> _start(ExternalLibrary? library) async {
    try {
      await RustLib.init(externalLibrary: library ?? openParticleLibrary());
    } on ParticleCodeException {
      rethrow;
    } on StateError catch (e) {
      throw ParticleCodeException('cannot start particlecode: ${e.message}');
    } on ArgumentError catch (e) {
      throw ParticleCodeException('cannot start particlecode: ${e.message}');
    }
  }

  /// Throws a [StateError] telling the caller to run [init] unless it has completed.
  static void checkInitialized() {
    if (!isInitialized) {
      throw StateError(
        'ParticleCode is not initialized. Call `await ParticleCode.init()` '
        'before using the sender (for example in main()).',
      );
    }
  }
}
