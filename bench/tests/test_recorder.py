"""Recorder round-trip and robustness (#70)."""

import json
import tempfile
from pathlib import Path

from a320_bench.recorder import TrajectoryRecorder, read_trajectory


def test_round_trip_preserves_typed_records():
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "runs" / "x" / "r.jsonl"  # parents created by the recorder
        with TrajectoryRecorder(path) as rec:
            rec.write("meta", run_id="r1", nested={"a": [1, 2]}, flag=True)
            rec.write("final", reason="agent_done", value=1.5)
            assert rec.records_written == 2

        records = read_trajectory(path)
        assert [r["type"] for r in records] == ["meta", "final"]
        assert records[0]["nested"] == {"a": [1, 2]}
        assert records[1]["value"] == 1.5


def test_non_json_values_are_stringified_not_fatal():
    """A weird value must never kill a run mid-episode (default=str)."""
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "r.jsonl"
        with TrajectoryRecorder(path) as rec:
            rec.write("meta", path_obj=Path("somewhere"))
        record = read_trajectory(path)[0]
        assert record["path_obj"] == "somewhere"


def test_lines_are_valid_json_one_per_record():
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "r.jsonl"
        with TrajectoryRecorder(path) as rec:
            for i in range(5):
                rec.write("tool_call", i=i)
        lines = path.read_text(encoding="utf-8").splitlines()
        assert len(lines) == 5
        assert all(json.loads(line)["type"] == "tool_call" for line in lines)


if __name__ == "__main__":
    tests = sorted(
        (name, fn) for name, fn in globals().items() if name.startswith("test_") and callable(fn)
    )
    for name, fn in tests:
        fn()
        print(f"ok  {name}")
    print(f"\n{len(tests)} recorder tests passed.")
