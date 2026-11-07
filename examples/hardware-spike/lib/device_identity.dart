import 'dart:ffi' show Abi;
import 'dart:io' show Platform;

import 'package:device_info_plus/device_info_plus.dart';
import 'package:package_info_plus/package_info_plus.dart';

import 'model/device_identity.dart';
import 'model/observed.dart';

/// The git revision the app was built from: pass
/// `--dart-define=SOURCE_REVISION=$(git rev-parse HEAD)`. Empty means unknown.
const String kSourceRevision = String.fromEnvironment('SOURCE_REVISION');

/// Reads the device identity.
abstract class DeviceIdentityProvider {
  /// Collects the facts; whatever cannot be read is unknown.
  Future<DeviceIdentity> load();
}

/// [DeviceIdentityProvider] over `device_info_plus` and `package_info_plus`.
///
/// Deliberately not read: Android ID, `identifierForVendor`, serial numbers, the
/// user-assigned device name and the build fingerprint (anything that is persistent or
/// personal). The model identifiers recorded here are shared by all units of a model.
class PlatformDeviceIdentityProvider implements DeviceIdentityProvider {
  /// Creates the provider; the arguments are test seams.
  const PlatformDeviceIdentityProvider({
    this.sourceRevision = kSourceRevision,
    DeviceInfoPlugin? deviceInfo,
  }) : _deviceInfo = deviceInfo;

  /// Revision string baked into the build, empty if unknown.
  final String sourceRevision;
  final DeviceInfoPlugin? _deviceInfo;

  @override
  Future<DeviceIdentity> load() async {
    final f = <String, Observed<Object>>{};
    void m(String k, Object? v) {
      if (v != null && !(v is String && v.isEmpty)) {
        f[k] = Observed<Object>.measured(v);
      }
    }

    try {
      m('architecture', Abi.current().toString());
    } on Object {
      // Unknown ABI.
    }
    try {
      m('dart_version', Platform.version.split(' ').first);
    } on Object {
      // Unknown.
    }
    try {
      final info = await PackageInfo.fromPlatform();
      m('app_name', info.appName);
      m('app_version', info.version);
      m('app_build_number', info.buildNumber);
    } on Object {
      // Unknown app version.
    }
    if (sourceRevision.isNotEmpty) m('source_revision', sourceRevision);

    try {
      final plugin = _deviceInfo ?? DeviceInfoPlugin();
      if (Platform.isAndroid) {
        final a = await plugin.androidInfo;
        m('platform', 'android');
        m('manufacturer', a.manufacturer);
        m('brand', a.brand);
        m('model_identifier', a.model);
        m('device_codename', a.device);
        m('product', a.product);
        m('os_name', 'Android');
        m('os_version', a.version.release);
        m('os_build', '${a.display} (incremental ${a.version.incremental})');
        m('os_api_level', a.version.sdkInt);
        m('supported_abis', a.supportedAbis);
        if (a.physicalRamSize > 0) m('physical_ram_mb', a.physicalRamSize);
        m('low_ram_device', a.isLowRamDevice);
        m('is_physical_device', a.isPhysicalDevice);
      } else if (Platform.isIOS) {
        final i = await plugin.iosInfo;
        m('platform', 'ios');
        m('manufacturer', 'Apple');
        m('model_identifier', i.utsname.machine);
        m('os_name', i.systemName);
        m('os_version', i.systemVersion);
        if (i.physicalRamSize > 0) m('physical_ram_mb', i.physicalRamSize);
        m('is_physical_device', i.isPhysicalDevice);
        // device_info_plus exposes no OS build number on iOS (e.g. 22A3354).
      } else {
        m('platform', Platform.operatingSystem);
        m('os_version', Platform.operatingSystemVersion);
      }
    } on Object {
      // Leave the rest unknown.
    }
    return DeviceIdentity(f);
  }
}
