"""Measure native compiler wall time and Linux peak RSS through GNU Time.

The native observer forks the compiler from its small address space, avoiding
Python's inherited RSS floor. Timing includes observer startup and wait overhead.
"""

import shutil
import subprocess
import sys
import tempfile
import time


def measure(command):
    if not sys.platform.startswith("linux"):
        raise RuntimeError("the current peak-RSS measurement protocol requires Linux")
    observer = shutil.which("time")
    if observer is None:
        raise RuntimeError("native peak-RSS measurement requires GNU Time")
    with (
        tempfile.TemporaryFile() as output,
        tempfile.TemporaryFile() as errors,
        tempfile.NamedTemporaryFile() as metrics,
    ):
        started = time.perf_counter()
        process = subprocess.run(
            [observer, "--quiet", "--format=%M", "--output=" + metrics.name, "--", *command],
            stdout=output,
            stderr=errors,
        )
        elapsed = time.perf_counter() - started
        metrics.seek(0)
        try:
            # Signal diagnostics may precede the final numeric record.
            peak_kib = int(metrics.read().splitlines()[-1])
            if peak_kib < 0:
                raise ValueError("negative peak RSS")
        except (IndexError, ValueError) as error:
            raise RuntimeError("native observer did not report compiler peak RSS") from error
        output.seek(0)
        errors.seek(0)
        return {
            "wall_seconds": elapsed,
            "compiler_peak_rss_bytes": peak_kib * 1024,
            "status": process.returncode,
            "stdout": output.read().decode().strip(),
            "stderr": errors.read().decode().strip(),
        }
