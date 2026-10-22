#!/usr/bin/env python3
"""Exact, fail-closed comparison of --shift-overlap TSV snapshots (stdlib only)."""
import argparse
import collections
import hashlib
import json
from pathlib import Path
import sys
import tempfile
import unittest

FIELDS = 'id profile split group bucket keys expected previous scope partial final mark enter space'.split()
SURFACES = ('partial', 'final', 'mark', 'enter', 'space')
CONTRACT = FIELDS[:9]


def unescape(value):
    out = []
    i = 0
    escapes = {'\\': '\\', 't': '\t', 'n': '\n', 'r': '\r'}
    while i < len(value):
        if value[i] == '\\':
            i += 1
            if i == len(value) or value[i] not in escapes:
                raise ValueError('invalid TSV escape')
            out.append(escapes[value[i]])
        else:
            out.append(value[i])
        i += 1
    return ''.join(out)


def load(path):
    raw = Path(path).read_bytes()
    lines = raw.decode('utf-8').split('\n')
    if lines and lines[-1] == '':
        lines.pop()
    if not lines or lines[0].split('\t') != FIELDS:
        raise ValueError(f'{path}: invalid header')
    rows = {}
    for number, line in enumerate(lines[1:], 2):
        values = line.split('\t')
        if len(values) != len(FIELDS):
            raise ValueError(f'{path}:{number}: invalid column count')
        row = dict(zip(FIELDS, map(unescape, values)))
        if not row['id'] or row['id'] in rows:
            raise ValueError(f'{path}:{number}: missing/duplicate ID {row["id"]!r}')
        if row['split'] not in ('dev', 'holdout') or row['scope'] not in ('decision', 'decision+session'):
            raise ValueError(f'{path}:{number}: invalid split/scope')
        if row['scope'] == 'decision' and any(row[s] for s in SURFACES[2:]):
            raise ValueError(f'{path}:{number}: contextual row claims session output')
        if (row['previous'] == '-') != (row['scope'] == 'decision+session'):
            raise ValueError(f'{path}:{number}: inconsistent session context')
        rows[row['id']] = row
    if not rows:
        raise ValueError(f'{path}: empty dataset')
    return rows, hashlib.sha256(raw).hexdigest()


def surfaces(row):
    return SURFACES if row['scope'] == 'decision+session' else SURFACES[:2]


def correct(row, surface):
    expected = row['expected'] + (' ' if surface == 'space' else '')
    return row[surface] == expected


def compare(base_path, candidate_path):
    base, base_hash = load(base_path)
    candidate, candidate_hash = load(candidate_path)
    if base.keys() != candidate.keys():
        raise ValueError(f'ID mismatch: missing={sorted(base.keys() - candidate.keys())!r}; '
                         f'extra={sorted(candidate.keys() - base.keys())!r}')
    buckets = {}
    changes = []
    transitions = []
    totals = collections.Counter()
    for row_id in sorted(base):
        old, new = base[row_id], candidate[row_id]
        for field in CONTRACT:
            if old[field] != new[field]:
                raise ValueError(f'{row_id!r}: contract differs at {field}')
        bucket_key = '/'.join((old['profile'], old['split'], old['bucket']))
        bucket = buckets.setdefault(bucket_key, {'rows': 0, 'surfaces': {}})
        bucket['rows'] += 1
        for surface in surfaces(old):
            counts = bucket['surfaces'].setdefault(surface, dict.fromkeys(
                ('evaluated', 'baseline_wrong', 'candidate_wrong', 'improved', 'newly_wrong', 'changed'), 0))
            before, after = correct(old, surface), correct(new, surface)
            counts['evaluated'] += 1
            counts['baseline_wrong'] += int(not before)
            counts['candidate_wrong'] += int(not after)
            improved = not before and after
            regression = before and not after
            counts['improved'] += int(improved)
            counts['newly_wrong'] += int(regression)
            totals['improved'] += int(improved)
            totals['newly_wrong'] += int(regression)
            if old[surface] != new[surface]:
                counts['changed'] += 1
                changes.append({'id': row_id, 'bucket': bucket_key, 'surface': surface,
                                'keys': old['keys'], 'expected': old['expected'] + (' ' if surface == 'space' else ''),
                                'baseline': old[surface], 'candidate': new[surface],
                                'outcome': 'newly_wrong' if regression else 'improved' if improved else 'wrong_to_wrong'})
        pairs = [('partial', 'final')]
        if old['scope'] == 'decision+session':
            pairs += [('mark', 'enter'), ('mark', 'space')]
        for preview, commit in pairs:
            before = correct(old, preview) and not correct(old, commit)
            after = correct(new, preview) and not correct(new, commit)
            label = f'{preview}_good_to_{commit}_bad'
            counts = bucket.setdefault('preview_commit', {}).setdefault(label, {'baseline': 0, 'candidate': 0})
            counts['baseline'] += int(before)
            counts['candidate'] += int(after)
            if before or after:
                transitions.append({'id': row_id, 'transition': label, 'baseline': before, 'candidate': after})
    return {'schema': 1, 'baseline_sha256': base_hash, 'candidate_sha256': candidate_hash,
            'rows': len(base), 'groups': len({r['group'] for r in base.values()}),
            'split_rows': dict(collections.Counter(r['split'] for r in base.values())),
            'surface_observations': sum(len(surfaces(r)) for r in base.values()),
            'improved': totals['improved'], 'newly_wrong': totals['newly_wrong'],
            'buckets': buckets, 'changes': changes, 'good_preview_bad_commit': transitions}


class ComparatorTests(unittest.TestCase):
    def fixture(self, path, rows):
        def escape(s):
            return s.replace('\\', '\\\\').replace('\t', '\\t').replace('\n', '\\n').replace('\r', '\\r')
        path.write_text('\t'.join(FIELDS) + '\n' + ''.join(
            '\t'.join(escape(row[f]) for f in FIELDS) + '\n' for row in rows), encoding='utf-8')

    def row(self):
        return dict(zip(FIELDS, ['one', 'enko', 'dev', 'word', 'en.raw', 'word', 'word', '-',
                                'decision+session', 'word', 'word', 'word', 'word', 'word ']))

    def test_regression_and_improvement_on_every_surface(self):
        with tempfile.TemporaryDirectory() as directory:
            a, b = Path(directory) / 'a.tsv', Path(directory) / 'b.tsv'
            for surface in SURFACES:
                old = self.row()
                new = dict(old, **{surface: 'wrong\\\t\n\r'})
                self.fixture(a, [old]); self.fixture(b, [new])
                report = compare(a, b)
                self.assertEqual(report['newly_wrong'], 1)
                self.assertEqual(report['changes'][0]['candidate'], new[surface])
                self.assertEqual(compare(b, a)['improved'], 1)
                self.assertEqual(compare(b, b)['newly_wrong'], 0)

    def test_reject_missing_duplicate_empty_and_contract(self):
        with tempfile.TemporaryDirectory() as directory:
            a, b = Path(directory) / 'a.tsv', Path(directory) / 'b.tsv'
            row = self.row()
            self.fixture(a, [row])
            for rows in ([], [row, row], [dict(row, id='other')], [dict(row, expected='different')]):
                self.fixture(b, rows)
                with self.assertRaises(ValueError):
                    compare(a, b)

    def test_escape_validation(self):
        for invalid in ('\\', '\\x'):
            with self.assertRaises(ValueError):
                unescape(invalid)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('baseline', nargs='?')
    parser.add_argument('candidate', nargs='?')
    parser.add_argument('--json', type=Path)
    parser.add_argument('--selftest', action='store_true')
    args = parser.parse_args()
    if args.selftest:
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(ComparatorTests)
        return 0 if unittest.TextTestRunner().run(suite).wasSuccessful() else 1
    if not args.baseline or not args.candidate:
        parser.error('BASE.tsv and CANDIDATE.tsv are required')
    try:
        report = compare(args.baseline, args.candidate)
    except (ValueError, OSError, UnicodeError) as error:
        print(f'comparison rejected: {error}', file=sys.stderr)
        return 2
    if args.json:
        args.json.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({k: v for k, v in report.items() if k not in ('buckets', 'changes', 'good_preview_bad_commit')}, ensure_ascii=False))
    for name, counts in sorted(report['buckets'].items()):
        print('BUCKET ' + json.dumps({'bucket': name, **counts}, ensure_ascii=False))
    for change in report['changes']:
        print('CHANGE ' + json.dumps(change, ensure_ascii=False))
    for transition in report['good_preview_bad_commit']:
        print('PREVIEW_COMMIT ' + json.dumps(transition, ensure_ascii=False))
    return 1 if report['newly_wrong'] else 0


if __name__ == '__main__':
    sys.exit(main())
