import 'package:flutter_test/flutter_test.dart';
import 'package:particle_code_hardware_spike/fixtures.dart';
import 'package:particle_code_hardware_spike/report/csv_codec.dart';
import 'package:particle_code_hardware_spike/report/pairing.dart';

Map<String, Object?> _v(Object? value) => {
  'value': value,
  'source': value == null ? 'unavailable' : 'measured',
};

Map<String, Object?> _trial({
  required String id,
  required String role,
  required String device,
  required String start,
  required String end,
  String pairId = '',
  String? onScreen,
  String? observed,
  String fixture = 'f1k',
  String profile = 'M',
  String status = 'completed',
  String? sha,
}) => {
  'trial_id': id,
  'status': status,
  'created_at_utc': start,
  'started_at_utc': start,
  'ended_at_utc': end,
  'config': {
    'role': role,
    'pair_id': pairId,
    'fixture_id': fixture,
    'profile': profile,
    'shape_id': 0,
    'fps': 10,
    'static': false,
    'repair_per_pass': 2,
    'motion': 'breathing',
  },
  'conditions': {'distance_cm': _v(role == 'receiver' ? 30 : null)},
  'device': {'model_identifier': _v(device), 'os_version': _v('1')},
  if (role == 'sender') 'sender': {'pair_id_on_screen': _v(onScreen)},
  if (role == 'receiver')
    'receiver': {
      'observed_pair_id': _v(observed),
      'received_sha256': sha,
      'counters': {'frame_events': 10},
    },
};

String _t(int s) =>
    DateTime.utc(2026, 11, 7, 10).add(Duration(seconds: s)).toIso8601String();

Map<String, Object?> _report(List<Map<String, Object?>> trials) => {
  'trials': trials,
};

void main() {
  test('a receiver pairs with the sender whose code carried its pair id', () {
    final trials = collectTrials([
      _report([
        _trial(
          id: 'S1',
          role: 'sender',
          device: 'A',
          start: _t(0),
          end: _t(60),
          pairId: 'P-beef',
          onScreen: 'P-beef',
        ),
        _trial(
          id: 'S2',
          role: 'sender',
          device: 'A',
          start: _t(0),
          end: _t(60),
          pairId: 'P-0001',
          onScreen: 'P-0001',
        ),
      ]),
      _report([
        _trial(
          id: 'R1',
          role: 'receiver',
          device: 'B',
          start: _t(10),
          end: _t(20),
          observed: 'P-beef',
        ),
      ]),
    ]);
    final p = pairTrials(trials).single;
    expect(p.match, PairMatch.pairId);
    expect(p.sender!['trial_id'], 'S1');
  });

  test('a repeated 16-bit pair id goes to the sender closest in time', () {
    final pairs = pairTrials([
      _trial(
        id: 'old',
        role: 'sender',
        device: 'A',
        start: _t(0),
        end: _t(30),
        onScreen: 'P-beef',
      ),
      _trial(
        id: 'new',
        role: 'sender',
        device: 'A',
        start: _t(3600),
        end: _t(3660),
        onScreen: 'P-beef',
      ),
      _trial(
        id: 'R',
        role: 'receiver',
        device: 'B',
        start: _t(3610),
        end: _t(3620),
        observed: 'P-beef',
      ),
    ]);
    expect(pairs.single.sender!['trial_id'], 'new');
  });

  test('without a pair id, the one overlapping sender on another phone', () {
    final pairs = pairTrials([
      _trial(id: 'S', role: 'sender', device: 'A', start: _t(0), end: _t(60)),
      // Same phone as the receiver: never its partner.
      _trial(
        id: 'self',
        role: 'sender',
        device: 'B',
        start: _t(0),
        end: _t(60),
      ),
      // Before the receiver started, beyond the clock slack.
      _trial(
        id: 'early',
        role: 'sender',
        device: 'A',
        start: _t(-100),
        end: _t(-30),
      ),
      _trial(
        id: 'R',
        role: 'receiver',
        device: 'B',
        start: _t(10),
        end: _t(20),
      ),
    ]);
    expect(pairs.single.match, PairMatch.time);
    expect(pairs.single.sender!['trial_id'], 'S');
  });

  test('phone clocks a few seconds apart still overlap', () {
    final pairs = pairTrials([
      _trial(id: 'S', role: 'sender', device: 'A', start: _t(0), end: _t(10)),
      _trial(
        id: 'R',
        role: 'receiver',
        device: 'B',
        start: _t(13),
        end: _t(20),
      ),
    ]);
    expect(pairs.single.match, PairMatch.time);
  });

  test('two overlapping senders are ambiguous, none is unpaired', () {
    final ambiguous = pairTrials([
      _trial(id: 'S1', role: 'sender', device: 'A', start: _t(0), end: _t(60)),
      _trial(id: 'S2', role: 'sender', device: 'C', start: _t(0), end: _t(60)),
      _trial(
        id: 'R',
        role: 'receiver',
        device: 'B',
        start: _t(10),
        end: _t(20),
      ),
    ]).single;
    expect(ambiguous.match, PairMatch.ambiguous);
    expect(ambiguous.candidates, 2);
    expect(ambiguous.sender, isNull);

    final alone = pairTrials([
      _trial(
        id: 'R',
        role: 'receiver',
        device: 'B',
        start: _t(10),
        end: _t(20),
      ),
    ]).single;
    expect(alone.match, PairMatch.unpaired);
  });

  test('a trial in two exports is kept once, the finished copy', () {
    final trials = collectTrials([
      _report([
        _trial(
          id: 'R',
          role: 'receiver',
          device: 'B',
          start: _t(0),
          end: _t(1),
          status: 'running',
        ),
      ]),
      _report([
        _trial(
          id: 'R',
          role: 'receiver',
          device: 'B',
          start: _t(0),
          end: _t(9),
        ),
      ]),
    ]);
    expect(trials, hasLength(1));
    expect(trials.single['status'], 'completed');
  });

  test('integrity is judged against what the sender showed', () {
    final s = _trial(
      id: 'S',
      role: 'sender',
      device: 'A',
      start: _t(0),
      end: _t(60),
      fixture: 'f200',
      profile: 'S',
      onScreen: 'P-0001',
    );
    // The receiver declared f1k/M but decoded the f200 the sender really showed.
    final r = _trial(
      id: 'R',
      role: 'receiver',
      device: 'B',
      start: _t(1),
      end: _t(2),
      observed: 'P-0001',
      sha: Fixtures.f200.sha256Hex,
    );
    expect(integrityVsSender(r, s), 'ok');
    expect(
      declaredDiffers(
        r['config']! as Map<String, Object?>,
        s['config']! as Map<String, Object?>,
      ),
      ['fixture_id', 'profile'],
    );
    final wrong = _trial(
      id: 'R2',
      role: 'receiver',
      device: 'B',
      start: _t(1),
      end: _t(2),
      sha: Fixtures.f1k.sha256Hex,
    );
    expect(integrityVsSender(wrong, s), 'mismatch');
    final nothing = _trial(
      id: 'R3',
      role: 'receiver',
      device: 'B',
      start: _t(1),
      end: _t(2),
    );
    expect(integrityVsSender(nothing, s), 'none');
  });

  test('paired.csv carries the sender settings and the receiver outcome', () {
    final pairs = pairTrials([
      _trial(
        id: 'S',
        role: 'sender',
        device: 'A',
        start: _t(0),
        end: _t(60),
        fixture: 'f200',
        profile: 'S',
        onScreen: 'P-0001',
      ),
      _trial(
        id: 'R',
        role: 'receiver',
        device: 'B',
        start: _t(1),
        end: _t(2),
        observed: 'P-0001',
        status: 'incomplete',
      ),
    ]);
    final rows = parseCsv(pairedCsv(pairs));
    expect(rows.first, pairedHeader);
    String cell(String name) => rows[1][pairedHeader.indexOf(name)];
    expect(cell('pair_id'), 'P-0001');
    expect(cell('match'), 'pairId');
    expect(cell('fixture_id'), 'f200');
    expect(cell('profile'), 'S');
    expect(cell('status'), 'incomplete');
    expect(cell('distance_cm'), '30');
    expect(cell('receiver_device'), 'B');
    expect(cell('sender_device'), 'A');
  });
}
