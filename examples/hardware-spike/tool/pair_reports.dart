// Pairs the trials of report bundles from both phones of a test campaign.
//
//   dart run tool/pair_reports.dart ~/dev/tests/results/particles [-o paired.csv]
//
// Arguments are report zips or directories holding them. Each receiver trial is matched
// to the sender trial whose code it scanned: by the pair id read from the code, or by
// time for reports without one. Writes one paired.csv row per receiver trial, with the
// sender's real code settings next to the receiver's outcome.
import 'dart:convert';
import 'dart:io';

import 'package:archive/archive.dart';
import 'package:particle_code_hardware_spike/report/pairing.dart';

void main(List<String> args) {
  var out = 'paired.csv';
  final inputs = <String>[];
  for (var i = 0; i < args.length; i++) {
    if (args[i] == '-o' && i + 1 < args.length) {
      out = args[++i];
    } else if (args[i] == '-h' || args[i] == '--help') {
      stdout.writeln(
        'usage: dart run tool/pair_reports.dart <zip|dir>... [-o paired.csv]',
      );
      return;
    } else {
      inputs.add(args[i]);
    }
  }
  if (inputs.isEmpty) {
    stderr.writeln(
      'usage: dart run tool/pair_reports.dart <zip|dir>... [-o paired.csv]',
    );
    exitCode = 64;
    return;
  }

  final zips = <File>[];
  for (final path in inputs) {
    final expanded = path.replaceFirst(
      RegExp(r'^~'),
      Platform.environment['HOME'] ?? '~',
    );
    if (FileSystemEntity.isDirectorySync(expanded)) {
      zips.addAll(
        Directory(
          expanded,
        ).listSync().whereType<File>().where((f) => f.path.endsWith('.zip')),
      );
    } else {
      zips.add(File(expanded));
    }
  }

  final reports = <Map<String, Object?>>[];
  for (final zip in zips) {
    try {
      final archive = ZipDecoder().decodeBytes(zip.readAsBytesSync());
      final entry = archive.findFile('report.json');
      if (entry == null) throw const FormatException('no report.json');
      final json = jsonDecode(utf8.decode(entry.content as List<int>));
      reports.add((json as Map).cast<String, Object?>());
    } on Object catch (e) {
      stderr.writeln('skipped ${zip.path}: $e');
    }
  }

  final trials = collectTrials(reports);
  final pairs = pairTrials(trials);
  File(out).writeAsStringSync(pairedCsv(pairs));

  final counts = matchCounts(pairs);
  stdout
    ..writeln(
      '${reports.length} report(s), ${trials.length} trial(s), '
      '${pairs.length} receiver trial(s)',
    )
    ..writeln('  paired by pair id: ${counts[PairMatch.pairId]}')
    ..writeln('  paired by time:    ${counts[PairMatch.time]}')
    ..writeln('  ambiguous:         ${counts[PairMatch.ambiguous]}')
    ..writeln('  unpaired:          ${counts[PairMatch.unpaired]}')
    ..writeln('wrote $out');
}
