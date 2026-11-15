"""Alternate built renderers on the same backend; keep every image and timing report.

Requires Pillow for offline pixel comparison. Build both binaries first (see README).
Times cover scene preparation + encoding + submission + serialized GPU completion.
"""
import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import sys

from PIL import Image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--frames", type=int, default=120)
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--backends", nargs="+", choices=["dx12", "vulkan", "gles"], default=["dx12", "vulkan", "gles"])
    parser.add_argument("--output", default="crates/jadix_2d/evidence/vello-comparison")
    parser.add_argument("--premultiplied", action="store_true", help="Use Jadix's direct premultiplied output for hosts that accept it")
    parser.add_argument("--quality", choices=["balanced", "high", "reference"], default="balanced")
    parser.add_argument("--include-blur", action="store_true", help="Also compare Gaussian rounded rectangles; preserves existing scenes/thresholds")
    args = parser.parse_args()
    if not 2 <= args.frames <= 10000 or not 1 <= args.repeats <= 20:
        parser.error("frames must be 2..10000, repeats 1..20")
    root = Path(__file__).resolve().parents[2]
    suffix = ".exe" if sys.platform == "win32" else ""
    binaries = {"jadix": root / f"target/debug/examples/gpu_acceptance{suffix}",
                "vello": root / f"benchmarks/vello-reference/target/debug/jadix-vello-reference{suffix}"}
    for binary in binaries.values():
        if not binary.is_file():
            raise SystemExit(f"Build the reference and acceptance binaries first: missing {binary}")
    output = (root / args.output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    runs = []
    for backend in args.backends:
        for repeat in range(args.repeats):
            pair = {}
            for renderer in (["jadix", "vello"] if repeat % 2 == 0 else ["vello", "jadix"]):
                destination = output / backend / str(repeat) / renderer
                command = [str(binaries[renderer]), backend, str(destination), str(args.frames)]
                if renderer == "jadix":
                    command.extend(["premultiplied" if args.premultiplied else "straight", args.quality, "reference-scenes-blur" if args.include_blur else "reference-scenes"])
                elif args.include_blur:
                    command.append("include-blur")
                subprocess.run(command, cwd=root, check=True, stdout=subprocess.DEVNULL, timeout=180)
                report = json.loads((destination / "report.json").read_text(encoding="utf-8"))
                if report["status"] != "pass":
                    raise RuntimeError(f"{renderer} failed: {destination}")
                pair[renderer] = report
            for field in ["adapter", "platform", "arch", "driver", "driver_info"]:
                if pair["jadix"][field] != pair["vello"][field]:
                    raise RuntimeError(f"Incompatible comparison: {backend} {field}")
            rows = []
            for reference in pair["vello"]["results"]:
                case, size = reference["case"], reference["size"]
                own = next(v for v in pair["jadix"]["results"] if (v["case"], v["size"]) == (case, size))
                pixels = []
                for renderer in ["jadix", "vello"]:
                    with Image.open(output / backend / str(repeat) / renderer / f"{case}-{size}-gpu.png") as im:
                        if im.size != (size, size):
                            raise RuntimeError("Image dimensions differ")
                        pixels.append(list(im.convert("RGBA").get_flattened_data()))
                opaque = [(a, b) for a, b in zip(*pixels) if a[3] >= 254 and b[3] >= 254]
                rgb_mae = sum(abs(a[c] - b[c]) for a, b in opaque for c in range(3)) / (3 * len(opaque)) if opaque else None
                alpha_mae = sum(abs(a[3] - b[3]) for a, b in zip(*pixels)) / (size * size)
                rows.append({"case": case, "size": size, "jadix": own["total_frame"], "vello": reference["total_frame"],
                             "opaque_rgb_mae": rgb_mae, "opaque_pixels": len(opaque), "alpha_mae": alpha_mae,
                             "finite_quality_envelope_pass": rgb_mae is not None and rgb_mae <= 2 and alpha_mae <= 8})
            runs.append({"backend": backend, "repeat": repeat, "adapter": pair["jadix"]["adapter"], "measurements": rows})
            deduplicate_images(output / backend, repeat)
            print(f"Completed {backend} pair {repeat + 1}/{args.repeats}", flush=True)
    summary = []
    for backend in args.backends:
        for case in ["compound", "gradient-grid", "dynamic-curves"] + (["blurred-rrect"] if args.include_blur else []):
            for size in [256, 512]:
                rows = [v for run in runs if run["backend"] == backend for v in run["measurements"] if (v["case"], v["size"]) == (case, size)]
                own = statistics.median(v["jadix"]["p50_ms"] for v in rows)
                ref = statistics.median(v["vello"]["p50_ms"] for v in rows)
                summary.append({"backend": backend, "case": case, "size": size, "jadix_median_p50_ms": own,
                                "vello_median_p50_ms": ref, "vello_over_jadix_ratio": ref / own,
                                "all_quality_envelopes_pass": all(v["finite_quality_envelope_pass"] for v in rows)})
    report = {"reference": "vello_gpu =0.3.0; upstream wgpu =30.0.0; own Cargo.lock", "frames": args.frames, "repeats": args.repeats,
              "included_blur":args.include_blur, "jadix_quality": args.quality, "jadix_output": "linear-premultiplied" if args.premultiplied else "straight",
              "blur_limits": "Optional blur grid uses 16 rectangles with radius 8 and dynamic sigma 2..4 in local units; Vello bounds at 2.5 sigma, Jadix at 4 sigma. Opaque RGB and all alpha only; different blur approximations, no glyph/luminance performance comparison.",
              "method": "Alternating process order, identical dimensions, opaque colors, linear-sRGB conic gradients and dynamic cubic control points; new scene each frame; 5 warmups; dev opt-level=1; serialized completion.",
              "limits": "Jadix uses its wgpu fork and MSAA/SSAA; Vello uses upstream wgpu and analytic AA. Vello exports display-sRGB premultiplied pixels to Unorm; Jadix exports straight display-sRGB or linear-light premultiplied RGB through an sRGB target, according to jadix_output. Only common opaque RGB and all alpha are compared. These finite envelopes do not prove renderer equivalence. No memory, browser, text, images or other Vello families measured.",
              "quality_status": "complete-with-gaps" if any(not v["all_quality_envelopes_pass"] for v in summary) else "finite-envelopes-pass",
              "runs": runs, "summary": summary}
    (output / "comparison.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(summary, indent=2))


def deduplicate_images(backend_directory, repeat):
    """Keep differing captures, reference byte-identical repeats by SHA-256 manifest."""
    for renderer in ["jadix", "vello"]:
        directory = backend_directory / str(repeat) / renderer
        entries = []
        for path in sorted(directory.glob("*.png")):
            content = path.read_bytes()
            canonical = backend_directory / "0" / renderer / path.name
            same = repeat > 0 and canonical.exists() and content == canonical.read_bytes()
            entries.append({"name": path.name, "sha256": hashlib.sha256(content).hexdigest(),
                            "bytes": len(content), "capture": str((canonical if same else path).relative_to(backend_directory)).replace("\\", "/")})
            if same:
                path.unlink()
        (directory / "image-manifest.json").write_text(json.dumps(entries, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
