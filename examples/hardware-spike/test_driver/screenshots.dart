// Host side of integration_test/screenshots_test.dart (see tool/screenshots.sh): writes each
// screenshot to $SHOTS_DIR (default build/screenshots).
import 'dart:io';

import 'package:integration_test/integration_test_driver_extended.dart';

Future<void> main() {
  final dir = Platform.environment['SHOTS_DIR'] ?? 'build/screenshots';
  return integrationDriver(
    onScreenshot: (name, bytes, [args]) async {
      final file = File('$dir/$name.png')..createSync(recursive: true);
      file.writeAsBytesSync(bytes);
      return true;
    },
  );
}
