import importlib.util
import hashlib
import json
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest


SCRIPT = Path(__file__).parents[1] / "acceptance" / "native-android-check.py"
SPEC = importlib.util.spec_from_file_location("native_android_check", SCRIPT)
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


class NativeAndroidCheckTests(unittest.TestCase):
    def test_extended_acceptance_requires_explicit_ime_proof(self):
        marker = "JADIX_NATIVE_CHECKS_PASS: extensions=true; switch_checked=true; retained_editing_checked=true; retained_editors=3;"
        self.assertFalse(CHECK.instrumentation_passed(marker, True))
        self.assertFalse(CHECK.instrumentation_passed(marker + "; dialog_ime_checked=false", True))
        self.assertTrue(CHECK.instrumentation_passed(marker + "; dialog_ime_checked=true", True))
        self.assertFalse(CHECK.instrumentation_passed(marker + "; dialog_ime_checked=true; JADIX_NATIVE_CHECKS_FAIL", True))
        self.assertTrue(CHECK.instrumentation_passed("JADIX_NATIVE_CHECKS_PASS: extensions=false; retained_editing_checked=true; retained_editors=1;", False))

    def test_acceptance_requires_current_retained_editing_proof(self):
        marker = "JADIX_NATIVE_CHECKS_PASS: extensions=true; switch_checked=true; dialog_ime_checked=true;"
        self.assertFalse(CHECK.instrumentation_passed(marker, True))
        self.assertFalse(CHECK.instrumentation_passed(marker + " retained_editing_checked=false; retained_editors=3;", True))
        self.assertFalse(CHECK.instrumentation_passed(marker + " retained_editing_checked=true; retained_editors=1;", True))
        self.assertTrue(CHECK.instrumentation_passed(marker + " retained_editing_checked=true; retained_editors=3;", True))

    def test_extended_acceptance_rejects_receipts_without_switch_proof(self):
        marker = "JADIX_NATIVE_CHECKS_PASS: extensions=true; dialog_ime_checked=true; retained_editing_checked=true; retained_editors=3;"
        self.assertFalse(CHECK.instrumentation_passed(marker, True))
        self.assertFalse(CHECK.instrumentation_passed(marker + " switch_checked=false;", True))
        self.assertTrue(CHECK.instrumentation_passed(marker + " switch_checked=true;", True))

    def test_proof_must_belong_to_one_complete_receipt(self):
        marker = "JADIX_NATIVE_CHECKS_PASS: extensions=true; dialog_ime_checked=true; retained_editing_checked=true; retained_editors=3;"
        self.assertFalse(CHECK.instrumentation_passed(marker + "\nother output: switch_checked=true;", True))
        self.assertFalse(CHECK.instrumentation_passed(marker + " switch_checked=true;\n" + marker, True))
        self.assertFalse(CHECK.instrumentation_passed(marker + " switch_checked=true; switch_checked=false;", True))
        self.assertTrue(CHECK.instrumentation_passed("INSTRUMENTATION_RESULT: stream=" + marker + " switch_checked=true;\nINSTRUMENTATION_CODE: -1", True))

    def test_receipt_values_are_exact(self):
        marker = "JADIX_NATIVE_CHECKS_PASS: extensions=true; dialog_ime_checked=true; retained_editing_checked=true; retained_editors=3; switch_checked=true;"
        for source, replacement in [("extensions=true", "extensions=trueish"), ("retained_editors=3", "retained_editors=30"), ("switch_checked=true", "switch_checked=trueish")]:
            self.assertFalse(CHECK.instrumentation_passed(marker.replace(source, replacement), True))

    def test_editor_without_hint_uses_exact_placeholder_then_exact_value(self):
        self.assertTrue(CHECK.name_editor({"class": "android.widget.EditText", "text": "Name / 中文"}))
        self.assertTrue(CHECK.name_editor({"class": "android.widget.EditText", "text": "restored"}, "restored"))
        for node in [{"class": "android.widget.TextView", "text": "restored"}, {"class": "android.widget.EditText", "text": "stale"}, {"class": "android.widget.EditText", "text": "restored", "hint": "different"}]:
            self.assertFalse(CHECK.name_editor(node, "restored"))


    def keyboard_fixture(self, token="fresh-token", rows=100):
        labels = ["Accept", "Airplane mode", "Password / 密码", "Multiline / 多行", "Open editor", "Continue", "Name / 中文", "Reset"]
        phases = [{"name": name, "forward": [{"control": label, "fully_visible": True, "bounds": [0, 0, 100, 44]} for label in labels], "backward": [{"control": label, "fully_visible": True, "bounds": [0, 0, 100, 44]} for label in labels[::-1]]} for name in ("initial", "compact-viewport", "after-25-remounts")]
        return {"schema": 1, "token": token, "passed": True, "retained_rows": rows, "controls": 8, "key_events": 84, "initial_focus_key_events": 6, "remounts": 25, "event_authority": "Instrumentation-injected Tab/Shift+Tab; programmatic initial focus, viewport and remounts; soft keyboard suppressed", "phases": phases}

    def keyboard_log(self, receipt):
        return "INSTRUMENTATION_RESULT: jadix_keyboard=" + json.dumps(receipt) + chr(10) + "INSTRUMENTATION_RESULT: stream=JADIX_NATIVE_KEYBOARD_PASS" + chr(10) + "INSTRUMENTATION_CODE: -1"

    def test_keyboard_receipt_accepts_both_row_counts_and_all_phases(self):
        for rows in (100, 1000):
            value = self.keyboard_fixture(rows=rows)
            self.assertEqual(CHECK.keyboard_receipt(self.keyboard_log(value), "fresh-token", rows), value)

    def test_keyboard_receipt_rejects_missing_repeated_failed_or_stale_proof(self):
        valid = self.keyboard_log(self.keyboard_fixture())
        for log in ("JADIX_NATIVE_KEYBOARD_PASS", valid + chr(10) + valid, valid.replace("PASS", "FAIL"), valid.replace("PASS", "PASSX"), valid.replace("-1", "-10"), valid.replace("-1", "0"), valid.replace("fresh-token", "stale-token")):
            with self.subTest(log=log), self.assertRaises(ValueError):
                CHECK.keyboard_receipt(log, "fresh-token", 100)

    def test_keyboard_receipt_rejects_partial_order_counts_geometry_and_authority(self):
        for mutation in ("schema", "rows", "controls", "key-events", "remounts", "authority", "phase-count", "phase-order", "control-count", "control-order", "visibility", "bounds", "boolean-bounds", "negative-size"):
            value = self.keyboard_fixture()
            if mutation == "schema": value["schema"] = True
            elif mutation == "rows": value["retained_rows"] = 1000
            elif mutation == "controls": value["controls"] = 7
            elif mutation == "key-events": value["key_events"] = 83
            elif mutation == "remounts": value["remounts"] = 24
            elif mutation == "authority": value["event_authority"] = "physical keyboard"
            elif mutation == "phase-count": value["phases"].pop()
            elif mutation == "phase-order": value["phases"].reverse()
            elif mutation == "control-count": value["phases"][1]["backward"].pop()
            elif mutation == "control-order": value["phases"][1]["forward"].reverse()
            elif mutation == "visibility": value["phases"][2]["forward"][0]["fully_visible"] = False
            elif mutation == "bounds": value["phases"][2]["forward"][0]["bounds"] = [0, 0, 10]
            elif mutation == "boolean-bounds": value["phases"][2]["forward"][0]["bounds"][0] = True
            elif mutation == "negative-size": value["phases"][2]["forward"][0]["bounds"][2] = -1
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                CHECK.keyboard_receipt(self.keyboard_log(value), "fresh-token", 100)


    def editing_fixture(self, token="fresh-token", rows=100):
        checks = ["unicode-commit", "copy", "cut", "paste", "undo", "redo", "history-after-republish-reorder", "controlled-reset-clears-history"]
        return {"schema": 1, "token": token, "passed": True, "retained_rows": rows, "key_events": 42, "editors": [{"control": label, "checks": checks.copy(), "native_identity_retained": True, "business_roundtrip": True} for label in ("Name / 中文", "Multiline / 多行")], "password_checks": ["unicode-paste", "copy-blocked", "cut-blocked", "public-state-isolation"], "clipboard_restored": True, "event_authority": "Instrumentation-injected clipboard/undo shortcuts; programmatic InputConnection seed, selection, controlled reset and sibling perturbation; soft keyboard suppressed"}

    def editing_log(self, receipt):
        return "INSTRUMENTATION_RESULT: jadix_editing=" + json.dumps(receipt) + chr(10) + "INSTRUMENTATION_RESULT: stream=JADIX_NATIVE_EDITING_PASS" + chr(10) + "INSTRUMENTATION_CODE: -1"

    def test_editing_receipt_accepts_both_row_counts(self):
        for rows in (100, 1000):
            value = self.editing_fixture(rows=rows)
            self.assertEqual(CHECK.editing_receipt(self.editing_log(value), "fresh-token", rows), value)

    def test_editing_receipt_rejects_missing_repeated_failed_and_wrong_authority(self):
        valid = self.editing_log(self.editing_fixture())
        for log in ("JADIX_NATIVE_EDITING_PASS", valid + chr(10) + valid, valid.replace("PASS", "PASSX"), valid.replace("PASS", "FAIL"), valid.replace("-1", "-10"), valid.replace("fresh-token", "stale"), self.editing_log([])):
            with self.subTest(log=log), self.assertRaises(ValueError):
                CHECK.editing_receipt(log, "fresh-token", 100)

    def test_editing_receipt_rejects_partial_history_privacy_and_clipboard_proof(self):
        for mutation in ("schema", "keys", "rows", "editors", "order", "missing-check", "duplicate-check", "identity", "business", "password", "clipboard", "authority"):
            value = self.editing_fixture()
            if mutation == "schema": value["schema"] = True
            elif mutation == "keys": value["key_events"] = 40
            elif mutation == "rows": value["retained_rows"] = 1000
            elif mutation == "editors": value["editors"].pop()
            elif mutation == "order": value["editors"].reverse()
            elif mutation == "missing-check": value["editors"][0]["checks"].pop()
            elif mutation == "duplicate-check": value["editors"][0]["checks"][1] = "cut"
            elif mutation == "identity": value["editors"][0]["native_identity_retained"] = False
            elif mutation == "business": value["editors"][1]["business_roundtrip"] = False
            elif mutation == "password": value["password_checks"].pop()
            elif mutation == "clipboard": value["clipboard_restored"] = False
            elif mutation == "authority": value["event_authority"] = "physical keyboard"
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                CHECK.editing_receipt(self.editing_log(value), "fresh-token", 100)


    def test_owned_install_refuses_registered_and_retained_packages(self):
        for retained in (False, True):
            commands = []
            def run(command):
                commands.append(command)
                return SimpleNamespace(stdout="package:android\npackage:dev.jadix.nativeui\n")
            with self.subTest(retained=retained), self.assertRaisesRegex(RuntimeError, "already registered"):
                with CHECK.owned_install(run, ["adb"], Path("sample.apk"), {}, retained):
                    self.fail("registered package was used")
            self.assertEqual(commands, [["adb", "shell", "pm", "list", "packages", "-u"]])

    def test_owned_install_requires_valid_full_registry(self):
        for output in ("", "error: offline\n", "package:android\npackage:android\n"):
            commands = []
            def run(command):
                commands.append(command)
                return SimpleNamespace(stdout=output)
            with self.subTest(output=output), self.assertRaisesRegex(RuntimeError, "registry"):
                with CHECK.owned_install(run, ["adb"], Path("sample.apk"), {}):
                    self.fail("invalid registry was accepted")
            self.assertEqual(len(commands), 1)

    def test_failed_install_never_grants_uninstall_ownership(self):
        for output in ("Failure [INSTALL_FAILED_UPDATE_INCOMPATIBLE]", "", "Success\nSuccess"):
            commands = []
            def run(command):
                commands.append(command)
                return SimpleNamespace(stdout="package:android\n" if command[1] == "shell" else output)
            report = {}
            with self.subTest(output=output), self.assertRaisesRegex(RuntimeError, "Success"):
                with CHECK.owned_install(run, ["adb"], Path("sample.apk"), report):
                    self.fail("failed install was accepted")
            self.assertFalse(report["cleanup"]["installed_by_this_run"])
            self.assertFalse(any("uninstall" in command or "-r" in command for command in commands))

    def test_owned_success_and_probe_failure_restore_registry(self):
        for fail_probe in (False, True):
            commands = []
            def run(command):
                commands.append(command)
                return SimpleNamespace(stdout="package:android\n" if command[1] == "shell" else "Success\n")
            report = {}
            def exercise():
                with CHECK.owned_install(run, ["adb"], Path("sample.apk"), report, keep_installed=fail_probe):
                    if fail_probe:
                        raise RuntimeError("probe failure")
            if fail_probe:
                with self.assertRaisesRegex(RuntimeError, "probe failure"):
                    exercise()
            else:
                exercise()
            self.assertTrue(report["cleanup"]["package_registry_restored"])
            self.assertTrue(report["cleanup"]["completed"])
            self.assertFalse(report["cleanup"]["kept_installed"])
            self.assertEqual(sum("uninstall" in command for command in commands), 1)

    def test_install_command_failure_never_uninstalls(self):
        commands = []
        def run(command):
            commands.append(command)
            if command[1] == "install":
                raise RuntimeError("install command failed")
            return SimpleNamespace(stdout="package:android\n")
        report = {}
        with self.assertRaisesRegex(RuntimeError, "install command failed"):
            with CHECK.owned_install(run, ["adb"], Path("sample.apk"), report):
                self.fail("install command failure was accepted")
        self.assertFalse(report["cleanup"]["installed_by_this_run"])
        self.assertFalse(any("uninstall" in command for command in commands))

    def test_ui_dump_uses_fresh_owned_path_and_cleans_on_pull_failure(self):
        paths = []
        for fail_pull in (False, True):
            commands = []
            def run(command):
                commands.append(command)
                if command[1] == "pull" and fail_pull:
                    raise RuntimeError("pull failed")
                return SimpleNamespace(stdout="UI hierchary dumped to: " + command[-1] if "uiautomator" in command else "", stderr="")
            if fail_pull:
                with self.assertRaisesRegex(RuntimeError, "pull failed"):
                    CHECK.dump_tree(run, ["adb"], Path("tree.xml"))
            else:
                CHECK.dump_tree(run, ["adb"], Path("tree.xml"))
            remote = commands[0][-1]
            self.assertRegex(remote, r"^/data/local/tmp/jadix-native-[0-9a-f-]{36}\.xml$")
            self.assertEqual(commands[-1], ["adb", "shell", "rm", "-f", remote])
            paths.append(remote)
        self.assertNotEqual(paths[0], paths[1])

    def test_ui_dump_retries_only_explicit_transient_null_root(self):
        for failures, diagnostic in ((1, "ERROR: null root node returned by UiTestAutomationBridge."),
                                     (3, "ERROR: null root node returned by UiTestAutomationBridge."),
                                     (1, "unconfirmed output")):
            commands = []
            attempts = 0
            def run(command):
                nonlocal attempts
                commands.append(command)
                if "uiautomator" in command:
                    attempts += 1
                    if attempts <= failures:
                        return SimpleNamespace(stdout="", stderr=diagnostic)
                    return SimpleNamespace(stdout="UI hierarchy dumped to: " + command[-1], stderr="")
                return SimpleNamespace(stdout="", stderr="")
            if failures == 1 and diagnostic.startswith("ERROR:"):
                CHECK.dump_tree(run, ["adb"], Path("tree.xml"))
                self.assertEqual(attempts, 2)
                self.assertTrue(any("pull" in command for command in commands))
            else:
                with self.assertRaisesRegex(RuntimeError, "not confirmed"):
                    CHECK.dump_tree(run, ["adb"], Path("tree.xml"))
                self.assertEqual(attempts, 3 if failures == 3 else 1)
                self.assertFalse(any("pull" in command for command in commands))
            self.assertEqual(commands[-1][1:4], ["shell", "rm", "-f"])

    def test_keep_installed_requires_success_and_exact_registry(self):
        commands = []
        queries = 0
        def run(command):
            nonlocal queries
            commands.append(command)
            if command[1] == "shell":
                queries += 1
                return SimpleNamespace(stdout="package:android\n" + ("package:dev.jadix.nativeui\n" if queries > 1 else ""))
            return SimpleNamespace(stdout="Success\n")
        report = {}
        with CHECK.owned_install(run, ["adb"], Path("sample.apk"), report, keep_installed=True):
            pass
        self.assertTrue(report["cleanup"]["kept_installed"])
        self.assertFalse(report["cleanup"]["package_registry_restored"])
        self.assertFalse(any("uninstall" in command for command in commands))

    def test_cleanup_failure_rejects_success(self):
        for failure in ("uninstall", "registry"):
            commands = []
            queries = 0
            def run(command):
                nonlocal queries
                commands.append(command)
                if command[1] == "shell":
                    queries += 1
                    return SimpleNamespace(stdout="package:android\n" + ("package:unexpected\n" if queries > 1 else ""))
                return SimpleNamespace(stdout="Failure" if command[1] == failure else "Success\n")
            report = {}
            with self.subTest(failure=failure), self.assertRaisesRegex(RuntimeError, "uninstall|baseline"):
                with CHECK.owned_install(run, ["adb"], Path("sample.apk"), report):
                    pass
            self.assertFalse(report["cleanup"]["completed"])
            self.assertIn("error", report["cleanup"])

    def test_artifact_must_match_controls_scenario_before_device_mutation(self):
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            apk = directory / "sample.apk"
            apk.write_bytes(b"fixture")
            expected = {"application": "controls", "package_name": "dev.jadix.nativeui", "extended": True,
                        "retained_rows": 100, "apk_bytes": 7, "apk_sha256": hashlib.sha256(b"fixture").hexdigest()}
            for field, value in [(None, None), ("application", "selection"), ("package_name", "user.app"),
                                 ("extended", False), ("retained_rows", 1000), ("retained_rows", True),
                                 ("apk_bytes", 8), ("apk_sha256", "changed")]:
                build = dict(expected)
                if field is not None:
                    build[field] = value
                (directory / "build.json").write_text(json.dumps(build), encoding="utf8")
                with self.subTest(field=field, value=value):
                    if field is None:
                        self.assertEqual(CHECK.artifact(apk, True, 100)["apk_sha256"], expected["apk_sha256"])
                    else:
                        with self.assertRaises(ValueError):
                            CHECK.artifact(apk, True, 100)


if __name__ == "__main__":
    unittest.main()
