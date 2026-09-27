"""Ticket 07: full-screen Profile lifecycle in disposable HOME, without Ghostty reload.

cargo build --locked && python3 -m unittest discover -s tests -p management_pty.py -v
"""
import fcntl
import json
import struct
import subprocess
import tempfile
import termios
import time
import unittest
from pathlib import Path

from edit_pty import EditSession
from tui_pty import ALT_ENTER, BINARY, ROOT


class ManagementPty(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="gw-management-")
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name)
        self.root = self.home / "config/ghostty/ghostty-wall"
        self.env = {
            "HOME": str(self.home), "XDG_CONFIG_HOME": str(self.home / "config"),
            "XDG_RUNTIME_DIR": str(self.home / "runtime"), "PATH": "/nonexistent",
            "TERM": "xterm-256color", "TERM_PROGRAM": "management-test",
        }
        self.cli("init")
        self.cli("new", "boy", str(ROOT / "tests/fixtures/white.png"))
        self.cli("apply", "welcome")

    def cli(self, *args):
        result = subprocess.run([str(BINARY), *args], env=self.env, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def snapshot(self):
        return {p.relative_to(self.root): p.read_bytes() for p in self.root.rglob("*") if p.is_file()}

    def session(self):
        session = EditSession(BINARY, self.env, "tui")
        self.addCleanup(session.close)
        session.wait(ALT_ENTER)
        session.wait(b"n Create")
        return session

    def finish(self, session):
        session.send("q")
        session.finish()

    def test_selection_sample_and_edit_cancel_are_read_only_with_active_marker(self):
        before = self.snapshot()
        session = self.session()
        for label in [b"welcome [active]", b"e Edit", b"x Delete", b"a Use", b"Internal sample", b"Selected text", b"Cursor", b"NOT live Ghostty reload"]:
            session.wait(label)
        session.send("jkp")
        self.assertEqual(self.snapshot(), before)
        session.send("e")
        session.wait(b"Edit Profile boy")
        session.send("\t\t\x1b[B\r18.125\r")
        session.wait(b"18.125")
        session.send("q")
        session.wait(b"draft discarded")
        session.wait(b"welcome [active]")
        self.finish(session)
        self.assertEqual(self.snapshot(), before)

    def test_generated_creation_variant_save_not_now_then_use_without_exiting(self):
        before = self.snapshot()
        session = self.session()
        session.send("n")
        session.wait(b"Profile ID:")
        session.send("welcome\r")
        session.wait(b"collision")
        session.send("\x7f" * 7 + "generated\rg")
        session.wait(b"Review generated")
        session.wait(b"Internal sample")
        session.wait(b"Another variant")
        self.assertEqual(self.snapshot(), before)
        session.send("a")
        time.sleep(0.4)
        self.assertEqual(self.snapshot(), before)
        session.send("s")
        session.wait(b"Saved Profile generated.")
        session.wait(b"Not now (default)")
        session.send("\r")
        session.wait(b"n Create")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), before[Path("current.ghostty")])
        self.assertEqual(len(list((self.root / "history/activations").glob("*.json"))), 1)
        document = (self.root / "profiles/generated.toml").read_text()
        self.assertIn('algorithm = "gradient-v1"', document)
        session.send("a")
        session.wait(b"Profile applied.")
        session.wait(b"generated [active]")
        session.send("v")
        session.wait(b"Ghostty reload: unavailable; Activation remains committed")
        session.send("\r")
        session.wait(b"n Create")
        self.finish(session)
        self.assertEqual(len(list((self.root / "history/activations").glob("*.json"))), 2)
        self.assertEqual((self.root / "profiles/generated.toml").read_text(), document)

    def test_image_picker_localized_roots_validation_cancel_and_saved_profile(self):
        pictures = self.home / "Immagini mie"
        downloads = self.home / "Scaricati miei"
        pictures.mkdir()
        downloads.mkdir()
        (self.home / "config/user-dirs.dirs").write_text('XDG_DOWNLOAD_DIR="$HOME/Scaricati miei"\nXDG_PICTURES_DIR="$HOME/Immagini mie"\n')
        original = pictures / "scene.png"
        original.write_bytes((ROOT / "tests/fixtures/palette.png").read_bytes())
        (pictures / "bad.png").write_bytes(b"not an image")
        (pictures / "hidden.txt").write_text("not offered")
        before = self.snapshot()
        session = self.session()
        session.send("ncopy\ri")
        session.wait(str(downloads).encode())
        session.send("p\n")
        session.wait(str(pictures).encode())
        session.send("bad.png\n")
        session.wait(b"PNG or JPEG")
        session.send("/scene\n1\n")
        session.wait(b"Review copy")
        self.assertEqual(self.snapshot(), before)
        session.send("\x1b")
        session.wait(b"Creation cancelled")
        self.assertEqual(self.snapshot(), before)
        session.send("ncopy\ri")
        session.wait(b"Number/relative path")
        session.send("p\n/scene\n1\n")
        session.wait(b"Review copy")
        session.send("s")
        session.wait(b"Saved Profile copy.")
        session.send("y")
        session.wait(b"copy [active]")
        self.finish(session)
        self.assertEqual(original.read_bytes(), (ROOT / "tests/fixtures/palette.png").read_bytes())
        self.assertEqual(original.read_bytes(), (self.root / "profiles/copy.png").read_bytes())

    def test_visual_edit_decline_confirm_apply_once_then_delete_fallback(self):
        before = self.snapshot()
        session = self.session()
        session.send("e")
        session.wait(b"Edit Profile boy")
        session.send("\t\rhabcdef\r\t\x1b[B\r17.125\rs")
        session.wait(b"Save and use Profile boy?")
        session.send("\r")
        session.wait(b"Back to editor; draft intact")
        self.assertEqual(self.snapshot(), before)
        session.send("sy")
        session.wait(b"boy [active]")
        self.assertEqual(len(list((self.root / "history/activations").glob("*.json"))), 2)
        plan = json.loads(self.cli("plan", "boy", "--json"))
        self.assertEqual(plan["environment"]["manifest"]["colors"]["background"], "abcdef")
        self.assertEqual(plan["environment"]["manifest"]["terminal"]["font_size_millipoints"], 17125)
        entries = session.data.count(ALT_ENTER)
        session.send("x")
        session.wait(b"Cancel (default)")
        session.send("\n")
        session.wait(ALT_ENTER, entries + 1)
        session.wait(b"n Create")
        self.assertTrue((self.root / "profiles/boy.toml").exists())
        session.send("x")
        session.wait(b"Cancel (default)")
        session.send("y\n")
        session.wait(ALT_ENTER, entries + 2, timeout=60)
        session.wait(b"welcome [active]")
        self.finish(session)
        self.assertFalse((self.root / "profiles/boy.toml").exists())
        self.assertEqual(len(list((self.root / "history/activations").glob("*.json"))), 3)
        self.cli("previous")
        self.assertIn("font-size = 17.125", (self.root / "current.ghostty").read_text())

    def test_small_layout_sample_controls_and_cancellation(self):
        before = self.snapshot()
        session = self.session()
        fcntl.ioctl(session.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 40, 0, 0))
        session.send("p")
        session.wait(b"Cursor")
        session.wait(b"p list")
        session.send("p")
        session.wait(b"n Create")
        session.send("nsmall\rg")
        session.wait(b"s Save")
        session.wait(b"a Another variant")
        session.send("p")
        session.wait(b"Selected text")
        session.wait(b"Cursor")
        session.send("\x03")
        session.wait(b"Creation cancelled")
        session.send("e")
        session.wait(b"Edit Profile boy")
        session.send("\t\t\x1b[B\r")
        session.wait(b"Enter accept")
        session.send("18\r")
        session.wait(b"Font size (points)")
        session.wait(b"18")
        session.send("p")
        session.wait(b"Cursor")
        session.send("\x03")
        session.wait(b"n Create")
        self.finish(session)
        self.assertEqual(self.snapshot(), before)

    def test_saved_edit_survives_apply_failure_and_remains_in_list(self):
        session = self.session()
        session.send("e")
        session.wait(b"Edit Profile boy")
        session.send("\t\t\x1b[B\r19\r")
        (self.home / "config/ghostty/config.ghostty").write_text("# integration removed externally\n")
        session.send("sy")
        session.wait(b"Profile saved but apply failed")
        session.wait(b"welcome [active]")
        session.send("v")
        session.wait(b"Profile saved but apply failed")
        session.send("\r")
        session.wait(b"n Create")
        self.finish(session)
        self.assertIn("font_size = 19", (self.root / "profiles/boy.toml").read_text())
        self.assertEqual(len(list((self.root / "history/activations").glob("*.json"))), 1)

    def test_failed_preview_and_replay_never_invent_active_profile(self):
        self.cli("apply", "boy")
        self.cli("previous")
        (self.root / "profiles/boy.toml").write_text('schema_version = 999\n')
        before = self.snapshot()
        session = self.session()
        session.wait(b"unsupported")
        self.assertNotIn(b"[active]", session.screen())
        session.send("v")
        session.wait(b"unsupported")
        session.send("\rj")
        session.wait(b"Internal sample")
        self.finish(session)
        self.assertEqual(self.snapshot(), before)


if __name__ == "__main__":
    unittest.main()
