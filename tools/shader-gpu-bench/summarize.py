"""Summarize raw timestamp samples and optionally encode exact BGRA readbacks."""
import argparse
from collections import defaultdict
import csv
import hashlib
import json
from pathlib import Path
import statistics


def distribution(values):
    values = sorted(values)

    def quantile(fraction):
        index = (len(values) - 1) * fraction
        low = int(index)
        return values[low] + (values[min(low + 1, len(values) - 1)] - values[low]) * (index - low)

    return {"n": len(values), "mean_ms": statistics.mean(values),
            "median_ms": statistics.median(values), "p95_ms": quantile(.95),
            "p99_ms": quantile(.99), "min_ms": values[0], "max_ms": values[-1]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixture", type=Path, default=Path(__file__).resolve().parent / "fixture")
    parser.add_argument("--skip-png", action="store_true", help="Summarize timings without Pillow/image encoding")
    args = parser.parse_args()
    output = args.fixture.resolve()
    provenance = json.loads((output / "provenance.json").read_text(encoding="utf-8"))
    for group in ["source_files", "fixture_files", "harness_files"]:
        for name, expected in provenance[group].items():
            actual = hashlib.sha256(Path(name).read_bytes()).hexdigest()
            assert actual == expected, f"Changed frozen source/fixture: {name}"
    with (output / "gpu-samples.csv").open() as source:
        rows = list(csv.DictReader(source))
    groups = defaultdict(list)
    pairs = defaultdict(lambda: defaultdict(list))
    for row in rows:
        value = float(row["gpu_ms"])
        assert 0 < value < 1000
        groups[(row["case"], row["variant"])].append(value)
        if row["case"] != "native":
            pairs[(row["case"], int(row["round"]))][row["variant"]].append(value)
    adapter = (output / "adapter.txt").read_text()
    adapter_fields = dict(line.split("=", 1) for line in adapter.splitlines()[1:] if "=" in line)
    rounds = int(adapter_fields["rounds"])
    samples = int(adapter_fields["samples_per_variant_per_round"])
    expected_cases = {f"{mode}-{quality}" for mode in ["space", "aurora", "planet"]
                      for quality in ["balanced", "performance"]}
    assert set(groups) == {("native", "native")} | {(case, variant) for case in expected_cases for variant in ["original", "optimized"]}
    assert len(groups[("native", "native")]) == samples * 2
    for case in expected_cases:
        for variant in ["original", "optimized"]:
            assert len(groups[(case, variant)]) == rounds * samples
        for round_index in range(rounds):
            for variant in ["original", "optimized"]:
                assert len(pairs[(case, round_index)][variant]) == samples
    summary = {"kind": provenance["kind"], "scope": provenance["timing_scope"],
               "globals_provenance": provenance["globals_provenance"],
               "distributions": [], "paired_rounds": [], "comparisons": []}
    for (case, variant), values in groups.items():
        summary["distributions"].append({"case": case, "variant": variant, **distribution(values)})
    for (case, round_index), variants in pairs.items():
        old, new = map(statistics.median, [variants["original"], variants["optimized"]])
        summary["paired_rounds"].append({"case": case, "round": round_index,
            "original_median_ms": old, "optimized_median_ms": new,
            "reduction_percent": (old - new) / old * 100})
    for case in sorted(expected_cases):
        old, new = map(statistics.median, [groups[(case, "original")], groups[(case, "optimized")]])
        reductions = [item["reduction_percent"] for item in summary["paired_rounds"] if item["case"] == case]
        summary["comparisons"].append({"case": case, "original_median_ms": old,
            "optimized_median_ms": new, "median_reduction_percent": (old - new) / old * 100,
            "paired_reduction_percent_range": [min(reductions), max(reductions)]})
    summary["captures"] = {}
    if not args.skip_png:
        from PIL import Image

        for path in output.glob("*.bgra"):
            raw = path.read_bytes()
            assert len(raw) == 2000 * 1125 * 4
            image = Image.frombytes("RGBA", (2000, 1125), raw, "raw", "BGRA")
            target = path.with_suffix(".png")
            image.save(target)
            summary["captures"][str(target)] = {
                "sha256": hashlib.sha256(target.read_bytes()).hexdigest(),
                "raw_bgra_sha256": hashlib.sha256(raw).hexdigest(),
                "dimensions": [2000, 1125],
                "processing": "Exact GPU byte readback encoded as PNG; no crop/rescale/composite/color adjustment.",
            }
    summary["adapter"] = adapter
    summary["nvidia_smi"] = (output / "nvidia-smi.txt").read_text()
    summary["source_hashes_unchanged"] = True
    (output / "summary.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    print(json.dumps(summary["comparisons"], indent=2))


if __name__ == "__main__":
    main()
