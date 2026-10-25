"""Release PTY timings through COMPLETE PNG transmission, never a loading label.

GHOSTTY_WALL_BIN=... python3 tests/preview_ready_latency.py [--managed-root PATH]
Optional state is copied to disposable HOME; source images are only read.
No apply, install, or changes to the supplied state. OS page caches are not flushed.
"""
import argparse
import base64
import hashlib
import json
import os
import re
import select
import shutil
import time
import zlib
from pathlib import Path

from edit_pty import EditSession
from management_pty import ManagementPty
from tui_pty import BINARY


def pngs(data):
    """Only count fully transmitted, decompressible PNGs with positive dimensions."""
    images = []
    payload = bytearray()
    for match in re.finditer(rb"\x1b_G([^;]*);([^\x1b]*)\x1b\\", data):
        control, chunk = match.groups()
        if b"a=T,f=100,i=42" in control:
            payload.clear()
        elif not control.startswith(b"m="):
            continue
        payload.extend(chunk)
        if b"m=1" in control:
            continue
        png = base64.b64decode(payload, validate=True)
        assert png.startswith(b"\x89PNG\r\n\x1a\n") and png.endswith(b"IEND\xaeB`\x82")
        assert int.from_bytes(png[16:20], "big") > 0 and int.from_bytes(png[20:24], "big") > 0
        position, compressed = 8, bytearray()
        while position < len(png):
            length = int.from_bytes(png[position:position + 4], "big")
            if png[position + 4:position + 8] == b"IDAT":
                compressed.extend(png[position + 8:position + 8 + length])
            position += length + 12
        assert zlib.decompress(compressed)
        images.append(hashlib.sha256(png).hexdigest())
    return images


def wait(session, predicate, start):
    deadline = time.monotonic() + 45
    while not predicate():
        if time.monotonic() >= deadline:
            raise AssertionError(f"Preview not ready: {EditSession.screen(session)!r}")
        if select.select([session.master], [], [], 0.01)[0]:
            session.data.extend(os.read(session.master, 65536))
            session.latency_received_ns = time.perf_counter_ns()
    observed = time.perf_counter_ns()
    # Replay/decompression runs in this reader: receipt is still an upper bound,
    # not product emission or compositor latency. Keep the historical total too.
    received = max(start, getattr(session, "latency_received_ns", start))
    if not hasattr(session, "latency_observations"):
        session.latency_observations = []
    session.latency_observations.append({
        "pty_receipt_upper_bound_ms": (received - start) / 1_000_000,
        "validation_after_receipt_ms": (observed - received) / 1_000_000,
    })
    return (observed - start) / 1_000_000


def trial(managed_root, small, editor_images=False):
    case = ManagementPty()
    try:
        case.setUp()
        if managed_root:
            shutil.rmtree(case.root)
            shutil.copytree(managed_root, case.root)
        if small:
            for path in (case.root / "history/activations").glob("*.json"):
                path.unlink()
        case.env["TERM_PROGRAM"] = "ghostty"
        start = time.perf_counter_ns()
        session = EditSession(BINARY, case.env, "tui")
        case.addCleanup(session.close)
        first_frame = wait(session, lambda: b"n Create" in session.data, start)
        # Fresh menu response proves input loop is running, not merely first paint.
        start_input = time.perf_counter_ns()
        session.send("?")
        input_ms = wait(session, lambda: b"Create Profile" in EditSession.screen(session), start_input)
        session.send("\x1b")
        first = wait(session, lambda: len(pngs(session.data)) >= 1, start)
        hashes = pngs(session.data)
        start = time.perf_counter_ns()
        session.send("j")
        next_ms = wait(session, lambda: len(pngs(session.data)) >= 2, start)
        start = time.perf_counter_ns()
        session.send("k")
        revisit = wait(session, lambda: len(pngs(session.data)) >= 3, start)
        assert pngs(session.data)[2] == hashes[0], "unchanged revisit image differs"
        count = len(pngs(session.data))
        start = time.perf_counter_ns()
        session.send("e")
        edit = wait(session, lambda: b"Edit Profile" in EditSession.screen(session)
                    and b"Cursor" in EditSession.screen(session), start)
        updates = {}
        if editor_images:
            updates["edit_png_ready_ms"] = wait(session, lambda: len(pngs(session.data)) > count, start)
            count = len(pngs(session.data))
            start = time.perf_counter_ns()
            session.send("\x1b[B" * 6 + "\r9\r")
            updates["blur_feedback_ms"] = wait(session, lambda: "Image blur (0–32) 9".encode() in b" ".join(EditSession.screen(session).split()), start)
            updates["blur_png_ready_ms"] = wait(session, lambda: len(pngs(session.data)) > count, start)
            count = len(pngs(session.data))
            start = time.perf_counter_ns()
            session.send("\x1b[B\r61\r")
            updates["darken_feedback_ms"] = wait(session, lambda: "Image darkening (0–100%) 61".encode() in b" ".join(EditSession.screen(session).split()), start)
            updates["darken_png_ready_ms"] = wait(session, lambda: len(pngs(session.data)) > count, start)
        session.send("q")
        session.wait(b"draft discarded")
        case.finish(session)
        timings = dict(first_frame_ms=first_frame, first_input_ms=input_ms,
                       first_png_ready_ms=first, next_png_ready_ms=next_ms,
                       revisit_png_ready_ms=revisit, edit_sample_ready_ms=edit, **updates)
        assert len(timings) == len(session.latency_observations)
        return dict(**timings, observation={
            name: evidence for name, evidence in zip(timings, session.latency_observations)
        })
    finally:
        case.doCleanups()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--managed-root", type=Path)
    parser.add_argument("--trials", type=int, default=3)
    parser.add_argument("--editor-images", action="store_true", help="Also wait for editor, blur and darkening PNGs")
    args = parser.parse_args()
    print(json.dumps(dict(binary=str(BINARY), sha256=hashlib.sha256(BINARY.read_bytes()).hexdigest(),
                         graphics="complete PNG payload over PTY; not real Ghostty display latency",
                         timing="top-level ms include observer replay/validation (historical metric); observation separates matching PTY receipt upper bound and subsequent validation; reader work can delay receipt",
                         cache="process-first, OS caches uncontrolled; revisit same process",
                         trials={name: [trial(args.managed_root, small, args.editor_images) for _ in range(args.trials)]
                                 for name, small in [("small_history", True), ("repeated_history", False)]}), indent=2))


if __name__ == "__main__":
    main()
