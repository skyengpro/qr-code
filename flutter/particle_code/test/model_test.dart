import 'dart:convert';
import 'dart:io';

import 'package:flutter/painting.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code/particle_code.dart';
import 'package:particle_code/src/native_library.dart';
import 'package:particle_code/src/palette.dart';

void main() {
  test('palette is the exact core palette', () {
    // particlecode_core::profile::colors
    expect(
      ParticleCodePalette.hues.map((c) => c.toARGB32() & 0xFFFFFF).toList(),
      [
        (20 << 16) | (110 << 8) | 245,
        (40 << 16) | (165 << 8) | 30,
        (235 << 16) | (100 << 8) | 0,
        (215 << 16) | (40 << 8) | 175,
      ],
    );
    expect(ParticleCodePalette.background.toARGB32(), 0xFFF6F4EF);
    expect(ParticleCodePalette.anchor.toARGB32(), 0xFF181C34);
    expect(ParticleCodePalette.white.toARGB32(), 0xFFFFFFFF);
    expect(ParticleCodePalette.black.toARGB32(), 0xFF0C0C10);
    expect(ParticleCodePalette.particle.toARGB32(), 0xFF787A80);
  });

  test('color helpers are exact at the ends and symmetric in the middle', () {
    final a = ParticleCodePalette.hues[0];
    final b = ParticleCodePalette.hues[2];
    expect(lerpRgb(a, b, 0), a);
    expect(lerpRgb(a, b, 1), b);
    expect(lerpHue(a, b, 0), a);
    expect(lerpHue(a, b, 1), b);
    expect(lerpRgb(a, b, 0.5), const Color.fromARGB(255, 128, 105, 123));
    for (final c in ParticleCodePalette.hues) {
      // HSV round trip of the palette is exact (same as the Rust test).
      expect(lerpHue(c, c, 0.37), c);
    }
    expect(
      lighten(ParticleCodePalette.hues[1], 0),
      ParticleCodePalette.hues[1],
    );
  });

  test('FrameSpec parses the serde JSON', () {
    final json =
        jsonDecode('''
{"index":3,
 "header":{"shape_table_version":0,"profile":"S","palette":"Hues4","is_static":false,
   "shape_id":28,"session_id":4660,"payload_len":10,"segment_kb_log2":3,
   "segment_index":2,"packet_seed":7},
 "profile":"S","frame_half_side":30.5,
 "particle_band":{"inner":25.0,"outer":29.0},
 "anchors":[{"at":{"x":0.0,"y":-20.0},"core_r":0.7,"ring_in":1.3,"ring_out":2.15,"pupil_r":0.25}],
 "swatches":[{"at":{"x":1.0,"y":2.0},"color":{"Hue":2},"anchor":0,"slot":1},
             {"at":{"x":1.0,"y":2.0},"color":"White","anchor":0,"slot":0},
             {"at":{"x":1.0,"y":2.0},"color":"Black","anchor":0,"slot":8}],
 "header_dots":[{"q":0,"r":0,"at":{"x":-1.5,"y":2},"color":3,"size":1}],
 "dots":[{"q":-2,"r":5,"at":{"x":0.5,"y":4.33},"color":1,"size":0}],
 "particles":[{"sector":9,"angle":-1.2}]}
''')
            as Map<String, Object?>;
    final f = FrameSpec.fromJson(json);
    expect(f.index, 3);
    expect(f.profile, CodeProfile.s);
    expect(f.header.shapeId, 28);
    expect(f.header.sessionId, 4660);
    expect(f.header.segmentIndex, 2);
    expect(f.header.packetSeed, 7);
    expect(f.frameHalfSide, 30.5);
    expect(f.particleBand.mid, 27.0);
    expect(f.anchors.single.at, const Offset(0, -20));
    expect(f.anchors.single.pupilR, 0.25);
    expect(f.swatches.map((s) => s.color.kind), [
      SwatchKind.hue,
      SwatchKind.white,
      SwatchKind.black,
    ]);
    expect(f.swatches.first.color.hue, 2);
    expect(f.swatches.first.color.color, ParticleCodePalette.hues[2]);
    expect(f.swatches[1].color.color, ParticleCodePalette.white);
    expect(f.swatches[2].color.color, ParticleCodePalette.black);
    expect(f.headerDots.single.y, 2.0);
    expect(f.dots.single.q, -2);
    expect(f.dots.single.radius, CodeDims.dotRSmall);
    expect(f.headerDots.single.radius, CodeDims.dotRLarge);
    expect(f.particles.single.sector, 9);
    expect(f.halfExtent(2.5), 33.0);
    expect(
      f.halfExtent(0),
      32.0,
      reason: 'margin is raised to the 1.5 quiet zone',
    );
    // Immutable lists.
    expect(() => f.dots.add(f.dots.first), throwsUnsupportedError);
  });

  test('malformed frames are format errors', () {
    expect(() => SwatchColor.fromJson('Pink'), throwsFormatException);
    expect(() => CodeProfile.fromLetter('X'), throwsFormatException);
  });

  group('library lookup', () {
    test('PARTICLECODE_LIB wins', () {
      expect(
        findHostLibrary(environment: {libraryEnvVar: '/some/where/lib.so'}),
        '/some/where/lib.so',
      );
    });

    test('searches upwards for the workspace target dir', () {
      final tmp = Directory.systemTemp.createTempSync('pc_lib_');
      addTearDown(() => tmp.deleteSync(recursive: true));
      final name = hostLibraryName(Platform.operatingSystem);
      final dbg = Directory('${tmp.path}/target/debug')
        ..createSync(recursive: true);
      final rel = Directory('${tmp.path}/target/release')
        ..createSync(recursive: true);
      File('${dbg.path}/$name').writeAsBytesSync([0]);
      final nested = Directory('${tmp.path}/a/b/c')
        ..createSync(recursive: true);
      expect(
        findHostLibrary(start: nested, environment: const {}),
        '${dbg.path}/$name',
      );
      // The newer of debug and release is preferred.
      final relFile = File('${rel.path}/$name')..writeAsBytesSync([0]);
      relFile.setLastModifiedSync(
        DateTime.now().add(const Duration(minutes: 5)),
      );
      expect(
        findHostLibrary(start: nested, environment: const {}),
        '${rel.path}/$name',
      );
    });

    test('nothing found is null', () {
      final tmp = Directory.systemTemp.createTempSync('pc_none_');
      addTearDown(() => tmp.deleteSync(recursive: true));
      // The temp dir's ancestors may hold a target dir; only assert the shape.
      final r = findHostLibrary(start: tmp, environment: const {});
      expect(r == null || File(r).existsSync(), isTrue);
    });

    test('library names per OS', () {
      expect(hostLibraryName('linux'), 'libparticlecode_ffi.so');
      expect(hostLibraryName('macos'), 'libparticlecode_ffi.dylib');
      expect(hostLibraryName('windows'), 'particlecode_ffi.dll');
    });
  });
}
