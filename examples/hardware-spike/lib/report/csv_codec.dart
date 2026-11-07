// Plain Dart (no Flutter): also used by tool/pair_reports.dart.

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
