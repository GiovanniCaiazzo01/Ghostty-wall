#!/usr/bin/env python3
"""Owned real-Ghostty evidence; never captures an output or injects global input.

Requires Ghostty, Hyprland, grim -T, wayland-scanner, cc and wayland-client.
Artifacts include exact commands, hashes, private configuration and PTY bytes.
The management HOME must already contain disposable fixtures. No updater runs.
"""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import signal
import socket
import subprocess
import sys
import termios
import time
import tty
import uuid

HELPER = r'''
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wayland-client.h>
#include "toplevel.h"
struct item { char *title, *app, *id; };
static const char *want_title, *want_app;
static int found, supported;
static void closed(void *d, struct ext_foreign_toplevel_handle_v1 *h) { (void)d; (void)h; }
static void done(void *d, struct ext_foreign_toplevel_handle_v1 *h) {
  (void)h; struct item *i=d;
  if(i->title && i->app && i->id && !strcmp(i->title,want_title) && !strcmp(i->app,want_app)) {
    puts(i->id); found++;
  }
}
static void title(void *d, struct ext_foreign_toplevel_handle_v1 *h, const char *s) { (void)h; struct item *i=d; free(i->title); i->title=strdup(s); }
static void app(void *d, struct ext_foreign_toplevel_handle_v1 *h, const char *s) { (void)h; struct item *i=d; free(i->app); i->app=strdup(s); }
static void ident(void *d, struct ext_foreign_toplevel_handle_v1 *h, const char *s) { (void)h; struct item *i=d; free(i->id); i->id=strdup(s); }
static const struct ext_foreign_toplevel_handle_v1_listener hl={closed,done,title,app,ident};
static void top(void *d, struct ext_foreign_toplevel_list_v1 *l, struct ext_foreign_toplevel_handle_v1 *h) {
  (void)d; (void)l; ext_foreign_toplevel_handle_v1_add_listener(h,&hl,calloc(1,sizeof(struct item)));
}
static void finished(void *d, struct ext_foreign_toplevel_list_v1 *l) { (void)d; (void)l; }
static const struct ext_foreign_toplevel_list_v1_listener ll={top,finished};
static void global(void *d, struct wl_registry *r, uint32_t n, const char *s, uint32_t v) {
  (void)d; (void)v;
  if(!strcmp(s,"ext_foreign_toplevel_list_v1")) {
    supported=1;
    struct ext_foreign_toplevel_list_v1 *l=wl_registry_bind(r,n,&ext_foreign_toplevel_list_v1_interface,1);
    ext_foreign_toplevel_list_v1_add_listener(l,&ll,NULL);
  }
}
static void removed(void *d, struct wl_registry *r, uint32_t n) { (void)d; (void)r; (void)n; }
static const struct wl_registry_listener rl={global,removed};
int main(int argc,char **argv) {
  if(argc!=3) return 2;
  want_app=argv[1]; want_title=argv[2];
  struct wl_display *d=wl_display_connect(NULL); if(!d) return 3;
  wl_registry_add_listener(wl_display_get_registry(d),&rl,NULL);
  for(int i=0;i<3;i++) if(wl_display_roundtrip(d)<0) return 4;
  wl_display_disconnect(d);
  if(!supported) { fputs("BLOCKED: ext_foreign_toplevel_list_v1 unavailable\n",stderr); return 5; }
  return found==1 ? 0 : 6;
}
'''


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def private_env(home):
    home = Path(home).resolve()
    env = {"HOME": str(home), "PATH": "/nonexistent", "LANG": "C.UTF-8",
           "TERM": "xterm-ghostty", "TERM_PROGRAM": "ghostty", "COLORTERM": "truecolor"}
    for key, name in [("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"),
                      ("XDG_CACHE_HOME", "cache"), ("XDG_STATE_HOME", "state"),
                      ("XDG_RUNTIME_DIR", "runtime")]:
        directory = home / name
        directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        env[key] = str(directory)
    return env


def managed_snapshot(home):
    root = Path(home) / "config/ghostty/ghostty-wall"
    return {str(p.relative_to(root)): digest(p) for p in sorted(root.rglob("*")) if p.is_file()}


def driver(out, binary, home):
    # The short /tmp socket stays private even when artifact paths are long.
    endpoint = Path(out, "socket-path").read_text()
    server = socket.socket(socket.AF_UNIX)
    server.bind(endpoint)
    os.chmod(endpoint, 0o600)
    server.listen()
    child = None
    master = None
    saved_termios = termios.tcgetattr(0)
    tty.setraw(0)
    if binary:
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, fcntl.ioctl(0, termios.TIOCGWINSZ, b"\0" * 8))
        child = subprocess.Popen([binary, "tui"], stdin=slave, stdout=slave, stderr=slave,
                                 env=private_env(home), start_new_session=True)
        os.close(slave)

        def resized(signum, frame):
            size = fcntl.ioctl(0, termios.TIOCGWINSZ, b"\0" * 8)
            Path(out, "latest-winsize.bin").write_bytes(size)
            fcntl.ioctl(master, termios.TIOCSWINSZ, size)
            os.killpg(child.pid, signal.SIGWINCH)

        signal.signal(signal.SIGWINCH, resized)
        Path(out, "initial-winsize.bin").write_bytes(fcntl.ioctl(0, termios.TIOCGWINSZ, b"\0" * 8))
    else:
        os.write(1, b"\x1b[2J\x1b[H\x1b[?25l")
        text = ["GHOSTTY-WALL / NEW PROFILE WALLPAPER COMPARISON", "",
                "Same public photograph, crop, font and opaque terminal.",
                "Only background-image-opacity changes: old 0.10 / proposed 0.05.", "",
                "README.md  src/profile_workflow.rs  tests/create_command.rs",
                "$ cargo test --locked --test create_command",
                "running 24 tests", "test explicit_wallpaper_opacity_is_preserved ... ok", "",
                "fn create_profile(id: &str) -> Result<Profile> {",
                "    let opacity = NEW_PROFILE_WALLPAPER_OPACITY;",
                "    save_profile(id, opacity)?; // save is not apply", "}", "",
                "The quick brown fox jumps over the lazy dog. 0123456789",
                "Thin strokes: ilI1 | [] {} () <> / \\ ; : , .", ""]
        for line in text:
            os.write(1, (line + "\r\n").encode())
        os.write(1, b"\x1b[1mBold heading\x1b[0m  \x1b[2mDim secondary text\x1b[0m\r\n")
        for code in range(30, 38):
            os.write(1, f"\x1b[{code}m ANSI {code} \x1b[0m".encode())
        os.write(1, b"\r\n\x1b[7m Selection contrast example \x1b[0m\r\n")
    Path(out, "driver-ready").touch()
    try:
        with Path(out, "terminal.bin").open("wb") as log:
            while True:
                ready, _, _ = select.select([server, 0] + ([master] if master is not None else []), [], [], 1)
                if server in ready:
                    conn, _ = server.accept()
                    with conn:
                        request = json.loads(conn.recv(65536))
                        if request.get("stop"):
                            break
                        if master is not None:
                            os.write(master, bytes.fromhex(request["hex"]))
                        conn.sendall(b"ok")
                if 0 in ready:
                    response = os.read(0, 65536)
                    if master is not None:
                        os.write(master, response)
                if master is not None and master in ready:
                    try:
                        data = os.read(master, 65536)
                    except OSError:
                        break
                    if not data:
                        break
                    log.write(data)
                    log.flush()
                    os.write(1, data)
    finally:
        if child and child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=3)
            except subprocess.TimeoutExpired:
                child.kill()
        termios.tcsetattr(0, termios.TCSANOW, saved_termios)
        server.close()
        Path(endpoint).unlink(missing_ok=True)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "_driver":
        driver(*sys.argv[2:])
        return
    if len(sys.argv) > 1 and sys.argv[1] == "_exec":
        Path(sys.argv[2]).write_text(str(os.getpid()))
        os.execv("/usr/bin/ghostty", ["/usr/bin/ghostty", *sys.argv[3:]])
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--photo", type=Path)
    parser.add_argument("--background", choices=["dark", "light"], default="dark")
    parser.add_argument("--opacity", choices=["0.1", "0.05"], default="0.05")
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--home", type=Path)
    parser.add_argument("--steps", type=Path, help='JSON list: {"label":..., "hex":..., "wait":2}')
    parser.add_argument("--width", type=int, default=1200)
    parser.add_argument("--height", type=int, default=800)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, mode=0o700)
    nonce = uuid.uuid4().hex
    app = "com.ghosttywall.Visual" + nonce
    title = "GW-VISUAL-" + nonce
    endpoint_dir = Path("/tmp") / ("gw-proof-" + nonce)
    endpoint_dir.mkdir(mode=0o700)
    (out / "socket-path").write_text(str(endpoint_dir / "driver"))
    manifest = {"command": sys.argv, "app_id": app, "title": title,
                "ghostty_sha256": digest("/usr/bin/ghostty"),
                "script_sha256": digest(__file__), "captures": []}

    def run(command, **kw):
        manifest.setdefault("commands", []).append([str(x) for x in command])
        result = subprocess.run(command, capture_output=True, text=True, timeout=20, **kw)
        if result.returncode:
            raise RuntimeError(f"{command!r}: exit {result.returncode}: {result.stdout} {result.stderr}")
        return result

    protocol = "/usr/share/wayland-protocols/staging/ext-foreign-toplevel-list/ext-foreign-toplevel-list-v1.xml"
    (out / "toplevel.c").write_text(HELPER)
    run(["wayland-scanner", "client-header", protocol, out / "toplevel.h"])
    run(["wayland-scanner", "private-code", protocol, out / "protocol.c"])
    run(["cc", "-Wall", "-Wextra", str(out / "toplevel.c"), str(out / "protocol.c"),
         "-o", str(out / "toplevel"), *run(["pkg-config", "--cflags", "--libs", "wayland-client"]).stdout.split()])
    home = (args.home or out / "home").resolve()
    gui_env = private_env(home)
    if args.binary:
        manifest["managed_before"] = managed_snapshot(home)
    socket_path = Path(os.environ["WAYLAND_DISPLAY"])
    if not socket_path.is_absolute():
        socket_path = Path(os.environ["XDG_RUNTIME_DIR"]) / socket_path
    gui_env.update(WAYLAND_DISPLAY=str(socket_path), GDK_BACKEND="wayland", PATH="/usr/bin:/bin",
                   XDG_DATA_DIRS="/usr/local/share:/usr/share", NO_AT_BRIDGE="1")
    bg, fg = ("161b22", "e6edf3") if args.background == "dark" else ("f6f8fa", "1f2328")
    config = [f"background = {bg}", f"foreground = {fg}", "background-opacity = 1",
              "background-blur = false", "font-size = 13", "window-decoration = false",
              "gtk-titlebar = false", "shell-integration = none", "confirm-close-surface = false",
              "window-padding-x = 16", "window-padding-y = 16", "cursor-style-blink = false",
              "window-width = 110", "window-height = 38"]
    if args.photo:
        manifest["photo"] = {"path": str(args.photo.resolve()), "sha256": digest(args.photo)}
        config += [f"background-image = {args.photo.resolve()}", f"background-image-opacity = {args.opacity}",
                   "background-image-fit = cover", "background-image-position = center", "background-image-repeat = false"]
    if args.binary:
        manifest["binary"] = {"path": str(args.binary.resolve()), "sha256": digest(args.binary)}
    (out / "ghostty.conf").write_text("\n".join(config) + "\n")
    command = ["/usr/bin/dbus-run-session", "--", sys.executable, str(Path(__file__).resolve()), "_exec",
               str(out / "ghostty.pid"), "--config-default-files=false", f"--config-file={out / 'ghostty.conf'}",
               "--gtk-single-instance=false", f"--class={app}", f"--title={title}",
               f"--working-directory={out}", "-e", sys.executable, str(Path(__file__).resolve()), "_driver",
               str(out), str(args.binary.resolve()) if args.binary else "", str(home)]
    manifest["launch"] = command
    proc = None
    try:
        with (out / "ghostty.log").open("wb") as log:
            proc = subprocess.Popen(command, env=gui_env, stdout=log, stderr=log, start_new_session=True)
        deadline = time.monotonic() + 20
        owned = []
        while time.monotonic() < deadline:
            if (out / "ghostty.pid").exists():
                pid = int((out / "ghostty.pid").read_text())
                owned = [c for c in json.loads(run(["hyprctl", "-j", "clients"]).stdout)
                         if c["pid"] == pid and c["class"] == app and c["title"] == title]
                if len(owned) == 1 and (out / "driver-ready").exists():
                    break
            if proc.poll() is not None:
                raise RuntimeError("Ghostty exited before ownership/driver-ready gate; see ghostty.log")
            time.sleep(0.1)
        if len(owned) != 1 or not (out / "driver-ready").exists():
            raise RuntimeError("BLOCKED: exact PID + app ID + title + driver-ready gate")
        if Path(f"/proc/{pid}/exe").resolve() != Path("/usr/bin/ghostty").resolve():
            raise RuntimeError("BLOCKED: PID executable mismatch")
        manifest["owned_initial"] = owned[0]
        address = owned[0]["address"]
        run(["hyprctl", "dispatch", f'hl.dsp.window.float({{action="on",window="address:{address}"}})'])
        run(["hyprctl", "dispatch", f'hl.dsp.window.resize({{x={args.width},y={args.height},relative=false,window="address:{address}"}})'])
        time.sleep(1)
        steps = json.loads(args.steps.read_text()) if args.steps else [{"label": "sample", "wait": 2}]
        for step in steps:
            if step.get("hex"):
                with socket.socket(socket.AF_UNIX) as s:
                    s.connect(str(endpoint_dir / "driver"))
                    s.sendall(json.dumps({"hex": step["hex"]}).encode())
                    s.recv(128)
            time.sleep(step.get("wait", 2))
            matches = [c for c in json.loads(run(["hyprctl", "-j", "clients"]).stdout)
                       if c["pid"] == pid and c["class"] == app and c["title"] == title]
            if len(matches) != 1:
                raise RuntimeError("BLOCKED: ownership changed before capture")
            identifier = run([out / "toplevel", app, title], env=gui_env).stdout.strip()
            if not identifier or "\n" in identifier:
                raise RuntimeError("BLOCKED: unique foreign-toplevel identifier unavailable")
            capture = out / (step["label"] + ".png")
            run(["grim", "-T", identifier, str(capture)], env=gui_env)
            manifest["captures"].append({"path": str(capture), "sha256": digest(capture),
                                          "identifier": identifier, "owned": matches[0], "step": step})
        manifest["status"] = "captured; requires visual inspection"
    except Exception as error:
        manifest["status"] = "BLOCKED: " + str(error)
        raise
    finally:
        if (endpoint_dir / "driver").exists():
            try:
                with socket.socket(socket.AF_UNIX) as s:
                    s.settimeout(2)
                    s.connect(str(endpoint_dir / "driver"))
                    s.sendall(b'{"stop":true}')
            except OSError:
                pass
        if proc:
            try:
                proc.wait(timeout=4)
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGTERM)
                proc.wait(timeout=4)
        if args.binary:
            manifest["managed_after"] = managed_snapshot(home)
            manifest["browsing_read_only"] = manifest["managed_before"] == manifest["managed_after"]
        (out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        (endpoint_dir / "driver").unlink(missing_ok=True)
        endpoint_dir.rmdir()


if __name__ == "__main__":
    main()
