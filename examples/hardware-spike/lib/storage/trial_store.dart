import 'dart:convert';
import 'dart:io';

import '../model/trial.dart';
import '../model/trial_config.dart';
import '../model/trial_setup.dart';

/// Trials read from disk.
class LoadedTrials {
  /// Creates a result.
  const LoadedTrials(this.trials, {this.unreadable = 0, this.recovered = 0});

  /// Trials, oldest first.
  final List<Trial> trials;

  /// Files that could not be parsed (kept on disk, not loaded).
  final int unreadable;

  /// Trials found still `running` (the app died) and rewritten as incomplete.
  final int recovered;
}

/// Local persistence: one JSON file per trial under `<root>/trials/`, plus the tester's
/// annotations and the export/share state. Writes are atomic (temp file, then rename), so
/// a crash never leaves a half-written record.
class TrialStore {
  /// A store rooted at [root] (the app support directory in production).
  TrialStore(this.root);

  /// Root directory.
  final Directory root;

  Directory get _trialsDir => Directory('${root.path}/trials');

  File _trialFile(String id) => File(
    '${_trialsDir.path}/${id.replaceAll(RegExp(r'[^A-Za-z0-9_-]'), '_')}.json',
  );

  /// Writes [trial] (replacing an earlier version of the same trial).
  Future<void> save(Trial trial) async {
    await _atomicWrite(
      _trialFile(trial.trialId),
      const JsonEncoder.withIndent(' ').convert(trial.toJson()),
    );
  }

  /// Loads every trial. A trial left `running` by an earlier process is rewritten as
  /// `incomplete` with reason `interrupted_by_app_exit`.
  Future<LoadedTrials> loadAll() async {
    final dir = _trialsDir;
    if (!await dir.exists()) return const LoadedTrials([]);
    final trials = <Trial>[];
    var unreadable = 0;
    var recovered = 0;
    for (final entity in await dir.list().toList()) {
      if (entity is! File || !entity.path.endsWith('.json')) continue;
      try {
        final raw = jsonDecode(await entity.readAsString());
        var trial = Trial.fromJson((raw as Map).cast<String, Object?>());
        if (trial.status == TrialStatus.running) {
          trial = trial.copyWith(
            status: TrialStatus.incomplete,
            reason: TrialReason.interruptedByAppExit,
            reasonDetail: 'the app was closed while the trial was running',
          );
          await save(trial);
          recovered++;
        }
        trials.add(trial);
      } on Object {
        unreadable++;
      }
    }
    trials.sort((a, b) => a.createdAtUtc.compareTo(b.createdAtUtc));
    return LoadedTrials(trials, unreadable: unreadable, recovered: recovered);
  }

  /// Removes a trial record.
  Future<void> delete(String trialId) async {
    final f = _trialFile(trialId);
    if (await f.exists()) await f.delete();
  }

  File get _annotationsFile => File('${root.path}/annotations.json');

  /// The saved annotations (all unknown if none).
  Future<Annotations> loadAnnotations() async {
    try {
      return Annotations.fromJson(
        jsonDecode(await _annotationsFile.readAsString()),
      );
    } on Object {
      return const Annotations();
    }
  }

  /// Saves [annotations].
  Future<void> saveAnnotations(Annotations annotations) async {
    await _atomicWrite(_annotationsFile, jsonEncode(annotations.toJson()));
  }

  File get _setupFile => File('${root.path}/last_setup.json');

  /// The New trial form as it was last used (defaults if none or unreadable).
  Future<TrialSetup> loadLastSetup() async {
    try {
      return TrialSetup.fromJson(jsonDecode(await _setupFile.readAsString()));
    } on Object {
      return const TrialSetup();
    }
  }

  /// Saves the New trial form as it was used.
  Future<void> saveLastSetup(TrialSetup setup) async {
    await _atomicWrite(_setupFile, jsonEncode(setup.toJson()));
  }

  File get _stateFile => File('${root.path}/results_state.json');

  /// The last export / share state as JSON, or an empty map.
  Future<Map<String, Object?>> loadResultsState() async {
    try {
      final raw = jsonDecode(await _stateFile.readAsString());
      return (raw as Map).cast<String, Object?>();
    } on Object {
      return {};
    }
  }

  /// Saves the export / share state.
  Future<void> saveResultsState(Map<String, Object?> state) async {
    await _atomicWrite(_stateFile, jsonEncode(state));
  }

  // Writes are serialized: a trial is saved when it starts and again when it ends, and
  // the two must not race on the same temporary file. A write is queued synchronously,
  // in call order; nothing may be awaited before that (creating the directory used to
  // be, which let a trial's start record land after its end record).
  Future<void> _writes = Future<void>.value();

  Future<void> _atomicWrite(File target, String content) {
    final next = _writes.then((_) async {
      await target.parent.create(recursive: true);
      final tmp = File('${target.path}.tmp');
      await tmp.writeAsString(content, flush: true);
      await tmp.rename(target.path);
    });
    // A failed write must not poison later ones; the caller still sees the error.
    _writes = next.then((_) {}, onError: (Object _) {});
    return next;
  }
}
