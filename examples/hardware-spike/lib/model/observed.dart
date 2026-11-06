/// Where a recorded value came from.
enum ValueSource {
  /// Read by the app from the system or the plugin.
  measured,

  /// Typed in by the tester.
  manual,

  /// Not obtainable; the value is `null`. Serialized as `"unavailable"`.
  unknown;

  /// The JSON spelling.
  String get wire => this == unknown ? 'unavailable' : name;

  /// Parses [wire]; anything unrecognized is [unknown].
  static ValueSource parse(Object? value) => switch (value) {
    'measured' => measured,
    'manual' => manual,
    _ => unknown,
  };
}

/// A value together with its provenance. Unknowns are explicit: `value == null` and
/// `source == ValueSource.unknown`.
class Observed<T extends Object> {
  const Observed._(this.value, this.source);

  /// A value the app measured.
  const Observed.measured(T value) : this._(value, ValueSource.measured);

  /// A value the tester entered.
  const Observed.manual(T value) : this._(value, ValueSource.manual);

  /// A value that could not be obtained.
  const Observed.unknown() : this._(null, ValueSource.unknown);

  /// [measured] when [value] is non-null, else [unknown].
  factory Observed.measuredOrUnknown(T? value) =>
      value == null ? Observed<T>.unknown() : Observed<T>.measured(value);

  /// [manual] when [value] is non-null (and non-empty for strings), else [unknown].
  factory Observed.manualOrUnknown(T? value) {
    if (value == null || (value is String && value.trim().isEmpty)) {
      return Observed<T>.unknown();
    }
    return Observed<T>.manual(value);
  }

  /// The value, or `null` when unknown.
  final T? value;

  /// Provenance.
  final ValueSource source;

  /// Whether the value is unknown.
  bool get isUnknown => value == null;

  /// `{"value": ..., "source": "measured" | "manual" | "unavailable"}`.
  Map<String, Object?> toJson() => {'value': value, 'source': source.wire};

  /// Parses [toJson]. Anything malformed becomes unknown. [decode] converts the raw JSON
  /// value (for numbers or lists); by default the value must already be a [T].
  static Observed<T> fromJson<T extends Object>(
    Object? json, {
    T? Function(Object?)? decode,
  }) {
    if (json is! Map) return Observed<T>.unknown();
    final raw = json['value'];
    final T? value = raw == null
        ? null
        : (decode != null ? decode(raw) : (raw is T ? raw : null));
    if (value == null) return Observed<T>.unknown();
    final source = ValueSource.parse(json['source']);
    return Observed<T>._(
      value,
      source == ValueSource.unknown ? ValueSource.measured : source,
    );
  }

  @override
  String toString() => 'Observed($value, ${source.wire})';
}

/// Decodes a JSON number as a double.
double? decodeDouble(Object? v) => v is num ? v.toDouble() : null;

/// Decodes a JSON number as an int.
int? decodeInt(Object? v) => v is num ? v.toInt() : null;

/// Decodes a JSON list of strings.
List<String>? decodeStringList(Object? v) =>
    v is List ? v.map((e) => '$e').toList() : null;
