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

  /// `P-xxxx`: a fresh pair id. It is also the code's 16-bit session id (see
  /// [sessionIdOf]), which every frame header carries, so a receiver learns it from the
  /// code and the two phones' trials pair up without anyone typing it.
  static String newPairId(Random random) => pairIdOf(random.nextInt(0x10000));

  /// The pair id of a code whose header carries [sessionId].
  static String pairIdOf(int sessionId) =>
      'P-${(sessionId & 0xFFFF).toRadixString(16).padLeft(4, '0')}';

  /// The session id a `P-xxxx` pair id stands for, or `null` if [pairId] is not one.
  static int? sessionIdOf(String pairId) {
    final m = RegExp(r'^P-([0-9a-fA-F]{4})$').firstMatch(pairId.trim());
    return m == null ? null : int.parse(m.group(1)!, radix: 16);
  }

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
