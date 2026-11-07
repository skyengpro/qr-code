import 'package:flutter/material.dart';
import 'package:particle_code/particle_code.dart';

import '../app.dart';
import '../fixtures.dart';
import '../model/measurements.dart';
import '../model/observed.dart';
import '../model/trial.dart';
import '../model/trial_config.dart';
import '../trial/sender_recorder.dart';
import '../trial/trial_factory.dart';

/// Shows the fixture as a full-screen code until the tester taps Done.
///
/// The trial starts when the code appears and ends on Done (a "sender session" record);
/// it documents the sender settings and the measured display so the paired receiver
/// trials can be interpreted.
class SenderScreen extends StatefulWidget {
  /// Creates the screen for a trial in the `created` state.
  const SenderScreen({super.key, required this.trial});

  /// The trial to run.
  final Trial trial;

  @override
  State<SenderScreen> createState() => _SenderScreenState();
}

class _SenderScreenState extends State<SenderScreen> {
  late final SenderRecorder _recorder;
  ParticleEncoder? _encoder;
  String? _error;
  double _side = 0;
  bool _ended = false;

  TrialConfig get _config => widget.trial.config;

  @override
  void initState() {
    super.initState();
    final state = SpikeScope.read(context);
    _recorder = SenderRecorder(
      base: widget.trial,
      clock: state.clock,
      wallClock: state.wallClock,
    );
    final fixture = Fixtures.byId(_config.fixtureId)!;
    try {
      _encoder = ParticleEncoder.create(
        fixture.bytes,
        options: EncoderOptions(
          profile: CodeProfile.fromLetter(_config.profile),
          shape: Shape(_config.shapeId),
          fps: _config.fps,
          isStatic: _config.isStatic,
          repairPerPass: _config.repairPerPass,
          segmentKbLog2: _config.segmentKbLog2,
          // The pair id is the code's session id, so the receiver reads it from the code.
          sessionId: TrialFactory.sessionIdOf(_config.pairId),
        ),
      );
    } on Object catch (e) {
      _error = '$e';
    }
    WidgetsBinding.instance.addPostFrameCallback((_) => _begin());
  }

  @override
  void dispose() {
    final encoder = _encoder;
    _encoder = null;
    if (encoder != null) {
      WidgetsBinding.instance.addPostFrameCallback((_) => encoder.dispose());
    }
    super.dispose();
  }

  SenderMeasurements _measure() {
    final media = MediaQuery.of(context);
    final info = _encoder?.info;
    final dpr = media.devicePixelRatio;
    return SenderMeasurements(
      logicalWidth: Observed<double>.measured(media.size.width),
      logicalHeight: Observed<double>.measured(media.size.height),
      devicePixelRatio: Observed<double>.measured(dpr),
      codeSideLogicalPx: _side > 0
          ? Observed<double>.measured(_side)
          : const Observed.unknown(),
      codeSidePhysicalPx: _side > 0
          ? Observed<double>.measured(_side * dpr)
          : const Observed.unknown(),
      dotsAcross: Observed<int>.measuredOrUnknown(
        info == null
            ? null
            : CodeProfile.fromLetter(_config.profile).dotsAcross,
      ),
      dotCount: Observed<int>.measuredOrUnknown(info?.dotCount),
      blockSizeBytes: Observed<int>.measuredOrUnknown(info?.blockSize),
      segmentCount: Observed<int>.measuredOrUnknown(info?.segmentCount),
      framesPerPass: Observed<int>.measuredOrUnknown(info?.framesPerPass),
      frameHalfSidePitches: Observed<double>.measuredOrUnknown(
        info?.frameHalfSide,
      ),
      quietZonePitches: Observed<double>.measuredOrUnknown(info?.quietZone),
      pairIdOnScreen: Observed<String>.measuredOrUnknown(
        info == null ? null : TrialFactory.pairIdOf(info.sessionId),
      ),
    );
  }

  Future<void> _begin() async {
    if (!mounted) return;
    final state = SpikeScope.of(context);
    if (_error != null) {
      final t = _recorder.fail(_measure(), _error!);
      _ended = true;
      await state.saveTrial(t);
      return;
    }
    await state.saveTrial(_recorder.begin(_measure()));
  }

  Future<void> _end(bool done) async {
    if (_ended) return;
    _ended = true;
    final state = SpikeScope.of(context);
    final navigator = Navigator.of(context);
    final m = _measure();
    final t = done ? _recorder.done(m) : _recorder.cancel(m);
    await state.saveTrial(t);
    navigator.pop();
  }

  @override
  Widget build(BuildContext context) {
    final encoder = _encoder;
    final motion = switch (_config.motion) {
      MotionChoice.stationary => null,
      MotionChoice.breathing => const MotionConfig(mood: Mood.breathing),
      MotionChoice.flowField => const MotionConfig(mood: Mood.flowField),
    };
    return PopScope(
      canPop: false,
      onPopInvokedWithResult: (didPop, _) {
        if (!didPop && _error == null) _end(false);
        if (!didPop && _error != null) Navigator.of(context).pop();
      },
      child: Scaffold(
        backgroundColor: ParticleCodePalette.background,
        body: SafeArea(
          child: Column(
            children: [
              Padding(
                padding: const EdgeInsets.all(8),
                child: Row(
                  children: [
                    Expanded(
                      child: Text(
                        '${widget.trial.trialId} | pair ${_config.pairId} | '
                        '${_config.fixtureId} ${_config.profile} '
                        '${_config.shapeName} ${_config.fps} fps',
                        key: const Key('sender-label'),
                        style: const TextStyle(
                          color: Colors.black87,
                          fontSize: 12,
                        ),
                      ),
                    ),
                    TextButton(
                      key: const Key('sender-cancel'),
                      onPressed: () => _end(false),
                      child: const Text('Cancel'),
                    ),
                    FilledButton(
                      key: const Key('sender-done'),
                      onPressed: _error != null
                          ? () => Navigator.of(context).pop()
                          : () => _end(true),
                      child: Text(_error != null ? 'Close' : 'Done'),
                    ),
                  ],
                ),
              ),
              Expanded(
                child: _error != null
                    ? Center(
                        child: Padding(
                          padding: const EdgeInsets.all(24),
                          child: Text(
                            'Could not create the code: $_error',
                            style: const TextStyle(color: Colors.white),
                          ),
                        ),
                      )
                    : LayoutBuilder(
                        builder: (context, box) {
                          final side = box.biggest.shortestSide;
                          _side = side;
                          return Center(
                            child: ParticleCodeView(
                              encoder: encoder!,
                              motion: motion,
                              size: side,
                            ),
                          );
                        },
                      ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}
