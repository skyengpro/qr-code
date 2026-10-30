// The background frame worker: identical frames to the synchronous path, clean shutdown,
// and the view's use of it.
import 'dart:async';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code/particle_code.dart';
import 'package:particle_code/src/encoder.dart' show EncoderWorkerSpec;
import 'package:particle_code/src/ffi/bindings.dart';
import 'package:particle_code/src/frame_worker.dart';

Uint8List _payload(int n) =>
    Uint8List.fromList([for (var i = 0; i < n; i++) (i * 29 + 3) & 0xFF]);

/// Requests [wanted] (index to unix second) and returns the frames that came back.
Future<Map<int, FrameSpec>> _collect(
  EncoderWorkerSpec spec,
  Map<int, int> wanted, {
  int floor = 0,
}) async {
  final got = <int, FrameSpec>{};
  final all = Completer<void>();
  Object? failure;
  final worker = FrameWorker.start(
    spec,
    onFrame: (i, f) {
      got[i] = f;
      if (got.length == wanted.length && !all.isCompleted) all.complete();
    },
    onFailure: (e) {
      failure = e;
      if (!all.isCompleted) all.complete();
    },
  );
  // Wait for the worker to be ready, then ask.
  for (var i = 0; i < 400 && !worker.isReady && failure == null; i++) {
    await Future<void>.delayed(const Duration(milliseconds: 10));
  }
  expect(failure, isNull);
  for (final e in wanted.entries) {
    worker.request(e.key, e.value, floor: floor);
  }
  await all.future.timeout(const Duration(seconds: 20));
  worker.dispose();
  await worker.done.timeout(const Duration(seconds: 10));
  expect(failure, isNull);
  return got;
}

void main() {
  group('FrameSpec message form', () {
    test('round trips every field', () {
      final enc = ParticleEncoder.create(
        _payload(700),
        options: EncoderOptions(
          shape: Shape.heart,
          segmentKbLog2: 0,
          sessionId: 5,
          key: List.generate(32, (i) => i),
          clockOriginUnix: 1700000000,
        ),
      );
      addTearDown(enc.dispose);
      for (final i in [0, 1, 7]) {
        final f = enc.frame(i, unixSeconds: 1700000000 + i);
        final back = FrameSpec.fromMessage(f.toMessage());
        expect(back.sameAs(f), isTrue, reason: 'frame $i');
        expect(back.dots.length, f.dots.length);
        expect(back.dots[123].x, f.dots[123].x);
        expect(back.swatches[8].color, f.swatches[8].color);
        expect(back.header.segmentIndex, f.header.segmentIndex);
      }
      final a = enc.frame(0, unixSeconds: 1);
      final b = enc.frame(0, unixSeconds: 99999);
      expect(a.sameAs(b), isFalse, reason: 'particles differ');
    });
  });

  group('worker output equals the synchronous path', () {
    for (final (name, options) in [
      (
        'fixed identity',
        EncoderOptions(
          profile: CodeProfile.s,
          sessionId: 0x4242,
          key: List.generate(32, (i) => 255 - i),
          clockOriginUnix: 1700000000,
        ),
      ),
      (
        'random identity and clock origin',
        const EncoderOptions(profile: CodeProfile.s),
      ),
      (
        'multi-segment heart, profile M',
        const EncoderOptions(shape: Shape.heart, segmentKbLog2: 0, fps: 20),
      ),
      (
        'automatic profile and a shape that may fall back',
        EncoderOptions(
          profile: null,
          shape: Shape.letter('W').preferred,
          display: const DisplayInfo(),
        ),
      ),
    ]) {
      test(name, () async {
        final enc = ParticleEncoder.create(_payload(2500), options: options);
        addTearDown(enc.dispose);
        final wanted = {
          0: 1700000100,
          1: 1700000100,
          2: 1700000101,
          9: 1700000199,
          enc.framesPerPass + 2: 1700000300,
        };
        final got = await _collect(enc.workerSpec!, wanted);
        expect(got.keys.toSet(), wanted.keys.toSet());
        for (final e in wanted.entries) {
          final expected = enc.frame(e.key, unixSeconds: e.value);
          expect(
            got[e.key]!.sameAs(expected),
            isTrue,
            reason: 'frame ${e.key}',
          );
          expect(got[e.key]!.index, e.key);
        }
      });
    }

    test('stale requests below the floor are dropped', () async {
      final enc = ParticleEncoder.create(
        _payload(300),
        options: const EncoderOptions(profile: CodeProfile.s),
      );
      addTearDown(enc.dispose);
      final spec = enc.workerSpec!;
      final got = <int>{};
      final done = Completer<void>();
      final w = FrameWorker.start(
        spec,
        onFrame: (i, f) {
          got.add(i);
          if (i == 30 && !done.isCompleted) done.complete();
        },
        onFailure: (e) => fail('$e'),
      );
      while (!w.isReady) {
        await Future<void>.delayed(const Duration(milliseconds: 5));
      }
      for (var i = 0; i <= 30; i++) {
        w.request(i, 1700000000, floor: 30);
      }
      await done.future.timeout(const Duration(seconds: 20));
      w.dispose();
      await w.done;
      expect(got, {30});
    });

    test('createWith bindings cannot be mirrored', () {
      final enc = ParticleEncoder.createWith(
        ParticleBindings.instance,
        _payload(10),
      );
      addTearDown(enc.dispose);
      expect(enc.workerSpec, isNull);
    });
  });

  group('lifecycle', () {
    test('dispose frees the isolate and is idempotent', () async {
      final enc = ParticleEncoder.create(_payload(100));
      addTearDown(enc.dispose);
      final before = FrameWorker.liveCount;
      final w = FrameWorker.start(
        enc.workerSpec!,
        onFrame: (_, _) {},
        onFailure: (e) => fail('$e'),
      );
      expect(FrameWorker.liveCount, before + 1);
      while (!w.isReady) {
        await Future<void>.delayed(const Duration(milliseconds: 5));
      }
      w.dispose();
      w.dispose();
      expect(w.isDisposed, isTrue);
      expect(w.isReady, isFalse);
      await w.done.timeout(const Duration(seconds: 10));
      expect(FrameWorker.liveCount, before);
      // Requests after dispose are ignored.
      w.request(0, 0, floor: 0);
    });

    test('dispose right after start, before the isolate is up', () async {
      final enc = ParticleEncoder.create(_payload(100));
      addTearDown(enc.dispose);
      final before = FrameWorker.liveCount;
      var frames = 0;
      final w = FrameWorker.start(
        enc.workerSpec!,
        onFrame: (_, _) => frames++,
        onFailure: (e) => fail('$e'),
      );
      w.dispose();
      await w.done.timeout(const Duration(seconds: 10));
      expect(FrameWorker.liveCount, before);
      expect(frames, 0);
    });

    test('a spec the library rejects reports a failure and exits', () async {
      final failures = <Object>[];
      final w = FrameWorker.start(
        EncoderWorkerSpec.fromMessage([_payload(10), '{"fps": "nope"}']),
        onFrame: (_, _) => fail('no frames expected'),
        onFailure: failures.add,
      );
      await w.done.timeout(const Duration(seconds: 10));
      expect(failures, hasLength(1));
      expect(failures.single.toString(), contains('invalid'));
      expect(w.isReady, isFalse);
    });
  });

  group('ParticleCodeView with the worker', () {
    ParticleCodePainter painterOf(WidgetTester tester) {
      final paint = tester.widget<CustomPaint>(
        find.descendant(
          of: find.byType(ParticleCodeView),
          matching: find.byType(CustomPaint),
        ),
      );
      return paint.painter! as ParticleCodePainter;
    }

    Widget host(Widget child) => Directionality(
      textDirection: TextDirection.ltr,
      child: Center(child: child),
    );

    ParticleEncoder makeEncoder() => ParticleEncoder.create(
      _payload(500),
      options: EncoderOptions(
        profile: CodeProfile.s,
        sessionId: 77,
        key: List.generate(32, (i) => i + 9),
        clockOriginUnix: 1700000000,
      ),
    );

    /// Advance the fake data clock by one data frame, giving the worker real time.
    Future<void> step(WidgetTester tester) async {
      await tester.pump(const Duration(milliseconds: 100));
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 70)),
      );
    }

    testWidgets('frames come from the worker, not the UI isolate', (
      tester,
    ) async {
      final enc = makeEncoder();
      addTearDown(enc.dispose);
      await tester.pumpWidget(
        host(
          ParticleCodeView(
            encoder: enc,
            size: 120,
            unixClock: () => 1700000000,
          ),
        ),
      );
      expect(FrameWorker.liveCount, greaterThan(0));
      final shown = <int, FrameSpec>{};
      var callsAtFive = -1;
      for (var i = 0; i < 80 && painterOf(tester).frame.index < 14; i++) {
        await step(tester);
        final p = painterOf(tester);
        shown[p.frame.index] = p.frame;
        if (callsAtFive < 0 && p.frame.index >= 5) {
          callsAtFive = enc.debugFrameCalls;
        }
      }
      expect(painterOf(tester).frame.index, greaterThanOrEqualTo(14));
      expect(
        enc.debugFrameCalls,
        callsAtFive,
        reason: 'after start-up the UI isolate builds no more frames',
      );

      // Every frame shown after that equals the synchronous frame for the same index.
      var compared = 0;
      for (final e in shown.entries.where((e) => e.key >= 5)) {
        final candidates = [
          for (final s in [1699999999, 1700000000, 1700000001])
            enc.frame(e.key, unixSeconds: s),
        ];
        expect(
          candidates.any((c) => c.sameAs(e.value)),
          isTrue,
          reason: 'frame ${e.key}',
        );
        compared++;
      }
      expect(compared, greaterThan(5));

      // The previous frame is still the one before.
      final p = painterOf(tester);
      expect(p.prev, isNotNull);
      expect(p.prev!.index, lessThan(p.frame.index));
    });

    testWidgets('disposing the view disposes the worker', (tester) async {
      final enc = makeEncoder();
      addTearDown(enc.dispose);
      final before = FrameWorker.liveCount;
      await tester.pumpWidget(host(ParticleCodeView(encoder: enc, size: 80)));
      expect(FrameWorker.liveCount, before + 1);
      await step(tester);
      await tester.pumpWidget(host(const SizedBox()));
      await tester.runAsync(() async {
        for (var i = 0; i < 200 && FrameWorker.liveCount > before; i++) {
          await Future<void>.delayed(const Duration(milliseconds: 10));
        }
      });
      expect(FrameWorker.liveCount, before);
    });

    testWidgets('showing: false stops the worker, true restarts it', (
      tester,
    ) async {
      final enc = makeEncoder();
      addTearDown(enc.dispose);
      final before = FrameWorker.liveCount;
      Widget view(bool showing) =>
          host(ParticleCodeView(encoder: enc, size: 80, showing: showing));
      await tester.pumpWidget(view(true));
      expect(FrameWorker.liveCount, before + 1);
      await tester.pumpWidget(view(false));
      await tester.runAsync(() async {
        for (var i = 0; i < 200 && FrameWorker.liveCount > before; i++) {
          await Future<void>.delayed(const Duration(milliseconds: 10));
        }
      });
      expect(FrameWorker.liveCount, before);
      await tester.pumpWidget(view(true));
      expect(FrameWorker.liveCount, before + 1);
      await tester.pumpWidget(host(const SizedBox()));
      await tester.runAsync(() async {
        for (var i = 0; i < 200 && FrameWorker.liveCount > before; i++) {
          await Future<void>.delayed(const Duration(milliseconds: 10));
        }
      });
      expect(FrameWorker.liveCount, before);
    });

    testWidgets('an encoder without a worker spec stays synchronous', (
      tester,
    ) async {
      final enc = ParticleEncoder.createWith(
        ParticleBindings.instance,
        _payload(100),
      );
      addTearDown(enc.dispose);
      final before = FrameWorker.liveCount;
      await tester.pumpWidget(host(ParticleCodeView(encoder: enc, size: 80)));
      expect(FrameWorker.liveCount, before);
      await tester.pump();
      for (var i = 0; i < 4; i++) {
        await tester.pump(const Duration(milliseconds: 100));
      }
      expect(painterOf(tester).frame.index, 4);
      expect(enc.debugFrameCalls, 5);
    });

    testWidgets('useBackgroundIsolate: false never starts a worker', (
      tester,
    ) async {
      final enc = makeEncoder();
      addTearDown(enc.dispose);
      final before = FrameWorker.liveCount;
      await tester.pumpWidget(
        host(
          ParticleCodeView(encoder: enc, size: 80, useBackgroundIsolate: false),
        ),
      );
      expect(FrameWorker.liveCount, before);
    });
  });
}
