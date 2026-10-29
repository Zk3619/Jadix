"""Serial matched-content ANGLE/native-DX11/Fast CPU timing comparison on Windows."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
from datetime import datetime


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--jadix', required=True, type=Path)
    parser.add_argument('--fast', required=True, type=Path)
    parser.add_argument('--angle-library', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--rounds', default=3, type=int)
    parser.add_argument('--seconds', default=10, type=int)
    parser.add_argument('--warmup', default=12, type=int)
    parser.add_argument('--scroll-warmup', default=4, type=int)
    parser.add_argument('--policy', choices=('default', 'full-frame', 'both'), default='default',
                        help='full-frame disables Jadix partial damage and Fast scroll layers')
    parser.add_argument('--cache-diagnostics', action='store_true',
                        help='record cache work separately from acceptance timing')
    args = parser.parse_args()
    if args.rounds < 1 or args.seconds < 1 or args.warmup < 1:
        parser.error('rounds, seconds and warmup must be positive')
    if args.scroll_warmup < 0:
        parser.error('scroll-warmup must be nonnegative')
    if args.scroll_warmup % 2:
        parser.error('scroll-warmup must be even for round-trip pixel validation')
    repo = Path(__file__).resolve().parents[2]
    files = [args.jadix.resolve(), args.fast.resolve(), args.angle_library.resolve(), args.angle_library.resolve().with_name('libGLESv2.dll')]
    hashes = {str(p): hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        raise FileExistsError('choose a fresh output directory to preserve evidence')
    started = datetime.now().astimezone().isoformat()
    engines = ['angle', 'native', 'fast']
    rows = []
    identity = None
    policies = ['default', 'full-frame'] if args.policy == 'both' else [args.policy]
    for r in range(args.rounds):
        for engine in engines[r % 3:] + engines[:r % 3]:
            for policy in policies if r % 2 == 0 else policies[::-1]:
                env = os.environ.copy()
                env['JADIX_PARTIAL_DAMAGE'] = '0' if policy == 'full-frame' else '1'
                env['JADIX_SCROLL_STRIP_BLIT'] = '1'
                env['JADIX_RETAINED_UPLOADS'] = '1'
                env['GPUI_SCROLL_LAYERS'] = '0' if policy == 'full-frame' else '1'
                env.pop('GPUI_BENCH_CACHE_DIAGNOSTICS', None)
                env.pop('JADIX_BENCH_CACHE_DIAGNOSTICS', None)
                if args.cache_diagnostics:
                    env['GPUI_BENCH_CACHE_DIAGNOSTICS'] = '1'
                    env['JADIX_BENCH_CACHE_DIAGNOSTICS'] = '1'
                env['WGPU_ANGLE_LIBRARY'] = str(args.angle_library.resolve())
                env['JADIX_GPU_BACKEND'] = 'dx11-angle' if engine == 'angle' else 'dx11'
                capture = output / f'{policy}-{engine}-{r+1}.json'
                command = [sys.executable, str(repo / 'scripts/benchmarks/cpu-bench.py'), '--exe', str(args.fast.resolve() if engine == 'fast' else args.jadix.resolve()), '--calibration', '--client-size', '--width', '1280', '--height', '800', '--sections', 'ordinary,dense,nested', '--seconds', str(args.seconds), '--rounds', '1', '--warmup', str(args.warmup), '--scroll-only', '--screenshots', '--no-gate', '--output', str(capture)]
                if args.cache_diagnostics:
                    command.append('--profile')
                command.extend(['--scroll-warmup', str(args.scroll_warmup)])
                if args.scroll_warmup:
                    command.append('--verify-round-trip')
                with capture.with_suffix('.log').open('w', encoding='utf-8') as log:
                    subprocess.run(command, cwd=repo, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
                data = json.loads(capture.read_text(encoding='utf-8'))
                for row in data['results']:
                    gpu = row['renderer']
                    if engine == 'angle':
                        assert gpu['backend'] == 'Gl' and gpu['implementation'] == 'angle-d3d11', gpu
                    else:
                        assert gpu['backend'] == 'Dx11', gpu
                    if engine == 'native':
                        assert gpu['implementation'] == 'native-wgpu-hal-d3d11', gpu
                    luid = gpu['dxgi']['adapter_luid']
                    current = (luid, row['dpi'], row['client_size'])
                    if identity is None:
                        identity = current
                    assert current == identity and row['client_size'] == [1280, 800], (identity, current)
                    assert row['visual_nonblank'] and row['visual_palette'] and row['valid'], row
                    rows.append(dict(row, engine=engine, comparison_round=r+1, policy=policy))
                    print(f"{policy}/{engine}/{r+1}/{row['section']}: {row['single_core_percent']:.3f}% CPU, {row['cpu_ms_per_observed_frame']:.4f} CPU ms/frame, {row['fps']:.3f} FPS", flush=True)
    summary = []
    for policy in policies:
        for section in ('ordinary', 'dense', 'nested'):
            metrics = {}
            for engine in engines:
                selected = [x for x in rows if x['engine'] == engine and x['section'] == section and x['policy'] == policy]
                metrics[engine] = {key: statistics.median(x[key] for x in selected) for key in ('single_core_percent', 'cpu_ms_per_observed_frame', 'cpu_seconds', 'wall_seconds', 'fps', 'frame_interval_p95_ms')}
            native = metrics['native']['cpu_ms_per_observed_frame']
            fast = metrics['fast']['cpu_ms_per_observed_frame']
            distances = {engine: statistics.median(x['scroll_range_px'] for x in rows
                         if x['engine'] == engine and x['section'] == section and x['policy'] == policy) for engine in engines}
            distance_ratio = max(distances.values()) / min(distances.values())
            assert distance_ratio <= 1.10, (section, distances)
            summary.append(dict(policy=policy, section=section, metrics=metrics, scroll_distance_medians=distances,
                                scroll_distance_ratio=distance_ratio,
                                native_cpu_reduction_vs_fast_percent=100*(1-native/fast), native_lower_cpu_than_fast=native < fast))
    assert all(hashlib.sha256(p.read_bytes()).hexdigest() == hashes[str(p)] for p in files), 'input artifacts changed during capture'
    if len(policies) == 2:
        for engine in engines:
            for section in ('ordinary', 'dense', 'nested'):
                distances = [statistics.median(x['scroll_range_px'] for x in rows
                             if x['engine'] == engine and x['section'] == section
                             and x['policy'] == policy) for policy in policies]
                assert max(distances) / min(distances) <= 1.10, (engine, section, distances)
    result = dict(started_at=started, completed_at=datetime.now().astimezone().isoformat(), rounds=args.rounds, seconds=args.seconds, warmup=args.warmup, scroll_warmup=args.scroll_warmup, warmup_drain_seconds=1 if args.scroll_warmup else 0, policy=args.policy, diagnostic_profiling=args.cache_diagnostics, hashes=hashes, summary=summary, rows=rows, limitations='Synthetic shared content, 60Hz wheel input, one visible process at a time. CPU = all process threads user+kernel time; CPU ms/frame is total CPU divided by observed frames. Jadix counters are successful presentations; Fast counters are paint callbacks. Frame interval p95 is cadence, not GPU or input-to-display latency. Original absolute 10% CPU gate is reported separately; no production Forms/Layout or broad superiority claim.')
    (output / 'comparison.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(summary, indent=2), flush=True)


if __name__ == '__main__':
    main()
