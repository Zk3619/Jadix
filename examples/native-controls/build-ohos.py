"""Build the shared Rust page and ArkUI NativeNode host against one explicit SDK."""

import argparse
from datetime import datetime, timezone
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tomllib
import zipfile


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("native_toolchain", ROOT / "scripts/native.py")
NATIVE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(NATIVE)


def project_configuration(project):
    config = json.loads((project / "native-ohos.json").read_text(encoding="utf-8"))
    if (not isinstance(config, dict) or config.get("schema") != 1
            or not isinstance(config.get("name"), str)
            or not isinstance(config.get("application_id"), str)
            or not re.fullmatch(r"[a-z][a-z0-9_-]{0,63}", config["name"])
            or not re.fullmatch(r"dev\.jadix\.created\.[a-z][a-z0-9_]{0,63}", config["application_id"])
            or config.get("module") != "entry" or config.get("ability") != "EntryAbility"):
        raise ValueError("OHOS project configuration is invalid")
    cargo = tomllib.loads((project / "Cargo.toml").read_text(encoding="utf-8"))
    if (cargo.get("package", {}).get("name") != config["name"]
            or cargo.get("lib", {}).get("name") != "jadix_native_controls"
            or "workspace" not in cargo):
        raise ValueError("OHOS project Cargo manifest differs from its configuration or NAPI ABI")
    app = json.loads((project / "ohos/AppScope/app.json5").read_text(encoding="utf-8"))["app"]
    module = json.loads((project / "ohos/entry/src/main/module.json5").read_text(encoding="utf-8"))["module"]
    if (app.get("bundleName") != config["application_id"] or module.get("name") != config["module"]
            or module.get("mainElement") != config["ability"]
            or not any(ability.get("name") == config["ability"] for ability in module.get("abilities", []))):
        raise ValueError("OHOS project bundle or ability differs from its configuration")
    return config


def environment(sdk, target, triple, output):
    native = Path(sdk["native"])
    result = os.environ.copy()
    suffix = ".exe" if os.name == "nt" else ""
    key = target.replace("-", "_")
    result.update({"JADIX_OHOS_SDK": sdk["directory"], "OHOS_NDK_HOME": sdk["directory"],
                   f"CC_{key}": str(native / f"llvm/bin/clang{suffix}"),
                   f"CXX_{key}": str(native / f"llvm/bin/clang++{suffix}"),
                   f"AR_{key}": str(native / f"llvm/bin/llvm-ar{suffix}"),
                   f"CFLAGS_{key}": f'--target={triple} --sysroot="{native / "sysroot"}"',
                   f"CXXFLAGS_{key}": f'--target={triple} --sysroot="{native / "sysroot"}"',
                   "CC_SHELL_ESCAPED_FLAGS": "1", "CARGO_INCREMENTAL": "0", "CARGO_PROFILE_DEV_DEBUG": "0"})
    compiler = str(native / f"llvm/bin/clang{suffix}")
    result[f"CARGO_TARGET_{key.upper()}_LINKER"] = compiler
    flags = ["-C", f"link-arg=--target={triple}", "-C", f"link-arg=--sysroot={native / 'sysroot'}",
             "-C", f"link-arg=-L{native / 'llvm/lib' / triple}"]
    result["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
    result.pop("RUSTFLAGS", None)
    result["HVIGOR_USER_HOME"] = str(output / "hvigor-cache")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--project", type=Path, help="Independent project created by native.py create --platform ohos")
    parser.add_argument("--architecture", choices=["x86_64", "arm64"], default="x86_64")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--native-only", action="store_true")
    parser.add_argument("--features", choices=["popover", "selection"], help="Build a shared Popover or Select product instead of Controls")
    parser.add_argument("--release", action="store_true")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    if args.project and args.features:
        parser.error("sample features cannot override a generated project's manifest")
    application = args.project.resolve() if args.project else Path(__file__).parent
    config = project_configuration(application) if args.project else None
    cargo_root = application if config else ROOT
    output = args.output.resolve()
    if output.exists() or any((directory / "snapshot.json").exists() for directory in [output, *output.parents]):
        parser.error("output must be a new directory outside archived snapshots")
    sdk = NATIVE.sdk_identity(args.sdk)
    if not sdk["available"] or int(sdk["metadata"].get("apiVersion", 0)) < 20:
        parser.error("select an installed OpenHarmony native SDK at API 20 or later")
    target = "x86_64-unknown-linux-ohos" if args.architecture == "x86_64" else "aarch64-unknown-linux-ohos"
    triple = "x86_64-linux-ohos" if args.architecture == "x86_64" else "aarch64-linux-ohos"
    abi = "x86_64" if args.architecture == "x86_64" else "arm64-v8a"
    output.mkdir(parents=True)
    report = {"schema": 1, "started_at": datetime.now(timezone.utc).isoformat(), "sdk": sdk,
              "target": target, "abi": abi, "profile": "release" if args.release else "debug",
              "source": NATIVE.tree_identity(application), "host_source": NATIVE.tree_identity(ROOT / "crates/jadix_native"),
              "shared_source": [NATIVE.tree_identity(ROOT / "crates" / name) for name in ["jadix", "jadix_core", "jadix_components", "jadix_macros", "jadix_tasks", "jadix_layout", "jadix_style_tokens", "jadix_taffy", "jadix_theme", "jadix_animation", "jadix_hot_reload", "jadix_document"]],
              "workspace_manifest": NATIVE.file_identity(ROOT / "Cargo.toml"),
              "cargo_lock": NATIVE.file_identity(ROOT / "Cargo.lock"), "commands": [],
              "source_unchanged": False,
              "native_built": False, "hap_built": False, "signed": False, "runtime_acceptance": False,
              "application": "project" if config else "controls", "configuration": config,
              "features": None if config else ["ohos", *([args.features] if args.features else [])],
              "application_lock": NATIVE.file_identity(cargo_root / "Cargo.lock"),
              "builder": NATIVE.file_identity(Path(__file__).resolve())}
    env = environment(sdk, target, triple, output)
    target_directory = Path(env.get("CARGO_TARGET_DIR", ROOT / "target")).resolve()
    env["CARGO_TARGET_DIR"] = str(target_directory)

    def run(command, cwd=ROOT):
        print(subprocess.list2cmdline([str(value) for value in command]), flush=True)
        with (output / "build.log").open("a", encoding="utf-8") as log:
            result = subprocess.run(command, cwd=cwd, env=env, stdout=log, stderr=subprocess.STDOUT)
        report["commands"].append({"arguments": [str(value) for value in command], "cwd": str(cwd), "exit_code": result.returncode})
        if result.returncode:
            raise RuntimeError(f"command failed ({result.returncode}); see {output / 'build.log'}")

    def assert_sources():
        for key in ["source", "host_source"]:
            if report[key] != NATIVE.tree_identity(Path(report[key]["directory"])):
                raise RuntimeError("native source changed during compilation or packaging")
        for source in report["shared_source"]:
            if source != NATIVE.tree_identity(Path(source["directory"])):
                raise RuntimeError("shared framework source changed during compilation or packaging")
        if report["cargo_lock"] != NATIVE.file_identity(ROOT / "Cargo.lock"):
            raise RuntimeError("Cargo.lock changed during compilation or packaging")
        if report["workspace_manifest"] != NATIVE.file_identity(ROOT / "Cargo.toml"):
            raise RuntimeError("workspace Cargo.toml changed during compilation or packaging")
        if report["application_lock"] != NATIVE.file_identity(cargo_root / "Cargo.lock"):
            raise RuntimeError("application Cargo.lock changed during compilation or packaging")
        if report["builder"] != NATIVE.file_identity(Path(__file__).resolve()):
            raise RuntimeError("OHOS builder changed during compilation or packaging")

    exit_code = 0
    try:
        command = ["cargo", "build", "--locked", "-p", "jadix_native_controls", "--lib", "--no-default-features", "--features", ",".join(report["features"] or []), "--target", target]
        if config:
            command = ["cargo", "build", "--locked", "--manifest-path", str(application / "Cargo.toml"), "--lib", "--target", target]
        if args.release:
            command.append("--release")
        if args.offline:
            command.append("--offline")
        run(command, cargo_root)
        assert_sources()
        library = target_directory / target / report["profile"] / "libjadix_native_controls.so"
        with library.open("rb") as stream:
            header = stream.read(20)
        machine = 62 if args.architecture == "x86_64" else 183
        if len(header) != 20 or header[:6] != b"\x7fELF\x02\x01" or int.from_bytes(header[18:20], "little") != machine:
            raise RuntimeError("native library has the wrong ELF machine or encoding")
        shutil.copy2(library, output / library.name)
        report["native_built"] = True
        report["library"] = NATIVE.file_identity(output / library.name)
        if not args.native_only:
            if int(sdk["metadata"]["apiVersion"]) < 22:
                raise RuntimeError("the sample HAP project currently selects HarmonyOS API 22; use --native-only for API 20")
            project = output / "project"
            shutil.copytree(application / "ohos", project, ignore=shutil.ignore_patterns("oh_modules", ".hvigor", "build", "libs"))
            libs = project / "entry/libs" / abi
            libs.mkdir(parents=True)
            shutil.copy2(library, libs / library.name)
            shutil.copy2(Path(sdk["native"]) / "llvm/lib" / triple / "libc++_shared.so", libs / "libc++_shared.so")
            deveco = Path(os.environ.get("DEVECO_SDK_HOME", args.sdk)).parent
            env["DEVECO_SDK_HOME"] = str(deveco / "sdk")
            env["NODE_HOME"] = str(deveco / "tools/node")
            env["PATH"] = env["NODE_HOME"] + os.pathsep + env["PATH"]
            batch = ".bat" if os.name == "nt" else ""
            run([str(deveco / f"tools/ohpm/bin/ohpm{batch}"), "install", "--all"], project)
            run([str(deveco / f"tools/hvigor/bin/hvigorw{batch}"), "--mode", "module", "-p", "product=default", "-p", "module=entry@default", "-p", "buildMode=release", "assembleHap", "--no-daemon"], project)
            haps = list((project / "entry/build").rglob("*-unsigned.hap"))
            if len(haps) != 1:
                raise RuntimeError("expected exactly one unsigned HAP")
            hap = output / f"jadix-native-controls-{args.architecture}-unsigned.hap"
            shutil.copy2(haps[0], hap)
            stripped = output / "libjadix_native_controls.packaged.so"
            shutil.copy2(library, stripped)
            strip = Path(sdk["native"]) / ("llvm/bin/llvm-strip.exe" if os.name == "nt" else "llvm/bin/llvm-strip")
            run([str(strip), "--strip-all", str(stripped)])
            with zipfile.ZipFile(hap) as archive:
                member = f"libs/{abi}/libjadix_native_controls.so"
                if member not in archive.namelist() or archive.read(member) != stripped.read_bytes():
                    raise RuntimeError("HAP native library does not match the independently reproduced SDK stripping transformation")
            report["packaged_library"] = {"member": member, **NATIVE.file_identity(stripped), "transformation": "selected SDK llvm-strip --strip-all", "strip_tool": NATIVE.file_identity(strip)}
            report["hap_built"] = True
            report["hap"] = NATIVE.file_identity(hap)
        assert_sources()
        report["source_unchanged"] = True
    except (OSError, ValueError, RuntimeError) as error:
        report["error"] = str(error)
        print(str(error), file=sys.stderr)
        exit_code = 1
    finally:
        report["finished_at"] = datetime.now(timezone.utc).isoformat()
        report["limits"] = ["Building does not establish installation or ArkUI runtime acceptance.", "Unsigned HAP requires a compatible test system or a separately configured signing profile."]
        (output / "build.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
