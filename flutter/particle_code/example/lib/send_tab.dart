import 'dart:convert';
import 'dart:math' as math;
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:particle_code/particle_code.dart';

import 'payloads.dart';

enum PayloadKind { ticket, credential, text }

enum ShapeChoice {
  disc('Disc', Icons.circle_outlined),
  heart('Heart', Icons.favorite_border),
  roundedSquare('Rounded square', Icons.crop_square),
  hexagon('Hexagon', Icons.hexagon_outlined),
  ring('Ring', Icons.radio_button_unchecked),
  letter('Letter', Icons.abc);

  const ShapeChoice(this.label, this.icon);
  final String label;
  final IconData icon;
}

/// Pick a payload, a look and a mood, then show the code.
class SendTab extends StatefulWidget {
  const SendTab({super.key, this.active = true});

  /// `false` while another tab is in front: the code stops showing and the screen
  /// brightness is restored, but the settings stay.
  final bool active;

  @override
  State<SendTab> createState() => _SendTabState();
}

class _SendTabState extends State<SendTab> {
  PayloadKind _kind = PayloadKind.ticket;
  final TextEditingController _text = TextEditingController(
    text: 'Hello from Particle Code',
  );
  CodeProfile _profile = CodeProfile.m;
  ShapeChoice _shape = ShapeChoice.disc;
  String _letter = 'P';
  Mood _mood = Mood.breathing;
  bool _trails = false;
  bool _heartbeat = false;
  bool _reveal = false;

  ParticleEncoder? _encoder;
  String? _error;

  bool get _showing => _encoder != null;

  @override
  void dispose() {
    _encoder?.dispose();
    _text.dispose();
    super.dispose();
  }

  Uint8List get _payload => switch (_kind) {
    PayloadKind.ticket => SamplePayloads.ticket,
    PayloadKind.credential => SamplePayloads.credential,
    PayloadKind.text => Uint8List.fromList(utf8.encode(_text.text)),
  };

  Shape get _shapeValue => switch (_shape) {
    ShapeChoice.disc => Shape.disc,
    ShapeChoice.heart => Shape.heart,
    ShapeChoice.roundedSquare => Shape.roundedSquare,
    ShapeChoice.hexagon => Shape.hexagon,
    ShapeChoice.ring => Shape.ring,
    ShapeChoice.letter => Shape.letter(_letter),
  };

  MotionConfig get _motion => MotionConfig(
    mood: _mood,
    particleTrails: _trails,
    anchorHeartbeat: _heartbeat,
    reveal: _reveal,
  );

  /// Builds an encoder for the current settings and swaps it in. The old one is freed
  /// after the frame in which the view stops using it.
  void _show() {
    final payload = _payload;
    ParticleEncoder? next;
    String? error;
    if (payload.isEmpty) {
      error = 'Type something to send.';
    } else {
      try {
        next = ParticleEncoder.create(
          payload,
          options: EncoderOptions(profile: _profile, shape: _shapeValue),
        );
      } on ParticleCodeException catch (e) {
        error = e.message;
      }
    }
    final old = _encoder;
    setState(() {
      _encoder = next;
      _error = error;
    });
    _retire(old);
  }

  /// Stops showing and releases the screen brightness lock.
  void _hide() {
    final old = _encoder;
    setState(() {
      _encoder = null;
      _error = null;
    });
    _retire(old);
  }

  void _retire(ParticleEncoder? encoder) {
    if (encoder == null) return;
    WidgetsBinding.instance.addPostFrameCallback((_) => encoder.dispose());
  }

  /// Applies a settings change; a code on screen is rebuilt right away.
  void _change(VoidCallback update) {
    setState(update);
    if (_showing || _error != null) _show();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final encoder = _encoder;
    return Center(
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 560),
        child: ListView(
          padding: const EdgeInsets.fromLTRB(16, 8, 16, 32),
          children: [
            _CodeCard(encoder: encoder, active: widget.active, motion: _motion),
            const SizedBox(height: 12),
            if (encoder != null)
              _InfoLine(
                payloadBytes: _payload.length,
                encoder: encoder,
                profile: _profile,
              ),
            if (_error != null) _ErrorBanner(message: _error!),
            const SizedBox(height: 8),
            if (_showing)
              FilledButton.tonalIcon(
                onPressed: _hide,
                icon: const Icon(Icons.visibility_off_outlined),
                label: const Text('Hide'),
              )
            else
              FilledButton.icon(
                onPressed: _show,
                icon: const Icon(Icons.blur_on),
                label: const Text('Show code'),
              ),
            const SizedBox(height: 24),
            _Section(title: 'Payload', child: _payloadControls(theme)),
            _Section(
              title: 'Profile',
              caption:
                  '${_profile.dotsAcross} dots across. Denser carries more '
                  'per frame but needs a closer, sharper camera.',
              child: SegmentedButton<CodeProfile>(
                showSelectedIcon: false,
                expandedInsets: EdgeInsets.zero,
                segments: [
                  for (final p in CodeProfile.values)
                    ButtonSegment(value: p, label: Text(p.letter)),
                ],
                selected: {_profile},
                onSelectionChanged: (s) => _change(() => _profile = s.first),
              ),
            ),
            _Section(
              title: 'Shape',
              child: Wrap(
                spacing: 8,
                runSpacing: 4,
                crossAxisAlignment: WrapCrossAlignment.center,
                children: [
                  for (final s in ShapeChoice.values)
                    ChoiceChip(
                      avatar: Icon(s.icon, size: 18),
                      label: Text(s.label),
                      selected: _shape == s,
                      showCheckmark: false,
                      onSelected: (_) => _change(() => _shape = s),
                    ),
                  if (_shape == ShapeChoice.letter)
                    DropdownButton<String>(
                      value: _letter,
                      underline: const SizedBox.shrink(),
                      items: [
                        for (var c = 0x41; c <= 0x5A; c++)
                          DropdownMenuItem(
                            value: String.fromCharCode(c),
                            child: Text(String.fromCharCode(c)),
                          ),
                      ],
                      onChanged: (v) => _change(() => _letter = v ?? _letter),
                    ),
                ],
              ),
            ),
            _Section(
              title: 'Mood',
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  SegmentedButton<Mood>(
                    showSelectedIcon: false,
                    expandedInsets: EdgeInsets.zero,
                    segments: const [
                      ButtonSegment(
                        value: Mood.breathing,
                        label: Text('Breathing'),
                        icon: Icon(Icons.air),
                      ),
                      ButtonSegment(
                        value: Mood.flowField,
                        label: Text('Flow field'),
                        icon: Icon(Icons.waves),
                      ),
                    ],
                    selected: {_mood},
                    onSelectionChanged: (s) => _change(() => _mood = s.first),
                  ),
                  const SizedBox(height: 8),
                  Wrap(
                    spacing: 8,
                    children: [
                      FilterChip(
                        label: const Text('Trails'),
                        selected: _trails,
                        onSelected: (v) => _change(() => _trails = v),
                      ),
                      FilterChip(
                        label: const Text('Heartbeat'),
                        selected: _heartbeat,
                        onSelected: (v) => _change(() => _heartbeat = v),
                      ),
                      FilterChip(
                        label: const Text('Reveal'),
                        selected: _reveal,
                        onSelected: (v) => _change(() => _reveal = v),
                      ),
                    ],
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }

  Widget _payloadControls(ThemeData theme) {
    final bytes = _payload.length;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        SegmentedButton<PayloadKind>(
          showSelectedIcon: false,
          expandedInsets: EdgeInsets.zero,
          segments: const [
            ButtonSegment(
              value: PayloadKind.ticket,
              label: Text('Ticket'),
              icon: Icon(Icons.confirmation_number_outlined),
            ),
            ButtonSegment(
              value: PayloadKind.credential,
              label: Text('Credential'),
              icon: Icon(Icons.badge_outlined),
            ),
            ButtonSegment(
              value: PayloadKind.text,
              label: Text('Text'),
              icon: Icon(Icons.notes),
            ),
          ],
          selected: {_kind},
          onSelectionChanged: (s) => _change(() => _kind = s.first),
        ),
        const SizedBox(height: 8),
        if (_kind == PayloadKind.text)
          TextField(
            controller: _text,
            maxLength: 1000,
            minLines: 1,
            maxLines: 4,
            decoration: const InputDecoration(
              border: OutlineInputBorder(),
              labelText: 'Text to send',
            ),
            onChanged: (_) => _change(() {}),
          )
        else
          Text(
            _kind == PayloadKind.ticket
                ? 'A signed event ticket (COSE_Sign1), $bytes bytes.'
                : 'A signed credential with a portrait, $bytes bytes.',
            style: theme.textTheme.bodySmall,
          ),
      ],
    );
  }
}

class _CodeCard extends StatelessWidget {
  const _CodeCard({
    required this.encoder,
    required this.active,
    required this.motion,
  });

  final ParticleEncoder? encoder;
  final bool active;
  final MotionConfig motion;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        final side = math.min(constraints.maxWidth, 440.0);
        final encoder = this.encoder;
        return Center(
          child: ClipRRect(
            borderRadius: BorderRadius.circular(28),
            child: SizedBox.square(
              dimension: side,
              child: ColoredBox(
                color: ParticleCodePalette.background,
                child: encoder == null
                    ? const _CodePlaceholder()
                    : ParticleCodeView(
                        key: const Key('code-view'),
                        encoder: encoder,
                        motion: motion,
                        showing: active,
                      ),
              ),
            ),
          ),
        );
      },
    );
  }
}

class _CodePlaceholder extends StatelessWidget {
  const _CodePlaceholder();

  @override
  Widget build(BuildContext context) {
    const ink = Color(0xFF6B665C);
    return const Center(
      child: Column(
        mainAxisSize: MainAxisSize.min,
        children: [
          Icon(Icons.blur_on, size: 56, color: ink),
          SizedBox(height: 8),
          Text(
            'Your code appears here',
            style: TextStyle(color: ink, fontSize: 15),
          ),
        ],
      ),
    );
  }
}

class _InfoLine extends StatelessWidget {
  const _InfoLine({
    required this.payloadBytes,
    required this.encoder,
    required this.profile,
  });

  final int payloadBytes;
  final ParticleEncoder encoder;
  final CodeProfile profile;

  @override
  Widget build(BuildContext context) {
    final session = encoder.sessionId
        .toRadixString(16)
        .toUpperCase()
        .padLeft(4, '0');
    return Text(
      '$payloadBytes bytes · ${encoder.framesPerPass} frames per pass · '
      'profile ${profile.letter} · session $session',
      key: const Key('info-line'),
      textAlign: TextAlign.center,
      style: Theme.of(context).textTheme.bodySmall?.copyWith(
        color: Theme.of(context).colorScheme.onSurfaceVariant,
        fontFeatures: const [FontFeature.tabularFigures()],
      ),
    );
  }
}

class _ErrorBanner extends StatelessWidget {
  const _ErrorBanner({required this.message});

  final String message;

  @override
  Widget build(BuildContext context) {
    final cs = Theme.of(context).colorScheme;
    return Container(
      key: const Key('send-error'),
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: cs.errorContainer,
        borderRadius: BorderRadius.circular(12),
      ),
      child: Row(
        children: [
          Icon(Icons.error_outline, color: cs.onErrorContainer),
          const SizedBox(width: 12),
          Expanded(
            child: Text(message, style: TextStyle(color: cs.onErrorContainer)),
          ),
        ],
      ),
    );
  }
}

class _Section extends StatelessWidget {
  const _Section({required this.title, required this.child, this.caption});

  final String title;
  final String? caption;
  final Widget child;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.only(bottom: 20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(title, style: theme.textTheme.titleSmall),
          const SizedBox(height: 8),
          child,
          if (caption != null) ...[
            const SizedBox(height: 6),
            Text(caption!, style: theme.textTheme.bodySmall),
          ],
        ],
      ),
    );
  }
}
