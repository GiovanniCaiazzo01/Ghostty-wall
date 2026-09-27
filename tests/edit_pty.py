"""Visual edit CLI, real PTYs and disposable HOME; no Ghostty or user bus.

Run: cargo build --locked && python3 -m unittest discover -s tests -p edit_pty.py -v
"""
import fcntl
import json
import os
import re
import select
import time
import struct
import subprocess
import tempfile
import termios
import unittest
from pathlib import Path

from tui_pty import Session, BINARY, ROOT, ALT_ENTER


class EditSession(Session):
    def screen(self):
        # Replay the cursor/erase subset emitted by Crossterm. Unlike stripping
        # ANSI, this preserves unchanged characters between Ratatui diff frames.
        cells, x, y = {}, 0, 0
        for token in re.findall(r"\x1b\[[0-?]*[ -/]*[@-~]|[^\x1b]+", self.data.decode(errors="replace")):
            if token.startswith("\x1b["):
                args, command = token[2:-1], token[-1]
                if command in "Hf":
                    position = [int(v or 1) for v in args.split(";")]
                    y, x = position[0] - 1, (position[1] if len(position) > 1 else 1) - 1
                elif command == "J" and args in ("2", "3"):
                    cells.clear()
                elif command == "h" and args == "?1049":
                    cells.clear()
                    x, y = 0, 0
                continue
            for char in token:
                if char == "\r":
                    x = 0
                elif char == "\n":
                    y += 1
                elif char >= " ":
                    cells[x, y] = char
                    x += 1
        return "\n".join("".join(cells.get((x, y), " ") for x in range(160)) for y in range(max((y for x, y in cells), default=0) + 1)).encode()

    def wait(self, needle, occurrences=1, timeout=30):
        def output():
            return bytes(self.data) if needle.startswith(b"\x1b") else b" ".join(self.screen().split())
        deadline = time.monotonic() + timeout
        while output().count(needle) < occurrences and time.monotonic() < deadline:
            if select.select([self.master], [], [], 0.1)[0]:
                self.data.extend(os.read(self.master, 65536))
            elif self.proc.poll() is not None:
                break
        if output().count(needle) < occurrences:
            raise AssertionError(f"Missing {needle!r}: {output()[-2500:]!r}")

    def finish(self):
        self.proc.wait(timeout=30)
        while select.select([self.master], [], [], 0)[0]:
            self.data.extend(os.read(self.master, 65536))
        super().finish()


class EditPty(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="gw-edit-pty-")
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name)
        self.env = {
            "HOME": str(self.home), "XDG_CONFIG_HOME": str(self.home / "config"),
            "XDG_RUNTIME_DIR": str(self.home / "runtime"), "PATH": "/nonexistent",
            "TERM": "xterm-256color", "TERM_PROGRAM": "edit-test",
        }
        self.root = self.home / "config/ghostty/ghostty-wall"
        self.cli("init")
        self.cli("new", "boy", str(ROOT / "tests/fixtures/white.png"))
        self.cli("apply", "welcome")

    def cli(self, *args):
        result = subprocess.run([str(BINARY), *args], env=self.env, capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def snapshot(self):
        return {p.relative_to(self.root): p.read_bytes() for p in self.root.rglob("*") if p.is_file()}

    def session(self, *args):
        session = EditSession(BINARY, self.env, "edit", *args)
        self.addCleanup(session.close)
        session.wait(ALT_ENTER, timeout=30)
        return session

    def test_keyboard_colors_numbers_replacement_decline_then_save_and_use_once(self):
        before = self.snapshot()
        session = self.session("boy")
        session.wait(b"Edit Profile boy")
        self.assertNotIn(b"Select Profile", session.data)
        session.wait(b"NOT live Ghostty reload")
        # Colors: choose a keyboard sample for Background, then exact hex for Foreground.
        session.send("\t\r")
        session.wait(b"Color samples")
        session.send("\x1b[C\r\x1b[B\r")
        session.send("h")
        session.wait(b"Exact hex color")
        session.send("#AABBCC\r")
        # Terminal background opacity, then Font size: step and direct entry.
        session.send("\t\r0.812345\r\x1b[B\x1b[C\x1b[D\r14.125\r")
        session.send("s")
        session.wait(b"Save and use Profile boy?")
        self.assertEqual(self.snapshot(), before)
        session.send("\r")  # Confirmation defaults to Back, never Save.
        session.wait(b"Back to editor; draft intact.")
        self.assertEqual(self.snapshot(), before)
        # Image replacement must be the same localized create picker, not a new path-only UI.
        downloads = self.home / "Scaricati personali"
        downloads.mkdir()
        original = downloads / "new.png"
        original.write_bytes((ROOT / "tests/fixtures/palette.png").read_bytes())
        (self.home / "config/user-dirs.dirs").write_text('XDG_DOWNLOAD_DIR="$HOME/Scaricati personali"\n')
        session.send("\t\r")
        session.wait(str(downloads).encode())
        session.wait(b"Number/relative path")
        session.send("/new\n1\n")
        session.wait(ALT_ENTER, 2)
        self.assertEqual(self.snapshot(), before)
        session.send("s")
        session.wait(b"Save and use Profile boy?")
        session.send("n")
        session.wait(b"Back to editor; draft intact.")
        session.send("sy")
        session.wait(b"Saved Profile boy.")
        session.wait(b"Ghostty reload: unavailable; Activation remains committed.")
        session.finish()
        plan = json.loads(self.cli("plan", "boy", "--json"))
        manifest = plan["environment"]["manifest"]
        self.assertEqual(manifest["terminal"]["font_size_millipoints"], 14125)
        self.assertEqual(manifest["terminal"]["background_opacity_millionths"], 812345)
        self.assertEqual(manifest["colors"]["foreground"], "aabbcc")
        self.assertEqual(manifest["colors"]["background"], "ffffff", "keyboard-selected sample must survive replacement")
        doc = (self.root / "profiles/boy.toml").read_text()
        self.assertIn('[colors.overrides]', doc)
        self.assertIn('foreground = "aabbcc"', doc)
        self.assertEqual(len(list((self.root / "history/activations").glob("*.json"))), 2)
        self.assertEqual(original.read_bytes(), (ROOT / "tests/fixtures/palette.png").read_bytes())
        self.assertEqual((self.root / "profiles/boy.png").read_bytes(), before[Path("profiles/boy.png")])

    def test_inline_palette_exact_input_survives_decline_and_reaches_activation(self):
        profile = self.root / "profiles/boy.toml"
        for version in [1, 2]:
            with self.subTest(version=version):
                profile.write_text(
                    f'schema_version = {version}\n'
                    'colors = { mode = "generated" }\n'
                    '[wallpaper]\nmode = "source"\nsource = "welcome"\n'
                    'selection = "path"\npath = "boy.png"\n'
                )
                initial = json.loads(self.cli("plan", "boy", "--json"))["environment"]["manifest"]
                before = self.snapshot()
                count = len(list((self.root / "history/activations").glob("*.json")))
                session = self.session("boy")
                session.wait(b"Edit Profile boy")
                session.send("\t" + "\x1b[B" * 20 + "\rh#12AB34\r")
                session.wait(b"ANSI palette 15 #12ab34 Customized")
                session.send("s")
                session.wait(b"Save and use Profile boy?")
                session.send("n")
                session.wait(b"Back to editor; draft intact.")
                session.wait(b"ANSI palette 15 #12ab34 Customized")
                self.assertEqual(self.snapshot(), before)
                session.send("sy")
                session.wait(b"Saved Profile boy.")
                session.wait(b"Ghostty reload: unavailable; Activation remains committed.")
                session.finish()

                plan = json.loads(self.cli("plan", "boy", "--json"))
                expected = initial
                expected["colors"]["palette"][15] = "12ab34"
                self.assertEqual(plan["environment"]["manifest"], expected)
                self.assertEqual(plan["profile"]["schema_version"], 2)
                history = sorted((self.root / "history/activations").glob("*.json"))
                self.assertEqual(len(history), count + 1)
                activation = json.loads(history[-1].read_text())
                self.assertEqual(activation["profile"], {"id": "boy", "schema_version": 2})
                record = self.root / "environments" / f'{activation["environment_id"]}.json'
                self.assertEqual(json.loads(record.read_text())["manifest"], expected)

    def test_selector_small_layout_invalid_input_and_cancel_preserve_active_welcome(self):
        before = self.snapshot()
        session = self.session()
        session.wait(b"Select Profile to edit")
        fcntl.ioctl(session.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 14, 60, 0, 0))
        session.send("\r")  # boy sorts first
        session.wait(b"Edit Profile boy")
        session.send("\t\t\x1b[B\r")
        session.wait(b"Exact number")
        session.send("0\r")
        session.wait(b"Draft retained")
        self.assertEqual(self.snapshot(), before)
        session.send("\x1b")
        time.sleep(0.1)  # Do not encode Alt-p as one terminal key.
        session.send("p")
        session.wait(b"Internal sample")
        session.send("\x03")  # Ctrl-C is normal cancellation, not a terminal-mode leak.
        session.wait(b"Cancelled; draft discarded")
        session.finish()
        self.assertEqual(self.snapshot(), before)

    def test_cancel_replacement_and_stale_save_leave_newer_state_intact(self):
        session = self.session("boy")
        session.wait(b"Edit Profile boy")
        session.send("\r")
        session.wait(b"Number/relative path")
        session.send("cancel\n")
        session.wait(ALT_ENTER, 2)
        session.send("\t\t\x1b[B\r18\r")
        # Internal mode holds no preview lock. A newer apply must never be rolled back on cancel.
        self.cli("apply", "boy")
        path = self.root / "profiles/boy.toml"
        path.write_text(path.read_text() + "\n# external edit\n")
        before = self.snapshot()
        session.send("sy")
        session.wait(b"Profile changed during editing")
        session.send("q")
        session.finish()
        self.assertEqual(self.snapshot(), before)

    def test_save_success_apply_failure_is_not_reported_as_unsaved(self):
        session = self.session("boy")
        session.wait(b"Edit Profile boy")
        session.send("\t\t\x1b[B\r17.5\r")
        hook = self.home / "config/ghostty/config.ghostty"
        hook.write_text("# externally removed hook\n")
        session.send("sy")
        session.wait(b"Saved Profile boy.")
        session.wait(b"Profile saved but apply failed")
        session.proc.wait(timeout=30)
        self.assertEqual(session.proc.returncode, 6)
        self.assertIn("font_size = 17.5", (self.root / "profiles/boy.toml").read_text())
        self.assertEqual(len(list((self.root / "history/activations").glob("*.json"))), 1)
        attrs = termios.tcgetattr(session.slave)
        self.assertTrue(attrs[3] & termios.ICANON and attrs[3] & termios.ECHO)


if __name__ == "__main__":
    unittest.main()
