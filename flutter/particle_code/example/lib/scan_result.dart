import 'dart:convert';
import 'dart:typed_data';

import 'package:flutter/material.dart';
import 'package:particle_code/particle_code.dart';

/// `1.2 KB` style size.
String formatBytes(int n) {
  if (n < 1024) return '$n B';
  return '${(n / 1024).toStringAsFixed(1)} KB';
}

/// The first [max] bytes as lowercase hex pairs, with the number left over.
String hexPreview(Uint8List bytes, {int max = 48}) {
  final shown = bytes.length < max ? bytes.length : max;
  final hex = [
    for (var i = 0; i < shown; i++) bytes[i].toRadixString(16).padLeft(2, '0'),
  ].join(' ');
  final rest = bytes.length - shown;
  return rest > 0 ? '$hex  … +$rest more' : hex;
}

/// The payload as text, or `null` if it is not valid UTF-8 or looks binary.
String? utf8Preview(Uint8List bytes, {int maxChars = 240}) {
  if (bytes.isEmpty) return null;
  final String text;
  try {
    text = utf8.decode(bytes);
  } on FormatException {
    return null;
  }
  for (final unit in text.runes) {
    final control = unit < 0x20 && unit != 0x0A && unit != 0x0D && unit != 0x09;
    if (control || unit == 0x7F) return null;
  }
  return text.length > maxChars ? '${text.substring(0, maxChars)}…' : text;
}

/// What a finished scan shows: size, previews, liveness and a way to go again.
class ScanResultSheet extends StatelessWidget {
  const ScanResultSheet({
    super.key,
    required this.payload,
    required this.sessionId,
    required this.liveness,
  });

  final Uint8List payload;
  final int sessionId;

  /// The last liveness reading before the code completed, if any.
  final ScanLiveness? liveness;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final cs = theme.colorScheme;
    final text = utf8Preview(payload);
    final session = sessionId.toRadixString(16).toUpperCase().padLeft(4, '0');
    return SafeArea(
      child: SingleChildScrollView(
        padding: const EdgeInsets.fromLTRB(24, 0, 24, 24),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Row(
              children: [
                Icon(Icons.check_circle, color: cs.primary, size: 32),
                const SizedBox(width: 12),
                Text('Code received', style: theme.textTheme.titleLarge),
              ],
            ),
            const SizedBox(height: 16),
            _Row(
              label: 'Size',
              value: '${payload.length} bytes (${formatBytes(payload.length)})',
            ),
            _Row(label: 'Session', value: session),
            _Row(label: 'Liveness consistent', value: _livenessText()),
            const SizedBox(height: 12),
            Text('Hex', style: theme.textTheme.titleSmall),
            const SizedBox(height: 4),
            SelectableText(
              hexPreview(payload),
              key: const Key('hex-preview'),
              style: theme.textTheme.bodySmall?.copyWith(
                fontFamily: 'monospace',
              ),
            ),
            if (text != null) ...[
              const SizedBox(height: 12),
              Text('Text', style: theme.textTheme.titleSmall),
              const SizedBox(height: 4),
              SelectableText(text, key: const Key('text-preview')),
            ],
            const SizedBox(height: 20),
            SizedBox(
              width: double.infinity,
              child: FilledButton.icon(
                onPressed: () => Navigator.of(context).pop(),
                icon: const Icon(Icons.qr_code_scanner),
                label: const Text('Scan again'),
              ),
            ),
          ],
        ),
      ),
    );
  }

  String _livenessText() {
    final l = liveness;
    if (l == null) return 'not read';
    return l.consistent ? 'yes' : 'no';
  }
}

class _Row extends StatelessWidget {
  const _Row({required this.label, required this.value});

  final String label;
  final String value;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Padding(
      padding: const EdgeInsets.symmetric(vertical: 2),
      child: Row(
        children: [
          Expanded(
            child: Text(
              label,
              style: theme.textTheme.bodyMedium?.copyWith(
                color: theme.colorScheme.onSurfaceVariant,
              ),
            ),
          ),
          Text(value, style: theme.textTheme.bodyMedium),
        ],
      ),
    );
  }
}
