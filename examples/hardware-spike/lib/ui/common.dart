import 'package:flutter/material.dart';

import '../model/observed.dart';
import '../model/trial.dart';

/// `1234.5` -> `1.23 s`; `null` -> `n/a`.
String fmtSeconds(double? ms) =>
    ms == null ? 'n/a' : '${(ms / 1000).toStringAsFixed(2)} s';

/// `12.345` -> `12.3 ms`.
String fmtMs(double? ms) => ms == null ? 'n/a' : '${ms.toStringAsFixed(1)} ms';

/// Text for an [Observed]: the value, or `unavailable`; manual values are tagged.
String fmtObserved(Observed<Object> o) {
  if (o.isUnknown) return 'unavailable';
  return o.source == ValueSource.manual ? '${o.value} (manual)' : '${o.value}';
}

/// A coloured outcome chip.
class StatusChip extends StatelessWidget {
  /// Creates a chip for [trial].
  const StatusChip(this.trial, {super.key});

  /// The trial.
  final Trial trial;

  @override
  Widget build(BuildContext context) {
    final scheme = Theme.of(context).colorScheme;
    var label = trial.status.name;
    Color color = scheme.outline;
    if (trial.status == TrialStatus.completed) {
      if (trial.config.role.name == 'sender') {
        label = 'sender session';
        color = scheme.primary;
      } else if (trial.metTwoSeconds) {
        label = 'completed <= 2 s';
        color = Colors.green;
      } else {
        label = 'completed > 2 s';
        color = Colors.orange;
      }
    } else if (trial.status == TrialStatus.error) {
      color = scheme.error;
    } else if (trial.status == TrialStatus.running) {
      color = scheme.primary;
    }
    return Chip(
      label: Text(label, style: TextStyle(color: color, fontSize: 12)),
      side: BorderSide(color: color),
      visualDensity: VisualDensity.compact,
      padding: EdgeInsets.zero,
    );
  }
}

/// A label/value row.
class KeyValue extends StatelessWidget {
  /// Creates a row.
  const KeyValue(this.label, this.value, {super.key});

  /// Left column.
  final String label;

  /// Right column.
  final String value;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          SizedBox(
            width: 150,
            child: Text(
              label,
              style: Theme.of(context).textTheme.bodySmall?.copyWith(
                color: Theme.of(context).colorScheme.onSurfaceVariant,
              ),
            ),
          ),
          Expanded(child: SelectableText(value)),
        ],
      ),
    );
  }
}
