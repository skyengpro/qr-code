import 'dart:io';

import 'package:share_plus/share_plus.dart';

/// What happened when the report was handed to the OS share sheet.
enum ShareOutcomeKind {
  /// The share sheet was shown and the tester chose a target. This is a handoff only:
  /// whether the target app sent anything, or the maintainers received it, is unknown.
  handedToShareSheet,

  /// The tester closed the share sheet without choosing a target.
  dismissed,

  /// The share sheet was shown but the platform cannot report what the tester did.
  unavailable,

  /// The share sheet could not be shown.
  failed;

  /// Parses [name].
  static ShareOutcomeKind parse(Object? v) => ShareOutcomeKind.values
      .firstWhere((k) => k.name == v, orElse: () => ShareOutcomeKind.failed);
}

/// Outcome of one share attempt, with an accurate label.
class ShareOutcome {
  /// Creates an outcome.
  const ShareOutcome(this.kind, [this.error]);

  /// Handed to the share sheet and a target was chosen.
  const ShareOutcome.handedToShareSheet()
    : this(ShareOutcomeKind.handedToShareSheet);

  /// Share sheet dismissed.
  const ShareOutcome.dismissed() : this(ShareOutcomeKind.dismissed);

  /// Result unknown.
  const ShareOutcome.unavailable() : this(ShareOutcomeKind.unavailable);

  /// Failed with [error].
  const ShareOutcome.failed(String error)
    : this(ShareOutcomeKind.failed, error);

  /// What happened.
  final ShareOutcomeKind kind;

  /// Error text for [ShareOutcomeKind.failed].
  final String? error;

  /// Text for the UI. Never claims delivery.
  String get label => switch (kind) {
    ShareOutcomeKind.handedToShareSheet =>
      'Handed to the share sheet (delivery not confirmed)',
    ShareOutcomeKind.dismissed =>
      'Share sheet dismissed, nothing was sent. The report is kept; try again.',
    ShareOutcomeKind.unavailable =>
      'Share sheet shown, but the platform reports no result (delivery not confirmed)',
    ShareOutcomeKind.failed =>
      'Sharing failed: ${error ?? 'unknown error'}. The report is kept; try again.',
  };
}

/// Hands a report bundle to the OS share sheet. No credentials, no backend.
abstract class ResultSharer {
  /// Shares [bundle]. Never throws: failures are returned as [ShareOutcomeKind.failed].
  /// The bundle is never deleted.
  Future<ShareOutcome> share(File bundle, {required String subject});
}

/// [ResultSharer] over `share_plus`.
class SharePlusResultSharer implements ResultSharer {
  /// Creates the sharer; [sharePlus] is a test seam.
  const SharePlusResultSharer({SharePlus? sharePlus}) : _sharePlus = sharePlus;

  final SharePlus? _sharePlus;

  @override
  Future<ShareOutcome> share(File bundle, {required String subject}) async {
    try {
      final result = await (_sharePlus ?? SharePlus.instance).share(
        ShareParams(
          files: [XFile(bundle.path, mimeType: 'application/zip')],
          subject: subject,
          title: subject,
          text:
              'Particle Code hardware spike report ($subject). '
              'Benchmark data only.',
        ),
      );
      return switch (result.status) {
        ShareResultStatus.success => const ShareOutcome.handedToShareSheet(),
        ShareResultStatus.dismissed => const ShareOutcome.dismissed(),
        ShareResultStatus.unavailable => const ShareOutcome.unavailable(),
      };
    } on Object catch (e) {
      return ShareOutcome.failed('$e');
    }
  }
}

/// A sharer for tests: returns a scripted outcome and records what it was given.
class FakeResultSharer implements ResultSharer {
  /// Creates a fake that answers [outcome].
  FakeResultSharer([this.outcome = const ShareOutcome.handedToShareSheet()]);

  /// The next answer.
  ShareOutcome outcome;

  /// Bundles passed to [share], in order.
  final List<File> shared = [];

  @override
  Future<ShareOutcome> share(File bundle, {required String subject}) async {
    shared.add(bundle);
    return outcome;
  }
}
