#!/usr/bin/env python3

import tempfile
from pathlib import Path
import sys
import unittest
import xml.etree.ElementTree as ET

sys.path.insert(0, str(Path(__file__).resolve().parent))

from lcov_to_generic import parse_lcov, write_generic_coverage


class LcovConversionTest(unittest.TestCase):
    def test_converts_lines_branches_and_ignores_external_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "src" / "example.cpp"
            source.parent.mkdir()
            source.write_text("one\ntwo\n", encoding="utf-8")
            lcov = root / "lcov.info"
            lcov.write_text(
                "\n".join(
                    [
                        f"SF:{source}",
                        "DA:1,3",
                        "DA:2,0",
                        "BRDA:1,0,0,2",
                        "BRDA:1,0,1,-",
                        "end_of_record",
                        "SF:/outside/project.cpp",
                        "DA:1,1",
                        "end_of_record",
                    ]
                ),
                encoding="utf-8",
            )
            files = parse_lcov(lcov, root)
            self.assertEqual(list(files), ["src/example.cpp"])
            output = root / "coverage.xml"
            write_generic_coverage(files, output)
            tree = ET.parse(output)
            lines = tree.findall("./file/lineToCover")
            self.assertEqual(lines[0].attrib["covered"], "true")
            self.assertEqual(lines[0].attrib["branchesToCover"], "2")
            self.assertEqual(lines[0].attrib["coveredBranches"], "1")
            self.assertEqual(lines[1].attrib["covered"], "false")


if __name__ == "__main__":
    unittest.main()
