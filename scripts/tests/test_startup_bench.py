"""Tests for ``scripts/startup_bench.py``.

Run with ``python -m unittest discover -s scripts/tests``. The bench is a
measurement tool, not part of the gate; these tests pin the parsing of the
engine's trace lines so a report never quotes the wrong mark.
"""

from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import startup_bench


def trace_line(event: str, marks: list[tuple[str, float]], spans=None) -> str:
    payload = {
        "event": event,
        "stage": "first-present" if event == "startup-trace" else "exit",
        "trace": {
            "version": 1,
            "total_ms": marks[-1][1] if marks else 0.0,
            "marks": [{"name": name, "at_ms": at} for name, at in marks],
            "spans": spans or [],
        },
    }
    prefix = (
        startup_bench.TRACE_PREFIX
        if event == "startup-trace"
        else startup_bench.CLOSE_PREFIX
    )
    return prefix + json.dumps(payload)


class ParseStreamsTest(unittest.TestCase):
    def test_both_reports_are_read_from_noisy_output(self) -> None:
        output = "\n".join(
            [
                "desktop: enter",
                "[WGPU] Selected Adapter: Test GPU",
                trace_line(
                    "startup-trace",
                    [("engine: entry", 1.0), ("engine: first frame presented", 1500.0)],
                ),
                trace_line(
                    "startup-close",
                    [
                        ("engine: close requested", 1510.0),
                        ("engine: window hidden", 1515.0),
                        ("engine: teardown done", 1600.0),
                    ],
                ),
            ]
        )
        first, close = startup_bench.parse_streams(output)
        self.assertEqual(first["engine: first frame presented"], 1500.0)
        self.assertEqual(close["engine: window hidden"], 1515.0)
        self.assertEqual(
            close["engine: teardown done"] - close["engine: close requested"],
            90.0,
        )

    def test_a_run_without_trace_yields_empty_dicts(self) -> None:
        first, close = startup_bench.parse_streams("nothing here\n")
        self.assertEqual(first, {})
        self.assertEqual(close, {})

    def test_span_durations_come_from_the_first_present_report(self) -> None:
        output = trace_line(
            "startup-trace",
            [("engine: entry", 0.0)],
            spans=[
                {"name": "motioner: font Inter 400", "start_ms": 100.0, "end_ms": 280.0}
            ],
        )
        self.assertAlmostEqual(
            startup_bench.span_durations(output)["motioner: font Inter 400"], 180.0
        )


class PercentileTest(unittest.TestCase):
    def test_p90_uses_nearest_rank(self) -> None:
        values = [float(value) for value in range(1, 21)]
        self.assertEqual(startup_bench.p90(values), 18.0)

    def test_p90_of_one_sample_is_that_sample(self) -> None:
        self.assertEqual(startup_bench.p90([7.5]), 7.5)

    def test_unsorted_input_is_sorted_first(self) -> None:
        self.assertEqual(startup_bench.p90([20.0, 1.0, 10.0]), 20.0)


class WindowOrderTest(unittest.TestCase):
    def test_a_normal_path_visible_after_the_first_frame_passes(self) -> None:
        marks = {
            startup_bench.FIRST_PRESENT: 500.0,
            startup_bench.WINDOW_VISIBLE: 500.1,
        }
        self.assertIsNone(startup_bench.window_before_first_frame(marks))

    def test_a_normal_path_visible_before_the_first_frame_fails(self) -> None:
        marks = {
            startup_bench.FIRST_PRESENT: 500.0,
            startup_bench.WINDOW_VISIBLE: 50.0,
        }
        violation = startup_bench.window_before_first_frame(marks)
        self.assertIsNotNone(violation)
        self.assertIn("before the first frame", violation)

    def test_the_timeout_path_is_accepted(self) -> None:
        marks = {
            startup_bench.FIRST_PRESENT: 900.0,
            startup_bench.WINDOW_VISIBLE_TIMEOUT: 450.0,
        }
        self.assertIsNone(startup_bench.window_before_first_frame(marks))

    def test_missing_marks_do_not_invent_a_violation(self) -> None:
        self.assertIsNone(startup_bench.window_before_first_frame({}))


class BudgetTest(unittest.TestCase):
    def budgets(self) -> dict:
        return {
            "scenarios": {
                "start": {
                    "metrics": {
                        "span: motioner: host": {
                            "median_ms": 5.0,
                            "p90_ms": 6.0,
                        }
                    }
                }
            }
        }

    def test_medians_and_p90_within_budget_pass(self) -> None:
        metrics = startup_bench.Metrics()
        for value in (0.2, 0.3, 0.4):
            metrics.add("span: motioner: host", value)
        self.assertEqual(
            startup_bench.evaluate_budgets("start", metrics, self.budgets()), []
        )

    def test_a_median_over_budget_fails(self) -> None:
        metrics = startup_bench.Metrics()
        for value in (10.0, 11.0, 12.0):
            metrics.add("span: motioner: host", value)
        failures = startup_bench.evaluate_budgets("start", metrics, self.budgets())
        self.assertTrue(any("median" in failure for failure in failures))

    def test_a_missing_metric_fails_instead_of_silently_passing(self) -> None:
        failures = startup_bench.evaluate_budgets(
            "start", startup_bench.Metrics(), self.budgets()
        )
        self.assertTrue(any("was not measured" in failure for failure in failures))

    def test_generated_budgets_are_double_and_floored(self) -> None:
        metrics = startup_bench.Metrics()
        metrics.add("span: motioner: host", 0.4)
        metrics.add(startup_bench.OUR_CRITICAL_PATH, 100.0)
        metrics.add(startup_bench.CLOSE_TO_HIDDEN, 2.0)
        # `start` requires the other metrics too; only the selected ones are
        # generated, so the dict is filtered by what was measured.
        entries = startup_bench.budget_entries(metrics, "start", 2.0)
        self.assertEqual(entries["span: motioner: host"]["median_ms"], 5.0)
        self.assertEqual(entries[startup_bench.OUR_CRITICAL_PATH]["median_ms"], 200.0)
        self.assertEqual(
            entries[startup_bench.CLOSE_TO_HIDDEN]["median_ms"],
            startup_bench.CLOSE_BUDGET_MS,
        )


class MetricsTest(unittest.TestCase):
    def test_rows_report_median_and_p90(self) -> None:
        metrics = startup_bench.Metrics()
        for value in (10.0, 20.0, 30.0):
            metrics.add("mark: engine: entry", value)
        (name, median, ninety, count) = metrics.rows()[0]
        self.assertEqual(name, "mark: engine: entry")
        self.assertEqual(median, 20.0)
        self.assertEqual(ninety, 30.0)
        self.assertEqual(count, 3)

    def test_json_rounds_and_keeps_the_run_count(self) -> None:
        metrics = startup_bench.Metrics()
        metrics.add("close: requested to hidden", 5.12345)
        payload = metrics.to_json()
        self.assertEqual(payload["close: requested to hidden"]["median_ms"], 5.123)
        self.assertEqual(payload["close: requested to hidden"]["runs"], 1)


if __name__ == "__main__":
    unittest.main()
