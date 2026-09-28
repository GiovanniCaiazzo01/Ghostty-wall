"""Linux updater publication faults; run with a built lib-test binary argument.

No installed executable or live release service is used. The child unit fixture
owns a disposable prefix; LD_PRELOAD changes only its publication syscalls.
"""
import os
from pathlib import Path
import subprocess
import sys
import tempfile

SHIM = r'''
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
static int fired;
static void fault(int dir, const char *name) {
    const char *mode = getenv("GW_QA_UPDATE_FAULT");
    if (fired == 3 && mode && !strcmp(mode, "rollback") && !strcmp(name, "ghostty-wall")) { fired = 2; return; }
    if (fired) return;
    char fd[80], path[4096];
    snprintf(fd, sizeof(fd), "/proc/self/fd/%d", dir);
    ssize_t size = readlink(fd, path, sizeof(path)-1);
    if (size < 0) return;
    path[size] = 0;
    if (!strstr(path, "/fault-prefix")) return;
    if (!mode) return;
    if (!strcmp(mode, "race") && !strcmp(name, "ghostty-wall")) {
        fired = 1;
        int out = openat(dir, ".substituted", O_CREAT|O_EXCL|O_WRONLY, 0755);
        if (out < 0 || write(out, "substituted binary", 18) != 18) _exit(91);
        close(out);
        int (*real)(int,const char*,int,const char*) = dlsym(RTLD_NEXT, "renameat");
        if (real(dir, ".substituted", dir, name)) _exit(92);
    }
    if (((!strcmp(mode, "toml") || !strcmp(mode, "rollback")) && !strcmp(name, ".crates.toml")) ||
        (!strcmp(mode, "json") && !strcmp(name, ".crates2.json")) ||
        (!strcmp(mode, "marker") && !strcmp(name, ".ghostty-wall-release.sha256"))) fired = 2;
}
int renameat(int a,const char *b,int c,const char *d) {
    fault(c,d);
    if (fired == 2) { fired = 3; errno = EACCES; return -1; }
    int (*real)(int,const char*,int,const char*) = dlsym(RTLD_NEXT,"renameat");
    return real(a,b,c,d);
}
int renameat2(int a,const char *b,int c,const char *d,unsigned flags) {
    fault(c,d);
    if (fired == 2) { fired = 3; errno = EACCES; return -1; }
    int (*real)(int,const char*,int,const char*,unsigned) = dlsym(RTLD_NEXT,"renameat2");
    return real(a,b,c,d,flags);
}
'''

CARGO = r'''#!/usr/bin/python3
import json, os, pathlib, sys
args = sys.argv[1:]
assert args[:5] == ['install', '--git', 'https://github.com/GiovanniCaiazzo01/Ghostty-wall', '--tag', 'v1.0.10']
assert args[5:7] == ['--locked', '--root']
root = pathlib.Path(args[7])
assert args[8:] == ['--bin', 'ghostty-wall', 'ghostty-wall']
assert str(root / 'target') == os.environ['CARGO_TARGET_DIR']
print('private compiler output', file=sys.stderr)
if os.environ['GW_QA_CARGO'] == 'failure': sys.exit(17)
(root / 'bin').mkdir()
exe = root / 'bin/ghostty-wall'
exe.write_text("#!/bin/sh\\necho 'ghostty-wall 1.0.10'\\n".replace('\\n', '\n'))
exe.chmod(0o755)
key = 'ghostty-wall 1.0.10 (git+https://github.com/GiovanniCaiazzo01/Ghostty-wall?tag=v1.0.10#0123456789012345678901234567890123456789)'
(root / '.crates.toml').write_text('[v1]\n' + json.dumps(key) + ' = ["ghostty-wall"]\n')
(root / '.crates2.json').write_text(json.dumps({'installs': {key: {'bins': ['ghostty-wall']}}}))
'''

def main():
    binary = str(Path(sys.argv[1]).resolve())
    with tempfile.TemporaryDirectory(prefix="gw-update-faults-") as temporary:
        root = Path(temporary)
        source, shim = root / "fault.c", root / "fault.so"
        source.write_text(SHIM)
        subprocess.run(["cc", "-shared", "-fPIC", str(source), "-ldl", "-o", str(shim)], check=True)
        for fault in ["race", "toml", "json", "marker", "rollback"]:
            home = root / fault
            home.mkdir()
            env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(home / "config"),
                       XDG_DATA_HOME=str(home / "data"), XDG_STATE_HOME=str(home / "state"),
                       XDG_CACHE_HOME=str(home / "cache"), LD_PRELOAD=str(shim), GW_QA_UPDATE_FAULT=fault)
            subprocess.run([binary, "update::tests::publication_fault_preserves_installation_and_substitutions",
                            "--exact", "--ignored", "--nocapture"], env=env, check=True)
            print(f"PASS: {fault}", flush=True)
        tools = root / "tools"
        tools.mkdir()
        cargo = tools / "cargo"
        cargo.write_text(CARGO)
        cargo.chmod(0o755)
        for mode in ["missing", "failure", "success"]:
            home = root / mode
            home.mkdir()
            env = dict(os.environ, HOME=str(home), XDG_CONFIG_HOME=str(home / "config"),
                       XDG_DATA_HOME=str(home / "data"), XDG_STATE_HOME=str(home / "state"),
                       XDG_CACHE_HOME=str(home / "cache"), GW_QA_CARGO=mode,
                       PATH=str(root / "missing-tools" if mode == "missing" else tools))
            env.pop("LD_PRELOAD", None)
            subprocess.run([binary, "update::tests::source_build_process_boundary", "--exact", "--ignored", "--nocapture"], env=env, check=True)
            print(f"PASS: Cargo process {mode}", flush=True)

if __name__ == "__main__":
    main()
