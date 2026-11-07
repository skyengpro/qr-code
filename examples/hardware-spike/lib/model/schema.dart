/// Version of the exported report (`report.json`). Bump on any incompatible change.
const int kReportSchemaVersion = 1;

/// Version of one stored trial record (`trials/<id>.json`).
const int kTrialSchemaVersion = 1;

/// Identifier of the report format.
const String kReportSchemaName = 'particle-code-hardware-spike-report';

/// The product target for a verified scan. The decoder has no deadline: crossing this is
/// a measurement, never a reason to stop or reset the scan.
const Duration kTargetScanTime = Duration(seconds: 2);
