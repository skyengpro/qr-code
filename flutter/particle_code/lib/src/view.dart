import 'dart:math' as math;

import 'package:flutter/foundation.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

import 'display.dart';
import 'encoder.dart';
import 'frame_spec.dart';
import 'frame_worker.dart';
import 'motion.dart';
import 'painter.dart';
import 'palette.dart';

/// Shows a [ParticleEncoder]'s animated code.
///
/// A [Ticker] drives the animation. The data frame index is derived from elapsed time
/// (`floor(elapsed * fps)`), never from the number of paints, so a 120 Hz display
/// repaints the same frame several times. Frames are fetched with the real wall clock
/// (`frame(i, unixSeconds: now)`) so the particles carry the live liveness value.
///
/// While mounted and [showing], the platform forces maximum screen brightness and keeps
/// the screen on (`particle_code/display`); both are restored when the widget is
/// disposed or [showing] turns `false`.
///
/// Frames are prefetched on a background isolate (see [useBackgroundIsolate]); the
/// particles' liveness second is the one at which each frame will be shown.
///
/// The widget is a square: with [size] it is `size x size` logical pixels, otherwise it
/// takes the largest square that fits the incoming constraints.
class ParticleCodeView extends StatefulWidget {
  /// Creates a view for [encoder].
  const ParticleCodeView({
    super.key,
    required this.encoder,
    this.motion = const MotionConfig(),
    this.size,
    this.background = ParticleCodePalette.background,
    this.showing = true,
    this.margin = defaultMargin,
    this.unixClock,
    this.useBackgroundIsolate = true,
  });

  /// The encoder to show. Not owned: the caller disposes it, after removing this view.
  final ParticleEncoder encoder;

  /// Animation layers; `null` shows a static, motionless code.
  final MotionConfig? motion;

  /// Side length in logical pixels, or `null` to fill the available square.
  final double? size;

  /// Background and quiet-zone color.
  final Color background;

  /// Whether the code is on screen. `false` stops the animation, restores brightness and
  /// paints an empty background. Use it while a dialog covers the code, for example.
  final bool showing;

  /// Quiet zone in pitches (at least 1.5).
  final double margin;

  /// Source of the wall clock in Unix seconds; defaults to `DateTime.now()`. Tests only.
  final int Function()? unixClock;

  /// Build frames on a background isolate (default). The isolate has its own encoder
  /// with the same identity, runs ahead of the data clock and hands over compact typed
  /// arrays, so the UI isolate never does Reed-Solomon or JSON work after start-up. The
  /// first frames, and any frame the worker is badly late with, are built synchronously;
  /// if the isolate cannot start, the view silently stays on the synchronous path.
  final bool useBackgroundIsolate;

  @override
  State<ParticleCodeView> createState() => _ParticleCodeViewState();
}

/// How many data frames the worker prefetches ahead of the clock.
const int _prefetchAhead = 2;

class _ParticleCodeViewState extends State<ParticleCodeView>
    with SingleTickerProviderStateMixin {
  late final Ticker _ticker;

  FrameSpec? _current;
  FrameSpec? _previous;
  int _index = 0;
  double _t = 0;
  int _elapsedMs = 0;
  bool _holdsDisplay = false;

  // Background generation.
  FrameWorker? _worker;
  bool _workerFailed = false;
  bool _workerDelivered = false;
  final Map<int, FrameSpec> _ready = {};
  final Set<int> _requested = {};
  int? _lateSinceMs;

  int get _fps => widget.encoder.fps < 1 ? 1 : widget.encoder.fps;

  /// Wall clock in milliseconds. An injected [ParticleCodeView.unixClock] counts whole
  /// seconds.
  int _nowMs() => widget.unixClock != null
      ? widget.unixClock!() * 1000
      : DateTime.now().millisecondsSinceEpoch;

  @override
  void initState() {
    super.initState();
    _ticker = createTicker(_onTick);
    _sync();
  }

  @override
  void didUpdateWidget(ParticleCodeView old) {
    super.didUpdateWidget(old);
    if (old.encoder != widget.encoder) {
      _workerFailed = false;
      _stop();
    } else if (old.useBackgroundIsolate != widget.useBackgroundIsolate) {
      _stopWorker();
    }
    _sync();
  }

  @override
  void dispose() {
    _ticker.dispose();
    _stopWorker();
    _releaseDisplay();
    super.dispose();
  }

  /// Brings ticker, frames and brightness in line with the widget's configuration.
  void _sync() {
    if (widget.showing) {
      if (_current == null) {
        _t = 0;
        _elapsedMs = 0;
        _index = 0;
        _previous = null;
        _current = _fetchSync(0);
      }
      _startWorker();
      if (!_ticker.isActive) {
        // Restarting resets `elapsed`, so the clock restarts with the frame index.
        _ticker.start();
      }
      _acquireDisplay();
    } else {
      _stop();
    }
  }

  void _stop() {
    if (_ticker.isActive) _ticker.stop();
    _current = null;
    _previous = null;
    _stopWorker();
    _releaseDisplay();
  }

  void _startWorker() {
    if (_worker != null || _workerFailed || !widget.useBackgroundIsolate) {
      return;
    }
    final spec = widget.encoder.workerSpec;
    if (spec == null) return;
    _worker = FrameWorker.start(
      spec,
      onFrame: _onWorkerFrame,
      onFailure: _onWorkerFailure,
    );
  }

  void _stopWorker() {
    _worker?.dispose();
    _worker = null;
    _workerDelivered = false;
    _ready.clear();
    _requested.clear();
    _lateSinceMs = null;
  }

  void _onWorkerFrame(int index, FrameSpec frame) {
    _requested.remove(index);
    _workerDelivered = true;
    if (index > _index) _ready[index] = frame;
  }

  void _onWorkerFailure(Object error) {
    debugPrint(
      'particle_code: background frame generation failed ($error); '
      'building frames on the UI isolate instead',
    );
    _workerFailed = true;
    _stopWorker();
  }

  void _acquireDisplay() {
    if (_holdsDisplay) return;
    _holdsDisplay = true;
    ParticleCodeDisplay.acquire();
  }

  void _releaseDisplay() {
    if (!_holdsDisplay) return;
    _holdsDisplay = false;
    ParticleCodeDisplay.release();
  }

  /// Builds frame [index] on this isolate, for the current wall-clock second.
  FrameSpec _fetchSync(int index) =>
      widget.encoder.frame(index, unixSeconds: _nowMs() ~/ 1000);

  /// The wall-clock second at which frame [index] will be on screen, predicted from the
  /// data clock: `now + (start of frame - elapsed)`.
  int _predictedUnix(int index) {
    final startMs = index * 1000 ~/ _fps;
    return (_nowMs() + startMs - _elapsedMs) ~/ 1000;
  }

  /// Keeps `[clockIndex, clockIndex + _prefetchAhead]` requested from the worker.
  void _prefetch(int clockIndex) {
    final worker = _worker;
    if (worker == null || !worker.isReady) return;
    _requested.removeWhere((i) => i < clockIndex);
    for (var i = clockIndex; i <= clockIndex + _prefetchAhead; i++) {
      if (i == _index || _ready.containsKey(i) || _requested.contains(i)) {
        continue;
      }
      _requested.add(i);
      worker.request(i, _predictedUnix(i), floor: clockIndex);
    }
  }

  void _onTick(Duration elapsed) {
    if (widget.encoder.isDisposed) {
      _ticker.stop();
      return;
    }
    final micros = elapsed.inMicroseconds;
    _elapsedMs = micros ~/ 1000;
    final clockIndex = (micros * _fps) ~/ 1000000;

    FrameSpec? next;
    var nextIndex = _index;
    if (clockIndex != _index || _current == null) {
      // The newest prefetched frame that is due.
      for (final i in _ready.keys) {
        if (i <= clockIndex && i > nextIndex) nextIndex = i;
      }
      if (nextIndex != _index) {
        next = _ready[nextIndex];
        _lateSinceMs = null;
      } else if (_worker != null && _workerDelivered && _worker!.isReady) {
        // The worker is running but late with this frame: keep showing the current
        // one, up to a limit, instead of stalling the UI with a synchronous build.
        _lateSinceMs ??= _elapsedMs;
        final limit = math.max(250, 3000 ~/ _fps);
        if (_elapsedMs - _lateSinceMs! > limit) {
          nextIndex = clockIndex;
          next = _fetchSync(clockIndex);
          _lateSinceMs = null;
        }
      } else {
        nextIndex = clockIndex;
        next = _fetchSync(clockIndex);
      }
    }
    _ready.removeWhere((i, _) => i <= nextIndex);
    if (next != null) {
      _previous = _current;
      _current = next;
      _index = nextIndex;
    }
    _prefetch(clockIndex);
    setState(() => _t = micros / 1e6);
  }

  @override
  Widget build(BuildContext context) {
    final frame = _current;
    final Widget child;
    if (!widget.showing || frame == null) {
      child = ColoredBox(color: widget.background);
    } else {
      final m = widget.motion;
      final tSinceSwitch = _t - _index / _fps;
      final prev = _previous;
      final segmentChanged =
          prev != null && prev.header.segmentIndex != frame.header.segmentIndex;
      child = CustomPaint(
        painter: ParticleCodePainter(
          frame: frame,
          prev: prev,
          motion: m,
          seed: seedFromSession(widget.encoder.sessionId),
          t: _t,
          tSinceSwitch: prev == null ? double.infinity : tSinceSwitch,
          tSinceSegmentComplete: segmentChanged ? tSinceSwitch : null,
          tSinceStart: (m?.reveal ?? false) ? _t : null,
          background: widget.background,
          margin: widget.margin,
        ),
        isComplex: true,
        willChange: true,
      );
    }
    final boxed = RepaintBoundary(child: SizedBox.expand(child: child));
    final size = widget.size;
    if (size != null) {
      return SizedBox.square(dimension: size, child: boxed);
    }
    return AspectRatio(aspectRatio: 1, child: boxed);
  }

  @override
  void debugFillProperties(DiagnosticPropertiesBuilder properties) {
    super.debugFillProperties(properties);
    properties.add(IntProperty('frameIndex', _index));
  }
}
