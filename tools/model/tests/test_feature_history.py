"""Feature timeline actions must remain usable through the generated MCP contract."""
import json
import pathlib
import tempfile
import unittest
from client import Client

class FeatureHistoryTests(unittest.TestCase):
    def test_checkout_suppress_and_move_with_states_and_guards(self):
        with tempfile.TemporaryDirectory(prefix='occb-history-mcp-') as folder:
            client = Client(pathlib.Path(folder) / 'builds')
            try:
                request = client.tool('occt_get_example', dict(name='feature-history'))['structuredContent']
                request.update(step=False, stl=False, preview=False)
                result = client.tool('occt_build', request)
                self.assertFalse(result['isError'], result)
                parent = result['structuredContent']
                states = parent['report']['feature_states'][0]['features']
                self.assertEqual({s['state'] for s in states}, {'active', 'suppressed', 'blocked', 'rolled_back'})
                original_path = pathlib.Path(parent['directory'], 'model.json')
                original = original_path.read_bytes()
                model = json.loads(original)
                for index, change in enumerate((
                    dict(action='checkout_feature', head=None),
                    dict(action='suppress_feature', feature='notched', suppressed=False),
                    dict(action='move_feature', feature='tool', after=None),
                )):
                    change.update(family='history-part', expected=([f['id'] for f in model['family']['features']] if change['action']=='move_feature' else model['family'].get('history', {})))
                    args = dict(build_id=parent['build_id'], expected_model_sha256=parent['model_sha256'], changes=[change], outputs=request['outputs'],
                        revision=dict(id=f'history-{index}', author='test', recorded_at='test-time', message=change['action']), step=False, stl=False, preview=False)
                    result = client.tool('occt_edit_build', args)
                    self.assertFalse(result['isError'], result)
                    parent = result['structuredContent']
                    model = json.loads(pathlib.Path(parent['directory'], 'model.json').read_text())
                self.assertEqual(model['family']['features'][0]['id'], 'tool')
                self.assertTrue(any(c['path'][-1]=={'field':'feature_order'} for c in model['revisions'][-1]['changes']))
                self.assertEqual(original_path.read_bytes(), original)
                args.update(build_id=parent['build_id'], expected_model_sha256=parent['model_sha256'], changes=[dict(action='checkout_feature',family='history-part',expected={},head='base')])
                rejected = client.tool('occt_edit_build', args)
                self.assertTrue(rejected['isError'])
                self.assertEqual(rejected['structuredContent']['stage'], 'conflict')
            finally:
                client.close()
