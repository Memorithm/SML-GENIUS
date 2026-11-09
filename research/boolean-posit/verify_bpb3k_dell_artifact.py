"""Read-only, independent audit of the frozen Dell campaign artifact."""
import argparse
import array
import collections
import hashlib
import json
import math
from pathlib import Path, PurePosixPath
import statistics
import struct
import sys
import zipfile

MODES = ('stock', 'scalar', 'simd', 'tiled', 'hybrid', 'fused', 'fused_v2_stock')
CANDIDATE = MODES[-1]
SML = '9eedaa6d454ae8bbbb1522f11375b98be33f0910'
MODEL = '7add77b8d3736d6b2fd21dc96e69be026f19bde843e9988b43cef1315fc5eebe'
UPSTREAM = 'f7591a6a01f9273de03331732bdc0b39721168be'


def close(actual, expected):
    assert math.isfinite(actual) and math.isfinite(expected)
    assert math.isclose(actual, expected, rel_tol=1e-9, abs_tol=1e-9), (actual, expected)


def paired(ratios):
    assert len(ratios) == 14 and all(math.isfinite(r) and r > 0 for r in ratios)
    logs = [math.log(r) for r in ratios]
    mean = math.fsum(logs) / 14
    variance = math.fsum((x - mean) ** 2 for x in logs) / 13
    radius = 2.160368656 * math.sqrt(variance / 14)
    return dict(geomean_ratio=math.exp(mean), lower95=math.exp(mean - radius), upper95=math.exp(mean + radius))


def parse_dump(data):
    assert data[:8] == b'BPB3FID2'
    nv, np = struct.unpack_from('<II', data, 8)
    assert 0 < nv <= 200000 and 0 < np <= 4096
    prompt = struct.unpack_from(f'<{np}i', data, 16)
    offset = 16 + 4 * np
    steps, = struct.unpack_from('<I', data, offset)
    offset += 4
    assert steps == 8 and len(data) == offset + steps * (nv * 4 + 4)
    rows = []
    for _ in range(steps):
        values = array.array('f')
        values.frombytes(data[offset:offset + nv * 4])
        if sys.byteorder != 'little':
            values.byteswap()
        offset += nv * 4
        token, = struct.unpack_from('<i', data, offset)
        offset += 4
        assert all(math.isfinite(x) for x in values)
        assert token == max(range(nv), key=values.__getitem__)
        rows.append((values, token))
    return (nv, prompt), rows


def check_fidelity(z):
    names = z.namelist()
    candidates = [n for n in names if 'fidelity' in PurePosixPath(n).parts and n.endswith('-fused_v2_stock.bin')]
    assert len(candidates) == 24
    expected = {f't{t}-c{c}-{p}-r{r}' for t in (2, 8) for c in (128, 512)
                for p in ('arithmetic', 'french', 'context') for r in (1, 2)}
    assert {PurePosixPath(n).name.removesuffix('-fused_v2_stock.bin') for n in candidates} == expected
    checked, identical, max_abs, tokens = 0, 0, 0.0, 0
    for name in candidates:
        prefix = name.removesuffix('fused_v2_stock.bin')
        source, candidate = z.read(prefix + 'stock.bin'), z.read(name)
        left_header, left = parse_dump(source)
        right_header, right = parse_dump(candidate)
        assert left_header == right_header
        identical += source == candidate
        for (a, ta), (b, tb) in zip(left, right):
            assert ta == tb
            tokens += 1
            for ref, actual in zip(a, b):
                delta = abs(actual - ref)
                max_abs = max(max_abs, delta)
                assert delta <= 2e-4 + 2e-5 * abs(ref), (name, delta, ref)
                checked += 1
        assert z.read(prefix + 'fused.bin') == z.read(prefix + 'fused_v2.bin')
        if '-r2-' in prefix:
            first = prefix.replace('-r2-', '-r1-')
            for mode in ('stock', 'fused', 'fused_v2', 'fused_v2_stock', 'stock_vector', 'stock_ref', 'fused_ref'):
                assert z.read(first + mode + '.bin') == z.read(prefix + mode + '.bin'), (prefix, mode)
    return dict(cases=24, logits=checked, greedy_tokens=tokens, max_abs=max_abs, byte_identical_pairs=identical)


def audit(path, expected_digest, controller):
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    assert digest == expected_digest
    with zipfile.ZipFile(path) as z:
        def read(leaf):
            found = [n for n in z.namelist() if PurePosixPath(n).name == leaf]
            assert len(found) == 1, (leaf, found)
            return z.read(found[0])

        identities = dict(line.split('=', 1) for line in read('identities.txt').decode().splitlines())
        for key, value in dict(controller=controller, sml=SML, ggml=UPSTREAM, model=MODEL,
                               candidate=CANDIDATE, GGML_CPU_TILED_MM='1', GGML_CPU_TILED_MM_FORCE='0').items():
            assert identities[key] == value, key
        hashes = read('binary-model-sha256.txt').decode()
        assert f'{MODEL}  original.gguf' in hashes
        fidelity = check_fidelity(z)
        summary_text = read('fidelity-summary.txt').decode()
        assert 'PASS_BPB3K_MULTIPROMPT_ALL_LOGITS_AND_TOKENS' in summary_text
        assert 'candidate=fused_v2_stock' in summary_text
        admission = json.loads(read('admission.json'))
        assert len(admission['cpus']) == len(set(admission['cpus'])) == 8
        assert admission['ram_available_kib'] >= 12 * 1024**2
        assert admission['disk_available_bytes'] >= 8 * 1024**3
        assert admission['virtual_memory_limit_kib'] == 8388608
        assert admission['disk_budget_bytes'] == 5 * 1024**3
        assert admission['loadavg'][0] < 4
        assert max(admission['idle_probe_busy_fraction'].values()) < 0.10
        topology = {}
        for line in read('topology.csv').decode().splitlines():
            if line and not line.startswith('#'):
                cpu, core, socket, node = map(int, line.split(','))
                topology[cpu] = (socket, node, core)
        selected = [topology[c] for c in admission['cpus']]
        assert len({(s, n) for s, n, _ in selected}) == 1
        assert len({(s, c) for s, _, c in selected}) == 8
        assert selected[0][:2] == (admission['socket'], admission['node'])
        if 'selection_policy' in admission:
            assert admission['selection_policy'] == 'initial_observed_idle_physical_cores_with_idle_siblings'
            probe = json.loads(read('admission-probe.json'))
            assert probe['elapsed_s'] > 0
            busy = {int(c): v for c, v in probe['all_cpu_busy_fraction'].items()}
            allowed = set(probe['allowed_cpus'])
            physical = collections.defaultdict(list)
            for cpu, identity in topology.items():
                physical[identity].append(cpu)
            nodes = collections.defaultdict(list)
            for identity, siblings in physical.items():
                available = sorted(allowed.intersection(siblings))
                values = [busy.get(c) for c in siblings]
                if available and all(type(v) in (int, float) and math.isfinite(v) and 0 <= v < .10 for v in values):
                    nodes[identity[:2]].append((max(values), identity[2], available[0], sorted(siblings)))
            options = []
            for key, cores in nodes.items():
                if len(cores) >= 8:
                    eight = sorted(cores)[:8]
                    score = (max(c[0] for c in eight), sum(c[0] for c in eight), key)
                    options.append((score, key, eight))
            assert options
            _, key, chosen = min(options)
            assert key == (admission['socket'], admission['node'])
            assert sorted(c[2] for c in chosen) == admission['cpus']
            siblings = sorted(cpu for c in chosen for cpu in c[3])
            assert admission['physical_sibling_probe_busy_fraction'] == {str(c): busy[c] for c in siblings}
            assert admission['idle_probe_busy_fraction'] == {str(c): busy[c] for c in admission['cpus']}

        records = [json.loads(line) for line in read('measurements.jsonl').splitlines()]
        assert len(records) == 203
        expected_order = [('qualification', 0, 2, i, m) for i, m in enumerate(MODES)]
        for round_id in range(14):
            rotated = list(MODES if round_id < 7 else reversed(MODES))
            rotation = round_id % 7
            rotated = rotated[rotation:] + rotated[:rotation]
            for threads in ((2, 8) if round_id % 2 == 0 else (8, 2)):
                expected_order.extend(('campaign', round_id, threads, i, m) for i, m in enumerate(rotated))
        assert [(r['phase'], r['round'], r['threads'], r['position'], r['mode']) for r in records] == expected_order
        metrics = {}
        contaminated = False
        for r in records:
            phase, rd, threads, pos, mode = r['phase'], r['round'], r['threads'], r['position'], r['mode']
            reps = 3 if phase == 'qualification' else 5
            assert r['exit_code'] == 0 and r['repetitions'] == reps
            assert r['cpus'] == admission['cpus'][:threads] and r['numa_node'] == admission['node']
            assert r['observed_task_disk_bytes'] <= 5 * 1024**3
            command = r['command']
            assert command[command.index('-c') + 1] == ','.join(map(str, r['cpus']))
            for flag, value in (('-t', threads), ('-r', reps), ('-p', 64), ('-n', 32), ('-ngl', 0)):
                assert command[command.index(flag) + 1] == str(value)
            stem = f'{phase}-t{threads}-r{rd:02d}-p{pos}-{mode}'
            raw = read(stem + '.json')
            assert hashlib.sha256(raw).hexdigest() == r['raw_sha256']
            rows = json.loads(raw)
            assert len(rows) == 2 and {(x['n_prompt'], x['n_gen']) for x in rows} == {(64, 0), (0, 32)}
            values = {}
            for row in rows:
                assert row['n_threads'] == threads and row['n_gpu_layers'] == 0
                assert len(row['samples_ns']) == len(row['samples_ts']) == reps
                assert all(type(ns) is int and ns > 0 for ns in row['samples_ns'])
                count = row['n_prompt'] + row['n_gen']
                speeds = [count * 1e9 / ns for ns in row['samples_ns']]
                for speed, printed in zip(speeds, row['samples_ts']):
                    assert math.isfinite(printed) and abs(speed - printed) <= 1e-4 * max(1, speed)
                label = 'pp64' if row['n_prompt'] else 'tg32'
                values[label] = math.fsum(speeds) / reps
                values[label + '_raw_samples'] = speeds
                if label == 'tg32':
                    values['ms_token'] = math.fsum(row['samples_ns']) / reps / count / 1e6
                close(values[label], r['metrics'][label]['tok_s'])
            usage = json.loads(read(stem + '.usage.json'))
            assert usage == r['usage'] and usage['max_rss_kib'] > 0
            own = usage['user_seconds'] + usage['system_seconds']
            assert r['elapsed_wall_s'] > 0
            unaccounted = max(0, r['selected_cpu_busy_s'] - own) / r['elapsed_wall_s']
            close(unaccounted, r['unattributed_cpu_per_wall_s'])
            assert r['contaminated'] == (unaccounted > 0.10)
            contaminated |= r['contaminated']
            stderr = read(stem + '.stderr.txt').decode()
            if mode == CANDIDATE:
                lines = [l for l in stderr.splitlines() if l.startswith('BPB3K_DIRECT_DOT_EXECUTED kernel=FusedV2Stock rows=')]
                assert all(any(f'stock_tiled={v}' in l and 'avx2=true' in l and 'fma=true' in l for l in lines) for v in ('true', 'false'))
            if phase == 'campaign':
                key = (threads, rd, mode)
                assert key not in metrics
                metrics[key] = dict(**values, rss=usage['max_rss_kib'])

        summary = json.loads(read('summary.json'))
        assert summary['candidate_kernel'] == CANDIDATE
        assert summary['complete_processes'] == 203 and summary['rounds'] == 14
        assert summary['fidelity'] == 'passed_frozen_bounded_corpus'
        cohorts, comparisons = {}, {}
        for threads in (2, 8):
            cohorts[str(threads)] = {mode: dict(
                tg32=statistics.mean(metrics[(threads, i, mode)]['tg32'] for i in range(14)),
                tg32_process_sd=statistics.stdev(metrics[(threads, i, mode)]['tg32'] for i in range(14)),
                pp64=statistics.mean(metrics[(threads, i, mode)]['pp64'] for i in range(14)),
                ms_token=statistics.mean(metrics[(threads, i, mode)]['ms_token'] for i in range(14)),
                peak_rss_kib=max(metrics[(threads, i, mode)]['rss'] for i in range(14))) for mode in MODES}
            for mode in MODES:
                for label in ('tg32', 'pp64'):
                    raw_speeds = [speed for i in range(14) for speed in metrics[(threads, i, mode)][label + '_raw_samples']]
                    assert len(raw_speeds) == 70
                    cohorts[str(threads)][mode][label + '_raw_cv'] = statistics.stdev(raw_speeds) / statistics.mean(raw_speeds)
            for mode in MODES:
                declared = summary['resources'][str(threads)][mode]
                calculated = cohorts[str(threads)][mode]
                for reported, field in (('mean_tg32', 'tg32'), ('mean_pp64', 'pp64'),
                                        ('mean_ms_token', 'ms_token'), ('max_rss_kib', 'peak_rss_kib')):
                    close(declared[reported], calculated[field])
            comparisons[str(threads)] = {}
            for baseline in ('stock', 'fused'):
                ratios = [metrics[(threads, i, CANDIDATE)]['tg32'] / metrics[(threads, i, baseline)]['tg32'] for i in range(14)]
                comparison = paired(ratios)
                for field, expected in comparison.items():
                    close(summary['comparisons'][str(threads)][baseline][field], expected)
                comparisons[str(threads)][baseline] = comparison
        resource_regression = any(cohorts[str(t)][CANDIDATE]['peak_rss_kib'] >
                                  max(1.10 * cohorts[str(t)]['fused']['peak_rss_kib'],
                                      cohorts[str(t)]['fused']['peak_rss_kib'] + 32768) for t in (2, 8))
        better = lambda base: all(comparisons[str(t)][base]['lower95'] > 1 for t in (2, 8))
        slower = any(comparisons[str(t)]['fused']['upper95'] < 1 for t in (2, 8))
        category = 'E' if contaminated else 'D' if resource_regression or slower else 'A' if better('stock') and better('fused') else 'B' if better('fused') else 'C'
        assert summary['category'] == category and summary['resource_regression'] == resource_regression
        return dict(artifact_sha256=digest, identities=identities, fidelity=fidelity,
                    processes=len(records), raw_durations=203 * 2 * 5 - 7 * 2 * 2,
                    category=category, contaminated=contaminated, resource_regression=resource_regression,
                    cohorts=cohorts, comparisons=comparisons,
                    limits='single Dell session; cooperative admission; no host-population or universal-equivalence claim')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('archive', type=Path)
    parser.add_argument('--sha256', required=True)
    parser.add_argument('--controller', required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    result = audit(args.archive, args.sha256, args.controller)
    args.out.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(result, indent=2))
