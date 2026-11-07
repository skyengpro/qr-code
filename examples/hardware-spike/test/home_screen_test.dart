import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code/particle_code.dart';
import 'package:particle_code_hardware_spike/app.dart';
import 'package:particle_code_hardware_spike/app_state.dart';
import 'package:particle_code_hardware_spike/bootstrap.dart';
import 'package:particle_code_hardware_spike/fixtures.dart';
import 'package:particle_code_hardware_spike/model/measurements.dart';
import 'package:particle_code_hardware_spike/model/trial.dart';
import 'package:particle_code_hardware_spike/share/result_sharer.dart';

import 'support/fakes.dart';
import 'support/trials.dart';

void main() {
  late Directory dir;
  late FakeResultSharer sharer;
  setUp(() {
    dir = Directory.systemTemp.createTempSync('spike_home');
    sharer = FakeResultSharer(const ShareOutcome.dismissed());
  });
  tearDown(() => dir.deleteSync(recursive: true));

  Future<SpikeState> makeState(WidgetTester tester) async {
    tester.view.physicalSize = const Size(800, 3000);
    tester.view.devicePixelRatio = 1;
    addTearDown(tester.view.reset);
    final state = await tester.runAsync(
      () => bootstrap(
        supportDir: Directory('${dir.path}/support'),
        documentsDir: Directory('${dir.path}/docs'),
        sharer: sharer,
        scannerFactory: FakeScannerPort.new,
        identityProvider: FakeIdentityProvider(),
      ),
    );
    return state!;
  }

  /// File IO completes on the real event loop but its continuations run in the test's
  /// fake-async zone, so alternate real waiting with pumping.
  Future<void> settle(WidgetTester tester, {int rounds = 80}) async {
    for (var i = 0; i < rounds; i++) {
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 10)),
      );
      await tester.pump(const Duration(milliseconds: 20));
    }
  }

  testWidgets('Home shows the device, the actions and an empty trial list', (
    tester,
  ) async {
    final state = await makeState(tester);
    await tester.pumpWidget(SpikeApp(state: state));
    expect(find.text('Particle hardware spike'), findsOneWidget);
    expect(find.textContaining('Pixel 9 Pro XL'), findsOneWidget);
    expect(find.textContaining('12288 MB'), findsOneWidget);
    expect(find.byKey(const Key('export-results')), findsOneWidget);
    expect(find.byKey(const Key('send-results')), findsOneWidget);
    expect(find.text('Export results'), findsOneWidget);
    expect(find.text('Send results'), findsOneWidget);
    expect(find.text('No trials yet.'), findsOneWidget);
    expect(find.textContaining('not defined yet'), findsOneWidget);
  });

  testWidgets('stored trials are listed with their outcome', (tester) async {
    final state = await makeState(tester);
    await tester.runAsync(() async {
      await state.saveTrial(
        receiverTrial(
          'T-A',
          TrialStatus.completed,
          completionMs: 1500,
          integrity: Integrity.ok,
        ),
      );
      await state.saveTrial(receiverTrial('T-B', TrialStatus.incomplete));
    });
    await tester.pumpWidget(SpikeApp(state: state));
    expect(find.text('T-A'), findsOneWidget);
    expect(find.text('T-B'), findsOneWidget);
    expect(find.text('completed <= 2 s'), findsOneWidget);
    expect(find.text('incomplete'), findsOneWidget);
    expect(find.textContaining('within 2 s: 1 of 2 attempts'), findsOneWidget);
  });

  testWidgets('Export results writes a bundle without sharing', (tester) async {
    final state = await makeState(tester);
    await tester.pumpWidget(SpikeApp(state: state));
    await tester.tap(find.byKey(const Key('export-results')));
    await settle(tester);
    expect(find.textContaining('Exported 0 trial(s)'), findsOneWidget);
    expect(state.results.lastExport!.file.existsSync(), isTrue);
    expect(sharer.shared, isEmpty);
    expect(find.byKey(const Key('last-export')), findsOneWidget);
  });

  testWidgets('Send results shows an honest outcome and keeps the file', (
    tester,
  ) async {
    final state = await makeState(tester);
    await tester.pumpWidget(SpikeApp(state: state));
    await tester.tap(find.byKey(const Key('send-results')));
    await settle(tester);
    expect(find.textContaining('Share sheet dismissed'), findsWidgets);
    expect(sharer.shared.single.existsSync(), isTrue);

    sharer.outcome = const ShareOutcome.handedToShareSheet();
    await tester.tap(find.byKey(const Key('send-results')));
    await settle(tester);
    expect(
      find.text('Handed to the share sheet (delivery not confirmed)'),
      findsWidgets,
    );
    expect(sharer.shared.length, 2);
    expect(sharer.shared[0].path, sharer.shared[1].path);
  });

  testWidgets('annotations are saved and shown again', (tester) async {
    final state = await makeState(tester);
    await tester.pumpWidget(SpikeApp(state: state));
    await tester.enterText(
      find.byKey(const Key('annotation-sender')),
      'iPhone 13',
    );
    await tester.tap(find.byKey(const Key('save-annotations')));
    await settle(tester);
    expect(state.annotations.senderHardware.value, 'iPhone 13');
    expect(find.text('Annotations saved.'), findsOneWidget);
  });

  testWidgets('New trial offers both roles and the fixtures', (tester) async {
    final state = await makeState(tester);
    await tester.pumpWidget(SpikeApp(state: state));
    await tester.tap(find.byKey(const Key('new-trial')));
    await tester.pumpAndSettle();
    expect(find.text('Sender'), findsOneWidget);
    expect(find.text('Receiver'), findsOneWidget);
    for (final id in ['f200', 'f1k', 'f6k', 'f30k']) {
      expect(find.byKey(Key('fixture-$id')), findsOneWidget);
    }
    // Nothing to type: the receiver reads the sender's pair id from the code.
    expect(find.byKey(const Key('pair-id')), findsNothing);
    expect(find.textContaining('nothing to type'), findsOneWidget);
  });

  testWidgets(
    'a receiver trial runs past 2 s and records the late completion',
    (tester) async {
      tester.view.physicalSize = const Size(800, 3000);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.reset);
      final ports = <FakeScannerPort>[];
      final state = await tester.runAsync(
        () => bootstrap(
          supportDir: Directory('${dir.path}/support'),
          documentsDir: Directory('${dir.path}/docs'),
          sharer: sharer,
          scannerFactory: () => FakeScannerPort()..also(ports.add),
          identityProvider: FakeIdentityProvider(),
        ),
      );
      await tester.pumpWidget(SpikeApp(state: state!));
      await tester.tap(find.byKey(const Key('new-trial')));
      await tester.pumpAndSettle();
      await tester.tap(find.byKey(const Key('create-trial')));
      await tester.pumpAndSettle();
      expect(find.byKey(const Key('receiver-start')), findsOneWidget);
      await tester.tap(find.byKey(const Key('receiver-start')));
      await tester.pump();
      final port = ports.single;
      expect(port.startCalls, 1);
      port.emit(scanFrame(have: 3, need: 9));
      // Let the wall clock run well past two seconds.
      await tester.runAsync(
        () => Future<void>.delayed(const Duration(milliseconds: 2300)),
      );
      await tester.pump(const Duration(milliseconds: 100));
      expect(find.textContaining('past 2 s'), findsOneWidget);
      expect(port.stopCalls, 0);
      expect(find.byKey(const Key('receiver-stop')), findsOneWidget);

      port.emit(ScanDone(Fixtures.f1k.bytes, 7));
      await settle(tester, rounds: 400);
      expect(find.text('completed > 2 s'), findsWidgets);
      expect(find.byKey(const Key('receiver-close')), findsOneWidget);
      final t = state.trials.single;
      expect(t.status, TrialStatus.completed);
      expect(t.receiver!.withinTwoSeconds, isFalse);
      expect(t.receiver!.integrity, Integrity.ok);
      expect(t.receiver!.completionMs.value!, greaterThan(2000));
    },
  );
}

extension on FakeScannerPort {
  void also(void Function(FakeScannerPort) f) => f(this);
}
