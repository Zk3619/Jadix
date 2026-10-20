"""Validate rotating fixed-content captures; preserve the production CPU gates."""
import argparse
import hashlib
import json
from pathlib import Path
import statistics

ENGINES = ("jadix", "gpui-kit", "gpui-fast")
SCENES = ("ordinary", "dense", "nested")


def summarize(directory, rounds):
    groups = {}
    identities = {}
    sizes = set()
    renderers = {}
    for engine in ENGINES:
        for index in range(1, rounds + 1):
            capture = json.loads((directory / f"{engine}-{index}.json").read_text(encoding="utf-8"))
            assert capture["calibration"] and capture["physical_client_size"]
            assert not capture["diagnostic_profiling"]
            identities.setdefault(engine, set()).add(capture["executable_sha256"])
            keys = [(r["section"], r["phase"]) for r in capture["results"]]
            assert set(keys) == {(s, p) for s in SCENES for p in ("scroll", "settled")}
            assert len(keys) == 6, "duplicate/missing phase"
            for row in capture["results"]:
                assert row["valid"] and row["foreground_visible"], f"invalid {engine}/{index}"
                assert row.get("visual_nonblank") is True, "missing/blank visual evidence"
                assert row.get("visual_palette") is True, "missing/incorrect fixture colors"
                assert row["wall_seconds"] >= 10, "acceptance requires >=10 s"
                sizes.add((*row["client_size"], row["dpi"]))
                renderer = row.get("renderer")
                renderers.setdefault(engine, set()).add(json.dumps(renderer, sort_keys=True))
                groups.setdefault((engine, row["section"], row["phase"]), []).append(row)
    assert len(sizes) == 1, "client size/DPI differs"
    assert all(len(v) == 1 for v in identities.values()), "executable changed within a round set"
    assert all(len(v) == 1 for v in renderers.values()), "renderer changed within a round set"
    renderers = {engine: json.loads(next(iter(values))) for engine, values in renderers.items()}
    renderer_status = "verified-d3d11"
    gpu_ids = set()
    for engine, renderer in renderers.items():
        if not renderer:
            renderer_status = "incomparable-renderer"
            continue
        native = renderer.get("backend") == "Dx11" and renderer.get("implementation") == "native"
        translated = renderer.get("backend") == "Gl" and renderer.get("implementation") == "angle-d3d11"
        dxgi = renderer.get("dxgi") or {}
        if not (native or translated) or renderer.get("device_type") in {"Cpu", "Other"}:
            renderer_status = "incomparable-renderer"
        if not all(key in dxgi for key in ("adapter_luid", "vendor_id", "device_id")):
            renderer_status = "incomparable-renderer"
        else:
            gpu_ids.add((tuple(dxgi["adapter_luid"]), dxgi["vendor_id"], dxgi["device_id"]))
    if len(gpu_ids) != 1:
        renderer_status = "incomparable-renderer"
    rows = []
    for scene in SCENES:
        offsets = [statistics.median(r["scroll_range_px"] for r in groups[e, scene, "scroll"]) for e in ENGINES]
        travel_ratio = max(offsets) / min(offsets)
        for engine in ENGINES:
            samples = groups[engine, scene, "scroll"]
            settled = groups[engine, scene, "settled"]
            rows.append(dict(engine=engine, scene=scene,
                cpu_percent=[r["single_core_percent"] for r in samples],
                cpu_median=statistics.median(r["single_core_percent"] for r in samples),
                cpu_ms_per_observed_frame_median=statistics.median(r["cpu_ms_per_observed_frame"] for r in samples),
                fps=[r["fps"] for r in samples],
                counter_kind=samples[0]["frame_counter_kind"],
                scroll_range_px=[r["scroll_range_px"] for r in samples],
                travel_ratio=travel_ratio, equal_travel=travel_ratio <= 1.10,
                settled_cpu_percent=[r["single_core_percent"] for r in settled],
                settled_observed_frames=[r.get("presented_frames", r.get("painted_frames", r["redraws"])) for r in settled],
                numeric_cpu_target_passed=all(r["passed"] for r in samples)))
    status = renderer_status if renderer_status != "verified-d3d11" else (
        "calibration-only" if all(r["equal_travel"] for r in rows) else "incomparable-scroll-travel")
    comparison_class = "d3d11-translated-vs-native" if any(
        r and r.get("implementation") == "angle-d3d11" for r in renderers.values()) else "d3d11-native"
    return dict(status=status, renderer_status=renderer_status, renderers=renderers, comparison_class=comparison_class,
        limitations=["GPUI counts paint callbacks, Jadix counts presentations; no cross-framework presentation or GPU winner is certified.",
            "ANGLE translation cost is included in Jadix results; this is not a native wgpu-hal D3D11 result.",
            "10-second settled phases are diagnostics; idle acceptance requires separate >=60-second windows.",
            "Synthetic dense/nested fixtures do not calibrate the production Forms/Layout thresholds."],
        workload_sha256=hashlib.sha256(Path(__file__).with_name("workload.rs").read_bytes()).hexdigest(),
        executable_sha256={k: next(iter(v)) for k, v in identities.items()}, client_size_and_dpi=list(next(iter(sizes))),
        rounds=rounds, results=rows)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--rounds", type=int, default=3)
    args = parser.parse_args()
    assert args.rounds >= 3, "at least three rotating rounds are required"
    result = summarize(args.directory, args.rounds)
    (args.directory / "summary.json").write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps(result, indent=2))
    if result["status"] != "calibration-only":
        raise SystemExit(1)
