import 'package:flutter/material.dart';
import 'package:particle_code/particle_code.dart';

import 'scan_tab.dart';
import 'send_tab.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  // Loads the Rust library behind the sender; required before any ParticleEncoder.
  await ParticleCode.init();
  runApp(const ExampleApp());
}

class ExampleApp extends StatelessWidget {
  const ExampleApp({super.key});

  static const Color _seed = Color(0xFF3B5BDB);

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Particle Code',
      debugShowCheckedModeBanner: false,
      themeMode: ThemeMode.system,
      theme: ThemeData(
        useMaterial3: true,
        colorScheme: ColorScheme.fromSeed(seedColor: _seed),
      ),
      darkTheme: ThemeData(
        useMaterial3: true,
        colorScheme: ColorScheme.fromSeed(
          seedColor: _seed,
          brightness: Brightness.dark,
        ),
      ),
      home: const HomePage(),
    );
  }
}

class HomePage extends StatefulWidget {
  const HomePage({super.key, this.scanTabBuilder});

  /// Replaces the scan tab (tests inject one on mocked channels).
  final WidgetBuilder? scanTabBuilder;

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  int _tab = 0;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(
        title: Text(_tab == 0 ? 'Show a code' : 'Scan a code'),
        centerTitle: false,
      ),
      // The send tab keeps its settings while hidden; the scan tab only exists while
      // it is in front, so leaving it stops the camera.
      body: IndexedStack(
        index: _tab,
        children: [
          SendTab(active: _tab == 0),
          if (_tab == 1)
            widget.scanTabBuilder?.call(context) ?? const ScanTab()
          else
            const SizedBox.shrink(),
        ],
      ),
      bottomNavigationBar: NavigationBar(
        selectedIndex: _tab,
        onDestinationSelected: (i) => setState(() => _tab = i),
        destinations: const [
          NavigationDestination(icon: Icon(Icons.blur_on), label: 'Send'),
          NavigationDestination(
            icon: Icon(Icons.qr_code_scanner),
            label: 'Scan',
          ),
        ],
      ),
    );
  }
}
