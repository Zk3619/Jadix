import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import random
import statistics
import subprocess
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def interval(values, center=statistics.median):
    generator = random.Random(20261029)
    estimates = sorted(center(generator.choices(values, k=len(values))) for _ in range(5000))
    return [estimates[124], estimates[4874]]


def p95(values):
    return sorted(values)[max(0, (len(values) * 95 + 99) // 100 - 1)]


def measurements(result):
    values = {"first_present_ms": result["first_present_ms"]}
    for index, phase in enumerate(result["phases"]):
        if index == 0:
            continue
        name = phase["name"]
        values[name + "_update_render_p95_ms"] = p95(phase["milliseconds"])
        values[name + "_present_latency_p95_ms"] = p95(result["present_latency_ms"][index])
        values[name + "_process_cpu_ms_per_frame"] = result["phase_cpu_ms"][index] / result["frames_per_phase"]
    values["private_mib"] = statistics.median(result["private_mib"][1:])
    return values


def run_case(executable, output, frames, production):
    output.mkdir()
    environment = dict(os.environ, JADIX_HR6_COST_EVIDENCE=str(output), JADIX_HR6_COST_FRAMES=str(frames), NO_COLOR="1")
    command = [str(executable)] if production else [str(executable), "--ignored", "--exact", "app::native_cost_tests::native_release_cost", "--test-threads=1", "--nocapture"]
    started = time.perf_counter()
    with (output / "native.log").open("w", encoding="utf-8") as log:
        process = subprocess.Popen(command, env=environment, stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline=time.monotonic()+300
            while True:
                try:
                    exit_code=process.wait(timeout=min(2,max(0.01,deadline-time.monotonic())))
                    break
                except subprocess.TimeoutExpired:
                    text=(output/"native.log").read_text(encoding="utf-8",errors="replace")
                    if "panicked at" in text:
                        subprocess.run(["taskkill","/PID",str(process.pid),"/T","/F"],stdout=log,stderr=subprocess.STDOUT)
                        process.wait();exit_code=101
                        break
                    if time.monotonic()>=deadline:
                        raise subprocess.TimeoutExpired(command,300)
        except subprocess.TimeoutExpired:
            subprocess.run(["taskkill", "/PID", str(process.pid), "/T", "/F"], stdout=log, stderr=subprocess.STDOUT)
            process.wait()
            raise
    record = {"command":command,"pid":process.pid,"exit_code":exit_code,"elapsed_seconds":time.perf_counter()-started,
              "executable_sha256":digest(executable),"log_sha256":digest(output / "native.log")}
    (output / "execution.json").write_text(json.dumps(record, indent=2), encoding="utf-8")
    marker = "HR6_NATIVE_COST_COMPLETE" if production else "1 passed; 0 failed"
    if exit_code or marker not in (output / "native.log").read_text(encoding="utf-8"):
        raise RuntimeError(f"native cost scenario failed: {output}")
    result = json.loads((output / "native-cost.json").read_text(encoding="utf-8"))
    assert result["schema"] == 2 and not result["debug_assertions"]
    if production:
        assert not result["cfg_test"] and result["panic_abort"]
        assert result["verified_text_input"]
        assert result["verified_text_input_frames"] == frames
    assert all(len(phase["milliseconds"]) == frames for phase in result["phases"])
    assert result["adapter"]["backend"] == "Dx12"
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--candidate", type=Path, required=True)
    parser.add_argument("--baseline-app", type=Path, required=True)
    parser.add_argument("--candidate-app", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--calibration", type=int, default=8)
    parser.add_argument("--pairs", type=int, default=16)
    parser.add_argument("--frames", type=int, default=240)
    parser.add_argument("--production", action="store_true")
    arguments = parser.parse_args()
    directory = arguments.evidence.resolve()
    directory.mkdir(exist_ok=False)
    baseline = arguments.baseline.resolve(strict=True)
    candidate = arguments.candidate.resolve(strict=True)
    records = []
    adapter = None
    for label, executable in [("warm-baseline",baseline),("warm-candidate",candidate)]:
        run_case(executable,directory / label,30,arguments.production)
    calibration = []
    for index in range(arguments.calibration):
        label = f"calibration-{index:02}"
        result = run_case(baseline,directory / label,arguments.frames,arguments.production)
        if adapter is None:
            adapter = result["adapter"]
        assert result["adapter"] == adapter
        calibration.append(measurements(result))
        print(label, flush=True)
    for index in range(arguments.pairs):
        pair = {}
        order = [("baseline",baseline),("candidate",candidate)]
        if index % 2:
            order.reverse()
        for label, executable in order:
            result = run_case(executable,directory / f"pair-{index:02}-{label}",arguments.frames,arguments.production)
            assert result["adapter"] == adapter
            pair[label] = measurements(result)
        records.append(pair)
        (directory / "pairs.json").write_text(json.dumps(records,indent=2),encoding="utf-8")
        print(f"pair-{index:02}", flush=True)
    metrics = {}
    for name in calibration[0]:
        before = [record[name] for record in calibration]
        reference = statistics.median(before)
        noise_interval = [value / reference - 1 for value in interval(before)]
        ratios = [pair["candidate"][name] / pair["baseline"][name] for pair in records]
        bounds = interval(ratios)
        observed_ratio = statistics.median(ratios)
        statistic = "median of paired ratios"
        if name == "first_present_ms":
            generator = random.Random(20261029)
            reference = p95(before)
            estimates = sorted(p95(generator.choices(before,k=len(before))) / reference - 1 for _ in range(5000))
            noise_interval = [estimates[124],estimates[4874]]
            sampled = []
            for _ in range(5000):
                chosen = generator.choices(records,k=len(records))
                sampled.append(p95([pair["candidate"][name] for pair in chosen]) / p95([pair["baseline"][name] for pair in chosen]))
            sampled.sort()
            bounds = [sampled[124],sampled[4874]]
            observed_ratio = p95([pair["candidate"][name] for pair in records]) / p95([pair["baseline"][name] for pair in records])
            statistic = "p95 across native runs"
        elif name.endswith("_process_cpu_ms_per_frame"):
            reference = statistics.fmean(before)
            noise_interval = [value / reference - 1 for value in interval(before, statistics.fmean)]
            generator = random.Random(20261029)
            sampled = []
            for _ in range(5000):
                chosen = generator.choices(records,k=len(records))
                sampled.append(sum(pair["candidate"][name] for pair in chosen) / sum(pair["baseline"][name] for pair in chosen))
            sampled.sort()
            bounds = [sampled[124],sampled[4874]]
            observed_ratio = sum(pair["candidate"][name] for pair in records) / sum(pair["baseline"][name] for pair in records)
            statistic = "total process CPU / total presented frames; paired ratio of sums"
        metrics[name] = {"baseline_calibration_reference":reference,"calibration_statistic":"p95" if name == "first_present_ms" else ("mean" if name.endswith("_process_cpu_ms_per_frame") else "median"),"comparison_statistic":statistic,"baseline_min":min(before),"baseline_max":max(before),
                         "baseline_median_ci95_relative":noise_interval,"paired_ratios":ratios,
                         "observed_regression_percent":(observed_ratio-1)*100,
                         "ci95_regression_percent":[(value-1)*100 for value in bounds],
                         "passed":bounds[1] <= 1.05 and max(abs(value) for value in noise_interval) <= 0.05}
    before_app = arguments.baseline_app.resolve(strict=True)
    after_app = arguments.candidate_app.resolve(strict=True)
    size = {"baseline_bytes":before_app.stat().st_size,"candidate_bytes":after_app.stat().st_size,
            "baseline_sha256":digest(before_app),"candidate_sha256":digest(after_app)}
    size["regression_percent"] = (size["candidate_bytes"] / size["baseline_bytes"] - 1) * 100
    size["passed"] = size["regression_percent"] <= 5
    instrumented_size = {"baseline_bytes":baseline.stat().st_size,"candidate_bytes":candidate.stat().st_size}
    instrumented_size["regression_percent"] = (instrumented_size["candidate_bytes"] / instrumented_size["baseline_bytes"] - 1) * 100
    instrumented_size["passed"] = instrumented_size["regression_percent"] <= 5
    forbidden = [b"JADIX_HOT_RELOAD_ADDR",b"JADIX_DEVTOOLS_ROOT",b"tokens-v1:",b"compiled template or opaque Rust slot changed",b"HMR commit"]
    isolation = {str(path):[marker.decode() for marker in forbidden if marker in path.read_bytes()] for path in [baseline,candidate,before_app,after_app]}
    passed = all(value["passed"] for value in metrics.values()) and size["passed"] and instrumented_size["passed"] and not any(isolation.values())
    summary = {"schema":2,"passed":passed,"production":arguments.production,"fixed_budget_percent":5,"created":datetime.datetime.now().isoformat(),
               "device":adapter,"frames_per_phase":arguments.frames,"calibration_runs":arguments.calibration,
               "paired_runs":arguments.pairs,"statistics":"fixed-seed 5000-sample bootstrap; frame p95 in each native run, CI of repeated paired ratios; CPU total per presented frame with paired ratio-of-sums CI; first display p95 across paired runs; alternating order; warmups excluded",
               "metrics":metrics,"standalone_application_size":size,"instrumented_production_size":instrumented_size,"release_forbidden_markers":isolation}
    (directory / "summary.json").write_text(json.dumps(summary,indent=2),encoding="utf-8")
    print(json.dumps({"passed":passed,"failing_metrics":[name for name,value in metrics.items() if not value["passed"]]},ensure_ascii=False),flush=True)
    raise SystemExit(0 if passed else 1)


if __name__ == "__main__":
    main()
