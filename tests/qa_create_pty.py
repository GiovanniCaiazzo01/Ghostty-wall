"""Ticket 03 public-CLI PTY regressions; no Ghostty process or real user bus is used.

Run: cargo build --locked && python3 -m unittest discover -s tests -p qa_create_pty.py -v
Set GHOSTTY_WALL_BIN to test a different binary.
"""

import fcntl
import json
import os
import pty
import select
import shutil
import signal
import struct
import subprocess
import tempfile
import termios
import time
import unittest
from pathlib import Path


from tui_pty import BINARY, ROOT


class InlineSession:
    def __init__(self, env, *args):
        self.master, self.slave = pty.openpty()
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        self.before_terminal = termios.tcgetattr(self.slave)
        self.proc = subprocess.Popen(
            [str(BINARY), *args],
            env=env,
            stdin=self.slave,
            stdout=self.slave,
            stderr=self.slave,
            # Background runners may ignore SIGINT; only normalize this test child.
            preexec_fn=lambda: signal.signal(signal.SIGINT, signal.SIG_DFL),
        )
        self.output = bytearray()

    def wait_for(self, text, occurrences=1):
        needle = text.encode()
        deadline = time.monotonic() + 30
        while self.output.count(needle) < occurrences and time.monotonic() < deadline:
            if select.select([self.master], [], [], 0.1)[0]:
                self.output.extend(os.read(self.master, 65536))
            elif self.proc.poll() is not None:
                break
        if self.output.count(needle) < occurrences:
            raise AssertionError(f"Missing {text!r}: {self.output.decode(errors='replace')}")

    def send(self, text):
        os.write(self.master, text.encode())

    def finish(self, expected=0):
        self.proc.wait(timeout=30)
        while select.select([self.master], [], [], 0)[0]:
            self.output.extend(os.read(self.master, 65536))
        assert self.proc.returncode == expected, self.output.decode(errors="replace")
        assert termios.tcgetattr(self.slave) == self.before_terminal, "terminal mode changed"
        assert b"\x1b[?1049h" not in self.output, "create should stay inline"

    def close(self):
        if self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait(timeout=10)
        os.close(self.master)
        os.close(self.slave)


class CreatePty(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="gw-create-pty-qa-")
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name)
        self.env = {
            "HOME": str(self.home),
            "XDG_CONFIG_HOME": str(self.home / "config"),
            "XDG_RUNTIME_DIR": str(self.home / "runtime"),
            "PATH": "/nonexistent",
            "TERM": "xterm-256color",
        }
        self.root = self.home / "config/ghostty/ghostty-wall"
        self.cli("init")

    def cli(self, *args):
        result = subprocess.run(
            [str(BINARY), *args], env=self.env, capture_output=True, text=True, timeout=30
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def snapshot(self):
        return {p.relative_to(self.root): p.read_bytes() for p in self.root.rglob("*") if p.is_file()}

    def session(self, *args):
        session = InlineSession(self.env, "create", *args)
        self.addCleanup(session.close)
        return session

    def stage(self, session, image):
        session.wait_for("Wallpaper:")
        session.send("i\n")
        session.wait_for("Number/relative path")
        session.send(f"path:{image}\n")
        session.wait_for("[s]ave copied image and colors")

    def test_real_terminal_id_retry_explicit_variant_and_eof_cancel(self):
        before = self.snapshot()
        session = self.session()
        session.wait_for("Profile ID (or cancel):")
        session.send("Bad ID\n")
        session.wait_for("Profile ID (or cancel):", 2)
        session.send("welcome\n")
        session.wait_for("Profile ID (or cancel):", 3)
        session.send("guided\n")
        session.wait_for("Wallpaper:")
        session.send("g\n")
        session.wait_for("[s]ave, [a]nother")
        self.assertEqual(self.snapshot(), before)
        session.send("wrong\n")
        session.wait_for("[s]ave, [a]nother", 2)
        self.assertEqual(session.output.count(b"Generated variant "), 1)
        session.send("a\n")
        session.wait_for("[s]ave, [a]nother", 3)
        self.assertEqual(session.output.count(b"Generated variant "), 2)
        self.assertEqual(self.snapshot(), before)
        session.send("\x04")
        session.wait_for("Cancelled; no Profile saved.")
        session.finish()
        self.assertEqual(self.snapshot(), before)

    def test_staged_image_is_frozen_until_save_and_use_now_is_durable_not_live_proof(self):
        downloads = self.home / "Scaricati personali"
        downloads.mkdir()
        (self.home / "config/user-dirs.dirs").write_text(
            'XDG_DOWNLOAD_DIR="$HOME/Scaricati personali"\nXDG_PICTURES_DIR="$HOME/Immagini"\n'
        )
        original = downloads / "sky.png"
        shutil.copyfile(ROOT / "tests/fixtures/palette.png", original)
        selected = original.read_bytes()
        before = self.snapshot()
        session = self.session("owned")
        session.wait_for("Wallpaper:")
        self.assertNotIn(b"Profile ID (or cancel)", session.output)
        session.send("i\n")
        session.wait_for(f"Images in {downloads}")
        session.wait_for("Number/relative path")
        session.send("1\n")
        session.wait_for("[s]ave copied image and colors")
        self.assertEqual(self.snapshot(), before)
        # A user may replace the original while the prompt is open; Save must use decoded staged bytes.
        replacement = (ROOT / "tests/fixtures/white.png").read_bytes()
        original.write_bytes(replacement)
        session.send("s\n")
        session.wait_for("[y] Use now / [n] Not now")
        self.assertEqual((self.root / "profiles/owned.png").read_bytes(), selected)
        self.assertEqual(original.read_bytes(), replacement)
        self.assertFalse((self.root / "current.ghostty").exists())
        self.assertEqual(list((self.root / "history/activations").iterdir()), [])
        session.send("y\n")
        session.wait_for("Activated act-v1-0000000000000001 for Profile owned.")
        session.wait_for("Ghostty reload: unavailable; Activation remains committed.")
        session.finish()
        self.assertTrue((self.root / "current.ghostty").is_file())
        record = json.loads(
            (self.root / "history/activations/act-v1-0000000000000001.json").read_text()
        )
        self.assertEqual(record["profile"], {"id": "owned", "schema_version": 2})
        self.assertEqual(original.read_bytes(), replacement)
        self.assertIn(record["environment_id"], self.cli("history"))

    def test_concurrent_duplicate_save_cannot_overwrite_the_first_profile(self):
        first = self.session("same")
        second = self.session("same")
        self.stage(first, ROOT / "tests/fixtures/palette.png")
        self.stage(second, ROOT / "tests/fixtures/white.png")
        first.send("s\n")
        first.wait_for("[y] Use now / [n] Not now")
        first.send("n\n")
        first.finish()
        before_second_save = self.snapshot()
        second.send("s\n")
        second.wait_for("collision")
        second.finish(expected=3)
        self.assertEqual(self.snapshot(), before_second_save)

    def test_registry_drift_aborts_save_without_publishing_either_file(self):
        session = self.session("drift")
        self.stage(session, ROOT / "tests/fixtures/palette.png")
        self.cli("source", "add", "additional", "local", "profiles")
        before_save = self.snapshot()
        session.send("s\n")
        session.wait_for("Source registry changed")
        session.finish(expected=3)
        self.assertEqual(self.snapshot(), before_save)

    def test_signal_before_save_leaves_no_profile_or_staged_image(self):
        before = self.snapshot()
        session = self.session("interrupted")
        self.stage(session, ROOT / "tests/fixtures/palette.png")
        session.proc.send_signal(signal.SIGINT)
        session.finish(expected=-signal.SIGINT)
        self.assertEqual(self.snapshot(), before)


if __name__ == "__main__":
    unittest.main()
