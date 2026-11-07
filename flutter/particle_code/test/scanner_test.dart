import 'dart:async';
import 'dart:ffi' hide Size;

import 'package:ffi/ffi.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code/particle_code.dart';
import 'package:particle_code/src/rust/api/dto.dart';
import 'package:particle_code/src/scanner/decoder.dart';
import 'package:particle_code/src/scanner/mapping.dart';

Map<Object?, Object?> _frameMap({Map<Object?, Object?> overrides = const {}}) =>
    {
      'type': 'frame',
      'status': 'accepted',
      'locked': true,
      'profile': 1,
      'shapeId': 28,
      'have': 12,
      'need': 40,
      'segmentsDone': 1,
      'segmentsTotal': 3,
      'liveness': {'consistent': true, 'seedLow': 0x34, 'rolling': 4242},
      'anchors': [0.5, 0.1, 0.8, 0.7, 0.2, 0.7, 0.5, 0.45],
      'sharpness': 3.25,
      'hint': 'Hold steady',
      'hintCode': 3,
      'rejectReason': null,
      ...overrides,
    };

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  group('event parsing', () {
    test('a full frame', () {
      final e = ScanEvent.fromMap(_frameMap())! as ScanFrame;
      expect(e.status, ScanStatus.accepted);
      expect(e.locked, isTrue);
      expect(e.profile, CodeProfile.m);
      expect(e.shapeId, 28);
      expect((e.have, e.need), (12, 40));
      expect((e.segmentsDone, e.segmentsTotal), (1, 3));
      expect(e.liveness!.consistent, isTrue);
      expect(e.liveness!.seedLow, 0x34);
      expect(e.liveness!.rolling, 4242);
      expect(e.anchors, const [
        Offset(0.5, 0.1),
        Offset(0.8, 0.7),
        Offset(0.2, 0.7),
        Offset(0.5, 0.45),
      ]);
      expect(e.sharpness, 3.25);
      expect(e.hint, 'Hold steady');
      expect(e.hintCode, ScanHint.holdSteady);
      expect(e.rejectReason, isNull);
      expect(e.progress, closeTo(0.3, 1e-12));
    });

    test('unknown header, no lock, rejected', () {
      final e =
          ScanEvent.fromMap(
                _frameMap(
                  overrides: {
                    'status': 'rejected',
                    'locked': false,
                    'profile': -1,
                    'shapeId': -1,
                    'liveness': null,
                    'anchors': <Object?>[],
                    'hint': null,
                    'hintCode': 0,
                    'rejectReason': 'no anchors',
                    'need': 0,
                  },
                ),
              )!
              as ScanFrame;
      expect(e.status, ScanStatus.rejected);
      expect(e.locked, isFalse);
      expect(e.profile, isNull);
      expect(e.shapeId, isNull);
      expect(e.liveness, isNull);
      expect(e.anchors, isEmpty);
      expect(e.hint, isNull);
      expect(e.hintCode, ScanHint.none);
      expect(e.rejectReason, 'no anchors');
      expect(e.progress, 0);
    });

    test('three anchors and ints given as doubles', () {
      final e =
          ScanEvent.fromMap(
                _frameMap(
                  overrides: {
                    'anchors': [0, 0, 1, 0, 0, 1],
                    'have': 3.0,
                  },
                ),
              )!
              as ScanFrame;
      expect(e.anchors.length, 3);
      expect(e.anchors[1], const Offset(1, 0));
      expect(e.have, 3);
    });

    test('status and hint codes', () {
      expect(ScanStatus.parse('done'), ScanStatus.done);
      expect(ScanStatus.parse('accepted'), ScanStatus.accepted);
      expect(ScanStatus.parse('rejected'), ScanStatus.rejected);
      expect(ScanStatus.parse('???'), ScanStatus.rejected);
      expect(ScanStatus.parse(null), ScanStatus.rejected);
      expect(
        [for (var i = 0; i < 7; i++) ScanHint.fromCode(i)],
        [
          ScanHint.none,
          ScanHint.pointAtCode,
          ScanHint.moveBack,
          ScanHint.holdSteady,
          ScanHint.lighting,
          ScanHint.updateApp,
          ScanHint.other,
        ],
      );
      expect(ScanHint.fromCode(99), ScanHint.other);
      expect(ScanHint.fromCode(-1), ScanHint.other);
    });

    test('profile codes', () {
      expect(CodeProfile.fromCode(0), CodeProfile.s);
      expect(CodeProfile.fromCode(1), CodeProfile.m);
      expect(CodeProfile.fromCode(2), CodeProfile.l);
      expect(CodeProfile.fromCode(-1), isNull);
      expect(CodeProfile.fromCode(7), isNull);
    });

    test('segment, done and error', () {
      final seg =
          ScanEvent.fromMap({
                'type': 'segment',
                'bytes': Uint8List.fromList([1, 2, 3]),
              })!
              as ScanSegment;
      expect(seg.bytes, [1, 2, 3]);

      final done =
          ScanEvent.fromMap({
                'type': 'done',
                'payload': Uint8List.fromList([9, 8]),
                'sessionId': 0x1234,
              })!
              as ScanDone;
      expect(done.payload, [9, 8]);
      expect(done.sessionId, 0x1234);

      final err =
          ScanEvent.fromMap({
                'type': 'error',
                'code': 'camera_error',
                'message': 'boom',
              })!
              as ScanError;
      expect(err.code, 'camera_error');
      expect(err.message, 'boom');
      expect(err.kind, ParticleScannerError.cameraError);
    });

    test('unknown types are ignored, malformed events throw', () {
      expect(ScanEvent.fromMap({'type': 'future-thing'}), isNull);
      expect(() => ScanEvent.fromMap('nope'), throwsFormatException);
      expect(
        () => ScanEvent.fromMap({'type': 'done', 'payload': 3, 'sessionId': 1}),
        throwsFormatException,
      );
      expect(
        () => ScanEvent.fromMap({'type': 'segment'}),
        throwsFormatException,
      );
    });
  });

  group('error mapping', () {
    test('codes', () {
      expect(
        ParticleScannerError.fromCode('permission_denied'),
        ParticleScannerError.permissionDenied,
      );
      expect(
        ParticleScannerError.fromCode('no_camera'),
        ParticleScannerError.noCamera,
      );
      expect(
        ParticleScannerError.fromCode('camera_error'),
        ParticleScannerError.cameraError,
      );
      expect(ParticleScannerError.fromCode('x'), ParticleScannerError.unknown);
      expect(ParticleScannerError.fromCode(null), ParticleScannerError.unknown);
    });

    test('PlatformException keeps code and message', () {
      final e = ParticleScannerException.fromPlatform(
        PlatformException(code: 'no_camera', message: 'none here'),
      );
      expect(e.error, ParticleScannerError.noCamera);
      expect(e.message, 'none here');
      expect(e.code, 'no_camera');
      expect(e.toString(), contains('noCamera'));
    });
  });

  group('mapping', () {
    test('anchors go through the toPreview affine', () {
      // x' = 2x + 3y + 5, y' = -x + 4y + 7
      const m = [2.0, 3.0, 5.0, -1.0, 4.0, 7.0];
      expect(mapToPreview(10, 20, m), const Offset(85, 77));
      expect(
        mapAnchorsToPreview(const [
          ImagePointDto(x: 0, y: 0),
          ImagePointDto(x: 1, y: 1),
        ], m),
        const [Offset(5, 7), Offset(10, 10)],
      );
      expect(mapAnchorsToPreview(const [], m), isEmpty);
    });

    test('a rotation with the aspect-fill crop can leave 0..1', () {
      // 90 degrees clockwise of a 640x480 buffer: x' = 1 - y/480, y' = x/640.
      const m = [0.0, -1 / 480, 1.0, 1 / 640, 0.0, 0.0];
      expect(mapToPreview(0, 0, m), const Offset(1, 0));
      expect(mapToPreview(640, 480, m), const Offset(0, 1));
      expect(mapToPreview(960, 0, m).dy, greaterThan(1));
    });

    test('decoder hint codes map to scan hints', () {
      expect(scanHintFor(HintDto.none), ScanHint.none);
      expect(scanHintFor(HintDto.pointAtCode), ScanHint.pointAtCode);
      expect(scanHintFor(HintDto.moveBack), ScanHint.moveBack);
      expect(scanHintFor(HintDto.holdSteady), ScanHint.holdSteady);
      expect(scanHintFor(HintDto.lighting), ScanHint.lighting);
      expect(scanHintFor(HintDto.updateApp), ScanHint.updateApp);
    });
  });

  group('method channel', () {
    late _Rig rig;
    late ParticleScanner scanner;

    setUp(() {
      rig = _Rig()..install();
      scanner = rig.scanner();
    });

    tearDown(() => scanner.dispose());

    test('requestPermission', () async {
      rig.handler = (_) => true;
      expect(await scanner.requestPermission(), isTrue);
      rig.handler = (_) => false;
      expect(await scanner.requestPermission(), isFalse);
      rig.handler = (_) => null;
      expect(await scanner.requestPermission(), isFalse);
      expect(rig.methods, everyElement('requestPermission'));
      expect(rig.calls.first.arguments, isNull);
    });

    test('start sends the torch flag', () async {
      await scanner.start();
      await scanner.start(torch: true);
      expect(rig.methods, ['start', 'start']);
      expect(rig.calls[0].arguments, {'torch': false});
      expect(rig.calls[1].arguments, {'torch': true});
      expect(rig.listens, 1); // one frames subscription for both
    });

    test('stop is idempotent', () async {
      await scanner.start();
      await scanner.stop();
      await scanner.stop();
      expect(rig.methods, ['start', 'stop', 'stop']);
    });

    test('setTorch returns whether a torch exists', () async {
      rig.handler = (_) => true;
      expect(await scanner.setTorch(true), isTrue);
      expect(rig.calls.single.arguments, {'on': true});
      rig.handler = (_) => false;
      expect(await scanner.setTorch(false), isFalse);
    });

    for (final code in ['permission_denied', 'no_camera', 'camera_error']) {
      test('start maps $code and stops listening for frames', () async {
        rig.handler = (_) =>
            throw PlatformException(code: code, message: 'm-$code');
        await expectLater(
          scanner.start(),
          throwsA(
            isA<ParticleScannerException>()
                .having(
                  (e) => e.error,
                  'error',
                  ParticleScannerError.fromCode(code),
                )
                .having((e) => e.message, 'message', 'm-$code')
                .having((e) => e.code, 'code', code),
          ),
        );
        await pumpEventQueue();
        expect(rig.cancels, 1);
      });
    }

    test('an unknown code becomes unknown', () async {
      rig.handler = (_) => throw PlatformException(code: 'weird');
      await expectLater(
        scanner.start(),
        throwsA(
          isA<ParticleScannerException>().having(
            (e) => e.error,
            'error',
            ParticleScannerError.unknown,
          ),
        ),
      );
    });

    test('no plugin registered is reported as unsupported', () async {
      rig.messenger.setMockMethodCallHandler(ParticleScanner.channel, null);
      await expectLater(
        scanner.start(),
        throwsA(
          isA<ParticleScannerException>().having(
            (e) => e.error,
            'error',
            ParticleScannerError.unsupported,
          ),
        ),
      );
    });

    test('dispose stops the camera, frees the decoder, forbids use', () async {
      await scanner.start();
      final decoder = rig.decoders.single;
      scanner.dispose();
      await pumpEventQueue();
      expect(rig.methods.last, 'stop');
      expect(decoder.disposed, isTrue);
      await expectLater(scanner.start(), throwsStateError);
      await expectLater(scanner.reset(), throwsStateError);
      scanner.dispose(); // idempotent
    });

    test('listening to events calls no method and starts no frames', () async {
      final sub = scanner.events.listen((_) {});
      await pumpEventQueue();
      expect(rig.calls, isEmpty);
      expect(rig.listens, 0);
      await sub.cancel();
    });
  });

  group('frames', () {
    late _Rig rig;
    late ParticleScanner scanner;
    late List<ScanEvent> got;
    late _Clock clock;

    setUp(() async {
      rig = _Rig()..install();
      clock = _Clock();
      scanner = rig.scanner(clock: clock.call);
      got = [];
      scanner.events.listen(got.add);
      await scanner.start();
      await pumpEventQueue();
    });

    tearDown(() => scanner.dispose());

    test(
      'a rejected frame is a ScanFrame with its hint, then released',
      () async {
        rig.next(
          _rejected(
            'no lock',
            hint: 'Point the camera at the code',
            hintCode: HintDto.pointAtCode,
          ),
        );
        rig.frame(7);
        await pumpEventQueue();
        final f = got.single as ScanFrame;
        expect(f.status, ScanStatus.rejected);
        expect(f.locked, isFalse);
        expect(f.hint, 'Point the camera at the code');
        expect(f.hintCode, ScanHint.pointAtCode);
        expect(f.rejectReason, 'no lock');
        expect(f.anchors, isEmpty);
        expect(rig.released, [7]);
        expect(rig.decoders.single.fed, hasLength(1));
        final fed = rig.decoders.single.fed.single;
        expect((fed.width, fed.height), (640, 480));
        expect((fed.yStride, fed.uvStride, fed.uvPixelStride), (640, 320, 2));
        expect(fed.yAddress, 1000);
        expect(fed.vLen, 2999);
      },
    );

    test('anchors are mapped through toPreview', () async {
      rig.next(
        _result(
          locked: true,
          anchors: const [
            ImagePointDto(x: 320, y: 0),
            ImagePointDto(x: 640, y: 480),
          ],
          progress: const ProgressDto.accepted(
            have: 1,
            need: 4,
            segmentsDone: 0,
            segmentsTotal: 1,
          ),
        ),
      );
      rig.frame(1, toPreview: [0, -1 / 480, 1, 1 / 640, 0, 0]);
      await pumpEventQueue();
      final f = got.single as ScanFrame;
      expect(f.locked, isTrue);
      expect(f.anchors, [const Offset(1, 0.5), const Offset(0, 1)]);
      expect(f.profile, CodeProfile.m);
      expect(f.shapeId, 28);
      expect(f.liveness!.seedLow, 9);
      expect(f.progress, 0.25);
    });

    test(
      'a frame arriving while one is in flight is released at once',
      () async {
        final gate = rig.decoders.single.gate = Completer<void>();
        rig.next(_rejected('x'));
        rig.frame(1);
        await pumpEventQueue();
        rig.frame(2);
        await pumpEventQueue();
        expect(rig.released, [2]);
        expect(rig.decoders.single.fed, hasLength(1));
        gate.complete();
        await pumpEventQueue();
        expect(rig.released, [2, 1]);
        expect(got, hasLength(1));
      },
    );

    test('a decoder failure still releases the frame', () async {
      rig.decoders.single.failWith = StateError('boom');
      rig.frame(3);
      await pumpEventQueue();
      expect(rig.released, [3]);
      expect(got, isEmpty);
      rig.decoders.single.failWith = null;
      rig.next(_rejected('x'));
      rig.frame(4);
      await pumpEventQueue();
      expect(rig.released, [3, 4]);
      expect(got, hasLength(1));
    });

    test(
      'stop waits for the frame in flight, releases it, then stops',
      () async {
        final gate = rig.decoders.single.gate = Completer<void>();
        rig.next(_rejected('x'));
        rig.frame(1);
        await pumpEventQueue();
        var stopped = false;
        final stop = scanner.stop().then((_) => stopped = true);
        await pumpEventQueue();
        expect(stopped, isFalse);
        expect(rig.methods, ['start']);
        rig.frame(2); // while stopping
        await pumpEventQueue();
        expect(rig.released, [2]);
        gate.complete();
        await stop;
        expect(rig.methods, ['start', 'releaseFrame', 'releaseFrame', 'stop']);
        expect(rig.released, [2, 1]);
        expect(rig.decoders.single.fed, hasLength(1));
        expect(rig.cancels, 1);
      },
    );

    test('frames after stop are released, never decoded', () async {
      await scanner.stop();
      expect(rig.cancels, 1);
      // Feed the handler directly as if the platform sent one late.
      rig.frame(5);
      await pumpEventQueue();
      expect(rig.decoders.single.fed, isEmpty);
    });

    test('have and need are sticky across rejected frames', () async {
      rig.next(
        _result(
          locked: true,
          progress: const ProgressDto.accepted(
            have: 5,
            need: 20,
            segmentsDone: 1,
            segmentsTotal: 3,
          ),
        ),
      );
      rig.next(_rejected('blur'));
      rig.frame(1);
      await pumpEventQueue();
      clock.advance(const Duration(seconds: 1));
      rig.frame(2);
      await pumpEventQueue();
      final frames = got.whereType<ScanFrame>().toList();
      expect(frames, hasLength(2));
      expect((frames[0].have, frames[0].need), (5, 20));
      expect(frames[1].status, ScanStatus.rejected);
      expect((frames[1].have, frames[1].need), (5, 20));
      expect((frames[1].segmentsDone, frames[1].segmentsTotal), (1, 3));
    });

    test(
      'frames are limited to about ten per second, progress is not',
      () async {
        for (var i = 0; i < 5; i++) {
          rig.next(_rejected('x'));
          rig.frame(i);
          await pumpEventQueue();
          clock.advance(const Duration(milliseconds: 30));
        }
        // t = 0 (emitted), 30, 60, 90 (dropped), 120 (emitted)
        expect(got.whereType<ScanFrame>(), hasLength(2));
        for (var i = 0; i < 3; i++) {
          rig.next(
            _result(
              locked: true,
              progress: ProgressDto.accepted(
                have: i + 1,
                need: 9,
                segmentsDone: 0,
                segmentsTotal: 1,
              ),
            ),
          );
          rig.frame(10 + i);
          await pumpEventQueue();
          clock.advance(const Duration(milliseconds: 1));
        }
        expect(got.whereType<ScanFrame>(), hasLength(5));
      },
    );

    test('segments follow accepted frames', () async {
      rig.decoders.single.ready = Uint8List.fromList([1, 2, 3]);
      rig.next(
        _result(
          locked: true,
          progress: const ProgressDto.accepted(
            have: 2,
            need: 9,
            segmentsDone: 1,
            segmentsTotal: 2,
          ),
        ),
      );
      rig.frame(1);
      await pumpEventQueue();
      expect(got.map((e) => e.runtimeType), [ScanFrame, ScanSegment]);
      expect((got[1] as ScanSegment).bytes, [1, 2, 3]);
    });

    test(
      'done: the payload once, then frames are released undecoded',
      () async {
        rig.decoders.single.session = 0x2A3B;
        rig.next(
          _result(
            locked: true,
            progress: const ProgressDto.accepted(
              have: 3,
              need: 6,
              segmentsDone: 0,
              segmentsTotal: 1,
            ),
          ),
        );
        rig.next(
          _result(
            locked: true,
            progress: ProgressDto.done(payload: Uint8List.fromList([9, 9, 9])),
          ),
        );
        rig.frame(1);
        await pumpEventQueue();
        rig.frame(2);
        await pumpEventQueue();
        rig.frame(3);
        rig.frame(4);
        await pumpEventQueue();
        expect(got.map((e) => e.runtimeType), [ScanFrame, ScanFrame, ScanDone]);
        final last = got.whereType<ScanFrame>().last;
        expect(last.status, ScanStatus.done);
        expect((last.have, last.need), (6, 6));
        expect((last.segmentsDone, last.segmentsTotal), (1, 1));
        final done = got.last as ScanDone;
        expect(done.payload, [9, 9, 9]);
        expect(done.sessionId, 0x2A3B);
        expect(rig.decoders.single.fed, hasLength(2));
        expect(rig.released, [1, 2, 3, 4]);
        expect(
          rig.methods,
          isNot(contains('stop')),
        ); // the camera keeps running
      },
    );

    test('reset: a new decoder, clean progress, no platform call', () async {
      rig.next(
        _result(
          locked: true,
          progress: ProgressDto.done(payload: Uint8List(0)),
        ),
      );
      rig.frame(1);
      await pumpEventQueue();
      expect(got.last, isA<ScanDone>());
      final before = rig.calls.length;
      final old = rig.decoders.single;
      await scanner.reset();
      expect(rig.calls.length, before);
      expect(rig.decoders, hasLength(2));
      expect(old.disposed, isTrue);
      expect(rig.decoders.last.disposed, isFalse);

      // Decoding resumes after a done.
      rig.next(_rejected('x'));
      rig.frame(2);
      await pumpEventQueue();
      final f = got.last as ScanFrame;
      expect((f.have, f.need), (0, 0));
      expect(rig.decoders.last.fed, hasLength(1));
      expect(old.fed, hasLength(1));
    });

    test(
      'reset during a decode drops its result and frees the old decoder later',
      () async {
        final old = rig.decoders.single;
        final gate = old.gate = Completer<void>();
        rig.next(_rejected('x'));
        rig.frame(1);
        await pumpEventQueue();
        await scanner.reset();
        expect(old.disposed, isFalse);
        gate.complete();
        await pumpEventQueue();
        expect(old.disposed, isTrue);
        expect(got, isEmpty);
        expect(rig.released, [1]);
      },
    );

    test('a payload nobody listened for reaches the next listener', () async {
      scanner.dispose();
      final s = rig.scanner(clock: clock.call);
      addTearDown(s.dispose);
      await s.start();
      rig.next(
        _result(
          locked: true,
          progress: ProgressDto.done(payload: Uint8List.fromList([4])),
        ),
      );
      rig.frame(1);
      await pumpEventQueue();
      final late = <ScanEvent>[];
      s.events.listen(late.add);
      await pumpEventQueue();
      expect((late.single as ScanDone).payload, [4]);
    });

    test('platform errors and malformed frames become ScanErrors', () async {
      rig.sink!.error(code: 'camera_error', message: 'lost');
      rig.sink!.success({
        'type': 'error',
        'code': 'camera_error',
        'message': 'dead',
      });
      rig.sink!.success({'type': 'unheard-of'});
      rig.sink!.success({'type': 'frame', 'id': 8, 'width': 'wide'});
      rig.sink!.success({'type': 'frame', 'width': 1});
      rig.sink!.success('nonsense');
      await pumpEventQueue();
      expect(got.map((e) => (e as ScanError).code), [
        'camera_error',
        'camera_error',
        'malformed_event',
        'malformed_event',
        'malformed_event',
      ]);
      expect((got[1] as ScanError).message, 'dead');
      expect(rig.released, [8]); // a frame with an id is always released
      expect(rig.decoders.single.fed, isEmpty);
    });
  });

  group('native clocks', () {
    test('since subtracts within one clock only', () {
      const a = NativeTimestamp(5000000, NativeClock.androidRealtime);
      const b = NativeTimestamp(2000000, NativeClock.androidRealtime);
      expect(a.since(b), const Duration(milliseconds: 3));
      expect(
        a.since(const NativeTimestamp(1, NativeClock.androidUnknown)),
        isNull,
      );
      expect(a.since(const NativeTimestamp(1, NativeClock.iosHost)), isNull);
      const u = NativeTimestamp(1, NativeClock.unknown);
      expect(u.since(u), isNull);
      expect(NativeClock.fromWire('ios_host'), NativeClock.iosHost);
      expect(NativeClock.fromWire(null), NativeClock.unknown);
    });
  });

  group('diagnostics', () {
    late _Rig rig;
    late _TestTime time;
    late ParticleScanner scanner;
    late List<ScanDiagnostic> diag;
    late List<ScanEvent> got;

    setUp(() {
      rig = _Rig()..install();
      time = _TestTime();
      scanner = rig.scanner(stopwatch: time.watch);
      diag = [];
      got = [];
      scanner.diagnostics.listen(diag.add);
      scanner.events.listen(got.add);
    });

    tearDown(() => scanner.dispose());

    const nativeFields = <String, Object?>{
      'format': 'YUV_420_888',
      'rotationDegrees': 90,
      'cameraId': '0',
      'timestampNanos': 9000000000,
      'timestampClock': 'android_realtime',
      'receivedNanos': 9004000000,
      'skippedBefore': 3,
      'droppedByOutputBefore': 2,
    };

    test('ScanCameraStarted once per start, with startup and camera', () async {
      rig.handler = (call) =>
          call.method == 'start' ? {'cameraId': '2', 'facing': 'back'} : null;
      final started = scanner.start();
      time.advance(const Duration(milliseconds: 350));
      rig.frame(1);
      await started;
      await pumpEventQueue();
      rig.next(_rejected('x'));
      rig.frame(2);
      await pumpEventQueue();
      final cameras = diag.whereType<ScanCameraStarted>().toList();
      expect(cameras, hasLength(1));
      expect(cameras.single.startup, const Duration(milliseconds: 350));
      expect(cameras.single.cameraId, '2');
      expect(cameras.single.facing, 'back');
      expect(diag.first, isA<ScanCameraStarted>());
      expect(diag.whereType<ScanFrameDiagnostics>(), hasLength(2));

      // A second start measures from its own call.
      await scanner.stop();
      time.advance(const Duration(seconds: 5));
      rig.handler = (call) =>
          call.method == 'start' ? {'cameraId': '3', 'facing': 'front'} : null;
      final again = scanner.start();
      time.advance(const Duration(milliseconds: 80));
      rig.frame(3);
      await again;
      await pumpEventQueue();
      final all = diag.whereType<ScanCameraStarted>().toList();
      expect(all, hasLength(2));
      expect(all.last.startup, const Duration(milliseconds: 80));
      expect((all.last.cameraId, all.last.facing), ('3', 'front'));
    });

    test('a null or odd start result gives unknown facing', () async {
      await scanner.start(); // the mock returns null
      rig.frame(1);
      await pumpEventQueue();
      final c = diag.whereType<ScanCameraStarted>().single;
      expect(c.cameraId, isNull);
      expect(c.facing, 'unknown');
    });

    test('a decoded frame carries every field', () async {
      await scanner.start();
      rig.handler = (call) =>
          call.method == 'releaseFrame' ? {'heldNanos': 12500000} : null;
      rig.next(
        _result(
          locked: true,
          progress: const ProgressDto.accepted(
            have: 5,
            need: 40,
            segmentsDone: 1,
            segmentsTotal: 3,
          ),
          packet: const PacketDto(
            segmentIndex: 2,
            packetSeed: 77,
            distinct: true,
          ),
        ),
      );
      time.advance(const Duration(milliseconds: 100));
      final gate = rig.decoders.single.gate = Completer<void>();
      rig.frame(9, extra: nativeFields);
      await pumpEventQueue();
      time.advance(const Duration(milliseconds: 42));
      gate.complete();
      await pumpEventQueue();
      final f = diag.whereType<ScanFrameDiagnostics>().single;
      expect(f.frameId, 9);
      expect(f.disposition, FrameDisposition.decoded);
      expect(f.sinceStart, const Duration(milliseconds: 100));
      expect(f.bridge, const Duration(milliseconds: 42)); // around feed only
      expect(f.held, const Duration(microseconds: 12500)); // from the release
      expect(f.skippedBefore, 3);
      expect(f.droppedByOutputBefore, 2);
      expect(f.buffer!.format, 'YUV_420_888');
      expect(f.buffer!.rotationDegrees, 90);
      expect(f.buffer!.cameraId, '0');
      expect((f.buffer!.width, f.buffer!.height), (640, 480));
      expect(f.capture!.clock, NativeClock.androidRealtime);
      expect(f.received!.since(f.capture!), const Duration(milliseconds: 4));
      expect(f.decode!.validate, const Duration(microseconds: 10));
      expect(f.decode!.locate, const Duration(milliseconds: 2));
      expect(f.decode!.read, const Duration(milliseconds: 3));
      expect(f.decode!.receive, const Duration(microseconds: 400));
      expect(f.decode!.total, const Duration(microseconds: 5500));
      expect(f.locked, isTrue);
      expect(f.rejectStage, isNull);
      expect(f.rejectCategory, isNull);
      expect((f.packet!.segmentIndex, f.packet!.packetSeed), (2, 77));
      expect(f.packet!.distinct, isTrue);
      expect((f.have, f.need), (5, 40));
      expect((f.segmentsDone, f.segmentsTotal), (1, 3));
      expect(f.done, isFalse);
    });

    test('capture and receipt clocks are not mixed', () async {
      await scanner.start();
      rig.frame(
        1,
        extra: {...nativeFields, 'timestampClock': 'android_unknown'},
      );
      await pumpEventQueue();
      final f = diag.whereType<ScanFrameDiagnostics>().single;
      expect(f.capture!.clock, NativeClock.androidUnknown);
      expect(f.received!.since(f.capture!), isNull);
    });

    test('absent native fields give nulls and defaults', () async {
      await scanner.start();
      rig.next(_rejected('x'));
      rig.frame(4); // no diagnostics fields, releaseFrame returns null
      await pumpEventQueue();
      final f = diag.whereType<ScanFrameDiagnostics>().single;
      expect(f.held, isNull);
      expect(f.capture, isNull);
      expect(f.received, isNull);
      expect(f.skippedBefore, 0);
      expect(f.droppedByOutputBefore, isNull);
      expect(f.buffer!.format, 'unknown');
      expect(f.buffer!.rotationDegrees, isNull);
      expect(f.buffer!.cameraId, isNull);
      expect(f.disposition, FrameDisposition.decoded);
    });

    test('rejections name their stage and category', () async {
      await scanner.start();
      final cases = {
        RejectStageDto.buffers: RejectCategoryDto.invalidBuffers,
        RejectStageDto.locate: RejectCategoryDto.noLock,
        RejectStageDto.read: RejectCategoryDto.badHeaderCrc,
        RejectStageDto.receiver: RejectCategoryDto.otherSession,
      };
      var id = 0;
      for (final e in cases.entries) {
        rig.next(_rejected('x', stage: e.key, category: e.value));
        rig.frame(++id);
        await pumpEventQueue();
      }
      final frames = diag.whereType<ScanFrameDiagnostics>().toList();
      expect(
        [for (final f in frames) f.rejectStage],
        [
          RejectStage.buffers,
          RejectStage.locate,
          RejectStage.read,
          RejectStage.receiver,
        ],
      );
      expect(
        [for (final f in frames) f.rejectCategory],
        ['invalidBuffers', 'noLock', 'badHeaderCrc', 'otherSession'],
      );
    });

    test('every RejectCategoryDto has a lowerCamelCase name', () {
      for (final c in RejectCategoryDto.values) {
        expect(c.name, matches(RegExp(r'^[a-z][A-Za-z0-9]*$')));
      }
    });

    test('dispositions of skipped and malformed frames', () async {
      await scanner.start();
      rig.handler = (call) =>
          call.method == 'releaseFrame' ? {'heldNanos': 1000} : null;
      List<ScanFrameDiagnostics> frames() =>
          diag.whereType<ScanFrameDiagnostics>().toList();

      // Busy: a second frame while one decodes.
      final gate = rig.decoders.single.gate = Completer<void>();
      rig.next(_rejected('x'));
      rig.frame(1);
      await pumpEventQueue();
      rig.frame(2);
      await pumpEventQueue();
      expect(frames().single.frameId, 2);
      expect(frames().single.disposition, FrameDisposition.skippedBusy);
      expect(frames().single.decode, isNull);
      expect(frames().single.bridge, isNull);
      expect(frames().single.held, const Duration(microseconds: 1));
      gate.complete();
      await pumpEventQueue();
      expect(frames().last.frameId, 1);
      expect(frames().last.disposition, FrameDisposition.decoded);

      // Malformed: has an id, bad fields.
      rig.frame(3, extra: {'width': 'wide'});
      await pumpEventQueue();
      expect(frames().last.frameId, 3);
      expect(frames().last.disposition, FrameDisposition.malformed);
      expect(frames().last.buffer, isNull);
      expect(got.whereType<ScanError>().single.code, 'malformed_event');

      // Inactive: while stopping.
      rig.decoders.single.gate = Completer<void>();
      rig.next(_rejected('x'));
      rig.frame(4);
      await pumpEventQueue();
      final stop = scanner.stop();
      await pumpEventQueue();
      rig.frame(5);
      await pumpEventQueue();
      expect(frames().last.frameId, 5);
      expect(frames().last.disposition, FrameDisposition.skippedInactive);
      rig.decoders.single.gate!.complete();
      await stop;
    });

    test('frames after done are skippedDone', () async {
      await scanner.start();
      rig.next(
        _result(
          locked: true,
          progress: ProgressDto.done(payload: Uint8List.fromList([1])),
        ),
      );
      rig.frame(1);
      await pumpEventQueue();
      rig.frame(2);
      await pumpEventQueue();
      final frames = diag.whereType<ScanFrameDiagnostics>().toList();
      expect(frames[0].done, isTrue);
      expect(frames[0].disposition, FrameDisposition.decoded);
      expect(frames[1].disposition, FrameDisposition.skippedDone);
      expect(frames[1].done, isFalse);
    });

    test('a failing decoder is still reported, without decode data', () async {
      await scanner.start();
      rig.decoders.single.failWith = StateError('boom');
      rig.frame(1);
      await pumpEventQueue();
      final f = diag.whereType<ScanFrameDiagnostics>().single;
      expect(f.disposition, FrameDisposition.decoded);
      expect(f.decode, isNull);
      expect(f.bridge, isNotNull);
    });

    test(
      'nothing is emitted, and nothing breaks, without a listener',
      () async {
        final quiet = rig.scanner(stopwatch: time.watch);
        addTearDown(quiet.dispose);
        await quiet.start();
        rig.next(_rejected('x'));
        rig.frame(1);
        await pumpEventQueue();
        expect(rig.released, contains(1));
      },
    );

    test(
      'no deadline: minutes between frames keep decoder and progress',
      () async {
        final clock = _Clock();
        final s = rig.scanner(clock: clock.call, stopwatch: time.watch);
        addTearDown(s.dispose);
        final events = <ScanEvent>[];
        final records = <ScanDiagnostic>[];
        s.events.listen(events.add);
        s.diagnostics.listen(records.add);
        await s.start();
        const accepted = ProgressDto.accepted(
          have: 4,
          need: 40,
          segmentsDone: 0,
          segmentsTotal: 3,
        );
        rig.next(_result(locked: true, progress: accepted));
        rig.frame(1);
        await pumpEventQueue();
        for (var i = 0; i < 3; i++) {
          clock.advance(const Duration(minutes: 7));
          time.advance(const Duration(minutes: 7));
          rig.next(
            _result(
              locked: true,
              progress: ProgressDto.accepted(
                have: 5 + i,
                need: 40,
                segmentsDone: 0,
                segmentsTotal: 3,
              ),
            ),
          );
          rig.frame(2 + i);
          await pumpEventQueue();
        }
        // Same decoder, progress continues, no terminal event or error.
        expect(rig.decoders, hasLength(1));
        expect(rig.decoders.single.disposed, isFalse);
        expect(events.map((e) => e.runtimeType), everyElement(ScanFrame));
        expect(events.whereType<ScanFrame>().last.have, 7);
        expect(events.whereType<ScanFrame>().last.need, 40);
        // A long gap, and a rejected frame after it, still keeps the progress.
        clock.advance(const Duration(minutes: 30));
        rig.next(_rejected('x'));
        rig.frame(9);
        await pumpEventQueue();
        final last = events.whereType<ScanFrame>().last;
        expect((last.have, last.need), (7, 40));
        expect(rig.decoders, hasLength(1));
        expect(
          records.whereType<ScanFrameDiagnostics>().last.sinceStart,
          const Duration(minutes: 21),
        );
      },
    );
  });

  test('start before ParticleCode.init is a StateError', () async {
    expect(ParticleCode.isInitialized, isFalse);
    final rig = _Rig()..install();
    final scanner = ParticleScanner(); // the real decoder
    addTearDown(scanner.dispose);
    await expectLater(
      scanner.start(),
      throwsA(
        isA<StateError>().having(
          (e) => e.message,
          'message',
          contains('ParticleCode.init()'),
        ),
      ),
    );
    expect(rig.methods, isNot(contains('start')));
  });

  group('with the Rust decoder', () {
    setUpAll(ParticleCode.init);

    test(
      'blank camera memory is rejected with a hint, read in place',
      () async {
        final rig = _Rig()..install();
        const w = 64, h = 48, c = w * h ~/ 4;
        final y = calloc<Uint8>(w * h);
        final u = calloc<Uint8>(c);
        final v = calloc<Uint8>(c);
        addTearDown(() {
          calloc.free(y);
          calloc.free(u);
          calloc.free(v);
        });
        y.asTypedList(w * h).fillRange(0, w * h, 128);
        u.asTypedList(c).fillRange(0, c, 128);
        v.asTypedList(c).fillRange(0, c, 128);
        final scanner = ParticleScanner();
        addTearDown(scanner.dispose);
        final got = <ScanEvent>[];
        final records = <ScanFrameDiagnostics>[];
        scanner.events.listen(got.add);
        scanner.diagnostics
            .where((d) => d is ScanFrameDiagnostics)
            .cast<ScanFrameDiagnostics>()
            .listen(records.add);
        await scanner.start();

        rig.frame(
          42,
          width: w,
          height: h,
          yAddress: y.address,
          yLen: w * h,
          uAddress: u.address,
          uLen: c,
          vAddress: v.address,
          vLen: c,
          yStride: w,
          uvStride: w ~/ 2,
          uvPixelStride: 1,
        );
        await rig.waitForRelease(42); // the decode runs on the Rust thread pool
        final f = got.single as ScanFrame;
        expect(f.status, ScanStatus.rejected);
        expect(f.locked, isFalse);
        expect(f.hint, isNotNull);
        expect(f.hintCode, isNot(ScanHint.none));
        expect(f.anchors, isEmpty);
        expect(rig.released, [42]);
        await pumpEventQueue();
        final blank = records.single;
        expect(blank.disposition, FrameDisposition.decoded);
        expect(blank.rejectStage, RejectStage.locate);
        expect(blank.rejectCategory, 'noLock');
        expect(blank.packet, isNull);
        expect(blank.decode!.validate, isNotNull);
        expect(blank.decode!.locate, isNotNull);
        expect(blank.decode!.read, isNull);
        expect(blank.decode!.receive, isNull);
        expect(blank.bridge, isNotNull);
        expect(blank.bridge, greaterThanOrEqualTo(Duration.zero));

        // Inconsistent buffers are a rejection, not a crash.
        rig.frame(
          43,
          width: w,
          height: h,
          yAddress: y.address,
          yLen: 10,
          uAddress: u.address,
          uLen: 10,
          vAddress: v.address,
          vLen: 10,
          yStride: w,
          uvStride: w ~/ 2,
          uvPixelStride: 1,
        );
        await rig.waitForRelease(43);
        expect(rig.released, [42, 43]);
        await pumpEventQueue();
        final bad = records.last;
        expect(bad.frameId, 43);
        expect(bad.rejectStage, RejectStage.buffers);
        expect(bad.rejectCategory, 'invalidBuffers');
        expect(bad.decode!.locate, isNull);

        await scanner.stop();
        expect(rig.methods.last, 'stop');
      },
    );
  });

  group('widgets', () {
    testWidgets('progress ring paints for several states', (tester) async {
      for (final ring in [
        const ScanProgressRing(have: 0, need: 0),
        const ScanProgressRing(
          have: 5,
          need: 10,
          segmentsDone: 1,
          segmentsTotal: 4,
        ),
        const ScanProgressRing(
          have: 10,
          need: 10,
          segmentsDone: 3,
          segmentsTotal: 3,
        ),
        const ScanProgressRing(
          have: 99,
          need: 10,
          segmentsDone: 40,
          segmentsTotal: 100,
        ),
        const ScanProgressRing(
          have: 1,
          need: 3,
          segmentsDone: 0,
          segmentsTotal: 1,
        ),
      ]) {
        await tester.pumpWidget(
          Directionality(
            textDirection: TextDirection.ltr,
            child: Center(child: ring),
          ),
        );
        expect(tester.takeException(), isNull);
        expect(
          tester.getSize(find.byType(ScanProgressRing)),
          const Size(72, 72),
        );
      }
      final frame = ScanEvent.fromMap(_frameMap())! as ScanFrame;
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: ScanProgressRing.fromFrame(frame, child: const Text('30%')),
        ),
      );
      expect(find.text('30%'), findsOneWidget);
    });

    test('anchors map from normalized to widget coordinates', () {
      expect(
        ScanAnchorsPainter.toWidget(
          const Offset(0.25, 0.5),
          const Size(400, 800),
        ),
        const Offset(100, 400),
      );
    });

    testWidgets('overlay shows anchors and a ring while there is progress', (
      tester,
    ) async {
      final frame = ScanEvent.fromMap(_frameMap())! as ScanFrame;
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 300,
            height: 600,
            child: ScanOverlay(frame: frame),
          ),
        ),
      );
      expect(find.byType(ScanProgressRing), findsOneWidget);
      expect(
        find.byWidgetPredicate(
          (w) => w is CustomPaint && w.painter is ScanAnchorsPainter,
        ),
        findsOneWidget,
      );
      await tester.pumpWidget(
        const Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 300,
            height: 600,
            child: ScanOverlay(frame: null),
          ),
        ),
      );
      expect(find.byType(ScanProgressRing), findsNothing);
    });

    for (final platform in [TargetPlatform.android, TargetPlatform.iOS]) {
      testWidgets('preview creates the native view on $platform', (
        tester,
      ) async {
        debugDefaultTargetPlatformOverride = platform;
        try {
          // The platform view needs engine support that the test host lacks; the widget
          // type and its parameters are what we can check.
          await tester.pumpWidget(
            const Directionality(
              textDirection: TextDirection.ltr,
              child: ParticleScannerPreview(),
            ),
          );
          if (platform == TargetPlatform.android) {
            final v = tester.widget<AndroidView>(find.byType(AndroidView));
            expect(v.viewType, 'particle_code/preview');
            expect(v.creationParams, isNull);
          } else {
            final v = tester.widget<UiKitView>(find.byType(UiKitView));
            expect(v.viewType, 'particle_code/preview');
            expect(v.creationParams, isNull);
          }
        } finally {
          debugDefaultTargetPlatformOverride = null;
        }
        tester.takeException();
      });
    }

    testWidgets('preview shows a placeholder elsewhere and feeds the overlay', (
      tester,
    ) async {
      debugDefaultTargetPlatformOverride = TargetPlatform.linux;
      final rig = _Rig()..install();
      final scanner = rig.scanner();
      addTearDown(scanner.dispose);
      await scanner.start();
      ScanFrame? seen;
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: ParticleScannerPreview(
            scanner: scanner,
            overlayBuilder: (context, frame) {
              seen = frame;
              return Text(frame == null ? 'waiting' : 'have ${frame.have}');
            },
          ),
        ),
      );
      expect(find.textContaining('not available'), findsOneWidget);
      expect(find.text('waiting'), findsOneWidget);
      rig.next(
        _result(
          locked: true,
          progress: const ProgressDto.accepted(
            have: 12,
            need: 40,
            segmentsDone: 1,
            segmentsTotal: 3,
          ),
        ),
      );
      rig.frame(1);
      await tester.pump();
      await tester.pump();
      expect(find.text('have 12'), findsOneWidget);
      expect(seen?.have, 12);
      debugDefaultTargetPlatformOverride = null;
    });
  });
}

// region test rig

/// A Stopwatch over a time the test moves by hand.
class _TestTime {
  Duration now = Duration.zero;
  void advance(Duration d) => now += d;
  Stopwatch watch() => _FakeWatch(this);
}

class _FakeWatch implements Stopwatch {
  _FakeWatch(this._time);

  final _TestTime _time;
  Duration? _startedAt;
  Duration _before = Duration.zero;

  @override
  void start() => _startedAt ??= _time.now;

  @override
  void stop() {
    final at = _startedAt;
    if (at == null) return;
    _before += _time.now - at;
    _startedAt = null;
  }

  @override
  void reset() {
    _before = Duration.zero;
    if (_startedAt != null) _startedAt = _time.now;
  }

  @override
  bool get isRunning => _startedAt != null;

  @override
  Duration get elapsed {
    final at = _startedAt;
    return at == null ? _before : _before + (_time.now - at);
  }

  @override
  int get elapsedMicroseconds => elapsed.inMicroseconds;

  @override
  int get elapsedMilliseconds => elapsed.inMilliseconds;

  @override
  int get elapsedTicks => elapsed.inMicroseconds;

  @override
  int get frequency => 1000000;
}

class _Clock {
  DateTime _now = DateTime(2026);
  DateTime call() => _now;
  void advance(Duration d) => _now = _now.add(d);
}

const _timings = DecodeTimingsDto(
  validateUs: 10,
  locateUs: 2000,
  readUs: 3000,
  receiveUs: 400,
  totalUs: 5500,
);

FrameResultDto _result({
  bool locked = false,
  List<ImagePointDto> anchors = const [],
  required ProgressDto progress,
  String? hint,
  HintDto hintCode = HintDto.none,
  RejectStageDto? rejectStage,
  RejectCategoryDto? rejectCategory,
  PacketDto? packet,
}) => FrameResultDto(
  timings: _timings,
  rejectStage: rejectStage,
  rejectCategory: rejectCategory,
  packet: packet,
  locked: locked,
  profile: locked ? ProfileDto.m : null,
  shapeId: locked ? 28 : null,
  anchors: anchors,
  sharpness: locked ? 2.5 : 0,
  liveness: locked
      ? const LivenessDto.read(seedLow: 9, rolling: 77, consistent: true)
      : const LivenessDto.unknown(),
  progress: progress,
  hint: hint,
  hintCode: hintCode,
);

FrameResultDto _rejected(
  String reason, {
  String? hint,
  HintDto hintCode = HintDto.none,
  RejectStageDto stage = RejectStageDto.locate,
  RejectCategoryDto category = RejectCategoryDto.noLock,
}) => FrameResultDto(
  timings: _timings,
  rejectStage: stage,
  rejectCategory: category,
  locked: false,
  anchors: const [],
  sharpness: 0,
  liveness: const LivenessDto.unknown(),
  progress: ProgressDto.rejected(reason: reason),
  rejectReason: reason,
  hint: hint,
  hintCode: hintCode,
);

/// A scripted decoder: results come from a queue, `gate` holds a decode open.
class _FakeDecoder implements FrameDecoder {
  _FakeDecoder(this.queue);

  final List<FrameResultDto> queue;
  final List<CameraFrameDto> fed = [];
  Completer<void>? gate;
  Object? failWith;
  Uint8List? ready;
  int? session;
  bool disposed = false;

  @override
  Future<FrameResultDto> feed(CameraFrameDto frame) async {
    fed.add(frame);
    await gate?.future;
    final e = failWith;
    if (e != null) throw e;
    return queue.isEmpty ? _rejected('empty queue') : queue.removeAt(0);
  }

  @override
  Uint8List? takeReady() {
    final r = ready;
    ready = null;
    return r;
  }

  @override
  int? sessionId() => session;

  @override
  void dispose() => disposed = true;
}

/// A fake platform: records method calls, and delivers events on the camera frames channel
/// the way the engine does (a success envelope).
class _Rig {
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  final List<MethodCall> calls = [];
  final List<_FakeDecoder> decoders = [];
  final List<FrameResultDto> results = [];
  Object? Function(MethodCall) handler = (_) => null;
  MockStreamHandlerEventSink? sink;
  int listens = 0;
  int cancels = 0;

  List<String> get methods => [for (final c in calls) c.method];
  List<int> get released => [
    for (final c in calls)
      if (c.method == 'releaseFrame') (c.arguments as Map)['id'] as int,
  ];

  void install() {
    messenger.setMockMethodCallHandler(ParticleScanner.channel, (call) async {
      calls.add(call);
      return handler(call);
    });
    messenger.setMockStreamHandler(
      ParticleScanner.framesChannel,
      MockStreamHandler.inline(
        onListen: (args, events) {
          listens++;
          sink = events;
        },
        onCancel: (args) {
          cancels++;
        },
      ),
    );
    addTearDown(() {
      messenger.setMockMethodCallHandler(ParticleScanner.channel, null);
      messenger.setMockStreamHandler(ParticleScanner.framesChannel, null);
    });
  }

  ParticleScanner scanner({
    DateTime Function()? clock,
    Stopwatch Function()? stopwatch,
  }) => ParticleScanner(
    decoderFactory: () {
      final d = _FakeDecoder(results);
      decoders.add(d);
      return d;
    },
    clock: clock,
    stopwatchFactory: stopwatch,
  );

  /// Queues the result for the next decoded frame.
  void next(FrameResultDto r) => results.add(r);

  void frame(
    int id, {
    int width = 640,
    int height = 480,
    int yAddress = 1000,
    int? yLen,
    int uAddress = 5000,
    int uLen = 3000,
    int vAddress = 5001,
    int vLen = 2999,
    int yStride = 640,
    int uvStride = 320,
    int uvPixelStride = 2,
    List<double> toPreview = const [1, 0, 0, 0, 1, 0],
    Map<String, Object?> extra = const {},
  }) {
    final event = <String, Object?>{
      'type': 'frame',
      'id': id,
      'width': width,
      'height': height,
      'yAddress': yAddress,
      'yLen': yLen ?? width * height,
      'uAddress': uAddress,
      'uLen': uLen,
      'vAddress': vAddress,
      'vLen': vLen,
      'yStride': yStride,
      'uvStride': uvStride,
      'uvPixelStride': uvPixelStride,
      'toPreview': toPreview,
      ...extra,
    };
    final data = const StandardMethodCodec().encodeSuccessEnvelope(event);
    messenger.handlePlatformMessage(
      ParticleScanner.framesChannel.name,
      data,
      (_) {},
    );
  }

  Future<void> waitForRelease(int id) async {
    for (var i = 0; i < 500 && !released.contains(id); i++) {
      await Future<void>.delayed(const Duration(milliseconds: 10));
    }
    expect(released, contains(id));
  }
}

// endregion
