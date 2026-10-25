"""Observer timing regression; no product process or personal state needed."""
import unittest
from types import SimpleNamespace
from unittest.mock import patch

import preview_ready_latency as probe


class ReceiptTimingTests(unittest.TestCase):
    def test_validation_cost_does_not_change_receipt_timestamp(self):
        session = SimpleNamespace(master=123, data=bytearray())
        clock = [10_000_000]

        def read(*args):
            clock[0] = 20_000_000
            return b"complete frame"

        def predicate():
            if session.data:
                clock[0] = 150_000_000  # Simulated slow screen/PNG validation.
            return bool(session.data)

        with patch.object(probe.select, "select", return_value=([123], [], [])), \
             patch.object(probe.os, "read", side_effect=read), \
             patch.object(probe.time, "perf_counter_ns", side_effect=lambda: clock[0]):
            elapsed = probe.wait(session, predicate, 10_000_000)
        self.assertEqual(elapsed, 140)  # Preserve historical, observer-inclusive result.
        self.assertEqual(session.latency_observations, [{
            "pty_receipt_upper_bound_ms": 10,
            "validation_after_receipt_ms": 130,
        }])
        # A following PNG wait can already be satisfied by the same read.
        with patch.object(probe.time, "perf_counter_ns", return_value=170_000_000):
            probe.wait(session, lambda: True, 10_000_000)
        self.assertEqual(session.latency_observations[-1]["pty_receipt_upper_bound_ms"], 10)
        # A predicate satisfied by pre-input data must never yield negative time.
        with patch.object(probe.time, "perf_counter_ns", return_value=180_000_000):
            probe.wait(session, lambda: True, 175_000_000)
        self.assertEqual(session.latency_observations[-1]["pty_receipt_upper_bound_ms"], 0)


if __name__ == "__main__":
    unittest.main()
