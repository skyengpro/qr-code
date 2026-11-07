import 'dart:convert';

import 'package:flutter/material.dart';

import '../model/trial.dart';
import '../model/trial_config.dart';
import 'common.dart';

/// All recorded facts of one trial.
class TrialDetailScreen extends StatelessWidget {
  /// Creates the screen.
  const TrialDetailScreen({super.key, required this.trial});

  /// The trial.
  final Trial trial;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final t = trial;
    final c = t.config;
    final r = t.receiver;
    final s = t.sender;
    final json = Map<String, Object?>.of(t.toJson());
    // The per-frame list can be thousands of rows; it is in frames.csv.
    final recv = json['receiver'];
    if (recv is Map) json['receiver'] = {...recv, 'frames': '(see frames.csv)'};
    return Scaffold(
      appBar: AppBar(title: Text(t.trialId)),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Row(
            children: [
              StatusChip(t),
              const SizedBox(width: 8),
              Expanded(
                child: Text('${t.reason ?? ''} ${t.reasonDetail ?? ''}'.trim()),
              ),
            ],
          ),
          const SizedBox(height: 8),
          Text('Setup', style: theme.textTheme.titleMedium),
          KeyValue('Role / pair', '${c.role.name} / ${c.pairId}'),
          KeyValue('Device', t.device.label),
          KeyValue(
            'OS',
            '${t.device.text('os_name') ?? ''} ${t.device.text('os_version') ?? 'unavailable'}',
          ),
          KeyValue('Fixture', '${c.fixtureId} (${c.payloadBytes} B)'),
          KeyValue(
            'Settings',
            '${c.profile} | ${c.shapeName} | ${c.fps} fps | ${c.ecc} | '
                'repair ${c.repairPerPass} | ${c.motion.name} | ${c.configSource.name}',
          ),
          KeyValue('Distance', fmtObserved(t.conditions.distanceCm)),
          KeyValue('Code size', fmtObserved(t.conditions.codeSizeMm)),
          KeyValue('Lighting', fmtObserved(t.conditions.lighting)),
          KeyValue(
            'Screen protector',
            fmtObserved(t.conditions.screenProtector),
          ),
          KeyValue('Notes', fmtObserved(t.conditions.notes)),
          KeyValue(
            'Sender hardware',
            fmtObserved(t.annotations.senderHardware),
          ),
          KeyValue('Duration', fmtSeconds(t.durationMs)),
          if (c.role == TrialRole.receiver && r != null) ...[
            const SizedBox(height: 12),
            Text('Receiver', style: theme.textTheme.titleMedium),
            KeyValue('Camera startup', fmtMs(r.cameraStartupMs.value)),
            KeyValue(
              'First accepted packet',
              fmtMs(r.firstAcceptedPacketMs.value),
            ),
            KeyValue('Completion', fmtMs(r.completionMs.value)),
            KeyValue('Within 2 s', '${r.withinTwoSeconds ?? 'n/a'}'),
            KeyValue('Integrity', r.integrity?.name ?? 'n/a'),
            KeyValue('Progress', '${r.have} / ${r.need}'),
            KeyValue(
              'Diagnostics',
              r.diagnosticsReceived ? 'received' : 'none emitted',
            ),
            KeyValue(
              'Frames',
              '${r.counters.frameEvents} events, ${r.counters.decoded} decoded, '
                  '${r.counters.skippedBeforeTotal} skipped before events, '
                  '${r.counters.distinctPackets} distinct packets',
            ),
            KeyValue(
              'Camera buffer',
              r.buffer == null
                  ? 'unavailable'
                  : '${r.buffer!.width}x${r.buffer!.height} ${r.buffer!.format}',
            ),
            KeyValue('Camera pipeline drops', 'unavailable (not observable)'),
            KeyValue(
              'Delivery gap p50/p90',
              '${fmtMs(r.timings['delivery_gap']?.p50)} / ${fmtMs(r.timings['delivery_gap']?.p90)}',
            ),
          ],
          if (c.role == TrialRole.sender && s != null) ...[
            const SizedBox(height: 12),
            Text('Sender display', style: theme.textTheme.titleMedium),
            KeyValue(
              'Screen (logical)',
              '${fmtObserved(s.logicalWidth)} x ${fmtObserved(s.logicalHeight)}',
            ),
            KeyValue('Pixel ratio', fmtObserved(s.devicePixelRatio)),
            KeyValue(
              'Code side',
              '${fmtObserved(s.codeSideLogicalPx)} logical, ${fmtObserved(s.codeSidePhysicalPx)} physical px',
            ),
            KeyValue('Brightness', s.brightnessPolicy),
            KeyValue('Shown for', fmtMs(s.shownMs.value)),
          ],
          const SizedBox(height: 12),
          ExpansionTile(
            title: const Text('Raw record (JSON)'),
            children: [
              SelectableText(
                const JsonEncoder.withIndent('  ').convert(json),
                style: const TextStyle(fontFamily: 'monospace', fontSize: 11),
              ),
            ],
          ),
        ],
      ),
    );
  }
}
