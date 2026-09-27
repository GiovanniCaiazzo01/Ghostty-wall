"""Ticket 06 public-CLI/PTY fault and consent tests; no real Ghostty is contacted.

Run: cargo build --locked && python3 -m unittest discover -s tests -p qa_delete_safety.py -v
The Linux shim affects only a disposable child and explicitly named test paths.
"""

import fcntl
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

from tui_pty import BINARY, ROOT, Session


SHIM = r"""
#define _GNU_SOURCE
#include <dlfcn.h>
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/file.h>
#include <unistd.h>

static int fd_path(int fd, char target[PATH_MAX]) {
    char label[64];
    snprintf(label, sizeof label, "/proc/self/fd/%d", fd);
    ssize_t n = readlink(label, target, PATH_MAX - 1);
    if (n < 0) return 0;
    target[n] = '\0';
    return 1;
}

static int fd_matches(int fd, const char *expected) {
    char target[PATH_MAX];
    return expected && fd_path(fd, target) && !strcmp(target, expected);
}

/* Only map a private workspace in this fixture's Managed Root, never arbitrary directories. */
static int workspace_for(int fd, const char *profiles) {
    char target[PATH_MAX], prefix[PATH_MAX];
    if (!profiles || !fd_path(fd, target)) return 0;
    const char *base = strrchr(profiles, '/');
    if (!base || strcmp(base, "/profiles")) return 0;
    int n = snprintf(prefix, sizeof prefix, "%.*s/.tmp-delete-", (int)(base - profiles), profiles);
    return n > 0 && n < (int)sizeof prefix && !strncmp(target, prefix, n) &&
        strlen(target + n) == 32 && strspn(target + n, "0123456789abcdef") == 32;
}

static void mark(const char *env, const char *message) {
    const char *path = getenv(env);
    if (!path) return;
    int fd = open(path, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0600);
    if (fd < 0 || write(fd, message, strlen(message)) < 0) _exit(120);
    close(fd);
}

/* Publish after the renewed proof's directory snapshot, without modifying private state.
 * The replacement must still be caught by the public-name revalidation. */
struct dirent *readdir(DIR *stream) {
    static struct dirent *(*real_readdir)(DIR *);
    static unsigned completed_scans;
    if (!real_readdir) real_readdir = dlsym(RTLD_NEXT, "readdir");
    struct dirent *entry = real_readdir(stream);
    int saved_errno = errno;
    const char *parent = getenv("GW_QA_SCAN_PARENT");
    if (!entry && !saved_errno && parent && fd_matches(dirfd(stream), parent) &&
        ++completed_scans == 2) {
        if (renameat(AT_FDCWD, getenv("GW_QA_SCAN_REPLACEMENT"),
                     dirfd(stream), "boy.toml")) _exit(128);
        mark("GW_QA_FAULT_SEEN", "ownership-scan-replacement\n");
    }
    errno = saved_errno;
    return entry;
}

int fsync(int fd) {
    static int (*real_fsync)(int);
    static unsigned matches;
    if (!real_fsync) real_fsync = dlsym(RTLD_NEXT, "fsync");
    const char *count = getenv("GW_QA_SYNC_NUMBER");
    if (count && (fd_matches(fd, getenv("GW_QA_SYNC_PATH")) ||
                  workspace_for(fd, getenv("GW_QA_PRIVATE_SYNC_PARENT"))) &&
        ++matches == strtoul(count, NULL, 10)) {
        mark("GW_QA_FAULT_SEEN", "fsync\n");
        if (getenv("GW_QA_CRASH_AFTER_SYNC")) {
            if (real_fsync(fd)) _exit(122);
            _exit(121);
        }
        errno = EIO;
        return -1;
    }
    return real_fsync(fd);
}

static void swap_profiles_before_unlink(const char *path) {
    static int swapped;
    const char *directory = getenv("GW_QA_SWAP_DIR");
    const char *name = strrchr(path, '/');
    name = name ? name + 1 : path;
    if (!directory || swapped || strcmp(name, "boy.toml")) return;
    if (rename(directory, getenv("GW_QA_SAVED_DIR")) ||
        symlink(getenv("GW_QA_OUTSIDE_DIR"), directory)) _exit(123);
    swapped = 1;
    mark("GW_QA_FAULT_SEEN", "directory-swap\n");
}

int unlink(const char *path) {
    swap_profiles_before_unlink(path);
    static int (*real_unlink)(const char *);
    if (!real_unlink) real_unlink = dlsym(RTLD_NEXT, "unlink");
    const char *target = getenv("GW_QA_UNLINK_PATH");
    if (target && !strcmp(target, path)) {
        mark("GW_QA_FAULT_SEEN", "unlink\n");
        errno = EACCES;
        return -1;
    }
    return real_unlink(path);
}

/* Keep the original public-name substitution oracle at both implementations' capture
 * boundary: unlinkat for the old protocol, renameat2 for isolated removal. */
static void replace_entry_before_unlink(int dirfd, const char *path) {
    static int replaced;
    const char *name = getenv("GW_QA_REPLACE_NAME");
    if (!name || replaced || strcmp(name, path) ||
        !fd_matches(dirfd, getenv("GW_QA_REPLACE_PARENT"))) return;
    if (renameat(AT_FDCWD, getenv("GW_QA_REPLACEMENT"), dirfd, path)) _exit(124);
    replaced = 1;
    mark("GW_QA_FAULT_SEEN", "entry-replacement\n");
}

int renameat2(int oldfd, const char *old, int newfd, const char *new, unsigned flags) {
    static int (*real_renameat2)(int, const char *, int, const char *, unsigned);
    if (!real_renameat2) real_renameat2 = dlsym(RTLD_NEXT, "renameat2");
    swap_profiles_before_unlink(old);
    if (!getenv("GW_QA_REPLACE_AT_ISOLATED_UNLINK") &&
        !getenv("GW_QA_REPLACE_AT_WORKSPACE_CLEANUP"))
        replace_entry_before_unlink(oldfd, old);
    const char *parent = getenv("GW_QA_REPLACE_PARENT");
    if (getenv("GW_QA_RESTORE_CONFLICT") && workspace_for(oldfd, parent) &&
        fd_matches(newfd, parent)) {
        if (renameat(AT_FDCWD, getenv("GW_QA_RESTORE_CONFLICT"), newfd, new)) _exit(125);
        mark("GW_QA_FAULT_SEEN", "restore-conflict\n");
    }
    if (getenv("GW_QA_DETACH_FAIL") &&
        fd_matches(oldfd, getenv("GW_QA_DETACH_PARENT"))) {
        mark("GW_QA_FAULT_SEEN", "rename\n");
        errno = EOPNOTSUPP;
        return -1;
    }
    int result = real_renameat2(oldfd, old, newfd, new, flags);
    const char *pause_parent = getenv("GW_QA_PAUSE_PARENT");
    if (!result && pause_parent && !strcmp(old, "boy.toml") &&
        fd_matches(oldfd, pause_parent) && workspace_for(newfd, pause_parent)) {
        mark("GW_QA_PAUSE_READY", "captured\n");
        unsigned remaining = 3000;
        while (access(getenv("GW_QA_PAUSE_RELEASE"), F_OK)) {
            if (!remaining--) _exit(129);
            usleep(10000);
        }
    }
    const char *detach_name = getenv("GW_QA_DETACH_NAME");
    if (!result && getenv("GW_QA_CRASH_AFTER_DETACH") &&
        (!detach_name || !strcmp(detach_name, old)) &&
        workspace_for(newfd, getenv("GW_QA_DETACH_PARENT"))) {
        if (fsync(newfd)) _exit(122);
        mark("GW_QA_FAULT_SEEN", "detached\n");
        _exit(121);
    }
    return result;
}

/* Publish only at the public name, while isolated removal cleans its empty workspace. */
static void replace_during_workspace_cleanup(int dirfd, const char *path, int flags) {
    static int replaced;
    const char *parent = getenv("GW_QA_REPLACE_PARENT");
    const char *name = getenv("GW_QA_REPLACE_NAME");
    if (replaced || flags != AT_REMOVEDIR || !parent || !name ||
        !getenv("GW_QA_REPLACE_AT_WORKSPACE_CLEANUP") ||
        strncmp(path, ".tmp-delete-", 12)) return;
    const char *base = strrchr(parent, '/');
    char root[PATH_MAX];
    if (!base || strcmp(base, "/profiles")) return;
    int n = snprintf(root, sizeof root, "%.*s", (int)(base - parent), parent);
    if (n <= 0 || n >= (int)sizeof root || !fd_matches(dirfd, root)) return;
    int profiles = open(parent, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC);
    if (profiles < 0 ||
        renameat(AT_FDCWD, getenv("GW_QA_REPLACEMENT"), profiles, name)) _exit(127);
    close(profiles);
    replaced = 1;
    mark("GW_QA_FAULT_SEEN", "cleanup-entry-replacement\n");
}

int unlinkat(int dirfd, const char *path, int flags) {
    replace_during_workspace_cleanup(dirfd, path, flags);
    swap_profiles_before_unlink(path);
    replace_entry_before_unlink(dirfd, path);
    const char *parent = getenv("GW_QA_REPLACE_PARENT");
    const char *name = getenv("GW_QA_REPLACE_NAME");
    if (getenv("GW_QA_REPLACE_AT_ISOLATED_UNLINK") && name && !strcmp(name, path) &&
        workspace_for(dirfd, parent)) {
        char target[PATH_MAX];
        int n = snprintf(target, sizeof target, "%s/%s", parent, path);
        if (n < 0 || n >= (int)sizeof target ||
            renameat(AT_FDCWD, getenv("GW_QA_REPLACEMENT"), AT_FDCWD, target)) _exit(126);
        mark("GW_QA_FAULT_SEEN", "entry-replacement\n");
    }
    static int (*real_unlinkat)(int, const char *, int);
    if (!real_unlinkat) real_unlinkat = dlsym(RTLD_NEXT, "unlinkat");
    if (flags == AT_REMOVEDIR && !strncmp(path, ".tmp-delete-", 12) &&
        fd_matches(dirfd, getenv("GW_QA_RMDIR_ROOT"))) {
        mark("GW_QA_FAULT_SEEN", "rmdir\n");
        errno = EACCES;
        return -1;
    }
    const char *target = getenv("GW_QA_UNLINK_PATH");
    const char *basename = target ? strrchr(target, '/') : NULL;
    if (basename && !strcmp(basename + 1, path)) {
        char parent[PATH_MAX];
        int n = snprintf(parent, sizeof parent, "%.*s", (int)(basename - target), target);
        if (n > 0 && n < (int)sizeof parent &&
            (fd_matches(dirfd, parent) || workspace_for(dirfd, parent))) {
            mark("GW_QA_FAULT_SEEN", "unlink\n");
            errno = EACCES;
            return -1;
        }
    }
    return real_unlinkat(dirfd, path, flags);
}

int linkat(int oldfd, const char *old, int newfd, const char *new, int flags) {
    static int (*real_linkat)(int, const char *, int, const char *, int);
    if (!real_linkat) real_linkat = dlsym(RTLD_NEXT, "linkat");
    const char *name = getenv("GW_QA_LINK_NAME");
    if (name && !strcmp(name, new) && fd_matches(newfd, getenv("GW_QA_LINK_PARENT"))) {
        mark("GW_QA_FAULT_SEEN", "linkat\n");
        errno = EIO;
        return -1;
    }
    return real_linkat(oldfd, old, newfd, new, flags);
}

int flock(int fd, int operation) {
    static int (*real_flock)(int, int);
    static unsigned exclusive;
    if (!real_flock) real_flock = dlsym(RTLD_NEXT, "flock");
    if (operation == LOCK_EX && ++exclusive == 2)
        mark("GW_QA_LOCK_SEEN", "second-exclusive-lock\n");
    if (operation == LOCK_SH)
        mark("GW_QA_SHARED_LOCK_SEEN", "shared-lock\n");
    return real_flock(fd, operation);
}
"""


def snapshot(directory):
    result = {}
    for path in directory.rglob("*"):
        relative = str(path.relative_to(directory))
        if path.is_symlink():
            result[relative] = ("symlink", os.readlink(path))
        elif path.is_file():
            result[relative] = ("file", path.read_bytes())
        elif path.is_dir():
            result[relative] = ("directory",)
        else:
            raise AssertionError(f"unexpected special entry: {path}")
    return result


class DeleteSandbox(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="gw-delete-qa-")
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name)
        self.root = self.home / "config/ghostty/ghostty-wall"
        self.env = {
            "HOME": str(self.home), "XDG_CONFIG_HOME": str(self.home / "config"),
            "XDG_RUNTIME_DIR": str(self.home / "runtime"), "PATH": "/nonexistent",
            "TERM": "xterm-256color",
        }
        self.cli("init")
        self.original = self.home / "originals/boy.png"
        self.original.parent.mkdir()
        self.original.write_bytes((ROOT / "tests/fixtures/white.png").read_bytes())
        self.cli("new", "boy", str(self.original))
        self.original_bytes = self.original.read_bytes()

    def cli(self, *args, input="", env=None, success=True):
        result = subprocess.run([str(BINARY), *args], input=input, env=env or self.env,
                                capture_output=True, text=True, timeout=30)
        if success:
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        elif success is False:
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        return result

    def session(self, env=None):
        session = Session(BINARY, env or self.env, "delete", "boy")
        self.addCleanup(session.close)
        session.wait(b"Cancel (default)")
        return session

    def finish(self, session, success=True):
        session.send("y\n")
        session.proc.wait(timeout=30)
        session.wait(b"Profile boy deleted" if success else b"ghostty-wall:")
        self.assertEqual(session.proc.returncode == 0, success, bytes(session.data))
        return bytes(session.data)

    def records(self):
        return [json.loads(p.read_bytes())
                for p in sorted((self.root / "history/activations").glob("act-*.json"))]

    def assert_original(self):
        self.assertTrue(self.original.is_file(), "deletion removed the user's original image")
        self.assertEqual(self.original.read_bytes(), self.original_bytes)


class DeleteConsent(DeleteSandbox):
    def test_profile_or_registry_change_requires_new_confirmation(self):
        for kind in ("profile", "registry"):
            with self.subTest(kind=kind):
                session = self.session()
                path = self.root / ("profiles/boy.toml" if kind == "profile" else "config.toml")
                before_text = path.read_text()
                addition = "\n# changed while confirming\n" if kind == "profile" else (
                    "\n[sources.other]\nkind = 'local-directory'\npath = 'other'\n")
                path.write_text(before_text + addition)
                before = snapshot(self.root)
                self.finish(session, success=False)
                self.assertEqual(snapshot(self.root), before)
                self.assert_original()
                path.write_text(before_text)

    def test_image_cleanup_scope_cannot_expand_after_confirmation(self):
        self.cli("duplicate", "boy", "peer")
        session = self.session()
        self.assertIn(b"no proven-exclusive owned copy", session.data)
        self.cli("delete", "peer", input="y\n")
        self.finish(session)
        self.assertTrue((self.root / "profiles/boy.png").is_file())
        self.assert_original()
        self.assertEqual(self.records(), [])

    def test_new_shared_reference_after_confirmation_prevents_image_cleanup(self):
        session = self.session()
        self.assertIn(b"Eligible owned image", session.data)
        self.cli("duplicate", "boy", "peer")
        peer = (self.root / "profiles/peer.toml").read_bytes()
        self.finish(session)
        self.assertTrue((self.root / "profiles/boy.png").is_file())
        self.assertEqual((self.root / "profiles/peer.toml").read_bytes(), peer)
        self.cli("plan", "peer", "--json")
        self.assert_original()

    def test_image_changed_while_confirming_is_retained(self):
        session = self.session()
        changed = b"new user-owned bytes replacing the confirmed candidate"
        (self.root / "profiles/boy.png").write_bytes(changed)
        self.finish(session)
        self.assertEqual((self.root / "profiles/boy.png").read_bytes(), changed)
        self.assert_original()

    def test_byte_identical_profile_replacement_does_not_inherit_consent(self):
        session = self.session()
        target = self.root / "profiles/boy.toml"
        replacement = self.home / "replacement.toml"
        replacement.write_bytes(target.read_bytes())
        self.assertNotEqual(target.stat().st_ino, replacement.stat().st_ino)
        replacement.replace(target)
        before = snapshot(self.root)
        self.finish(session, success=False)
        self.assertEqual(snapshot(self.root), before)
        self.assert_original()

    def test_byte_identical_image_replacement_during_confirmation_is_retained(self):
        session = self.session()
        target = self.root / "profiles/boy.png"
        replacement = self.home / "replacement.png"
        replacement.write_bytes(target.read_bytes())
        self.assertNotEqual(target.stat().st_ino, replacement.stat().st_ino)
        replacement.replace(target)
        output = self.finish(session)
        self.assertIn(b"Images retained", output)
        self.assertEqual(target.read_bytes(), self.original_bytes)
        self.assert_original()
        self.assertEqual(self.records(), [])

    def test_eof_at_named_confirmation_cancels_in_real_terminal(self):
        before = snapshot(self.root)
        session = self.session()
        session.send("\x04")
        session.wait(b"Deletion cancelled")
        session.proc.wait(timeout=20)
        self.assertEqual(session.proc.returncode, 0)
        self.assertEqual(snapshot(self.root), before)
        self.assert_original()


@unittest.skipUnless(sys.platform == "linux" and shutil.which("cc"),
                     "Linux filesystem-fault/interleaving probe needs cc; not a live Ghostty test")
class DeleteFaults(DeleteSandbox):
    @classmethod
    def setUpClass(cls):
        cls.build = tempfile.TemporaryDirectory(prefix="gw-delete-shim-")
        cls.addClassCleanup(cls.build.cleanup)
        source = Path(cls.build.name) / "fault.c"
        cls.shim = Path(cls.build.name) / "fault.so"
        source.write_text(SHIM)
        result = subprocess.run([shutil.which("cc"), "-shared", "-fPIC", "-Wall", "-Wextra",
                                 "-Werror", "-o", str(cls.shim), str(source), "-ldl"],
                                env={"PATH": os.defpath}, capture_output=True, text=True, timeout=30)
        if result.returncode:
            raise AssertionError(result.stderr)

    def fault_env(self, **variables):
        return dict(self.env, LD_PRELOAD=str(self.shim),
                    GW_QA_FAULT_SEEN=str(self.home / "fault-seen"), **variables)

    def active(self):
        # Simple valid customized Welcome keeps fault probes independent of palette generation.
        (self.root / "profiles/welcome.toml").write_text(
            "schema_version = 1\n[terminal]\nfont_size = 15\n")
        self.cli("apply", "boy")
        self.first_record = (self.root / "history/activations/act-v1-0000000000000001.json").read_bytes()
        self.first_projection = (self.root / "current.ghostty").read_bytes()

    def assert_fault_seen(self, expected):
        self.assertEqual((self.home / "fault-seen").read_text(), expected + "\n")
        self.assert_original()
        if hasattr(self, "first_record"):
            self.assertEqual((self.root / "history/activations/act-v1-0000000000000001.json").read_bytes(),
                             self.first_record)

    def assert_welcome(self):
        self.assertEqual(len(self.records()), 2)
        self.assertEqual(self.records()[-1]["profile"]["id"], "welcome")
        self.assertIn(b"font-size = 15", (self.root / "current.ghostty").read_bytes())
        self.cli("doctor")

    def record_reload(self, status):
        binary_dir = self.home / "bin"
        binary_dir.mkdir()
        stub = binary_dir / "systemctl"
        stub.write_text("#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$GW_QA_RELOAD_LOG\"\n"
                        "case \"$*\" in *is-active*) exit 0;; esac\n"
                        "[ -f \"$GW_QA_ROOT/history/activations/act-v1-0000000000000002.json\" ] || exit 80\n"
                        "exit \"$GW_QA_RELOAD_STATUS\"\n")
        stub.chmod(0o700)
        self.env.update(PATH=str(binary_dir), GW_QA_ROOT=str(self.root),
                        GW_QA_RELOAD_LOG=str(self.home / "reload-log"),
                        GW_QA_RELOAD_STATUS=str(status))

    def test_fallback_publication_failure_retains_profile_and_retry_reconciles(self):
        self.active()
        self.record_reload(0)
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_LINK_NAME="act-v1-0000000000000002.json",
            GW_QA_LINK_PARENT=str(self.root / "history/activations")))
        self.assert_fault_seen("linkat")
        self.assertIn("Welcome fallback failed", result.stderr)
        self.assertTrue((self.root / "profiles/boy.toml").is_file())
        self.assertTrue((self.root / "profiles/boy.png").is_file())
        self.assertEqual(len(self.records()), 1)
        self.assertFalse((self.home / "reload-log").exists())
        self.cli("delete", "boy", input="y\n")
        self.assert_welcome()
        self.cli("previous")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), self.first_projection)

    def test_fallback_commit_uncertainty_never_removes_profile_or_image(self):
        self.active()
        self.record_reload(0)
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_SYNC_PATH=str(self.root / "history/activations"), GW_QA_SYNC_NUMBER="1"))
        self.assert_fault_seen("fsync")
        self.assertIn("Profile and image retained", result.stderr)
        self.assertIn("inspect doctor/history", result.stderr)
        self.assertTrue((self.root / "profiles/boy.toml").is_file())
        self.assertTrue((self.root / "profiles/boy.png").is_file())
        self.assertFalse((self.home / "reload-log").exists())
        self.cli("delete", "boy", input="y\n")
        self.assert_welcome()

    def test_crash_after_committed_welcome_leaves_safe_retry_without_duplicate_activation(self):
        self.active()
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_SYNC_PATH=str(self.root / "history/activations"), GW_QA_SYNC_NUMBER="1",
            GW_QA_CRASH_AFTER_SYNC="1"))
        self.assertEqual(result.returncode, 121)
        self.assert_fault_seen("fsync")
        self.assert_welcome()
        self.assertTrue((self.root / "profiles/boy.toml").is_file())
        self.assertTrue((self.root / "profiles/boy.png").is_file())
        self.cli("delete", "boy", input="y\n")
        self.assert_welcome()
        self.assertFalse((self.root / "profiles/boy.png").exists())

    def test_postcommit_profile_unlink_failure_keeps_profile_and_reloads_committed_welcome(self):
        self.active()
        self.record_reload(1)
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_UNLINK_PATH=str(self.root / "profiles/boy.toml")))
        self.assert_fault_seen("unlink")
        self.assertIn("Deletion of Profile boy incomplete", result.stderr)
        self.assertIn("Welcome Activation", result.stderr)
        self.assertIn("reload Failed", result.stderr)
        self.assertTrue((self.root / "profiles/boy.toml").is_file())
        self.assertTrue((self.root / "profiles/boy.png").is_file())
        self.assertIn("--user reload", (self.home / "reload-log").read_text())
        self.assert_welcome()
        self.cli("delete", "boy", input="y\n")
        self.assert_welcome()

    def test_profile_unlink_directory_sync_failure_retains_image_and_reports_uncertainty(self):
        self.active()
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_SYNC_PATH=str(self.root / "profiles"), GW_QA_SYNC_NUMBER="1"))
        self.assert_fault_seen("fsync")
        self.assertIn("Profile removal may be visible, image retained", result.stderr)
        self.assertNotIn("Profile boy deleted", result.stdout)
        self.assertFalse((self.root / "profiles/boy.toml").exists())
        self.assertTrue((self.root / "profiles/boy.png").is_file())
        self.assert_welcome()
        self.cli("previous")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), self.first_projection)

    def test_optional_image_cleanup_failure_preserves_original_and_durable_replay(self):
        self.active()
        result = self.cli("delete", "boy", input="y\n", env=self.fault_env(
            GW_QA_UNLINK_PATH=str(self.root / "profiles/boy.png")))
        self.assert_fault_seen("unlink")
        self.assertIn("Owned image cleanup incomplete", result.stdout)
        self.assertFalse((self.root / "profiles/boy.toml").exists())
        self.assertTrue((self.root / "profiles/boy.png").is_file())
        self.assert_welcome()
        self.cli("previous")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), self.first_projection)

    def test_image_cleanup_directory_sync_failure_is_not_reported_as_certain_removal(self):
        self.active()
        result = self.cli("delete", "boy", input="y\n", env=self.fault_env(
            GW_QA_SYNC_PATH=str(self.root / "profiles"), GW_QA_SYNC_NUMBER="2"))
        self.assert_fault_seen("fsync")
        self.assertIn("removal durability uncertain; inspect before retrying", result.stdout)
        self.assertNotIn("Removed owned image", result.stdout)
        self.assert_welcome()
        self.cli("previous")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), self.first_projection)

    def test_inactive_delete_never_calls_even_an_available_reload_adapter(self):
        self.record_reload(0)
        projection = b"unreconciled draft\n"
        (self.root / "current.ghostty").write_bytes(projection)
        self.cli("delete", "boy", input="y\n")
        self.assertFalse((self.home / "reload-log").exists())
        self.assertEqual((self.root / "current.ghostty").read_bytes(), projection)
        self.assertEqual(self.records(), [])
        self.assert_original()

    def test_successful_adapter_action_is_not_claimed_as_visible_ghostty_change(self):
        self.active()
        self.record_reload(0)
        result = self.cli("delete", "boy", input="y\n")
        self.assertIn("action accepted; visible change is not verified", result.stdout)
        self.assert_welcome()
        self.assert_original()

    def test_failed_reload_after_fallback_does_not_undo_durable_delete(self):
        self.active()
        self.record_reload(1)
        result = self.cli("delete", "boy", input="y\n")
        self.assertIn("reload: failed; Welcome Activation remains committed", result.stdout)
        self.assertNotIn("visible change is verified", result.stdout)
        self.assertFalse((self.root / "profiles/boy.toml").exists())
        self.assertFalse((self.root / "profiles/boy.png").exists())
        self.assert_welcome()
        self.assert_original()
        self.cli("previous")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), self.first_projection)

    def test_byte_identical_image_replaced_at_unlink_is_not_deleted(self):
        target = self.root / "profiles/boy.png"
        replacement = self.home / "new-user-image.png"
        replacement.write_bytes(target.read_bytes())
        self.assertNotEqual(target.stat().st_ino, replacement.stat().st_ino)
        result = self.cli("delete", "boy", input="y\n", success=None, env=self.fault_env(
            GW_QA_REPLACE_PARENT=str(target.parent), GW_QA_REPLACE_NAME=target.name,
            GW_QA_REPLACEMENT=str(replacement)))
        self.assert_fault_seen("entry-replacement")
        self.assertFalse(replacement.exists(), "replacement was not moved into the candidate slot")
        self.assertTrue(target.is_file(),
                        "delete removed an unconfirmed replacement image after its final identity check: "
                        + result.stdout)
        self.assertEqual(target.read_bytes(), self.original_bytes)
        self.assertNotIn("Removed owned image", result.stdout)
        self.assertEqual(self.records(), [])

    def test_byte_identical_profile_replaced_at_unlink_does_not_inherit_consent(self):
        target = self.root / "profiles/boy.toml"
        replacement = self.home / "new-user-profile.toml"
        replacement.write_bytes(target.read_bytes())
        self.assertNotEqual(target.stat().st_ino, replacement.stat().st_ino)
        result = self.cli("delete", "boy", input="y\n", success=None, env=self.fault_env(
            GW_QA_REPLACE_PARENT=str(target.parent), GW_QA_REPLACE_NAME=target.name,
            GW_QA_REPLACEMENT=str(replacement)))
        self.assert_fault_seen("entry-replacement")
        self.assertTrue(target.is_file(), "delete removed a Profile that was not confirmed: "
                        + result.stdout)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue((target.parent / "boy.png").is_file())
        self.assertNotIn("Profile boy deleted", result.stdout)
        self.assertEqual(self.records(), [])

    def test_image_symlink_substituted_at_capture_is_restored_without_following_it(self):
        target = self.root / "profiles/boy.png"
        replacement = self.home / "replacement-link.png"
        replacement.symlink_to(self.original)
        result = self.cli("delete", "boy", input="y\n", env=self.fault_env(
            GW_QA_REPLACE_PARENT=str(target.parent), GW_QA_REPLACE_NAME=target.name,
            GW_QA_REPLACEMENT=str(replacement)))
        self.assert_fault_seen("entry-replacement")
        self.assertTrue(target.is_symlink())
        self.assertEqual(target.readlink(), self.original)
        self.assertIn("cleanup incomplete", result.stdout)
        self.assertNotIn("Removed owned image", result.stdout)
        self.assertEqual(list(self.root.glob(".tmp-delete-*")), [])

    def test_directory_published_during_restoration_is_never_recursively_removed(self):
        target = self.root / "profiles/boy.toml"
        replacement = self.home / "replacement-directory"
        replacement.mkdir()
        (replacement / "precious.txt").write_bytes(b"unconfirmed directory contents")
        # POSIX cannot rename a directory over the existing file. This fixture's
        # replacement is moved into the now-empty public slot by the adapter below.
        target_bytes = target.read_bytes()
        real_replacement = self.home / "replacement.toml"
        real_replacement.write_bytes(target_bytes)
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_REPLACE_PARENT=str(target.parent), GW_QA_REPLACE_NAME=target.name,
            GW_QA_REPLACEMENT=str(real_replacement), GW_QA_RESTORE_CONFLICT=str(replacement)))
        self.assert_fault_seen("entry-replacement\nrestore-conflict")
        self.assertEqual((target / "precious.txt").read_bytes(), b"unconfirmed directory contents")
        retained, = self.root.glob(".tmp-delete-*/boy.toml")
        self.assertEqual(retained.read_bytes(), target_bytes)
        self.assertIn(str(retained), result.stderr)
        self.assertTrue((target.parent / "boy.png").is_file())
        self.assertEqual(self.records(), [])

    def test_replacement_profile_published_during_isolated_unlink_is_retained_with_image(self):
        target = self.root / "profiles/boy.toml"
        replacement = self.home / "replacement.toml"
        replacement.write_bytes(target.read_bytes())
        identity = replacement.stat().st_ino
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_REPLACE_PARENT=str(target.parent), GW_QA_REPLACE_NAME=target.name,
            GW_QA_REPLACEMENT=str(replacement), GW_QA_REPLACE_AT_ISOLATED_UNLINK="1"))
        self.assert_fault_seen("entry-replacement")
        self.assertEqual(target.stat().st_ino, identity)
        self.assertTrue((target.parent / "boy.png").is_file())
        self.assertNotIn("Profile boy deleted", result.stdout)
        self.assertIn("new entry at confirmed name retained", result.stderr)
        self.assertEqual(list(self.root.glob(".tmp-delete-*")), [])
        self.assertEqual(self.records(), [])

    def test_replacement_image_published_during_isolated_unlink_is_retained(self):
        target = self.root / "profiles/boy.png"
        replacement = self.home / "replacement.png"
        replacement.write_bytes(self.original_bytes)
        identity = replacement.stat().st_ino
        result = self.cli("delete", "boy", input="y\n", env=self.fault_env(
            GW_QA_REPLACE_PARENT=str(target.parent), GW_QA_REPLACE_NAME=target.name,
            GW_QA_REPLACEMENT=str(replacement), GW_QA_REPLACE_AT_ISOLATED_UNLINK="1"))
        self.assert_fault_seen("entry-replacement")
        self.assertEqual(target.stat().st_ino, identity)
        self.assertEqual(target.read_bytes(), self.original_bytes)
        self.assertNotIn("Removed owned image", result.stdout)
        self.assertIn("cleanup incomplete", result.stdout)
        self.assertEqual(list(self.root.glob(".tmp-delete-*")), [])
        self.assertEqual(self.records(), [])

    def test_failed_restoration_never_overwrites_new_profile_or_discards_captured_replacement(self):
        target = self.root / "profiles/boy.toml"
        replacement = self.home / "replacement.toml"
        replacement.write_bytes(target.read_bytes())
        captured_identity = replacement.stat().st_ino
        newer = self.home / "newer.toml"
        newer.write_bytes(b"schema_version = 1\n")
        newer_identity = newer.stat().st_ino
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_REPLACE_PARENT=str(target.parent), GW_QA_REPLACE_NAME=target.name,
            GW_QA_REPLACEMENT=str(replacement), GW_QA_RESTORE_CONFLICT=str(newer)))
        self.assert_fault_seen("entry-replacement\nrestore-conflict")
        self.assertEqual(target.stat().st_ino, newer_identity)
        self.assertEqual(target.read_bytes(), b"schema_version = 1\n")
        retained, = self.root.glob(".tmp-delete-*/boy.toml")
        self.assertEqual(retained.stat().st_ino, captured_identity)
        self.assertEqual(retained.parent.stat().st_mode & 0o777, 0o700)
        self.assertIn(str(retained), result.stderr)
        self.assertIn("cannot restore without replacing", result.stderr)
        self.assertTrue((target.parent / "boy.png").is_file())
        self.assertEqual(self.records(), [])
        self.cli("delete", "boy", input="y\n")
        self.assertEqual(retained.stat().st_ino, captured_identity,
                         "a retry must not adopt or sweep earlier retained entries")

    def test_unsupported_atomic_detachment_fails_closed_and_removes_empty_workspace(self):
        before = snapshot(self.root)
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_DETACH_PARENT=str(self.root / "profiles"), GW_QA_DETACH_FAIL="1"))
        self.assert_fault_seen("rename")
        self.assertNotIn("Profile boy deleted", result.stdout)
        self.assertEqual(snapshot(self.root), before)

    def test_crash_after_detachment_retains_private_entry_without_affecting_replay(self):
        self.active()
        target = self.root / "profiles/boy.toml"
        profile_bytes = target.read_bytes()
        identity = target.stat().st_ino
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_DETACH_PARENT=str(target.parent), GW_QA_CRASH_AFTER_DETACH="1"))
        self.assertEqual(result.returncode, 121)
        self.assert_fault_seen("detached")
        self.assert_welcome()
        self.assertFalse(target.exists())
        retained, = self.root.glob(".tmp-delete-*/boy.toml")
        self.assertEqual(retained.stat().st_ino, identity)
        self.assertEqual(retained.read_bytes(), profile_bytes)
        self.assertTrue((target.parent / "boy.png").is_file())
        self.cli("previous")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), self.first_projection)
        self.assertEqual(retained.read_bytes(), profile_bytes)
        self.cli("list")
        self.assertFalse(target.exists(), "read-only commands must not restore interrupted removal")

    def test_interrupted_image_cleanup_does_not_add_a_candidate_to_profile_source(self):
        target = self.root / "profiles/boy.png"
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_DETACH_PARENT=str(target.parent), GW_QA_CRASH_AFTER_DETACH="1",
            GW_QA_DETACH_NAME=target.name))
        self.assertEqual(result.returncode, 121)
        self.assert_fault_seen("detached")
        self.assertFalse((target.parent / "boy.toml").exists())
        self.assertFalse(target.exists())
        retained, = self.root.glob(".tmp-delete-*/boy.png")
        self.assertEqual(retained.read_bytes(), self.original_bytes)
        (target.parent / "random.toml").write_text(
            'schema_version = 1\n[wallpaper]\nmode = "source"\nsource = "welcome"\nselection = "random"\n')
        plan = json.loads(self.cli("plan", "random", "--seed", "00" * 32, "--json").stdout)
        self.assertEqual(plan["selection"]["candidate_count"], 1)
        self.assertEqual(plan["selection"]["candidate"], "welcome.png")
        self.assertEqual(retained.read_bytes(), self.original_bytes)
        self.assertEqual(self.records(), [])

    def test_replacement_profile_published_during_workspace_cleanup_keeps_its_image(self):
        self.check_replacement_during_workspace_cleanup()

    def test_replacement_profile_during_active_delete_workspace_cleanup_keeps_its_image(self):
        self.active()
        self.check_replacement_during_workspace_cleanup(active=True)

    def check_replacement_during_workspace_cleanup(self, active=False):
        target = self.root / "profiles/boy.toml"
        image = target.with_suffix(".png")
        replacement = self.home / "replacement.toml"
        replacement.write_bytes(target.read_bytes())
        identity = replacement.stat().st_ino
        result = self.cli("delete", "boy", input="y\n", success=None, env=self.fault_env(
            GW_QA_REPLACE_PARENT=str(target.parent), GW_QA_REPLACE_NAME=target.name,
            GW_QA_REPLACEMENT=str(replacement), GW_QA_REPLACE_AT_WORKSPACE_CLEANUP="1"))
        self.assert_fault_seen("cleanup-entry-replacement")
        self.assertEqual(target.stat().st_ino, identity)
        self.assertFalse(replacement.exists())
        if active:
            self.assert_welcome()
        else:
            self.assertEqual(self.records(), [])
        plan = self.cli("plan", "boy", "--json", success=None)
        self.assertTrue(image.is_file(),
                        "renewed ownership proof ignored a replacement Profile already present "
                        f"at the public name; exit={result.returncode}; " + result.stdout + result.stderr +
                        f"\nRetained Profile plan exit={plan.returncode}: {plan.stdout}{plan.stderr}")
        self.assertEqual(image.read_bytes(), self.original_bytes)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertNotIn("Profile boy deleted", result.stdout)
        self.assertEqual(plan.returncode, 0, plan.stdout + plan.stderr)

    def test_profile_replacement_without_image_reference_still_requires_fresh_confirmation(self):
        target = self.root / "profiles/boy.toml"
        replacement = self.home / "replacement.toml"
        replacement.write_text("schema_version = 1\n")
        identity = replacement.stat().st_ino
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_REPLACE_PARENT=str(target.parent), GW_QA_REPLACE_NAME=target.name,
            GW_QA_REPLACEMENT=str(replacement), GW_QA_REPLACE_AT_WORKSPACE_CLEANUP="1"))
        self.assert_fault_seen("cleanup-entry-replacement")
        self.assertEqual(target.stat().st_ino, identity)
        self.assertEqual((target.parent / "boy.png").read_bytes(), self.original_bytes)
        self.assertIn("start delete again", result.stderr)
        self.assertNotIn("Profile boy deleted", result.stdout)
        self.cli("plan", "boy", "--json")
        self.assertEqual(self.records(), [])

    def test_replacement_of_profile_without_owned_image_is_not_reported_as_deleted(self):
        target = self.root / "profiles/boy.toml"
        target.write_text("schema_version = 1\n")
        replacement = self.home / "replacement.toml"
        replacement.write_bytes(target.read_bytes())
        identity = replacement.stat().st_ino
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_REPLACE_PARENT=str(target.parent), GW_QA_REPLACE_NAME=target.name,
            GW_QA_REPLACEMENT=str(replacement), GW_QA_REPLACE_AT_WORKSPACE_CLEANUP="1"))
        self.assert_fault_seen("cleanup-entry-replacement")
        self.assertIn("no proven-exclusive owned copy", result.stdout)
        self.assertEqual(target.stat().st_ino, identity)
        self.assertEqual((target.parent / "boy.png").read_bytes(), self.original_bytes)
        self.assertIn("start delete again", result.stderr)
        self.assertNotIn("Profile boy deleted", result.stdout)
        self.assertEqual(self.records(), [])

    def test_image_replacement_during_profile_workspace_cleanup_is_retained(self):
        target = self.root / "profiles/boy.png"
        replacement = self.home / "replacement.png"
        replacement.write_bytes(self.original_bytes)
        identity = replacement.stat().st_ino
        # The first workspace cleanup follows Profile removal, before the renewed image proof.
        result = self.cli("delete", "boy", input="y\n", env=self.fault_env(
            GW_QA_REPLACE_PARENT=str(target.parent), GW_QA_REPLACE_NAME=target.name,
            GW_QA_REPLACEMENT=str(replacement), GW_QA_REPLACE_AT_WORKSPACE_CLEANUP="1"))
        self.assert_fault_seen("cleanup-entry-replacement")
        self.assertEqual(target.stat().st_ino, identity)
        self.assertEqual(target.read_bytes(), self.original_bytes)
        self.assertNotIn("Removed owned image", result.stdout)
        self.assertIn("Images retained", result.stdout)
        self.assertFalse(target.with_suffix(".toml").exists())
        self.assertEqual(self.records(), [])

    def test_replacement_during_renewed_ownership_scan_requires_fresh_confirmation(self):
        self.check_replacement_during_ownership_scan("same")

    def test_active_replacement_during_renewed_ownership_scan_preserves_fallback_and_image(self):
        self.active()
        self.check_replacement_during_ownership_scan("same", active=True)

    def test_unmanaged_replacement_during_renewed_ownership_scan_still_retains_image(self):
        self.check_replacement_during_ownership_scan("unmanaged")

    def test_symlink_replacement_during_renewed_ownership_scan_is_not_followed_or_deleted(self):
        self.check_replacement_during_ownership_scan("symlink")

    def check_replacement_during_ownership_scan(self, kind, active=False):
        target = self.root / "profiles/boy.toml"
        replacement = self.home / "replacement.toml"
        if kind == "symlink":
            replacement.symlink_to(self.original)
        else:
            replacement.write_bytes(target.read_bytes() if kind == "same" else b"schema_version = 1\n")
        identity = replacement.lstat().st_ino
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_SCAN_PARENT=str(target.parent), GW_QA_SCAN_REPLACEMENT=str(replacement)))
        self.assert_fault_seen("ownership-scan-replacement")
        self.assertEqual(target.lstat().st_ino, identity)
        self.assertEqual((target.parent / "boy.png").read_bytes(), self.original_bytes)
        self.assertIn("start delete again", result.stderr)
        self.assertNotIn("Profile boy deleted", result.stdout)
        self.assertEqual(list(self.root.glob(".tmp-delete-*")), [])
        if kind == "symlink":
            self.assertEqual(target.readlink(), self.original)
        else:
            self.cli("plan", "boy", "--json")
            retry = self.cli("delete", "boy", input="y\n")
            self.assertIn("Delete Profile boy?", retry.stdout)
            self.assertFalse(target.exists())
            self.assertEqual((target.parent / "boy.png").exists(), kind == "unmanaged")
            self.assert_original()
        if active:
            self.assert_welcome()
            self.cli("previous")
            self.assertEqual((self.root / "current.ghostty").read_bytes(), self.first_projection)
        else:
            self.assertEqual(self.records(), [])

    def test_competing_apply_waits_through_fallback_and_isolated_profile_removal(self):
        self.active()
        (self.root / "profiles/other.toml").write_text(
            "schema_version = 1\n[terminal]\nfont_size = 23.125\n")
        ready = self.home / "capture-ready"
        release = self.home / "capture-release"
        waiting = self.home / "apply-waiting"
        deletion = self.session(env=self.fault_env(
            GW_QA_PAUSE_PARENT=str(self.root / "profiles"), GW_QA_PAUSE_READY=str(ready),
            GW_QA_PAUSE_RELEASE=str(release)))
        deletion.send("y\n")
        self.wait_for_marker(ready)
        self.assertEqual(ready.read_text(), "captured\n")
        self.assertEqual(len(self.records()), 2)
        self.assertEqual(self.records()[-1]["profile"]["id"], "welcome")
        retained, = self.root.glob(".tmp-delete-*/boy.toml")
        self.assertTrue(retained.is_file())
        with (self.root / "state.lock").open("rb") as probe:
            with self.assertRaises(BlockingIOError, msg="removal lost its exclusive writer lock"):
                fcntl.flock(probe, fcntl.LOCK_SH | fcntl.LOCK_NB)

        competing = Session(BINARY, self.fault_env(GW_QA_SHARED_LOCK_SEEN=str(waiting)),
                            "apply", "other")
        self.addCleanup(competing.close)
        self.wait_for_marker(waiting)
        self.assertIsNone(competing.proc.poll(), "apply passed the writer lock during removal")
        self.assertEqual(len(self.records()), 2)
        release.write_text("continue\n")
        deletion.proc.wait(timeout=30)
        deletion.wait(b"Profile boy deleted")
        self.assertEqual(deletion.proc.returncode, 0, bytes(deletion.data))
        competing.proc.wait(timeout=30)
        self.assertEqual(competing.proc.returncode, 0, bytes(competing.data))
        self.assertEqual([r["profile"]["id"] for r in self.records()], ["boy", "welcome", "other"])
        self.assertIn(b"font-size = 23.125", (self.root / "current.ghostty").read_bytes())
        self.assertFalse((self.root / "profiles/boy.toml").exists())
        self.assertFalse((self.root / "profiles/boy.png").exists())
        self.assertEqual(list(self.root.glob(".tmp-delete-*")), [])
        self.assert_original()
        self.cli("previous")
        self.cli("previous")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), self.first_projection)

    def wait_for_marker(self, path):
        deadline = time.monotonic() + 10
        while not path.exists() and time.monotonic() < deadline:
            time.sleep(0.01)
        self.assertTrue(path.exists(), f"child did not reach {path.name}")

    def test_private_sync_failure_before_profile_unlink_restores_same_entry_and_reloads_fallback(self):
        self.active()
        self.record_reload(0)
        before = snapshot(self.root / "profiles")
        target = self.root / "profiles/boy.toml"
        identity = target.stat().st_ino
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_PRIVATE_SYNC_PARENT=str(target.parent), GW_QA_SYNC_NUMBER="1"))
        self.assert_fault_seen("fsync")
        self.assertEqual(snapshot(self.root / "profiles"), before)
        self.assertEqual(target.stat().st_ino, identity)
        self.assertEqual(list(self.root.glob(".tmp-delete-*")), [])
        self.assertIn("Welcome Activation", result.stderr)
        self.assertIn("--user reload", (self.home / "reload-log").read_text())
        self.assert_welcome()
        self.cli("previous")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), self.first_projection)

    def test_private_sync_failure_after_profile_unlink_retains_image_and_reports_uncertainty(self):
        self.active()
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_PRIVATE_SYNC_PARENT=str(self.root / "profiles"), GW_QA_SYNC_NUMBER="2"))
        self.assert_fault_seen("fsync")
        self.assertFalse((self.root / "profiles/boy.toml").exists())
        self.assertTrue((self.root / "profiles/boy.png").is_file())
        self.assertNotIn("Profile boy deleted", result.stdout)
        self.assertIn("Profile removal may be visible, image retained", result.stderr)
        self.assertEqual(list(self.root.glob(".tmp-delete-*")), [])
        self.assert_welcome()
        self.cli("previous")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), self.first_projection)

    def test_private_sync_failure_before_image_unlink_restores_image_without_undoing_profile_delete(self):
        self.active()
        image = self.root / "profiles/boy.png"
        identity = image.stat().st_ino
        result = self.cli("delete", "boy", input="y\n", env=self.fault_env(
            GW_QA_PRIVATE_SYNC_PARENT=str(image.parent), GW_QA_SYNC_NUMBER="3"))
        self.assert_fault_seen("fsync")
        self.assertFalse(image.with_suffix(".toml").exists())
        self.assertEqual(image.stat().st_ino, identity)
        self.assertEqual(image.read_bytes(), self.original_bytes)
        self.assertIn("Owned image cleanup incomplete", result.stdout)
        self.assertNotIn("Removed owned image", result.stdout)
        self.assertEqual(list(self.root.glob(".tmp-delete-*")), [])
        self.assert_welcome()
        self.cli("previous")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), self.first_projection)

    def test_workspace_cleanup_failure_retains_image_and_old_workspace_on_retry(self):
        self.active()
        result = self.cli("delete", "boy", input="y\n", success=False, env=self.fault_env(
            GW_QA_RMDIR_ROOT=str(self.root)))
        self.assert_fault_seen("rmdir")
        self.assertFalse((self.root / "profiles/boy.toml").exists())
        self.assertTrue((self.root / "profiles/boy.png").is_file())
        workspace, = self.root.glob(".tmp-delete-*")
        self.assertEqual(list(workspace.iterdir()), [])
        self.assertEqual(workspace.stat().st_mode & 0o777, 0o700)
        self.assertIn(str(workspace), result.stderr)
        self.assertNotIn("Profile boy deleted", result.stdout)
        self.assert_welcome()
        self.cli("new", "other", str(self.original))
        self.cli("delete", "other", input="y\n")
        self.assertTrue(workspace.is_dir(), "a later deletion swept an earlier workspace")
        self.assertTrue((self.root / "profiles/boy.png").is_file())
        self.cli("previous")
        self.assertEqual((self.root / "current.ghostty").read_bytes(), self.first_projection)

    def test_managed_root_replacement_while_waiting_for_lock_aborts_before_fallback(self):
        self.active()
        signal = self.home / "waiting-for-lock"
        session = self.session(env=self.fault_env(GW_QA_SHARED_LOCK_SEEN=str(signal)))
        before = snapshot(self.root)
        saved = self.root.with_name("saved-managed-root")
        with (self.root / "state.lock").open("rb") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            session.send("y\n")
            deadline = time.monotonic() + 10
            while not signal.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertTrue(signal.exists(), "delete did not reach its post-confirmation lock")
            self.root.rename(saved)
            shutil.copytree(saved, self.root)
        session.proc.wait(timeout=30)
        session.wait(b"ghostty-wall:")
        self.assertNotEqual(session.proc.returncode, 0, bytes(session.data))
        self.assertEqual(snapshot(saved), before)
        self.assertEqual(snapshot(self.root), before)
        self.assert_original()

    def test_directory_substitution_at_unlink_cannot_redirect_removal(self):
        external_profile = self.original.parent / "boy.toml"
        external_profile.write_bytes((self.root / "profiles/boy.toml").read_bytes())
        outside_before = snapshot(self.original.parent)
        saved = self.root / "profiles-before-unlink"
        self.cli("delete", "boy", input="y\n", env=self.fault_env(
            GW_QA_SWAP_DIR=str(self.root / "profiles"),
            GW_QA_SAVED_DIR=str(saved),
            GW_QA_OUTSIDE_DIR=str(self.original.parent)))
        self.assert_fault_seen("directory-swap")
        self.assertEqual(snapshot(self.original.parent), outside_before,
                         "unlink followed a directory substituted after the final check")
        self.assertFalse((saved / "boy.toml").exists())
        self.assertTrue((saved / "boy.png").is_file())
        self.assertEqual(self.records(), [])

    def test_profiles_symlink_substitution_while_waiting_for_lock_cannot_delete_original(self):
        external_profile = self.original.parent / "boy.toml"
        external_profile.write_bytes((self.root / "profiles/boy.toml").read_bytes())
        outside_before = snapshot(self.original.parent)
        signal = self.home / "waiting-for-lock"
        session = self.session(env=self.fault_env(GW_QA_LOCK_SEEN=str(signal)))
        with (self.root / "state.lock").open("rb") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            session.send("y\n")
            deadline = time.monotonic() + 10
            while not signal.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertTrue(signal.exists(), "delete did not reach its post-confirmation lock")
            (self.root / "profiles").rename(self.root / "profiles-before-swap")
            (self.root / "profiles").symlink_to(self.original.parent, target_is_directory=True)
        session.proc.wait(timeout=30)
        session.wait(b"Profile boy deleted" if session.proc.returncode == 0 else b"ghostty-wall:")
        self.assertEqual(snapshot(self.original.parent), outside_before,
                         "delete traversed a substituted profiles/ symlink and removed outside originals; "
                         f"exit={session.proc.returncode}, output={bytes(session.data[-800:])!r}")
        self.assert_original()


if __name__ == "__main__":
    unittest.main()
