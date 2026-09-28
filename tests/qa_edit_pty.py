"""Ticket 05 independent public-CLI regressions; disposable paths and real PTYs only.

Run: cargo build --locked && python3 -m unittest discover -s tests -p qa_edit_pty.py -v
No Ghostty executable, session bus, or live reload is available to these children.
"""

import hashlib
import json
import os
import re
import select
import shutil
import signal
import subprocess
import sys
import tempfile
import termios
import unittest
from pathlib import Path

from edit_pty import EditSession
from qa_create_publication import FAULT_SHIM
from tui_pty import ALT_ENTER, ALT_LEAVE, BINARY, ROOT

DOWN = "\x1b[B"
UP = "\x1b[A"
RIGHT = "\x1b[C"
LEFT = "\x1b[D"


class EditFixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="gw-qa-edit-")
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name)
        self.root = self.home / "xdg/ghostty/ghostty-wall"
        self.env = {
            "HOME": str(self.home),
            "XDG_CONFIG_HOME": str(self.home / "xdg"),
            "XDG_RUNTIME_DIR": str(self.home / "runtime"),
            "PATH": "/nonexistent",
            "TERM": "xterm-256color",
            "TERM_PROGRAM": "qa-edit",
        }
        self.cli("init")
        self.profile = self.root / "profiles/boy.toml"
        (self.root / "profiles/old.png").write_bytes((ROOT / "tests/fixtures/white.png").read_bytes())
        self.profile.write_text(
            '# Existing version-1 recipe, not a newly generated v2 fixture.\n'
            'schema_version = 1\n[wallpaper]\nmode = "source"\n'
            'source = "welcome"\nselection = "path"\npath = "old.png"\n'
            'fit = "contain"\nposition = "bottom-right"\nrepeat = true\n'
            'opacity = 0.234567\n[colors]\nmode = "generated"\n'
        )
        (self.root / "profiles/active.toml").write_text('schema_version = 1\n[terminal]\nfont_size = 11\n')
        self.cli("apply", "active")
        self.original = self.home / "chosen.png"
        self.original.write_bytes((ROOT / "tests/fixtures/palette.png").read_bytes())

    def cli(self, *args, input="", expected=0):
        result = subprocess.run(
            [str(BINARY), *args], input=input, env=self.env,
            capture_output=True, text=True, timeout=45,
        )
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        return result

    def snapshot(self):
        return {p.relative_to(self.root): p.read_bytes()
                for p in self.root.rglob("*") if p.is_file()}

    def plan(self, name="boy"):
        return json.loads(self.cli("plan", name, "--json").stdout)

    def session(self, *args, env=None):
        session = EditSession(BINARY, env or self.env, "edit", *args)
        self.addCleanup(session.close)
        session.wait(ALT_ENTER)
        return session

    def open(self, env=None):
        session = self.session("boy", env=env)
        session.wait(b"Edit Profile boy")
        return session

    def replace(self, session, path=None):
        session.send("\r")
        session.wait(b"Number/relative path")
        session.send(f"path:{path or self.original}\n")
        session.wait(b"Edit Profile boy")
        self.assertNotIn(ALT_LEAVE, session.data)

    def save(self, session):
        session.send("s")
        session.wait(b"Save and use Profile boy?")
        session.send("y")
        session.wait(b"Saved Profile boy.")
        session.wait(b"Ghostty reload: unavailable; Activation remains committed.")
        session.finish()

    def cancel(self, session):
        session.send("q")
        session.wait(b"Cancelled; draft discarded")
        session.finish()

    def assert_exit(self, session, code):
        session.proc.wait(timeout=30)
        while select.select([session.master], [], [], 0)[0]:
            session.data.extend(os.read(session.master, 65536))
        self.assertEqual(session.proc.returncode, code, bytes(session.data[-2000:]))
        self.assertEqual(session.data.count(ALT_ENTER), session.data.count(ALT_LEAVE))
        attrs = termios.tcgetattr(session.slave)
        self.assertTrue(attrs[3] & termios.ICANON and attrs[3] & termios.ECHO)


class EditPublicPath(EditFixture):
    def test_legacy_inline_generated_color_exact_input_reaches_saved_profile_and_activation(self):
        self.profile.write_text(self.profile.read_text()
                                .replace('[colors]\nmode = "generated"\n', '')
                                .replace('schema_version = 1\n', 'schema_version = 1\ncolors = { mode = "generated" }\n'))
        before = self.plan()["environment"]["manifest"]["colors"]
        session = self.open()
        session.send("\t\rh#123456\r")
        self.save(session)
        saved = self.plan()["environment"]["manifest"]["colors"]
        self.assertEqual(saved["background"], "123456",
                         "editor reported Saved/Activated but discarded valid exact-hex customization")
        self.assertEqual(saved["foreground"], before["foreground"])
        self.assertIn("123456", self.profile.read_text())

    def test_guided_create_edit_cancel_decline_confirm_use_and_previous(self):
        self.profile.unlink()
        baseline = (self.root / "current.ghostty").read_bytes()
        created = self.cli("create", "boy", input=f"i\npath:{self.original}\ns\nn\n")
        self.assertIn("Saved Profile boy", created.stdout)
        before = self.snapshot()
        session = self.open()
        session.send("\t\t" + DOWN + "\r18.125\r")
        self.cancel(session)
        self.assertEqual(self.snapshot(), before)
        session = self.session()
        session.wait(b"Select Profile to edit")
        session.send(DOWN + "\r")  # active sorts first, then boy.
        session.wait(b"Edit Profile boy")
        session.send("\t\t" + DOWN + "\r18.125\rs")
        session.wait(b"Save and use Profile boy?")
        session.send("n")
        session.wait(b"Back to editor; draft intact.")
        session.wait(b"18.125")
        self.assertEqual(self.snapshot(), before)
        self.save(session)
        self.assertEqual(self.plan()["environment"]["manifest"]["terminal"]["font_size_millipoints"], 18125)
        history = sorted((self.root / "history/activations").glob("*.json"))
        self.assertEqual(len(history), 2)
        self.assertEqual(json.loads(history[-1].read_text())["profile"]["id"], "boy")
        self.cli("apply", "active")
        self.cli("previous")
        self.assertIn("font-size = 18.125", (self.root / "current.ghostty").read_text())
        self.cli("previous")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), baseline)

    def test_all_color_slots_reset_and_replacement_use_public_resolution_oracle(self):
        self.cli("create", "reference", input=f"i\npath:{self.original}\ns\nn\n")
        reference = self.plan("reference")["environment"]["manifest"]["colors"]
        before = self.snapshot()
        session = self.open()
        session.send("\t")
        customized = {}
        for slot in range(21):
            color = f"{0x123400 + slot:06x}"
            customized[slot] = color
            session.send("\rh" + color + "\r")
            if slot % 2 == 0:
                session.send("\ra")
            if slot != 20:
                session.send(DOWN)
        session.send("s")
        session.wait(b"Save and use Profile boy?")
        session.send("n")
        session.wait(b"Back to editor; draft intact.")
        self.assertEqual(self.snapshot(), before)
        # Tab from Colors through Terminal to Wallpaper; replacement must not reset overrides.
        session.send("\t\t")
        self.replace(session)
        self.assertEqual(self.snapshot(), before)
        self.save(session)
        colors = self.plan()["environment"]["manifest"]["colors"]
        keys = ["background", "foreground", "cursor", "selection_background", "selection_foreground"]
        actual = [colors[key] for key in keys] + colors["palette"]
        automatic = [reference[key] for key in keys] + reference["palette"]
        for slot in range(21):
            self.assertEqual(actual[slot], automatic[slot] if slot % 2 == 0 else customized[slot], f"slot {slot}")
        self.assertIn("schema_version = 2", self.profile.read_text())
        wall = self.plan()["environment"]["manifest"]["wallpaper"]
        self.assertEqual((wall["fit"], wall["position"], wall["repeat"], wall["opacity_millionths"]),
                         ("contain", "bottom-right", True, 234567))
        self.assertEqual(self.original.read_bytes(), (ROOT / "tests/fixtures/palette.png").read_bytes())
        self.assertEqual(len(list((self.root / "history/activations").glob("*.json"))), 2)
        self.cli("previous")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), before[Path("current.ghostty")])

    def test_numeric_controls_choices_decline_and_reopen_preserve_exact_values(self):
        before = self.snapshot()
        session = self.open()
        session.send(DOWN * 2 + RIGHT + DOWN + LEFT + DOWN + RIGHT + DOWN + RIGHT)
        session.send("\t\t\r0.812345\r" + LEFT + DOWN + "\r14.125\r" + RIGHT)
        session.send(DOWN + RIGHT + DOWN + "\r254\r" + RIGHT)
        session.send("s")
        session.wait(b"Save and use Profile boy?")
        session.send("\r")
        session.wait(b"Back to editor; draft intact.")
        self.assertEqual(self.snapshot(), before)
        self.save(session)
        manifest = self.plan()["environment"]["manifest"]
        self.assertEqual(manifest["wallpaper"]["fit"], "cover")
        self.assertEqual(manifest["wallpaper"]["position"], "bottom-center")
        self.assertFalse(manifest["wallpaper"]["repeat"])
        self.assertEqual(manifest["wallpaper"]["opacity_millionths"], 244567)
        self.assertEqual(manifest["terminal"], {
            "background_opacity_millionths": 802345, "font_size_millipoints": 14625,
            "cursor_style": "block", "background_blur_intensity": 255,
        })
        self.assertIn("schema_version = 1", self.profile.read_text())
        saved = self.snapshot()
        session = self.open()
        session.send("\t\t")
        session.wait(b"14.625")
        session.wait(b"0.802345")
        self.cancel(session)
        self.assertEqual(self.snapshot(), saved)

    def test_legacy_without_welcome_unmanaged_colors_replacement_and_explicit_enable(self):
        (self.root / "profiles/welcome.toml").unlink()
        (self.root / "profiles/welcome.png").unlink()
        config = self.root / "config.toml"
        config.write_text(config.read_text().replace("sources.welcome", "sources.local"))
        self.profile.write_text(self.profile.read_text().replace('source = "welcome"', 'source = "local"')
                                .replace('[colors]\nmode = "generated"\n', ''))
        before = self.snapshot()
        session = self.open()
        self.replace(session)
        session.send("\t")
        session.wait(b"Unmanaged")
        self.cancel(session)
        self.assertEqual(self.snapshot(), before)
        session = self.open()
        self.replace(session)
        self.save(session)
        self.assertNotIn("colors", self.plan()["environment"]["manifest"])
        self.assertEqual(config.read_bytes(), before[Path("config.toml")])
        self.assertFalse((self.root / "profiles/welcome.toml").exists())
        session = self.open()
        session.send("\t" + DOWN * 21 + "\r")
        session.wait(b"Reset ALL colors to Automatic?")
        session.send("y")
        self.save(session)
        self.assertIn("colors", self.plan()["environment"]["manifest"])

    def test_theme_colors_survive_replacement_then_single_reset_preserves_other_slots(self):
        themes = self.home / "xdg/ghostty/themes"
        themes.mkdir()
        theme = themes / "QA Theme"
        theme.write_text("background = 112233\nforeground = eeddcc\ncursor-color = 778899\n"
                         "selection-background = 223344\nselection-foreground = aabbcc\n" +
                         "".join(f"palette = {i}={0x112200 + i:06x}\n" for i in range(16)))
        self.profile.write_text(self.profile.read_text().replace('mode = "generated"', 'mode = "theme"\ntheme = "QA Theme"'))
        resolved = self.plan()["environment"]["manifest"]["colors"]
        session = self.open()
        self.replace(session)
        self.save(session)
        self.assertEqual(self.plan()["environment"]["manifest"]["colors"], resolved)
        self.assertIn('mode = "theme"', self.profile.read_text())
        session = self.open()
        session.send("\t" + DOWN * 2 + "\rh#ABCD01\r" + UP * 2 + "\ra")
        self.save(session)
        actual = self.plan()["environment"]["manifest"]["colors"]
        self.assertEqual(actual["cursor"], "abcd01")
        self.assertNotEqual(actual["background"], resolved["background"])
        for key in ["foreground", "palette", "selection_background", "selection_foreground"]:
            self.assertEqual(actual[key], resolved[key])
        self.assertIn('[colors.overrides]', self.profile.read_text())

    def test_invalid_image_retry_and_cancel_then_process_crash_never_publish_draft(self):
        before = self.snapshot()
        broken = self.home / "broken.png"
        broken.write_bytes(b"not a PNG")
        session = self.open()
        session.send("\t\t" + DOWN + "\r15.125\r\t\r")
        session.wait(b"Number/relative path")
        session.send(f"path:{broken}\n")
        session.wait(b"Cannot use")
        self.assertEqual(self.snapshot(), before)
        session.send("cancel\n")
        session.wait(b"Edit Profile boy")
        self.assertNotIn(ALT_LEAVE, session.data)
        session.send("\t\t")
        session.wait(b"15.125")
        session.send("\t")
        self.replace(session)
        session.proc.send_signal(signal.SIGKILL)
        session.proc.wait(timeout=5)
        self.assertEqual(self.snapshot(), before)
        self.assertFalse((self.root / "preview.session").exists())
        session = self.open()
        self.cancel(session)
        self.assertEqual(self.snapshot(), before)

    def test_random_legacy_profile_keeps_opening_selection_through_decline_and_confirmation(self):
        self.profile.write_text(self.profile.read_text()
                                .replace('selection = "path"\npath = "old.png"', 'selection = "random"'))
        before = self.snapshot()
        session = self.open()
        session.send("\t")
        session.wait(b"Foreground")
        background = re.search(r"Background\s+#([0-9a-f]{6}) Automatic", session.screen().decode())
        self.assertIsNotNone(background, session.screen())
        session.send(DOWN * 2 + "\rh#12AB34\rs")
        session.wait(b"Save and use Profile boy?")
        session.send("\x1b")
        session.wait(b"Back to editor; draft intact.")
        session.wait(b"Cursor #12ab34 Customized")
        self.assertEqual(self.snapshot(), before)
        session.send("s")
        session.wait(b"Save and use Profile boy?")
        session.send(RIGHT + "\r")
        session.wait(b"Saved Profile boy.")
        session.wait(b"Ghostty reload: unavailable; Activation remains committed.")
        session.finish()

        history = sorted((self.root / "history/activations").glob("*.json"))
        self.assertEqual(len(history), 2)
        activation = json.loads(history[-1].read_text())
        self.assertEqual(activation["profile"], {"id": "boy", "schema_version": 2})
        self.assertEqual(activation["selection"]["kind"], "random")
        plan = json.loads(self.cli("plan", "boy", "--seed", activation["selection"]["seed"], "--json").stdout)
        self.assertEqual(plan["selection"], activation["selection"])
        record = self.root / "environments" / f'{activation["environment_id"]}.json'
        committed = json.loads(record.read_text())["manifest"]
        self.assertEqual(committed, plan["environment"]["manifest"])
        self.assertEqual(committed["colors"]["background"], background.group(1))
        self.assertEqual(committed["colors"]["cursor"], "12ab34")
        self.assertIn('selection = "random"', self.profile.read_text())
        self.cli("previous")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), before[Path("current.ghostty")])

    def test_invalid_and_future_profiles_fail_before_editor_and_preserve_all_files(self):
        original = self.profile.read_text()
        for document in [
            original.replace('schema_version = 1', 'schema_version = 99'),
            original + '\n[future]\nenabled = true\n',
            original.replace('schema_version = 1', 'schema_version = 2') +
            '\n[colors.overrides]\nbackground = "not-hex"\n',
        ]:
            with self.subTest(document=document):
                self.profile.write_text(document)
                before = self.snapshot()
                result = self.cli("edit", "boy", expected=3)
                self.assertNotIn("interactive terminal", result.stderr)
                self.assertNotIn("Saved Profile", result.stdout)
                self.assertEqual(self.snapshot(), before)

    def test_source_registry_conflict_retains_draft_and_does_not_publish_image(self):
        session = self.open()
        self.replace(session)
        config = self.root / "config.toml"
        config.write_text(config.read_text() + '\n[sources.external]\nkind = "local-directory"\npath = "outside"\n')
        before = self.snapshot()
        session.send("sy")
        session.wait(b"Source registry changed")
        self.assertEqual(self.snapshot(), before)
        self.cancel(session)
        self.assertEqual(self.snapshot(), before)


@unittest.skipUnless(sys.platform == "linux" and shutil.which("cc"),
                     "Ticket 05 publication faults need Linux LD_PRELOAD and a C compiler, not live Ghostty")
class EditPublication(EditFixture):
    @classmethod
    def setUpClass(cls):
        cls.build = tempfile.TemporaryDirectory(prefix="gw-qa-edit-fault-")
        cls.addClassCleanup(cls.build.cleanup)
        source = Path(cls.build.name) / "fault.c"
        cls.shim = Path(cls.build.name) / "fault.so"
        source.write_text(FAULT_SHIM)
        result = subprocess.run(
            [shutil.which("cc"), "-shared", "-fPIC", "-Wall", "-Wextra", "-Werror",
             "-o", str(cls.shim), str(source), "-ldl"],
            env={"PATH": os.defpath}, capture_output=True, text=True, timeout=30,
        )
        if result.returncode:
            raise AssertionError(result.stderr)

    def test_failed_profile_save_rolls_back_only_new_image_and_can_retry_same_draft(self):
        before = self.snapshot()
        session = self.open()
        self.replace(session)
        # Unsafe write permissions fail Profile replacement after the image was published.
        # This filesystem fault works even when the test runner has elevated privileges.
        self.profile.chmod(0o622)
        session.send("sy")
        session.wait(b"was not saved:")
        self.assertEqual(self.snapshot(), before)
        self.profile.chmod(0o600)
        self.save(session)
        candidate = self.plan()["selection"]["candidate"]
        self.assertEqual((self.root / "profiles" / candidate).read_bytes(), self.original.read_bytes())
        self.assertEqual(len(list((self.root / "history/activations").glob("*.json"))), 2)

    def test_failed_profile_save_never_deletes_reused_image(self):
        digest = hashlib.sha256(self.original.read_bytes()).hexdigest()
        reused = self.root / f"profiles/boy-{digest[:16]}.png"
        reused.write_bytes(self.original.read_bytes())
        before = self.snapshot()
        session = self.open()
        self.replace(session)
        self.profile.chmod(0o622)
        session.send("sy")
        session.wait(b"was not saved:")
        self.assertEqual(self.snapshot(), before)
        self.cancel(session)
        self.assertEqual(self.snapshot(), before)
        self.assertTrue(reused.exists())

    def test_post_publication_sync_uncertainty_blocks_retry_and_cannot_claim_unsaved(self):
        for sync_number in [1, 2]:
            with self.subTest(sync_number=sync_number):
                before = self.snapshot()
                session = self.open(env=dict(self.env, LD_PRELOAD=str(self.shim),
                                            GW_QA_SYNC_DIRECTORY=str(self.root / "profiles"),
                                            GW_QA_FAIL_SYNC=str(sync_number)))
                self.replace(session)
                session.send("sy")
                session.wait(b"Publication at")
                session.send("sy")
                session.wait(b"Save blocked:")
                self.assertEqual({k: v for k, v in self.snapshot().items() if k.parts[0] != "profiles"},
                                 {k: v for k, v in before.items() if k.parts[0] != "profiles"})
                self.assertNotIn(b"Saved Profile boy.", session.data)
                session.send("q")
                self.assert_exit(session, 6)
                self.assertIn(b"durability is uncertain", session.data)
                self.assertNotIn(b"Cancelled; draft discarded", session.data)
                self.assertNotIn(b"no files changed", session.data)
                digest = hashlib.sha256(self.original.read_bytes()).hexdigest()
                staged = self.root / f"profiles/boy-{digest[:16]}.png"
                self.assertEqual(staged.read_bytes(), self.original.read_bytes())
                if sync_number == 1:
                    self.assertEqual(self.profile.read_bytes(), before[Path("profiles/boy.toml")])
                    # Restore only this disposable fixture to exercise the second publication point.
                    staged.unlink()
                else:
                    self.assertNotEqual(self.profile.read_bytes(), before[Path("profiles/boy.toml")])
                    self.assertEqual(self.plan()["selection"]["candidate"], staged.name)


if __name__ == "__main__":
    unittest.main()
