import argparse
import functools
import hashlib
import http.server
import json
import subprocess
import tarfile
import threading
import time
from pathlib import Path


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--closure", type=Path, required=True)
    parser.add_argument("--registry", type=Path, required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--package")
    parser.add_argument("--sources", type=Path, required=True)
    arguments = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    evidence = arguments.evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=False)
    registry = arguments.registry.resolve()
    sources = arguments.sources.resolve()
    if sources == root or root in sources.parents:
        raise ValueError("dry-run sources must be outside the repository workspace")
    sources.mkdir(parents=True, exist_ok=False)
    requests = []
    closure = json.loads(arguments.closure.read_text(encoding="utf-8-sig"))
    packages = {package["id"]: package for package in closure["packages"]}

    class Handler(http.server.SimpleHTTPRequestHandler):
        def log_message(self, format, *values):
            requests.append({"method": self.command, "path": self.path, "status": values[1]})

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Handler, directory=str(registry)))
    origin = f"http://127.0.0.1:{server.server_port}"
    configuration = registry / "index/config.json"
    previous = configuration.read_bytes() if configuration.exists() else None
    configuration.write_text(json.dumps({"dl": origin + "/{crate}-{version}.crate", "api": origin}), encoding="utf-8")
    config = evidence / "registry-config.toml"
    config.write_text('[registries.r4-candidate]\nindex = ' + json.dumps("sparse+" + origin + "/index/") +
                      '\n[source.crates-io]\nreplace-with = "r4-local"\n[source.r4-local]\nlocal-registry = ' + json.dumps(registry.as_posix()) + '\n[http]\nproxy = ""\n', encoding="utf-8")
    worker = threading.Thread(target=server.serve_forever, daemon=True)
    worker.start()
    records = []
    try:
        for identity in closure["publication_order"]:
            package = packages[identity]
            if arguments.package and package["name"] != arguments.package:
                continue
            filename = f"{package['name']}-{package['version']}"
            archive = registry / (filename + ".crate")
            with tarfile.open(archive, "r:gz") as packaged:
                packaged.extractall(sources, filter="data")
            manifest = sources / filename / "Cargo.toml"
            omitted_metadata = {}
            for reserved in ("Cargo.toml.orig", ".cargo_vcs_info.json"):
                generated = manifest.parent / reserved
                if generated.exists():
                    omitted_metadata[reserved] = hashlib.sha256(generated.read_bytes()).hexdigest()
                    generated.unlink()
            command = ["cargo", "publish", "--manifest-path", str(manifest), "--dry-run", "--no-verify",
                       "--allow-dirty", "--registry", "r4-candidate", "--config", str(config),
                       "--target-dir", str(root / "target")]
            started = time.monotonic()
            log = evidence / (package["name"] + ".log")
            with log.open("wb") as output:
                result = subprocess.run(command, cwd=root, stdout=output, stderr=subprocess.STDOUT)
            records.append({"package": package["name"], "arguments": command, "exit_code": result.returncode,
                            "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
                            "packaged_manifest_sha256": hashlib.sha256(manifest.read_bytes()).hexdigest(),
                            "omitted_generated_metadata": omitted_metadata,
                            "elapsed_seconds": time.monotonic() - started,
                            "log_sha256": hashlib.sha256(log.read_bytes()).hexdigest()})
            (evidence / "results.json").write_text(json.dumps(records, indent=2), encoding="utf-8")
            print(f"{package['name']}: exit={result.returncode}", flush=True)
        if not records:
            raise ValueError("no package selected")
    finally:
        server.shutdown()
        server.server_close()
        worker.join()
        if previous is None:
            configuration.unlink()
        else:
            configuration.write_bytes(previous)
        (evidence / "http-requests.json").write_text(json.dumps(requests, indent=2), encoding="utf-8")
        assert not any(request["method"] not in ("GET", "HEAD") for request in requests)
    if any(record["exit_code"] for record in records):
        raise SystemExit(1)


if __name__ == "__main__":
    main()
