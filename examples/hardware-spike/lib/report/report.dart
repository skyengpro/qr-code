import '../model/device_identity.dart';
import '../model/schema.dart';
import '../model/trial.dart';
import '../model/trial_config.dart';
import 'aggregation.dart';

/// Units used by every numeric field of the report.
const Map<String, String> reportUnits = {
  '*_ms': 'milliseconds (durations; monotonic, except where stated)',
  '*_px':
      'pixels (logical = Flutter logical pixels; physical = logical * device_pixel_ratio)',
  '*_mm': 'millimetres',
  '*_cm': 'centimetres',
  '*_bytes': 'bytes',
  'physical_ram_mb': 'megabytes as reported by the OS',
  '*_utc':
      'ISO 8601 wall-clock instants, UTC; for ordering only, never subtracted for durations',
  'fps': 'data frames per second',
  '*_pitches': 'dot pitches of the code lattice',
};

/// How the main quantities are defined.
const Map<String, String> reportDefinitions = {
  'trial_start':
      'The receiver tester taps Start; the camera start is requested immediately after. '
      'Time zero for every receiver duration.',
  'camera_startup_ms':
      'ParticleScanner: from the start call to the first camera frame event '
      '(plugin Dart stopwatch, monotonic).',
  'first_accepted_packet_ms':
      'Trial start to the first diagnostics frame with an accepted packet '
      '(trial monotonic clock, read when the app receives the record).',
  'completion_ms':
      'Trial start to the scanner reporting a verified payload that equals the selected '
      'fixture (trial monotonic clock). Only set for verified completions.',
  'within_two_seconds':
      'completion_ms <= 2000. Computed afterwards; the scan is never stopped or reset at '
      'two seconds.',
  'delivery_gap_ms':
      'Difference of consecutive frame-event capture timestamps, computed only when both '
      'are in the same native clock domain (capture_clock). Frames the plugin closed '
      'unsent (skipped_before) are inside the gap.',
  'camera_pipeline_drops':
      'Not observable: frames dropped inside the camera pipeline before delivery are '
      'never reported to the app.',
  'held_ms': 'Native time a frame was held, receipt to release.',
  'bridge_ms': 'Dart round trip of the decoder call (bridge plus decode).',
  'decode_*_ms': 'Rust stage durations inside one decoder call.',
};

/// What the report leaves out on purpose.
const List<String> reportPrivacy = [
  'No payload contents: fixtures are synthetic; only fixture id, byte count and a SHA-256 of the decoded bytes are recorded.',
  'No session or signing keys, no credentials.',
  'No camera images or frames.',
  'No Android ID, identifierForVendor, serial number, user-assigned device name or other persistent device identifier.',
  'No unrelated logs.',
  'Tester-typed annotations and notes are included as typed; do not write personal data into them.',
];

/// Statements that bound what the numbers mean.
const List<String> reportCaveats = [
  'Results from a handful of devices or only successful trials are not evidence of CEMAC-wide compatibility or of two-second reliability.',
  'The two-second rate divides by all terminal receiver attempts, including incomplete, cancelled and failed ones; latency percentiles describe verified completions only.',
  'Latency and rates are for the recorded conditions only (devices, distance, lighting, fixture, settings).',
  'A share-sheet handoff is not evidence that the maintainers received the report.',
];

/// Builds the `report.json` map.
Map<String, Object?> buildReport({
  required List<Trial> trials,
  required DeviceIdentity device,
  required Annotations annotations,
  required DateTime generatedAtUtc,
}) {
  final agg = aggregate(trials);
  return {
    'schema': kReportSchemaName,
    'schema_version': kReportSchemaVersion,
    'trial_schema_version': kTrialSchemaVersion,
    'generated_at_utc': generatedAtUtc.toUtc().toIso8601String(),
    'app': {
      'name': device.fields['app_name']!.toJson(),
      'version': device.fields['app_version']!.toJson(),
      'build_number': device.fields['app_build_number']!.toJson(),
      'source_revision': device.fields['source_revision']!.toJson(),
    },
    'exporting_device': device.toJson(),
    'annotations': annotations.toJson(),
    'target_scan_time_ms': kTargetScanTime.inMilliseconds,
    'units': reportUnits,
    'definitions': reportDefinitions,
    'privacy_exclusions': reportPrivacy,
    'caveats': reportCaveats,
    'counts': {
      'trials': trials.length,
      'receiver_attempts': agg.overall.attempts,
      'sender_sessions': agg.senderSessions,
    },
    'aggregation': agg.toJson(),
    'trials': [for (final t in trials) t.toJson()],
  };
}

/// A report read back from JSON.
class ParsedReport {
  /// Creates a parsed report.
  const ParsedReport({
    required this.schemaVersion,
    required this.generatedAtUtc,
    required this.exportingDevice,
    required this.trials,
    required this.raw,
  });

  /// `schema_version` of the file.
  final int schemaVersion;

  /// Export instant.
  final DateTime? generatedAtUtc;

  /// The exporting phone.
  final DeviceIdentity exportingDevice;

  /// The trial records.
  final List<Trial> trials;

  /// The whole JSON map.
  final Map<String, Object?> raw;
}

/// Parses a `report.json` map. Throws a [FormatException] for another schema or a newer
/// schema version than this app understands.
ParsedReport parseReport(Map<String, Object?> json) {
  if (json['schema'] != kReportSchemaName) {
    throw const FormatException('not a particle-code hardware spike report');
  }
  final version = (json['schema_version'] as num?)?.toInt() ?? 0;
  if (version < 1 || version > kReportSchemaVersion) {
    throw FormatException('unsupported report schema version $version');
  }
  final list = json['trials'];
  return ParsedReport(
    schemaVersion: version,
    generatedAtUtc: DateTime.tryParse('${json['generated_at_utc']}'),
    exportingDevice: DeviceIdentity.fromJson(json['exporting_device']),
    trials: [
      if (list is List)
        for (final t in list)
          Trial.fromJson((t as Map).cast<String, Object?>()),
    ],
    raw: json,
  );
}
