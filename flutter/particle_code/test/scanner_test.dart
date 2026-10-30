import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code/particle_code.dart';

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

  group('method channel', () {
    final calls = <MethodCall>[];
    late Object? Function(MethodCall) handler;
    late ParticleScanner scanner;
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;

    setUp(() {
      calls.clear();
      handler = (_) => null;
      messenger.setMockMethodCallHandler(ParticleScanner.channel, (call) async {
        calls.add(call);
        return handler(call);
      });
      scanner = ParticleScanner();
    });

    tearDown(() {
      scanner.dispose();
      messenger.setMockMethodCallHandler(ParticleScanner.channel, null);
    });

    test('requestPermission', () async {
      handler = (_) => true;
      expect(await scanner.requestPermission(), isTrue);
      handler = (_) => false;
      expect(await scanner.requestPermission(), isFalse);
      handler = (_) => null;
      expect(await scanner.requestPermission(), isFalse);
      expect(calls.map((c) => c.method), everyElement('requestPermission'));
      expect(calls.first.arguments, isNull);
    });

    test('start sends the torch flag', () async {
      await scanner.start();
      await scanner.start(torch: true);
      expect(calls.map((c) => c.method), ['start', 'start']);
      expect(calls[0].arguments, {'torch': false});
      expect(calls[1].arguments, {'torch': true});
    });

    test('stop and reset', () async {
      await scanner.stop();
      await scanner.reset();
      expect(calls.map((c) => c.method), ['stop', 'reset']);
    });

    test('setTorch returns whether a torch exists', () async {
      handler = (_) => true;
      expect(await scanner.setTorch(true), isTrue);
      expect(calls.single.arguments, {'on': true});
      handler = (_) => false;
      expect(await scanner.setTorch(false), isFalse);
    });

    for (final code in ['permission_denied', 'no_camera', 'camera_error']) {
      test('start maps $code', () async {
        handler = (_) =>
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
      });
    }

    test('an unknown code becomes unknown', () async {
      handler = (_) => throw PlatformException(code: 'weird');
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
      messenger.setMockMethodCallHandler(ParticleScanner.channel, null);
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

    test('dispose stops the camera and forbids further use', () async {
      final s = ParticleScanner();
      s.dispose();
      await Future<void>.delayed(Duration.zero);
      expect(calls.map((c) => c.method), contains('stop'));
      await expectLater(s.start(), throwsStateError);
      s.dispose(); // idempotent
    });
  });

  group('event channel', () {
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    late MockStreamHandlerEventSink? sink;
    var listens = 0;
    var cancels = 0;

    setUp(() {
      sink = null;
      listens = 0;
      cancels = 0;
      messenger.setMockStreamHandler(
        ParticleScanner.eventsChannel,
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
    });

    tearDown(
      () => messenger.setMockStreamHandler(ParticleScanner.eventsChannel, null),
    );

    test('delivers typed events, ignores unknown, flags malformed', () async {
      final scanner = ParticleScanner();
      addTearDown(scanner.dispose);
      final got = <ScanEvent>[];
      final sub = scanner.events.listen(got.add);
      await Future<void>.delayed(Duration.zero);
      expect(listens, 1);

      sink!.success(_frameMap());
      sink!.success({'type': 'unheard-of'});
      sink!.success({'type': 'done', 'payload': 5, 'sessionId': 1});
      sink!.success({
        'type': 'segment',
        'bytes': Uint8List.fromList([7]),
      });
      sink!.success({
        'type': 'done',
        'payload': Uint8List.fromList([1, 2]),
        'sessionId': 3,
      });
      sink!.error(code: 'camera_error', message: 'lost');
      await Future<void>.delayed(Duration.zero);

      expect(got.map((e) => e.runtimeType), [
        ScanFrame,
        ScanError,
        ScanSegment,
        ScanDone,
        ScanError,
      ]);
      expect((got[1] as ScanError).code, 'malformed_event');
      expect((got[4] as ScanError).code, 'camera_error');
      await sub.cancel();
      await Future<void>.delayed(Duration.zero);
      expect(cancels, 1);
    });

    test('is broadcast and attaches once for many listeners', () async {
      final scanner = ParticleScanner();
      addTearDown(scanner.dispose);
      final a = <ScanEvent>[];
      final b = <ScanEvent>[];
      final s1 = scanner.events.listen(a.add);
      final s2 = scanner.events.listen(b.add);
      await Future<void>.delayed(Duration.zero);
      expect(listens, 1);
      sink!.success(_frameMap());
      await Future<void>.delayed(Duration.zero);
      expect((a.length, b.length), (1, 1));
      await s1.cancel();
      await s2.cancel();
    });

    test('listening does not call any method', () async {
      final calls = <MethodCall>[];
      messenger.setMockMethodCallHandler(ParticleScanner.channel, (c) async {
        calls.add(c);
        return null;
      });
      addTearDown(
        () => messenger.setMockMethodCallHandler(ParticleScanner.channel, null),
      );
      final scanner = ParticleScanner();
      final sub = scanner.events.listen((_) {});
      await Future<void>.delayed(Duration.zero);
      expect(calls, isEmpty);
      await sub.cancel();
      scanner.dispose();
    });
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
      final messenger =
          TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
      late MockStreamHandlerEventSink sink;
      messenger.setMockStreamHandler(
        ParticleScanner.eventsChannel,
        MockStreamHandler.inline(
          onListen: (args, events) {
            sink = events;
          },
        ),
      );
      addTearDown(
        () =>
            messenger.setMockStreamHandler(ParticleScanner.eventsChannel, null),
      );
      final scanner = ParticleScanner();
      addTearDown(scanner.dispose);
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
      sink.success(_frameMap());
      await tester.pump();
      await tester.pump();
      expect(find.text('have 12'), findsOneWidget);
      expect(seen?.have, 12);
      debugDefaultTargetPlatformOverride = null;
    });
  });
}
