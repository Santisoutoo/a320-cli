"""JSONL trajectory writer: one file per run, one typed record per line.

The trajectory is the benchmark's primary artifact — #20 scores it without
re-simulating — so it is self-contained: the scenario is embedded in the meta
record, every tool call carries its result and the simulated clock around it,
and the final record carries the harness's own success evaluation.
"""

import json
from pathlib import Path
from types import TracebackType
from typing import Any

TRAJECTORY_SCHEMA_VERSION = 1


class TrajectoryRecorder:
    """Appends typed records to a JSONL file. Use as a context manager."""

    def __init__(self, path: "str | Path"):
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self._file = self.path.open("w", encoding="utf-8", newline="\n")
        self._records = 0

    def write(self, type_: str, **fields: Any) -> None:
        record = {"type": type_, **fields}
        # default=str: a record must never kill a run mid-episode over a
        # non-JSON value; a stringified oddity is diagnosable, a crash is not.
        self._file.write(json.dumps(record, ensure_ascii=False, default=str) + "\n")
        self._file.flush()  # a crashed run keeps its partial trajectory
        self._records += 1

    @property
    def records_written(self) -> int:
        return self._records

    def close(self) -> None:
        self._file.close()

    def __enter__(self) -> "TrajectoryRecorder":
        return self

    def __exit__(
        self,
        exc_type: "type[BaseException] | None",
        exc: "BaseException | None",
        tb: "TracebackType | None",
    ) -> None:
        self.close()


def read_trajectory(path: "str | Path") -> list[dict[str, Any]]:
    """Read a trajectory back as a list of records (the #20 entry point).

    Tolerates a truncated *final* line (an OS-level crash mid-write leaves
    one): the partial trajectory is still evidence. A malformed line anywhere
    else is corruption and raises.
    """
    lines = [
        line for line in Path(path).read_text(encoding="utf-8").splitlines() if line.strip()
    ]
    records = []
    for i, line in enumerate(lines):
        try:
            records.append(json.loads(line))
        except json.JSONDecodeError:
            if i == len(lines) - 1:
                break
            raise
    return records
