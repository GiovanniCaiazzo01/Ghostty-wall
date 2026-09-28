"""Standalone CLI presentation at the public CLI/PTY seam; isolated HOME/XDG only.

Run: cargo build --locked && python3 -m unittest discover -s tests -p cli_choices_pty.py -v
"""

import fcntl
import json
import shutil
import struct
import subprocess
import termios
import tempfile
import unittest
from pathlib import Path

from qa_create_pty import InlineSession
from tui_pty import BINARY, ROOT


class CliChoices(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="gw-choices-")
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name)
        self.env = {
            "HOME": str(self.home),
            "XDG_CONFIG_HOME": str(self.home / "config"),
            "XDG_RUNTIME_DIR": str(self.home / "runtime"),
            "PATH": "/nonexistent",
            "TERM": "xterm-256color",
        }
        self.cli("init")

    def cli(self, *args, input=""):
        result = subprocess.run(
            [str(BINARY), *args], env=self.env, input=input,
            capture_output=True, text=True, timeout=30,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stderr, "")
        return result.stdout

    def test_piped_creation_choices_retry_and_cancel_are_readable(self):
        output = self.cli("create", "readable", input="wrong\ncancel\n")
        self.assertIn("\nWallpaper:\n\n  [g] Generate wallpaper\n  [i] Image from Downloads/Pictures\n  cancel  Discard draft\n\n", output)
        self.assertIn("Choice (g/i/cancel; no default): ", output)
        self.assertIn("Error: Choose g or i; nothing saved.\n", output)
        self.assertEqual(output.count("Choice (g/i/cancel; no default): "), 2)
        self.assertIn("Cancelled; no Profile saved.", output)
        self.assertNotIn("\x1b", output)
        self.assertNotIn("readable", self.cli("list"))

    def test_terminal_choices_show_save_and_safe_default_with_optional_color(self):
        for name, overrides, colored in [
            ("styled", {}, True),
            ("no-color", {"NO_COLOR": ""}, False),
            ("dumb", {"TERM": "dumb"}, False),
        ]:
            with self.subTest(name=name):
                session = InlineSession({**self.env, **overrides}, "create", name)
                self.addCleanup(session.close)
                session.wait_for("Choice (g/i/cancel; no default):")
                session.send("wrong\n")
                session.wait_for("Choice (g/i/cancel; no default):", 2)
                session.send("g\n")
                session.wait_for("Choice (s/a/cancel; no default):")
                session.send("s\n")
                session.wait_for("Choice (y/n; default: n):")
                session.send("\n")
                session.finish()
                output = bytes(session.output)
                self.assertIn(b"[s]ave", output)
                self.assertIn(b"[a]nother generated variant", output)
                self.assertIn(b"[n] Not now (default)", output)
                self.assertIn(f"Saved Profile {name}; terminal unchanged.".encode(), output)
                self.assertEqual(b"\x1b[" in output, colored)
                if colored:
                    self.assertIn(b"\x1b[38;5;14m", output)  # cyan headings
                    self.assertIn(b"\x1b[38;5;12m", output)  # blue choices
                    self.assertIn(b"\x1b[38;5;10m", output)  # green success
                    self.assertIn(b"\x1b[38;5;11m", output)  # yellow warning
                    self.assertIn(b"\x1b[38;5;9m", output)  # red retry error

    def test_narrow_image_choices_keep_long_names_distinguishable_and_retry(self):
        downloads = self.home / "Downloads"
        downloads.mkdir()
        for suffix in ("one", "two"):
            shutil.copyfile(ROOT / "tests/fixtures/white.png", downloads / ("long-name-" * 6 + suffix + ".png"))
        session = InlineSession({**self.env, "NO_COLOR": "1"}, "create", "narrow")
        self.addCleanup(session.close)
        session.wait_for("Choice (g/i/cancel; no default):")
        fcntl.ioctl(session.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 36, 0, 0))
        session.send("i\n")
        session.wait_for("Number/relative path")
        menu = bytes(session.output).decode().split("Images in", 1)[1]
        self.assertIn("  1: ", menu)
        self.assertIn("  2: ", menu)
        unwrapped = menu.replace("\r\n    ", "")
        self.assertIn("long-name-" * 6 + "one.png", unwrapped)
        self.assertIn("long-name-" * 6 + "two.png", unwrapped)
        self.assertTrue(all(len(line) <= 36 for line in menu.splitlines()), menu)
        session.send("missing.png\n")
        session.wait_for("Error: Cannot use")
        session.wait_for("Number/relative path", 2)
        session.send("cancel\n")
        session.finish()
        self.assertNotIn(b"\x1b", session.output)
        self.assertNotIn("narrow", self.cli("list"))

    def test_delete_selection_retry_and_confirmation_keep_default_cancel(self):
        self.cli("new", "delete-me", str(ROOT / "tests/fixtures/white.png"))
        output = self.cli("delete", input="unknown\ndelete-me\n\n")
        self.assertIn("\nSelect Profile to delete", output)
        self.assertIn("Error: Choose a listed Profile, or cancel; no files changed.", output)
        self.assertIn("\nDelete Profile delete-me?\n", output)
        self.assertIn("\n  [y] Delete\n  [n] Cancel (default)\n", output)
        self.assertIn("Choice (y/n; default: n): ", output)
        self.assertIn("Deletion cancelled; no files changed.", output)
        self.assertIn("delete-me", self.cli("list"))
        self.assertNotIn("\x1b", output)

    def test_line_mode_source_edit_and_maintenance_prompts_keep_cancellation(self):
        output = self.cli("tui", input="o\nsource-id\nb\ntab\nt\nb\nU\n\nX\n\np\n\nM\n\nq\n")
        self.assertIn("\nSource kind:\n\n  local  Directory\n  github  GitHub repository\n", output)
        self.assertIn("\nSource kind (local/github): ", output)
        self.assertIn("\nField: ", output)
        self.assertIn("Warning: Only explicit confirmation performs this action.", output)
        self.assertIn("Enter or b: cancel (default)", output)
        self.assertIn("\nType update to install release (b to cancel): ", output)
        self.assertIn("\nType uninstall to remove integration (Intent and History stay): ", output)
        self.assertIn("\nType previous to replay prior Environment (b to cancel): ", output)
        self.assertIn("\nType migrate to import legacy configuration: ", output)
        self.assertNotIn("\x1b", output)
        self.assertIn("welcome", self.cli("list"))

    def test_json_and_fatal_error_routing_stay_unstyled(self):
        output = self.cli("plan", "welcome", "--json")
        self.assertIn("environment", json.loads(output))
        self.assertNotIn("\x1b", output)
        session = InlineSession(self.env, "plan", "welcome", "--json")
        self.addCleanup(session.close)
        session.wait_for('"environment"')
        session.finish()
        self.assertIn("environment", json.loads(bytes(session.output)))
        self.assertNotIn(b"\x1b", session.output)
        error = subprocess.run(
            [str(BINARY), "create", "Bad ID"], env=self.env,
            input="", capture_output=True, text=True, timeout=30,
        )
        self.assertEqual(error.returncode, 2)
        self.assertEqual(error.stdout, "")
        self.assertIn("ghostty-wall: Cannot create Profile", error.stderr)
        self.assertNotIn("\x1b", error.stderr)
