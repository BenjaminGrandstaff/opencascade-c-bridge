"""Wire-protocol, schema, and geometry checks against the actual model binary."""
import json
import math
import os
import pathlib
import sys
import tempfile
import time
import unittest

import jsonschema

ROOT = pathlib.Path(__file__).resolve().parents[3]
BINARY = pathlib.Path(os.environ.get('OCCT_MODEL_BINARY', ROOT / 'rust/occt-parametric/target/debug/occt-model')).resolve()
SERVER = ROOT / 'tools/model/mcp_server.py'

from client import Client

class McpTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='occb-mcp-test-')
        self.root = pathlib.Path(self.temporary.name)
        self.client = Client(self.root / 'builds')

    def tearDown(self):
        self.client.close()
        self.temporary.cleanup()

    def test_initialization_order_version_negotiation_and_invalid_client_info(self):
        self.client.close()
        self.client = Client(self.root / 'unready', initialize=False)
        self.assertEqual(self.client.call('tools/list')['error']['code'], -32600)
        self.assertEqual(self.client.call('initialize', dict(protocolVersion='2025-06-18', capabilities={}, clientInfo={}))['error']['code'], -32602)
        result = self.client.call('initialize', dict(protocolVersion='2025-06-18', capabilities={}, clientInfo=dict(name='test', version='1')))
        self.assertEqual(result['result']['protocolVersion'], '2025-06-18')
        self.assertEqual(self.client.call('tools/list')['error']['code'], -32600)
        self.client.send(dict(jsonrpc='2.0', method='notifications/initialized'))
        self.assertEqual(len(self.client.call('tools/list')['result']['tools']), 7)
        self.assertEqual(self.client.call('initialize')['error']['code'], -32600)

    def test_discovery_schemas_and_complete_examples(self):
        tools = self.client.call('tools/list')['result']['tools']
        self.assertEqual({tool['name'] for tool in tools}, {'occt_get_schema', 'occt_get_example', 'occt_build', 'occt_inspect_model', 'occt_inspect_build', 'occt_edit_build', 'occt_visualize_model'})
        schema = next(tool['inputSchema'] for tool in tools if tool['name'] == 'occt_build')
        jsonschema.Draft202012Validator.check_schema(schema)
        self.assertEqual(schema['properties']['model']['$ref'], '#/$defs/ModelDocument')
        variants = schema['$defs']['FeatureOperation']['oneOf']
        self.assertEqual(len(variants), 38)
        for name in ['request', 'model', 'feature', 'parameter', 'sketch', 'requirement', 'inspection', 'face_selector', 'edge_selector', 'edit', 'change', 'view']:
            result = self.client.tool('occt_get_schema', dict(name=name))
            self.assertFalse(result['isError'])
            jsonschema.Draft202012Validator.check_schema(result['structuredContent'])
        for name in ['bracket', 'enclosure', 'shaft', 'mating-parts']:
            example = self.client.tool('occt_get_example', dict(name=name))['structuredContent']
            jsonschema.validate(example, schema)
        bad = self.client.tool('occt_get_example', dict(name='shaft'))['structuredContent']
        bad['unexpected'] = True
        with self.assertRaises(jsonschema.ValidationError):
            jsonschema.validate(bad, schema)
        resources = self.client.call('resources/list')['result']['resources']
        self.assertEqual(len(resources), 44)
        self.assertEqual(len(self.client.call('resources/templates/list')['result']['resourceTemplates']), 1)
        read = self.client.call('resources/read', dict(uri='occt://schema/request'))['result']['contents'][0]
        self.assertEqual(json.loads(read['text']), schema)

    def test_build_edit_reject_repair_and_artifact_resources(self):
        request = self.client.tool('occt_get_example', dict(name='bracket'))['structuredContent']
        built = self.client.tool('occt_build', request)
        self.assertFalse(built['isError'])
        content = built['structuredContent']
        self.assertEqual(json.loads(built['content'][0]['text']), content)
        self.assertEqual(content['report']['status'], 'built')
        original_report = pathlib.Path(content['directory'], 'report.json').read_bytes()
        def read(name):
            return self.client.call('resources/read', dict(uri=content['resources'][name]))['result']['contents'][0]
        saved = json.loads(read('model.json')['text'])
        for actual, expected in zip(saved['family']['requirements'], request['model']['family']['requirements']):
            for field in ['id', 'version', 'kind', 'priority', 'statement', 'provenance', 'traces']:
                self.assertEqual(actual[field], expected[field])
            self.assertEqual(actual['rule'].keys(), expected['rule'].keys())
            if 'minimum_wall' in actual['rule']:
                actual['rule']['minimum_wall'].setdefault('maximum_samples', 1000)
            self.assertEqual(actual['rule'], expected['rule'])
        self.assertIn('<polyline', read('0001.svg')['text'])
        self.assertTrue(read('parts.step')['blob'])
        self.assertTrue(read('0001.stl')['blob'])
        request.update(model=saved, preview=False, step=False, stl=False)
        request['edits'] = [dict(instance='bracket', parameter='hole_spacing', value=dict(scalar=dict(value=35, dimension='length', unit='millimeter')))]
        edited = self.client.tool('occt_build', request)['structuredContent']
        persisted = json.loads(self.client.call('resources/read', dict(uri=edited['resources']['model.json']))['result']['contents'][0]['text'])
        request['model'] = persisted
        request['edits'] = [dict(instance='bracket', parameter='thickness', value=dict(scalar=dict(value=1, dimension='length', unit='millimeter')))]
        rejected = self.client.tool('occt_build', request)
        self.assertTrue(rejected['isError'])
        self.assertEqual(rejected['structuredContent']['stage'], 'regeneration')
        self.assertIn('wall', rejected['structuredContent']['message'])
        self.assertEqual(len(list((self.root / 'builds').iterdir())), 2)
        request['edits'][0]['value']['scalar']['value'] = 5
        repaired = self.client.tool('occt_build', request)
        self.assertFalse(repaired['isError'])
        self.assertEqual(pathlib.Path(content['directory'], 'report.json').read_bytes(), original_report)
        traversal = self.client.call('resources/read', dict(uri=f"occt://build/{content['build_id']}/../request.json"))
        self.assertEqual(traversal['error']['code'], -32002)
        forbidden = self.client.call('resources/read', dict(uri=f"occt://build/{content['build_id']}/kernel.stderr"))
        self.assertEqual(forbidden['error']['code'], -32002)

    def test_read_only_inspection_paging_queries_and_preserved_build(self):
        request = self.client.tool('occt_get_example', dict(name='shaft'))['structuredContent']
        request.update(step=False, stl=False, preview=False)
        inspection = dict(schema='occb-model-inspection-v1', model=request['model'], instance='shaft', limit=1)
        schema = self.client.tool('occt_get_schema', dict(name='inspection'))['structuredContent']
        jsonschema.validate(inspection, schema)
        inventory = self.client.tool('occt_inspect_model', inspection)
        self.assertFalse(inventory['isError'])
        self.assertFalse(inventory['structuredContent']['geometry_generated'])
        self.assertEqual(list((self.root / 'builds').iterdir()), [])
        built = self.client.tool('occt_build', request)['structuredContent']
        before = pathlib.Path(built['directory'], 'model.json').read_bytes()
        selector = {'normal_aligned': dict(direction={'literal': {key: dict(value=value, dimension='scalar', unit=None) for key,value in zip('xyz', (0,0,1))}}, minimum_dot={'literal': dict(value=0.99, dimension='scalar', unit=None)})}
        arguments = dict(build_id=built['build_id'], instance='shaft', output='body', face_selector=selector, limit=1)
        tools = self.client.call('tools/list')['result']['tools']
        jsonschema.validate(arguments, next(t['inputSchema'] for t in tools if t['name']=='occt_inspect_build'))
        report = self.client.tool('occt_inspect_build', arguments)
        self.assertFalse(report['isError'])
        geometry = report['structuredContent']['geometry']
        self.assertEqual(geometry['faces']['total'], 1)
        self.assertEqual(geometry['faces']['items'][0]['normal'], [0,0,1])
        self.assertAlmostEqual(geometry['faces']['items'][0]['area_mm2'], 25 * 3.141592653589793)
        self.assertEqual(geometry['edges']['next_offset'], 1)
        self.assertIn('family-local', geometry['coordinate_system'])
        arguments['offset'] = 1
        second = self.client.tool('occt_inspect_build', arguments)['structuredContent']
        self.assertEqual(second['geometry']['edges']['items'][0]['selection_index'], 1)
        self.assertEqual(pathlib.Path(built['directory'], 'model.json').read_bytes(), before)
        self.assertEqual(len(list((self.root / 'builds').iterdir())), 1)
        invalid = self.client.tool('occt_inspect_build', dict(build_id=built['build_id'], instance='shaft', output='missing'))
        self.assertTrue(invalid['isError'])
        self.assertEqual(invalid['structuredContent']['stage'], 'selection')
        self.assertEqual(self.client.call('tools/call', dict(name='occt_inspect_build', arguments=dict(build_id='../bad')))['error']['code'], -32602)

    def test_guarded_edit_build_revision_history_and_fingerprint_conflicts(self):
        request = self.client.tool('occt_get_example', dict(name='shaft'))['structuredContent']
        request.update(step=False, stl=False, preview=False)
        parent = self.client.tool('occt_build', request)['structuredContent']
        inspection = self.client.tool('occt_inspect_build', dict(build_id=parent['build_id'], instance='shaft'))['structuredContent']
        self.assertEqual(inspection['source']['model_sha256'], parent['model_sha256'])
        original = pathlib.Path(parent['directory'], 'model.json').read_bytes()
        expected = request['model']['family']['features'][0]
        replacement = json.loads(json.dumps(expected))
        replacement['operation']['cylinder']['radius']['literal']['value'] = 6
        arguments = dict(build_id=parent['build_id'], expected_model_sha256=parent['model_sha256'],
            changes=[dict(action='replace_feature', family='shaft', expected=expected, feature=replacement)],
            outputs=request['outputs'], revision=dict(id='edit-1', author='test', recorded_at='test-time', message='increase radius'), step=False, stl=False, preview=False)
        tools = self.client.call('tools/list')['result']['tools']
        schema = next(t['inputSchema'] for t in tools if t['name']=='occt_edit_build')
        jsonschema.validate(arguments, schema)
        edited = self.client.tool('occt_edit_build', arguments)
        self.assertFalse(edited['isError'])
        child = edited['structuredContent']
        self.assertNotEqual(child['build_id'], parent['build_id'])
        self.assertNotEqual(child['model_sha256'], parent['model_sha256'])
        self.assertAlmostEqual(child['report']['outputs'][0]['volume_mm3'], 2160 * 3.141592653589793)
        self.assertEqual(child['report']['source']['build_id'], parent['build_id'])
        self.assertEqual(child['report']['revision']['id'], 'edit-1')
        revised = json.loads(self.client.call('resources/read', dict(uri=child['resources']['model.json']))['result']['contents'][0]['text'])
        self.assertEqual(revised['family']['version'], 2)
        self.assertEqual(revised['family']['requirements'], request['model']['family']['requirements'])
        change_record = json.loads(self.client.call('resources/read', dict(uri=child['resources']['changes.json']))['result']['contents'][0]['text'])
        self.assertEqual(change_record, revised['revisions'][0])
        self.assertEqual(pathlib.Path(parent['directory'], 'model.json').read_bytes(), original)
        # A different expected fingerprint stops before creating worker/artifact files.
        arguments['expected_model_sha256'] = '0' * 64
        conflict = self.client.tool('occt_edit_build', arguments)
        self.assertTrue(conflict['isError'])
        self.assertEqual(conflict['structuredContent']['stage'], 'conflict')
        self.assertEqual(len(list((self.root / 'builds').iterdir())), 2)
        # A feature guard remains necessary even with the correct file fingerprint.
        arguments.update(build_id=child['build_id'], expected_model_sha256=child['model_sha256'])
        arguments['revision']['id'] = 'edit-2'
        stale = self.client.tool('occt_edit_build', arguments)
        self.assertTrue(stale['isError'])
        self.assertEqual(stale['structuredContent']['stage'], 'conflict')
        self.assertEqual(len(list((self.root / 'builds').iterdir())), 2)
        # Invalid geometry still rejects an edit, without waiving intent.
        arguments['changes'][0].update(expected=replacement)
        arguments['changes'][0]['feature'] = json.loads(json.dumps(replacement))
        arguments['changes'][0]['feature']['operation']['cylinder']['radius']['literal']['value'] = -1
        rejected = self.client.tool('occt_edit_build', arguments)
        self.assertTrue(rejected['isError'])
        self.assertEqual(len(list((self.root / 'builds').iterdir())), 2)
        arguments['changes'] = [dict(action='add_requirement', family='shaft', requirement=dict(
            id='volume.limit', version=1, kind='validation', priority='required', statement='volume below 5000 mm3',
            rule={'volume_range': dict(output='body', minimum=dict(value=0,unit='millimeter'), maximum=dict(value=5000,unit='millimeter'))},
            provenance='test', traces=[dict(feature='body')]))]
        failed = self.client.tool('occt_edit_build', arguments)
        self.assertTrue(failed['isError'])
        self.assertEqual(failed['structuredContent']['stage'], 'regeneration')
        self.assertIn('volume.limit', failed['structuredContent']['message'])
        self.assertEqual(len(list((self.root / 'builds').iterdir())), 2)

    def test_annotated_viewer_sketch_conflicts_and_diagnostic_build_separation(self):
        request = self.client.tool('occt_get_example',dict(name='sketch-block'))['structuredContent']
        schema = self.client.tool('occt_get_schema',dict(name='view'))['structuredContent']
        jsonschema.validate(request,schema)
        result = self.client.tool('occt_visualize_model',request)
        self.assertFalse(result['isError'])
        view = result['structuredContent']
        self.assertIn('visualization_id',view)
        self.assertNotIn('build_id',view)
        self.assertEqual(view['report']['status'],'visualized')
        html = self.client.call('resources/read',dict(uri=view['resources']['viewer.html']))['result']['contents'][0]
        self.assertEqual(html['mimeType'],'text/html')
        self.assertIn('Dimensions &amp;',html['text'].replace('& constraints','&amp; constraints'))
        data = json.loads(self.client.call('resources/read',dict(uri=view['resources']['view.json']))['result']['contents'][0]['text'])
        self.assertEqual(len(data['scenes']),2)
        self.assertTrue(data['scenes'][1]['solver']['solved'])
        self.assertEqual(data['scenes'][1]['annotations'][4]['parameters'],['width'])
        snapshot = self.client.call('resources/read',dict(uri=view['resources']['view-0002.svg']))['result']['contents'][0]['text']
        self.assertIn('data-annotation="constraint-4"',snapshot)
        forbidden = self.client.call('tools/call',dict(name='occt_inspect_build',arguments=dict(build_id=view['visualization_id'])))
        self.assertEqual(forbidden['error']['code'],-32602)
        conflict = self.client.tool('occt_get_example',dict(name='sketch-conflict'))['structuredContent']
        failed = self.client.tool('occt_visualize_model',conflict)['structuredContent']
        data = json.loads(self.client.call('resources/read',dict(uri=failed['resources']['view.json']))['result']['contents'][0]['text'])
        self.assertFalse(data['scenes'][1]['solver']['solved'])
        self.assertTrue(any(a['status']=='failed' for a in data['scenes'][1]['annotations']))
        curved = self.client.tool('occt_get_example',dict(name='curved-extrusions'))['structuredContent']
        jsonschema.validate(curved,schema)
        result = self.client.tool('occt_visualize_model',curved)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        rounded = next(s for s in data['scenes'] if s['kind']=='solid' and s['feature']=='body')
        extent = next(a for a in rounded['annotations'] if a['id']=='driving-extrusion')
        self.assertAlmostEqual(extent['detail']['value_mm'],20.0)
        self.assertEqual(extent['detail']['measurement'],'profile_centroid_ray')
        pointed = self.client.tool('occt_get_example',dict(name='drill-point'))['structuredContent']
        jsonschema.validate(pointed,schema)
        result = self.client.tool('occt_visualize_model',pointed)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        tip = next(a for a in data['scenes'][0]['annotations'] if a['id']=='measured-drill-tip')
        self.assertAlmostEqual(tip['detail']['value_mm'],3.0**0.5)
        self.assertEqual(tip['parameters'],['diameter','point_angle'])
        holes = self.client.tool('occt_get_example',dict(name='hole-limits'))['structuredContent']
        jsonschema.validate(holes,schema)
        result = self.client.tool('occt_visualize_model',holes)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        limit = next(a for a in data['scenes'][0]['annotations'] if a['id']=='measured-hole-limit')
        self.assertAlmostEqual(limit['detail']['value_mm'],20.0)
        self.assertIn('depth',limit['parameters'])
        loft = self.client.tool('occt_get_example',dict(name='profile-loft'))['structuredContent']
        jsonschema.validate(loft,schema)
        result = self.client.tool('occt_visualize_model',loft)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        spacing = next(a for a in data['scenes'][0]['annotations'] if a['id']=='loft-spacing-0')
        self.assertAlmostEqual(spacing['detail']['value_mm'],20.0)
        self.assertIn('height',spacing['parameters'])
        pipe = self.client.tool('occt_get_example',dict(name='curved-pipe'))['structuredContent']
        jsonschema.validate(pipe,schema)
        result = self.client.tool('occt_visualize_model',pipe)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        route = next(a for a in data['scenes'][0]['annotations'] if a['id']=='sweep-route-length')
        self.assertAlmostEqual(route['detail']['value_mm'],10+5*math.pi)
        self.assertEqual(route['parameters'],['bend_radius','run'])
        self.assertEqual(len(route['detail']['dimension_paths']),2)
        ring = self.client.tool('occt_get_example',dict(name='revolved-ring'))['structuredContent']
        jsonschema.validate(ring,schema)
        result = self.client.tool('occt_visualize_model',ring)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        angle = next(a for a in data['scenes'][0]['annotations'] if a['id']=='driving-revolve-angle')
        self.assertAlmostEqual(angle['detail']['value_radians'],math.tau)
        self.assertEqual(angle['parameters'],['angle'])
        self.assertEqual(len(angle['detail']['angular_arc']),65)
        symmetric = self.client.tool('occt_get_example',dict(name='symmetric-revolve'))['structuredContent']
        jsonschema.validate(symmetric,schema)
        result = self.client.tool('occt_visualize_model',symmetric)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        dimension = next(a for a in data['scenes'][0]['annotations'] if a['id']=='driving-revolve-angle')
        self.assertEqual(dimension['detail']['extent'],'symmetric')
        self.assertAlmostEqual(dimension['detail']['start_angle_radians'],-math.pi/4)
        self.assertAlmostEqual(dimension['detail']['end_angle_radians'],math.pi/4)
        hollow = self.client.tool('occt_get_example',dict(name='hollow-profile'))['structuredContent']
        jsonschema.validate(hollow,schema)
        result = self.client.tool('occt_visualize_model',hollow)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        self.assertTrue(data['scenes'][0]['valid'])
        self.assertIn('inner_radius',data['scenes'][0]['parameters'])
        self.assertEqual(len(data['scenes']),3)
        hollow_sweep = self.client.tool('occt_get_example',dict(name='hollow-sweep'))['structuredContent']
        jsonschema.validate(hollow_sweep,schema)
        result = self.client.tool('occt_visualize_model',hollow_sweep)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        self.assertTrue(data['scenes'][0]['valid'])
        self.assertIn('inner_radius',data['scenes'][0]['parameters'])
        self.assertEqual(len(data['scenes']),4)
        hollow_loft = self.client.tool('occt_get_example',dict(name='hollow-loft'))['structuredContent']
        jsonschema.validate(hollow_loft,schema)
        result = self.client.tool('occt_visualize_model',hollow_loft)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        self.assertTrue(data['scenes'][0]['valid'])
        self.assertIn('lower_bore_radius',data['scenes'][0]['parameters'])
        self.assertIn('upper_bore_radius',data['scenes'][0]['parameters'])
        self.assertEqual(len(data['scenes']),5)
        mirrored = self.client.tool('occt_get_example',dict(name='mirrored-part'))['structuredContent']
        jsonschema.validate(mirrored,schema)
        result = self.client.tool('occt_visualize_model',mirrored)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        plane = next(a for a in data['scenes'][0]['annotations'] if a['id']=='driving-mirror-plane')
        self.assertEqual(plane['parameters'],['plane_tilt','plane_x'])
        self.assertEqual(plane['detail']['plane_normal'],[1.0,0.0,0.0])
        self.assertAlmostEqual(data['scenes'][0]['bounds'][0][0],-25.0)
        scaled = self.client.tool('occt_get_example',dict(name='scaled-part'))['structuredContent']
        jsonschema.validate(scaled,schema)
        result = self.client.tool('occt_visualize_model',scaled)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        factor = next(a for a in data['scenes'][0]['annotations'] if a['id']=='driving-scale-factor')
        self.assertEqual(factor['detail']['factor'],1.2)
        self.assertEqual(factor['parameters'],['scale_center_x','scale_factor'])
        self.assertAlmostEqual(data['scenes'][0]['bounds'][1][0],30.0)
        spring = self.client.tool('occt_get_example',dict(name='spring'))['structuredContent']
        jsonschema.validate(spring,schema)
        result = self.client.tool('occt_visualize_model',spring)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        self.assertTrue(data['scenes'][0]['valid'])
        annotations = {a['id']:a for a in data['scenes'][0]['annotations']}
        self.assertEqual(annotations['helix-rise']['detail']['value'],20)
        self.assertEqual(annotations['helix-radius']['parameters'],['coil_radius'])
        self.assertEqual(len(annotations['sweep-route-length']['detail']['dimension_paths'][0]),161)
        offset = self.client.tool('occt_get_example',dict(name='offset-part'))['structuredContent']
        jsonschema.validate(offset,schema)
        result = self.client.tool('occt_visualize_model',offset)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        self.assertTrue(data['scenes'][0]['valid'])
        annotation = next(a for a in data['scenes'][0]['annotations'] if a['id']=='driving-skin-offset')
        self.assertEqual(annotation['detail']['value_mm'],1)
        self.assertEqual(annotation['parameters'],['allowance','offset_tolerance'])
        self.assertAlmostEqual(data['scenes'][0]['bounds'][1][0],11)
        plate = self.client.tool('occt_get_example',dict(name='multi-hole-plate'))['structuredContent']
        jsonschema.validate(plate,schema)
        plate['outputs'].append(dict(instance='plate',output='tools'))
        result = self.client.tool('occt_visualize_model',plate)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        self.assertTrue(data['scenes'][0]['valid'])
        group = next(a for a in data['scenes'][1]['annotations'] if a['id']=='compound-inputs')
        self.assertEqual(group['detail']['input_count'],9)
        self.assertEqual(len(group['detail']['inputs']),9)
        plate = self.client.tool('occt_get_example',dict(name='patterned-plate'))['structuredContent']
        jsonschema.validate(plate,schema)
        plate['outputs'].append(dict(instance='plate',output='column-tools'))
        result = self.client.tool('occt_visualize_model',plate)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        self.assertTrue(data['scenes'][0]['valid'])
        count = next(a for a in data['scenes'][1]['annotations'] if a['id']=='linear-pattern-count')
        self.assertEqual(count['detail']['count'],3)
        self.assertEqual(count['parameters'],['columns'])
        circle = self.client.tool('occt_get_example',dict(name='bolt-circle'))['structuredContent']
        jsonschema.validate(circle,schema)
        circle['outputs'].append(dict(instance='plate',output='tools'))
        result = self.client.tool('occt_visualize_model',circle)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        self.assertTrue(data['scenes'][0]['valid'])
        angle = next(a for a in data['scenes'][1]['annotations'] if a['id']=='circular-pattern-angle')
        self.assertEqual(angle['parameters'],['count','sweep_angle'])
        self.assertEqual(len(angle['detail']['angular_arc']),33)
        threaded = self.client.tool('occt_get_example',dict(name='threaded-rod'))['structuredContent']
        jsonschema.validate(threaded,schema)
        result = self.client.tool('occt_visualize_model',threaded)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        self.assertTrue(data['scenes'][0]['valid'])
        annotations={a['id']:a for a in data['scenes'][0]['annotations']}
        self.assertEqual(annotations['thread-major-diameter']['detail']['value_mm'],10)
        self.assertEqual(annotations['thread-pitch']['parameters'],['pitch'])
        self.assertAlmostEqual(annotations['thread-turns']['detail']['turns'],8/1.5)
        treatments = self.client.tool('occt_get_example',dict(name='edge-treatments'))['structuredContent']
        jsonschema.validate(treatments,schema)
        result = self.client.tool('occt_visualize_model',treatments)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        self.assertEqual(len(data['scenes']),2)
        for scene,kind in zip(data['scenes'],['fillet','chamfer']):
            self.assertTrue(scene['valid'])
            annotation=next(a for a in scene['annotations'] if a['id']==f'driving-{kind}')
            self.assertEqual(annotation['detail']['selected_edge_count'],4)
            self.assertEqual(len(annotation['detail']['dimension_paths']),4)
            self.assertTrue(annotation['detail']['source_reference'])
        variable = self.client.tool('occt_get_example',dict(name='variable-fillet'))['structuredContent']
        jsonschema.validate(variable,schema)
        result = self.client.tool('occt_visualize_model',variable)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        self.assertTrue(data['scenes'][0]['valid'])
        law = next(a for a in data['scenes'][0]['annotations'] if a['id']=='driving-variable-fillet')
        self.assertEqual(law['detail']['radius_law'],[dict(position=0,radius_mm=1),dict(position=.25,radius_mm=2.5),dict(position=1,radius_mm=2)])
        self.assertFalse(law['detail']['spatial_stations'])
        self.assertIn('station_position',law['parameters'])
        plate = self.client.tool('occt_get_example',dict(name='equal-radius-plate'))['structuredContent']
        jsonschema.validate(plate,schema)
        result = self.client.tool('occt_visualize_model',plate)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        self.assertEqual(len(data['scenes']),4)
        self.assertTrue(data['scenes'][0]['valid'])
        matched = [a for scene in data['scenes'] for a in scene['annotations'] if a['label']=='=R']
        self.assertEqual(len(matched),2)
        self.assertTrue(all(a['status']=='passed' and 'hole_radius' in a['parameters'] for a in matched))
        pocket = self.client.tool('occt_get_example',dict(name='face-pocket'))['structuredContent']
        jsonschema.validate(pocket,schema)
        result = self.client.tool('occt_visualize_model',pocket)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        self.assertEqual(len(data['scenes']),3)
        source = next(s for s in data['scenes'] if s['kind']=='sketch')
        self.assertEqual(source['face_support']['status'],'resolved')
        self.assertEqual(source['face_support']['origin_mm'],[30,20,20])
        self.assertEqual(source['face_support']['normal'],[0,0,1])
        self.assertIsNone(source['profile_error'])
        projected = self.client.tool('occt_get_example',dict(name='projected-pocket'))['structuredContent']
        jsonschema.validate(projected,schema)
        result = self.client.tool('occt_visualize_model',projected)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        source = next(s for s in data['scenes'] if s['kind']=='sketch')
        self.assertTrue(source['solver']['solved'])
        self.assertTrue(next(e for e in source['entities'] if e['id']=='front-edge')['external'])
        self.assertAlmostEqual(source['points']['guide'][1],20)
        self.assertIn('depth',next(a for a in source['annotations'] if a['id']=='projection-front-edge')['parameters'])
        midpoint = next(a for a in source['annotations'] if 'midpoint' in a['detail'].get('constraint', {}))
        self.assertEqual(midpoint['status'], 'passed')
        self.assertEqual(midpoint['detail']['residual_unit'], 'mm')
        distance = next(a for a in source['annotations'] if 'point_line_distance' in a['detail'].get('constraint', {}))
        self.assertEqual(distance['kind'], 'dimension')
        self.assertEqual(distance['status'], 'passed')
        self.assertIn('margin', distance['parameters'])
        self.assertAlmostEqual(distance['anchors'][0][1], 20)
        self.assertAlmostEqual(distance['anchors'][1][1], 16)
        bushing = self.client.tool('occt_get_example', dict(name='concentric-bushing'))['structuredContent']
        jsonschema.validate(bushing, schema)
        result = self.client.tool('occt_visualize_model', bushing)
        self.assertFalse(result['isError'])
        data = json.loads(self.client.call('resources/read', dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        for scene in [s for s in data['scenes'] if s['kind']=='sketch']:
            self.assertTrue(scene['solver']['solved'])
            relation = next(a for a in scene['annotations'] if 'concentric' in a['detail'].get('constraint', {}))
            self.assertEqual(relation['status'], 'passed')
            self.assertLess(relation['detail']['max_residual'], 1e-7)
            self.assertIn('center_x', relation['parameters'])
        # Ordinary accepted builds also publish the annotated viewer when preview is on.
        build_request = dict(schema='occb-model-request-v1',model=request['model'],outputs=request['outputs'],preview=True,step=False,stl=False)
        built = self.client.tool('occt_build',build_request)['structuredContent']
        self.assertEqual(built['report']['status'],'built')
        self.assertIn('viewer.html',built['resources'])

    def test_protocol_errors_and_no_native_stdout_contamination(self):
        self.assertEqual(self.client.call('unknown')['error']['code'], -32601)
        self.assertEqual(self.client.call('tools/call', dict(name='unknown'))['error']['code'], -32602)
        self.assertEqual(self.client.call('tools/call', dict(name='occt_get_schema', arguments=dict(name='request', path='/tmp')))['error']['code'], -32602)
        self.client.process.stdin.write(b'not-json\n')
        self.client.process.stdin.flush()
        self.assertEqual(self.client.receive()['error']['code'], -32700)
        self.client.send(['batch-not-supported'])
        self.assertEqual(self.client.receive()['error']['code'], -32600)
        result = self.client.tool('occt_build', dict(schema='wrong', model={}))
        self.assertTrue(result['isError'])
        self.assertEqual(self.client.call('ping')['result'], {})

    def slow_worker(self):
        worker = self.root / 'slow-worker'
        worker.write_text('#!' + sys.executable + '\n' +
                          'import os,sys,pathlib,time\n' +
                          'if sys.argv[1] == "--schema": os.execv(' + repr(str(BINARY)) + ', [' + repr(str(BINARY)) + '] + sys.argv[1:])\n' +
                          'pathlib.Path(sys.argv[1]).with_name("started").touch()\n' +
                          'print("native noise",flush=True)\n' +
                          'time.sleep(60)\n')
        worker.chmod(0o700)
        return worker

    def test_timeout_and_cancellation_stop_worker_and_cleanup(self):
        worker = self.slow_worker()
        self.client.close()
        self.client = Client(self.root / 'timeout', worker, 0.1)
        request = self.client.tool('occt_get_example', dict(name='shaft'))['structuredContent']
        result = self.client.tool('occt_build', request)
        self.assertTrue(result['isError'])
        self.assertEqual(result['structuredContent']['stage'], 'timeout')
        self.assertEqual(list((self.root / 'timeout').iterdir()), [])
        self.client.close()
        self.client = Client(self.root / 'cancelled', worker)
        self.client.send(dict(jsonrpc='2.0', id=100, method='tools/call', params=dict(name='occt_build', arguments=request)))
        deadline = time.monotonic() + 10
        while not list((self.root / 'cancelled').glob('*/started')) and time.monotonic() < deadline:
            time.sleep(0.01)
        self.assertTrue(list((self.root / 'cancelled').glob('*/started')))
        self.client.send(dict(jsonrpc='2.0', method='notifications/cancelled', params=dict(requestId=100, reason='test')))
        self.client.send(dict(jsonrpc='2.0', method='notifications/cancelled', params=dict(requestId=100)))
        self.assertEqual(self.client.call('ping')['result'], {})
        deadline = time.monotonic() + 10
        while list((self.root / 'cancelled').iterdir()) and time.monotonic() < deadline:
            time.sleep(0.01)
        self.assertEqual(list((self.root / 'cancelled').iterdir()), [])
        self.client.close()
        self.client = Client(self.root / 'closed', worker)
        self.client.send(dict(jsonrpc='2.0', id=200, method='tools/call', params=dict(name='occt_build', arguments=request)))
        deadline = time.monotonic() + 10
        while not list((self.root / 'closed').glob('*/started')) and time.monotonic() < deadline:
            time.sleep(0.01)
        self.assertTrue(list((self.root / 'closed').glob('*/started')))
        self.client.close()
        self.assertEqual(self.client.process.returncode, 0)
        self.assertEqual(list((self.root / 'closed').iterdir()), [])

    def test_pattern_source_history_inspection_resolves_all_bore_faces(self):
        selector = {'history':dict(source_feature='cutter',source={'largest_area':dict(planar_only=False,allow_ties=False,relative_tolerance={'literal':dict(value=1e-9,dimension='scalar',unit=None)})},relation='modified')}
        for name,count in [('patterned-plate',9),('bolt-circle',6)]:
            example=self.client.tool('occt_get_example',dict(name=name))['structuredContent']
            request=dict(schema='occb-model-request-v1',model=example['model'],outputs=example['outputs'],step=False,stl=False,preview=False)
            built=self.client.tool('occt_build',request)
            self.assertFalse(built['isError'])
            inspected=self.client.tool('occt_inspect_build',dict(build_id=built['structuredContent']['build_id'],instance='plate',output='body',face_selector=selector,limit=100))
            self.assertFalse(inspected['isError'])
            self.assertEqual(inspected['structuredContent']['geometry']['faces']['total'],count)

if __name__ == '__main__':
    unittest.main()
