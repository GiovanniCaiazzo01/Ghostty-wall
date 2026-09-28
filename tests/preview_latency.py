"""Controlled release-build key-response measurement, not a tight CI timing assertion.

GHOSTTY_WALL_BIN=$PWD/target/release/ghostty-wall python3 tests/preview_latency.py
The FIFO reader/writer handshake proves preparation is blocked before each key.
"""
import hashlib
import json
import os
import platform
import select
import time

from qa_management_pty import viewport
from responsive_preview_pty import ResponsivePreviewPty
from tui_pty import BINARY, ALT_ENTER, ALT_LEAVE


def response(session, key, completed):
    while select.select([session.master], [], [], 0)[0]:
        session.data.extend(os.read(session.master, 65536))
    output = bytearray()
    assert not completed(session, output), "response state already present before key"
    start = time.perf_counter_ns()
    session.send(key)
    deadline = start + 2_000_000_000
    while True:
        remaining = (deadline - time.perf_counter_ns()) / 1_000_000_000
        if remaining <= 0 or not select.select([session.master], [], [], remaining)[0]:
            raise AssertionError(f"No completed current state after {key!r}: {viewport(session)!r}")
        chunk = os.read(session.master, 65536)
        if not chunk:
            raise AssertionError(f"PTY closed before response to {key!r}")
        output.extend(chunk)
        session.data.extend(chunk)
        if completed(session, output):
            return (time.perf_counter_ns() - start) / 1_000_000


def main():
    trials = []
    for _ in range(10):
        case = ResponsivePreviewPty()
        try:
            case.setUp()
            session, writer, _ = case.blocked_session()
            before = viewport(session)
            assert before.startswith("Preview: boy") and "> boy" in before
            assert not writer.closed
            select_ms = response(
                session, "j",
                lambda current, _: viewport(current).startswith("Preview: welcome")
                and "> welcome [active]" in viewport(current)
                and "> boy" not in viewport(current),
            )
            selected_frame = viewport(session)
            assert session.data.count(ALT_ENTER) == 1 and ALT_LEAVE not in session.data
            exit_ms = response(session, "q", lambda _, output: ALT_LEAVE in output)
            session.finish()
            assert not writer.closed
            trials.append({
                "selection_ms": select_ms, "exit_ms": exit_ms,
                "selected_current_frame": selected_frame,
                "fifo_writer_open_through_exit": True,
                "terminal_restored": True,
            })
        finally:
            case.doCleanups()
    print(json.dumps({
        "binary": str(BINARY), "sha256": hashlib.sha256(BINARY.read_bytes()).hexdigest(),
        "platform": platform.platform(), "python": platform.python_version(),
        "processor": platform.processor(), "cpu_count": os.cpu_count(),
        "cpu_affinity": sorted(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else None,
        "load_average": os.getloadavg(),
        "clock": vars(time.get_clock_info("perf_counter")),
        "pty_cells": [100, 30], "graphics": "unsupported", "TERM": "xterm-256color",
        "method": "perf_counter_ns from PTY key write until reconstructed CURRENT viewport has Preview: welcome header AND > welcome [active] selection, without > boy; exit ends on fresh alternate-screen-leave bytes, followed by process/ICANON/ECHO restoration checks; continuously drain PTY; open empty FIFO handshake blocks preparation through selection and exit; 10 independent disposable HOME/XDG sessions",
        "preview_completion": "deliberately blocked; not measured as key response",
        "trials": trials,
        "within_100ms": all(max(t["selection_ms"], t["exit_ms"]) <= 100 for t in trials),
    }, indent=2))


if __name__ == "__main__":
    main()
