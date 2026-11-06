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
  setUpAll(ParticleCode.init);
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
    var now = 1700000000;
    await tester.pumpWidget(
      host(ParticleCodeView(encoder: enc, size: 200, unixClock: () => now)),
    );
    // Frame 0 is built synchronously at mount, with the wall clock passed through.
    expect(_painter(tester).frame.index, 0);
    expect(
      _painter(tester).frame.sameAs(enc.frame(0, unixSeconds: 1700000000)),
      isTrue,
    );

    // Many paints inside one 100 ms data frame: still frame 0.
    await tester.pump(); // first tick (elapsed 0)
    for (var i = 0; i < 5; i++) {
      await tester.pump(const Duration(milliseconds: 16));
    }
    expect(_painter(tester).frame.index, 0);

    // 80 ms elapsed so far; cross the 100 ms boundary. Nothing was prefetched (the
    // fake-async test clock never lets the Rust thread pool deliver), so frame 1 is
    // built synchronously for the current second.
    now = 1700000001;
    await tester.pump(const Duration(milliseconds: 30));
    expect(_painter(tester).frame.index, 1);
    expect(
      _painter(tester).frame.sameAs(enc.frame(1, unixSeconds: 1700000001)),
      isTrue,
    );
    expect(_painter(tester).prev?.index, 0);
    expect(_painter(tester).tSinceSwitch, closeTo(0.01, 1e-9));

    // A long gap skips frames: shows the current index, previous is what was visible.
    now = 1700000002;
    await tester.pump(const Duration(milliseconds: 1000));
    expect(_painter(tester).frame.index, 11);
    expect(_painter(tester).prev?.index, 1);
    expect(
      _painter(tester).frame.sameAs(enc.frame(11, unixSeconds: 1700000002)),
      isTrue,
    );
  });

  testWidgets('prefetched frames arrive from the Rust thread pool', (
    tester,
  ) async {
    final enc = _encoder();
    addTearDown(enc.dispose);
    const now = 1700000000;
    await tester.pumpWidget(
      host(ParticleCodeView(encoder: enc, size: 100, unixClock: () => now)),
    );
    await tester.pump(); // first tick requests frames 0..2 (0 is already shown)
    // Let the real event loop deliver the futures.
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 300)),
    );
    await tester.pump(const Duration(milliseconds: 5));

    // Frame 1 became due: it must be the prefetched one, equal to a synchronous build
    // for the predicted second (the unixClock is constant, so second `now`).
    await tester.pump(const Duration(milliseconds: 100));
    expect(_painter(tester).frame.index, 1);
    expect(
      _painter(tester).frame.sameAs(enc.frame(1, unixSeconds: now)),
      isTrue,
    );
    expect(_painter(tester).prev?.index, 0);
  });

  testWidgets('a late prefetch holds the current frame', (tester) async {
    final enc = _encoder();
    addTearDown(enc.dispose);
    await tester.pumpWidget(
      host(
        ParticleCodeView(encoder: enc, size: 100, unixClock: () => 1700000000),
      ),
    );
    await tester.pump();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 300)),
    );
    // Prefetch delivered frames 1 and 2. Jump far ahead: the newest delivered frame is
    // shown, and when nothing newer arrives the view keeps it for a while instead of
    // building synchronously.
    await tester.pump(const Duration(milliseconds: 500));
    expect(_painter(tester).frame.index, 2);
    await tester.pump(const Duration(milliseconds: 100)); // starts waiting
    await tester.pump(const Duration(milliseconds: 200));
    expect(_painter(tester).frame.index, 2, reason: 'still within the limit');
    // After the hold limit (300 ms at 10 fps) it catches up with a synchronous build.
    await tester.pump(const Duration(milliseconds: 200));
    expect(_painter(tester).frame.index, greaterThanOrEqualTo(8));
  });

  testWidgets('disposing the encoder with frames in flight is harmless', (
    tester,
  ) async {
    final enc = _encoder();
    await tester.pumpWidget(host(ParticleCodeView(encoder: enc, size: 100)));
    await tester.pump();
    await tester.pumpWidget(host(const SizedBox()));
    enc.dispose();
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 100)),
    );
    expect(tester.takeException(), isNull);
  });

  testWidgets('the fps of the encoder sets the data frame rate', (
    tester,
  ) async {
    final enc = _encoder(fps: 20);
    addTearDown(enc.dispose);
    await tester.pumpWidget(host(ParticleCodeView(encoder: enc, size: 100)));
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
      host(ParticleCodeView(encoder: enc, size: 100, unixClock: () => now)),
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
    await tester.pumpWidget(host(ParticleCodeView(encoder: enc, size: 100)));
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
    Widget view(bool showing) =>
        host(ParticleCodeView(encoder: enc, size: 100, showing: showing));

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
      host(ParticleCodeView(encoder: enc, size: 100, showing: false)),
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
            ParticleCodeView(encoder: enc, size: 50),
            ParticleCodeView(encoder: enc, size: 50),
          ],
        ),
      ),
    );
    await tester.pump();
    expect(displayCalls, [true]);
    await tester.pumpWidget(
      host(Column(children: [ParticleCodeView(encoder: enc, size: 50)])),
    );
    expect(displayCalls, [true], reason: 'one view still showing');
    await tester.pumpWidget(host(const SizedBox()));
    expect(displayCalls, [true, false]);
  });

  testWidgets('a missing plugin is swallowed', (tester) async {
    messenger.setMockMethodCallHandler(ParticleCodeDisplay.channel, null);
    final enc = _encoder();
    addTearDown(enc.dispose);
    await tester.pumpWidget(host(ParticleCodeView(encoder: enc, size: 100)));
    await tester.pump(const Duration(milliseconds: 150));
    await tester.pumpWidget(host(const SizedBox()));
    expect(tester.takeException(), isNull);
  });

  testWidgets('sizing: explicit size and fill-the-square', (tester) async {
    final enc = _encoder();
    addTearDown(enc.dispose);
    await tester.pumpWidget(host(ParticleCodeView(encoder: enc, size: 123)));
    expect(tester.getSize(find.byType(ParticleCodeView)), const Size(123, 123));
    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: Align(
          child: ConstrainedBox(
            constraints: BoxConstraints.loose(const Size(300, 500)),
            child: ParticleCodeView(encoder: enc),
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
      host(ParticleCodeView(encoder: enc, size: 100, motion: null)),
    );
    expect(_painter(tester).motion, isNull);
  });

  testWidgets('swapping the encoder restarts at frame 0', (tester) async {
    final a = _encoder();
    final b = _encoder();
    addTearDown(a.dispose);
    addTearDown(b.dispose);
    await tester.pumpWidget(host(ParticleCodeView(encoder: a, size: 100)));
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 350));
    expect(_painter(tester).frame.index, 3);
    await tester.pumpWidget(host(ParticleCodeView(encoder: b, size: 100)));
    expect(_painter(tester).frame.index, 0);
    expect(_painter(tester).prev, isNull);
  });

  testWidgets('the previous frame is always the one shown before', (
    tester,
  ) async {
    final enc = _encoder();
    addTearDown(enc.dispose);
    await tester.pumpWidget(host(ParticleCodeView(encoder: enc, size: 100)));
    await tester.pump();
    for (var i = 0; i < 6; i++) {
      await tester.pump(const Duration(milliseconds: 100));
      final p = _painter(tester);
      expect(p.prev?.index, p.frame.index - 1);
    }
  });
}
