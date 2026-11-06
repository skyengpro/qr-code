// The scan tab on mocked platform channels, the way the plugin's own tests do it.
import 'dart:convert';

import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code/particle_code.dart';
import 'package:particle_code/src/rust/api/dto.dart';
import 'package:particle_code/src/scanner/decoder.dart';
import 'package:particle_code_example/main.dart';
import 'package:particle_code_example/scan_result.dart';
import 'package:particle_code_example/scan_tab.dart';

/// What the scripted decoder returns for one frame.
FrameResultDto _frame({
  bool locked = true,
  int have = 10,
  int need = 40,
  String? hint,
  bool consistent = true,
}) => FrameResultDto(
  locked: locked,
  profile: ProfileDto.m,
  shapeId: 0,
  anchors: const [
    ImagePointDto(x: 0.5, y: 0.2),
    ImagePointDto(x: 0.8, y: 0.7),
    ImagePointDto(x: 0.2, y: 0.7),
  ],
  sharpness: 2.0,
  liveness: LivenessDto.read(seedLow: 7, rolling: 99, consistent: consistent),
  progress: ProgressDto.accepted(
    have: have,
    need: need,
    segmentsDone: 1,
    segmentsTotal: 3,
  ),
  hint: hint,
  hintCode: hint == null ? HintDto.none : HintDto.holdSteady,
);

FrameResultDto _done(Uint8List payload, {bool consistent = true}) =>
    FrameResultDto(
      locked: true,
      profile: ProfileDto.m,
      shapeId: 0,
      anchors: const [],
      sharpness: 2.0,
      liveness: LivenessDto.read(
        seedLow: 7,
        rolling: 99,
        consistent: consistent,
      ),
      progress: ProgressDto.done(payload: payload),
      hintCode: HintDto.none,
    );

/// A decoder that returns what the test queued.
class _ScriptedDecoder implements FrameDecoder {
  final List<FrameResultDto> queue = [];
  Uint8List? ready;
  int? session;

  @override
  Future<FrameResultDto> feed(CameraFrameDto frame) async => queue.removeAt(0);

  @override
  Uint8List? takeReady() {
    final r = ready;
    ready = null;
    return r;
  }

  @override
  int? sessionId() => session;

  @override
  void dispose() {}
}

/// A fake native side: records method calls and lets a test push events.
class _FakePlatform {
  _FakePlatform({
    this.permission = true,
    this.startError,
    this.hasTorch = true,
    this.missing = false,
  });

  bool permission;
  PlatformException? startError;
  bool hasTorch;
  bool missing;
  final List<MethodCall> calls = [];
  MockStreamHandlerEventSink? sink;
  final _ScriptedDecoder decoder = _ScriptedDecoder();
  DateTime _now = DateTime(2026);
  int _id = 0;

  /// A scanner on the mocked channels and the scripted decoder.
  ParticleScanner scanner() =>
      ParticleScanner(decoderFactory: () => decoder, clock: () => _now);

  List<String> get methods => [for (final c in calls) c.method];

  void install() {
    final messenger =
        TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(ParticleScanner.channel, (call) async {
      calls.add(call);
      if (missing) throw MissingPluginException();
      switch (call.method) {
        case 'requestPermission':
          return permission;
        case 'start':
          if (startError != null) throw startError!;
          return null;
        case 'setTorch':
          return hasTorch;
        default:
          return null;
      }
    });
    messenger.setMockStreamHandler(
      ParticleScanner.framesChannel,
      MockStreamHandler.inline(
        onListen: (args, events) {
          sink = events;
        },
      ),
    );
    addTearDown(() {
      messenger.setMockMethodCallHandler(ParticleScanner.channel, null);
      messenger.setMockStreamHandler(ParticleScanner.framesChannel, null);
    });
  }

  /// The camera hands over one frame; the decoder answers with [result]. [ready] are the
  /// payload bytes the decoder delivers with it.
  Future<void> send(
    WidgetTester tester,
    FrameResultDto result, {
    Uint8List? ready,
  }) async {
    decoder.queue.add(result);
    decoder.ready = ready;
    _now = _now.add(const Duration(seconds: 1));
    sink!.success({
      'type': 'frame',
      'id': ++_id,
      'width': 640,
      'height': 480,
      'yAddress': 1,
      'yLen': 1,
      'uAddress': 1,
      'uLen': 1,
      'vAddress': 1,
      'vLen': 1,
      'yStride': 640,
      'uvStride': 320,
      'uvPixelStride': 2,
      'toPreview': [1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
    });
    await tester.pump();
    await tester.pump();
    await tester.pump();
  }
}

Future<void> _pumpTab(WidgetTester tester, _FakePlatform platform) async {
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(body: ScanTab(scannerFactory: platform.scanner)),
    ),
  );
  await tester.pump();
  await tester.pump();
}

/// A widget test on a non-mobile platform: the camera preview is a platform view that
/// only exists on Android and iOS, and elsewhere the plugin paints a placeholder.
void scanTest(String description, Future<void> Function(WidgetTester) body) {
  testWidgets(description, (tester) async {
    debugDefaultTargetPlatformOverride = TargetPlatform.linux;
    try {
      await body(tester);
    } finally {
      debugDefaultTargetPlatformOverride = null;
    }
  });
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  // The Home test below builds the real scanner, which needs the Rust library.
  setUpAll(ParticleCode.init);

  group('result helpers', () {
    test('hex preview truncates', () {
      final bytes = Uint8List.fromList([for (var i = 0; i < 60; i++) i]);
      final hex = hexPreview(bytes);
      expect(hex, startsWith('00 01 02'));
      expect(hex, endsWith('… +12 more'));
      expect(hexPreview(Uint8List.fromList([0xAB, 0x05])), 'ab 05');
    });

    test('text preview only for readable UTF-8', () {
      expect(utf8Preview(Uint8List.fromList(utf8.encode('héllo'))), 'héllo');
      expect(utf8Preview(Uint8List.fromList([0xD2, 0x84, 0x00, 0xFF])), isNull);
      expect(utf8Preview(Uint8List.fromList([0x01, 0x02])), isNull);
      expect(utf8Preview(Uint8List(0)), isNull);
    });

    test('sizes', () {
      expect(formatBytes(193), '193 B');
      expect(formatBytes(2048), '2.0 KB');
    });
  });

  group('Scan tab', () {
    scanTest('asks permission, starts, shows hint, segments and progress', (
      tester,
    ) async {
      final platform = _FakePlatform()..install();
      await _pumpTab(tester, platform);
      expect(platform.methods, ['requestPermission', 'start']);
      expect(find.text('Point the camera at a Particle Code'), findsOneWidget);
      expect(find.text('Received 0 B'), findsOneWidget);
      expect(find.text('Waiting for the first segment'), findsOneWidget);

      await platform.send(tester, _frame(hint: 'Hold steady'));
      expect(find.text('Hold steady'), findsOneWidget);
      expect(find.text('25%'), findsOneWidget);
      expect(find.textContaining('Segment 1 of 3'), findsOneWidget);
      expect(find.textContaining('symbols 10/40'), findsOneWidget);

      await platform.send(tester, _frame());
      expect(find.text('Reading… keep it in view'), findsOneWidget);

      await platform.send(tester, _frame(), ready: Uint8List(1500));
      await platform.send(tester, _frame(), ready: Uint8List(548));
      expect(find.text('Received 2.0 KB'), findsOneWidget);
    });

    scanTest('reset clears scan progress and accepts a new transfer', (
      tester,
    ) async {
      final platform = _FakePlatform()..install();
      await _pumpTab(tester, platform);
      await platform.send(tester, _frame(hint: 'Hold steady'));
      await platform.send(tester, _frame(), ready: Uint8List(1500));
      expect(find.text('25%'), findsOneWidget);
      expect(find.text('Received 1.5 KB'), findsOneWidget);

      await tester.tap(find.byKey(const Key('reset-scan')));
      await tester.pump();
      await tester.pump();
      // Reset is a new decoder in Dart; the platform is not involved.
      expect(platform.methods, isNot(contains('reset')));
      expect(find.text('0%'), findsOneWidget);
      expect(find.text('Received 0 B'), findsOneWidget);
      expect(find.text('Waiting for the first segment'), findsOneWidget);
      expect(find.text('Point the camera at a Particle Code'), findsOneWidget);

      await platform.send(tester, _frame(have: 2, need: 4));
      expect(find.text('50%'), findsOneWidget);
    });

    scanTest('torch toggles through the platform', (tester) async {
      final platform = _FakePlatform()..install();
      await _pumpTab(tester, platform);
      await tester.tap(find.byKey(const Key('torch')));
      await tester.pump();
      expect(platform.calls.last.method, 'setTorch');
      expect(platform.calls.last.arguments, {'on': true});
      expect(find.byIcon(Icons.flash_on), findsOneWidget);

      await tester.tap(find.byKey(const Key('torch')));
      await tester.pump();
      expect(platform.calls.last.arguments, {'on': false});
      expect(find.byIcon(Icons.flash_off), findsOneWidget);
    });

    scanTest('a device without a torch says so', (tester) async {
      final platform = _FakePlatform(hasTorch: false)..install();
      await _pumpTab(tester, platform);
      await tester.tap(find.byKey(const Key('torch')));
      await tester.pump();
      expect(find.text('This device has no torch.'), findsOneWidget);
      expect(find.byIcon(Icons.flash_off), findsOneWidget);
    });

    scanTest('done opens the result sheet; Scan again resets', (tester) async {
      final platform = _FakePlatform()..install();
      await _pumpTab(tester, platform);
      await platform.send(tester, _frame(consistent: true));

      platform.decoder.session = 0x2A3B;
      await platform.send(
        tester,
        _done(Uint8List.fromList(utf8.encode('Hello, particles'))),
      );
      await tester.pump(const Duration(milliseconds: 500));

      expect(find.text('Code received'), findsOneWidget);
      expect(find.text('16 bytes (16 B)'), findsOneWidget);
      expect(find.text('2A3B'), findsOneWidget);
      expect(find.text('yes'), findsOneWidget);
      expect(find.byKey(const Key('hex-preview')), findsOneWidget);
      expect(
        tester
            .widget<SelectableText>(find.byKey(const Key('hex-preview')))
            .data,
        startsWith('48 65 6c 6c 6f'),
      );
      expect(
        tester
            .widget<SelectableText>(find.byKey(const Key('text-preview')))
            .data,
        'Hello, particles',
      );
      expect(platform.methods, isNot(contains('reset')));

      await tester.tap(find.text('Scan again'));
      await tester.pumpAndSettle();
      expect(find.text('Code received'), findsNothing);
      // Reset is a new decoder in Dart; the platform is not involved.
      expect(platform.methods, isNot(contains('reset')));
      expect(find.text('Received 0 B'), findsOneWidget);
    });

    scanTest('binary payload has no text preview; inconsistent liveness', (
      tester,
    ) async {
      final platform = _FakePlatform()..install();
      await _pumpTab(tester, platform);
      await platform.send(tester, _frame(consistent: false));
      platform.decoder.session = 1;
      await platform.send(
        tester,
        _done(
          Uint8List.fromList([0xD2, 0x84, 0x43, 0xA1, 0x01, 0x26, 0x00]),
          consistent: false,
        ),
      );
      await tester.pump(const Duration(milliseconds: 500));
      expect(find.byKey(const Key('text-preview')), findsNothing);
      expect(find.text('no'), findsOneWidget);
      expect(find.textContaining('d2 84 43 a1'), findsOneWidget);
    });

    scanTest('permission denied offers a retry that works', (tester) async {
      final platform = _FakePlatform(permission: false)..install();
      await _pumpTab(tester, platform);
      expect(find.text('Camera permission needed'), findsOneWidget);
      expect(platform.methods, ['requestPermission']);

      platform.permission = true;
      await tester.tap(find.text('Try again'));
      await tester.pump();
      await tester.pump();
      expect(platform.methods, [
        'requestPermission',
        'requestPermission',
        'start',
      ]);
      expect(find.text('Camera permission needed'), findsNothing);
      expect(find.byKey(const Key('scan-hint')), findsOneWidget);
    });

    scanTest('permission denied by start() is the same screen', (tester) async {
      final platform = _FakePlatform(
        startError: PlatformException(code: 'permission_denied'),
      )..install();
      await _pumpTab(tester, platform);
      expect(find.text('Camera permission needed'), findsOneWidget);
    });

    scanTest('no camera (simulator)', (tester) async {
      final platform = _FakePlatform(
        startError: PlatformException(code: 'no_camera', message: 'none'),
      )..install();
      await _pumpTab(tester, platform);
      expect(find.text('No camera available'), findsOneWidget);
      expect(find.textContaining('Simulators'), findsOneWidget);
      expect(find.text('Retry'), findsOneWidget);
    });

    scanTest('unsupported platform (no plugin registered)', (tester) async {
      // The native side answers "not implemented", which the plugin reports as
      // unsupported.
      final platform = _FakePlatform(missing: true)..install();
      await _pumpTab(tester, platform);
      expect(find.text('Scanning is not available here'), findsOneWidget);
      expect(find.text('Retry'), findsNothing);
    });

    scanTest('a camera error event shows the message and can retry', (
      tester,
    ) async {
      final platform = _FakePlatform()..install();
      await _pumpTab(tester, platform);
      platform.sink!.error(
        code: 'camera_error',
        message: 'camera disconnected',
      );
      await tester.pump();
      await tester.pump();
      expect(find.text('The camera stopped'), findsOneWidget);
      expect(find.text('camera disconnected'), findsOneWidget);

      await tester.tap(find.text('Retry'));
      await tester.pump();
      await tester.pump();
      expect(find.text('The camera stopped'), findsNothing);
      expect(platform.methods.where((m) => m == 'start'), hasLength(2));
    });

    scanTest('leaving the tab stops the camera', (tester) async {
      final platform = _FakePlatform()..install();
      await _pumpTab(tester, platform);
      await tester.pumpWidget(const MaterialApp(home: SizedBox()));
      await tester.pump();
      expect(platform.methods.last, 'stop');
    });
  });

  group('Home', () {
    scanTest('the scan tab only exists while selected', (tester) async {
      final platform = _FakePlatform()..install();
      await tester.pumpWidget(const ExampleApp());
      expect(find.byType(ScanTab), findsNothing);
      expect(platform.methods, isEmpty);

      await tester.tap(find.text('Scan'));
      await tester.pump();
      await tester.pump();
      expect(find.byType(ScanTab), findsOneWidget);
      expect(platform.methods, contains('start'));

      await tester.tap(find.text('Send'));
      await tester.pump();
      expect(find.byType(ScanTab), findsNothing);
      expect(platform.methods.last, 'stop');
    });
  });
}
