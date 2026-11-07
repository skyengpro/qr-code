import 'trial_config.dart';

/// What the New trial form was last set to, so the next trial starts from it and the
/// tester only changes the one or two settings under test.
///
/// Not part of any report: it is form state, saved on this phone only. The pair id and
/// the notes are deliberately left out (both belong to one trial).
class TrialSetup {
  /// Creates a setup; the defaults are the form's first-run values.
  const TrialSetup({
    this.role = TrialRole.receiver,
    this.fixtureId = 'f1k',
    this.profile = 'M',
    this.shapeId = 0,
    this.fps = 10,
    this.isStatic = false,
    this.repairPerPass = 2,
    this.motion = MotionChoice.breathing,
    this.distanceCm = '',
    this.codeSizeMm = '',
    this.lighting = '',
    this.screenProtector = '',
  });

  /// Parses [toJson]; anything missing or unreadable falls back to the default.
  factory TrialSetup.fromJson(Object? json) {
    final j = json is Map
        ? json.cast<String, Object?>()
        : const <String, Object?>{};
    const d = TrialSetup();
    T? as<T>(String k) => j[k] is T ? j[k] as T : null;
    return TrialSetup(
      role: j.containsKey('role') ? TrialRole.parse(j['role']) : d.role,
      fixtureId: as<String>('fixture_id') ?? d.fixtureId,
      profile: switch (as<String>('profile')) {
        final p? when const {'S', 'M', 'L'}.contains(p) => p,
        _ => d.profile,
      },
      shapeId: as<num>('shape_id')?.toInt() ?? d.shapeId,
      fps: as<num>('fps')?.toInt() ?? d.fps,
      isStatic: as<bool>('static') ?? d.isStatic,
      repairPerPass: as<num>('repair_per_pass')?.toInt() ?? d.repairPerPass,
      motion: j.containsKey('motion')
          ? MotionChoice.parse(j['motion'])
          : d.motion,
      distanceCm: as<String>('distance_cm') ?? d.distanceCm,
      codeSizeMm: as<String>('code_size_mm') ?? d.codeSizeMm,
      lighting: as<String>('lighting') ?? d.lighting,
      screenProtector: as<String>('screen_protector') ?? d.screenProtector,
    );
  }

  /// Sender or receiver.
  final TrialRole role;

  /// Fixture id (see `Fixtures`).
  final String fixtureId;

  /// Profile `S`, `M` or `L`.
  final String profile;

  /// Built-in shape id.
  final int shapeId;

  /// Update rate.
  final int fps;

  /// Static mode (stronger code, no animation).
  final bool isStatic;

  /// Repair packets per pass.
  final int repairPerPass;

  /// Motion layers.
  final MotionChoice motion;

  /// Condition fields, as typed (they are validated when a trial is created).
  final String distanceCm;

  /// See [distanceCm].
  final String codeSizeMm;

  /// See [distanceCm].
  final String lighting;

  /// See [distanceCm].
  final String screenProtector;

  /// JSON for `last_setup.json`.
  Map<String, Object?> toJson() => {
    'role': role.name,
    'fixture_id': fixtureId,
    'profile': profile,
    'shape_id': shapeId,
    'fps': fps,
    'static': isStatic,
    'repair_per_pass': repairPerPass,
    'motion': motion.name,
    'distance_cm': distanceCm,
    'code_size_mm': codeSizeMm,
    'lighting': lighting,
    'screen_protector': screenProtector,
  };
}
