import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code/particle_code.dart';

ParticleEncoder _encoder({int fps = 10}) => ParticleEncoder.create(
  Uint8List.fromList(List.generate(600, (i) => i * 13 & 0xFF)),
  options: EncoderOptions(
    fps: fps,
    segmentKbLog2: 0,
    sessionId: 99,
    key: List.generate(32, (i) => i),
    clockOriginUnix: 1700000000,
  ),
);

ParticleCodePainter _painter(WidgetTester tester) {
  final paint = tester.widget<CustomPaint>(
    find.descendant(
      of: find.byType(ParticleCodeView),
      matching: find.byType(CustomPaint),
    ),
  );
  return paint.painter! as ParticleCodePainter;
}

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  final messenger =
      TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger;
  final displayCalls = <bool>[];

  setUp(() {
    displayCalls.clear();
    ParticleCodeDisplay.debugReset();
    messenger.setMockMethodCallHandler(ParticleCodeDisplay.channel, (
      call,
    ) async {
      expect(call.method, 'setShowingCode');
      displayCalls.add((call.arguments as Map)['on'] as bool);
      return null;
    });
  });

  tearDown(() {
    messenger.setMockMethodCallHandler(ParticleCodeDisplay.channel, null);
  });

  Widget host(Widget child) => Directionality(
    textDirection: TextDirection.ltr,
    child: Center(child: child),
  );

  testWidgets('frame index follows elapsed time, not paints', (tester) async {
    final enc = _encoder();
    addTearDown(enc.dispose);
    final clocks = <int>[];
    var now = 1700000000;
    await tester.pumpWidget(
      host(
        ParticleCodeView(
          useBackgroundIsolate: false,
          encoder: enc,
          size: 200,
          unixClock: () {
            clocks.add(now);
            return now++;
          },
        ),
      ),
    );
    expect(_painter(tester).frame.index, 0);
    expect(clocks.length, 1, reason: 'frame 0 fetched once at mount');

    // Many paints inside one 100 ms data frame: no new fetch.
    await tester.pump(); // first tick (elapsed 0)
    for (var i = 0; i < 5; i++) {
      await tester.pump(const Duration(milliseconds: 16));
    }
    expect(_painter(tester).frame.index, 0);
    expect(clocks.length, 1);

    // 80 ms elapsed so far; cross the 100 ms boundary.
    await tester.pump(const Duration(milliseconds: 30));
    expect(_painter(tester).frame.index, 1);
    expect(clocks.length, 2);
    expect(_painter(tester).prev?.index, 0);
    expect(_painter(tester).tSinceSwitch, closeTo(0.01, 1e-9));

    // A long gap skips frames: shows the current index, previous is what was visible.
    await tester.pump(const Duration(milliseconds: 1000));
    expect(_painter(tester).frame.index, 11);
    expect(_painter(tester).prev?.index, 1);
    expect(clocks.length, 3);

    // The wall clock is passed through to the encoder.
    expect(clocks, [1700000000, 1700000001, 1700000002]);
  });

  testWidgets('the fps of the encoder sets the data frame rate', (
    tester,
  ) async {
    final enc = _encoder(fps: 20);
    addTearDown(enc.dispose);
    await tester.pumpWidget(
      host(
        ParticleCodeView(useBackgroundIsolate: false, encoder: enc, size: 100),
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 55));
    expect(_painter(tester).frame.index, 1);
    await tester.pump(const Duration(milliseconds: 50));
    expect(_painter(tester).frame.index, 2);
  });

  testWidgets('particles show the wall-clock liveness value', (tester) async {
    final enc = _encoder();
    addTearDown(enc.dispose);
    var now = 1700000000;
    await tester.pumpWidget(
      host(
        ParticleCodeView(
          useBackgroundIsolate: false,
          encoder: enc,
          size: 100,
          unixClock: () => now,
        ),
      ),
    );
    final first = _painter(
      tester,
    ).frame.particles.map((p) => p.sector).toList();
    final expected = enc
        .frame(0, unixSeconds: now)
        .particles
        .map((p) => p.sector)
        .toList();
    expect(first, expected);
    now = 1700009999;
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 150));
    final later = _painter(
      tester,
    ).frame.particles.map((p) => p.sector).toList();
    expect(
      later,
      enc.frame(1, unixSeconds: now).particles.map((p) => p.sector).toList(),
    );
  });

  testWidgets('max brightness while showing, restored on dispose', (
    tester,
  ) async {
    final enc = _encoder();
    addTearDown(enc.dispose);
    await tester.pumpWidget(
      host(
        ParticleCodeView(useBackgroundIsolate: false, encoder: enc, size: 100),
      ),
    );
    await tester.pump();
    expect(displayCalls, [true]);
    expect(ParticleCodeDisplay.holders, 1);

    await tester.pumpWidget(host(const SizedBox()));
    await tester.pump();
    expect(displayCalls, [true, false]);
    expect(ParticleCodeDisplay.holders, 0);
  });

  testWidgets('showing: false restores brightness and stops the clock', (
    tester,
  ) async {
    final enc = _encoder();
    addTearDown(enc.dispose);
    Widget view(bool showing) => host(
      ParticleCodeView(
        useBackgroundIsolate: false,
        encoder: enc,
        size: 100,
        showing: showing,
      ),
    );

    await tester.pumpWidget(view(true));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 250));
    expect(_painter(tester).frame.index, 2);
    expect(displayCalls, [true]);

    await tester.pumpWidget(view(false));
    await tester.pump();
    expect(displayCalls, [true, false]);
    expect(
      find.descendant(
        of: find.byType(ParticleCodeView),
        matching: find.byType(CustomPaint),
      ),
      findsNothing,
    );
    // Time passing while hidden schedules nothing.
    await tester.pump(const Duration(seconds: 5));

    await tester.pumpWidget(view(true));
    await tester.pump();
    expect(displayCalls, [true, false, true]);
    expect(
      _painter(tester).frame.index,
      0,
      reason: 'a shown code restarts at frame 0',
    );
    await tester.pumpWidget(host(const SizedBox()));
    expect(displayCalls, [true, false, true, false]);
  });

  testWidgets('starting with showing: false never raises brightness', (
    tester,
  ) async {
    final enc = _encoder();
    addTearDown(enc.dispose);
    await tester.pumpWidget(
      host(
        ParticleCodeView(
          useBackgroundIsolate: false,
          encoder: enc,
          size: 100,
          showing: false,
        ),
      ),
    );
    await tester.pumpWidget(host(const SizedBox()));
    expect(displayCalls, isEmpty);
  });

  testWidgets('two views share one brightness hold', (tester) async {
    final enc = _encoder();
    addTearDown(enc.dispose);
    await tester.pumpWidget(
      host(
        Column(
          children: [
            ParticleCodeView(
              useBackgroundIsolate: false,
              encoder: enc,
              size: 50,
            ),
            ParticleCodeView(
              useBackgroundIsolate: false,
              encoder: enc,
              size: 50,
            ),
          ],
        ),
      ),
    );
    await tester.pump();
    expect(displayCalls, [true]);
    await tester.pumpWidget(
      host(
        Column(
          children: [
            ParticleCodeView(
              useBackgroundIsolate: false,
              encoder: enc,
              size: 50,
            ),
          ],
        ),
      ),
    );
    expect(displayCalls, [true], reason: 'one view still showing');
    await tester.pumpWidget(host(const SizedBox()));
    expect(displayCalls, [true, false]);
  });

  testWidgets('a missing plugin is swallowed', (tester) async {
    messenger.setMockMethodCallHandler(ParticleCodeDisplay.channel, null);
    final enc = _encoder();
    addTearDown(enc.dispose);
    await tester.pumpWidget(
      host(
        ParticleCodeView(useBackgroundIsolate: false, encoder: enc, size: 100),
      ),
    );
    await tester.pump(const Duration(milliseconds: 150));
    await tester.pumpWidget(host(const SizedBox()));
    expect(tester.takeException(), isNull);
  });

  testWidgets('sizing: explicit size and fill-the-square', (tester) async {
    final enc = _encoder();
    addTearDown(enc.dispose);
    await tester.pumpWidget(
      host(
        ParticleCodeView(useBackgroundIsolate: false, encoder: enc, size: 123),
      ),
    );
    expect(tester.getSize(find.byType(ParticleCodeView)), const Size(123, 123));
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Align(
          child: ConstrainedBox(
            constraints: BoxConstraints.loose(const Size(300, 500)),
            child: ParticleCodeView(useBackgroundIsolate: false, encoder: enc),
          ),
        ),
      ),
    );
    expect(tester.getSize(find.byType(ParticleCodeView)), const Size(300, 300));
  });

  testWidgets('motion and background are forwarded to the painter', (
    tester,
  ) async {
    final enc = _encoder();
    addTearDown(enc.dispose);
    await tester.pumpWidget(
      host(
        ParticleCodeView(
          useBackgroundIsolate: false,
          encoder: enc,
          size: 100,
          motion: const MotionConfig.all(),
          background: Colors.black,
        ),
      ),
    );
    final p = _painter(tester);
    expect(p.motion, const MotionConfig.all());
    expect(p.background, Colors.black);
    expect(p.seed, seedFromSession(99));

    await tester.pumpWidget(
      host(
        ParticleCodeView(
          useBackgroundIsolate: false,
          encoder: enc,
          size: 100,
          motion: null,
        ),
      ),
    );
    expect(_painter(tester).motion, isNull);
  });

  testWidgets('swapping the encoder restarts at frame 0', (tester) async {
    final a = _encoder();
    final b = _encoder();
    addTearDown(a.dispose);
    addTearDown(b.dispose);
    await tester.pumpWidget(
      host(
        ParticleCodeView(useBackgroundIsolate: false, encoder: a, size: 100),
      ),
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 350));
    expect(_painter(tester).frame.index, 3);
    await tester.pumpWidget(
      host(
        ParticleCodeView(useBackgroundIsolate: false, encoder: b, size: 100),
      ),
    );
    expect(_painter(tester).frame.index, 0);
    expect(_painter(tester).prev, isNull);
  });

  testWidgets('the previous frame is always the one shown before', (
    tester,
  ) async {
    final enc = _encoder();
    addTearDown(enc.dispose);
    await tester.pumpWidget(
      host(
        ParticleCodeView(useBackgroundIsolate: false, encoder: enc, size: 100),
      ),
    );
    await tester.pump();
    for (var i = 0; i < 6; i++) {
      await tester.pump(const Duration(milliseconds: 100));
      final p = _painter(tester);
      expect(p.prev?.index, p.frame.index - 1);
    }
  });
}
