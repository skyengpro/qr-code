import 'package:flutter/material.dart';

import '../app.dart';
import '../fixtures.dart';
import '../model/observed.dart';
import '../model/trial_config.dart';
import 'receiver_screen.dart';
import 'sender_screen.dart';

/// Chooses role, pair id, fixture, code settings and manual conditions, then starts the
/// sender or receiver screen.
class NewTrialScreen extends StatefulWidget {
  /// Creates the screen.
  const NewTrialScreen({super.key});

  @override
  State<NewTrialScreen> createState() => _NewTrialScreenState();
}

class _NewTrialScreenState extends State<NewTrialScreen> {
  TrialRole _role = TrialRole.receiver;
  final _pair = TextEditingController();
  Fixture _fixture = Fixtures.f1k;
  String _profile = 'M';
  int _shapeId = 0;
  int _fps = 10;
  bool _static = false;
  int _repair = 2;
  MotionChoice _motion = MotionChoice.breathing;
  final _distance = TextEditingController();
  final _size = TextEditingController();
  final _lighting = TextEditingController();
  final _protector = TextEditingController();
  final _notes = TextEditingController();
  String? _error;
  bool _initialized = false;

  @override
  void dispose() {
    for (final c in [_pair, _distance, _size, _lighting, _protector, _notes]) {
      c.dispose();
    }
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final state = SpikeScope.of(context);
    if (!_initialized) {
      _initialized = true;
      _pair.text = state.newPairId();
    }
    final theme = Theme.of(context);
    final receiver = _role == TrialRole.receiver;
    return Scaffold(
      appBar: AppBar(title: const Text('New trial')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          SegmentedButton<TrialRole>(
            key: const Key('role'),
            showSelectedIcon: false,
            segments: const [
              ButtonSegment(
                value: TrialRole.sender,
                label: Text('Sender'),
                icon: Icon(Icons.blur_on),
              ),
              ButtonSegment(
                value: TrialRole.receiver,
                label: Text('Receiver'),
                icon: Icon(Icons.photo_camera_outlined),
              ),
            ],
            selected: {_role},
            onSelectionChanged: (s) => setState(() => _role = s.first),
          ),
          const SizedBox(height: 8),
          Text(
            receiver
                ? 'Receiver: declare the settings the sender uses (the receiver '
                      'cannot read them all). Use the same pair id on both phones.'
                : 'Sender: shows the code with these settings. Use the same pair id '
                      'on the receiver.',
            style: theme.textTheme.bodySmall,
          ),
          TextField(
            key: const Key('pair-id'),
            controller: _pair,
            decoration: const InputDecoration(labelText: 'Pair id'),
          ),
          const SizedBox(height: 16),
          Text('Fixture', style: theme.textTheme.titleSmall),
          Wrap(
            spacing: 8,
            children: [
              for (final f in Fixtures.all)
                ChoiceChip(
                  key: Key('fixture-${f.id}'),
                  label: Text('${f.id} (${f.byteCount} B)'),
                  selected: _fixture == f,
                  onSelected: (_) => setState(() => _fixture = f),
                ),
            ],
          ),
          const SizedBox(height: 12),
          Text('Profile', style: theme.textTheme.titleSmall),
          SegmentedButton<String>(
            showSelectedIcon: false,
            segments: const [
              ButtonSegment(value: 'S', label: Text('S')),
              ButtonSegment(value: 'M', label: Text('M')),
              ButtonSegment(value: 'L', label: Text('L')),
            ],
            selected: {_profile},
            onSelectionChanged: (s) => setState(() => _profile = s.first),
          ),
          const SizedBox(height: 12),
          Text('Shape', style: theme.textTheme.titleSmall),
          Wrap(
            spacing: 8,
            children: [
              for (final s in ShapeOption.all)
                ChoiceChip(
                  label: Text(s.name),
                  selected: _shapeId == s.id,
                  onSelected: (_) => setState(() => _shapeId = s.id),
                ),
            ],
          ),
          const SizedBox(height: 12),
          Text('ECC / mode', style: theme.textTheme.titleSmall),
          SegmentedButton<bool>(
            showSelectedIcon: false,
            segments: const [
              ButtonSegment(value: false, label: Text('Animated RS(255,191)')),
              ButtonSegment(value: true, label: Text('Static RS(255,159)')),
            ],
            selected: {_static},
            onSelectionChanged: (s) => setState(() => _static = s.first),
          ),
          const SizedBox(height: 12),
          Row(
            children: [
              Expanded(
                child: DropdownButtonFormField<int>(
                  initialValue: _fps,
                  decoration: const InputDecoration(
                    labelText: 'Update rate (fps)',
                  ),
                  items: [
                    for (final v in const [5, 8, 10, 12, 15, 20, 30])
                      DropdownMenuItem(value: v, child: Text('$v')),
                  ],
                  onChanged: (v) => setState(() => _fps = v ?? _fps),
                ),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: DropdownButtonFormField<int>(
                  initialValue: _repair,
                  decoration: const InputDecoration(
                    labelText: 'Repair per pass',
                  ),
                  items: [
                    for (final v in const [0, 1, 2, 3, 4, 6])
                      DropdownMenuItem(value: v, child: Text('$v')),
                  ],
                  onChanged: (v) => setState(() => _repair = v ?? _repair),
                ),
              ),
            ],
          ),
          const SizedBox(height: 12),
          Text('Motion', style: theme.textTheme.titleSmall),
          SegmentedButton<MotionChoice>(
            showSelectedIcon: false,
            segments: const [
              ButtonSegment(
                value: MotionChoice.stationary,
                label: Text('Stationary'),
              ),
              ButtonSegment(
                value: MotionChoice.breathing,
                label: Text('Breathing'),
              ),
              ButtonSegment(
                value: MotionChoice.flowField,
                label: Text('Flow field'),
              ),
            ],
            selected: {_motion},
            onSelectionChanged: (s) => setState(() => _motion = s.first),
          ),
          const SizedBox(height: 16),
          Text(
            'Conditions (manual, optional)',
            style: theme.textTheme.titleSmall,
          ),
          TextField(
            controller: _distance,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(labelText: 'Distance (cm)'),
          ),
          TextField(
            controller: _size,
            keyboardType: const TextInputType.numberWithOptions(decimal: true),
            decoration: const InputDecoration(
              labelText: 'Physical code size on the sender screen (mm)',
            ),
          ),
          TextField(
            controller: _lighting,
            decoration: const InputDecoration(
              labelText: 'Lighting (e.g. office, dim, sunlight)',
            ),
          ),
          TextField(
            controller: _protector,
            decoration: const InputDecoration(
              labelText: 'Screen protector (none, glass, film ...)',
            ),
          ),
          TextField(
            controller: _notes,
            decoration: const InputDecoration(labelText: 'Notes'),
            maxLines: 2,
          ),
          if (_error != null)
            Padding(
              padding: const EdgeInsets.only(top: 8),
              child: Text(
                _error!,
                style: TextStyle(color: theme.colorScheme.error),
              ),
            ),
          const SizedBox(height: 16),
          FilledButton(
            key: const Key('create-trial'),
            onPressed: () => _create(context),
            child: Text(receiver ? 'Prepare receiver trial' : 'Show code'),
          ),
        ],
      ),
    );
  }

  Observed<double> _number(TextEditingController c, String name) {
    final t = c.text.trim();
    if (t.isEmpty) return const Observed.unknown();
    final v = double.tryParse(t.replaceAll(',', '.'));
    if (v == null || v < 0) throw FormatException('$name must be a number');
    return Observed<double>.manual(v);
  }

  void _create(BuildContext context) {
    final state = SpikeScope.of(context);
    final navigator = Navigator.of(context);
    if (_pair.text.trim().isEmpty) {
      setState(() => _error = 'Enter a pair id.');
      return;
    }
    final Observed<double> distance;
    final Observed<double> size;
    try {
      distance = _number(_distance, 'Distance');
      size = _number(_size, 'Code size');
    } on FormatException catch (e) {
      setState(() => _error = e.message);
      return;
    }
    final config = TrialConfig(
      role: _role,
      pairId: _pair.text.trim(),
      fixtureId: _fixture.id,
      payloadBytes: _fixture.byteCount,
      profile: _profile,
      shapeId: _shapeId,
      fps: _fps,
      isStatic: _static,
      repairPerPass: _repair,
      motion: _motion,
      configSource: _role == TrialRole.receiver
          ? ConfigSource.declared
          : ConfigSource.configured,
    );
    final conditions = Conditions(
      distanceCm: distance,
      codeSizeMm: size,
      lighting: Observed<String>.manualOrUnknown(_lighting.text.trim()),
      screenProtector: Observed<String>.manualOrUnknown(_protector.text.trim()),
      notes: Observed<String>.manualOrUnknown(_notes.text.trim()),
    );
    final trial = state.createTrial(config, conditions);
    navigator.pushReplacement(
      MaterialPageRoute<void>(
        builder: (_) => _role == TrialRole.sender
            ? SenderScreen(trial: trial)
            : ReceiverScreen(trial: trial),
      ),
    );
  }
}
