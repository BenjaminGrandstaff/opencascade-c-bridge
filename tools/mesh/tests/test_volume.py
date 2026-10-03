import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("volume", Path(__file__).parents[1] / "volume.py")
volume = importlib.util.module_from_spec(spec)
spec.loader.exec_module(volume)


def face(index, x):
    return {"index": index, "physical_tag": 1, "area_mm2": 100.0,
            "center_mm": [x, 5.0, 5.0], "bounds_mm": [[x, 0.0, 0.0], [x, 10.0, 10.0]]}


class FaceMatching(unittest.TestCase):
    def test_numbering_and_tolerance_do_not_determine_boundary_identity(self):
        expected = [face(0, 0.0), face(1, 10.0)]
        actual = [dict(face(999, 10.0), entity=30), dict(face(999, 0.0), entity=20)]
        actual[0]["center_mm"][0] += 1e-7
        actual[1]["bounds_mm"][0][0] -= 1e-7
        self.assertEqual(volume.match_faces(expected, actual, 1e-5), {30: 1, 20: 1})

    def test_ambiguous_missing_and_duplicate_matches_fail(self):
        expected = [face(0, 0.0), face(1, 10.0)]
        actual = [dict(face(999, 10.0), entity=30), dict(face(999, 0.0), entity=20)]
        for invalid in [actual[:1], [actual[0], actual[0]], [dict(actual[0], area_mm2=200.0), actual[1]]]:
            with self.assertRaises(ValueError):
                volume.match_faces(expected, invalid, 1e-5)
        coincident = [face(0, 0.0), face(1, 0.0)]
        with self.assertRaisesRegex(ValueError, "ambiguous"):
            volume.match_faces(coincident, [dict(face(0, 0.0), entity=1), dict(face(1, 0.0), entity=2)], 1e-5)

    def test_invalid_manifests_are_rejected_before_meshing(self):
        valid = {"version": 1, "units": "mm", "volume_name": "part", "face_tags": ["fixed"], "faces": [face(0, 0.0)]}
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "volume.json"
            path.write_text(json.dumps(valid))
            self.assertEqual(volume.load_manifest(path), valid)
            for mutation in range(9):
                bad = copy.deepcopy(valid)
                if mutation == 0:
                    bad = []
                elif mutation == 1:
                    bad["version"] = 2
                elif mutation == 2:
                    bad["face_tags"] = ["fixed", "fixed"]
                elif mutation == 3:
                    bad["volume_name"] = "bad\nname"
                elif mutation == 4:
                    bad["faces"] = []
                elif mutation == 5:
                    bad["faces"] += copy.deepcopy(bad["faces"])
                elif mutation == 6:
                    bad["faces"][0]["physical_tag"] = 2
                elif mutation == 7:
                    bad["faces"][0]["area_mm2"] = -1
                else:
                    bad["faces"][0]["bounds_mm"][0][0] = 100.0
                path.write_text(json.dumps(bad))
                with self.assertRaises(ValueError):
                    volume.load_manifest(path)
            path.write_text(json.dumps(valid))
            for size, count, tolerance in [(0.0, 10, 1e-5), (1.0, 0, 1e-5), (1.0, 10, 0.0)]:
                with self.assertRaises(ValueError):
                    volume.generate(Path(directory), Path(directory)/"mesh.msh", size, count, tolerance)
            output = Path(directory)/"mesh.msh"
            output.write_text("preserve")
            with self.assertRaises(FileExistsError):
                volume.generate(Path(directory), output, 1.0, 10000, 1e-5)
            self.assertEqual(output.read_text(), "preserve")

    def test_ten_thousand_faces_use_geometric_buckets(self):
        import time
        expected = [face(index, float(index)) for index in range(10000)]
        actual = [dict(entry, entity=entry["index"]+1) for entry in reversed(expected)]
        start = time.monotonic()
        self.assertEqual(len(volume.match_faces(expected, actual, 1e-5)), 10000)
        self.assertLess(time.monotonic()-start, 3.0)


if __name__ == "__main__":
    unittest.main()
