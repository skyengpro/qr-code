// Runs the real Rust library on the host. Build it first:
//   cargo build -p particlecode-ffi
// (or point $PARTICLECODE_LIB at it).
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code/particle_code.dart';
import 'package:particle_code/src/ffi/bindings.dart';
import 'package:particle_code/src/ffi/library_loader.dart';

Uint8List _payload(int n) =>
    Uint8List.fromList([for (var i = 0; i < n; i++) (i * 31 + 7) & 0xFF]);

final _key = List<int>.generate(32, (i) => i + 1);

EncoderOptions _fixed({
  CodeProfile? profile = CodeProfile.m,
  Shape shape = Shape.disc,
  int segmentKbLog2 = 3,
  int fps = 10,
}) => EncoderOptions(
  profile: profile,
  shape: shape,
  segmentKbLog2: segmentKbLog2,
  fps: fps,
  sessionId: 0x1234,
  key: _key,
  clockOriginUnix: 1700000000,
);

void main() {
  test('library is found and speaks the expected ABI', () {
    expect(
      findHostLibrary(),
      isNotNull,
      reason: 'run cargo build -p particlecode-ffi',
    );
    final b = ParticleBindings.instance;
    expect(b.abiVersion(), expectedAbiVersion);
  });

  group('create', () {
    for (final profile in CodeProfile.values) {
      test('profile ${profile.letter}: info and frame agree', () {
        final enc = ParticleEncoder.create(
          _payload(300),
          options: _fixed(profile: profile),
        );
        addTearDown(enc.dispose);
        final info = enc.info;
        expect(info.profile, profile);
        expect(info.sessionId, 0x1234);
        expect(enc.sessionId, 0x1234);
        expect(info.fps, 10);
        expect(info.shapeId, 0);
        expect(info.segmentCount, 1);
        expect(enc.framesPerPass, info.framesPerPass);
        expect(enc.framesPerPass, greaterThan(0));
        expect(enc.key, _key);

        final f = enc.frame(0);
        expect(f.profile, profile);
        expect(f.header.profile, profile);
        expect(f.header.sessionId, 0x1234);
        expect(f.header.payloadLen, 300);
        expect(f.dots.length, info.dotCount);
        expect(f.headerDots.length, 48);
        expect(f.swatches.length, 48);
        expect(f.anchors.length, 4);
        expect(f.particles.length, 6);
        expect(f.frameHalfSide, closeTo(info.frameHalfSide, 1e-12));
        expect(f.anchors[0].pupilR, greaterThan(0));
        expect(f.anchors[1].pupilR, 0);
        // Dot density grows with the profile.
        final expectedMin = {
          CodeProfile.s: 1000,
          CodeProfile.m: 2500,
          CodeProfile.l: 4500,
        }[profile]!;
        expect(info.dotCount, greaterThan(expectedMin));
        // Every dot is inside the frame.
        for (final d in f.dots) {
          expect(d.x.abs(), lessThanOrEqualTo(f.frameHalfSide));
          expect(d.y.abs(), lessThanOrEqualTo(f.frameHalfSide));
          expect(d.color, inInclusiveRange(0, 3));
          expect(d.size, inInclusiveRange(0, 1));
        }
      });
    }

    test('multi-segment payloads', () {
      final enc = ParticleEncoder.create(
        _payload(9000),
        options: _fixed(segmentKbLog2: 1),
      );
      addTearDown(enc.dispose);
      expect(enc.info.segmentCount, greaterThan(1));
      expect(enc.framesPerPass, greaterThanOrEqualTo(enc.info.segmentCount));
      final segs = {
        for (var i = 0; i < enc.framesPerPass; i++)
          enc.frame(i).header.segmentIndex,
      };
      expect(segs.length, enc.info.segmentCount);
    });

    test('shapes', () {
      for (final shape in [
        Shape.heart,
        Shape.hexagon,
        Shape.ring,
        Shape.roundedSquare,
        Shape.letter('A'),
        Shape.letter('z'),
        Shape.heart.preferred,
        Shape.auto([Shape.letter('Q'), Shape.heart]),
      ]) {
        final enc = ParticleEncoder.create(
          _payload(100),
          options: _fixed(shape: shape, profile: CodeProfile.l),
        );
        addTearDown(enc.dispose);
        final f = enc.frame(0);
        expect(f.header.shapeId, enc.info.shapeId, reason: '$shape');
        expect(enc.info.dotCount, f.dots.length);
      }
      final heart = ParticleEncoder.create(
        _payload(100),
        options: _fixed(shape: Shape.heart, profile: CodeProfile.l),
      );
      addTearDown(heart.dispose);
      expect(heart.info.shapeId, 28);
      final disc = ParticleEncoder.create(
        _payload(100),
        options: _fixed(profile: CodeProfile.l),
      );
      addTearDown(disc.dispose);
      expect(heart.info.dotCount, lessThan(disc.info.dotCount));
    });

    test('profile auto-selection from the display', () {
      final enc = ParticleEncoder.create(
        _payload(100),
        options: const EncoderOptions(profile: null, display: DisplayInfo()),
      );
      addTearDown(enc.dispose);
      expect(CodeProfile.values, contains(enc.info.profile));
    });

    test('random identity when not fixed', () {
      final a = ParticleEncoder.create(_payload(50));
      final b = ParticleEncoder.create(_payload(50));
      addTearDown(a.dispose);
      addTearDown(b.dispose);
      expect(a.key.length, 32);
      expect(a.key, isNot(b.key));
    });
  });

  group('determinism', () {
    test('same options give identical frames', () {
      final a = ParticleEncoder.create(_payload(500), options: _fixed());
      final b = ParticleEncoder.create(_payload(500), options: _fixed());
      addTearDown(a.dispose);
      addTearDown(b.dispose);
      for (final i in [0, 1, 5, a.framesPerPass + 3]) {
        expect(a.frameJson(i), b.frameJson(i), reason: 'frame $i');
        expect(
          a.frameJson(i, unixSeconds: 1700000042),
          b.frameJson(i, unixSeconds: 1700000042),
        );
      }
    });

    test('unixSeconds changes the particles only', () {
      final enc = ParticleEncoder.create(_payload(500), options: _fixed());
      addTearDown(enc.dispose);
      final a = enc.frame(0, unixSeconds: 1700000000);
      final b = enc.frame(0, unixSeconds: 1700009999);
      expect(a.dots.length, b.dots.length);
      for (var i = 0; i < a.dots.length; i++) {
        expect(a.dots[i].color, b.dots[i].color);
        expect(a.dots[i].size, b.dots[i].size);
      }
      expect([
        for (final p in a.particles) p.sector,
      ], isNot([for (final p in b.particles) p.sector]));
    });

    test('frames beyond one pass keep going', () {
      final enc = ParticleEncoder.create(_payload(500), options: _fixed());
      addTearDown(enc.dispose);
      final far = enc.frame(enc.framesPerPass * 3 + 1);
      expect(far.index, enc.framesPerPass * 3 + 1);
    });
  });

  group('errors', () {
    test('invalid segment size is reported with the native text', () {
      expect(
        () => ParticleEncoder.create(
          _payload(10),
          options: _fixed(segmentKbLog2: 99),
        ),
        throwsA(
          isA<ParticleCodeException>().having(
            (e) => e.message,
            'message',
            isNotEmpty,
          ),
        ),
      );
    });

    test('unknown shape id', () {
      expect(
        () => ParticleEncoder.create(
          _payload(10),
          options: _fixed(shape: const Shape(200)),
        ),
        throwsA(isA<ParticleCodeException>()),
      );
    });

    test('empty payload is a valid, tiny transfer', () {
      final enc = ParticleEncoder.create(Uint8List(0), options: _fixed());
      addTearDown(enc.dispose);
      expect(enc.frame(0).header.payloadLen, 0);
    });

    test('wrong key length', () {
      expect(
        () => ParticleEncoder.create(
          _payload(10),
          options: const EncoderOptions(key: [1, 2, 3]),
        ),
        throwsA(isA<ParticleCodeException>()),
      );
    });

    test('non-finite options never reach the library', () {
      expect(
        () => ParticleEncoder.create(
          _payload(10),
          options: const EncoderOptions(minPitchMm: double.nan),
        ),
        throwsA(isA<ParticleCodeException>()),
      );
    });

    test('use after dispose', () {
      final enc = ParticleEncoder.create(_payload(10), options: _fixed());
      expect(enc.isDisposed, isFalse);
      enc.dispose();
      enc.dispose(); // idempotent
      expect(enc.isDisposed, isTrue);
      expect(() => enc.frame(0), throwsStateError);
      expect(() => enc.framesPerPass, throwsStateError);
    });

    test('negative frame index', () {
      final enc = ParticleEncoder.create(_payload(10), options: _fixed());
      addTearDown(enc.dispose);
      expect(() => enc.frame(-1), throwsRangeError);
    });

    test('bad shape letters', () {
      expect(() => Shape.letter('1'), throwsArgumentError);
      expect(() => Shape.letter('AB'), throwsArgumentError);
    });
  });

  group('liveness', () {
    test('verify accepts the encoder clock and rejects others', () {
      final enc = ParticleEncoder.create(_payload(200), options: _fixed());
      addTearDown(enc.dispose);
      // The rolling value lives in the particles and is not exposed as a number, so
      // this checks the call round trip and the argument validation.
      expect(
        () => livenessVerify(const [1, 2], 0, nowUnix: 0),
        throwsArgumentError,
      );
      // With a huge skew some second always matches.
      expect(
        livenessVerify(enc.key, 0, nowUnix: 1700000000, skew: 70000),
        isTrue,
      );
    });
  });

  group('options json', () {
    test('defaults mirror the C ABI config', () {
      final j = const EncoderOptions().toJson();
      expect(j['profile'], 'M');
      expect(j['shape'], {'kind': 'require', 'id': 0});
      expect(j['segment_kb_log2'], 3);
      expect(j['fps'], 10);
      expect(j['is_static'], false);
      expect(j['repair_per_pass'], 2);
      expect(j.containsKey('session_id'), isFalse);
      expect(j.containsKey('key'), isFalse);
    });

    test('everything set', () {
      final j = EncoderOptions(
        profile: null,
        shape: Shape.auto([Shape.heart, Shape.letter('B')]),
        sessionId: 7,
        key: _key,
        clockOriginUnix: 5,
        isStatic: true,
        display: const DisplayInfo(
          widthPx: 1,
          heightPx: 2,
          widthMm: 3,
          heightMm: 4,
        ),
      ).toJson();
      expect(j['profile'], isNull);
      expect(j['shape'], {
        'kind': 'auto',
        'ids': [28, 3],
      });
      expect(j['session_id'], 7);
      expect(j['clock_origin_unix'], 5);
      expect(j['is_static'], true);
      expect(j['display'], {
        'width_px': 1,
        'height_px': 2,
        'width_mm': 3.0,
        'height_mm': 4.0,
      });
    });

    test('shape ids match the core table', () {
      expect(Shape.disc.id, 0);
      expect(Shape.roundedSquare.id, 1);
      expect(Shape.letter('A').id, 2);
      expect(Shape.letter('Z').id, 27);
      expect(Shape.heart.id, 28);
      expect(Shape.hexagon.id, 29);
      expect(Shape.ring.id, 30);
      expect(Shape.heart.preferred.toJson(), {'kind': 'prefer', 'id': 28});
    });
  });
}
