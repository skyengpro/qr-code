/// A monotonic time source. Durations are differences of two [now] readings of the same
/// clock; never mix them with wall-clock or native timestamps.
abstract class MonotonicClock {
  /// Time since an arbitrary fixed origin.
  Duration get now;
}

/// [MonotonicClock] backed by a Dart [Stopwatch] (monotonic).
class StopwatchClock implements MonotonicClock {
  /// Starts counting at construction.
  StopwatchClock() : _watch = Stopwatch()..start();

  final Stopwatch _watch;

  @override
  Duration get now => _watch.elapsed;
}

/// A clock that only moves when told to. For tests.
class ManualClock implements MonotonicClock {
  Duration _now = Duration.zero;

  @override
  Duration get now => _now;

  /// Moves time forward.
  void advance(Duration d) => _now += d;
}

/// Wall-clock source (UTC instants in records; never used for durations).
typedef WallClock = DateTime Function();
