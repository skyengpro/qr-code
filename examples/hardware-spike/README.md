# Particle Code hardware benchmark spike

A standalone Flutter app (Android and iOS) that records repeatable sender/receiver trials of the
[`particle_code`](../../flutter/particle_code/README.md) plugin on real phones, stores them,
and exports a versioned report bundle that testers send back to the maintainers.

It uses only the plugin's public API (`path:` dependency on `../../flutter/particle_code`): no
decoder of its own, no copied Rust libraries. Benchmark screens, trial management, storage,
export and sharing live here and nowhere else.

**Targets.** The product target is a verified scan within two seconds when possible. The decoder
has **no deadline**: crossing two seconds keeps the scan running and keeps its progress. The app
only records whether a completion came in time; it never stops, resets or restarts the scanner
because time passed. A trial ends only by a verified completion, the tester pressing
**Stop** or **Cancel**, or a native error.

**What this proves, and what it does not.** Results from a few devices, or from successful trials
only, are **not** evidence of CEMAC-wide compatibility, of population coverage, or of
two-second reliability. The report divides the two-second count by *all* attempts and shows
incomplete, cancelled and failed attempts next to it; latency percentiles describe verified
completions only. Read them as measurements of the recorded conditions, nothing more.

## Run it

```sh
cd examples/hardware-spike
flutter run --dart-define=SOURCE_REVISION=$(git rev-parse HEAD)     # on a phone
flutter build apk --dart-define=SOURCE_REVISION=$(git rev-parse HEAD)
```

Always pass `SOURCE_REVISION`; without it the report says the source revision is *unavailable*.
Build the same revision for both phones of a pair. The first build compiles the Rust library
through the plugin's Native Assets hook (see the demo's README for toolchain needs). The
iOS deployment target is 15.0 and the Android `minSdk` 24, as in the plugin. Simulators and
emulators have no camera that can scan; use phones.

## Store builds (internal testing)

Secrets live outside the repo in `~/.keys/hardware-spike/` (back it up): the Android upload
keystore and `key.properties` (made by `tool/setup_android_key.sh`), `ios.env` (`APPLE_TEAM_ID`,
optionally `ASC_KEY_ID` / `ASC_ISSUER_ID`) and the App Store Connect key `AuthKey_<ID>.p8`.

```sh
tool/release.sh android   # build/app/outputs/bundle/release/app-release.aab -> Play Console, Internal testing
tool/release.sh ios       # build/ios/ipa/*.ipa -> TestFlight (uploaded when the ASC key is set, else Transporter)
```

The build number is minutes since the epoch (override with `BUILD_NUMBER`), so every run is
uploadable. The first Play upload must be done by hand in the console.

Store assets: `tool/make_icons.sh` rebuilds the app icons from `assets/icon/icon-source.png`
(and `assets/icon/play-store-512.png`); `tool/screenshots_all.sh [ios|android]` captures the store
screenshots on simulators/emulators into `build/screenshots/<device>/` (no receiver shot: it needs
a camera).

## How a tester runs trials

Start with the pair **Pixel 9 Pro XL and iPhone 13**, in both directions (each phone as sender,
the other as receiver), then older and low-memory Android phones as receivers.

1. On each phone open **Home** and fill in the **annotations**: the hardware variant (SKU,
   RAM/storage), the region/carrier variant, and the *other* phone's model and variant (the app
   cannot detect a remote phone). Press **Save annotations**.
2. **New trial** on both phones. Nothing has to be typed to link them: the sender gets a fresh
   pair id (`P-xxxx`) and puts it **inside the code** as its session id, and the receiver reads it
   from the code (`observed_pair_id`). On the sender, pick the fixture, profile (S/M/L), shape,
   update rate, animated/static, repair per pass and motion. The receiver's settings are only
   *declared*; the sender's report has the real ones, and `tool/pair_reports.dart` uses those.
   Optionally note distance, lighting, screen protector, the physical size of the code on the
   sender's screen, and free notes; these are stored as manual entries.
   The form **remembers its last settings** (`last_setup.json`, this phone only), so the next
   trial only needs the one or two settings under test changed; **Reset** restores the defaults.
3. **Sender**: press **Show code**. The code fills the screen at maximum brightness (forced by the
   plugin) with the trial id above it. Keep it up until the receiver is finished, then **Done**.
4. **Receiver**: press **Prepare receiver trial**, allow the camera, aim at the sender and press
   **Start**. Watch elapsed time, symbols held/needed and distinct packets; the bar has a 2 s
   marker, and the scan simply carries on past it. It ends when the payload is verified.
   **Stop (incomplete)** ends a scan that is going nowhere (recorded as *incomplete*); **Cancel**
   discards intent (recorded as *cancelled*). Both are kept and counted.
5. Repeat. Several trials per configuration; vary one thing at a time; do not delete failures.
6. **Export results** on both phones and collect the zips in one folder. Then pair them:

   ```sh
   dart run tool/pair_reports.dart <folder-or-zips> -o paired.csv
   ```

   One row per receiver trial with the sender's real settings next to the outcome. `match` says
   how it was paired: `pairId` (read from the code), `time` (no pair id read, or a report from
   before 0.1.3: the one sender trial on another phone showing a code at the time),
   `ambiguous` or `unpaired`. `integrity_vs_sender` checks the decoded bytes against the fixture
   the sender really showed, and `declared_differs` lists receiver settings that were declared
   wrong (not an error of the decoder).

### Fixtures

Deterministic synthetic payloads (pseudo-random bytes, no real data), identical on every
device; the receiver compares the decoded bytes with the selected fixture (size and SHA-256).

| Id | Bytes |
| --- | ---: |
| `f200` | 200 |
| `f1k` | 1 024 |
| `f6k` | 6 144 |
| `f30k` | 30 720 |

A decoded payload that differs from the fixture is recorded as an *error* (`integrity_mismatch`),
not as a success.

## Export and send

On Home:

* **Export results** writes a report bundle (a zip) into the app's documents directory
  (`reports/particle-spike-report-<UTC time>.zip`). It is plain file IO and works offline. The
  path is shown on screen.
* **Send results** exports (or reuses the up-to-date bundle) and opens the **OS share sheet** with
  the zip. There are no credentials and no backend for results (the optional error telemetry under
  *Privacy* is separate). Cancelling or failing leaves the
  bundle on disk, and the outcome is shown and kept, so you can press **Send results** again.

**Where to send it.** The collection recipient or channel is **not yet defined**. Until the
maintainers supply one, export, keep the bundle, and send it to whatever address or channel they
name when they ask for results. The app deliberately contains no address.

A share-sheet handoff is **not** evidence that anything arrived. The app says *"Handed to the
share sheet (delivery not confirmed)"* when you picked a target, *"Share sheet dismissed"* when
you closed it, and *"... the platform reports no result"* when the OS cannot tell. Confirm with
the maintainers that they received the file.

## What is measured

Sources are explicit in the data: **measured** (read by the app), **manual** (typed by the tester)
or **unavailable** (`value: null`, `source: "unavailable"`).

Receiver:

* Trial start, camera startup (`ScanCameraStarted.startup`), time to the first accepted packet,
  time to verified completion, `within_two_seconds`, integrity, incomplete/cancelled/error
  outcomes with a reason.
* Camera buffer size, pixel format, strides and rotation, the selected camera id and facing.
* Per camera frame (from `ParticleScanner.diagnostics`, capped at 3 000 records per trial; counters
  and statistics are never capped): capture-timestamp delivery gap, capture-to-receipt latency,
  frames the plugin received and closed unsent (`skipped_before`), native hold time, bridge round
  trip, Rust stage timings (validate, locate, read, receive), reject stage and category, accepted
  and distinct packets, receiver progress.
* Profile and shape read from the code's header.

Sender: configuration, logical screen size, device pixel ratio, the code's side in logical and
physical pixels, lattice facts (dots across, dot count, block size, segments, frame geometry), and
how long the code was shown. Screen brightness is forced to maximum by the plugin and not read
back, so the level is recorded as unavailable with that explanation.

Device (both): model identifier, manufacturer/brand/codename (Android), OS name, version and build
where the OS exposes it (not on iOS), API level, CPU architecture (Dart `Abi`), supported ABIs,
physical RAM (`device_info_plus`), low-RAM flag, physical vs simulator, app version and build
number, and the source revision.

### What it cannot observe

* Frames dropped inside the camera pipeline before delivery to the plugin
  (`camera_pipeline_drops` is always *unavailable*). Gaps between capture timestamps show them only
  indirectly, and the app infers nothing from the rate-limited `ScanFrame` UI events.
* The sender's actual hardware (annotation only), the real brightness, the physical size of the
  code (manual), distance and lighting (manual).
* iOS OS build number, and RAM or other fields a platform does not report.
* Whether anyone received a shared report.

### Clocks

* **Trial start** is the tester tapping **Start** on the receiver; the camera start is requested
  immediately after. Permission prompts happen before, so they are not in the measurement.
* Every duration the app computes (`first_accepted_packet_ms`, `completion_ms`, `duration_ms`) is
  a difference of two readings of one Dart `Stopwatch` (monotonic), taken when the app receives
  the event or diagnostics record. That adds the (small) Dart event delivery delay to each.
* `camera_startup_ms` is the plugin's own stopwatch, from `ParticleScanner.start` to the first
  frame event.
* Native capture timestamps are subtracted from each other only inside one native clock
  (`android_realtime`, `android_unknown`, `ios_host`, per `NativeTimestamp.since`); mixed or
  unknown clocks are skipped and counted in `cross_clock_pairs_ignored`. Native timestamps are
  never compared with Dart stopwatch values.
* Wall-clock `*_utc` fields order records and are never subtracted to get a duration.

## Report bundle

`particle-spike-report-<yyyyMMdd-HHmmss>Z.zip` contains:

| File | Content |
| --- | --- |
| `report.json` | Versioned machine-readable report (below). |
| `trials.csv` | One row per trial, every outcome including incomplete, cancelled and error; unknown values are empty. |
| `summary.csv` | One row per configuration group: attempts, completed within 2 s, eventual completions, incomplete, cancelled, error, `within_2s_of_all_attempts`, and p50/p90/max/count of **successful** completion latency. |
| `frames.csv` | Per-frame diagnostics of the receiver trials. |
| `README.txt` | Short description and the caveat above. |

### `report.json` (schema `particle-code-hardware-spike-report`, version 1)

Top level: `schema`, `schema_version`, `trial_schema_version`, `generated_at_utc`, `app`
(name, version, build number, source revision), `exporting_device`, `annotations`,
`target_scan_time_ms` (2000), `units`, `definitions`, `privacy_exclusions`, `caveats`, `counts`,
`aggregation` (`groups`, `overall`, `sender_sessions`), `trials`.

Each trial: `trial_id`, `status` (`completed | incomplete | cancelled | error`), `reason`,
`reason_detail`, `created_at_utc`, `started_at_utc`, `ended_at_utc`, `duration_ms`, `config`
(role, pair id, fixture id and bytes, profile, shape id and name, fps, static, ECC, repair per
pass, motion, `config_source` = `configured` for the sender and `declared` for the receiver),
`conditions` (manual), `annotations`, `device`, and `sender` or `receiver` measurements.
Pairing fields: the sender's `pair_id_on_screen` (the session id in its headers), the receiver's
`observed_pair_id` (read from the headers, the most frequent if several codes were seen) and
`pair_ids_seen` (frames per pair id), and each frame's `session_id` (`pair_id` in `frames.csv`;
`observed_pair_id` in `trials.csv`).

Units: `*_ms` milliseconds, `*_px` pixels (logical or physical as named), `*_mm` millimetres,
`*_cm` centimetres, `*_bytes` bytes, `physical_ram_mb` megabytes, `*_utc` ISO 8601 UTC, `fps`
frames per second, `*_pitches` dot pitches. The `units` and `definitions` sections of the file
repeat this. The schema version is bumped on any incompatible change; a reader should refuse a
newer version than it knows. The app can reparse its own reports (`parseReport` /
`readBundle` in `lib/report`).

Aggregation groups attempts by receiver model, receiver OS version, sender hardware annotation,
fixture, profile, shape, fps, ECC, repair per pass and motion. Only terminal receiver trials are
attempts; a trial still running (or recovered after an app crash, which is recorded as
*incomplete* with reason `interrupted_by_app_exit`) is handled accordingly.

## Privacy

The report contains benchmark data only: synthetic fixture ids and byte counts, a SHA-256 of the
decoded synthetic bytes, device model/OS/app facts, timings, counters, and what the tester typed
into annotations and notes (do not write personal data there). It excludes credential contents,
session and signing keys, camera images, persistent personal device identifiers (no Android ID,
`identifierForVendor`, serial number, or user-assigned device name) and unrelated logs.

### Error and crash telemetry (optional, off by default)

The app runs inside [`otel_zone`](https://github.com/vaam-apps/flutter-otel-zone): every uncaught
error is kept in a local log, and native crashes of earlier runs are read back from the OS
(MetricKit on iOS, `ApplicationExitInfo` on Android). A build exports them **only** when it was
built with `--dart-define=OTEL_EXPORTER_OTLP_ENDPOINT=https://<collector>` (`tool/release.sh` reads
it from `~/.keys/hardware-spike/otel.env`). Without it nothing leaves the phone, as before.

When on, it sends errors and crashes only (log level `warning` and above, plus the last few
lower-level records around a fault): exception text and stack traces, the route pattern the app
was on (`/sender`, never a trial id), the app version, build and source revision, and the device
model and OS version. No trial data, no annotations, no camera images, no account or device
identifier (`enduser.id` is never set). The platforms keep their raw crash files app-private and
out of backups until the report is delivered.

## Code map

```
lib/model/        immutable models, explicit toJson/fromJson, Observed<T> provenance wrapper
lib/fixtures.dart the four synthetic payloads
lib/device_identity.dart  device_info_plus + package_info_plus + SOURCE_REVISION
lib/trial/        lifecycle state machine, receiver/sender recorders, scanner port, clocks
lib/storage/      one JSON file per trial in the app support directory, atomic writes
lib/report/       report.json, CSVs, aggregation, zip bundle
lib/share/        ResultSharer (share_plus + fake), export/send workflow
lib/ui/           Home, New trial, Sender, Receiver, Trial detail
```

Trials are saved when they start (`running`) and when they end, so they survive an app restart.

## Tests

```sh
flutter test                                   # unit, widget and recorder tests (builds the Rust library for the host)
flutter test integration_test -d <device>      # launch evidence on an emulator/simulator; needs no camera
```

The unit tests drive the recorder with fake `ScanEvent` / `ScanDiagnostic` streams and a manual
clock. They cover diagnostics mapping and timing (including cross-clock delivery gaps), the
trial lifecycle and the absence of a deadline, JSON round trips with unknowns, aggregation
(denominator, percentiles over successes only), CSV rows against the raw trials (including
incomplete attempts), restart persistence, offline export with a zip that is reopened and parsed,
and share dismissal and failure keeping the bundle. The integration test launches the app,
records a sender trial with the real encoder and exports a bundle.
