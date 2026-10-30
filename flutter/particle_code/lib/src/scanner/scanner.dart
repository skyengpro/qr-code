import 'dart:async';

import 'package:flutter/services.dart';

import 'errors.dart';
import 'events.dart';

/// Controls the native camera scanner (CameraX on Android, AVFoundation on iOS).
///
/// Camera frames never cross into Dart: the native side decodes them with the Rust
/// library and sends only small progress events ([events]) and, at the end, the payload.
///
/// ```dart
/// final scanner = ParticleScanner();
/// if (await scanner.requestPermission()) {
///   scanner.events.listen((e) { if (e is ScanDone) use(e.payload); });
///   await scanner.start();
/// }
/// // ... later
/// scanner.dispose();
/// ```
class ParticleScanner {
  /// Creates a scanner. The channels default to the plugin's; override them in tests.
  ParticleScanner({MethodChannel? methodChannel, EventChannel? eventChannel})
    : _method = methodChannel ?? channel,
      _eventChannel = eventChannel ?? eventsChannel {
    _controller = StreamController<ScanEvent>.broadcast(
      onListen: _attach,
      onCancel: _detach,
    );
  }

  /// The method channel `particle_code/scanner`.
  static const MethodChannel channel = MethodChannel('particle_code/scanner');

  /// The event channel `particle_code/scan_events`.
  static const EventChannel eventsChannel = EventChannel(
    'particle_code/scan_events',
  );

  final MethodChannel _method;
  final EventChannel _eventChannel;
  late final StreamController<ScanEvent> _controller;
  StreamSubscription<dynamic>? _subscription;
  bool _disposed = false;

  /// Broadcast stream of scanner events. Listening does not start the camera; call
  /// [start] for that. Malformed events arrive as a [ScanError] with code
  /// `malformed_event`; events of unknown type are ignored.
  Stream<ScanEvent> get events => _controller.stream;

  void _attach() {
    _subscription = _eventChannel.receiveBroadcastStream().listen(
      _onRaw,
      onError: _onError,
    );
  }

  Future<void> _detach() async {
    final sub = _subscription;
    _subscription = null;
    await sub?.cancel();
  }

  void _onRaw(dynamic raw) {
    if (_controller.isClosed) return;
    try {
      final event = ScanEvent.fromMap(raw);
      if (event != null) _controller.add(event);
    } on FormatException catch (e) {
      _controller.add(ScanError('malformed_event', e.message));
    }
  }

  void _onError(Object error) {
    if (_controller.isClosed) return;
    if (error is PlatformException) {
      _controller.add(ScanError(error.code, error.message ?? ''));
    } else {
      _controller.add(ScanError('unknown', '$error'));
    }
  }

  /// Asks for the camera permission; returns whether it is granted.
  Future<bool> requestPermission() async =>
      await _invoke<bool>('requestPermission') ?? false;

  /// Starts the camera and decoder. Completes once the camera runs.
  ///
  /// Throws [ParticleScannerException] with [ParticleScannerError.permissionDenied],
  /// [ParticleScannerError.noCamera] or [ParticleScannerError.cameraError].
  Future<void> start({bool torch = false}) =>
      _invoke<void>('start', {'torch': torch});

  /// Stops the camera. Idempotent.
  Future<void> stop() => _invoke<void>('stop');

  /// Drops progress and starts a new decoder, keeping the camera running.
  Future<void> reset() => _invoke<void>('reset');

  /// Turns the torch on or off. Returns whether the device has a torch.
  Future<bool> setTorch(bool on) async =>
      await _invoke<bool>('setTorch', {'on': on}) ?? false;

  Future<T?> _invoke<T>(String method, [Object? arguments]) async {
    if (_disposed) {
      throw StateError('ParticleScanner used after dispose()');
    }
    try {
      return await _method.invokeMethod<T>(method, arguments);
    } on PlatformException catch (e) {
      throw ParticleScannerException.fromPlatform(e);
    } on MissingPluginException {
      throw const ParticleScannerException(
        ParticleScannerError.unsupported,
        'the particle_code scanner is not available on this platform',
      );
    }
  }

  /// Stops the camera (best effort) and closes [events].
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    // Fire and forget; the platform may already be gone.
    _method
        .invokeMethod<void>('stop')
        .then<void>((_) {}, onError: (Object _) {});
    final sub = _subscription;
    _subscription = null;
    unawaited(sub?.cancel());
    unawaited(_controller.close());
  }
}
