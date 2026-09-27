"""Deletion prompts in real PTYs; isolated HOME and no live Ghostty reload adapters."""

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from tui_pty import ALT_ENTER, BINARY, ROOT, Session


class DeletePty(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.env = dict(HOME=self.tmp.name, XDG_CONFIG_HOME=self.tmp.name + "/config",
                        XDG_RUNTIME_DIR=self.tmp.name + "/runtime", PATH="/nonexistent",
                        TERM="xterm-256color")
        self.root = Path(self.env["XDG_CONFIG_HOME"]) / "ghostty/ghostty-wall"
        self.cli("init")
        self.cli("new", "boy", str(ROOT / "tests/fixtures/white.png"))
        self.cli("apply", "boy")

    def cli(self, *args):
        result = subprocess.run([str(BINARY), *args], env=self.env, capture_output=True,
                                text=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def session(self, *args):
        session = Session(BINARY, self.env, *args)
        self.addCleanup(session.close)
        return session

    def snapshot(self):
        return {str(p.relative_to(self.root)): p.read_bytes()
                for p in self.root.rglob("*") if p.is_file()}

    def test_named_confirmation_and_selector_default_cancel(self):
        before = self.snapshot()
        session = self.session("delete", "boy")
        session.wait(b"Cancel (default)")
        self.assertIn(b"Delete Profile boy?", session.data)
        self.assertNotIn(b"Select Profile", session.data)
        session.send("\n")
        session.wait(b"Deletion cancelled")
        session.proc.wait(timeout=20)
        self.assertEqual(session.proc.returncode, 0)
        self.assertEqual(self.snapshot(), before)

        session = self.session("delete")
        session.wait(b"Profile (default: Cancel)")
        self.assertIn(b"boy [active]", session.data)
        session.send("1\n")
        session.wait(b"Delete Profile boy?")
        session.send("n\n")
        session.wait(b"Deletion cancelled")
        session.proc.wait(timeout=20)
        self.assertEqual(self.snapshot(), before)

    def test_competing_apply_during_confirmation_cannot_be_overwritten(self):
        session = self.session("delete", "boy")
        session.wait(b"Cancel (default)")
        self.cli("apply", "welcome")
        before = self.snapshot()
        session.send("y\n")
        session.wait(b"changed since confirmation")
        session.proc.wait(timeout=20)
        self.assertNotEqual(session.proc.returncode, 0)
        self.assertEqual(self.snapshot(), before)

    def test_fullscreen_delete_uses_same_confirmation_and_fallback(self):
        session = self.session("tui")
        session.wait(ALT_ENTER)
        session.send("\tx")
        session.wait(b"Cancel (default)")
        self.assertIn(b"Delete Profile boy?", session.data)
        self.assertIn(b"Preserve: original images", session.data)
        session.send("y\n")
        session.wait(b"Welcome Activation")
        session.wait(ALT_ENTER, 2)
        self.assertFalse((self.root / "profiles/boy.toml").exists())
        self.assertFalse((self.root / "profiles/boy.png").exists())
        records = sorted((self.root / "history/activations").glob("*.json"))
        self.assertEqual(len(records), 2)
        self.assertEqual(json.loads(records[-1].read_bytes())["profile"]["id"], "welcome")
        session.send("q")
        session.wait(b"Cancelled.")
        session.finish()
        self.cli("previous")
        self.cli("doctor")


if __name__ == "__main__":
    unittest.main()
