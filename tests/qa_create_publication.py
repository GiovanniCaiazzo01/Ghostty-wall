"""Ticket 03 public-CLI publication-uncertainty regressions, never a live Ghostty test.

Run: cargo build --locked && python3 -m unittest discover -s tests -p qa_create_publication.py -v
Only the create child gets LD_PRELOAD; the filesystem fault is scoped to its temporary profiles/.
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
BINARY = ROOT / "target/debug/ghostty-wall"
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
    const char *fail_at = getenv("GW_QA_FAIL_SYNC");
    if (directory && fail_at) {
        char descriptor[64], target[PATH_MAX];
        snprintf(descriptor, sizeof descriptor, "/proc/self/fd/%d", fd);
        ssize_t len = readlink(descriptor, target, sizeof target - 1);
        if (len >= 0) {
            target[len] = '\0';
            if (!strcmp(target, directory) && ++calls == strtoul(fail_at, NULL, 10)) {
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
    "Linux LD_PRELOAD filesystem fault probe requires a C compiler; not a live reload oracle",
)
class CreatePublication(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.build = tempfile.TemporaryDirectory(prefix="gw-create-fault-build-")
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

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="gw-create-publication-qa-")
        self.addCleanup(self.tmp.cleanup)
        self.home = Path(self.tmp.name)
        self.root = self.home / "config/ghostty/ghostty-wall"
        self.env = {
            "HOME": str(self.home),
            "XDG_CONFIG_HOME": str(self.home / "config"),
            "XDG_RUNTIME_DIR": str(self.home / "runtime"),
            "PATH": "/nonexistent",
        }
        self.cli("init")
        self.cli("apply", "welcome")
        self.original = self.home / "original.png"
        shutil.copyfile(ROOT / "tests/fixtures/palette.png", self.original)

    def cli(self, *args, input="", fault=None, expected=0):
        env = self.env.copy()
        if fault is not None:
            env.update({
                "LD_PRELOAD": str(self.shim),
                "GW_QA_SYNC_DIRECTORY": str(self.root / "profiles"),
                "GW_QA_FAIL_SYNC": str(fault),
            })
        result = subprocess.run(
            [str(BINARY), *args], input=input, env=env,
            capture_output=True, text=True, timeout=30,
        )
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        if expected == 0:
            self.assertEqual(result.stderr, "")
        return result

    def snapshot(self):
        return {p.relative_to(self.root): p.read_bytes()
                for p in self.root.rglob("*") if p.is_file()}

    def fail_save(self, sync_number):
        before = self.snapshot()
        result = self.cli(
            "create", "uncertain", input=f"i\npath:{self.original}\ns\ny\n",
            fault=sync_number, expected=6,
        )
        self.assertIn("durability is uncertain", result.stderr)
        self.assertIn("files retained; inspect before retrying", result.stderr)
        self.assertNotIn("pre-save files preserved", result.stderr)
        self.assertNotIn("Saved Profile", result.stdout)
        self.assertNotIn("Use now", result.stdout)
        self.assertEqual(self.original.read_bytes(), (ROOT / "tests/fixtures/palette.png").read_bytes())
        return before, result

    def test_image_directory_sync_failure_keeps_image_without_claiming_clean_rollback(self):
        before, result = self.fail_save(1)
        self.assertIn(str(self.root / "profiles/uncertain.png"), result.stderr)
        after = self.snapshot()
        image = after.pop(Path("profiles/uncertain.png"))
        self.assertEqual(image, self.original.read_bytes())
        self.assertEqual(after, before)
        inspected = self.snapshot()
        different = ROOT / "tests/fixtures/white.png"
        retry = self.cli(
            "create", "uncertain", input=f"i\npath:{different}\ns\nn\n", expected=3,
        )
        self.assertIn("collision", retry.stderr)
        self.assertEqual(self.snapshot(), inspected)
        self.cli("create", "uncertain", input=f"i\npath:{self.original}\ns\nn\n")
        after = self.snapshot()
        self.assertEqual(after.pop(Path("profiles/uncertain.png")), image)
        after.pop(Path("profiles/uncertain.toml"))
        self.assertEqual(after, before)
        self.cli("plan", "uncertain", "--json")

    def test_profile_directory_sync_failure_retains_resolvable_profile_and_its_image(self):
        before, result = self.fail_save(2)
        self.assertIn(str(self.root / "profiles/uncertain.toml"), result.stderr)
        after = self.snapshot()
        self.assertEqual(after.pop(Path("profiles/uncertain.png")), self.original.read_bytes())
        after.pop(Path("profiles/uncertain.toml"))
        self.assertEqual(after, before)
        inspected = self.snapshot()
        plan = json.loads(self.cli("plan", "uncertain", "--json").stdout)
        self.assertEqual(plan["profile"], {"id": "uncertain", "schema_version": 2})
        self.assertEqual(plan["selection"]["candidate"], "uncertain.png")
        self.assertEqual(len(plan["environment"]["manifest"]["colors"]["palette"]), 16)
        retry = self.cli("create", "uncertain", expected=3)
        self.assertIn("collision", retry.stderr)
        self.assertEqual(self.snapshot(), inspected)


if __name__ == "__main__":
    unittest.main()
