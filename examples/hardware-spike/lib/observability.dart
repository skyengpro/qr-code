import 'dart:io' show Platform;

import 'package:device_info_plus/device_info_plus.dart';
import 'package:flutter/widgets.dart';
import 'package:otel_zone/otel_zone.dart';
import 'package:package_info_plus/package_info_plus.dart';
import 'package:path_provider/path_provider.dart';

import 'device_identity.dart' show kSourceRevision;

/// The OTLP collector this build reports to. Pass
/// `--dart-define=OTEL_EXPORTER_OTLP_ENDPOINT=https://...`; without it telemetry is off:
/// errors are still caught and kept in the local log, and nothing leaves the phone.
const String kOtelEndpoint = String.fromEnvironment(
  'OTEL_EXPORTER_OTLP_ENDPOINT',
);

/// The one sink for every uncaught error, built at the top level so that anything that
/// fails before [startObservability] finishes still has somewhere to land.
final OtelZone observability = OtelZone(
  OtelZoneConfig(
    serviceName: 'particle-hardware-spike',
    endpoint: kOtelEndpoint,
    deploymentEnvironmentName: 'spike',
    exportFloor: ExportFloor.parse(
      const String.fromEnvironment(
        'OTEL_LOG_EXPORT_LEVEL',
        defaultValue: 'warning',
      ),
    ),
    // The cache directory is excluded from device backups; the OS may clear it, which only
    // costs an undelivered batch.
    spoolDirectory: getApplicationCacheDirectory,
  ),
);

/// Starts the OpenTelemetry SDK and recovers native crash reports of earlier runs.
/// Never throws; does nothing when the build has no collector endpoint.
Future<void> startObservability() async {
  if (kOtelEndpoint.isEmpty) {
    observability.talker.info(
      'Telemetry is off: built without OTEL_EXPORTER_OTLP_ENDPOINT',
    );
    return;
  }
  try {
    final build = await PackageInfo.fromPlatform();
    await observability.start(
      serviceVersion: build.version,
      buildId: build.buildNumber,
      resourceAttributes: await _resourceAttributes(),
    );
  } on Object catch (error, stack) {
    observability.talker.handle(error, stack, 'telemetry did not start');
  }
}

Future<Map<String, String>> _resourceAttributes() async {
  final attributes = <String, String>{
    'os.type': Platform.isIOS ? 'darwin' : 'linux',
    'os.name': Platform.isIOS ? 'iOS' : 'Android',
    if (kSourceRevision.isNotEmpty) 'vcs.ref.head.revision': kSourceRevision,
  };
  try {
    final plugin = DeviceInfoPlugin();
    if (Platform.isIOS) {
      final info = await plugin.iosInfo;
      attributes['os.version'] = info.systemVersion;
      attributes['device.model.identifier'] = info.utsname.machine;
    } else if (Platform.isAndroid) {
      final info = await plugin.androidInfo;
      attributes['os.version'] = info.version.release;
      attributes['device.model.identifier'] = info.model;
      attributes['device.manufacturer'] = info.manufacturer;
    }
  } on Object {
    // The model is a nice-to-have; telemetry starts without it.
  }
  return attributes;
}

/// Tells the platform which screen is showing, so a native crash report names it.
///
/// Only named routes count (the route pattern, never an id); dialogs and sheets have no
/// name and leave the screen underneath as the answer.
class CrashScreenObserver extends NavigatorObserver {
  /// Reports to [zone].
  CrashScreenObserver(this.zone);

  /// Where screens are reported.
  final OtelZone zone;

  void _show(Route<dynamic>? route) {
    final name = route?.settings.name;
    if (name != null) zone.setCrashScreen(name);
  }

  @override
  void didPush(Route<dynamic> route, Route<dynamic>? previousRoute) =>
      _show(route);

  @override
  void didPop(Route<dynamic> route, Route<dynamic>? previousRoute) =>
      _show(previousRoute);

  @override
  void didReplace({Route<dynamic>? newRoute, Route<dynamic>? oldRoute}) =>
      _show(newRoute);

  @override
  void didRemove(Route<dynamic> route, Route<dynamic>? previousRoute) =>
      _show(previousRoute);
}
