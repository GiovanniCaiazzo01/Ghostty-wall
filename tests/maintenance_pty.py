"""Maintenance containment through the public TUI in disposable HOME/XDG."""
import fcntl
import hashlib
import os
import shutil
import socket
import struct
import termios
import threading
import unittest
from pathlib import Path
import tui_pty as base


class MaintenancePty(unittest.TestCase):
    setUp = base.TuiPty.setUp
    cli = base.TuiPty.cli
    session = base.TuiPty.session
    def test_destructive_actions_require_consent_and_errors_stay_inside(self):
        config = (self.managed / "config.toml").read_bytes()
        session = self.session()
        for action in ["U", "X", "p", "M", "R", "I", "W"]:
            session.send("?" + action)
            session.wait_screen(b"Cancel (default)", timeout=5)
            self.assertNotIn(base.ALT_LEAVE, session.data)
            session.send("\r")
            session.wait_screen(b"n Create")
        session.send("?p")
        session.wait_screen(b"Cancel (default)")
        session.send("y")
        session.wait_screen(b"Failed:")
        session.wait_screen(b"Enter/Esc/q back")
        self.assertNotIn(base.ALT_LEAVE, session.data)
        session.send("\r")
        session.wait_screen(b"n Create")
        session.send("q")
        session.finish()
        self.assertEqual((self.managed / "config.toml").read_bytes(), config)
        self.assertFalse(list((self.managed / "history/activations").glob("*.json")))

    def test_update_progress_can_close_check_but_not_abort_install(self):
        for install in [False, True]:
            with self.subTest(install=install):
                proxy = socket.socket()
                proxy.bind(("127.0.0.1", 0))
                proxy.listen()
                proxy.settimeout(15)
                self.addCleanup(proxy.close)
                connected, release, finished = threading.Event(), threading.Event(), threading.Event()
                def serve():
                    try:
                        conn, _ = proxy.accept()
                        with conn:
                            conn.recv(4096)
                            connected.set()
                            release.wait(15)
                            conn.sendall(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
                    finally:
                        finished.set()
                threading.Thread(target=serve, daemon=True).start()
                self.addCleanup(release.set)
                prefix = Path(self.tmp.name) / ("install-fixture" if install else "check-fixture")
                prefix.mkdir()
                binary = prefix / "ghostty-wall"
                shutil.copy2(base.BINARY, binary)
                before = hashlib.sha256(binary.read_bytes()).digest()
                self.env.update({key: "http://127.0.0.1:" + str(proxy.getsockname()[1]) for key in ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"]})
                self.env.update(NO_PROXY="", no_proxy="")
                session = base.Session(binary, self.env, "tui")
                self.addCleanup(session.close)
                session.wait_screen(b"n Create")
                session.send("?U" if install else "?u")
                if install:
                    session.wait_screen(b"Cancel (default)")
                    session.send("y")
                self.assertTrue(connected.wait(10), "updater never reached controlled proxy")
                session.wait_screen(b"Checking latest stable")
                session.send("\x03")
                if install:
                    session.wait_screen(b"Cancellation unavailable", timeout=3)
                    self.assertIsNone(session.proc.poll())
                    release.set()
                    session.wait_screen(b"Failed:")
                    session.wait_screen(b"Enter/Esc/q back")
                    session.send("\r")
                else:
                    session.wait_screen(b"n Create", timeout=3)
                    release.set()
                session.wait_screen(b"n Create")
                self.assertNotIn(base.ALT_LEAVE, session.data)
                session.send("q")
                session.finish()
                self.assertTrue(finished.wait(5))
                self.assertEqual(hashlib.sha256(binary.read_bytes()).digest(), before)

    def test_migration_dry_run_decline_success_and_collision_preserve_legacy_files(self):
        legacy = self.managed.parent / "wallpaper_repos.txt"
        legacy.write_text("landscape|owner/repo|main|wallpapers\n")
        config = self.managed / "config.toml"
        before = config.read_bytes()
        session = self.session()
        session.send("?Y")
        session.wait_screen(b"Would import 1 Source(s)")
        session.send("\r")
        session.wait_screen(b"n Create")
        self.assertEqual(config.read_bytes(), before)
        session.send("?M")
        session.wait_screen(b"Cancel (default)")
        session.send("\x1b")
        session.wait_screen(b"n Create")
        self.assertEqual(config.read_bytes(), before)
        session.send("?M")
        session.wait_screen(b"Cancel (default)")
        session.send("y")
        session.wait_screen(b"Migrated 1 Source(s)")
        session.send("\r")
        session.wait_screen(b"n Create")
        migrated = config.read_bytes()
        self.assertIn(b'owner/repo', migrated)
        self.assertTrue(legacy.is_file())
        legacy.write_text("landscape|other/repo|main|\n")
        session.send("?M")
        session.wait_screen(b"Cancel (default)")
        session.send("y")
        session.wait_screen(b"Failed:")
        session.send("\r")
        session.wait_screen(b"n Create")
        self.assertNotIn(base.ALT_LEAVE, session.data)
        session.send("q")
        session.finish()
        self.assertEqual(config.read_bytes(), migrated)
        self.assertEqual(legacy.read_text(), "landscape|other/repo|main|\n")

    def test_source_validation_retry_and_cancel_preserve_configuration(self):
        config = self.managed / "config.toml"
        before = config.read_bytes()
        session = self.session()
        session.send("o")
        session.wait_screen(b"New Source ID:")
        session.send("sample\rinvalid\r/tmp/photos\r")
        session.wait_screen(b"Source kind must be local or github")
        self.assertEqual(config.read_bytes(), before)
        session.send("\x1b[Z\x15local\r\r")
        session.wait_screen(b"Added Source sample.")
        saved = config.read_bytes()
        session.send("o")
        session.wait_screen(b"New Source ID:")
        session.send("sample\rlocal\r/tmp/different\r")
        session.wait_screen(b"already differs; no files changed")
        session.send("\x04")
        session.wait_screen(b"n Create")
        self.assertNotIn(base.ALT_LEAVE, session.data)
        session.send("q")
        session.finish()
        self.assertEqual(config.read_bytes(), saved)

    def test_welcome_creation_for_old_empty_installation_requires_consent(self):
        config = self.managed / "config.toml"
        config.write_text("schema_version = 1\n\n[sources]\n")
        (self.managed / "profiles/welcome.toml").unlink()
        (self.managed / "profiles/welcome.png").unlink()
        session = self.session()
        session.send("?W")
        session.wait_screen(b"Cancel (default)")
        session.send("n")
        session.wait_screen(b"n Create")
        self.assertFalse((self.managed / "profiles/welcome.toml").exists())
        session.send("?W")
        session.wait_screen(b"Cancel (default)")
        session.send("y")
        session.wait_screen(b"Completed")
        session.send("\r")
        session.wait_screen(b"n Create")
        session.wait_screen(b"welcome")
        self.assertNotIn(base.ALT_LEAVE, session.data)
        session.send("q")
        session.finish()
        self.assertIn('"id":"welcome"', self.cli("plan", "welcome", "--json"))
        self.assertFalse(list((self.managed / "history/activations").glob("*.json")))

    def test_initialization_and_migration_actions_report_without_leaving(self):
        config = (self.managed / "config.toml").read_bytes()
        session = self.session()
        for action in ["y", "Y", "I", "R", "W", "M"]:
            session.send("?" + action)
            if action in "IRWM":
                session.wait_screen(b"Cancel (default)")
                session.send("y")
            session.wait_screen(b"Enter/Esc/q back")
            self.assertNotIn(base.ALT_LEAVE, session.data)
            session.send("\r")
            session.wait_screen(b"n Create")
        session.send("q")
        session.finish()
        self.assertEqual((self.managed / "config.toml").read_bytes(), config)
        self.assertFalse(list((self.managed / "history/activations").glob("*.json")))

    def test_fresh_start_dry_run_cancel_and_failure_are_contained(self):
        shutil.rmtree(self.managed)
        session = base.Session(base.BINARY, self.env, "tui")
        self.addCleanup(session.close)
        session.wait_screen(b"Not initialized")
        session.send("y")
        session.wait_screen(b"Completed")
        self.assertFalse(self.managed.exists())
        session.send("\r")
        session.wait_screen(b"Not initialized")
        session.send("i")
        session.wait_screen(b"Cancel (default)")
        session.send("\x04")
        session.wait_screen(b"Not initialized")
        self.assertFalse(self.managed.exists())
        # A non-directory managed root forces init to fail, without touching user data.
        self.managed.write_text("do not overwrite")
        session.send("i")
        session.wait_screen(b"Cancel (default)")
        session.send("y")
        session.wait_screen(b"Failed:")
        session.send("\r")
        session.wait_screen(b"Not initialized")
        self.assertNotIn(base.ALT_LEAVE, session.data)
        session.send("q")
        session.finish()
        self.assertEqual(self.managed.read_text(), "do not overwrite")

    def test_github_source_options_retry_cancel_and_configuration_report(self):
        session = self.session()
        session.send("o")
        session.wait_screen(b"New Source ID:")
        session.send("remote\rgithub\rowner/repo\r")
        session.wait_screen(b"Ref (blank for default):")
        self.assertNotIn(base.ALT_LEAVE, session.data)
        session.send("\x1b")
        session.wait_screen(b"New Source ID:")
        session.send("\r\r\r")
        session.wait_screen(b"Ref (blank for default):")
        session.send("main\rwallpapers\r")
        session.wait_screen(b"Added Source remote.")
        session.send("?s")
        session.wait_screen(b"Settings:")
        session.send("\x1b[6~")
        session.wait_screen(b"owner/repo")
        session.send("\r")
        session.wait_screen(b"n Create")
        self.assertNotIn(base.ALT_LEAVE, session.data)
        session.send("q")
        session.finish()
        text = (self.managed / "config.toml").read_text()
        self.assertIn('repository = "owner/repo"', text)
        self.assertIn('ref = "main"', text)
        self.assertIn('path = "wallpapers"', text)

    def test_blocked_plan_and_image_can_be_cancelled_without_waiting_for_profile_io(self):
        for action in ["P", "i"]:
            with self.subTest(action=action):
                session = self.session()
                session.wait_screen(b"Cursor")
                profile = self.managed / "profiles/welcome.toml"
                original = profile.read_bytes()
                profile.unlink()
                os.mkfifo(profile)
                try:
                    session.send("?P" if action == "P" else "i")
                    session.wait_screen(b"Working" if action == "P" else b"Loading wallpaper image", timeout=3)
                    session.send("\x1b")
                    session.wait_screen(b"n Create", timeout=3)
                    session.send("?D")
                    session.wait_screen(b"Previous read-only operation is still finishing", timeout=3)
                    session.send("\r")
                    session.wait_screen(b"n Create")
                    self.assertNotIn(base.ALT_LEAVE, session.data)
                    session.send("q")
                    session.finish()
                finally:
                    if session.proc.poll() is None:
                        session.proc.kill()
                        session.proc.wait()
                    # Blocked read-only workers die with this disposable process.
                    profile.unlink()
                    profile.write_bytes(original)

    def test_image_graphics_resize_and_details_stay_fullscreen_and_read_only(self):
        self.cli("new", "aaa", str(base.ROOT / "tests/fixtures/white.png"))
        self.env["TERM_PROGRAM"] = "ghostty"
        session = self.session()
        session.send("i")
        session.wait(b"i=43,C=1,c=100,r=26")
        self.assertNotIn(base.ALT_LEAVE, session.data)
        fcntl.ioctl(session.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 40, 0, 0))
        session.send("\x00")
        session.wait(b"i=43,C=1,c=40,r=8")
        fcntl.ioctl(session.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 7, 25, 0, 0))
        session.send("\x00")
        session.wait_screen(b"Resize to 40x12", timeout=3)
        fcntl.ioctl(session.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 12, 40, 0, 0))
        session.send("\x00")
        session.wait(b"i=43,C=1,c=40,r=8", 2)
        session.send("\x1b")
        session.wait_screen(b"Resize management to 60x18.")
        self.assertNotIn(base.ALT_LEAVE, session.data)
        fcntl.ioctl(session.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
        session.send("\x00")
        session.wait_screen(b"n Create")
        session.send("v")
        session.wait_screen(b"Enter/Esc/q back")
        session.send("\x1b[6~\r")
        session.wait_screen(b"n Create")
        self.assertNotIn(base.ALT_LEAVE, session.data)
        session.send("q")
        session.finish()
        self.assertIn(b"a=d,d=I,i=43", session.data)
        self.assertFalse(list((self.managed / "history/activations").glob("*.json")))

    def test_reports_stay_fullscreen_and_scroll(self):
        session = self.session()
        session.send("?P")
        session.wait_screen(b'"asset":')
        self.assertNotIn(base.ALT_LEAVE, session.data)
        session.wait_screen(b"Enter/Esc/q back")
        session.send("\x1b[6~")
        session.wait_screen(b'"palette":')
        session.send("\r")
        session.wait_screen(b"n Create")
        for action, text in [("h", b"History"), ("s", b"Settings"), ("l", b"welcome"), ("D", b"managed-layout:")]:
            session.send("?" + action)
            session.wait_screen(text)
            session.wait_screen(b"Enter/Esc/q back")
            self.assertNotIn(base.ALT_LEAVE, session.data)
            session.send("\r")
            session.wait_screen(b"n Create")
        session.send("q")
        session.finish()
        self.assertEqual(session.data.count(base.ALT_ENTER), 1)
        self.assertFalse(list((self.managed / "history/activations").glob("*.json")))


if __name__ == "__main__":
    unittest.main()
