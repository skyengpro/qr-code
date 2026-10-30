import 'dart:math' as math;

import 'package:flutter/widgets.dart';

import 'events.dart';

/// A circular progress indicator for a scan.
///
/// The outer ring is split into one arc per segment (a continuous track when there are
/// more than [maxSegmentArcs]); finished segments are solid. The thinner inner ring
/// shows the fraction of needed fountain symbols held ([have] of [need]).
class ScanProgressRing extends StatelessWidget {
  /// Creates a ring.
  const ScanProgressRing({
    super.key,
    required this.have,
    required this.need,
    this.segmentsDone = 0,
    this.segmentsTotal = 0,
    this.size = 72,
    this.strokeWidth = 6,
    this.color = const Color(0xFF28A51E),
    this.trackColor = const Color(0x33FFFFFF),
    this.child,
  });

  /// Ring for the latest [frame] (empty ring if `null`).
  ScanProgressRing.fromFrame(
    ScanFrame? frame, {
    super.key,
    this.size = 72,
    this.strokeWidth = 6,
    this.color = const Color(0xFF28A51E),
    this.trackColor = const Color(0x33FFFFFF),
    this.child,
  }) : have = frame?.have ?? 0,
       need = frame?.need ?? 0,
       segmentsDone = frame?.segmentsDone ?? 0,
       segmentsTotal = frame?.segmentsTotal ?? 0;

  /// Largest segment count drawn as separate arcs.
  static const int maxSegmentArcs = 24;

  /// Fountain symbols held.
  final int have;

  /// Fountain symbols needed.
  final int need;

  /// Segments decoded and verified.
  final int segmentsDone;

  /// Segments in the transfer.
  final int segmentsTotal;

  /// Side length in logical pixels.
  final double size;

  /// Stroke width of the outer ring; the inner ring is half as thick.
  final double strokeWidth;

  /// Color of the filled parts.
  final Color color;

  /// Color of the unfilled track.
  final Color trackColor;

  /// Widget in the middle, for example a percentage.
  final Widget? child;

  @override
  Widget build(BuildContext context) {
    return SizedBox.square(
      dimension: size,
      child: CustomPaint(
        painter: ScanProgressRingPainter(
          have: have,
          need: need,
          segmentsDone: segmentsDone,
          segmentsTotal: segmentsTotal,
          strokeWidth: strokeWidth,
          color: color,
          trackColor: trackColor,
        ),
        child: child == null ? null : Center(child: child),
      ),
    );
  }
}

/// Painter of [ScanProgressRing].
class ScanProgressRingPainter extends CustomPainter {
  /// Creates a painter.
  const ScanProgressRingPainter({
    required this.have,
    required this.need,
    required this.segmentsDone,
    required this.segmentsTotal,
    required this.strokeWidth,
    required this.color,
    required this.trackColor,
  });

  /// Fountain symbols held.
  final int have;

  /// Fountain symbols needed.
  final int need;

  /// Segments decoded and verified.
  final int segmentsDone;

  /// Segments in the transfer.
  final int segmentsTotal;

  /// Stroke width of the outer ring.
  final double strokeWidth;

  /// Color of the filled parts.
  final Color color;

  /// Color of the unfilled track.
  final Color trackColor;

  @override
  void paint(Canvas canvas, Size size) {
    final side = math.min(size.width, size.height);
    final center = size.center(Offset.zero);
    final outerRect = Rect.fromCircle(
      center: center,
      radius: side / 2 - strokeWidth / 2,
    );
    final innerRect = Rect.fromCircle(
      center: center,
      radius: side / 2 - strokeWidth * 1.75,
    );
    final track = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = strokeWidth
      ..color = trackColor
      ..isAntiAlias = true;
    final fill = Paint()
      ..style = PaintingStyle.stroke
      ..strokeWidth = strokeWidth
      ..strokeCap = StrokeCap.butt
      ..color = color
      ..isAntiAlias = true;
    const start = -math.pi / 2;
    const full = 2 * math.pi;

    final total = segmentsTotal;
    if (total > 0 && total <= ScanProgressRing.maxSegmentArcs) {
      final gap = total == 1 ? 0.0 : 0.06;
      final arc = full / total - gap;
      for (var i = 0; i < total; i++) {
        final a = start + i * full / total + gap / 2;
        canvas.drawArc(
          outerRect,
          a,
          arc,
          false,
          i < segmentsDone ? fill : track,
        );
      }
    } else {
      canvas.drawArc(outerRect, start, full, false, track);
      if (total > 0) {
        final f = (segmentsDone / total).clamp(0.0, 1.0);
        canvas.drawArc(outerRect, start, full * f, false, fill);
      }
    }

    if (innerRect.width > 0) {
      final innerTrack = Paint.from(track)..strokeWidth = strokeWidth / 2;
      final innerFill = Paint.from(fill)..strokeWidth = strokeWidth / 2;
      canvas.drawArc(innerRect, start, full, false, innerTrack);
      if (need > 0) {
        final f = (have / need).clamp(0.0, 1.0);
        canvas.drawArc(innerRect, start, full * f, false, innerFill);
      }
    }
  }

  @override
  bool shouldRepaint(ScanProgressRingPainter old) =>
      old.have != have ||
      old.need != need ||
      old.segmentsDone != segmentsDone ||
      old.segmentsTotal != segmentsTotal ||
      old.strokeWidth != strokeWidth ||
      old.color != color ||
      old.trackColor != trackColor;
}
