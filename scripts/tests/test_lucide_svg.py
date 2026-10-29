import importlib.util
import json
from pathlib import Path
import unittest
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("lucide_svg", ROOT / "scripts/assets/export_lucide_svg.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class LucideSvgTests(unittest.TestCase):
    def test_every_frozen_path_has_an_exact_standalone_document(self):
        package = ROOT / "crates/jadix_lucide"
        expected = MODULE.documents(package)
        directory = package / "assets/svg"
        self.assertEqual({path.name for path in directory.glob("*.svg")}, set(expected))
        for name, document in expected.items():
            with self.subTest(icon=name):
                self.assertEqual((directory / name).read_text(encoding="utf-8"), document)
                root = ET.fromstring(document)
                self.assertEqual(root.tag, "{http://www.w3.org/2000/svg}svg")
                self.assertEqual(root.attrib["fill"], "none")
                self.assertEqual(root.attrib["stroke"], "currentColor")
                self.assertEqual(root.attrib["viewBox"], "0 0 24 24")
                self.assertGreater(len(root), 0)


if __name__ == "__main__":
    unittest.main()
