import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code/particle_code.dart';
import 'package:particle_code_hardware_spike/model/measurements.dart';
import 'package:particle_code_hardware_spike/model/trial.dart';
import 'package:particle_code_hardware_spike/trial/trial_machine.dart';

import 'support/fakes.dart';

const _android = NativeClock.androidRealtime;

NativeTimestamp _ts(int ms, [NativeClock clock = _android]) =>
    NativeTimestamp(ms * 1000000, clock);

void main() {
  group('trial lifecycle', () {
    test('created -> running -> completed with a verified payload', () async {
      final rig = RecorderRig();
      final r = rig.recorder;
      expect(r.status, TrialStatus.created);
      await r.start();
      expect(r.status, TrialStatus.running);
      expect(rig.port.startCalls, 1);
      rig.clock.advance(const Duration(milliseconds: 1500));
      rig.port.emit(ScanDone(rig.payload, 1));
      final trial = await r.finished;
      expect(trial.status, TrialStatus.completed);
      expect(trial.reason, TrialReason.verified);
      final m = trial.receiver!;
      expect(m.integrity, Integrity.ok);
      expect(m.completionMs.value, 1500);
      expect(m.withinTwoSeconds, isTrue);
      expect(trial.durationMs, 1500);
      expect(rig.port.stopCalls, 1);
    });

    test('exactly two seconds still counts as within two seconds', () async {
      final rig = RecorderRig();
      await rig.recorder.start();
      rig.clock.advance(const Duration(seconds: 2));
      rig.port.emit(ScanDone(rig.payload, 1));
      final t = await rig.recorder.finished;
      expect(t.receiver!.withinTwoSeconds, isTrue);
    });

    test(
      'no deadline: crossing 2 s keeps running and keeps progress',
      () async {
        final rig = RecorderRig();
        final r = rig.recorder;
        await r.start();
        rig.port.emit(scanFrame(have: 4, need: 10));
        rig.clock.advance(const Duration(seconds: 2, milliseconds: 1));
        rig.port.emit(scanFrame(have: 6, need: 10));
        rig.clock.advance(const Duration(seconds: 30));
        expect(r.status, TrialStatus.running);
        expect(r.have, 6);
        expect(r.need, 10);
        expect(rig.port.stopCalls, 0, reason: 'never stopped on time');
        expect(rig.port.startCalls, 1, reason: 'never restarted on time');
        rig.port.emit(ScanDone(rig.payload, 1));
        final t = await r.finished;
        expect(t.status, TrialStatus.completed);
        expect(t.receiver!.completionMs.value, 32001);
        expect(t.receiver!.withinTwoSeconds, isFalse);
        expect(t.metTwoSeconds, isFalse);
        expect(t.isVerifiedCompletion, isTrue);
      },
    );

    test(
      'payload differing from the fixture is an error, not a success',
      () async {
        final rig = RecorderRig();
        await rig.recorder.start();
        rig.clock.advance(const Duration(seconds: 1));
        final wrong = rig.payload..[0] ^= 0xFF;
        rig.port.emit(ScanDone(wrong, 1));
        final t = await rig.recorder.finished;
        expect(t.status, TrialStatus.error);
        expect(t.reason, TrialReason.integrityMismatch);
        expect(t.receiver!.integrity, Integrity.mismatch);
        expect(t.receiver!.completionMs.isUnknown, isTrue);
        expect(t.receiver!.withinTwoSeconds, isNull);
        expect(t.isVerifiedCompletion, isFalse);
      },
    );

    test(
      'cancel -> cancelled, stop -> incomplete, with progress kept',
      () async {
        final a = RecorderRig();
        await a.recorder.start();
        a.port.emit(scanFrame(have: 3, need: 10));
        await a.recorder.cancel();
        final cancelled = await a.recorder.finished;
        expect(cancelled.status, TrialStatus.cancelled);
        expect(cancelled.reason, TrialReason.cancelledByTester);

        final b = RecorderRig();
        await b.recorder.start();
        b.port.emit(scanFrame(have: 3, need: 10));
        b.clock.advance(const Duration(seconds: 9));
        await b.recorder.stopIncomplete();
        final incomplete = await b.recorder.finished;
        expect(incomplete.status, TrialStatus.incomplete);
        expect(incomplete.receiver!.have, 3);
        expect(incomplete.durationMs, 9000);
        expect(incomplete.receiver!.completionMs.isUnknown, isTrue);
      },
    );

    test('a native error event ends the trial as error', () async {
      final rig = RecorderRig();
      await rig.recorder.start();
      rig.port.emit(const ScanError('camera_error', 'boom'));
      final t = await rig.recorder.finished;
      expect(t.status, TrialStatus.error);
      expect(t.reason, TrialReason.scannerError);
      expect(t.reasonDetail, contains('camera_error'));
    });

    test('a failing start is an error and still produces a record', () async {
      final rig = RecorderRig();
      rig.port.startError = const ParticleScannerException(
        ParticleScannerError.noCamera,
        'no camera',
      );
      await rig.recorder.start();
      final t = await rig.recorder.finished;
      expect(t.status, TrialStatus.error);
      expect(t.reason, TrialReason.startFailed);
    });

    test(
      'events after the end are ignored; terminal states are final',
      () async {
        final rig = RecorderRig();
        await rig.recorder.start();
        rig.port.emit(ScanDone(rig.payload, 1));
        await rig.recorder.finished;
        rig.port.emit(const ScanError('camera_error', 'late'));
        rig.clock.advance(const Duration(seconds: 5));
        await rig.recorder.cancel();
        await rig.recorder.stopIncomplete();
        expect(rig.recorder.status, TrialStatus.completed);
      },
    );

    test('the state machine rejects illegal transitions', () {
      final m = TrialMachine();
      expect(() => m.complete('x'), throwsA(isA<IllegalTrialTransition>()));
      m.start();
      expect(m.start, throwsA(isA<IllegalTrialTransition>()));
      m.markIncomplete('x');
      expect(() => m.cancel('x'), throwsA(isA<IllegalTrialTransition>()));
      expect(() => m.fail('x'), throwsA(isA<IllegalTrialTransition>()));
    });
  });

  group('diagnostics mapping and timing', () {
    test('camera startup, first accepted packet and counters', () async {
      final rig = RecorderRig();
      await rig.recorder.start();
      rig.port.diag(
        const ScanCameraStarted(
          startup: Duration(milliseconds: 420),
          cameraId: '0',
          facing: 'back',
        ),
      );
      rig.clock.advance(const Duration(milliseconds: 500));
      rig.port.diag(
        diagFrame(
          id: 1,
          rejectStage: RejectStage.locate,
          rejectCategory: 'noLock',
          locked: false,
        ),
      );
      rig.clock.advance(const Duration(milliseconds: 300));
      rig.port.diag(diagFrame(id: 2, accepted: true, bridgeMs: 9, heldMs: 12));
      rig.clock.advance(const Duration(milliseconds: 100));
      rig.port.diag(diagFrame(id: 3, accepted: true, distinct: false));
      rig.port.diag(
        diagFrame(
          id: 4,
          disposition: FrameDisposition.skippedBusy,
          skippedBefore: 2,
        ),
      );
      rig.port.diag(
        diagFrame(
          id: 5,
          rejectStage: RejectStage.receiver,
          rejectCategory: 'packetCrc',
        ),
      );
      await rig.recorder.cancel();
      final m = (await rig.recorder.finished).receiver!;
      expect(m.diagnosticsReceived, isTrue);
      expect(m.cameraStartupMs.value, 420);
      expect(m.cameraId.value, '0');
      expect(m.cameraFacing.value, 'back');
      expect(m.firstAcceptedPacketMs.value, 800);
      expect(m.counters.frameEvents, 5);
      expect(m.counters.decoded, 4);
      expect(m.counters.skippedBusy, 1);
      expect(m.counters.skippedBeforeTotal, 2);
      expect(m.counters.pluginReceived, 7);
      expect(m.counters.acceptedPackets, 2);
      expect(m.counters.distinctPackets, 1);
      expect(m.rejectsByStage, {'locate': 1, 'receiver': 1});
      expect(m.rejectsByCategory, {'noLock': 1, 'packetCrc': 1});
      expect(m.timings['bridge']!.count, 1);
      expect(m.timings['bridge']!.p50, 9);
      expect(m.timings['held']!.p50, 12);
      expect(m.timings['decode_total']!.count, 4);
      expect(m.frames.length, 5);
      expect(m.frames[1].accepted, isTrue);
      expect(m.frames[0].rejectCategory, 'noLock');
      expect(m.toJson()['camera_pipeline_drops'], {
        'value': null,
        'source': 'unavailable',
      });
    });

    test('delivery gaps come only from the same native clock', () async {
      final rig = RecorderRig();
      await rig.recorder.start();
      rig.port.diag(diagFrame(id: 1, capture: _ts(1000)));
      rig.port.diag(diagFrame(id: 2, capture: _ts(1033)));
      // A different clock domain: must not be subtracted from the previous value.
      rig.port.diag(diagFrame(id: 3, capture: _ts(5, NativeClock.iosHost)));
      rig.port.diag(diagFrame(id: 4, capture: _ts(40, NativeClock.iosHost)));
      // An unknown clock never pairs, even with itself.
      rig.port.diag(diagFrame(id: 5, capture: _ts(100, NativeClock.unknown)));
      rig.port.diag(diagFrame(id: 6, capture: _ts(200, NativeClock.unknown)));
      await rig.recorder.cancel();
      final m = (await rig.recorder.finished).receiver!;
      expect(m.frames[0].captureGapMs, isNull);
      expect(m.frames[1].captureGapMs, 33);
      expect(m.frames[2].captureGapMs, isNull);
      expect(m.frames[3].captureGapMs, 35);
      expect(m.frames[4].captureGapMs, isNull);
      expect(m.frames[5].captureGapMs, isNull);
      expect(m.deliveryGapPairs, 2);
      expect(m.crossClockPairsIgnored, 3);
      expect(m.timings['delivery_gap']!.count, 2);
      expect(m.captureClock.value, 'android_realtime');
    });

    test(
      'receipt latency needs the same clock for capture and receipt',
      () async {
        final rig = RecorderRig();
        await rig.recorder.start();
        rig.port.diag(diagFrame(id: 1, capture: _ts(100), received: _ts(108)));
        rig.port.diag(
          diagFrame(
            id: 2,
            capture: _ts(200, NativeClock.androidUnknown),
            received: _ts(900),
          ),
        );
        await rig.recorder.cancel();
        final m = (await rig.recorder.finished).receiver!;
        expect(m.frames[0].receiptLatencyMs, 8);
        expect(m.frames[1].receiptLatencyMs, isNull);
      },
    );

    test('camera buffer is taken from the first frame that has one', () async {
      final rig = RecorderRig();
      await rig.recorder.start();
      rig.port.diag(diagFrame(id: 1));
      rig.port.diag(
        diagFrame(
          id: 2,
          buffer: const CameraBufferInfo(
            width: 1280,
            height: 720,
            format: 'YUV_420_888',
            yStride: 1280,
            uvStride: 1280,
            uvPixelStride: 2,
            rotationDegrees: 90,
            cameraId: '0',
          ),
        ),
      );
      await rig.recorder.cancel();
      final m = (await rig.recorder.finished).receiver!;
      expect(m.buffer!.width, 1280);
      expect(m.buffer!.format, 'YUV_420_888');
      expect(m.buffer!.uvPixelStride, 2);
      expect(m.cameraId.value, '0');
    });

    test('per-frame records are capped but counters are not', () async {
      final rig = RecorderRig(maxFrames: 3);
      await rig.recorder.start();
      for (var i = 1; i <= 10; i++) {
        rig.port.diag(diagFrame(id: i, accepted: true));
      }
      await rig.recorder.cancel();
      final m = (await rig.recorder.finished).receiver!;
      expect(m.frames.length, 3);
      expect(m.framesTotal, 10);
      expect(m.framesTruncated, isTrue);
      expect(m.counters.frameEvents, 10);
      expect(m.counters.acceptedPackets, 10);
    });

    test(
      'without diagnostics the trial says so instead of reporting zeros',
      () async {
        final rig = RecorderRig();
        await rig.recorder.start();
        rig.port.emit(ScanDone(rig.payload, 1));
        final m = (await rig.recorder.finished).receiver!;
        expect(m.diagnosticsReceived, isFalse);
        expect(m.cameraStartupMs.isUnknown, isTrue);
        expect(m.buffer, isNull);
      },
    );

    test('first accepted packet never exceeds the completion time', () async {
      final rig = RecorderRig();
      await rig.recorder.start();
      rig.clock.advance(const Duration(milliseconds: 700));
      rig.port.emit(ScanDone(rig.payload, 1));
      // The final frame's diagnostics arrive after the done event.
      rig.clock.advance(const Duration(milliseconds: 5));
      rig.port.diag(diagFrame(id: 9, accepted: true, done: true));
      final m = (await rig.recorder.finished).receiver!;
      expect(m.firstAcceptedPacketMs.value, lessThanOrEqualTo(700));
      expect(m.counters.frameEvents, 1);
    });
  });
}
