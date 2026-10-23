"""Use commits reviewed content, never an unreviewed/loading resolution."""
import fcntl
import json
import time

from edit_pty import EditSession
from responsive_preview_pty import ResponsivePreviewPty as _Fixture
from tui_pty import BINARY


class ReviewedPreviewPty(_Fixture):
    def activations(self):
        return sorted((self.root / "history/activations").glob("*.json"))

    def test_use_while_loading_does_not_resolve_or_block_navigation(self):
        session, writer, _ = self.blocked_session()
        session.send("a")
        session.wait(b"Wait for current preview", timeout=2)
        self.assertEqual(len(self.activations()), 1)
        self.finish(session)
        self.assertFalse(writer.closed)

    def test_fixed_and_random_use_commit_the_reviewed_manifest(self):
        for random in (False, True):
            with self.subTest(random=random):
                profile = self.root / "profiles/boy.toml"
                if random:
                    profile.write_text('schema_version = 1\n[wallpaper]\nmode = "source"\nsource = "welcome"\nselection = "random"\nopacity = 0.05\n[colors]\nmode = "generated"\n')
                seed = "04" * 32
                args = ["plan", "boy", "--json"] + (["--seed", seed] if random else [])
                expected = json.loads(self.cli(*args))
                session = EditSession(BINARY, self.env, "tui", "--seed", seed)
                self.addCleanup(session.close)
                session.wait(b"Selected text")
                before = len(self.activations())
                session.send("a")
                session.wait(b"Configuration updated")
                records = self.activations()
                self.assertEqual(len(records), before + 1)
                actual = json.loads(records[-1].read_text())
                self.assertEqual(actual["environment_id"], expected["environment"]["environment_id"])
                self.assertEqual(actual["asset"], expected["asset"])
                self.finish(session)

    def test_named_theme_content_is_captured_not_resolved_again_on_use(self):
        themes = self.home / "config/ghostty/themes"
        themes.mkdir(parents=True)
        theme = themes / "Reviewed"
        theme.write_text("background = 111111\nforeground = eeeeee\n" + "".join(
            f"palette = {i}=eeeeee\n" for i in range(16)
        ))
        profile = self.root / "profiles/boy.toml"
        profile.write_text(profile.read_text().replace('mode = "generated"', 'mode = "theme"\ntheme = "Reviewed"'))
        expected = json.loads(self.cli("plan", "boy", "--json"))
        session = self.session()
        session.wait(b"Cursor")
        theme.write_text(theme.read_text().replace("eeeeee", "aaaaaa"))
        session.send("a")
        session.wait(b"Configuration updated")
        actual = json.loads(self.activations()[-1].read_text())
        self.assertEqual(actual["environment_id"], expected["environment"]["environment_id"])
        self.assertEqual(actual["color_resolution"], expected["color_resolution"])
        self.finish(session)

    def test_failed_preview_cannot_apply(self):
        (self.root / "profiles/boy.png").write_bytes(b"corrupt image")
        session = self.session()
        session.wait(b"Preview unavailable:")
        session.send("a")
        session.wait(b"Wait for current preview")
        self.assertEqual(len(self.activations()), 1)
        self.finish(session)

    def test_profile_change_while_waiting_for_lock_requires_fresh_review(self):
        session = self.session()
        session.wait(b"Selected text")
        profile = self.root / "profiles/boy.toml"
        with (self.root / "state.lock").open("rb") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            session.send("a")
            time.sleep(0.3)
            profile.write_text(profile.read_text() + "\n[terminal]\nfont_size = 18\n")
            fcntl.flock(lock, fcntl.LOCK_UN)
        session.wait(b"Preview changed; review refreshed sample", timeout=5)
        self.assertEqual(len(self.activations()), 1)
        session.wait(b"Selected text")
        session.send("a")
        session.wait(b"Configuration updated")
        self.assertEqual(len(self.activations()), 2)
        self.finish(session)


# Inherit disposable fixtures, not the unrelated browser tests.
for _name in dir(_Fixture):
    if _name.startswith("test_") and _name not in ReviewedPreviewPty.__dict__:
        setattr(ReviewedPreviewPty, _name, None)
del _Fixture
