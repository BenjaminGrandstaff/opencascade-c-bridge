"""Printable-map examples must build as single solids and export closed meshes."""
import base64
import pathlib
import sys
import tempfile
import unittest

from client import Client

ROOT = pathlib.Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'tools/model/maps'))
from build import clean_stl


class PrintableMapTests(unittest.TestCase):
    def test_five_map_examples_build_and_export_closed_meshes(self):
        with tempfile.TemporaryDirectory(prefix='occb-map-tests-') as folder:
            client = Client(pathlib.Path(folder) / 'builds')
            try:
                for slug in ('crossroads', 'catacombs', 'shrine', 'bridge', 'cavern'):
                    with self.subTest(map=slug):
                        request = client.tool('occt_get_example', dict(name='map-' + slug))['structuredContent']
                        request.update(step=False, preview=False)
                        request.pop('renders')
                        result = client.tool('occt_build', request)
                        self.assertFalse(result['isError'], result)
                        build = result['structuredContent']
                        report = build['report']
                        self.assertTrue(all(r['status'] == 'passed' for r in report['verification']))
                        self.assertEqual(next(r for r in report['verification'] if r['requirement'] == 'one-solid')['measured']['value'], 1)
                        output = report['outputs'][0]
                        self.assertTrue(output['valid'])
                        self.assertAlmostEqual(output['bounds_mm']['min'][2], 0)
                        for axis in (0, 1):
                            self.assertLessEqual(output['bounds_mm']['max'][axis] - output['bounds_mm']['min'][axis], 220)
                        uri = build['resources'][output['stl']]
                        resource = client.call('resources/read', dict(uri=uri))['result']['contents'][0]
                        _, mesh = clean_stl(base64.b64decode(resource['blob']))
                        self.assertEqual(mesh['nonmanifold_edges'], 0)
                        self.assertGreater(mesh['triangles'], 100)
            finally:
                client.close()


if __name__ == '__main__':
    unittest.main()
