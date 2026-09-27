"""Real-terminal TUI checks. Run: cargo build && python3 -m unittest discover -s tests -p tui_pty.py -v

Set GHOSTTY_WALL_BIN to inspect a different binary. Requires Unix PTYs; never touches real HOME.
"""

import fcntl
import os
import pty
import re
import select
import shutil
import struct
import subprocess
import tempfile
import termios
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BINARY = Path(os.environ.get("GHOSTTY_WALL_BIN", ROOT / "target/debug/ghostty-wall")).resolve()
ALT_ENTER = b"\x1b[?1049h"
ALT_LEAVE = b"\x1b[?1049l"


class Session:
    def __init__(self, binary, env, *args):
        self.master, self.slave = pty.openpty()
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
        self.proc = subprocess.Popen(
            [str(binary), *args], stdin=self.slave, stdout=self.slave, stderr=self.slave, env=env
        )
        self.data = bytearray()

    def wait(self, needle, occurrences=1, timeout=45):
        deadline = time.monotonic() + timeout
        while self.data.count(needle) < occurrences and time.monotonic() < deadline:
            if select.select([self.master], [], [], 0.1)[0]:
                try:
                    self.data.extend(os.read(self.master, 65536))
                except OSError:
                    break
        if self.data.count(needle) < occurrences:
            raise AssertionError(f"TUI did not emit {needle!r}; final output: {bytes(self.data[-600:])!r}")

    def wait_screen(self, needle, timeout=45):
        # Wallpaper backgrounds split even one word into styled diff fragments.
        # Reuse the editor PTY's cursor/erase replay, not raw-byte substring matching.
        from edit_pty import EditSession
        deadline = time.monotonic() + timeout
        while needle not in b" ".join(EditSession.screen(self).split()) and time.monotonic() < deadline:
            if select.select([self.master], [], [], 0.1)[0]:
                self.data.extend(os.read(self.master, 65536))
        if needle not in b" ".join(EditSession.screen(self).split()):
            raise AssertionError(f"Missing {needle!r}: {EditSession.screen(self)[-2500:]!r}")

    def send(self, text):
        os.write(self.master, text.encode())

    def finish(self):
        self.proc.wait(timeout=20)
        # Use stays in the management center; consume the final quit frame too,
        # even when an earlier prompt already emitted LeaveAlternateScreen.
        while select.select([self.master], [], [], 0)[0]:
            self.data.extend(os.read(self.master, 65536))
        self.wait(ALT_LEAVE)
        assert self.proc.returncode == 0, f"TUI exit {self.proc.returncode}: {bytes(self.data[-600:])!r}"
        assert self.data.count(ALT_ENTER) == self.data.count(ALT_LEAVE), "alternate screen not restored"
        attrs = termios.tcgetattr(self.slave)
        assert attrs[3] & termios.ICANON and attrs[3] & termios.ECHO, "raw mode not restored"

    def close(self):
        if self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait()
        os.close(self.master)
        os.close(self.slave)


class TuiPty(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.env = dict(os.environ, HOME=self.tmp.name, XDG_CONFIG_HOME=self.tmp.name + "/config", XDG_RUNTIME_DIR=self.tmp.name + "/runtime", PATH="/nonexistent", TERM="xterm-256color", TERM_PROGRAM="tui-test")
        self.env.pop("DBUS_SESSION_BUS_ADDRESS", None)
        self.managed = Path(self.env["XDG_CONFIG_HOME"]) / "ghostty/ghostty-wall"
        self.cli("init")

    def cli(self, *args):
        result = subprocess.run([str(BINARY), *args], env=self.env, capture_output=True, text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def session(self):
        session = Session(BINARY, self.env, "tui")
        self.addCleanup(session.close)
        session.wait(ALT_ENTER)
        session.wait(b"n Create")
        return session

    def test_create_edit_apply_and_delete_keeps_history(self):
        session = self.session()
        session.send("N")  # Retained advanced image flow; guided Create is in management_pty.
        session.wait(b"New Profile ID:")
        session.send("trial\n")
        session.wait(b"PNG/JPEG image path:")
        session.send(str(ROOT / "tests/fixtures/white.png") + "\n")
        session.wait(ALT_ENTER, 2)
        session.send("t")
        session.wait(b"Field:")
        session.send("font_size\n")
        session.wait(b"Value:")
        session.send("19.0\n")
        session.wait(ALT_ENTER, 3)
        session.send("a")
        session.wait_screen(b"Profile applied.")
        session.send("q")
        session.finish()
        profile = self.managed / "profiles/trial.toml"
        self.assertTrue(profile.is_file())
        self.assertIn("font_size_millipoints", self.cli("plan", "trial", "--json"))
        self.assertIn("font-size = 19", (self.managed / "current.ghostty").read_text())

        session = self.session()
        session.wait_screen(b"Cursor")
        session.send("x")
        session.wait(b"Cancel (default)")
        session.send("wrong\n")
        session.wait(ALT_ENTER, 2)
        self.assertTrue(profile.is_file(), "wrong confirmation must not delete")
        session.send("x")
        session.wait(b"Cancel (default)", 2)
        session.send("y\n")
        session.wait(ALT_ENTER, 3)
        session.send("q")
        session.wait(b"Cancelled.")
        session.finish()
        self.assertFalse(profile.exists(), "confirmed active deletion must commit Welcome first")
        self.assertIn(b"Welcome Activation", session.data)
        self.assertEqual(len(list((self.managed / "history/activations").glob("*.json"))), 2)
        self.cli("previous")
        self.assertIn("font-size = 19", (self.managed / "current.ghostty").read_text())
        self.cli("doctor")

    def test_enter_on_source_starts_profile_creation_with_selected_source(self):
        session = self.session()
        session.send("\t\r")  # Profiles are focused initially; Tab opens Sources.
        session.wait(b"New Profile ID:")
        session.send("from-source\n")
        session.wait(b"Candidate path relative to Source root:")
        session.send("welcome.png\n")
        session.wait(ALT_ENTER, 2)
        session.send("q")
        session.wait(b"Cancelled.")
        session.finish()
        plan = self.cli("plan", "from-source", "--json")
        self.assertIn('"id":"welcome"', plan)
        self.assertFalse(list((self.managed / "history/activations").glob("*.json")))

    def test_piped_enter_on_source_uses_same_creation_flow(self):
        result = subprocess.run(
            [str(BINARY), "tui"], env=self.env, input="enter\npiped-source\nwelcome.png\nq\n",
            capture_output=True, text=True, timeout=20,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("Source: welcome", result.stdout)
        self.assertIn('"id":"welcome"', self.cli("plan", "piped-source", "--json"))

    def test_add_source_and_profile_from_candidate(self):
        directory = Path(self.tmp.name) / "pictures"
        directory.mkdir()
        shutil.copyfile(ROOT / "tests/fixtures/white.png", directory / "sky.png")
        session = self.session()
        session.send("o")
        session.wait(b"New Source ID:")
        session.send("photos\n")
        session.wait(b"Source kind")
        session.send("local\n")
        session.wait(b"Directory path or owner/repo")
        session.send(str(directory) + "\n")
        session.wait(ALT_ENTER, 2)
        session.send("\r")
        session.wait(b"New Profile ID:")
        session.send("sky\n")
        session.wait(b"Source: photos")
        session.wait(b"Candidate path")
        session.send("sky.png\n")
        session.wait(ALT_ENTER, 3)
        session.send("a")
        session.wait_screen(b"Profile applied.")
        session.send("q")
        session.finish()
        plan = self.cli("plan", "sky", "--json")
        self.assertIn('"id":"photos"', plan)
        self.assertIn('"candidate":"sky.png"', plan)

    def test_rename_duplicate_and_edit_wallpaper_and_color(self):
        self.cli("new", "night", str(ROOT / "tests/fixtures/white.png"))
        session = self.session()
        session.wait_screen(b"Cursor")
        session.send("r")
        session.wait(b"New Profile ID:")
        session.send("dusk\n")
        session.wait(ALT_ENTER, 2)
        session.send("d")
        session.wait(b"New Profile ID:", 2)
        session.send("copy\n")
        session.wait(ALT_ENTER, 3)
        session.send("w")
        session.wait(b"Wallpaper fields:")
        session.send("opacity\n")
        session.wait(b"Value:")
        session.send("0.2\n")
        session.wait(ALT_ENTER, 4)
        session.send("c")
        session.wait(b"Color fields:")
        session.send("background\n")
        session.wait(b"Value:", 2)
        session.send("101010\n")
        session.wait(ALT_ENTER, 5)
        session.send("q")
        session.wait(b"Cancelled.")
        session.finish()
        self.assertFalse((self.managed / "profiles/night.toml").exists())
        self.assertTrue((self.managed / "profiles/dusk.toml").is_file())
        plan = self.cli("plan", "copy", "--json")
        self.assertIn('"background":"101010"', plan)
        self.assertIn('"opacity_millionths":200000', plan)
        self.assertFalse(list((self.managed / "history/activations").glob("*.json")))

    def test_uninstall_and_repair_preserve_history(self):
        self.cli("apply", "welcome")
        session = self.session()
        session.send("?X")
        session.wait(b"Type uninstall to remove integration")
        session.send("uninstall\n")
        session.wait(b"Press Enter to return")
        session.send("\n")
        session.wait(b"Cancelled.")
        session.finish()
        self.assertFalse((self.managed / "current.ghostty").exists())
        self.assertEqual(len(list((self.managed / "history/activations").glob("*.json"))), 1)
        session = self.session()
        session.send("?R")
        session.wait(b"Press Enter to return")
        session.send("\n")
        session.wait(ALT_ENTER, 2)
        session.send("q")
        session.wait(b"Cancelled.")
        session.finish()
        report = subprocess.run([str(BINARY), "doctor"], env=self.env, capture_output=True, text=True, timeout=8)
        self.assertIn("integration-hook: verified", report.stdout)
        self.assertIn("projection: failed — projection.missing", report.stdout)
        session = self.session()
        session.wait_screen(b"Cursor")
        session.send("a")
        session.wait_screen(b"Profile applied.")
        session.send("q")
        session.finish()
        self.assertIn("projection: verified", self.cli("doctor"))

    def test_saved_profile_stays_visible_when_preview_fails(self):
        session = self.session()
        session.send("\tm")
        session.wait(b"New Profile ID:")
        session.send("missing\n")
        session.wait(b"Source: welcome")
        session.wait(b"Candidate path")
        session.send("no-such.png\n")
        session.wait(ALT_ENTER, 2)
        session.wait(b"n Create", 2)
        self.assertTrue((self.managed / "profiles/missing.toml").is_file())
        screen = re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", bytes(session.data).split(ALT_ENTER)[-1])
        self.assertIn(b"missing", screen, "saved Profile missing from browser after preview error")
        session.send("q")
        session.wait(b"Cancelled.")
        session.finish()

    def test_invalid_edit_preserves_intent_and_terminal(self):
        profile = self.managed / "profiles/welcome.toml"
        original = profile.read_bytes()
        session = self.session()
        session.send("t")
        session.wait(b"Field:")
        session.send("font_size\n")
        session.wait(b"Value:")
        session.send("-1\n")
        session.wait(ALT_ENTER, 2)
        session.send("q")
        session.wait(b"Cancelled.")
        session.finish()
        self.assertEqual(profile.read_bytes(), original)
        self.assertFalse(list((self.managed / "history/activations").glob("*.json")))

    def test_menu_exposes_cli_actions_and_previous_replays_history(self):
        self.cli("new", "night", str(ROOT / "tests/fixtures/white.png"))
        self.cli("apply", "welcome")
        self.cli("apply", "night")
        before = (self.managed / "current.ghostty").read_bytes()
        session = self.session()
        session.send("?")
        session.wait(b"Previous")
        session.send("p")
        session.wait(b"Type previous to replay prior Environment")
        self.assertEqual((self.managed / "current.ghostty").read_bytes(), before)
        session.send("previous\n")
        session.wait(b"Press Enter to return")
        session.send("\n")
        session.wait(ALT_ENTER, 2)
        session.send("q")
        session.wait(b"Cancelled.")
        session.finish()
        self.assertEqual(len(list((self.managed / "history/activations").glob("*.json"))), 3)
        self.assertNotEqual((self.managed / "current.ghostty").read_bytes(), before)

    def test_menu_read_only_and_destructive_cancel(self):
        session = self.session()
        session.send("?P")
        session.wait(b'"schema_version": 1')
        session.wait(b"Press Enter to return")
        self.assertFalse(list((self.managed / "history/activations").glob("*.json")))
        session.send("\n")
        session.wait(ALT_ENTER, 2)
        session.send("?D")
        session.wait(b"managed-layout:")
        session.send("\n")
        session.wait(ALT_ENTER, 3)
        session.send("?U")
        session.wait(b"Type update to install release")
        session.send("b\n")
        session.wait(b"Press Enter to return", 3)
        session.send("\n")
        session.wait(ALT_ENTER, 4)
        session.send("?X")
        session.wait(b"Type uninstall to remove integration")
        session.send("b\n")
        session.wait(b"Press Enter to return", 4)
        session.send("\n")
        session.wait(ALT_ENTER, 5)
        session.send("q")
        session.wait(b"Cancelled.")
        session.finish()
        self.assertTrue((self.managed / "config.toml").is_file())
        self.assertFalse(list((self.managed / "history/activations").glob("*.json")))

    def test_first_run_initializes_then_opens_fullscreen(self):
        shutil.rmtree(self.managed)
        session = Session(BINARY, self.env, "tui")
        self.addCleanup(session.close)
        session.wait(b"Type i then Enter to initialize")
        session.send("i\n")
        session.wait(ALT_ENTER)
        session.send("q")
        session.wait(b"Cancelled.")
        session.finish()
        self.assertTrue((self.managed / "config.toml").is_file())

    def test_ctrl_c_in_actions_menu_restores_terminal(self):
        session = self.session()
        session.send("?")
        session.wait(b"Doctor")
        session.send("\x03")
        session.wait(b"Cancelled.")
        session.finish()

    def test_tty_browser_previews_applies_and_restores_terminal(self):
        session = self.session()
        session.send("\r")  # Inspect sample; selection never activates.
        session.wait_screen(b"Cursor")
        self.assertFalse(list((self.managed / "history/activations").glob("*.json")))
        session.send("i")
        session.wait(b"image preview unavailable")
        session.send("\n")
        session.wait(ALT_ENTER, 2)
        session.send("a")
        session.wait_screen(b"Profile applied.")
        session.send("q")
        session.finish()
        self.assertTrue((self.managed / "current.ghostty").is_file())
        self.assertIn("1 ", self.cli("history"))
        self.assertIn("welcome", self.cli("list"))


if __name__ == "__main__":
    unittest.main()
