import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:otel_zone/otel_zone.dart';
import 'package:particle_code_hardware_spike/observability.dart';

void main() {
  testWidgets(
    'reports the named route that is showing, ignoring unnamed ones',
    (tester) async {
      final sink = RecordingCrashScreenSink();
      final zone = OtelZone(
        const OtelZoneConfig(serviceName: 'test', endpoint: ''),
        crashScreenSink: sink,
      );
      await tester.pumpWidget(
        MaterialApp(
          navigatorObservers: [CrashScreenObserver(zone)],
          home: const Scaffold(body: Text('home')),
        ),
      );
      await tester.pump();
      final navigator = tester.state<NavigatorState>(find.byType(Navigator));

      navigator.push(
        MaterialPageRoute<void>(
          settings: const RouteSettings(name: '/sender'),
          builder: (_) => const Scaffold(body: Text('sender')),
        ),
      );
      await tester.pumpAndSettle();
      expect(zone.crashScreen, '/sender');

      // A dialog has no name: the screen underneath stays the answer.
      showDialog<void>(
        context: navigator.context,
        builder: (_) => const AlertDialog(title: Text('share')),
      );
      await tester.pumpAndSettle();
      expect(zone.crashScreen, '/sender');

      navigator.pop();
      await tester.pumpAndSettle();
      navigator.pop();
      await tester.pumpAndSettle();
      expect(zone.crashScreen, '/');
      expect(sink.screens, containsAllInOrder(['/', '/sender', '/']));
    },
  );
}
