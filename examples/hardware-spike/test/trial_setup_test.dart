import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code_hardware_spike/app.dart';
import 'package:particle_code_hardware_spike/app_state.dart';
import 'package:particle_code_hardware_spike/bootstrap.dart';
import 'package:particle_code_hardware_spike/model/trial_config.dart';
import 'package:particle_code_hardware_spike/model/trial_setup.dart';
import 'package:particle_code_hardware_spike/share/result_sharer.dart';

import 'support/fakes.dart';

const _used = TrialSetup(
  role: TrialRole.sender,
  fixtureId: 'f200',
  profile: 'S',
  shapeId: 30,
  fps: 5,
  isStatic: true,
  repairPerPass: 4,
  motion: MotionChoice.stationary,
  distanceCm: '30',
  codeSizeMm: '52',
  lighting: 'office',
  screenProtector: 'glass',
);

void main() {
  group('TrialSetup JSON', () {
    test('round-trips every field', () {
      final back = TrialSetup.fromJson(_used.toJson());
      expect(back.toJson(), _used.toJson());
    });

    test('a missing, partial or garbage file falls back to the defaults', () {
      const d = TrialSetup();
      expect(TrialSetup.fromJson(null).toJson(), d.toJson());
      expect(TrialSetup.fromJson('nonsense').toJson(), d.toJson());
      final partial = TrialSetup.fromJson({
        'fps': 'fast',
        'profile': 'XL',
        'fixture_id': 'f6k',
      });
      expect(partial.fps, d.fps);
      expect(partial.profile, d.profile);
      expect(partial.fixtureId, 'f6k');
    });
  });

  late Directory dir;
  setUp(() => dir = Directory.systemTemp.createTempSync('spike_setup'));
  tearDown(() => dir.deleteSync(recursive: true));

  Future<SpikeState> boot(WidgetTester tester) async => (await tester.runAsync(
    () => bootstrap(
      supportDir: Directory('${dir.path}/support'),
      documentsDir: Directory('${dir.path}/docs'),
      sharer: FakeResultSharer(const ShareOutcome.dismissed()),
      scannerFactory: FakeScannerPort.new,
      identityProvider: FakeIdentityProvider(),
    ),
  ))!;

  testWidgets('the last setup survives an app restart', (tester) async {
    final first = await boot(tester);
    await tester.runAsync(() => first.saveLastSetup(_used));
    final second = await boot(tester);
    expect(second.lastSetup.toJson(), _used.toJson());
  });

  testWidgets(
    'New trial opens with the last setup, and Reset restores defaults',
    (tester) async {
      tester.view.physicalSize = const Size(800, 3000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final state = await boot(tester);
      await tester.runAsync(() => state.saveLastSetup(_used));
      await tester.pumpWidget(SpikeApp(state: state));
      await tester.tap(find.byKey(const Key('new-trial')));
      await tester.pumpAndSettle();

      bool chip(String key) =>
          tester.widget<ChoiceChip>(find.byKey(Key(key))).selected;
      Set<T> segments<T>() => tester
          .widget<SegmentedButton<T>>(find.byType(SegmentedButton<T>))
          .selected;

      expect(segments<TrialRole>(), {TrialRole.sender});
      expect(chip('fixture-f200'), isTrue);
      expect(segments<String>(), {'S'});
      expect(segments<bool>(), {true});
      expect(segments<MotionChoice>(), {MotionChoice.stationary});
      expect(find.text('30'), findsOneWidget);
      expect(find.text('glass'), findsOneWidget);

      await tester.tap(find.byKey(const Key('reset-setup')));
      await tester.pumpAndSettle();
      expect(segments<TrialRole>(), {TrialRole.receiver});
      expect(chip('fixture-f1k'), isTrue);
      expect(segments<String>(), {'M'});
      expect(find.text('glass'), findsNothing);
    },
  );

  testWidgets('creating a trial remembers the form for the next one', (
    tester,
  ) async {
    tester.view.physicalSize = const Size(800, 3000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final state = await boot(tester);
    await tester.pumpWidget(SpikeApp(state: state));
    await tester.tap(find.byKey(const Key('new-trial')));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const Key('fixture-f6k')));
    await tester.tap(find.text('L'));
    await tester.pumpAndSettle();
    await tester.tap(find.byKey(const Key('create-trial')));
    await tester.pumpAndSettle();

    expect(state.lastSetup.fixtureId, 'f6k');
    expect(state.lastSetup.profile, 'L');
    expect(state.lastSetup.role, TrialRole.receiver);
  });
}
