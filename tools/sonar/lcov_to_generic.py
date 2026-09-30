#!/usr/bin/env python3
"""Convert an LCOV tracefile to SonarQube generic coverage XML."""

from __future__ import annotations

import argparse
from collections import defaultdict
from pathlib import Path
import xml.etree.ElementTree as ET


def parse_lcov(path: Path, root: Path) -> dict[str, dict[int, dict[str, int]]]:
    files: dict[str, dict[int, dict[str, int]]] = {}
    source: str | None = None
    lines: dict[int, int] = {}
    branches: dict[int, list[bool]] = defaultdict(list)

    def finish_record() -> None:
        nonlocal source, lines, branches
        if source is None:
            return
        source_path = Path(source)
        if not source_path.is_absolute():
            source_path = root / source_path
        try:
            relative = source_path.resolve().relative_to(root).as_posix()
        except ValueError:
            source = None
            lines = {}
            branches = defaultdict(list)
            return
        covered = {}
        for number, hits in sorted(lines.items()):
            values = branches.get(number, [])
            covered[number] = {
                "hits": hits,
                "branches": len(values),
                "covered_branches": sum(values),
            }
        if covered:
            files[relative] = covered
        source = None
        lines = {}
        branches = defaultdict(list)

    for raw in path.read_text(encoding="utf-8").splitlines():
        if raw.startswith("SF:"):
            finish_record()
            source = raw[3:]
        elif raw.startswith("DA:") and source is not None:
            fields = raw[3:].split(",")
            number, hits = int(fields[0]), int(fields[1])
            lines[number] = max(lines.get(number, 0), hits)
        elif raw.startswith("BRDA:") and source is not None:
            fields = raw[5:].split(",")
            number = int(fields[0])
            branches[number].append(fields[3] != "-" and int(fields[3]) > 0)
        elif raw == "end_of_record":
            finish_record()
    finish_record()
    return files


def write_generic_coverage(
    files: dict[str, dict[int, dict[str, int]]], output: Path
) -> None:
    coverage = ET.Element("coverage", version="1")
    for path, lines in sorted(files.items()):
        file_element = ET.SubElement(coverage, "file", path=path)
        for number, values in sorted(lines.items()):
            attributes = {
                "lineNumber": str(number),
                "covered": str(values["hits"] > 0).lower(),
            }
            if values["branches"]:
                attributes["branchesToCover"] = str(values["branches"])
                attributes["coveredBranches"] = str(values["covered_branches"])
            ET.SubElement(file_element, "lineToCover", attributes)
    ET.indent(coverage)
    output.parent.mkdir(parents=True, exist_ok=True)
    ET.ElementTree(coverage).write(output, encoding="utf-8", xml_declaration=True)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("lcov", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    args = parser.parse_args()
    root = args.root.resolve()
    files = parse_lcov(args.lcov, root)
    if not files:
        raise SystemExit("LCOV report contains no covered source records under the project root")
    write_generic_coverage(files, args.output)
    line_count = sum(len(lines) for lines in files.values())
    print(f"Converted {len(files)} files and {line_count} executable lines")


if __name__ == "__main__":
    main()
