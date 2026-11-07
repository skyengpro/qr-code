import 'package:particle_code_hardware_spike/model/frame_record.dart';
import 'package:particle_code_hardware_spike/model/measurements.dart';
import 'package:particle_code_hardware_spike/model/observed.dart';
import 'package:particle_code_hardware_spike/model/stat.dart';
import 'package:particle_code_hardware_spike/model/trial.dart';
import 'package:particle_code_hardware_spike/model/trial_config.dart';

import 'fakes.dart';

/// A terminal receiver trial with the given outcome.
Trial receiverTrial(
  String id,
  TrialStatus status, {
  double? completionMs,
  Integrity? integrity,
  String reason = 'x',
  String model = 'Pixel 9 Pro XL',
  Conditions conditions = const Conditions(),
  bool withFrames = false,
}) {
  final ok = integrity == Integrity.ok && completionMs != null;
  final base = createdTrial(
    id: id,
    device: fakeDevice(model: model),
    conditions: conditions,
    annotations: Annotations(
      senderHardware: Observed<String>.manual('iPhone 13'),
      variant: Observed<String>.manual('12/256'),
    ),
  );
  return base.copyWith(
    status: status,
    reason: reason,
    startedAtUtc: DateTime.utc(2026, 1, 1, 12, 0, 1),
    endedAtUtc: DateTime.utc(2026, 1, 1, 12, 0, 5),
    durationMs: completionMs ?? 7000,
    receiver: ReceiverMeasurements(
      diagnosticsReceived: true,
      cameraStartupMs: const Observed<double>.measured(310.5),
      cameraId: const Observed<String>.measured('0'),
      firstAcceptedPacketMs: Observed<double>.measuredOrUnknown(
        completionMs == null ? null : completionMs / 2,
      ),
      completionMs: Observed<double>.measuredOrUnknown(
        ok ? completionMs : null,
      ),
      withinTwoSeconds: ok ? completionMs <= 2000 : null,
      integrity: integrity,
      have: 5,
      need: 9,
      counters: const FrameCounters(
        frameEvents: 12,
        decoded: 10,
        skippedBusy: 2,
        skippedBeforeTotal: 3,
        acceptedPackets: 6,
        distinctPackets: 5,
      ),
      timings: {
        'bridge': Stat.of([8.0, 9.0]),
      },
      rejectsByCategory: const {'noLock': 4},
      framesTotal: withFrames ? 2 : 0,
      frames: withFrames
          ? const [
              FrameRecord(
                index: 0,
                nativeFrameId: 1,
                disposition: 'decoded',
                sinceStartMs: 33.5,
                captureGapMs: 33.25,
                bridgeMs: 8,
                locked: true,
                segmentIndex: 0,
                packetSeed: 7,
                distinct: true,
              ),
              FrameRecord(
                index: 1,
                nativeFrameId: 2,
                disposition: 'skippedBusy',
                sinceStartMs: 66,
                skippedBefore: 1,
              ),
            ]
          : const [],
    ),
  );
}

/// A finished sender session.
Trial senderTrial(String id) =>
    createdTrial(
      id: id,
      config: testConfig(role: TrialRole.sender),
    ).copyWith(
      status: TrialStatus.completed,
      reason: TrialReason.senderSessionDone,
      startedAtUtc: DateTime.utc(2026, 1, 1, 12),
      endedAtUtc: DateTime.utc(2026, 1, 1, 12, 1),
      durationMs: 60000,
      sender: const SenderMeasurements(
        logicalWidth: Observed<double>.measured(412),
        devicePixelRatio: Observed<double>.measured(2.625),
      ),
    );
