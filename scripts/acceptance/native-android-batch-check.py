"""Execute the Android delta ledger on a JVM; this does not replace Android UI execution."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[2]
JSON_SHA256 = "7dd2de1533e766e2b6c07d8194da85145ad313a167a4760050f5c9b6839604fe"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json-jar", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    if arguments.output.exists() or any((parent / "snapshot.json").exists() for parent in [arguments.output.parent, *arguments.output.parents]):
        parser.error("choose a new report outside archived snapshots")
    digest = hashlib.sha256(arguments.json_jar.read_bytes()).hexdigest()
    if digest != JSON_SHA256:
        parser.error("expected unchanged org.json:json:20260814")
    java = Path(os.environ["JAVA_HOME"]) / "bin"
    sources = [ROOT / "crates/jadix_native/platforms/android/src/dev/jadix/nativeui/NativeBatch.java",
               ROOT / "crates/jadix_native/tests/android/NativeBatchHarness.java"]
    report = {"json_sha256": digest, "sources": [{"path": str(source), "sha256": hashlib.sha256(source.read_bytes()).hexdigest()} for source in sources]}
    with tempfile.TemporaryDirectory(prefix="jadix-native-batch-") as temporary:
        classes = Path(temporary)
        suffix = ".exe" if os.name == "nt" else ""
        command = [str(java / f"javac{suffix}"), "-encoding", "UTF-8", "--release", "8", "-classpath", str(arguments.json_jar.resolve()), "-d", str(classes), *map(str, sources)]
        subprocess.run(command, check=True, timeout=60)
        result = subprocess.run([str(java / f"java{suffix}"), "-cp", str(classes) + os.pathsep + str(arguments.json_jar.resolve()), "dev.jadix.nativeui.NativeBatchHarness"], check=True, timeout=30, capture_output=True, text=True, encoding="utf-8")
        report["result"] = json.loads(result.stdout)
    report["limits"] = ["Only delta-ledger behavior ran here; Android View, IME, JNI callbacks and rendering require Android execution."]
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report["result"], ensure_ascii=False))


if __name__ == "__main__":
    main()
