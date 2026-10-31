#!/usr/bin/env python3
# The test matrix's table from matrix.sh's logs: one row a test, one column a server, each cell PASS, FAIL or SKIP
# with what the test measured (cut short), then the kicks. Usage: matrix_report.py LOG... > table.md (a log's name
# without .log names its column).
import os, re, sys

LINE = re.compile(r"^(PASS|FAIL|SKIP) (.+?) - (.*)$")


def cell(result, note, room=110):
    note = note.replace("|", "/").strip()
    if len(note) > room:
        note = note[:room - 3].rstrip() + "..."
    return f"**{result}** {note}"


def main(paths):
    servers, rows, order, kicks, totals = [], {}, [], {}, {}
    for path in paths:
        name = os.path.basename(path).rsplit(".", 1)[0]
        servers.append(name)
        with open(path, errors="replace") as f:
            for line in f:
                line = line.rstrip("\n")
                if m := LINE.match(line):
                    result, test, note = m.groups()
                    if test not in rows:
                        rows[test] = {}
                        order.append(test)
                    rows[test][name] = (result, note)
                elif line.startswith("KICKED during "):
                    kicks.setdefault(name, []).append(line[len("KICKED during "):])
                elif re.match(r"^\d+/\d+ passed$", line):
                    totals[name] = line
    print("| Test | " + " | ".join(servers) + " |")
    print("|---|" + "---|" * len(servers))
    for test in order:
        cells = [cell(*rows[test][s]) if s in rows[test] else "not run" for s in servers]
        print(f"| {test} | " + " | ".join(cells) + " |")
    print()
    print("| Totals | " + " | ".join(totals.get(s, "did not finish") for s in servers) + " |")
    print("|---|" + "---|" * len(servers))
    for s in servers:
        for k in kicks.get(s, []):
            print(f"\nKicked on {s} during {k}")


if __name__ == "__main__":
    main(sys.argv[1:])
