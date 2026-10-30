import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart';

/// Bridge to the platform's "a code is on screen" behavior: force maximum screen
/// brightness and keep the screen awake while a Particle Code is shown, and restore both
/// afterwards (`particle_code/display`, method `setShowingCode`).
abstract final class ParticleCodeDisplay {
  /// The method channel (see `PROTOCOL.md`).
  static const MethodChannel channel = MethodChannel('particle_code/display');

  static int _holders = 0;

  /// Sends `setShowingCode(on)` to the platform.
  ///
  /// A missing plugin (unit tests, unsupported platforms) is not an error. Other
  /// platform failures are reported to [FlutterError] but do not throw.
  static Future<void> setShowingCode(bool on) async {
    try {
      await channel.invokeMethod<void>('setShowingCode', {'on': on});
    } on MissingPluginException {
      // No native side registered.
    } on PlatformException catch (e, s) {
      FlutterError.reportError(
        FlutterErrorDetails(
          exception: e,
          stack: s,
          library: 'particle_code',
          context: ErrorDescription('while calling setShowingCode'),
        ),
      );
    }
  }

  /// Registers one more visible code. The first holder turns max brightness on.
  ///
  /// Several [ParticleCodeView]s can be mounted at once; brightness is restored when
  /// the last one releases.
  static void acquire() {
    if (_holders++ == 0) {
      _fireAndForget(setShowingCode(true));
    }
  }

  /// Releases one holder registered with [acquire]. The last release restores brightness.
  static void release() {
    if (_holders == 0) return;
    if (--_holders == 0) {
      _fireAndForget(setShowingCode(false));
    }
  }

  /// Number of codes currently holding max brightness.
  @visibleForTesting
  static int get holders => _holders;

  /// Forgets all holders without notifying the platform. For tests.
  @visibleForTesting
  static void debugReset() => _holders = 0;

  static void _fireAndForget(Future<void> f) {
    f.ignore();
  }
}
