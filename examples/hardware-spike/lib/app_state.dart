import 'dart:math';

import 'package:flutter/foundation.dart';

import 'device_identity.dart';
import 'model/device_identity.dart';
import 'model/trial.dart';
import 'model/trial_config.dart';
import 'report/exporter.dart';
import 'share/results_service.dart';
import 'storage/trial_store.dart';
import 'trial/clock.dart';
import 'trial/scanner_port.dart';
import 'trial/trial_factory.dart';

/// Everything the screens share: stored trials, the device identity, annotations and the
/// export/send workflows.
class SpikeState extends ChangeNotifier {
  /// Creates the state; call [init] before showing it.
  SpikeState({
    required this.store,
    required this.identityProvider,
    required this.results,
    required this.clock,
    required this.wallClock,
    required this.scannerFactory,
    Random? random,
  }) : random = random ?? Random();

  /// Persistence.
  final TrialStore store;

  /// Reads the device identity once.
  final DeviceIdentityProvider identityProvider;

  /// Export and send.
  final ResultsService results;

  /// Monotonic clock for durations.
  final MonotonicClock clock;

  /// Wall clock for timestamps.
  final WallClock wallClock;

  /// Creates the scanner of a receiver trial.
  final ScannerPort Function() scannerFactory;

  /// Randomness for ids.
  final Random random;

  DeviceIdentity _device = DeviceIdentity.unknown();
  Annotations _annotations = const Annotations();
  List<Trial> _trials = const [];
  int _unreadable = 0;
  int _recovered = 0;

  /// This phone.
  DeviceIdentity get device => _device;

  /// The tester's annotations.
  Annotations get annotations => _annotations;

  /// Stored trials, newest first.
  List<Trial> get trials => _trials;

  /// Stored files that could not be read.
  int get unreadable => _unreadable;

  /// Trials found running at start-up and closed as incomplete.
  int get recovered => _recovered;

  /// Loads identity, annotations, trials and the export state.
  Future<void> init() async {
    _device = await identityProvider.load();
    _annotations = await store.loadAnnotations();
    final loaded = await store.loadAll();
    _trials = loaded.trials.reversed.toList();
    _unreadable = loaded.unreadable;
    _recovered = loaded.recovered;
    await results.init();
    notifyListeners();
  }

  /// Saves the annotations.
  Future<void> saveAnnotations(Annotations a) async {
    _annotations = a;
    await store.saveAnnotations(a);
    notifyListeners();
  }

  /// A new id for a trial created now.
  String newTrialId() => TrialFactory.newId(wallClock().toLocal(), random);

  /// A suggested pair id.
  String newPairId() => TrialFactory.newPairId(random);

  /// A fresh trial in the `created` state.
  Trial createTrial(TrialConfig config, Conditions conditions) =>
      TrialFactory.create(
        trialId: newTrialId(),
        config: config,
        conditions: conditions,
        annotations: _annotations,
        device: _device,
        nowUtc: wallClock().toUtc(),
      );

  /// Persists [trial] (a running or terminal record) and shows it in the list.
  Future<void> saveTrial(Trial trial) async {
    final list = [..._trials];
    final i = list.indexWhere((t) => t.trialId == trial.trialId);
    if (i >= 0) {
      list[i] = trial;
    } else {
      list.insert(0, trial);
    }
    _trials = list;
    notifyListeners();
    await store.save(trial);
  }

  /// Exports a report bundle.
  Future<ExportResult?> exportResults() => results.exportResults(
    trials: _chronological,
    device: _device,
    annotations: _annotations,
  );

  /// Sends the report through the share sheet.
  Future<ShareRecord?> sendResults() => results.sendResults(
    trials: _chronological,
    device: _device,
    annotations: _annotations,
  );

  List<Trial> get _chronological => _trials.reversed.toList();
}
