import 'dart:math';

import '../model/device_identity.dart';
import '../model/trial.dart';
import '../model/trial_config.dart';

/// Creates trial ids and fresh trial records.
abstract final class TrialFactory {
  /// `T-yyyyMMdd-HHmmss-xxxx` from a local time and 16 random bits as hex.
  static String newId(DateTime local, Random random) {
    String two(int v) => v.toString().padLeft(2, '0');
    final date =
        '${local.year.toString().padLeft(4, '0')}${two(local.month)}'
        '${two(local.day)}';
    final time = '${two(local.hour)}${two(local.minute)}${two(local.second)}';
    final suffix = random.nextInt(0x10000).toRadixString(16).padLeft(4, '0');
    return 'T-$date-$time-$suffix';
  }

  /// `P-xxxx`: a suggested pair id.
  static String newPairId(Random random) =>
      'P-${random.nextInt(0x10000).toRadixString(16).padLeft(4, '0')}';

  /// A trial in [TrialStatus.created].
  static Trial create({
    required String trialId,
    required TrialConfig config,
    required Conditions conditions,
    required Annotations annotations,
    required DeviceIdentity device,
    required DateTime nowUtc,
  }) => Trial(
    trialId: trialId,
    config: config,
    conditions: conditions,
    annotations: annotations,
    device: device,
    status: TrialStatus.created,
    createdAtUtc: nowUtc,
  );
}
