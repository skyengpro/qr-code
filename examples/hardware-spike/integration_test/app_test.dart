// Launch evidence for CI (iOS simulator / Android emulator). No camera is needed: the
// test starts the app, checks the Home screen, saves a sender trial with the real encoder
// and exports a report bundle.
//
//   flutter test integration_test/app_test.dart -d <device>
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:particle_code/particle_code.dart';
import 'package:particle_code_hardware_spike/app.dart';
import 'package:particle_code_hardware_spike/bootstrap.dart';
import 'package:particle_code_hardware_spike/model/trial.dart';
import 'package:particle_code_hardware_spike/share/result_sharer.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('launches, records a sender trial and exports a bundle', (
    tester,
  ) async {
    await tester.runAsync(ParticleCode.init);
    // On a phone or emulator the app uses its real directories (path_provider); on the
    // host the same test runs against a temporary directory.
    final onDevice = Platform.isAndroid || Platform.isIOS;
    final tmp = onDevice
        ? null
        : Directory.systemTemp.createTempSync('spike_integration');
    addTearDown(() => tmp?.deleteSync(recursive: true));
    final sharer = _RecordingSharer();
    final state = (await tester.runAsync(
      () => bootstrap(
        supportDir: tmp == null ? null : Directory('${tmp.path}/support'),
        documentsDir: tmp == null ? null : Directory('${tmp.path}/docs'),
        sharer: sharer,
      ),
    ))!;
    final before = state.trials.length;

    await tester.pumpWidget(SpikeApp(state: state));
    await tester.pump();

    // Home renders.
    expect(find.text('Particle hardware spike'), findsOneWidget);
    expect(find.text('Export results'), findsOneWidget);
    expect(find.text('Send results'), findsOneWidget);

    // New sender trial -> shows the code -> Done.
    await tester.tap(find.byKey(const Key('new-trial')));
    await tester.pumpAndSettle();
    await tester.tap(find.text('Sender'));
    await tester.pumpAndSettle();
    await tester.scrollUntilVisible(
      find.byKey(const Key('create-trial')),
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.tap(find.byKey(const Key('create-trial')));
    await tester.pump(const Duration(milliseconds: 500));
    await tester.pump(const Duration(milliseconds: 500));
    expect(find.byKey(const Key('sender-label')), findsOneWidget);
    await tester.tap(find.byKey(const Key('sender-done')));
    await _settle(tester);

    expect(state.trials.length, before + 1);
    final trial = state.trials.first;
    expect(trial.config.role.name, 'sender');
    expect(trial.status, TrialStatus.completed);
    expect(trial.sender!.devicePixelRatio.value, isNotNull);
    // The real encoder put the trial's pair id in every header as the session id.
    expect(trial.config.pairId, matches(RegExp(r'^P-[0-9a-f]{4}$')));
    expect(trial.sender!.pairIdOnScreen.value, trial.config.pairId);

    // Export works offline and writes a bundle.
    await tester.tap(find.byKey(const Key('export-results')));
    await _settle(tester);
    final export = state.results.lastExport;
    expect(export, isNotNull);
    expect(export!.file.existsSync(), isTrue);
    expect(find.textContaining('Exported'), findsWidgets);
  });
}

class _RecordingSharer implements ResultSharer {
  @override
  Future<ShareOutcome> share(File bundle, {required String subject}) async =>
      const ShareOutcome.dismissed();
}

/// File IO continues on the real event loop; alternate waiting and pumping.
Future<void> _settle(WidgetTester tester) async {
  for (var i = 0; i < 60; i++) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 20)),
    );
    await tester.pump(const Duration(milliseconds: 20));
  }
}
