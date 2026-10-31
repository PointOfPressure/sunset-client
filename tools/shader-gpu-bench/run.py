"""Run a prepared, explicitly enabled fixture and collect GPU clock telemetry."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

HERE = Path(__file__).resolve().parent


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path, default=HERE / "fixture")
    parser.add_argument("--binary", type=Path, required=True, help="Built sunset-shader-gpu-bench executable")
    parser.add_argument("--skip-png", action="store_true", help="Keep raw BGRA captures without requiring Pillow")
    args = parser.parse_args()
    output, binary = args.fixture.resolve(), args.binary.resolve()
    gate = output / "GO_GPU_RENDER"
    if not gate.is_file():
        parser.error("Create GO_GPU_RENDER explicitly when ready to use the GPU")
    provenance = json.loads((output / "provenance.json").read_text(encoding="utf-8"))
    for group in ["source_files", "fixture_files", "harness_files"]:
        for name, expected in provenance[group].items():
            assert hashlib.sha256(Path(name).read_bytes()).hexdigest() == expected, f"Frozen input changed: {name}"
    if (output / "gpu-samples.csv").exists():
        parser.error("Fixture already has measured samples; prepare a fresh --output directory")
    execution = {
        "started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "binary": str(binary),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "permission_gate": gate.read_text(encoding="utf-8"),
        "BENCH_ROUNDS": os.environ.get("BENCH_ROUNDS", "8"),
        "BENCH_SAMPLES": os.environ.get("BENCH_SAMPLES", "64"),
    }
    hidden = getattr(subprocess, "CREATE_NO_WINDOW", 0)
    monitor = process = None
    try:
        with (output / "nvidia-timeline.csv").open("w") as telemetry, \
             (output / "nvidia-monitor.stderr.txt").open("w") as monitor_errors, \
             (output / "console.txt").open("w") as console:
            try:
                monitor = subprocess.Popen([
                    "nvidia-smi", "--query-gpu=timestamp,name,driver_version,pstate,clocks.current.graphics,clocks.current.memory,temperature.gpu,power.draw,power.limit,utilization.gpu",
                    "--format=csv,noheader,nounits", "-lms", "200",
                ], stdout=telemetry, stderr=monitor_errors, creationflags=hidden)
            except OSError as error:
                monitor_errors.write(str(error))
            process = subprocess.Popen(
                [str(binary), str(output)], stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT, text=True, creationflags=hidden,
            )
            for line in process.stdout:
                print(line, end="", flush=True)
                console.write(line)
                console.flush()
            execution["exit_code"] = process.wait()
    finally:
        # Only processes created by this invocation are stopped here.
        for owned in [process, monitor]:
            if owned is not None and owned.poll() is None:
                owned.terminate()
                try:
                    owned.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    owned.kill()
                    owned.wait()
        gate.unlink(missing_ok=True)
        execution["completed_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        (output / "execution.json").write_text(json.dumps(execution, indent=2), encoding="utf-8")
    assert execution.get("exit_code") == 0, execution
    command = [sys.executable, str(HERE / "summarize.py"), "--fixture", str(output)]
    if args.skip_png:
        command.append("--skip-png")
    subprocess.run(command, check=True)


if __name__ == "__main__":
    main()
