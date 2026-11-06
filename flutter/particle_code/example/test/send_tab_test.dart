// Runs the real Rust encoder through flutter_rust_bridge. `flutter test` builds it first
// through the plugin's Native Assets hook.
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code/particle_code.dart';
import 'package:particle_code_example/payloads.dart';
import 'package:particle_code_example/send_tab.dart';

/// A tall phone-sized surface so every control is on screen without scrolling.
void _tallSurface(WidgetTester tester) {
  tester.view.physicalSize = const Size(600, 2600);
  tester.view.devicePixelRatio = 1;
  addTearDown(tester.view.reset);
}

Widget _app({bool active = true}) => MaterialApp(
  home: Scaffold(body: SendTab(active: active)),
);

String _info(WidgetTester tester) =>
    tester.widget<Text>(find.byKey(const Key('info-line'))).data!;

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();
  setUpAll(ParticleCode.init);

  final display = <bool>[];

  setUp(() {
    display.clear();
    ParticleCodeDisplay.debugReset();
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(ParticleCodeDisplay.channel, (call) async {
          display.add((call.arguments as Map)['on'] as bool);
          return null;
        });
  });

  tearDown(() {
    TestDefaultBinaryMessengerBinding.instance.defaultBinaryMessenger
        .setMockMethodCallHandler(ParticleCodeDisplay.channel, null);
  });

  group('sample payloads', () {
    test('sizes match their names and are deterministic', () {
      expect(SamplePayloads.ticket.length, inInclusiveRange(150, 260));
      expect(SamplePayloads.credential.length, inInclusiveRange(5900, 6100));
      // COSE_Sign1: tag 18, array of 4.
      expect(SamplePayloads.ticket.take(2), [0xD2, 0x84]);
      expect(SamplePayloads.credential.take(2), [0xD2, 0x84]);
      expect(SamplePayloads.ticket, SamplePayloads.ticket);
    });
  });

  group('every choice the UI offers encodes', () {
    final shapes = {
      'disc': Shape.disc,
      'heart': Shape.heart,
      'rounded square': Shape.roundedSquare,
      'hexagon': Shape.hexagon,
      'ring': Shape.ring,
      'letter P': Shape.letter('P'),
      'letter Z': Shape.letter('Z'),
    };
    for (final profile in CodeProfile.values) {
      for (final MapEntry(key: name, value: shape) in shapes.entries) {
        test('${profile.letter} / $name carries the credential', () {
          final encoder = ParticleEncoder.create(
            SamplePayloads.credential,
            options: EncoderOptions(profile: profile, shape: shape),
          );
          addTearDown(encoder.dispose);
          expect(encoder.framesPerPass, greaterThan(1));
          expect(encoder.frame(0).dots, isNotEmpty);
        });
      }
    }
  });

  group('Send tab', () {
    testWidgets('shows a code with its info line, then hides it', (
      tester,
    ) async {
      _tallSurface(tester);
      await tester.pumpWidget(_app());
      expect(find.text('Your code appears here'), findsOneWidget);
      expect(find.byType(ParticleCodeView), findsNothing);

      await tester.tap(find.text('Show code'));
      await tester.pump();
      expect(find.byType(ParticleCodeView), findsOneWidget);
      expect(find.text('Hide'), findsOneWidget);

      final info = _info(tester);
      expect(info, contains('${SamplePayloads.ticket.length} bytes'));
      expect(info, contains('frames per pass'));
      expect(info, contains('profile M'));
      expect(info, matches(RegExp(r'session [0-9A-F]{4}$')));
      expect(display, [true], reason: 'brightness locked while showing');

      // Let the animation run for a few data frames.
      for (var i = 0; i < 5; i++) {
        await tester.pump(const Duration(milliseconds: 100));
      }

      await tester.tap(find.text('Hide'));
      await tester.pump();
      await tester.pump();
      expect(find.byType(ParticleCodeView), findsNothing);
      expect(find.text('Show code'), findsOneWidget);
      expect(display, [true, false], reason: 'brightness restored on hide');
    });

    testWidgets('changing profile, shape and mood rebuilds a showing code', (
      tester,
    ) async {
      _tallSurface(tester);
      await tester.pumpWidget(_app());
      await tester.tap(find.text('Show code'));
      await tester.pump();
      final first = _info(tester);

      await tester.tap(find.text('L'));
      await tester.pump();
      expect(_info(tester), contains('profile L'));
      expect(_info(tester), isNot(first));

      await tester.tap(find.text('Heart'));
      await tester.pump();
      expect(find.byType(ParticleCodeView), findsOneWidget);

      await tester.tap(find.text('Flow field'));
      await tester.tap(find.text('Trails'));
      await tester.tap(find.text('Heartbeat'));
      await tester.tap(find.text('Reveal'));
      await tester.pump();
      final view = tester.widget<ParticleCodeView>(
        find.byType(ParticleCodeView),
      );
      expect(view.motion!.mood, Mood.flowField);
      expect(view.motion!.particleTrails, isTrue);
      expect(view.motion!.anchorHeartbeat, isTrue);
      expect(view.motion!.reveal, isTrue);
      for (var i = 0; i < 3; i++) {
        await tester.pump(const Duration(milliseconds: 100));
      }
      expect(tester.takeException(), isNull);
    });

    testWidgets('the credential is a bigger transfer than the ticket', (
      tester,
    ) async {
      _tallSurface(tester);
      await tester.pumpWidget(_app());
      await tester.tap(find.text('Show code'));
      await tester.pump();
      final ticketFrames = RegExp(
        r'(\d+) frames per pass',
      ).firstMatch(_info(tester))!.group(1)!;

      await tester.tap(find.text('Credential'));
      await tester.pump();
      final info = _info(tester);
      expect(info, contains('${SamplePayloads.credential.length} bytes'));
      final credentialFrames = RegExp(
        r'(\d+) frames per pass',
      ).firstMatch(info)!.group(1)!;
      expect(int.parse(credentialFrames), greaterThan(int.parse(ticketFrames)));
    });

    testWidgets('free text: empty input is an error, typing fixes it', (
      tester,
    ) async {
      _tallSurface(tester);
      await tester.pumpWidget(_app());
      await tester.tap(find.text('Text'));
      await tester.pump();
      await tester.enterText(find.byType(TextField), '');
      await tester.tap(find.text('Show code'));
      await tester.pump();
      expect(find.byKey(const Key('send-error')), findsOneWidget);
      expect(find.byType(ParticleCodeView), findsNothing);

      await tester.enterText(find.byType(TextField), 'héllo, wörld');
      await tester.pump();
      expect(find.byKey(const Key('send-error')), findsNothing);
      expect(find.byType(ParticleCodeView), findsOneWidget);
      expect(_info(tester), contains('${'héllo, wörld'.length + 2} bytes'));
    });

    testWidgets('an inactive tab releases brightness but keeps settings', (
      tester,
    ) async {
      _tallSurface(tester);
      await tester.pumpWidget(_app());
      await tester.tap(find.text('Show code'));
      await tester.pump();
      expect(display, [true]);

      await tester.pumpWidget(_app(active: false));
      await tester.pump();
      expect(display, [true, false]);

      _tallSurface(tester);
      await tester.pumpWidget(_app());
      await tester.pump();
      expect(display, [true, false, true]);
      expect(find.byType(ParticleCodeView), findsOneWidget);
    });
  });
}
