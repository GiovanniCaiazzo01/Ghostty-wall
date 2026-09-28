"""Read-only automatic preview regressions; all application files live in disposable HOME."""
import errno
import fcntl
import os
import struct
import termios
import time

from management_pty import ManagementPty


class ResponsivePreviewPty(ManagementPty):
    def blocked_session(self):
        (self.root / "profiles/welcome.toml").write_text('schema_version = 1\n[wallpaper]\nmode = "none"\n')
        session = self.session()
        session.wait(b"Internal sample")
        profile = self.root / "profiles/boy.toml"
        document = profile.read_bytes()
        profile.unlink()
        os.mkfifo(profile)
        session.send("j")
        session.wait(b"Preview: welcome")
        session.send("k")
        session.wait(b"Preview: boy")
        deadline = time.monotonic() + 40
        while True:
            try:
                writer = os.fdopen(os.open(profile, os.O_WRONLY | os.O_NONBLOCK), "wb", buffering=0)
                break
            except OSError as error:
                if error.errno != errno.ENXIO or time.monotonic() > deadline:
                    raise
                time.sleep(0.01)
        self.addCleanup(writer.close)
        return session, writer, document

    def test_image_and_source_errors_stay_in_panel_and_leave_navigation_usable(self):
        empty = self.home / "empty-source"
        empty.mkdir()
        self.cli("source", "add", "empty", "local", str(empty))
        (self.root / "profiles/boy.png").write_bytes(b"corrupt image")
        for source, selection, expected in [
            ("welcome", 'selection = "path"\npath = "boy.png"', b"selected asset is not PNG or JPEG"),
            ("welcome", 'selection = "path"\npath = "missing.png"', b"filesystem error"),
            ("empty", 'selection = "random"', b"empty Candidate Set"),
            ("empty", 'selection = "random"', b"filesystem error"),
        ]:
            with self.subTest(source=source, expected=expected):
                if source == "empty" and expected == b"filesystem error":
                    empty.rmdir()
                (self.root / "profiles/boy.toml").write_text(f'schema_version = 1\n[wallpaper]\nmode = "source"\nsource = "{source}"\n{selection}\n')
                before = self.snapshot()
                session = self.session()
                session.wait(expected)
                session.send("j")
                session.wait(b"Preview: welcome")
                self.finish(session)
                self.assertEqual(self.snapshot(), before)

    def test_random_profile_automatically_previews_without_cli_seed_read_only(self):
        (self.root / "profiles/boy.toml").write_text('schema_version = 1\n[wallpaper]\nmode = "source"\nsource = "welcome"\nselection = "random"\nopacity = 0.3\n[colors]\nmode = "generated"\n')
        before = self.snapshot()
        session = self.session()
        session.wait(b"Selected text")
        session.wait(b"Cursor")
        self.finish(session)
        self.assertEqual(self.snapshot(), before)

    def test_selected_image_change_invalidates_preview_without_navigation(self):
        (self.root / "profiles/boy.toml").write_text('schema_version = 1\n[wallpaper]\nmode = "source"\nsource = "welcome"\nselection = "path"\npath = "boy.png"\n[colors]\nmode = "generated"\n')
        session = self.session()
        session.wait(b"Internal sample")
        (self.root / "profiles/boy.png").write_bytes(b"corrupt image")
        session.wait(b"selected asset is not PNG or JPEG", timeout=3)
        self.finish(session)

    def test_selected_profile_change_invalidates_preview_without_navigation(self):
        session = self.session()
        session.wait(b"Internal sample")
        (self.root / "profiles/boy.toml").write_text("schema_version = 999\n")
        session.wait(b"unsupported", timeout=3)
        self.finish(session)

    def test_ghostty_receives_sized_graphics_automatically_and_cleans_up(self):
        self.env["TERM_PROGRAM"] = "ghostty"
        before = self.snapshot()
        session = self.session()
        session.wait(b"\x1b_Ga=T,f=100,i=42", timeout=30)
        time.sleep(0.3)
        session.send("p")  # wide-layout repaint, not a new resolution
        self.finish(session)
        self.assertEqual(session.data.count(b"\x1b_Ga=T,f=100,i=42"), 1)
        self.assertIn(b"z=-1", session.data)
        self.assertIn(b"\x1b_Ga=d,d=I,i=42", session.data)
        self.assertEqual(self.snapshot(), before)

    def test_undersized_terminal_retains_resize_guidance_and_exit(self):
        before = self.snapshot()
        session = self.session()
        fcntl.ioctl(session.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 8, 30, 0, 0))
        session.wait(b"Resize management to 60x18")
        session.send("\x1b")
        session.finish()
        self.assertEqual(self.snapshot(), before)

    def test_compact_selection_automatically_shows_sample_without_preview_key(self):
        before = self.snapshot()
        session = self.session()
        fcntl.ioctl(session.slave, termios.TIOCSWINSZ, struct.pack("HHHH", 18, 60, 0, 0))
        session.wait(b"Cursor")
        session.wait(b"Preview: boy")
        self.finish(session)
        self.assertEqual(self.snapshot(), before)

    def test_blocked_preparation_does_not_block_selection_or_exit(self):
        session, _, _ = self.blocked_session()
        session.send("j")
        session.wait(b"Preview: welcome", timeout=2)
        session.send("q")
        session.proc.wait(timeout=2)
        session.finish()

    def test_key_burst_coalesces_obsolete_pending_work(self):
        session, writer, document = self.blocked_session()
        session.send("jk" * 50 + "j?")
        session.wait(b"Create Profile")  # all preceding keys have been consumed
        session.send("\x1b")
        session.wait(b"Preview: welcome")
        writer.write(document)
        writer.close()
        # boy remains a FIFO: replaying any obsolete boy request would block again.
        session.wait(b"Colors unmanaged", timeout=5)
        self.finish(session)

    def test_obsolete_result_cannot_replace_latest_selection(self):
        session, writer, document = self.blocked_session()
        session.send("j")
        session.wait(b"Preview: welcome", timeout=2)
        writer.write(document)
        # Restore the path and close the writer to finish A, after B was selected.
        profile = self.root / "profiles/boy.toml"
        profile.unlink()
        profile.write_bytes(document)
        writer.close()
        session.wait(b"Colors unmanaged", timeout=40)
        self.assertIn(b"Preview: welcome", session.screen())
        self.finish(session)


# Reuse fixture helpers without duplicating the unrelated management/form tests.
for name in list(vars(ManagementPty)):
    if name.startswith("test_"):
        setattr(ResponsivePreviewPty, name, None)
del ManagementPty
