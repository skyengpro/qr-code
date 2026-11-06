import 'dart:io';

import 'package:path_provider/path_provider.dart';

import 'app_state.dart';
import 'device_identity.dart';
import 'report/exporter.dart';
import 'share/result_sharer.dart';
import 'share/results_service.dart';
import 'storage/trial_store.dart';
import 'trial/clock.dart';
import 'trial/scanner_port.dart';

/// Builds the production [SpikeState]: trials in the app support directory, bundles in
/// `<documents>/reports`, the real share sheet and the plugin's scanner.
Future<SpikeState> bootstrap({
  Directory? supportDir,
  Directory? documentsDir,
  ResultSharer? sharer,
  ScannerPort Function()? scannerFactory,
  DeviceIdentityProvider? identityProvider,
}) async {
  final support = supportDir ?? await getApplicationSupportDirectory();
  final docs = documentsDir ?? await getApplicationDocumentsDirectory();
  final store = TrialStore(Directory('${support.path}/spike'));
  DateTime wall() => DateTime.now();
  final state = SpikeState(
    store: store,
    identityProvider:
        identityProvider ?? const PlatformDeviceIdentityProvider(),
    results: ResultsService(
      store: store,
      exporter: ReportExporter(
        directory: Directory('${docs.path}/reports'),
        wallClock: wall,
      ),
      sharer: sharer ?? const SharePlusResultSharer(),
      wallClock: wall,
    ),
    clock: StopwatchClock(),
    wallClock: wall,
    scannerFactory: scannerFactory ?? PluginScannerPort.new,
  );
  await state.init();
  return state;
}
