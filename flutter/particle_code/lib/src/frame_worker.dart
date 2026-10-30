import 'dart:async';
import 'dart:isolate';

import 'package:flutter/foundation.dart';

import 'encoder.dart';
import 'frame_spec.dart';

/// Builds frames of a [ParticleEncoder] on a long-lived background isolate.
///
/// The isolate creates its own encoder from an [EncoderWorkerSpec] (payload, options and
/// the pinned session identity), so its frames are identical to the ones the main
/// encoder produces for the same `(index, unixSeconds)`. Reed-Solomon coding, JSON
/// serialization and parsing all happen there; frames come back as a handful of typed
/// arrays ([FrameSpec.toMessage]) and are rebuilt on the receiving isolate in microseconds.
///
/// Requests are processed lowest index first, and requests below the `floor` of the
/// latest request are dropped, so a worker that falls behind skips stale frames instead
/// of working through a backlog.
class FrameWorker {
  FrameWorker._(this._onFrame, this._onFailure);

  /// Spawns a worker for [spec].
  ///
  /// [onFrame] is called for every requested frame that was built; [onFailure] once if
  /// the worker cannot start, fails to build its encoder, or dies. Both are called from
  /// the isolate that called [start]. Never throws: failures go to [onFailure].
  factory FrameWorker.start(
    EncoderWorkerSpec spec, {
    required void Function(int index, FrameSpec frame) onFrame,
    required void Function(Object error) onFailure,
  }) {
    final w = FrameWorker._(onFrame, onFailure);
    _live++;
    w._spawn(spec);
    return w;
  }

  static int _live = 0;

  /// Workers that have been started and whose isolate has not exited yet. For tests.
  @visibleForTesting
  static int get liveCount => _live;

  final void Function(int index, FrameSpec frame) _onFrame;
  final void Function(Object error) _onFailure;

  final ReceivePort _inbox = ReceivePort();
  final ReceivePort _exit = ReceivePort();
  final ReceivePort _errors = ReceivePort();
  final Completer<void> _done = Completer<void>();
  SendPort? _toWorker;
  bool _disposed = false;
  bool _failed = false;

  /// Whether the worker has built its encoder and accepts requests.
  bool get isReady => _toWorker != null && !_disposed && !_failed;

  /// Whether [dispose] was called.
  bool get isDisposed => _disposed;

  /// Completes when the background isolate has exited (or never started).
  Future<void> get done => _done.future;

  void _spawn(EncoderWorkerSpec spec) {
    _inbox.listen(_onMessage);
    _errors.listen((Object? e) {
      final list = e is List ? e : [e];
      _fail(StateError('frame worker crashed: ${list.first}'));
    });
    _exit.listen((_) => _finish());
    Isolate.spawn<List<Object?>>(
      _workerMain,
      [_inbox.sendPort, spec.toMessage()],
      onExit: _exit.sendPort,
      onError: _errors.sendPort,
      debugName: 'particle_code frame worker',
    ).then<void>(
      (isolate) {
        if (_disposed) isolate.kill();
      },
      onError: (Object e, StackTrace s) {
        _fail(e);
        _finish();
      },
    );
  }

  void _onMessage(Object? raw) {
    if (raw is! List) return;
    switch (raw[0]) {
      case 'ready':
        _toWorker = raw[1]! as SendPort;
        if (_disposed) _toWorker!.send(const ['dispose']);
      case 'fail':
        _fail(StateError(raw[1]! as String));
      case 'frame':
        if (_disposed) return;
        _onFrame(
          raw[1]! as int,
          FrameSpec.fromMessage(raw[2]! as List<Object?>),
        );
      case 'error':
        // A single frame failed; the worker keeps running.
        if (!_disposed) _fail(StateError(raw[2]! as String));
    }
  }

  void _fail(Object error) {
    if (_failed || _disposed) return;
    _failed = true;
    _onFailure(error);
  }

  /// Asks for frame [index] with the particles showing the liveness value of
  /// [unixSeconds]. [floor] is the lowest index still wanted: older queued requests are
  /// dropped. Ignored until [isReady].
  void request(int index, int unixSeconds, {required int floor}) {
    final port = _toWorker;
    if (port == null || _disposed || _failed) return;
    port.send(['get', index, unixSeconds, floor]);
  }

  /// Stops the worker: it frees its encoder and exits. Idempotent. [done] completes
  /// once the isolate is gone.
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    // Still spawning or starting up: `_spawn` kills the isolate when it appears and
    // `_onMessage` answers 'ready' with 'dispose'.
    _toWorker?.send(const ['dispose']);
  }

  void _finish() {
    _inbox.close();
    _exit.close();
    _errors.close();
    if (!_done.isCompleted) {
      _live--;
      _done.complete();
    }
  }
}

class _Req {
  _Req(this.index, this.unix);
  final int index;
  final int unix;
}

@pragma('vm:entry-point')
void _workerMain(List<Object?> init) {
  final out = init[0]! as SendPort;
  final ParticleEncoder encoder;
  try {
    encoder = ParticleEncoder.fromWorkerSpec(
      EncoderWorkerSpec.fromMessage(init[1]! as List<Object?>),
    );
  } on Object catch (e) {
    out.send(['fail', '$e']);
    return;
  }

  final inbox = ReceivePort();
  final queue = <_Req>[];
  var floor = 0;
  var scheduled = false;
  var closed = false;

  void pump() {
    scheduled = false;
    if (closed) return;
    queue.removeWhere((r) => r.index < floor);
    if (queue.isEmpty) return;
    queue.sort((a, b) => a.index.compareTo(b.index));
    final r = queue.removeAt(0);
    try {
      final frame = encoder.frame(r.index, unixSeconds: r.unix);
      out.send(['frame', r.index, frame.toMessage()]);
    } on Object catch (e) {
      out.send(['error', r.index, '$e']);
    }
    if (queue.isNotEmpty) {
      scheduled = true;
      // Yield so messages that arrived meanwhile (a newer floor) are seen first.
      Timer.run(pump);
    }
  }

  inbox.listen((Object? m) {
    if (m is! List) return;
    switch (m[0]) {
      case 'get':
        final index = m[1]! as int;
        floor = floor > (m[3]! as int) ? floor : m[3]! as int;
        queue.add(_Req(index, m[2]! as int));
        if (!scheduled) {
          scheduled = true;
          Timer.run(pump);
        }
      case 'dispose':
        closed = true;
        queue.clear();
        encoder.dispose();
        inbox.close();
    }
  });
  out.send(['ready', inbox.sendPort]);
}
