import 'package:particle_code/particle_code.dart';

/// The small slice of `ParticleScanner` the spike uses, so the recorder can be tested
/// with fake event and diagnostics streams.
abstract class ScannerPort {
  /// Scanner events (progress, segments, done, errors).
  Stream<ScanEvent> get events;

  /// Unthrottled typed diagnostics.
  Stream<ScanDiagnostic> get diagnostics;

  /// Asks for the camera permission; whether it is granted.
  Future<bool> requestPermission();

  /// Starts the camera and decoding.
  Future<void> start();

  /// Stops the camera.
  Future<void> stop();

  /// Releases the scanner.
  void dispose();
}

/// [ScannerPort] over the plugin's real [ParticleScanner], using only its public API.
class PluginScannerPort implements ScannerPort {
  /// Wraps [scanner] (a new one by default).
  PluginScannerPort([ParticleScanner? scanner])
    : scanner = scanner ?? ParticleScanner();

  /// The scanner, for `ParticleScannerPreview`.
  final ParticleScanner scanner;

  @override
  Stream<ScanEvent> get events => scanner.events;

  @override
  Stream<ScanDiagnostic> get diagnostics => scanner.diagnostics;

  @override
  Future<bool> requestPermission() => scanner.requestPermission();

  @override
  Future<void> start() => scanner.start();

  @override
  Future<void> stop() => scanner.stop();

  @override
  void dispose() => scanner.dispose();
}
