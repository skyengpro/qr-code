import '../model/trial.dart';
import 'aggregation.dart';

/// Encodes rows as RFC 4180 CSV (comma separated, `\n` line ends, quotes doubled).
String encodeCsv(List<String> header, List<List<Object?>> rows) {
  final b = StringBuffer()..writeln(_line(header));
  for (final r in rows) {
    b.writeln(_line(r));
  }
  return b.toString();
}

String _line(List<Object?> cells) => cells.map(_cell).join(',');

String _cell(Object? v) {
  if (v == null) return '';
  final s = '$v';
  if (s.contains(',') ||
      s.contains('"') ||
      s.contains('\n') ||
      s.contains('\r')) {
    return '"${s.replaceAll('"', '""')}"';
  }
  return s;
}

/// Parses CSV text produced by [encodeCsv] into rows of cells.
List<List<String>> parseCsv(String text) {
  final rows = <List<String>>[];
  var row = <String>[];
  final cell = StringBuffer();
  var quoted = false;
  var any = false;
  for (var i = 0; i < text.length; i++) {
    final c = text[i];
    if (quoted) {
      if (c == '"') {
        if (i + 1 < text.length && text[i + 1] == '"') {
          cell.write('"');
          i++;
        } else {
          quoted = false;
        }
      } else {
        cell.write(c);
      }
    } else if (c == '"') {
      quoted = true;
      any = true;
    } else if (c == ',') {
      row.add(cell.toString());
      cell.clear();
      any = true;
    } else if (c == '\n') {
      row.add(cell.toString());
      cell.clear();
      rows.add(row);
      row = [];
      any = false;
    } else if (c != '\r') {
      cell.write(c);
      any = true;
    }
  }
  if (any || cell.isNotEmpty) {
    row.add(cell.toString());
    rows.add(row);
  }
  return rows;
}

/// Columns of `trials.csv`.
const List<String> trialsHeader = [
  'trial_id',
  'pair_id',
  'role',
  'status',
  'reason',
  'created_at_utc',
  'started_at_utc',
  'ended_at_utc',
  'duration_ms',
  'device_model',
  'os_version',
  'app_version',
  'source_revision',
  'variant',
  'region',
  'sender_hardware',
  'fixture_id',
  'payload_bytes',
  'profile',
  'shape',
  'fps',
  'static',
  'ecc',
  'repair_per_pass',
  'motion',
  'config_source',
  'distance_cm',
  'code_size_mm',
  'lighting',
  'screen_protector',
  'notes',
  'camera_startup_ms',
  'first_accepted_packet_ms',
  'completion_ms',
  'within_2s',
  'integrity',
  'have',
  'need',
  'diagnostics_received',
  'frame_events',
  'decoded_frames',
  'skipped_before_total',
  'accepted_packets',
  'distinct_packets',
  'camera_width',
  'camera_height',
  'camera_format',
  'delivery_gap_p50_ms',
  'delivery_gap_p90_ms',
  'bridge_p50_ms',
  'decode_total_p50_ms',
  'held_p50_ms',
  'frames_total',
  'frames_truncated',
];

/// One `trials.csv` row per trial (every status; unknown values are empty cells).
List<Object?> trialRow(Trial t) {
  final r = t.receiver;
  final c = t.config;
  return [
    t.trialId,
    c.pairId,
    c.role.name,
    t.status.name,
    t.reason,
    t.createdAtUtc.toIso8601String(),
    t.startedAtUtc?.toIso8601String(),
    t.endedAtUtc?.toIso8601String(),
    t.durationMs,
    t.device.text('model_identifier'),
    t.device.text('os_version'),
    t.device.text('app_version'),
    t.device.text('source_revision'),
    t.annotations.variant.value,
    t.annotations.region.value,
    t.annotations.senderHardware.value,
    c.fixtureId,
    c.payloadBytes,
    c.profile,
    c.shapeName,
    c.fps,
    c.isStatic,
    c.ecc,
    c.repairPerPass,
    c.motion.name,
    c.configSource.name,
    t.conditions.distanceCm.value,
    t.conditions.codeSizeMm.value,
    t.conditions.lighting.value,
    t.conditions.screenProtector.value,
    t.conditions.notes.value,
    r?.cameraStartupMs.value,
    r?.firstAcceptedPacketMs.value,
    r?.completionMs.value,
    r?.withinTwoSeconds,
    r?.integrity?.name,
    r?.have,
    r?.need,
    r?.diagnosticsReceived,
    r?.counters.frameEvents,
    r?.counters.decoded,
    r?.counters.skippedBeforeTotal,
    r?.counters.acceptedPackets,
    r?.counters.distinctPackets,
    r?.buffer?.width,
    r?.buffer?.height,
    r?.buffer?.format,
    r?.timings['delivery_gap']?.p50,
    r?.timings['delivery_gap']?.p90,
    r?.timings['bridge']?.p50,
    r?.timings['decode_total']?.p50,
    r?.timings['held']?.p50,
    r?.framesTotal,
    r?.framesTruncated,
  ];
}

/// `trials.csv`.
String trialsCsv(Iterable<Trial> trials) =>
    encodeCsv(trialsHeader, [for (final t in trials) trialRow(t)]);

/// Columns of `summary.csv`.
const List<String> summaryHeader = [
  'receiver_model',
  'receiver_os',
  'sender_hardware',
  'fixture',
  'profile',
  'shape',
  'fps',
  'ecc',
  'repair_per_pass',
  'motion',
  'attempts',
  'completed_within_2s',
  'eventual_completions',
  'incomplete',
  'cancelled',
  'error',
  'within_2s_of_all_attempts',
  'latency_successful_p50_ms',
  'latency_successful_p90_ms',
  'latency_successful_max_ms',
  'latency_successful_count',
];

/// One `summary.csv` row for a group.
List<Object?> summaryRow(GroupSummary g) => [
  g.fields['receiver_model'],
  g.fields['receiver_os'],
  g.fields['sender_hardware'],
  g.fields['fixture'],
  g.fields['profile'],
  g.fields['shape'],
  g.fields['fps'],
  g.fields['ecc'],
  g.fields['repair_per_pass'],
  g.fields['motion'],
  g.attempts,
  g.withinTwoSeconds,
  g.eventualCompletions,
  g.incomplete,
  g.cancelled,
  g.error,
  g.withinTwoSecondsRate,
  g.latency.p50,
  g.latency.p90,
  g.latency.max,
  g.latency.count,
];

/// `summary.csv`: one row per configuration group.
String summaryCsv(Aggregation a) =>
    encodeCsv(summaryHeader, [for (final g in a.groups) summaryRow(g)]);

/// Columns of `frames.csv`.
const List<String> framesHeader = [
  'trial_id',
  'index',
  'native_frame_id',
  'disposition',
  'since_start_ms',
  'capture_gap_ms',
  'receipt_latency_ms',
  'skipped_before',
  'held_ms',
  'bridge_ms',
  'decode_total_ms',
  'decode_validate_ms',
  'decode_locate_ms',
  'decode_read_ms',
  'decode_receive_ms',
  'locked',
  'reject_stage',
  'reject_category',
  'segment_index',
  'packet_seed',
  'distinct',
  'have',
  'need',
  'done',
];

/// `frames.csv`: the recorded per-frame rows of every receiver trial.
String framesCsv(Iterable<Trial> trials) {
  final rows = <List<Object?>>[];
  for (final t in trials) {
    for (final f in t.receiver?.frames ?? const []) {
      rows.add([
        t.trialId,
        f.index,
        f.nativeFrameId,
        f.disposition,
        f.sinceStartMs,
        f.captureGapMs,
        f.receiptLatencyMs,
        f.skippedBefore,
        f.heldMs,
        f.bridgeMs,
        f.decodeTotalMs,
        f.validateMs,
        f.locateMs,
        f.readMs,
        f.receiveMs,
        f.locked,
        f.rejectStage,
        f.rejectCategory,
        f.segmentIndex,
        f.packetSeed,
        f.distinct,
        f.have,
        f.need,
        f.done,
      ]);
    }
  }
  return encodeCsv(framesHeader, rows);
}
