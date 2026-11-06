import 'package:flutter/material.dart';
import 'package:particle_code/particle_code.dart';

import 'app.dart';
import 'bootstrap.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await ParticleCode.init();
  runApp(SpikeApp(state: await bootstrap()));
}
