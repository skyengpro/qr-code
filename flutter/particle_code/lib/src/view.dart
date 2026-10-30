import 'package:flutter/foundation.dart';
import 'package:flutter/scheduler.dart';
import 'package:flutter/widgets.dart';

import 'display.dart';
import 'encoder.dart';
import 'frame_spec.dart';
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

  @override
  State<ParticleCodeView> createState() => _ParticleCodeViewState();
}

class _ParticleCodeViewState extends State<ParticleCodeView>
    with SingleTickerProviderStateMixin {
  late final Ticker _ticker;

  FrameSpec? _current;
  FrameSpec? _previous;
  int _index = 0;
  double _t = 0;
  bool _holdsDisplay = false;

  int get _fps => widget.encoder.fps < 1 ? 1 : widget.encoder.fps;

  int _now() =>
      widget.unixClock?.call() ?? DateTime.now().millisecondsSinceEpoch ~/ 1000;

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
      _stop();
    }
    _sync();
  }

  @override
  void dispose() {
    _ticker.dispose();
    _releaseDisplay();
    super.dispose();
  }

  /// Brings ticker, frames and brightness in line with the widget's configuration.
  void _sync() {
    if (widget.showing) {
      if (_current == null) {
        _t = 0;
        _index = 0;
        _previous = null;
        _current = _fetch(0);
      }
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
    _releaseDisplay();
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

  FrameSpec _fetch(int index) =>
      widget.encoder.frame(index, unixSeconds: _now());

  void _onTick(Duration elapsed) {
    if (widget.encoder.isDisposed) {
      _ticker.stop();
      return;
    }
    final micros = elapsed.inMicroseconds;
    final index = (micros * _fps) ~/ 1000000;
    FrameSpec? next;
    if (index != _index || _current == null) {
      next = _fetch(index);
    }
    setState(() {
      if (next != null) {
        _previous = _current;
        _current = next;
        _index = index;
      }
      _t = micros / 1e6;
    });
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
