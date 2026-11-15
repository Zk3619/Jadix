"""The color fixture oracle compares cell geometry, including omitted blank cells."""
import importlib.util
from pathlib import Path
import unittest

SOURCE=Path(__file__).resolve().parents[1]/"acceptance/terminal-colors-check.py"
SPEC=importlib.util.spec_from_file_location("terminal_colors",SOURCE)
MODULE=importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

class ColorFixtureTests(unittest.TestCase):
    def test_sparse_spaces_wide_cells_and_unknown_commands(self):
        dense="\x1b[1;2H\x1b[38;2;1;2;3mA 中 B".encode()
        sparse="\x1b[1;2HA\x1b[1;4H中\x1b[1;7HB".encode()
        screen=MODULE.fixture_screen(dense)
        self.assertEqual(screen,MODULE.fixture_screen(sparse))
        self.assertTrue(screen[0].startswith(" A 中 B"))
        for invalid in [b"\x1b[2J",b"\x1b[99;1HA",b"\x1b[1;1H\n"]:
            with self.assertRaises(AssertionError):
                MODULE.fixture_screen(invalid)

if __name__=="__main__":
    unittest.main()
