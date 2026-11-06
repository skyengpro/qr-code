import 'observed.dart';

/// Hardware, OS and build facts of the phone running the app.
///
/// Contains no persistent personal identifier: no Android ID, no identifierForVendor, no
/// serial number, no user-assigned device name. The model identifier is shared by every
/// unit of the same model.
class DeviceIdentity {
  /// Creates an identity from [fields]; missing keys are unknown.
  DeviceIdentity(Map<String, Observed<Object>> fields)
    : fields = Map.unmodifiable({
        for (final k in keys) k: fields[k] ?? const Observed<Object>.unknown(),
      });

  /// An identity where everything is unknown.
  factory DeviceIdentity.unknown() => DeviceIdentity(const {});

  /// Parses [toJson].
  factory DeviceIdentity.fromJson(Object? json) {
    final m = json is Map ? json : const <String, Object?>{};
    return DeviceIdentity({
      for (final k in keys)
        k: k == 'supported_abis'
            ? _list(m[k])
            : Observed.fromJson<Object>(m[k]),
    });
  }

  static Observed<Object> _list(Object? raw) {
    final o = Observed.fromJson<List<String>>(raw, decode: decodeStringList);
    return o.isUnknown
        ? const Observed<Object>.unknown()
        : Observed<Object>.measured(o.value!);
  }

  /// The recorded keys, in report order.
  static const List<String> keys = [
    'platform',
    'manufacturer',
    'brand',
    'model_identifier',
    'device_codename',
    'product',
    'os_name',
    'os_version',
    'os_build',
    'os_api_level',
    'architecture',
    'supported_abis',
    'physical_ram_mb',
    'low_ram_device',
    'is_physical_device',
    'app_name',
    'app_version',
    'app_build_number',
    'source_revision',
    'dart_version',
  ];

  /// The recorded facts.
  final Map<String, Observed<Object>> fields;

  /// The string value of [key], or `null`.
  String? text(String key) {
    final v = fields[key]?.value;
    return v == null ? null : '$v';
  }

  /// A one-line label for lists: manufacturer and model identifier.
  String get label {
    final m = text('manufacturer');
    final id = text('model_identifier');
    if (id == null) return text('platform') ?? 'unknown device';
    return m == null || id.toLowerCase().startsWith(m.toLowerCase())
        ? id
        : '$m $id';
  }

  /// JSON form: each key maps to an [Observed].
  Map<String, Object?> toJson() => {
    for (final k in keys) k: fields[k]!.toJson(),
  };
}
