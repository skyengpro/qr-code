import 'observed.dart';

/// Which side of a pair a trial records.
enum TrialRole {
  /// Shows the code.
  sender,

  /// Scans it.
  receiver;

  /// Parses [name].
  static TrialRole parse(Object? v) =>
      v == 'sender' ? TrialRole.sender : TrialRole.receiver;
}

/// Motion setting of the sender.
enum MotionChoice {
  /// No motion layers (`motion: null`).
  stationary,

  /// `Mood.breathing`.
  breathing,

  /// `Mood.flowField`.
  flowField;

  /// Parses [name].
  static MotionChoice parse(Object? v) => MotionChoice.values.firstWhere(
    (m) => m.name == v,
    orElse: () => MotionChoice.breathing,
  );
}

/// A shape the tester can pick (ids of `particlecode_core`'s shape table).
class ShapeOption {
  /// Creates an option.
  const ShapeOption(this.id, this.name);

  /// Shape table id.
  final int id;

  /// Readable name.
  final String name;

  /// The offered shapes.
  static const List<ShapeOption> all = [
    ShapeOption(0, 'disc'),
    ShapeOption(1, 'rounded_square'),
    ShapeOption(28, 'heart'),
    ShapeOption(29, 'hexagon'),
    ShapeOption(30, 'ring'),
  ];

  /// The name for [id], or `shape_<id>`.
  static String nameOf(int id) => all
      .firstWhere((s) => s.id == id, orElse: () => ShapeOption(id, 'shape_$id'))
      .name;
}

/// How a configuration was obtained.
enum ConfigSource {
  /// The app configured the encoder with these values (sender).
  configured,

  /// The tester declares the sender's settings on the receiver (the receiver cannot
  /// observe them all).
  declared;

  /// Parses [name].
  static ConfigSource parse(Object? v) =>
      v == 'declared' ? ConfigSource.declared : ConfigSource.configured;
}

/// Encoder settings and the fixture of one trial.
class TrialConfig {
  /// Creates a configuration.
  const TrialConfig({
    required this.role,
    required this.pairId,
    required this.fixtureId,
    required this.payloadBytes,
    required this.profile,
    required this.shapeId,
    required this.fps,
    required this.isStatic,
    required this.repairPerPass,
    required this.motion,
    this.segmentKbLog2 = 3,
    this.configSource = ConfigSource.configured,
  });

  /// Parses [toJson].
  factory TrialConfig.fromJson(Map<String, Object?> j) => TrialConfig(
    role: TrialRole.parse(j['role']),
    pairId: j['pair_id'] as String? ?? '',
    fixtureId: j['fixture_id'] as String? ?? '',
    payloadBytes: (j['payload_bytes'] as num?)?.toInt() ?? 0,
    profile: j['profile'] as String? ?? 'M',
    shapeId: (j['shape_id'] as num?)?.toInt() ?? 0,
    fps: (j['fps'] as num?)?.toInt() ?? 10,
    isStatic: j['static'] as bool? ?? false,
    repairPerPass: (j['repair_per_pass'] as num?)?.toInt() ?? 2,
    motion: MotionChoice.parse(j['motion']),
    segmentKbLog2: (j['segment_kb_log2'] as num?)?.toInt() ?? 3,
    configSource: ConfigSource.parse(j['config_source']),
  );

  /// Sender or receiver.
  final TrialRole role;

  /// Identifier linking the sender and receiver trial of one pair (typed by the tester).
  final String pairId;

  /// Fixture id (see `fixtures.dart`).
  final String fixtureId;

  /// Fixture size in bytes.
  final int payloadBytes;

  /// `S`, `M` or `L`.
  final String profile;

  /// Shape table id.
  final int shapeId;

  /// Data frames per second.
  final int fps;

  /// Static (single frame, stronger ECC) or animated.
  final bool isStatic;

  /// Repair packets per segment per pass.
  final int repairPerPass;

  /// Motion setting.
  final MotionChoice motion;

  /// Segment size exponent (segment is `1024 << n` bytes).
  final int segmentKbLog2;

  /// Whether these are configured values or declared by the tester.
  final ConfigSource configSource;

  /// Reed-Solomon code of the data symbols, per the product spec.
  String get ecc => isStatic ? 'RS(255,159)' : 'RS(255,191)';

  /// Readable shape name.
  String get shapeName => ShapeOption.nameOf(shapeId);

  /// A copy with a different role or config source (the receiver side of a pair).
  TrialConfig copyWith({TrialRole? role, ConfigSource? configSource}) =>
      TrialConfig(
        role: role ?? this.role,
        pairId: pairId,
        fixtureId: fixtureId,
        payloadBytes: payloadBytes,
        profile: profile,
        shapeId: shapeId,
        fps: fps,
        isStatic: isStatic,
        repairPerPass: repairPerPass,
        motion: motion,
        segmentKbLog2: segmentKbLog2,
        configSource: configSource ?? this.configSource,
      );

  /// JSON form.
  Map<String, Object?> toJson() => {
    'role': role.name,
    'pair_id': pairId,
    'fixture_id': fixtureId,
    'payload_bytes': payloadBytes,
    'profile': profile,
    'shape_id': shapeId,
    'shape': shapeName,
    'fps': fps,
    'static': isStatic,
    'ecc': ecc,
    'repair_per_pass': repairPerPass,
    'segment_kb_log2': segmentKbLog2,
    'motion': motion.name,
    'config_source': configSource.name,
  };
}

/// Manual observations of the physical setup. Every field is optional and manual.
class Conditions {
  /// Creates conditions.
  const Conditions({
    this.distanceCm = const Observed.unknown(),
    this.codeSizeMm = const Observed.unknown(),
    this.lighting = const Observed.unknown(),
    this.screenProtector = const Observed.unknown(),
    this.notes = const Observed.unknown(),
  });

  /// Parses [toJson].
  factory Conditions.fromJson(Object? j) {
    final m = j is Map ? j : const <String, Object?>{};
    return Conditions(
      distanceCm: Observed.fromJson<double>(
        m['distance_cm'],
        decode: decodeDouble,
      ),
      codeSizeMm: Observed.fromJson<double>(
        m['code_size_mm'],
        decode: decodeDouble,
      ),
      lighting: Observed.fromJson<String>(m['lighting']),
      screenProtector: Observed.fromJson<String>(m['screen_protector']),
      notes: Observed.fromJson<String>(m['notes']),
    );
  }

  /// Camera-to-screen distance in centimetres.
  final Observed<double> distanceCm;

  /// Physical side of the code on the sender screen in millimetres.
  final Observed<double> codeSizeMm;

  /// Free text: lighting.
  final Observed<String> lighting;

  /// Free text: screen protector (none, glass, film, privacy ...).
  final Observed<String> screenProtector;

  /// Free text: anything else.
  final Observed<String> notes;

  /// JSON form.
  Map<String, Object?> toJson() => {
    'distance_cm': distanceCm.toJson(),
    'code_size_mm': codeSizeMm.toJson(),
    'lighting': lighting.toJson(),
    'screen_protector': screenProtector.toJson(),
    'notes': notes.toJson(),
  };
}

/// Hardware facts the app cannot detect, typed in by the tester.
class Annotations {
  /// Creates annotations.
  const Annotations({
    this.variant = const Observed.unknown(),
    this.region = const Observed.unknown(),
    this.senderHardware = const Observed.unknown(),
  });

  /// Parses [toJson].
  factory Annotations.fromJson(Object? j) {
    final m = j is Map ? j : const <String, Object?>{};
    return Annotations(
      variant: Observed.fromJson<String>(m['variant']),
      region: Observed.fromJson<String>(m['region']),
      senderHardware: Observed.fromJson<String>(m['sender_hardware']),
    );
  }

  /// Hardware variant / SKU as printed on the box or in settings.
  final Observed<String> variant;

  /// Sales region or carrier variant.
  final Observed<String> region;

  /// The other phone of the pair (model, variant), which cannot be detected remotely.
  final Observed<String> senderHardware;

  /// JSON form.
  Map<String, Object?> toJson() => {
    'variant': variant.toJson(),
    'region': region.toJson(),
    'sender_hardware': senderHardware.toJson(),
  };
}
