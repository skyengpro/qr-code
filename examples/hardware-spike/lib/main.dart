import 'package:flutter/material.dart';
import 'package:particle_code/particle_code.dart';

import 'app.dart';
import 'bootstrap.dart';
import 'observability.dart';

Future<void> main() => observability.runGuarded(() async {
  // Inside the zone: Flutter warns when runApp arrives from a different one.
  WidgetsFlutterBinding.ensureInitialized();
  observability.setCrashScreen('/');
  await startObservability();
  await ParticleCode.init();
  final state = await bootstrap();
  runApp(
    SpikeApp(
      state: state,
      navigatorObservers: [
        CrashScreenObserver(observability),
        ?observability.routeObserver(),
      ],
    ),
  );
});
