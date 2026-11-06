// Start-up of the sender. Lives in its own file because each test file runs in its own
// isolate, so the library is still uninitialized at the start.
import 'dart:typed_data';

import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code/particle_code.dart';

void main() {
  test('using the sender before init is a StateError that says what to do', () {
    expect(ParticleCode.isInitialized, isFalse);
    final matcher = throwsA(
      isA<StateError>().having(
        (e) => e.message,
        'message',
        contains('await ParticleCode.init()'),
      ),
    );
    expect(() => ParticleEncoder.create(Uint8List(4)), matcher);
    expect(() => livenessVerify(List.filled(32, 0), 0, nowUnix: 0), matcher);
  });

  test('init is idempotent, also when called concurrently', () async {
    final a = ParticleCode.init();
    final b = ParticleCode.init();
    await Future.wait([a, b]);
    expect(ParticleCode.isInitialized, isTrue);
    await ParticleCode.init(); // a no-op
    await ParticleCode.init();
    expect(ParticleCode.isInitialized, isTrue);
  });

  test('works after init', () {
    final enc = ParticleEncoder.create(Uint8List.fromList([1, 2, 3]));
    addTearDown(enc.dispose);
    expect(enc.frame(0).dots, isNotEmpty);
    expect(
      livenessVerify(enc.key, 0, nowUnix: 1700000000, skew: 70000),
      isTrue,
    );
  });
}
