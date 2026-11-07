import 'package:flutter/material.dart';

import '../app.dart';
import '../model/observed.dart';
import '../model/trial.dart';
import '../model/trial_config.dart';
import '../report/aggregation.dart';
import 'common.dart';
import 'new_trial_screen.dart';
import 'trial_detail_screen.dart';

/// Device summary, annotations, trial list and the Export / Send actions.
class HomeScreen extends StatefulWidget {
  /// Creates the screen.
  const HomeScreen({super.key});

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  final _variant = TextEditingController();
  final _region = TextEditingController();
  final _sender = TextEditingController();
  bool _loaded = false;
  String? _message;

  @override
  void dispose() {
    _variant.dispose();
    _region.dispose();
    _sender.dispose();
    super.dispose();
  }

  void _loadAnnotations() {
    if (_loaded) return;
    _loaded = true;
    final a = SpikeScope.of(context).annotations;
    _variant.text = a.variant.value ?? '';
    _region.text = a.region.value ?? '';
    _sender.text = a.senderHardware.value ?? '';
  }

  Future<void> _saveAnnotations() async {
    final state = SpikeScope.of(context);
    await state.saveAnnotations(
      Annotations(
        variant: Observed<String>.manualOrUnknown(_variant.text.trim()),
        region: Observed<String>.manualOrUnknown(_region.text.trim()),
        senderHardware: Observed<String>.manualOrUnknown(_sender.text.trim()),
      ),
    );
    if (mounted) setState(() => _message = 'Annotations saved.');
  }

  Future<void> _export() async {
    final state = SpikeScope.of(context);
    final r = await state.exportResults();
    if (!mounted) return;
    setState(
      () => _message = r == null
          ? (state.results.lastError ?? 'Export did not run.')
          : 'Exported ${r.trialCount} trial(s) to ${r.file.path}',
    );
  }

  Future<void> _send() async {
    final state = SpikeScope.of(context);
    final r = await state.sendResults();
    if (!mounted) return;
    setState(
      () => _message = r == null
          ? (state.results.lastError ?? 'Send did not run.')
          : r.outcome.label,
    );
  }

  @override
  Widget build(BuildContext context) {
    final state = SpikeScope.of(context);
    _loadAnnotations();
    final theme = Theme.of(context);
    final d = state.device;
    final agg = aggregate(state.trials);
    final results = state.results;
    return Scaffold(
      appBar: AppBar(title: const Text('Particle hardware spike')),
      body: ListView(
        padding: const EdgeInsets.all(16),
        children: [
          Card(
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text('This device', style: theme.textTheme.titleMedium),
                  const SizedBox(height: 4),
                  KeyValue('Model', d.label),
                  KeyValue(
                    'OS',
                    '${d.text('os_name') ?? 'unavailable'} '
                        '${d.text('os_version') ?? ''}',
                  ),
                  KeyValue(
                    'Architecture',
                    d.text('architecture') ?? 'unavailable',
                  ),
                  KeyValue(
                    'Physical RAM',
                    d.text('physical_ram_mb') == null
                        ? 'unavailable'
                        : '${d.text('physical_ram_mb')} MB',
                  ),
                  KeyValue(
                    'App',
                    '${d.text('app_version') ?? 'unavailable'}'
                        '+${d.text('app_build_number') ?? '?'}',
                  ),
                  KeyValue(
                    'Source revision',
                    d.text('source_revision') ??
                        'unavailable (build with --dart-define=SOURCE_REVISION=...)',
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 8),
          Wrap(
            spacing: 8,
            runSpacing: 8,
            children: [
              FilledButton.icon(
                key: const Key('new-trial'),
                onPressed: () => Navigator.of(context).push(
                  MaterialPageRoute<void>(
                    builder: (_) => const NewTrialScreen(),
                  ),
                ),
                icon: const Icon(Icons.add),
                label: const Text('New trial'),
              ),
              OutlinedButton.icon(
                key: const Key('export-results'),
                onPressed: results.busy ? null : _export,
                icon: const Icon(Icons.save_alt),
                label: const Text('Export results'),
              ),
              FilledButton.tonalIcon(
                key: const Key('send-results'),
                onPressed: results.busy ? null : _send,
                icon: const Icon(Icons.ios_share),
                label: const Text('Send results'),
              ),
            ],
          ),
          const SizedBox(height: 8),
          if (_message != null)
            Text(_message!, key: const Key('action-message')),
          if (results.lastExport != null)
            Text(
              'Last export: ${results.lastExport!.file.path} '
              '(${results.lastExport!.trialCount} trials, '
              '${results.lastExport!.createdAtUtc.toIso8601String()})',
              key: const Key('last-export'),
              style: theme.textTheme.bodySmall,
            ),
          if (results.lastShare != null)
            Text(
              'Last send: ${results.lastShare!.outcome.label} '
              '(${results.lastShare!.atUtc.toIso8601String()})',
              key: const Key('last-share'),
              style: theme.textTheme.bodySmall,
            ),
          Text(
            'Send opens the OS share sheet. The recipient is not defined yet: '
            'maintainers will say where to send the file.',
            style: theme.textTheme.bodySmall,
          ),
          const SizedBox(height: 8),
          Card(
            child: Padding(
              padding: const EdgeInsets.all(12),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Text(
                    'Annotations (typed by you)',
                    style: theme.textTheme.titleMedium,
                  ),
                  Text(
                    'Things the app cannot detect. Stored with every trial.',
                    style: theme.textTheme.bodySmall,
                  ),
                  TextField(
                    key: const Key('annotation-variant'),
                    controller: _variant,
                    decoration: const InputDecoration(
                      labelText:
                          'This phone: hardware variant (e.g. SKU, storage/RAM)',
                    ),
                  ),
                  TextField(
                    key: const Key('annotation-region'),
                    controller: _region,
                    decoration: const InputDecoration(
                      labelText: 'This phone: region / carrier variant',
                    ),
                  ),
                  TextField(
                    key: const Key('annotation-sender'),
                    controller: _sender,
                    decoration: const InputDecoration(
                      labelText:
                          'The other phone of the pair: model and variant',
                    ),
                  ),
                  const SizedBox(height: 8),
                  Align(
                    alignment: Alignment.centerRight,
                    child: TextButton(
                      key: const Key('save-annotations'),
                      onPressed: _saveAnnotations,
                      child: const Text('Save annotations'),
                    ),
                  ),
                ],
              ),
            ),
          ),
          const SizedBox(height: 16),
          Text('Trials', style: theme.textTheme.titleMedium),
          Text(
            'Receiver attempts: ${agg.overall.attempts} | verified: '
            '${agg.overall.eventualCompletions} | within 2 s: '
            '${agg.overall.withinTwoSeconds} of ${agg.overall.attempts} attempts | '
            'incomplete ${agg.overall.incomplete}, cancelled ${agg.overall.cancelled}, '
            'error ${agg.overall.error} | sender sessions: ${agg.senderSessions}',
            key: const Key('trial-summary'),
            style: theme.textTheme.bodySmall,
          ),
          Text(
            'Counts from a few trials say nothing about reliability.',
            style: theme.textTheme.bodySmall,
          ),
          if (state.recovered > 0)
            Text(
              '${state.recovered} trial(s) were running when the app closed '
              'and are recorded as incomplete.',
              style: theme.textTheme.bodySmall,
            ),
          if (state.trials.isEmpty)
            const Padding(
              padding: EdgeInsets.symmetric(vertical: 24),
              child: Center(child: Text('No trials yet.')),
            ),
          for (final t in state.trials)
            ListTile(
              key: Key('trial-${t.trialId}'),
              contentPadding: EdgeInsets.zero,
              title: Text(t.trialId),
              subtitle: Text(
                '${t.config.role.name} | pair ${t.config.pairId} | '
                '${t.config.fixtureId} ${t.config.profile} ${t.config.shapeName}'
                '${t.status == TrialStatus.completed && t.receiver?.completionMs.value != null ? ' | ${fmtSeconds(t.receiver!.completionMs.value)}' : ''}',
              ),
              trailing: StatusChip(t),
              onTap: () => Navigator.of(context).push(
                MaterialPageRoute<void>(
                  builder: (_) => TrialDetailScreen(trial: t),
                ),
              ),
            ),
        ],
      ),
    );
  }
}
