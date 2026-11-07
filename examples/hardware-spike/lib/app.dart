import 'package:flutter/material.dart';

import 'app_state.dart';
import 'ui/home_screen.dart';

/// Gives screens access to the shared [SpikeState].
class SpikeScope extends InheritedNotifier<SpikeState> {
  /// Creates the scope.
  const SpikeScope({super.key, required SpikeState state, required super.child})
    : super(notifier: state);

  /// The state of the nearest scope without subscribing (usable in `initState`).
  static SpikeState read(BuildContext context) =>
      context.getInheritedWidgetOfExactType<SpikeScope>()!.notifier!;

  /// The state of the nearest scope.
  static SpikeState of(BuildContext context) {
    final scope = context.dependOnInheritedWidgetOfExactType<SpikeScope>();
    assert(scope != null, 'no SpikeScope above this context');
    return scope!.notifier!;
  }
}

/// The hardware spike app.
class SpikeApp extends StatelessWidget {
  /// Creates the app over an initialized [state].
  const SpikeApp({
    super.key,
    required this.state,
    this.navigatorObservers = const [],
  });

  /// Shared state.
  final SpikeState state;

  /// Observers of the app's navigator (screen reporting for telemetry).
  final List<NavigatorObserver> navigatorObservers;

  @override
  Widget build(BuildContext context) {
    return SpikeScope(
      state: state,
      child: MaterialApp(
        title: 'Particle Hardware Spike',
        debugShowCheckedModeBanner: false,
        theme: ThemeData(
          colorSchemeSeed: const Color(0xFF3F51B5),
          useMaterial3: true,
        ),
        darkTheme: ThemeData(
          colorSchemeSeed: const Color(0xFF3F51B5),
          brightness: Brightness.dark,
          useMaterial3: true,
        ),
        navigatorObservers: navigatorObservers,
        home: const HomeScreen(),
      ),
    );
  }
}
