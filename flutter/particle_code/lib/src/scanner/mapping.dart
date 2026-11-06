import 'dart:ui';

import '../rust/api/dto.dart';
import 'events.dart';

/// Maps one point of the camera buffer (image pixels) through the frame's `toPreview`
/// affine `[a, b, c, d, e, f]` (`x' = a*x + b*y + c`, `y' = d*x + e*y + f`) to the preview,
/// normalized 0..1. Points the aspect-fill crop cut off land outside 0..1.
Offset mapToPreview(double x, double y, List<double> m) =>
    Offset(m[0] * x + m[1] * y + m[2], m[3] * x + m[4] * y + m[5]);

/// Maps the decoder's anchors (image pixels) to preview coordinates.
List<Offset> mapAnchorsToPreview(List<ImagePointDto> anchors, List<double> m) =>
    List.unmodifiable([for (final a in anchors) mapToPreview(a.x, a.y, m)]);

/// The hint code for the decoder's advice text (`FrameResultDto.hint`).
///
/// `null` is [ScanHint.none]; a text this version does not know is [ScanHint.other].
ScanHint hintCodeFor(String? hint) {
  if (hint == null || hint.isEmpty) return ScanHint.none;
  final h = hint.toLowerCase();
  if (h.startsWith('point the camera')) return ScanHint.pointAtCode;
  if (h.startsWith('move back')) return ScanHint.moveBack;
  if (h.startsWith('hold ')) return ScanHint.holdSteady;
  if (h.startsWith('too dark')) return ScanHint.lighting;
  if (h.contains('newer version')) return ScanHint.updateApp;
  return ScanHint.other;
}
