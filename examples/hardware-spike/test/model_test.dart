import 'dart:convert';

import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code_hardware_spike/fixtures.dart';
import 'package:particle_code_hardware_spike/model/device_identity.dart';
import 'package:particle_code_hardware_spike/model/observed.dart';
import 'package:particle_code_hardware_spike/model/stat.dart';
import 'package:particle_code_hardware_spike/model/trial.dart';
import 'package:particle_code_hardware_spike/model/trial_config.dart';
import 'package:particle_code_hardware_spike/trial/trial_factory.dart';
import 'dart:math';

import 'support/fakes.dart';
import 'support/trials.dart';

Object? _roundTrip(Object? json) => jsonDecode(jsonEncode(json));

void main() {
  group('Observed', () {
    test('unknown is an explicit null with source "unavailable"', () {
      expect(const Observed<String>.unknown().toJson(), {
        'value': null,
        'source': 'unavailable',
      });
      final back = Observed.fromJson<String>({
        'value': null,
        'source': 'unavailable',
      });
      expect(back.isUnknown, isTrue);
      expect(back.source, ValueSource.unknown);
    });

    test('measured and manual round trip with their source', () {
      for (final o in [
        const Observed<int>.measured(7),
        const Observed<int>.manual(8),
      ]) {
        final back = Observed.fromJson<int>(_roundTrip(o.toJson()));
        expect(back.value, o.value);
        expect(back.source, o.source);
      }
    });

    test('manualOrUnknown treats blank text as unknown', () {
      expect(Observed<String>.manualOrUnknown('  ').isUnknown, isTrue);
      expect(
        Observed<String>.manualOrUnknown('dim').source,
        ValueSource.manual,
      );
    });

    test('malformed input becomes unknown, never throws', () {
      expect(Observed.fromJson<int>('nope').isUnknown, isTrue);
      expect(
        Observed.fromJson<int>({'value': 'x', 'source': 'measured'}).isUnknown,
        isTrue,
      );
    });
  });

  group('Trial JSON', () {
    test('receiver trial with unknowns round trips', () {
      final t = receiverTrial(
        'T-1',
        TrialStatus.incomplete,
        withFrames: true,
        conditions: Conditions(
          distanceCm: const Observed<double>.manual(25),
          lighting: Observed<String>.manual('office, "bright"'),
        ),
      );
      final json = _roundTrip(t.toJson()) as Map<String, Object?>;
      final back = Trial.fromJson(json);
      expect(jsonEncode(back.toJson()), jsonEncode(t.toJson()));
      expect(back.conditions.codeSizeMm.isUnknown, isTrue);
      expect(back.conditions.codeSizeMm.toJson()['source'], 'unavailable');
      expect(back.conditions.distanceCm.source, ValueSource.manual);
      expect(back.receiver!.frames.length, 2);
      expect(back.receiver!.completionMs.isUnknown, isTrue);
    });

    test('sender trial round trips', () {
      final t = senderTrial('T-2');
      final back = Trial.fromJson(
        _roundTrip(t.toJson()) as Map<String, Object?>,
      );
      expect(jsonEncode(back.toJson()), jsonEncode(t.toJson()));
      expect(back.sender!.logicalHeight.isUnknown, isTrue);
      expect(back.sender!.brightnessPolicy, contains('not read back'));
    });

    test('a non-trial map is rejected', () {
      expect(() => Trial.fromJson({'x': 1}), throwsFormatException);
    });
  });

  group('device identity', () {
    test('every key is present, missing ones unknown', () {
      final d = DeviceIdentity({
        'model_identifier': const Observed<Object>.measured('X'),
      });
      final json = d.toJson();
      expect(json.keys, DeviceIdentity.keys);
      expect((json['physical_ram_mb'] as Map)['source'], 'unavailable');
      final back = DeviceIdentity.fromJson(_roundTrip(json));
      expect(back.text('model_identifier'), 'X');
      expect(back.fields['physical_ram_mb']!.isUnknown, isTrue);
    });

    test('has no persistent identifier fields', () {
      const banned = [
        'android_id',
        'identifier_for_vendor',
        'serial',
        'name',
        'fingerprint',
        'imei',
      ];
      for (final k in DeviceIdentity.keys) {
        for (final b in banned) {
          expect(
            k == b || k.contains('serial') || k.contains('vendor'),
            isFalse,
            reason: k,
          );
        }
      }
    });
  });

  group('fixtures and ids', () {
    test('are deterministic with the declared sizes and distinct hashes', () {
      expect(Fixtures.all.map((f) => f.byteCount), [200, 1024, 6144, 30720]);
      for (final f in Fixtures.all) {
        expect(f.bytes.length, f.byteCount);
        expect(f.bytes, f.bytes);
        expect(f.sha256Hex.length, 64);
      }
      expect(Fixtures.all.map((f) => f.sha256Hex).toSet().length, 4);
      expect(Fixtures.byId('f6k'), same(Fixtures.f6k));
      expect(Fixtures.byId('nope'), isNull);
    });

    test('trial ids are readable: T-yyyyMMdd-HHmmss-xxxx', () {
      final id = TrialFactory.newId(DateTime(2026, 3, 9, 7, 5, 2), Random(1));
      expect(id, matches(RegExp(r'^T-20260309-070502-[0-9a-f]{4}$')));
    });
  });

  test('Stat uses nearest-rank percentiles', () {
    final s = Stat.of([5, 1, 4, 2, 3, 10, 9, 8, 7, 6].map((e) => e * 1.0));
    expect(s.p50, 5);
    expect(s.p90, 9);
    expect(s.max, 10);
    expect(Stat.of(const []).p50, isNull);
    expect(Stat.fromJson(s.toJson()).p90, 9);
  });

  test('ECC follows static / animated', () {
    expect(testConfig().ecc, 'RS(255,191)');
    final s = TrialConfig.fromJson({...testConfig().toJson(), 'static': true});
    expect(s.ecc, 'RS(255,159)');
  });
}
