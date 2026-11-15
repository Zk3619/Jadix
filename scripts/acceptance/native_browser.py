import hashlib
import json
from pathlib import Path


def validate_output(output):
    if any((parent / "snapshot.json").exists() for parent in (output, *output.parents)):
        raise ValueError("output contains an archived snapshot; select a new directory")
    if output.exists() and any(output.iterdir()):
        raise ValueError("output must be empty; select a new directory")


def artifact(site, application="notes", *, release=True):
    site = site.resolve()
    source = (site / "build.json").read_bytes()
    build = json.loads(source)
    if application not in ("notes", "controls", "field") or build["application"] != application:
        raise ValueError("artifact application does not match the requested sample")
    if build["rust_profile"] not in ("release", "debug") or release and build["rust_profile"] != "release":
        raise ValueError("requires a release-profile application" if release else "unknown Rust build profile")
    seen = set()
    for item in build["files"]:
        path = (site / item["path"].replace(chr(92), "/")).resolve()
        if not path.is_relative_to(site) or path in seen:
            raise ValueError("artifact path escaped its site or was duplicated")
        seen.add(path)
        data = path.read_bytes()
        if len(data) != item["bytes"] or hashlib.sha256(data).hexdigest() != item["sha256"]:
            raise ValueError("artifact does not match its build report")
    required = ["index.html", "pkg/jadix_native_controls.js", "pkg/jadix_native_controls_bg.wasm"]
    if not all((site / name).resolve() in seen for name in required):
        raise ValueError("artifact inventory is missing required files")
    return {**build, "build_report_sha256": hashlib.sha256(source).hexdigest()}


def launch_options(engine, channel, executable=None):
    if engine not in ("chromium", "firefox", "webkit"):
        raise ValueError("unknown browser engine")
    if engine != "chromium" and channel is not None:
        raise ValueError("--channel is only valid for Chromium")
    if executable is not None:
        executable = Path(executable).resolve()
        if engine != "chromium" or not executable.is_file():
            raise ValueError("--executable requires an existing Chromium browser file")
        return {"headless": True, "channel": channel or "msedge", "executable_path": str(executable)}
    return {"headless": True, **({"channel": channel or "msedge"} if engine == "chromium" else {})}
