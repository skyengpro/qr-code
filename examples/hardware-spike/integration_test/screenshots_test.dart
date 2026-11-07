// Store screenshots: drives the app through its main screens on a simulator/emulator and
// captures each one. No camera is needed, so the receiver screen is not included.
//
//   tool/screenshots.sh <device-id> <label>
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:particle_code/particle_code.dart';
import 'package:particle_code_hardware_spike/app.dart';
import 'package:particle_code_hardware_spike/bootstrap.dart';
import 'package:particle_code_hardware_spike/share/result_sharer.dart';

void main() {
  final binding = IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('captures the store screenshots', (tester) async {
    await tester.runAsync(ParticleCode.init);
    final state = (await tester.runAsync(
      () => bootstrap(sharer: _NoopSharer()),
    ))!;
    if (Platform.isAndroid) await binding.convertFlutterSurfaceToImage();

    await tester.pumpWidget(SpikeApp(state: state));
    await tester.pump();

    // Two sender trials, so Home has history to show.
    for (var i = 0; i < 2; i++) {
      await _senderTrial(tester);
    }

    await tester.pumpAndSettle();
    await binding.takeScreenshot('1-home');

    await tester.tap(find.byKey(const Key('new-trial')));
    await tester.pumpAndSettle();
    await binding.takeScreenshot('2-new-trial');

    await tester.tap(find.text('Sender'));
    await tester.pumpAndSettle();
    await _createTrial(tester);
    await _pumpFor(tester, const Duration(seconds: 1));
    await binding.takeScreenshot('3-sender-code');
    await tester.tap(find.byKey(const Key('sender-done')));
    await _settle(tester);

    final tile = find.byKey(Key('trial-${state.trials.first.trialId}'));
    await tester.scrollUntilVisible(
      tile,
      300,
      scrollable: find.byType(Scrollable).first,
    );
    await tester.ensureVisible(tile);
    await tester.pump();
    await tester.tap(tile);
    await tester.pumpAndSettle();
    await binding.takeScreenshot('4-trial-detail');
  });
}

Future<void> _senderTrial(WidgetTester tester) async {
  await tester.tap(find.byKey(const Key('new-trial')));
  await tester.pumpAndSettle();
  await tester.tap(find.text('Sender'));
  await tester.pumpAndSettle();
  await _createTrial(tester);
  await _pumpFor(tester, const Duration(milliseconds: 500));
  await tester.tap(find.byKey(const Key('sender-done')));
  await _settle(tester);
}

Future<void> _createTrial(WidgetTester tester) async {
  await tester.scrollUntilVisible(
    find.byKey(const Key('create-trial')),
    300,
    scrollable: find.byType(Scrollable).first,
  );
  // scrollUntilVisible stops once the button is built, which can be in the list's
  // cache area just below the screen on a phone-sized view: scroll it fully in.
  await tester.ensureVisible(find.byKey(const Key('create-trial')));
  await tester.pump();
  await tester.tap(find.byKey(const Key('create-trial')));
}

Future<void> _pumpFor(WidgetTester tester, Duration total) async {
  for (
    var t = Duration.zero;
    t < total;
    t += const Duration(milliseconds: 100)
  ) {
    await tester.pump(const Duration(milliseconds: 100));
  }
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

class _NoopSharer implements ResultSharer {
  @override
  Future<ShareOutcome> share(File bundle, {required String subject}) async =>
      const ShareOutcome.dismissed();
}
