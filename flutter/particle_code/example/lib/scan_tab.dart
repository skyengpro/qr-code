import 'dart:async';

import 'package:flutter/material.dart';
import 'package:particle_code/particle_code.dart';

import 'scan_result.dart';

enum ScanPhase { starting, running, denied, noCamera, unsupported, failed }

/// Camera scanner: permission, preview with anchors, progress, result sheet.
class ScanTab extends StatefulWidget {
  const ScanTab({super.key, this.scannerFactory = ParticleScanner.new});

  /// Creates the scanner; tests pass one on mocked channels.
  final ParticleScanner Function() scannerFactory;

  @override
  State<ScanTab> createState() => _ScanTabState();
}

class _ScanTabState extends State<ScanTab> {
  late final ParticleScanner _scanner = widget.scannerFactory();
  StreamSubscription<ScanEvent>? _sub;

  ScanPhase _phase = ScanPhase.starting;
  String? _message;
  ScanFrame? _frame;
  int _received = 0;
  bool _torch = false;
  bool _sheetOpen = false;

  @override
  void initState() {
    super.initState();
    _sub = _scanner.events.listen(_onEvent);
    unawaited(_start());
  }

  @override
  void dispose() {
    unawaited(_sub?.cancel());
    _scanner.dispose(); // stops the camera
    super.dispose();
  }

  Future<void> _start() async {
    setState(() {
      _phase = ScanPhase.starting;
      _message = null;
    });
    try {
      if (!await _scanner.requestPermission()) {
        return _fail(ScanPhase.denied);
      }
      await _scanner.start(torch: _torch);
      if (mounted) setState(() => _phase = ScanPhase.running);
    } on ParticleScannerException catch (e) {
      _fail(switch (e.error) {
        ParticleScannerError.permissionDenied => ScanPhase.denied,
        ParticleScannerError.noCamera => ScanPhase.noCamera,
        ParticleScannerError.unsupported => ScanPhase.unsupported,
        _ => ScanPhase.failed,
      }, e.message);
    }
  }

  void _fail(ScanPhase phase, [String? message]) {
    if (!mounted) return;
    setState(() {
      _phase = phase;
      _message = message;
    });
  }

  void _onEvent(ScanEvent event) {
    if (!mounted) return;
    switch (event) {
      case ScanFrame():
        setState(() => _frame = event);
      case ScanSegment():
        setState(() => _received += event.bytes.length);
      case ScanDone():
        unawaited(_showResult(event));
      case ScanError():
        _fail(switch (event.kind) {
          ParticleScannerError.permissionDenied => ScanPhase.denied,
          ParticleScannerError.noCamera => ScanPhase.noCamera,
          _ => ScanPhase.failed,
        }, event.message);
    }
  }

  Future<void> _showResult(ScanDone done) async {
    if (_sheetOpen) return;
    _sheetOpen = true;
    final liveness = _frame?.liveness;
    setState(() => _received = done.payload.length);
    await showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      isScrollControlled: true,
      builder: (_) => ScanResultSheet(
        payload: done.payload,
        sessionId: done.sessionId,
        liveness: liveness,
      ),
    );
    _sheetOpen = false;
    if (!mounted) return;
    // Closing the sheet in any way means "scan again".
    await _scanner.reset();
    if (mounted) {
      setState(() {
        _frame = null;
        _received = 0;
      });
    }
  }

  Future<void> _toggleTorch() async {
    final want = !_torch;
    final hasTorch = await _scanner.setTorch(want);
    if (!mounted) return;
    if (!hasTorch) {
      ScaffoldMessenger.of(context).showSnackBar(
        const SnackBar(content: Text('This device has no torch.')),
      );
      return;
    }
    setState(() => _torch = want);
  }

  String get _hint {
    final f = _frame;
    if (f == null || (!f.locked && f.hint == null)) {
      return 'Point the camera at a Particle Code';
    }
    return f.hint ?? (f.need > 0 ? 'Reading… keep it in view' : 'Code found');
  }

  @override
  Widget build(BuildContext context) {
    if (_phase != ScanPhase.running) {
      return _StatusView(phase: _phase, message: _message, onRetry: _start);
    }
    return ColoredBox(
      color: Colors.black,
      child: Stack(
        fit: StackFit.expand,
        children: [
          ParticleScannerPreview(
            scanner: _scanner,
            overlayBuilder: (context, frame) =>
                ScanOverlay(frame: frame, showProgress: false),
          ),
          Positioned(
            top: 12,
            left: 16,
            right: 72,
            child: _HintPill(text: _hint),
          ),
          Positioned(
            top: 8,
            right: 8,
            child: IconButton.filledTonal(
              key: const Key('torch'),
              tooltip: _torch ? 'Torch off' : 'Torch on',
              onPressed: _toggleTorch,
              icon: Icon(_torch ? Icons.flash_on : Icons.flash_off),
            ),
          ),
          Positioned(
            left: 16,
            right: 16,
            bottom: 16,
            child: _ProgressPanel(frame: _frame, received: _received),
          ),
        ],
      ),
    );
  }
}

class _HintPill extends StatelessWidget {
  const _HintPill({required this.text});

  final String text;

  @override
  Widget build(BuildContext context) {
    return Align(
      alignment: Alignment.centerLeft,
      child: DecoratedBox(
        decoration: BoxDecoration(
          color: Colors.black.withAlpha(150),
          borderRadius: BorderRadius.circular(20),
        ),
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 8),
          child: Text(
            text,
            key: const Key('scan-hint'),
            style: const TextStyle(color: Colors.white, fontSize: 14),
          ),
        ),
      ),
    );
  }
}

class _ProgressPanel extends StatelessWidget {
  const _ProgressPanel({required this.frame, required this.received});

  final ScanFrame? frame;
  final int received;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final f = frame;
    final percent = ((f?.progress ?? 0) * 100).round();
    return Card(
      margin: EdgeInsets.zero,
      color: theme.colorScheme.surface.withAlpha(235),
      child: Padding(
        padding: const EdgeInsets.all(12),
        child: Row(
          children: [
            ScanProgressRing.fromFrame(
              f,
              size: 64,
              trackColor: theme.colorScheme.outlineVariant,
              color: theme.colorScheme.primary,
              child: Text('$percent%', style: theme.textTheme.labelMedium),
            ),
            const SizedBox(width: 16),
            Expanded(
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                mainAxisSize: MainAxisSize.min,
                children: [
                  Text(
                    'Received ${formatBytes(received)}',
                    key: const Key('received'),
                    style: theme.textTheme.titleMedium,
                  ),
                  const SizedBox(height: 2),
                  Text(
                    f == null || f.segmentsTotal == 0
                        ? 'Waiting for the first segment'
                        : 'Segment ${f.segmentsDone} of ${f.segmentsTotal} · '
                              'symbols ${f.have}/${f.need}',
                    style: theme.textTheme.bodySmall,
                  ),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// Everything but the running camera: starting, and the error cases.
class _StatusView extends StatelessWidget {
  const _StatusView({
    required this.phase,
    required this.message,
    required this.onRetry,
  });

  final ScanPhase phase;
  final String? message;
  final VoidCallback onRetry;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    if (phase == ScanPhase.starting) {
      return const Center(child: CircularProgressIndicator());
    }
    final (icon, title, body, action) = switch (phase) {
      ScanPhase.denied => (
        Icons.no_photography_outlined,
        'Camera permission needed',
        'Particle Code reads codes with the camera. Allow camera access, or '
            'enable it for this app in the system settings.',
        'Try again',
      ),
      ScanPhase.noCamera => (
        Icons.videocam_off_outlined,
        'No camera available',
        'This device has no usable camera. Simulators and emulators often '
            'have none; run the app on a phone.',
        'Retry',
      ),
      ScanPhase.unsupported => (
        Icons.phonelink_erase_outlined,
        'Scanning is not available here',
        'The scanner runs on Android and iOS. Showing codes works anywhere '
            'the native library is available.',
        null,
      ),
      _ => (
        Icons.error_outline,
        'The camera stopped',
        message?.isNotEmpty == true
            ? message!
            : 'Something went wrong while using the camera.',
        'Retry',
      ),
    };
    return Center(
      child: Padding(
        padding: const EdgeInsets.all(32),
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 420),
          child: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              Icon(icon, size: 56, color: theme.colorScheme.primary),
              const SizedBox(height: 16),
              Text(
                title,
                key: const Key('status-title'),
                style: theme.textTheme.titleLarge,
                textAlign: TextAlign.center,
              ),
              const SizedBox(height: 8),
              Text(
                body,
                style: theme.textTheme.bodyMedium,
                textAlign: TextAlign.center,
              ),
              if (action != null) ...[
                const SizedBox(height: 24),
                FilledButton(
                  key: const Key('status-action'),
                  onPressed: onRetry,
                  child: Text(action),
                ),
              ],
            ],
          ),
        ),
      ),
    );
  }
}
