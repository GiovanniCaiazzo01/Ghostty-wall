"""Ticket 04 public-session fsync failures; no Ghostty processes or live config.

Run: python3 -m unittest discover -s tests -p qa_preview_publication.py -v
LD_PRELOAD is confined to the Rust test child and one explicitly armed disposable directory.
"""

import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FAULT_SHIM = r"""
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int fsync(int fd) {
    static int (*real_fsync)(int);
    static unsigned calls;
    if (!real_fsync) {
        real_fsync = dlsym(RTLD_NEXT, "fsync");
        if (!real_fsync) _exit(120);
    }
    const char *directory = getenv("GW_QA_SYNC_DIRECTORY");
    const char *arm = getenv("GW_QA_SYNC_ARM");
    const char *fail_from = getenv("GW_QA_FAIL_FROM");
    if (directory && arm && fail_from && access(arm, F_OK) == 0) {
        char descriptor[64], target[PATH_MAX];
        snprintf(descriptor, sizeof descriptor, "/proc/self/fd/%d", fd);
        ssize_t len = readlink(descriptor, target, sizeof target - 1);
        if (len >= 0) {
            target[len] = '\0';
            if (!strcmp(target, directory) && ++calls >= strtoul(fail_from, NULL, 10)) {
                errno = EIO;
                return -1;
            }
        }
    }
    return real_fsync(fd);
}
"""


@unittest.skipUnless(
    sys.platform == "linux" and shutil.which("cc"),
    "Linux LD_PRELOAD filesystem-fault probe requires cc; not a live Ghostty oracle",
)
class PreviewPublication(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.build = tempfile.TemporaryDirectory(prefix="gw-preview-fault-build-")
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
        result = subprocess.run(
            [shutil.which("cargo"), "test", "--locked", "--test", "qa_preview_session",
             "--no-run", "--message-format=json"],
            cwd=ROOT, capture_output=True, text=True, timeout=120,
        )
        if result.returncode:
            raise AssertionError(result.stdout + result.stderr)
        artifacts = [json.loads(line) for line in result.stdout.splitlines()]
        cls.binary = next(
            artifact["executable"] for artifact in artifacts
            if artifact.get("reason") == "compiler-artifact"
            and artifact["target"]["name"] == "qa_preview_session"
            and artifact.get("executable")
        )

    def probe(self, mode, directory, fail_from):
        with tempfile.TemporaryDirectory(prefix="gw-preview-fsync-qa-") as temporary:
            home = Path(temporary)
            root = home / "xdg/ghostty/ghostty-wall"
            target = root if directory == "root" else root / "profiles"
            env = {
                "PATH": "/nonexistent",
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(home / "xdg"),
                "XDG_RUNTIME_DIR": str(home / "runtime"),
                "GW_QA_PREVIEW_HOME": str(home),
                "GW_QA_FAULT_MODE": mode,
                "GW_QA_SYNC_DIRECTORY": str(target),
                "GW_QA_SYNC_ARM": str(home / "fault-armed"),
                "GW_QA_FAIL_FROM": str(fail_from),
                "LD_PRELOAD": str(self.shim),
            }
            result = subprocess.run(
                [self.binary, "--exact", "publication_fault_child", "--test-threads=1", "--nocapture"],
                cwd=ROOT, env=env, capture_output=True, text=True, timeout=45,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("1 passed; 0 failed", result.stdout)

    def test_marker_publication_uncertainty_is_recoverable(self):
        self.probe("begin-sync", "root", 2)

    def test_image_directory_sync_failure_never_publishes_draft_or_reloads(self):
        self.probe("image-sync", "root", 1)

    def test_update_projection_sync_failure_retains_marker_and_never_reloads(self):
        self.probe("update-sync", "root", 1)

    def test_cancel_projection_sync_failure_retains_marker(self):
        self.probe("cancel-sync", "root", 1)

    def test_cleanup_sync_failure_retains_marker_after_projection_restored(self):
        self.probe("cleanup-sync", "root", 2)

    def test_edit_image_publication_uncertainty_preserves_old_profile(self):
        self.probe("save-image-sync", "profiles", 1)

    def test_edit_profile_publication_uncertainty_preserves_new_image_and_no_activation(self):
        self.probe("save-profile-sync", "profiles", 2)


if __name__ == "__main__":
    unittest.main()
