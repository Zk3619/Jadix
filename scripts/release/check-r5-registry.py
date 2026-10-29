import argparse
import datetime
import json
import os
from pathlib import Path
import tomllib
import urllib.error
import urllib.parse
import urllib.request


def request(path):
    address = "https://crates.io/api/v1/" + path
    message = urllib.request.Request(address, headers={"User-Agent": "Jadix-release-preflight (https://github.com/Zk3619/Jadix)"})
    with urllib.request.urlopen(message, timeout=30) as response:
        return json.load(response)


def windows_credential_present():
    if os.name != "nt":
        return False
    import ctypes
    from ctypes import wintypes

    class Credential(ctypes.Structure):
        _fields_ = [("Flags", wintypes.DWORD), ("Type", wintypes.DWORD),
                    ("TargetName", wintypes.LPWSTR), ("Comment", wintypes.LPWSTR),
                    ("LastWritten", wintypes.FILETIME), ("CredentialBlobSize", wintypes.DWORD),
                    ("CredentialBlob", ctypes.c_void_p)]

    library = ctypes.WinDLL("Advapi32.dll", use_last_error=True)
    pointer = ctypes.POINTER(ctypes.POINTER(Credential))()
    count = wintypes.DWORD()
    enumerate_credentials = library.CredEnumerateW
    enumerate_credentials.argtypes = [wintypes.LPCWSTR, wintypes.DWORD, ctypes.POINTER(wintypes.DWORD), ctypes.POINTER(ctypes.POINTER(ctypes.POINTER(Credential))) ]
    enumerate_credentials.restype = wintypes.BOOL
    if not enumerate_credentials("cargo-registry:*", 0, ctypes.byref(count), ctypes.byref(pointer)):
        if ctypes.get_last_error() == 1168:
            return False
        raise OSError("Windows credential inventory unavailable")
    try:
        return any(pointer[index].contents.TargetName in {
            "cargo-registry:sparse+https://index.crates.io/",
            "cargo-registry:https://index.crates.io/",
            "cargo-registry:https://github.com/rust-lang/crates.io-index",
        } and pointer[index].contents.CredentialBlobSize > 0 for index in range(count.value))
    finally:
        library.CredFree.argtypes = [ctypes.c_void_p]
        library.CredFree(pointer)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--closure", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    closure = json.loads(arguments.closure.read_text(encoding="utf-8"))
    packages = {row["id"]: row for row in closure["packages"]}
    expected = [packages[key] for key in closure["publication_order"]]
    cargo_home = Path(os.environ["CARGO_HOME"]) if os.environ.get("CARGO_HOME") else Path.home() / ".cargo"
    token_present = any(os.environ.get(name) for name in ("CARGO_REGISTRY_TOKEN", "CARGO_REGISTRIES_CRATES_IO_TOKEN"))
    for filename in ("credentials.toml", "credentials"):
        path = cargo_home / filename
        if path.is_file():
            credentials = tomllib.loads(path.read_text(encoding="utf-8-sig"))
            token_present = token_present or bool(credentials.get("registry", {}).get("token")) or bool(credentials.get("registries", {}).get("crates-io", {}).get("token"))
    windows_credential = windows_credential_present()
    token_present = token_present or windows_credential
    observations = {}
    network_error = None
    try:
        page = 1
        while True:
            response = request("crates?" + urllib.parse.urlencode({"q": "jadix", "page": page, "per_page": 100}))
            observations.update({row["id"]: row for row in response["crates"]})
            if page * 100 >= response["meta"]["total"]:
                break
            page += 1
    except (OSError, ValueError) as problem:
        network_error = str(problem)
    rows = []
    for package in expected:
        row = {"name": package["name"], "version": package["version"], "manifest_path": package["manifest"], "existing": None if network_error else package["name"] in observations, "version_already_published": None, "owners": [], "permission_verified": False}
        if row["existing"]:
            try:
                versions = request("crates/" + package["name"])["versions"]
                row["version_already_published"] = any(version["num"] == package["version"] for version in versions)
                row["owners"] = [owner.get("login") for owner in request("crates/" + package["name"] + "/owners")["users"]]
            except (OSError, ValueError) as problem:
                row["query_error"] = str(problem)
        elif row["existing"] is False:
            row["version_already_published"] = False
        rows.append(row)
    blockers = []
    if network_error:
        blockers.append("registry_query_failed")
    if not token_present:
        blockers.append("crates_io_publish_credential_not_found")
    if any(row["version_already_published"] for row in rows):
        blockers.append("planned_version_already_published_requires_content_identity_review")
    blockers.append("publish_permission_not_authenticated")
    result = {"created": datetime.datetime.now().astimezone().isoformat(), "registry": "crates-io", "index": "sparse+https://index.crates.io/", "api": "https://crates.io/api/v1", "repository": "https://github.com/Zk3619/Jadix", "credential_present": token_present, "windows_credential_present": windows_credential, "network_error": network_error, "publication_order": rows, "blockers": blockers, "ready_to_publish": not blockers, "uploads_performed": 0, "scope": "Public name/version/owner queries and credential-presence checks only. No token values are logged. Name availability and Cargo dry-run do not prove authenticated upload rights. Read-only preflight never publishes."}
    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({"registry": result["registry"], "packages": len(rows), "existing_names": sum(row["existing"] is True for row in rows), "credential_present": token_present, "blockers": blockers, "network_error": network_error}, ensure_ascii=False))
    return 0 if result["ready_to_publish"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
