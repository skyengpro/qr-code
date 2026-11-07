import 'dart:math' as math;

/// Nearest-rank percentile of an ascending list; `null` when empty. [p] in 0..100.
double? percentile(List<double> sorted, double p) {
  if (sorted.isEmpty) return null;
  final rank = (p / 100 * sorted.length).ceil();
  return sorted[math.max(0, math.min(sorted.length - 1, rank - 1))];
}

/// Summary of a series of durations in milliseconds.
class Stat {
  /// Creates a summary.
  const Stat({
    required this.count,
    this.min,
    this.mean,
    this.p50,
    this.p90,
    this.max,
  });

  /// Summarizes [values] (milliseconds).
  factory Stat.of(Iterable<double> values) {
    final sorted = values.toList()..sort();
    if (sorted.isEmpty) return const Stat(count: 0);
    final sum = sorted.fold<double>(0, (a, b) => a + b);
    return Stat(
      count: sorted.length,
      min: sorted.first,
      mean: sum / sorted.length,
      p50: percentile(sorted, 50),
      p90: percentile(sorted, 90),
      max: sorted.last,
    );
  }

  /// Parses [toJson].
  factory Stat.fromJson(Object? json) {
    if (json is! Map) return const Stat(count: 0);
    double? d(String k) => (json[k] as num?)?.toDouble();
    return Stat(
      count: (json['count'] as num?)?.toInt() ?? 0,
      min: d('min'),
      mean: d('mean'),
      p50: d('p50'),
      p90: d('p90'),
      max: d('max'),
    );
  }

  /// Number of samples.
  final int count;

  /// Statistics in milliseconds; `null` when [count] is 0.
  final double? min, mean, p50, p90, max;

  /// JSON form (unit: milliseconds).
  Map<String, Object?> toJson() => {
    'count': count,
    'min': min,
    'mean': mean,
    'p50': p50,
    'p90': p90,
    'max': max,
  };
}
