import 'dart:async';

import 'package:flutter/material.dart';
import 'package:particle_code/particle_code.dart';

import '../app.dart';
import '../app_state.dart';
import '../fixtures.dart';
import '../model/schema.dart';
import '../model/trial.dart';
import '../trial/receiver_recorder.dart';
import '../trial/scanner_port.dart';
import 'common.dart';

enum _Phase { preparing, denied, ready, running, finished }

/// Scans the sender's code and records the trial.
///
/// The trial starts when the tester taps **Start** and ends only by a verified
/// completion, **Stop** (incomplete), **Cancel** or a native error. The 2 s mark is a
/// marker: the scan keeps running past it.
class ReceiverScreen extends StatefulWidget {
  /// Creates the screen for a trial in the `created` state.
  const ReceiverScreen({super.key, required this.trial});

  /// The trial to run.
  final Trial trial;

  @override
  State<ReceiverScreen> createState() => _ReceiverScreenState();
}

class _ReceiverScreenState extends State<ReceiverScreen> {
  late final SpikeState _state;
  late final ScannerPort _port;
  late final ReceiverRecorder _recorder;
  Timer? _ticker;
  _Phase _phase = _Phase.preparing;
  String? _message;
  Trial? _result;
  bool _saved = false;

  @override
  void initState() {
    super.initState();
    final state = _state = SpikeScope.read(context);
    _port = state.scannerFactory();
    _recorder = ReceiverRecorder(
      port: _port,
      base: widget.trial,
      fixture: Fixtures.byId(widget.trial.config.fixtureId)!,
      clock: state.clock,
      wallClock: state.wallClock,
    )..addListener(_onRecorder);
    unawaited(_prepare());
  }

  @override
  void dispose() {
    _ticker?.cancel();
    _recorder.removeListener(_onRecorder);
    if (_recorder.isRunning) {
      // Left while running (e.g. the route was removed): record it, do not lose it.
      final state = _state;
      final recorder = _recorder;
      unawaited(
        recorder.interrupt().then((_) => state.saveTrial(recorder.trial)),
      );
    }
    _recorder.dispose();
    _port.dispose();
    super.dispose();
  }

  Future<void> _prepare() async {
    // The permission prompt happens before the trial starts, so it is not part of the
    // measured time.
    try {
      final granted = await _port.requestPermission();
      if (!mounted) return;
      setState(() => _phase = granted ? _Phase.ready : _Phase.denied);
    } on Object catch (e) {
      if (!mounted) return;
      setState(() {
        _phase = _Phase.denied;
        _message = '$e';
      });
    }
  }

  Future<void> _start() async {
    final state = SpikeScope.of(context);
    setState(() => _phase = _Phase.running);
    _ticker = Timer.periodic(
      const Duration(milliseconds: 50),
      (_) => mounted ? setState(() {}) : null,
    );
    // Start first (it marks the trial start and requests the camera), then persist.
    final started = _recorder.start();
    await state.saveTrial(_recorder.trial);
    await started;
  }

  void _onRecorder() {
    if (_recorder.status.isTerminal && !_saved) {
      _saved = true;
      unawaited(_finish());
    }
  }

  Future<void> _finish() async {
    final trial = await _recorder.finished;
    await _state.saveTrial(trial);
    _ticker?.cancel();
    if (!mounted) return;
    setState(() {
      _result = trial;
      _phase = _Phase.finished;
    });
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final cfg = widget.trial.config;
    final running = _phase == _Phase.running;
    return PopScope(
      canPop: !running,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop) {
          ScaffoldMessenger.of(context).showSnackBar(
            const SnackBar(
              content: Text('Use Cancel or Stop to end the trial.'),
            ),
          );
        }
      },
      child: Scaffold(
        appBar: AppBar(title: Text(widget.trial.trialId)),
        body: Column(
          children: [
            Padding(
              padding: const EdgeInsets.all(8),
              child: Text(
                'pair ${cfg.pairId} | ${cfg.fixtureId} (${cfg.payloadBytes} B) | '
                '${cfg.profile} ${cfg.shapeName} ${cfg.fps} fps ${cfg.ecc}',
                style: theme.textTheme.bodySmall,
              ),
            ),
            Expanded(child: _preview()),
            _panel(theme),
          ],
        ),
      ),
    );
  }

  Widget _preview() {
    final port = _port;
    if (_phase == _Phase.denied) {
      return Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Text(
            'Camera permission was not granted. Allow it in the system settings '
            'and open this trial again.${_message == null ? '' : '\n$_message'}',
          ),
        ),
      );
    }
    if (port is PluginScannerPort) {
      return ClipRect(
        child: ParticleScannerPreview(
          scanner: port.scanner,
          placeholder: const Center(child: Text('No camera on this platform')),
        ),
      );
    }
    return const Center(child: Text('Camera preview not available'));
  }

  Widget _panel(ThemeData theme) {
    final r = _recorder;
    final elapsed = r.elapsed;
    final target = kTargetScanTime;
    final past = elapsed > target;
    final need = r.need;
    return SafeArea(
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            if (_phase == _Phase.running) ...[
              Text(
                'Elapsed ${(elapsed.inMilliseconds / 1000).toStringAsFixed(2)} s'
                '${past ? ' | past 2 s: still scanning, there is no deadline' : ''}',
                key: const Key('elapsed'),
                style: theme.textTheme.titleMedium,
              ),
              const SizedBox(height: 4),
              Stack(
                alignment: Alignment.centerLeft,
                children: [
                  LinearProgressIndicator(
                    value: (elapsed.inMilliseconds / 4000).clamp(0.0, 1.0),
                    minHeight: 8,
                  ),
                  // The 2 s marker sits at the middle of a 4 s scale.
                  const Align(
                    alignment: Alignment.center,
                    child: SizedBox(
                      height: 14,
                      child: VerticalDivider(width: 2, thickness: 2),
                    ),
                  ),
                ],
              ),
              const Text('0 s            2 s marker            4 s+'),
              const SizedBox(height: 4),
              Text(
                'Symbols ${r.have} / ${need == 0 ? '?' : need} | '
                'distinct packets ${r.distinctPackets}',
                key: const Key('progress'),
              ),
              LinearProgressIndicator(value: need == 0 ? null : r.have / need),
              const SizedBox(height: 8),
              Row(
                children: [
                  OutlinedButton(
                    key: const Key('receiver-cancel'),
                    onPressed: () => _recorder.cancel(),
                    child: const Text('Cancel'),
                  ),
                  const SizedBox(width: 8),
                  FilledButton.tonal(
                    key: const Key('receiver-stop'),
                    onPressed: () => _recorder.stopIncomplete(),
                    child: const Text('Stop (incomplete)'),
                  ),
                ],
              ),
            ] else if (_phase == _Phase.ready) ...[
              const Text(
                'Point the camera at the sender, then tap Start. The trial clock '
                'starts when you tap.',
              ),
              const SizedBox(height: 8),
              FilledButton(
                key: const Key('receiver-start'),
                onPressed: _start,
                child: const Text('Start'),
              ),
            ] else if (_phase == _Phase.preparing)
              const Text('Requesting camera permission...')
            else if (_phase == _Phase.finished)
              _resultCard(theme),
          ],
        ),
      ),
    );
  }

  Widget _resultCard(ThemeData theme) {
    final t = _result!;
    final r = t.receiver;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Text('Outcome: ', style: theme.textTheme.titleMedium),
            StatusChip(t),
          ],
        ),
        KeyValue(
          'Reason',
          '${t.reason}${t.reasonDetail == null ? '' : ' (${t.reasonDetail})'}',
        ),
        KeyValue('Completion', fmtSeconds(r?.completionMs.value)),
        KeyValue(
          'Within 2 s',
          r?.withinTwoSeconds == null ? 'n/a' : '${r!.withinTwoSeconds}',
        ),
        KeyValue('Integrity', r?.integrity?.name ?? 'n/a'),
        KeyValue('Camera startup', fmtMs(r?.cameraStartupMs.value)),
        if (r != null && !r.diagnosticsReceived)
          const Text(
            'The plugin emitted no diagnostics: frame counters and timings are '
            'unavailable in this trial.',
          ),
        const SizedBox(height: 8),
        FilledButton(
          key: const Key('receiver-close'),
          onPressed: () => Navigator.of(context).pop(),
          child: const Text('Close'),
        ),
      ],
    );
  }
}
