"""Ticket 07 independent public-CLI/PTY regressions; not live Ghostty evidence.

GHOSTTY_WALL_BIN=target/release/ghostty-wall python3 -m unittest discover -s tests -p qa_management_pty.py -v
Children receive only disposable HOME/XDG paths and no Ghostty/reload executables.
"""

import fcntl
import hashlib
import json
import os
import re
import select
import shutil
import struct
import subprocess
import sys
import tempfile
import termios
import time
import unittest
from pathlib import Path

from edit_pty import EditSession
from qa_create_publication import FAULT_SHIM
from tui_pty import ALT_ENTER, BINARY, ROOT

DOWN = "\x1b[B"
UP = "\x1b[A"
RIGHT = "\x1b[C"


def viewport(session):
    rows, columns, _, _ = struct.unpack(
        "HHHH", fcntl.ioctl(session.slave, termios.TIOCGWINSZ, b"\0" * 8)
    )
    return "\n".join(line[:columns] for line in session.screen().decode().splitlines()[:rows])


def styled_cells(session):
    """Replay the cursor/SGR subset emitted by Ratatui, retaining sample RGB evidence."""
    cells, x, y, fg, bg = {}, 0, 0, None, None
    for token in re.findall(r"\x1b\[[0-?]*[ -/]*[@-~]|[^\x1b]+", session.data.decode(errors="replace")):
        if token.startswith("\x1b["):
            args, command = token[2:-1], token[-1]
            if command in "Hf":
                values = [int(v or 1) for v in args.split(";")]
                y, x = values[0] - 1, (values[1] if len(values) > 1 else 1) - 1
            elif command == "J" and args in ("2", "3"):
                cells.clear()
            elif command == "h" and args == "?1049":
                cells.clear()
                x, y = 0, 0
            elif command == "m":
                values = [int(v or 0) for v in args.split(";")]
                i = 0
                while i < len(values):
                    code = values[i]
                    if code == 0:
                        fg, bg = None, None
                    elif code == 39:
                        fg = None
                    elif code == 49:
                        bg = None
                    elif code in (38, 48) and i + 1 < len(values):
                        length = 5 if values[i + 1] == 2 else 3
                        color = tuple(values[i + 2:i + length])
                        if code == 38:
                            fg = color
                        else:
                            bg = color
                        i += length - 1
                    i += 1
            continue
        for char in token:
            if char == "\r":
                x = 0
            elif char == "\n":
                y += 1
            elif char >= " ":
                cells[x, y] = (char, fg, bg)
                x += 1
    return cells


class ManagementFixture(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="gw-qa-management-")
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name)
        self.root = self.home / "config/ghostty/ghostty-wall"
        self.env = {
            "HOME": str(self.home), "XDG_CONFIG_HOME": str(self.home / "config"),
            "XDG_RUNTIME_DIR": str(self.home / "runtime"), "PATH": "/nonexistent",
            "TERM": "xterm-256color", "TERM_PROGRAM": "qa-management",
        }
        self.cli("init")
        self.profile = self.root / "profiles/amber.toml"
        self.profile.write_text(
            '# Existing schema-v1 Profile with inline colors.\n'
            'schema_version = 1\ncolors = { mode = "generated" }\n'
            '[wallpaper]\nmode = "source"\nsource = "welcome"\n'
            'selection = "path"\npath = "old.png"\n'
            'opacity = 0.5\nfit = "contain"\nposition = "bottom-right"\nrepeat = true\n'
        )
        (self.root / "profiles/old.png").write_bytes((ROOT / "tests/fixtures/white.png").read_bytes())
        self.original = self.home / "original.png"
        self.original.write_bytes((ROOT / "tests/fixtures/palette.png").read_bytes())
        self.cli("apply", "welcome")

    def cli(self, *args, input=""):
        result = subprocess.run([str(BINARY), *args], env=self.env, input=input,
                                capture_output=True, text=True, timeout=45)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        return result.stdout

    def plan(self, profile="amber", *args):
        return json.loads(self.cli("plan", profile, *args, "--json"))

    def snapshot(self):
        return {p.relative_to(self.root): p.read_bytes()
                for p in self.root.rglob("*") if p.is_file()}

    def history(self):
        return [json.loads(p.read_text())
                for p in sorted((self.root / "history/activations").glob("*.json"))]

    def session(self, *args, ignore_file_size_signal=False):
        if ignore_file_size_signal:
            # Popen resets Python's ignored SIGXFSZ; retain EFBIG instead of killing the faulted child.
            session = EditSession("/bin/sh", self.env, "-c", 'trap "" XFSZ; exec "$@"',
                                  "qa-management", str(BINARY), "tui", *args)
        else:
            session = EditSession(BINARY, self.env, "tui", *args)
        self.addCleanup(session.close)
        session.wait(ALT_ENTER)
        session.wait(b"n Create")
        session.wait(b"Cursor")
        return session

    def finish(self, session):
        session.send("q")
        # Drain repaint output while quitting: a full PTY buffer can block the writer.
        deadline = time.monotonic() + 30
        while session.proc.poll() is None and time.monotonic() < deadline:
            if select.select([session.master], [], [], 0.1)[0]:
                session.data.extend(os.read(session.master, 65536))
        session.finish()

    def open_editor(self, session):
        session.send("e")
        session.wait(b"Edit Profile amber")

    def replace(self, session):
        session.send("\r")
        session.wait(b"Number/relative path")
        count = session.data.count(ALT_ENTER)
        session.send(f"path:{self.original}\n")
        session.wait(ALT_ENTER, count + 1)
        session.wait(b"Edit Profile amber")

    def details(self, session, text):
        session.send("v")
        session.wait(text)
        session.send("\r")

    def menu_previous(self, session):
        count = session.data.count(ALT_ENTER)
        session.send("?p")
        session.wait(b"Type previous")
        session.send("previous\n")
        session.wait(b"Press Enter to return")
        session.send("\n")
        session.wait(ALT_ENTER, count + 1)
        session.wait(b"n Create")


class ManagementPublicPath(ManagementFixture):
    def test_sample_rgb_wallpaper_text_all_ansi_cursor_selection_and_navigation_are_read_only(self):
        before = self.snapshot()
        manifest = self.plan()["environment"]["manifest"]
        session = self.session()
        cells = styled_cells(session)
        text = viewport(session)
        for label in ["welcome [active]", "n Create", "e Edit", "x Delete", "a Use", "NOT live Ghostty reload"]:
            self.assertIn(label, text)
        sample_x, sample_y = next((x, y) for (x, y), value in cells.items() if value[0] == "$")
        colors = manifest["colors"]
        rgb = lambda value: tuple(bytes.fromhex(value))
        blended = tuple((v + 255) // 2 for v in rgb(colors["background"]))
        self.assertEqual(cells[sample_x, sample_y], ("$", rgb(colors["foreground"]), blended))
        for i, color in enumerate(colors["palette"]):
            self.assertEqual(cells[sample_x + (i % 8) * 3, sample_y + 2 + i // 8][1], rgb(color), i)
        self.assertEqual(cells[sample_x + 1, sample_y + 4][1:],
                         (rgb(colors["selection_foreground"]), rgb(colors["selection_background"])))
        self.assertEqual(cells[sample_x, sample_y + 5][1], rgb(colors["cursor"]))
        session.send("jkp")
        session.wait(b"Cursor")
        self.finish(session)
        self.assertEqual(self.snapshot(), before)

    def test_v1_edit_decline_cancel_then_confirm_use_switch_and_replay(self):
        before = self.snapshot()
        session = self.session()
        self.open_editor(session)
        session.send("\t\t" + DOWN + "\r17.125\rs")
        session.wait(b"Save and use Profile amber?")
        session.send("\r")
        session.wait(b"Back to editor; draft intact")
        self.assertEqual(self.snapshot(), before)
        session.send("q")
        session.wait(b"draft discarded")
        self.assertEqual(self.snapshot(), before)
        self.open_editor(session)
        session.send("\t\t" + DOWN + "\r17.125\rsy")
        session.wait(b"amber [active]")
        self.assertEqual(len(self.history()), 2)
        plan = self.plan()
        self.assertEqual(plan["profile"]["schema_version"], 1)
        self.assertEqual(plan["environment"]["manifest"]["terminal"]["font_size_millipoints"], 17125)
        document = self.profile.read_bytes()
        session.send("ja")
        session.wait(b"welcome [active]")
        self.assertEqual(len(self.history()), 3)
        self.menu_previous(session)
        self.assertNotIn("[active]", viewport(session))
        self.assertEqual(self.history()[-1]["environment_id"], plan["environment"]["environment_id"])
        self.assertEqual(self.profile.read_bytes(), document)
        self.assertEqual((self.root / "config.toml").read_bytes(), before[Path("config.toml")])
        self.finish(session)

    def test_failed_embedded_save_rolls_back_new_image_and_retries_same_draft(self):
        before = self.snapshot()
        session = self.session()
        self.open_editor(session)
        self.replace(session)
        self.profile.chmod(0o622)
        self.addCleanup(self.profile.chmod, 0o600)
        session.send("sy")
        session.wait(b"Draft retained")
        self.details(session, b"pre-save files preserved")
        self.assertEqual(self.snapshot(), before)
        self.profile.chmod(0o600)
        session.send("sy")
        session.wait(b"amber [active]")
        plan = self.plan()
        candidate = self.root / "profiles" / plan["selection"]["candidate"]
        self.assertEqual(candidate.read_bytes(), self.original.read_bytes())
        self.assertEqual((self.root / "profiles/old.png").read_bytes(), before[Path("profiles/old.png")])
        self.assertEqual(len(self.history()), 2)
        self.finish(session)

    def test_stale_edit_and_cancel_preserve_concurrent_apply_and_newer_intent(self):
        session = self.session()
        self.open_editor(session)
        self.replace(session)
        self.cli("apply", "amber")
        self.profile.write_text(self.profile.read_text() + "\n# another editor saved this\n")
        newer = self.snapshot()
        session.send("sy")
        session.wait(b"Draft retained")
        self.details(session, b"Profile changed during editing")
        self.assertEqual(self.snapshot(), newer)
        session.send("q")
        session.wait(b"amber [active]")
        self.finish(session)
        self.assertEqual(self.snapshot(), newer)

    def test_changed_registry_rejects_save_without_partial_image_and_keeps_management_usable(self):
        session = self.session()
        self.open_editor(session)
        self.replace(session)
        self.cli("source", "add", "other", "local", "other-images")
        newer = self.snapshot()
        session.send("sy")
        session.wait(b"Draft retained")
        self.details(session, b"Source registry changed")
        session.send("q")
        session.wait(b"n Create")
        self.finish(session)
        self.assertEqual(self.snapshot(), newer)

    def test_old_install_without_welcome_failed_fallback_then_switch_delete_and_replay(self):
        self.cli("apply", "amber")
        (self.root / "profiles/welcome.toml").unlink()
        (self.root / "profiles/welcome.png").unlink()
        other = self.root / "profiles/other.toml"
        other.write_text("schema_version = 1\n[terminal]\nfont_size = 12\n")
        before = self.snapshot()
        old_projection = (self.root / "current.ghostty").read_bytes()
        session = self.session()
        session.send("x")
        session.wait(b"Cancel (default)")
        count = session.data.count(ALT_ENTER)
        session.send("y\n")
        session.wait(ALT_ENTER, count + 1)
        self.details(session, b"Welcome fallback failed")
        self.assertEqual(self.snapshot(), before)
        session.send("ja")
        session.wait(b"other [active]")
        session.send("kx")
        session.wait(b"Cancel (default)")
        count = session.data.count(ALT_ENTER)
        session.send("y\n")
        session.wait(ALT_ENTER, count + 1)
        session.wait(b"n Create")
        self.assertFalse(self.profile.exists())
        self.assertFalse((self.root / "profiles/welcome.toml").exists())
        (self.root / "profiles/old.png").unlink()
        self.menu_previous(session)
        self.assertEqual((self.root / "current.ghostty").read_bytes(), old_projection)
        self.assertNotIn("[active]", viewport(session))
        self.finish(session)

    def test_imported_creation_matches_cli_manifest_not_now_then_full_edit_delete_path(self):
        baseline = (self.root / "current.ghostty").read_bytes()
        session = self.session()
        session.send("nnewcomer\ri")
        session.wait(b"Number/relative path")
        session.send(f"path:{self.original}\n")
        session.wait(b"Review newcomer")
        session.send("s")
        session.wait(b"Saved Profile newcomer.")
        session.send("\x03")
        session.wait(b"n Create")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), baseline)
        self.assertEqual(len(self.history()), 1)
        self.cli("create", "reference", input=f"i\npath:{self.original}\ns\nn\n")
        self.assertEqual(self.plan("newcomer")["environment"], self.plan("reference")["environment"])
        session.send("e")
        session.wait(b"Edit Profile newcomer")
        session.send("\t\rh#123456\rsy")
        session.wait(b"newcomer [active]")
        self.assertEqual(self.plan("newcomer")["environment"]["manifest"]["colors"]["background"], "123456")
        session.send("x")
        session.wait(b"Cancel (default)")
        count = session.data.count(ALT_ENTER)
        session.send("y\n")
        session.wait(ALT_ENTER, count + 1)
        session.wait(b"welcome [active]")
        self.assertFalse((self.root / "profiles/newcomer.toml").exists())
        self.assertEqual(self.original.read_bytes(), (ROOT / "tests/fixtures/palette.png").read_bytes())
        self.assertEqual(len(self.history()), 3)
        self.menu_previous(session)
        self.assertIn("background = 123456", (self.root / "current.ghostty").read_text())
        self.finish(session)

    def test_explicit_random_seed_selection_preview_and_use_match_public_cli(self):
        self.profile.write_text(self.profile.read_text().replace('selection = "path"\npath = "old.png"', 'selection = "random"'))
        seed = "35" * 32
        expected = self.plan("amber", "--seed", seed)
        before = self.snapshot()
        session = self.session("--seed", seed)
        session.send("j")
        session.wait(b"Cursor")
        session.send("k")
        session.wait(b"Cursor")
        self.assertEqual(self.snapshot(), before)
        session.send("a")
        session.wait(b"amber [active]")
        activation = self.history()[-1]
        self.assertEqual(activation["selection"], expected["selection"])
        self.assertEqual(activation["environment_id"], expected["environment"]["environment_id"])
        self.assertEqual(self.profile.read_bytes(), before[Path("profiles/amber.toml")])
        self.finish(session)

    def test_reload_failure_and_action_acceptance_are_not_reported_as_verified_live(self):
        bin_dir = self.home / "bin"
        bin_dir.mkdir()
        script = bin_dir / "systemctl"
        log = self.home / "reload.log"
        self.env.update(PATH=str(bin_dir), GW_QA_RELOAD_LOG=str(log))
        for status, label in [(1, b"failed; Activation remains committed"),
                              (0, b"action accepted; visible change is not verified")]:
            with self.subTest(reload_exit=status):
                script.write_text(
                    '#!/bin/sh\nprintf "%s\\n" "$*" >> "$GW_QA_RELOAD_LOG"\n'
                    'case "$2" in\n  is-active) exit 0;;\n'
                    f'  reload) exit {status};;\nesac\nexit 1\n'
                )
                script.chmod(0o700)
                if log.exists():
                    log.unlink()
                before = self.snapshot()
                session = self.session()
                self.open_editor(session)
                session.send("q")
                session.wait(b"draft discarded")
                self.assertFalse(log.exists(), "internal browsing/editor must not call reload")
                self.assertEqual(self.snapshot(), before)
                count = len(self.history())
                session.send("a")
                session.wait(b"amber [active]")
                self.details(session, label)
                self.assertEqual(len(self.history()), count + 1)
                self.assertEqual(log.read_text().splitlines(), [
                    "--user is-active --quiet app-com.mitchellh.ghostty.service",
                    "--user reload app-com.mitchellh.ghostty.service",
                ])
                self.finish(session)

    @unittest.skipUnless(sys.platform == "linux", "Ticket 07 per-child file-size fault uses Linux prlimit")
    def test_create_failed_save_rolls_back_without_changing_source_files_then_same_draft_retries(self):
        import resource

        before = self.snapshot()
        session = self.session(ignore_file_size_signal=True)
        session.send("nrollback\ri")
        session.wait(b"Number/relative path")
        session.send(f"path:{self.original}\n")
        session.wait(b"Review rollback")
        # The PNG fits, but the TOML does not. Only this disposable child is limited.
        self.assertLess(self.original.stat().st_size, 200)
        limits = resource.prlimit(session.proc.pid, resource.RLIMIT_FSIZE)
        resource.prlimit(session.proc.pid, resource.RLIMIT_FSIZE, (200, limits[1]))
        session.send("s")
        session.wait(b"draft retained")
        self.details(session, b"pre-save files preserved")
        self.assertEqual(self.snapshot(), before)
        resource.prlimit(session.proc.pid, resource.RLIMIT_FSIZE, limits)
        session.send("s")
        session.wait(b"Saved Profile rollback.")
        session.send("n")
        session.wait(b"n Create")
        self.assertEqual(self.plan("rollback")["asset"]["sha256"],
                         hashlib.sha256(self.original.read_bytes()).hexdigest())
        self.assertEqual(len(self.history()), 1)
        self.finish(session)

    def test_minimum_viewport_long_ids_color_number_confirm_sample_and_menu(self):
        long_id = "a" * 64
        self.profile.rename(self.root / "profiles" / f"{long_id}.toml")
        before = self.snapshot()
        session = self.session()
        fcntl.ioctl(session.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 40, 0, 0))
        session.send("p")
        session.wait(b"p list")
        for label in ["Selected text", "Cursor", "n Create", "a Use"]:
            self.assertIn(label, viewport(session))
        session.send("pe\t\r")
        session.wait(b"Color samples")
        for label in ["h exact hex", "a Automatic", "Esc back"]:
            self.assertIn(label, viewport(session))
        session.send("h112233\r\t" + DOWN + "\r")
        session.wait(b"Exact number")
        for label in ["Enter accept", "Esc back"]:
            self.assertIn(label, viewport(session))
        session.send("18.125\rs")
        session.wait(b"Enter confirm")
        for label in ["Back to editor", "Enter confirm", "n/Esc back"]:
            self.assertIn(label, viewport(session))
        session.send("nq")
        session.wait(b"n Create")
        session.send("?\x1b[F")
        session.wait(b"Uninstall integration")
        self.assertIn("Uninstall integration", viewport(session))
        self.assertIn("Enter run", viewport(session))
        session.send("\x1b")
        session.wait(b"n Create")
        self.finish(session)
        self.assertEqual(self.snapshot(), before)

    def test_below_minimum_blocks_actions_until_resize_and_allows_cancellation(self):
        before = self.snapshot()
        session = self.session()
        fcntl.ioctl(session.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 8, 30, 0, 0))
        session.send("nasy")
        session.wait(b"Resize to 40x12")
        self.assertEqual(self.snapshot(), before)
        self.assertIn("Esc/Ctrl-C cancels", viewport(session))
        session.send("\x1b")
        session.finish()
        self.assertEqual(self.snapshot(), before)


@unittest.skipUnless(sys.platform == "linux" and shutil.which("cc"),
                     "Ticket 07 publication fault checks require Linux LD_PRELOAD and a C compiler")
class ManagementPublication(ManagementFixture):
    @classmethod
    def setUpClass(cls):
        cls.build = tempfile.TemporaryDirectory(prefix="gw-management-fault-build-")
        cls.addClassCleanup(cls.build.cleanup)
        source = Path(cls.build.name) / "fault.c"
        cls.shim = Path(cls.build.name) / "fault.so"
        source.write_text(FAULT_SHIM)
        result = subprocess.run(
            [shutil.which("cc"), "-shared", "-fPIC", "-Wall", "-Wextra", "-Werror",
             "-o", str(cls.shim), str(source), "-ldl"],
            capture_output=True, text=True, timeout=30,
        )
        if result.returncode:
            raise AssertionError(result.stderr)

    def test_create_uncertain_image_or_profile_publication_preserves_evidence_without_activation(self):
        for number in [1, 2]:
            with self.subTest(directory_sync=number):
                self.env.update(LD_PRELOAD=str(self.shim), GW_QA_SYNC_DIRECTORY=str(self.root / "profiles"),
                                GW_QA_FAIL_SYNC=str(number))
                before = self.snapshot()
                name = f"uncertain-{number}"
                session = self.session()
                session.send(f"n{name}\ri")
                session.wait(b"Number/relative path")
                session.send(f"path:{self.original}\n")
                session.wait(f"Review {name}".encode())
                session.send("s")
                session.wait(b"n Create")
                self.details(session, b"durability is uncertain")
                self.assertNotIn(f"Saved Profile {name}.".encode(), session.data)
                self.assertNotIn(b"Creation cancelled", session.data)
                after = self.snapshot()
                self.assertEqual(after.pop(Path(f"profiles/{name}.png")), self.original.read_bytes())
                if number == 2:
                    self.assertIn(b"schema_version = 2", after.pop(Path(f"profiles/{name}.toml")))
                    self.plan(name)
                self.assertEqual(after, before)
                self.finish(session)

    def test_embedded_uncertain_edit_blocks_retry_and_cancel_does_not_claim_unsaved(self):
        self.env.update(LD_PRELOAD=str(self.shim), GW_QA_SYNC_DIRECTORY=str(self.root / "profiles"),
                        GW_QA_FAIL_SYNC="2")
        before = self.snapshot()
        session = self.session()
        self.open_editor(session)
        self.replace(session)
        session.send("sy")
        session.wait(b"Publication at")
        self.details(session, b"durability is uncertain")
        session.send("sy")
        session.wait(b"Save blocked")
        session.send("q")
        session.wait(b"n Create")
        self.details(session, b"durability is uncertain")
        self.assertNotIn(b"Cancelled; draft discarded", session.data)
        self.assertNotIn(b"Saved Profile amber.", session.data)
        self.assertNotEqual(self.profile.read_bytes(), before[Path("profiles/amber.toml")])
        self.assertEqual({k: v for k, v in self.snapshot().items() if k.parts[0] != "profiles"},
                         {k: v for k, v in before.items() if k.parts[0] != "profiles"})
        self.plan()
        self.finish(session)


if __name__ == "__main__":
    unittest.main()
