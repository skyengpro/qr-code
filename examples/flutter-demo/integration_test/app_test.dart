// Launch check on a real platform (simulator / emulator in CI): the app starts, the Rust
// library that the plugin's Native Assets hook bundled loads through flutter_rust_bridge,
// and the Send tab draws a code built by the Rust encoder. The camera is not used.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';
import 'package:particle_code/particle_code.dart';
import 'package:particle_code_example/main.dart' as app;

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();

  testWidgets('launches and shows a code from the Rust encoder', (tester) async {
    await app.main();
    await tester.pump(const Duration(seconds: 1));

    expect(ParticleCode.isInitialized, isTrue);
    expect(find.text('Show a code'), findsOneWidget);
    await tester.ensureVisible(find.text('Show code'));
    await tester.tap(find.text('Show code'));
    // The code animates forever: pump frames instead of settling.
    for (var i = 0; i < 20; i++) {
      await tester.pump(const Duration(milliseconds: 100));
    }
    expect(find.byKey(const Key('code-view')), findsOneWidget);
    final info = tester.widget<Text>(find.byKey(const Key('info-line'))).data!;
    // ignore: avoid_print
    print('flutter-demo launched: $info');
    expect(info, isNotEmpty);
  });
}
