import 'dart:async';
import 'dart:typed_data';

import 'package:particle_code/particle_code.dart';
import 'package:particle_code_hardware_spike/device_identity.dart';
import 'package:particle_code_hardware_spike/fixtures.dart';
import 'package:particle_code_hardware_spike/model/device_identity.dart';
import 'package:particle_code_hardware_spike/model/observed.dart';
import 'package:particle_code_hardware_spike/model/trial.dart';
import 'package:particle_code_hardware_spike/model/trial_config.dart';
import 'package:particle_code_hardware_spike/trial/clock.dart';
import 'package:particle_code_hardware_spike/trial/receiver_recorder.dart';
import 'package:particle_code_hardware_spike/trial/scanner_port.dart';
import 'package:particle_code_hardware_spike/trial/trial_factory.dart';

/// A scanner port driven by the test. Streams are synchronous so an `add` is observed
/// immediately.
class FakeScannerPort implements ScannerPort {
  final StreamController<ScanEvent> eventsController =
      StreamController<ScanEvent>.broadcast(sync: true);
  final StreamController<ScanDiagnostic> diagController =
      StreamController<ScanDiagnostic>.broadcast(sync: true);

  int startCalls = 0;
  int stopCalls = 0;
  bool permission = true;
  Object? startError;

  @override
  Stream<ScanEvent> get events => eventsController.stream;

  @override
  Stream<ScanDiagnostic> get diagnostics => diagController.stream;

  @override
  Future<bool> requestPermission() async => permission;

  @override
  Future<void> start() async {
    startCalls++;
    if (startError != null) throw startError!;
  }

  @override
  Future<void> stop() async => stopCalls++;

  @override
  void dispose() {}

  void emit(ScanEvent e) => eventsController.add(e);
  void diag(ScanDiagnostic d) => diagController.add(d);
}

/// A [ScanFrame] with sensible defaults.
ScanFrame scanFrame({int have = 0, int need = 10}) => ScanFrame(
  status: ScanStatus.accepted,
  locked: true,
  profile: CodeProfile.m,
  shapeId: 0,
  have: have,
  need: need,
  segmentsDone: 0,
  segmentsTotal: 1,
  liveness: null,
  anchors: const [],
  sharpness: 1,
  hint: null,
  hintCode: ScanHint.none,
  rejectReason: null,
);

/// A diagnostics frame record.
ScanFrameDiagnostics diagFrame({
  int id = 1,
  FrameDisposition disposition = FrameDisposition.decoded,
  int sinceStartMs = 0,
  NativeTimestamp? capture,
  NativeTimestamp? received,
  int skippedBefore = 0,
  int? bridgeMs,
  int? heldMs,
  bool accepted = false,
  bool distinct = true,
  RejectStage? rejectStage,
  String? rejectCategory,
  bool locked = true,
  bool done = false,
  int? have,
  int? need,
  CameraBufferInfo? buffer,
}) => ScanFrameDiagnostics(
  frameId: id,
  disposition: disposition,
  sinceStart: Duration(milliseconds: sinceStartMs),
  capture: capture,
  received: received,
  skippedBefore: skippedBefore,
  bridge: bridgeMs == null ? null : Duration(milliseconds: bridgeMs),
  held: heldMs == null ? null : Duration(milliseconds: heldMs),
  decode: disposition == FrameDisposition.decoded
      ? DecodeTimings(
          locate: const Duration(milliseconds: 2),
          read: const Duration(milliseconds: 3),
          total: const Duration(milliseconds: 6),
        )
      : null,
  locked: locked,
  rejectStage: rejectStage,
  rejectCategory: rejectCategory,
  packet: accepted
      ? PacketInfo(segmentIndex: 0, packetSeed: id, distinct: distinct)
      : null,
  have: have,
  need: need,
  done: done,
  buffer: buffer,
);

/// A fixed device identity.
DeviceIdentity fakeDevice({
  String model = 'Pixel 9 Pro XL',
  String os = '16',
}) => DeviceIdentity({
  'platform': const Observed<Object>.measured('android'),
  'manufacturer': const Observed<Object>.measured('Google'),
  'model_identifier': Observed<Object>.measured(model),
  'os_name': const Observed<Object>.measured('Android'),
  'os_version': Observed<Object>.measured(os),
  'app_version': const Observed<Object>.measured('0.1.0'),
  'app_build_number': const Observed<Object>.measured('1'),
  'source_revision': const Observed<Object>.measured('abc123'),
  'physical_ram_mb': const Observed<Object>.measured(12288),
});

class FakeIdentityProvider implements DeviceIdentityProvider {
  FakeIdentityProvider([DeviceIdentity? identity])
    : identity = identity ?? fakeDevice();
  final DeviceIdentity identity;
  @override
  Future<DeviceIdentity> load() async => identity;
}

TrialConfig testConfig({
  TrialRole role = TrialRole.receiver,
  Fixture fixture = Fixtures.f200,
  String profile = 'M',
  int fps = 10,
}) => TrialConfig(
  role: role,
  pairId: 'P-0001',
  fixtureId: fixture.id,
  payloadBytes: fixture.byteCount,
  profile: profile,
  shapeId: 0,
  fps: fps,
  isStatic: false,
  repairPerPass: 2,
  motion: MotionChoice.breathing,
  configSource: role == TrialRole.receiver
      ? ConfigSource.declared
      : ConfigSource.configured,
);

Trial createdTrial({
  String id = 'T-20260101-120000-0001',
  TrialConfig? config,
  DeviceIdentity? device,
  Conditions conditions = const Conditions(),
  Annotations annotations = const Annotations(),
}) => TrialFactory.create(
  trialId: id,
  config: config ?? testConfig(),
  conditions: conditions,
  annotations: annotations,
  device: device ?? fakeDevice(),
  nowUtc: DateTime.utc(2026, 1, 1, 12),
);

/// Everything needed to run a receiver trial against fakes.
class RecorderRig {
  RecorderRig({
    Fixture fixture = Fixtures.f200,
    int maxFrames = 3000,
    String id = 'T-20260101-120000-0001',
  }) : fixture = fixture {
    recorder = ReceiverRecorder(
      port: port,
      base: createdTrial(
        id: id,
        config: testConfig(fixture: fixture),
      ),
      fixture: fixture,
      clock: clock,
      wallClock: () => DateTime.utc(2026, 1, 1, 12).add(clock.now),
      maxFrames: maxFrames,
      settleDelay: Duration.zero,
    );
  }

  final Fixture fixture;
  final FakeScannerPort port = FakeScannerPort();
  final ManualClock clock = ManualClock();
  late final ReceiverRecorder recorder;

  Uint8List get payload => fixture.bytes;
}
