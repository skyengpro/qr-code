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

/// The public hint code for the decoder's [HintDto] (`FrameResultDto.hintCode`).
ScanHint scanHintFor(HintDto code) => switch (code) {
  HintDto.none => ScanHint.none,
  HintDto.pointAtCode => ScanHint.pointAtCode,
  HintDto.moveBack => ScanHint.moveBack,
  HintDto.holdSteady => ScanHint.holdSteady,
  HintDto.lighting => ScanHint.lighting,
  HintDto.updateApp => ScanHint.updateApp,
};
