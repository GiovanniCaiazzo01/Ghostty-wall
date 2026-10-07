"""Source maintenance acceptance through real PTYs, disposable HOME only."""
import fcntl
import os
import socket
import subprocess
import threading
import unittest
from pathlib import Path
import tui_pty as base


class SourceManagementPty(unittest.TestCase):
    setUp = base.TuiPty.setUp
    cli = base.TuiPty.cli
    session = base.TuiPty.session

    def add_source(self, name="photos", kind="local", location="/old/photos", *options):
        self.cli("source", "add", name, kind, location, *options)

    def open_source(self, session, key, source="photos"):
        session.send("?" + key)
        session.wait_screen(b"Source ID:")
        session.send("\x15" + source + "\r")

    def state(self):
        return {str(p.relative_to(self.managed)): p.read_bytes() for p in self.managed.rglob("*") if p.is_file() and p.name != "config.toml"}

    def test_local_review_retry_decline_save_check_and_removal_stay_inside(self):
        self.add_source()
        config = self.managed / "config.toml"
        before = config.read_bytes()
        state = self.state()
        session = self.session()
        self.open_source(session, "S")
        session.wait_screen(b"Source photos (local)")
        session.wait_screen(b"Profiles: none.")
        session.send("\r")
        session.wait_screen(b"n Create")
        self.open_source(session, "E")
        session.wait_screen(b"Directory path:")
        session.send("\x15\r")
        session.wait_screen(b"must be nonempty")
        self.assertEqual(config.read_bytes(), before)
        session.send("/new/photos\r")
        session.wait_screen(b"Save Source changes")
        session.wait_screen(b"Proposed definition:")
        session.send("\r")  # default Cancel retains fields
        session.wait_screen(b"Directory path:")
        session.wait_screen(b"/new/photos")
        self.assertEqual(config.read_bytes(), before)
        session.send("\r")
        session.wait_screen(b"Save Source changes")
        session.send("y")
        session.wait_screen(b"Updated Source photos")
        self.assertIn(b'/new/photos', config.read_bytes())
        self.assertEqual(self.state(), state)
        self.open_source(session, "C")
        session.wait_screen(b"Check Source")
        session.wait_screen(b"unavailable")
        session.wait_screen(b"Enter/Esc/q back")
        session.send("\r")
        session.wait_screen(b"n Create")
        saved = config.read_bytes()
        self.open_source(session, "Z")
        session.wait_screen(b"Remove Source")
        session.wait_screen(b"Cancel (default)")
        session.send("\r")
        session.wait_screen(b"n Create")
        self.assertEqual(config.read_bytes(), saved)
        self.open_source(session, "Z")
        session.wait_screen(b"Remove Source")
        session.send("y")
        session.wait_screen(b"Removed Source photos")
        self.assertNotIn(b"sources.photos", config.read_bytes())
        self.assertEqual(self.state(), state)
        self.assertNotIn(base.ALT_LEAVE, session.data)
        session.send("q")
        session.finish()

    def test_github_prefills_all_options_and_validates_without_network(self):
        self.add_source("remote", "github", "owner/repo", "--ref", "main", "--path", "wallpapers")
        config = self.managed / "config.toml"
        session = self.session()
        self.open_source(session, "E", "remote")
        session.wait_screen(b"Repository (owner/repo):")
        session.wait_screen(b"owner/repo")
        session.send("\x15invalid\r")
        session.wait_screen(b"Ref (blank for default):")
        session.wait_screen(b"main")
        session.send("\x15feature/sky\r")
        session.wait_screen(b"Subdirectory (blank for repository root):")
        session.wait_screen(b"wallpapers")
        session.send("\x15images\r")
        session.wait_screen(b"owner/repo")  # validation error from repository codec
        session.send("\x1b[Z\x1b[Z\x15other/repo\r\r\r")
        session.wait_screen(b"Save Source changes")
        session.send("y")
        session.wait_screen(b"Updated Source remote")
        text = config.read_text()
        self.assertIn('repository = "other/repo"', text)
        self.assertIn('ref = "feature/sky"', text)
        self.assertIn('path = "images"', text)
        self.open_source(session, "E", "remote")
        session.wait_screen(b"Repository (owner/repo):")
        session.send("\r\x15\r\x15\r")
        session.wait_screen(b"Save Source changes")
        session.send("y")
        session.wait_screen(b"Updated Source remote")
        text = config.read_text()
        self.assertNotIn('ref =', text)
        self.assertNotIn('path = "images"', text)
        session.send("q")
        session.finish()

    def test_stale_edit_preserves_concurrent_configuration(self):
        self.add_source()
        config = self.managed / "config.toml"
        session = self.session()
        self.open_source(session, "E")
        session.wait_screen(b"Directory path:")
        session.send("\x15/new/photos\r")
        session.wait_screen(b"Save Source changes")
        self.add_source("concurrent", "local", "/other")
        concurrent = config.read_bytes()
        session.send("y")
        session.wait_screen(b"configuration changed while editing")
        self.assertEqual(config.read_bytes(), concurrent)
        self.assertNotIn(base.ALT_LEAVE, session.data)
        session.send("\r")
        session.wait_screen(b"n Create")
        session.send("q")
        session.finish()

    def test_new_reference_blocks_confirmed_removal_under_lock(self):
        self.add_source()
        config = self.managed / "config.toml"
        before = config.read_bytes()
        session = self.session()
        self.open_source(session, "Z")
        session.wait_screen(b"Remove Source")
        profile = self.managed / "profiles/new-reference.toml"
        profile.write_text("schema_version = 1\n[wallpaper]\nmode = 'source'\nsource = 'photos'\nselection = 'random'\n")
        session.send("y")
        session.wait_screen(b"used by Profiles: new-reference")
        self.assertEqual(config.read_bytes(), before)
        session.send("\r")
        session.wait_screen(b"n Create")
        self.open_source(session, "Z", "welcome")
        session.wait_screen(b"used by Profiles: welcome")
        self.assertEqual(config.read_bytes(), before)
        session.send("\r")
        session.wait_screen(b"n Create")
        session.send("q")
        session.finish()

    def test_cli_confirmation_detects_stale_config_and_new_references(self):
        self.add_source()
        config = self.managed / "config.toml"
        for mutation in ["config", "profile"]:
            with self.subTest(mutation=mutation):
                session = base.Session(base.BINARY, self.env, "source", "remove", "photos")
                self.addCleanup(session.close)
                session.wait(b"Cancel (default)")
                if mutation == "config":
                    self.add_source("concurrent", "local", "/other")
                    expected = b"configuration changed while editing"
                else:
                    (self.managed / "profiles/concurrent.toml").write_text("schema_version = 1\n[wallpaper]\nmode = 'source'\nsource = 'photos'\nselection = 'random'\n")
                    expected = b"used by Profiles: concurrent"
                before = config.read_bytes()
                session.send("y\r")
                session.wait(expected)
                self.assertEqual(session.proc.wait(timeout=10), 3)
                self.assertEqual(config.read_bytes(), before)

    def test_source_check_needs_no_state_lock_and_remote_cancel_cannot_replace_browser(self):
        with (self.managed / "state.lock").open("r+b") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            self.assertIn("available", self.cli("source", "check", "welcome"))
        self.add_source("remote", "github", "owner/repo")
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
        self.env.update({key: f"http://127.0.0.1:{proxy.getsockname()[1]}" for key in ["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"]})
        self.env.update(NO_PROXY="", no_proxy="")
        state = self.state()
        config = (self.managed / "config.toml").read_bytes()
        session = self.session()
        self.open_source(session, "C", "remote")
        self.assertTrue(connected.wait(10))
        session.send("\x1b")
        session.wait_screen(b"n Create", timeout=3)
        self.open_source(session, "S", "welcome")
        session.wait_screen(b"Source welcome (local)")
        release.set()
        self.assertTrue(finished.wait(5))
        session.wait_screen(b"Source welcome (local)")
        session.send("\r")
        session.wait_screen(b"n Create")
        self.assertEqual(self.state(), state)
        self.assertEqual((self.managed / "config.toml").read_bytes(), config)
        self.assertNotIn(base.ALT_LEAVE, session.data)
        session.send("q")
        session.finish()

    def test_source_edit_invalidates_selected_profile_preview_without_applying(self):
        directory = Path(self.tmp.name) / "pictures"
        directory.mkdir()
        (directory / "sky.png").write_bytes((base.ROOT / "tests/fixtures/white.png").read_bytes())
        self.add_source("photos", "local", str(directory))
        self.cli("new", "alpha", "--source", "photos", "--path", "sky.png")
        self.cli("apply", "welcome")
        state = self.state()
        session = self.session()
        session.wait_screen(b"Cursor")
        self.open_source(session, "E")
        session.wait_screen(b"Directory path:")
        session.send("\x15" + str(directory / "missing") + "\r")
        session.wait_screen(b"Save Source changes")
        session.send("y")
        session.wait_screen(b"Updated Source photos")
        session.wait_screen(b"filesystem error")
        session.wait_screen(b"missing")
        self.assertEqual(self.state(), state)
        self.open_source(session, "E")
        session.wait_screen(b"Directory path:")
        session.send("\x15" + str(directory) + "\r")
        session.wait_screen(b"Save Source changes")
        session.send("y")
        session.wait_screen(b"Updated Source photos")
        session.wait_screen(b"Cursor")
        self.assertEqual(self.state(), state)
        session.send("q")
        session.finish()

    def test_tui_source_inspection_and_check_do_not_wait_for_mutation_lock(self):
        session = self.session()
        with (self.managed / "state.lock").open("r+b") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            self.open_source(session, "S", "welcome")
            session.wait_screen(b"Source welcome (local)", timeout=3)
            session.send("\r")
            session.wait_screen(b"n Create", timeout=3)
            self.open_source(session, "C", "welcome")
            session.wait_screen(b"available", timeout=3)
            session.wait_screen(b"Enter/Esc/q back", timeout=3)
            session.send("\r")
            session.wait_screen(b"n Create", timeout=3)
        session.send("q")
        session.finish()


if __name__ == "__main__":
    unittest.main()
