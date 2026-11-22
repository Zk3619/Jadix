import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))
SPEC = importlib.util.spec_from_file_location("native_workflow_tests", SCRIPTS / "native_workflow.py")
WORKFLOW = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(WORKFLOW)


def elf_fixture(machine):
    return b"\x7fELF\x02\x01\x01" + bytes(9) + b"\x03\x00" + machine.to_bytes(2, "little")


class AndroidBuildInputTests(unittest.TestCase):
    def test_android_build_input_identity_detects_manifest_and_product_changes(self):
        builder_path = Path(__file__).resolve().parents[2] / "examples/native-controls/build-android.py"
        spec = importlib.util.spec_from_file_location("android_native_build", builder_path)
        builder = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(builder)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for name in ["Cargo.toml", "Cargo.lock"]:
                (root / name).write_text("initial", encoding="utf-8")
            for name in ["scripts/native.py", "scripts/native_android.py", "examples/native-controls/build-android.py"]:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("initial", encoding="utf-8")
            for name in [
                "crates/jadix_native", "crates/jadix", "crates/jadix_core",
                "crates/jadix_components", "crates/jadix_macros", "crates/jadix_tasks",
                "crates/jadix_layout", "crates/jadix_style_tokens", "crates/jadix_taffy",
                "crates/jadix_theme", "crates/jadix_animation", "crates/jadix_hot_reload",
                "crates/jadix_document", "crates/jadix_3d", "crates/jadix_net", "crates/jadix_shell", "examples/native-controls",
            ]:
                directory = root / name
                directory.mkdir(parents=True, exist_ok=True)
                (directory / "source.rs").write_text("initial", encoding="utf-8")
            native, identity = builder.build_inputs(root)
            builder.assert_build_inputs_unchanged(root, native, identity)
            (root / "Cargo.toml").write_text("changed", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "inputs changed"):
                builder.assert_build_inputs_unchanged(root, native, identity)
            (root / "Cargo.toml").write_text("initial", encoding="utf-8")
            (root / "examples/native-controls/source.rs").write_text("changed", encoding="utf-8")
            with self.assertRaisesRegex(RuntimeError, "inputs changed"):
                builder.assert_build_inputs_unchanged(root, native, identity)


class NativeWorkflowTests(unittest.TestCase):
    def test_ohos_create_and_project_identity_reject_mismatched_bundle_and_cargo(self):
        from argparse import Namespace

        spec = importlib.util.spec_from_file_location("ohos_project_build", SCRIPTS.parent / "examples/native-controls/build-ohos.py")
        builder = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(builder)
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            project = directory / "consumer"
            with patch.object(WORKFLOW, "capture"):
                result = WORKFLOW.create(Namespace(destination=project, name="field-app", platform="ohos"), directory / "create.json")
            self.assertEqual(result["platform"], "ohos")
            config = builder.project_configuration(project)
            self.assertEqual(config["application_id"], "dev.jadix.created.field_app")
            app_file = project / "ohos/AppScope/app.json5"
            original = app_file.read_text(encoding="utf-8")
            app_file.write_text(original.replace(config["application_id"], "dev.jadix.created.other"), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "bundle or ability"):
                builder.project_configuration(project)
            app_file.write_text(original, encoding="utf-8")
            cargo = project / "Cargo.toml"
            cargo.write_text(cargo.read_text(encoding="utf-8").replace('name = "jadix_native_controls"', 'name = "other_library"'), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "NAPI ABI"):
                builder.project_configuration(project)

    def test_ohos_retained_project_hap_checks_bundle_even_when_archive_hash_is_updated(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            library = directory / "libjadix_native_controls.so"
            library.write_bytes(elf_fixture(62))
            hap = directory / "jadix-native-controls-x86_64-unsigned.hap"
            member = "libs/x86_64/libjadix_native_controls.so"
            config = {"schema": 1, "application_id": "dev.jadix.created.field_app", "module": "entry", "ability": "EntryAbility"}
            build = {"schema": 1, "target": "x86_64-unknown-linux-ohos", "abi": "x86_64", "application": "project", "configuration": config,
                     "source_unchanged": True, "native_built": True, "hap_built": True, "signed": False, "runtime_acceptance": False,
                     "sdk": {"metadata": {"apiVersion": "22"}}, "commands": [{"exit_code": 0}],
                     "library": WORKFLOW.native.file_identity(library), "packaged_library": {"member": member, **WORKFLOW.native.file_identity(library)}}
            for bundle in [config["application_id"], "dev.jadix.created.other"]:
                with zipfile.ZipFile(hap, "w") as archive:
                    archive.writestr(member, library.read_bytes())
                    archive.writestr("module.json", json.dumps({"app": {"bundleName": bundle}, "module": {"name": "entry", "mainElement": "EntryAbility"}}))
                (directory / "build.json").write_text(json.dumps({**build, "hap": WORKFLOW.native.file_identity(hap)}), encoding="utf-8")
                if bundle == config["application_id"]:
                    self.assertEqual(WORKFLOW.ohos_artifact(directory, "x86_64", False)["configuration"], config)
                else:
                    with self.assertRaisesRegex(ValueError, "HAP bundle or ability mismatch"):
                        WORKFLOW.ohos_artifact(directory, "x86_64", False)

    def test_android_create_renders_independent_project_and_rejects_invalid_platform(self):
        from argparse import Namespace

        builder_path = SCRIPTS.parent / "examples/native-controls/build-android.py"
        spec = importlib.util.spec_from_file_location("android_project_build", builder_path)
        builder = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(builder)
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            destination = directory / "consumer"
            arguments = Namespace(destination=destination, name="field-app", platform="ios")
            with self.assertRaisesRegex(ValueError, "templates"):
                WORKFLOW.create(arguments, directory / "create.json")
            self.assertFalse(destination.exists())
            arguments.platform = "android"
            with patch.object(WORKFLOW, "capture") as capture:
                result = WORKFLOW.create(arguments, directory / "create.json")
            self.assertEqual(result["platform"], "android")
            capture.assert_called_once()
            config = builder.project_configuration(destination)
            self.assertEqual(config["name"], "field-app")
            self.assertEqual(config["application_id"], "dev.jadix.created.field_app")
            for path in destination.rglob("*"):
                if path.is_file() and path.name != "Cargo.lock":
                    self.assertNotIn("@ROOT@", path.read_text(encoding="utf-8"))
            configuration = destination / "native-android.json"
            import xml.etree.ElementTree as ET
            source = destination / "android/AndroidManifest.xml"
            original = source.read_bytes()
            routed = {**config, "url_schemes": ["field-app", "field.auth"]}
            configuration.write_text(json.dumps(routed), encoding="utf-8")
            checked = builder.project_configuration(destination)
            generated = builder.configured_manifest(source, directory / "routed.xml", checked)
            android = "{http://schemas.android.com/apk/res/android}"
            self.assertEqual([data.get(android + "scheme") for data in ET.parse(generated).findall("application/activity/intent-filter/data")], routed["url_schemes"])
            self.assertEqual(source.read_bytes(), original)
            for schemes in [["field", "field"], ["BadScheme"], ["field://callback"], "field", [None], ["a"] * 17]:
                configuration.write_text(json.dumps({**config, "url_schemes": schemes}), encoding="utf-8")
                with self.assertRaisesRegex(ValueError, "URL schemes"):
                    builder.project_configuration(destination)
            configuration.write_text(json.dumps({**config, "application_id": "dev.jadix.created.other"}), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "manifest package"):
                builder.project_configuration(destination)
            configuration.write_text(json.dumps({**config, "name": None}), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "configuration is invalid"):
                builder.project_configuration(destination)

    def android_bundle(self, directory, machine=62):
        library = elf_fixture(machine)
        apk = directory / "native-controls.apk"
        with zipfile.ZipFile(apk, "w") as archive:
            archive.writestr("AndroidManifest.xml", b"test manifest")
            archive.writestr("classes.dex", b"test dex")
            archive.writestr("lib/x86_64/libjadix_native_controls.so", library)
        identity = WORKFLOW.native.file_identity(apk)
        import hashlib

        report = {"schema": 1, "target": "x86_64-linux-android", "abi": "x86_64", "min_sdk": 26,
                  "source_unchanged": True, "signed": True, "debuggable": True,
                  "authenticity_verified": False, "runtime_acceptance": False,
                  "build_inputs": {"fixture": True}, "toolchain": {"fixture": True},
                  "apk_identity": identity, "apk_bytes": identity["bytes"], "apk_sha256": identity["sha256"],
                  "library_bytes": len(library), "library_sha256": hashlib.sha256(library).hexdigest()}
        (directory / "build.json").write_text(json.dumps(report), encoding="utf-8")
        return report

    def test_android_artifact_rejects_library_abi_and_retained_report_tampering(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            self.android_bundle(directory)
            observation = WORKFLOW.android_artifact(directory)
            retained = directory / "retained.json"
            retained.write_text(json.dumps({"schema": 1, "operation": "build", "observations": observation}), encoding="utf-8")
            self.assertEqual(WORKFLOW.artifact(retained)[0], directory / "native-controls.apk")
            report = self.android_bundle(directory, 183)
            with self.assertRaisesRegex(ValueError, "ELF ABI"):
                WORKFLOW.android_artifact(directory)
            with self.assertRaisesRegex(ValueError, "report path mismatch"):
                WORKFLOW.artifact(retained)
            self.android_bundle(directory)
            with (directory / "native-controls.apk").open("ab") as stream:
                stream.write(b"tamper")
            with self.assertRaisesRegex(ValueError, "APK identity"):
                WORKFLOW.android_artifact(directory)
            self.android_bundle(directory)
            report["source_unchanged"] = False
            (directory / "build.json").write_text(json.dumps(report), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "incomplete"):
                WORKFLOW.android_artifact(directory)

    def test_android_build_routes_explicit_tools_and_preserves_failed_child_log(self):
        from argparse import Namespace

        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            arguments = Namespace(platform="android", android_sdk=directory / "SDK with spaces",
                                  ndk=directory / "NDK", java_home=directory / "JDK",
                                  android_target="x86_64-linux-android", offline=True, release=True,
                                  extended=True, large_list=True)
            output = directory / "build.json"

            def build(command, **kwargs):
                self.assertIn("--offline", command)
                self.assertIn("--release", command)
                self.assertIn("--extended", command)
                self.assertIn("--large-list", command)
                self.assertEqual(command[command.index("--sdk") + 1], str(arguments.android_sdk))
                destination = Path(command[command.index("--output") + 1])
                self.assertFalse(destination.exists())
                destination.mkdir()
                self.android_bundle(destination)
                return subprocess.CompletedProcess(command, 0, "build completed", "")

            with patch.object(WORKFLOW.subprocess, "run", side_effect=build):
                result = WORKFLOW.build(arguments, output)
            self.assertEqual(result["kind"], "android-native-build")
            self.assertFalse(result["runtime_acceptance"])
            with patch.object(WORKFLOW.subprocess, "run", return_value=subprocess.CompletedProcess([], 101, "", "cargo failed")):
                with self.assertRaisesRegex(ValueError, "Android build failed"):
                    WORKFLOW.build(arguments, directory / "failed.json")
            self.assertEqual((directory / "failed.log").read_text(encoding="utf-8"), "cargo failed")
            self.assertFalse((directory / "failed.json").exists())

    def android_run_fixture(self, directory, receipt_changes=None, device_changes=None, project=False):
        from argparse import Namespace

        bundle = directory / "bundle"
        bundle.mkdir()
        build = self.android_bundle(bundle)
        build.update(application="controls", package_name="dev.jadix.nativeui", extended=False, retained_rows=100)
        if project:
            build.update(application="project", package_name="dev.jadix.created.field_app", retained_rows=0)
        (bundle / "build.json").write_text(json.dumps(build), encoding="utf-8")
        observations = WORKFLOW.android_artifact(bundle)
        report = directory / "retained.json"
        report.write_text(json.dumps({"schema": 1, "operation": "build", "observations": observations}), encoding="utf-8")
        adb = directory / "adb with spaces.exe"
        adb.write_bytes(b"mock adb, never executed")
        arguments = Namespace(artifact=report, adb=adb, serial="emulator-5554", process_recovery=not project)
        devices = {"get-state": "device", "get-serialno": arguments.serial,
                   "ro.product.cpu.abilist": "x86_64,x86", "ro.build.version.sdk": "35", "ro.kernel.qemu": "1"}
        devices.update(device_changes or {})
        commands = []

        def capture(command, cwd, log):
            commands.append([str(value) for value in command])
            log.write_text("mock command log", encoding="utf-8")
            if command[0] == adb:
                self.assertEqual(command[1:3], ["-s", arguments.serial])
                return devices[command[-1]]
            self.assertEqual(command[command.index("--adb") + 1], adb)
            self.assertEqual(command[command.index("--serial") + 1], arguments.serial)
            self.assertEqual("--process-recovery" in command, arguments.process_recovery)
            destination = Path(command[command.index("--output") + 1])
            destination.mkdir()
            receipt = {"passed": True, "nonce": command[command.index("--nonce") + 1],
                       "serial": arguments.serial, "sdk": 35, "extended": False, "retained_rows": 100,
                       "retained_editing_checked": True, "process_recovery": {"passed": True},
                       "apk_sha256": build["apk_sha256"], "apk_bytes": build["apk_bytes"],
                       "runner_sha256": WORKFLOW.native.file_identity(command[1])["sha256"],
                       "artifact": {"build_report_sha256": observations["build_report_identity"]["sha256"],
                                    "apk_sha256": build["apk_sha256"], "package_name": build["package_name"]},
                       "cleanup": {"completed": True, "installed_by_this_run": True,
                                   "kept_installed": False, "package_registry_restored": True}}
            if project:
                receipt["process_recovery"].update(nonce=receipt["nonce"], package_name=build["package_name"],
                    before_pid="123", after_pid="456", value="recovery_" + receipt["nonce"], checked=True, counter=1)
            receipt.update(receipt_changes or {})
            if project and (not receipt_changes or "ownership_runner_identity" not in receipt_changes):
                receipt["ownership_runner_identity"] = WORKFLOW.native.file_identity(SCRIPTS / "acceptance/native-android-check.py")
            (destination / "checks.json").write_text(json.dumps(receipt), encoding="utf-8")
            return "mock probe completed"

        return arguments, capture, commands

    def test_android_project_run_binds_ownership_helper_and_recovery(self):
        for changes in [None, {"ownership_runner_identity": {}}, {"artifact": {}}, {"process_recovery": None},
                        {"process_recovery": {"passed": True, "before_pid": "123", "after_pid": "123"}}]:
            with self.subTest(changes=changes), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                arguments, capture, commands = self.android_run_fixture(directory, receipt_changes=changes, project=True)
                arguments.process_recovery = True
                with patch.object(WORKFLOW, "capture", side_effect=capture):
                    if changes:
                        with self.assertRaisesRegex(ValueError, "receipt"):
                            WORKFLOW.run(arguments, directory / "run.json")
                    else:
                        result = WORKFLOW.run(arguments, directory / "run.json")
                        self.assertTrue(result["native_probe_observed"])
                        self.assertTrue(commands[-1][1].endswith("native-android-project-check.py"))

    def test_android_project_instrumentation_rejects_old_duplicate_or_cross_package_receipts(self):
        spec = importlib.util.spec_from_file_location("android_project_checks", SCRIPTS / "acceptance/native-android-project-check.py")
        runner = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(runner)
        nonce = "a" * 32
        package = "dev.jadix.created.field_app"
        log = f"INSTRUMENTATION_RESULT: jadix.native.project=PASS:{nonce}\nINSTRUMENTATION_RESULT: jadix.package={package}\nINSTRUMENTATION_CODE: -1\n"
        runner.validate_receipt(log, nonce, package)
        for invalid in [log.replace(nonce, "b" * 32), log + log, log.replace(package, package + "_other"),
                        log.replace("CODE: -1", "CODE: 0"), log + "INSTRUMENTATION_FAILED: crashed\n"]:
            with self.subTest(log=invalid), self.assertRaises(ValueError):
                runner.validate_receipt(invalid, nonce, package)

    def test_android_run_binds_device_artifact_nonce_and_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            arguments, capture, commands = self.android_run_fixture(directory)
            with patch.object(WORKFLOW, "capture", side_effect=capture):
                result = WORKFLOW.run(arguments, directory / "run.json")
            self.assertTrue(result["native_probe_observed"])
            self.assertEqual(result["device"]["kind"], "reported-emulator")
            self.assertEqual(result["scenario"]["nonce"], result["nonce"])
            self.assertEqual(len(commands), 6)
            self.assertEqual(result["probe_identity"], WORKFLOW.native.file_identity(Path(result["probe"])))
            self.assertFalse((directory / "run.json").exists())

    def test_android_run_rejects_unbound_receipts_and_missing_cleanup(self):
        cases = [{"nonce": "stale"}, {"serial": "another-device"}, {"artifact": {}},
                 {"runner_sha256": "changed"}, {"apk_sha256": "changed"}, {"sdk": 34},
                 {"extended": True}, {"retained_rows": 1000}, {"retained_editing_checked": False},
                 {"cleanup": {"completed": True}}, {"process_recovery": None}, {"passed": False}]
        for changes in cases:
            with self.subTest(changes=changes), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                arguments, capture, commands = self.android_run_fixture(directory, receipt_changes=changes)
                with patch.object(WORKFLOW, "capture", side_effect=capture):
                    with self.assertRaisesRegex(ValueError, "receipt"):
                        WORKFLOW.run(arguments, directory / "run.json")
                self.assertTrue((directory / "run-files/process.log").is_file())
                self.assertFalse((directory / "run.json").exists())

    def test_android_run_rejects_device_mismatch_before_installation(self):
        for changes in [{"get-state": "offline"}, {"get-serialno": "another-device"},
                        {"ro.product.cpu.abilist": "arm64-v8a"}, {"ro.build.version.sdk": "25"}]:
            with self.subTest(changes=changes), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary)
                arguments, capture, commands = self.android_run_fixture(directory, device_changes=changes)
                with patch.object(WORKFLOW, "capture", side_effect=capture):
                    with self.assertRaisesRegex(ValueError, "device"):
                        WORKFLOW.run(arguments, directory / "run.json")
                self.assertTrue(all(command[0] == str(arguments.adb) for command in commands))

    def test_android_run_requires_explicit_device_and_keeps_child_failure_log(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            arguments, capture, commands = self.android_run_fixture(directory)
            selected_adb = arguments.adb
            arguments.adb = None
            with self.assertRaisesRegex(ValueError, "explicit"):
                WORKFLOW.run(arguments, directory / "missing.json")
            self.assertFalse((directory / "missing-files").exists())
            arguments.adb = selected_adb

            def fail(command, cwd, log):
                if command[0] == selected_adb:
                    return capture(command, cwd, log)
                log.write_text("installation rejected; previous package preserved", encoding="utf-8")
                raise ValueError("command failed; see process.log")

            with patch.object(WORKFLOW, "capture", side_effect=fail):
                with self.assertRaisesRegex(ValueError, "command failed"):
                    WORKFLOW.run(arguments, directory / "failed.json")
            self.assertIn("previous package preserved", (directory / "failed-files/process.log").read_text(encoding="utf-8"))
            self.assertFalse((directory / "failed.json").exists())

    def test_android_run_rechecks_adb_identity_after_the_probe(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            arguments, capture, commands = self.android_run_fixture(directory)

            def mutate(command, cwd, log):
                result = capture(command, cwd, log)
                if command[0] != arguments.adb:
                    arguments.adb.write_bytes(b"changed after device discovery")
                return result

            with patch.object(WORKFLOW, "capture", side_effect=mutate):
                with self.assertRaisesRegex(ValueError, "changed during the probe"):
                    WORKFLOW.run(arguments, directory / "run.json")

    def test_ohos_build_routes_to_explicit_builder_and_retains_verified_identity(self):
        from argparse import Namespace

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repo"
            (repository / "examples/native-controls").mkdir(parents=True)
            output = root / "evidence" / "build.json"

            def build(command, **kwargs):
                destination = Path(command[command.index("--output") + 1])
                self.assertEqual(command[command.index("--features") + 1], "popover")
                self.assertFalse(destination.exists(), "the builder must own directory creation")
                destination.mkdir(parents=True)
                library = destination / "libjadix_native_controls.so"
                library.write_bytes(elf_fixture(62))
                sdk = {"metadata": {"apiVersion": "20"}}
                result = {"schema": 1, "target": "x86_64-unknown-linux-ohos", "abi": "x86_64",
                          "source_unchanged": True, "native_built": True, "hap_built": False,
                          "features": ["ohos", "popover"],
                          "signed": False, "runtime_acceptance": False, "sdk": sdk, "commands": [{"exit_code": 0}],
                          "library": WORKFLOW.native.file_identity(library), "hap": None}
                (destination / "build.json").write_text(json.dumps(result), encoding="utf-8")
                return subprocess.CompletedProcess(command, 0, "builder output", "")

            arguments = Namespace(platform="ohos", sdk=Path("D:/SDK 20"), architecture="x86_64",
                                  native_only=True, offline=True, project=None, bin="native-windows",
                                  workbench=False, features="popover")
            with patch.object(WORKFLOW.native, "ROOT", repository), patch.object(WORKFLOW.subprocess, "run", side_effect=build):
                result = WORKFLOW.build(arguments, output)
            self.assertEqual(result["kind"], "ohos-native-build")
            self.assertEqual(result["features"], ["ohos", "popover"])
            self.assertFalse(result["runtime_acceptance"])
            self.assertTrue(output.with_suffix(".log").is_file())
            manifest = {"schema": 1, "operation": "build", "observations": result}
            retained = root / "retained.json"
            retained.write_text(json.dumps(manifest), encoding="utf-8")
            self.assertTrue(WORKFLOW.artifact(retained)[0].is_file())

            def wrong_features(command, **kwargs):
                completion = build(command, **kwargs)
                report = Path(command[command.index("--output") + 1]) / "build.json"
                value = json.loads(report.read_text(encoding="utf-8"))
                value["features"] = ["ohos"]
                report.write_text(json.dumps(value), encoding="utf-8")
                return completion

            with patch.object(WORKFLOW.native, "ROOT", repository), patch.object(WORKFLOW.subprocess, "run", side_effect=wrong_features):
                with self.assertRaisesRegex(ValueError, "requested sample features"):
                    WORKFLOW.build(arguments, root / "evidence" / "mismatch.json")

    def test_ohos_selection_profile_retains_its_exact_feature_identity(self):
        from argparse import Namespace

        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repo"
            (repository / "examples/native-controls").mkdir(parents=True)

            def build(command, **kwargs):
                self.assertEqual(command[command.index("--features") + 1], "selection")
                destination = Path(command[command.index("--output") + 1])
                destination.mkdir(parents=True)
                library = destination / "libjadix_native_controls.so"
                library.write_bytes(elf_fixture(62))
                result = {"schema": 1, "target": "x86_64-unknown-linux-ohos", "abi": "x86_64",
                          "source_unchanged": True, "native_built": True, "hap_built": False,
                          "features": ["ohos", "selection"], "signed": False, "runtime_acceptance": False,
                          "sdk": {"metadata": {"apiVersion": "20"}}, "commands": [{"exit_code": 0}],
                          "library": WORKFLOW.native.file_identity(library), "hap": None}
                (destination / "build.json").write_text(json.dumps(result), encoding="utf-8")
                return subprocess.CompletedProcess(command, 0, "", "")

            arguments = Namespace(platform="ohos", sdk=Path("D:/SDK"), architecture="x86_64",
                                  native_only=True, offline=True, project=None, bin="native-windows",
                                  workbench=False, features="selection")
            output = root / "evidence" / "selection.json"
            with patch.object(WORKFLOW.native, "ROOT", repository), patch.object(WORKFLOW.subprocess, "run", side_effect=build):
                result = WORKFLOW.build(arguments, output)
            self.assertEqual(result["features"], ["ohos", "selection"])
            retained = root / "retained.json"
            retained.write_text(json.dumps({"schema": 1, "operation": "build", "observations": result}), encoding="utf-8")
            self.assertTrue(WORKFLOW.artifact(retained)[0].is_file())
            self.assertFalse(result["runtime_acceptance"])

    def test_ohos_artifact_requires_matching_target_library_and_requested_hap(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            library = directory / "libjadix_native_controls.so"
            library.write_bytes(elf_fixture(62))
            hap = directory / "jadix-native-controls-x86_64-unsigned.hap"
            member = "libs/x86_64/libjadix_native_controls.so"
            with zipfile.ZipFile(hap, "w") as archive:
                archive.writestr(member, library.read_bytes())
            hap_bytes = hap.read_bytes()
            build = {"schema": 1, "target": "x86_64-unknown-linux-ohos", "abi": "x86_64",
                     "source_unchanged": True, "native_built": True, "hap_built": True,
                     "signed": False, "runtime_acceptance": False, "sdk": {"metadata": {"apiVersion": "22"}},
                     "commands": [{"exit_code": 0}],
                     "library": WORKFLOW.native.file_identity(library),
                     "packaged_library": {"member": member, **WORKFLOW.native.file_identity(library)},
                     "hap": WORKFLOW.native.file_identity(hap)}
            (directory / "build.json").write_text(json.dumps(build), encoding="utf-8")
            retained = WORKFLOW.ohos_artifact(directory, "x86_64", False)
            report = directory / "retained.json"
            report.write_text(json.dumps({"schema": 1, "operation": "build", "observations": retained}),
                              encoding="utf-8")
            self.assertEqual(WORKFLOW.artifact(report)[0], library)
            hap.write_bytes(b"tampered HAP")
            with self.assertRaisesRegex(ValueError, "HAP identity mismatch"):
                WORKFLOW.artifact(report)
            hap.write_bytes(hap_bytes)
            library.write_bytes(b"tampered library")
            with self.assertRaisesRegex(ValueError, "identity"):
                WORKFLOW.artifact(report)

            library.write_bytes(elf_fixture(62))
            (directory / "build.json").write_text(json.dumps({**build, "target": "aarch64-unknown-linux-ohos"}),
                                                  encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "report path mismatch"):
                WORKFLOW.artifact(report)

    def test_ohos_native_only_and_failed_hap_reports_are_not_conflated(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            library = directory / "libjadix_native_controls.so"
            library.write_bytes(elf_fixture(183))
            build = {"schema": 1, "target": "aarch64-unknown-linux-ohos", "abi": "arm64-v8a",
                     "source_unchanged": True, "native_built": True, "hap_built": False,
                     "signed": False, "runtime_acceptance": False, "sdk": {"metadata": {"apiVersion": "20"}},
                     "library": WORKFLOW.native.file_identity(library), "hap": None,
                     "commands": [{"exit_code": 0}]}
            (directory / "build.json").write_text(json.dumps(build), encoding="utf-8")
            retained = WORKFLOW.ohos_artifact(directory, "arm64", True)
            self.assertIsNone(retained["hap"])
            with self.assertRaisesRegex(ValueError, "HAP build was requested"):
                WORKFLOW.ohos_artifact(directory, "arm64", False)
            with self.assertRaisesRegex(ValueError, "incomplete"):
                WORKFLOW.ohos_artifact(directory, "x86_64", True)
            library.write_bytes(elf_fixture(62))
            (directory / "build.json").write_text(json.dumps({**build, "library": WORKFLOW.native.file_identity(library)}), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "ELF ABI mismatch"):
                WORKFLOW.ohos_artifact(directory, "arm64", True)

    def test_platform_specific_build_options_are_not_silently_ignored(self):
        from argparse import Namespace

        base = dict(platform="windows", sdk=Path("D:/SDK"), native_only=False,
                    project=None, bin="native-windows", workbench=False, features=None)
        with tempfile.TemporaryDirectory() as temporary:
            with self.assertRaisesRegex(ValueError, "only to --platform ohos"):
                WORKFLOW.build(Namespace(**base), Path(temporary) / "build.json")
            base["sdk"] = None
            base["extended"] = True
            with self.assertRaisesRegex(ValueError, "only to --platform android"):
                WORKFLOW.build(Namespace(**base), Path(temporary) / "wrong-platform.json")
            base["extended"] = False
            base["platform"] = "ohos"
            base["features"] = "selection,popover"
            with self.assertRaisesRegex(ValueError, "only --features popover"):
                WORKFLOW.build(Namespace(**base), Path(temporary) / "wrong-features.json")
            base["features"] = "popover"
            base["project"] = Path(temporary) / "app"
            with self.assertRaisesRegex(ValueError, "without --project"):
                WORKFLOW.build(Namespace(**base), Path(temporary) / "project-features.json")
            base["project"] = None
            base["features"] = None
            base["bin"] = "other-binary"
            with self.assertRaisesRegex(ValueError, "fixed native-controls"):
                WORKFLOW.build(Namespace(**base), Path(temporary) / "ohos.json")

    def test_build_rejects_workspace_lock_or_document_mutation_outside_the_application_tree(self):
        from argparse import Namespace

        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            repository = directory / "framework"
            repository.mkdir()
            for name in ["Cargo.toml", "Cargo.lock"]:
                (repository / name).write_text("initial", encoding="utf-8")
            for name in ["jadix", "jadix_core", "jadix_components", "jadix_native", "jadix_macros",
                         "jadix_tasks", "jadix_layout", "jadix_style_tokens", "jadix_taffy",
                         "jadix_theme", "jadix_animation", "jadix_document", "jadix_hot_reload", "jadix_wgpu", "jadix_shell"]:
                crate = repository / "crates" / name
                crate.mkdir(parents=True)
                (crate / "Cargo.toml").write_text("fixture", encoding="utf-8")
            project = directory / "application"
            project.mkdir()
            (project / "Cargo.toml").write_text("fixture", encoding="utf-8")

            arguments = Namespace(project=project, bin="fixture", workbench=False, offline=True)
            for index, target in enumerate([repository / "Cargo.lock", repository / "crates/jadix_document/Cargo.toml"]):
                with self.subTest(target=target):
                    def mutate(*arguments):
                        target.write_text("changed", encoding="utf-8")
                        return ""

                    with patch.object(WORKFLOW.native, "ROOT", repository), patch.object(WORKFLOW, "capture", side_effect=mutate):
                        with self.assertRaisesRegex(ValueError, "source changed"):
                            WORKFLOW.build(arguments, directory / f"build-{index}.json")

    def fixture(self, directory):
        executable = directory / "fixture.exe"
        executable.write_bytes(b"identified test fixture, never executed")
        build = {"schema": 1, "operation": "build", "observations": {
            "kind": "windows-native-build", "executable": str(executable),
            "identity": WORKFLOW.native.file_identity(executable), "limits": ["fixture"]}}
        report = directory / "build.json"
        report.write_text(json.dumps(build), encoding="utf-8")
        return executable, report

    def test_retained_build_rejects_changed_executable_before_running_or_packaging(self):
        with tempfile.TemporaryDirectory() as temporary:
            executable, report = self.fixture(Path(temporary))
            self.assertEqual(WORKFLOW.artifact(report)[0], executable)
            executable.write_bytes(b"tampered")
            with self.assertRaisesRegex(ValueError, "identity mismatch"):
                WORKFLOW.artifact(report)

    def test_packaged_payload_and_build_report_are_both_verified(self):
        from argparse import Namespace

        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            _, report = self.fixture(directory)
            result = WORKFLOW.package(Namespace(artifact=report), directory / "package.json")
            self.assertTrue(result["integrity_verified"])
            self.assertFalse(result["authenticity_verified"])
            package = Path(result["package"])
            with zipfile.ZipFile(package) as source:
                payload = {name: source.read(name) for name in source.namelist()}
            for selected in ["fixture.exe", "build.json"]:
                altered = directory / f"altered-{selected}.zip"
                with zipfile.ZipFile(altered, "x") as target:
                    for name, value in payload.items():
                        target.writestr(name, b"changed" if name == selected else value)
                with self.assertRaisesRegex(ValueError, "identity mismatch"):
                    WORKFLOW.verify_package(altered)

    def test_android_package_is_portable_and_binds_payloads_to_retained_build(self):
        from argparse import Namespace
        import hashlib

        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            self.android_bundle(directory)
            observation = WORKFLOW.android_artifact(directory)
            report = directory / "retained.json"
            report.write_text(json.dumps({"schema": 1, "operation": "build", "observations": observation}), encoding="utf-8")
            result = WORKFLOW.package(Namespace(artifact=report), directory / "package.json")
            self.assertEqual(result["kind"], "android-native-package")
            self.assertFalse(result["signed"])
            self.assertFalse(result["authenticity_verified"])
            with zipfile.ZipFile(result["package"]) as source:
                payloads = {name: source.read(name) for name in source.namelist()}
            for selected in ["native-controls.apk", "builder.json", "build.json"]:
                for rewrite_manifest in [False, True]:
                    with self.subTest(member=selected, rewritten=rewrite_manifest):
                        changed = dict(payloads)
                        changed[selected] = b"changed"
                        if rewrite_manifest:
                            manifest = json.loads(changed["manifest.json"])
                            identity = {"bytes": 7, "sha256": hashlib.sha256(b"changed").hexdigest()}
                            if selected == "build.json":
                                manifest["build_report"] = identity
                            else:
                                manifest["files"][selected] = identity
                            changed["manifest.json"] = json.dumps(manifest).encode()
                        altered = directory / f"altered-{selected}-{rewrite_manifest}.zip"
                        with zipfile.ZipFile(altered, "x") as target:
                            for name, value in changed.items():
                                target.writestr(name, value)
                        with self.assertRaises(ValueError):
                            WORKFLOW.verify_package(altered)
            report.unlink()
            (directory / "native-controls.apk").unlink()
            (directory / "build.json").unlink()
            self.assertTrue(WORKFLOW.verify_package(Path(result["package"]))["integrity_verified"])

    def test_ohos_packages_preserve_native_only_and_hap_targets(self):
        from argparse import Namespace

        for architecture, target, abi, machine in [
            ("arm64", "aarch64-unknown-linux-ohos", "arm64-v8a", 183),
            ("x86_64", "x86_64-unknown-linux-ohos", "x86_64", 62),
        ]:
            for native_only in [False, True]:
                with self.subTest(architecture=architecture, native_only=native_only), tempfile.TemporaryDirectory() as temporary:
                    directory = Path(temporary)
                    library = directory / "libjadix_native_controls.so"
                    library.write_bytes(elf_fixture(machine))
                    build = {"schema": 1, "target": target, "abi": abi,
                             "source_unchanged": True, "native_built": True, "hap_built": not native_only,
                             "signed": False, "runtime_acceptance": False, "sdk": {"metadata": {"apiVersion": "22"}},
                             "library": WORKFLOW.native.file_identity(library), "hap": None,
                             "commands": [{"exit_code": 0}]}
                    if not native_only:
                        hap = directory / f"jadix-native-controls-{architecture}-unsigned.hap"
                        member = f"libs/{abi}/libjadix_native_controls.so"
                        with zipfile.ZipFile(hap, "x") as archive:
                            archive.write(library, member)
                        build["hap"] = WORKFLOW.native.file_identity(hap)
                        build["packaged_library"] = {"member": member, **WORKFLOW.native.file_identity(library)}
                    (directory / "build.json").write_text(json.dumps(build), encoding="utf-8")
                    observation = WORKFLOW.ohos_artifact(directory, architecture, native_only)
                    report = directory / "retained.json"
                    report.write_text(json.dumps({"schema": 1, "operation": "build", "observations": observation}), encoding="utf-8")
                    result = WORKFLOW.package(Namespace(artifact=report), directory / "package.json")
                    self.assertEqual(result["kind"], "ohos-native-package")
                    self.assertEqual(len(result["files"]), 2 if native_only else 3)
                    self.assertFalse(result["signed"])
                    self.assertTrue(WORKFLOW.verify_package(Path(result["package"]))["integrity_verified"])

    def test_zip_rejects_traversal_and_duplicate_payloads(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            for index, names in enumerate([["../outside"], ["same", "same"], ["C:/absolute"]]):
                package = directory / f"invalid-{index}.zip"
                with zipfile.ZipFile(package, "x") as archive:
                    for name in names:
                        archive.writestr(name, "invalid")
                with self.assertRaises(ValueError):
                    WORKFLOW.verify_package(package)

    def test_create_refuses_existing_project_without_changing_it(self):
        with tempfile.TemporaryDirectory() as temporary:
            project = Path(temporary) / "project"
            project.mkdir()
            marker = project / "keep.txt"
            marker.write_text("preserved", encoding="utf-8")
            result = subprocess.run([sys.executable, str(SCRIPTS / "native.py"), "create", str(project),
                                     "--output", str(Path(temporary) / "create.json")],
                                    capture_output=True, text=True, encoding="utf-8")
            self.assertEqual(result.returncode, 1)
            self.assertEqual(marker.read_text(encoding="utf-8"), "preserved")


if __name__ == "__main__":
    unittest.main()
