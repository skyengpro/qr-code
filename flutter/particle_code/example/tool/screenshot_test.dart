// Renders the Send tab to a PNG for the README. Not part of the regular test suite
// (it lives in tool/); run it from the example directory after building the host library:
//
//   cargo build -p particlecode-ffi
//   flutter test tool/screenshot_test.dart
//
// Text and icons use the Roboto and Material Icons fonts of the Flutter SDK, loaded
// here because widget tests otherwise draw every glyph as a box.
import 'dart:io';
import 'dart:ui' as ui;

import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code_example/main.dart';

Future<void> _loadFont(String family, List<String> files) async {
  final root = Platform.environment['FLUTTER_ROOT'] ?? '/opt/flutter';
  final loader = FontLoader(family);
  for (final f in files) {
    final bytes = File(
      '$root/bin/cache/artifacts/material_fonts/$f',
    ).readAsBytesSync();
    loader.addFont(Future.value(ByteData.sublistView(bytes)));
  }
  await loader.load();
}

void main() {
  testWidgets('render the Send tab', (tester) async {
    await _loadFont('Roboto', ['Roboto-Regular.ttf', 'Roboto-Medium.ttf']);
    await _loadFont('MaterialIcons', ['MaterialIcons-Regular.otf']);

    // Tall while setting things up, so every control can be tapped; phone-sized to shoot.
    tester.view.physicalSize = const Size(390 * 2, 1900 * 2);
    tester.view.devicePixelRatio = 2;
    addTearDown(tester.view.reset);

    final key = GlobalKey();
    await tester.pumpWidget(
      RepaintBoundary(key: key, child: const ExampleApp()),
    );
    await tester.tap(find.text('Heart'));
    await tester.tap(find.text('Trails'));
    await tester.tap(find.text('Show code'));
    await tester.pump();
    tester.view.physicalSize = const Size(390 * 2, 900 * 2);
    await tester.pump();
    for (var i = 0; i < 12; i++) {
      await tester.pump(const Duration(milliseconds: 100));
    }

    await tester.runAsync(() async {
      final boundary =
          key.currentContext!.findRenderObject()! as RenderRepaintBoundary;
      final image = await boundary.toImage(pixelRatio: 2);
      final png = await image.toByteData(format: ui.ImageByteFormat.png);
      final out =
          Platform.environment['SCREENSHOT_OUT'] ??
          '../../../docs/images/flutter_send.png';
      File(out).writeAsBytesSync(png!.buffer.asUint8List());
    });
  });
}
