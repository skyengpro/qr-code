import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import 'events.dart';
import 'progress_ring.dart';
import 'scanner.dart';

/// Platform view type of the native camera preview (`PROTOCOL.md`).
const String previewViewType = 'particle_code/preview';

/// Builds an overlay on top of the camera preview from the latest [ScanFrame]
/// (`null` before the first event).
typedef ScanOverlayBuilder =
    Widget Function(BuildContext context, ScanFrame? frame);

/// The native camera preview of a running [ParticleScanner] session.
///
/// Create it before or after [ParticleScanner.start]; it attaches when the session
/// starts. Pass the [scanner] to get an overlay: by default the lock anchors drawn over
/// the code plus a [ScanProgressRing]; supply [overlayBuilder] to draw your own. Without
/// a scanner only the preview is shown.
///
/// Supported on Android ([AndroidView]) and iOS ([UiKitView]); elsewhere a placeholder
/// is shown.
class ParticleScannerPreview extends StatelessWidget {
  /// Creates a preview.
  const ParticleScannerPreview({
    super.key,
    this.scanner,
    this.overlayBuilder,
    this.placeholder,
  });

  /// The scanner whose events feed the overlay.
  final ParticleScanner? scanner;

  /// Custom overlay. Defaults to [ScanOverlay] when [scanner] is set.
  final ScanOverlayBuilder? overlayBuilder;

  /// Shown instead of the preview on unsupported platforms.
  final Widget? placeholder;

  @override
  Widget build(BuildContext context) {
    final preview = _buildPlatformView();
    final scanner = this.scanner;
    if (scanner == null) return preview;
    return Stack(
      fit: StackFit.expand,
      children: [
        preview,
        _FrameListener(
          scanner: scanner,
          builder:
              overlayBuilder ?? (context, frame) => ScanOverlay(frame: frame),
        ),
      ],
    );
  }

  Widget _buildPlatformView() {
    if (!kIsWeb) {
      switch (defaultTargetPlatform) {
        case TargetPlatform.android:
          return const AndroidView(
            viewType: previewViewType,
            creationParams: null,
          );
        case TargetPlatform.iOS:
          return const UiKitView(
            viewType: previewViewType,
            creationParams: null,
            creationParamsCodec: StandardMessageCodec(),
          );
        default:
          break;
      }
    }
    return placeholder ??
        const ColoredBox(
          color: Color(0xFF202020),
          child: Center(
            child: Text(
              'Camera preview is not available on this platform',
              textAlign: TextAlign.center,
              textDirection: TextDirection.ltr,
              style: TextStyle(color: Color(0xFFBDBDBD)),
            ),
          ),
        );
  }
}

class _FrameListener extends StatefulWidget {
  const _FrameListener({required this.scanner, required this.builder});

  final ParticleScanner scanner;
  final ScanOverlayBuilder builder;

  @override
  State<_FrameListener> createState() => _FrameListenerState();
}

class _FrameListenerState extends State<_FrameListener> {
  ScanFrame? _frame;
  StreamSubscription<ScanEvent>? _sub;

  @override
  void initState() {
    super.initState();
    _listen();
  }

  @override
  void didUpdateWidget(_FrameListener old) {
    super.didUpdateWidget(old);
    if (old.scanner != widget.scanner) {
      _sub?.cancel().ignore();
      _frame = null;
      _listen();
    }
  }

  void _listen() {
    _sub = widget.scanner.events.listen((event) {
      if (event is ScanFrame && mounted) {
        setState(() => _frame = event);
      }
    });
  }

  @override
  void dispose() {
    _sub?.cancel().ignore();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => widget.builder(context, _frame);
}

/// The default scan overlay: lock anchors over the preview plus a progress ring.
class ScanOverlay extends StatelessWidget {
  /// Creates an overlay for the latest [frame].
  const ScanOverlay({
    super.key,
    required this.frame,
    this.anchorColor = const Color(0xFF28A51E),
    this.lostColor = const Color(0xFFEB6400),
    this.showProgress = true,
  });

  /// The latest frame event, or `null` before the first one.
  final ScanFrame? frame;

  /// Color of the anchors while locked.
  final Color anchorColor;

  /// Color of the anchors while the lock is lost.
  final Color lostColor;

  /// Whether to draw the progress ring.
  final bool showProgress;

  @override
  Widget build(BuildContext context) {
    final f = frame;
    return Stack(
      fit: StackFit.expand,
      children: [
        if (f != null && f.anchors.isNotEmpty)
          IgnorePointer(
            child: CustomPaint(
              painter: ScanAnchorsPainter(
                anchors: f.anchors,
                color: f.locked ? anchorColor : lostColor,
              ),
            ),
          ),
        if (showProgress && f != null && f.need > 0)
          Positioned(
            right: 16,
            bottom: 16,
            child: ScanProgressRing.fromFrame(f),
          ),
      ],
    );
  }
}

/// Draws the lock anchors. Anchor coordinates are normalized `0..1` in preview
/// coordinates and are scaled to the widget size here.
class ScanAnchorsPainter extends CustomPainter {
  /// Creates a painter.
  const ScanAnchorsPainter({
    required this.anchors,
    this.color = const Color(0xFF28A51E),
    this.radius = 10,
  });

  /// Anchor points (3 or 4), normalized 0..1.
  final List<Offset> anchors;

  /// Stroke color.
  final Color color;

  /// Marker radius in logical pixels.
  final double radius;

  /// Maps a normalized anchor to widget coordinates.
  static Offset toWidget(Offset normalized, Size size) =>
      Offset(normalized.dx * size.width, normalized.dy * size.height);

  @override
  void paint(Canvas canvas, Size size) {
    if (anchors.isEmpty) return;
    final points = [for (final a in anchors) toWidget(a, size)];
    final stroke = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = 3
      ..color = color
      ..isAntiAlias = true;
    if (points.length >= 3) {
      final outline = Path()..moveTo(points[0].dx, points[0].dy);
      for (var i = 1; i < 3; i++) {
        outline.lineTo(points[i].dx, points[i].dy);
      }
      outline.close();
      canvas.drawPath(outline, stroke..color = color.withAlpha(140));
    }
    stroke.color = color;
    for (final p in points) {
      canvas.drawCircle(p, radius, stroke);
    }
  }

  @override
  bool shouldRepaint(ScanAnchorsPainter old) =>
      !listEquals(old.anchors, anchors) ||
      old.color != color ||
      old.radius != radius;
}
