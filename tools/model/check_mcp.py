#!/usr/bin/env python3
"""Release-scale checks for MCP discovery and a 1,000-part build/report roundtrip."""
import math
import json
import os
import pathlib
import sys
import tempfile
import time

binary = pathlib.Path(sys.argv[1]).resolve()
os.environ['OCCT_MODEL_BINARY'] = str(binary)
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent / 'tests'))
from client import Client

with tempfile.TemporaryDirectory(prefix='occb-mcp-scale-') as root:
    client = Client(root)
    try:
        started = time.monotonic()
        for i in range(1000):
            result = client.tool('occt_get_schema', dict(name='parameter'))
            assert not result['isError'] and 'properties' in result['structuredContent']
        elapsed = time.monotonic() - started
        assert elapsed < 10
        print(f'PASS MCP 1000 generated schema calls: {elapsed:.3f}s / 10s')
        request = client.tool('occt_get_example', dict(name='shaft'))['structuredContent']
        request.update(step=False, stl=False, preview=False)
        for i in range(1, 1000):
            request['model']['instances'].append({'clone': dict(id=f'shaft-{i}', source='shaft', overrides={}, provenance='scale')})
            request['outputs'].append(dict(instance=f'shaft-{i}', output='body'))
        started = time.monotonic()
        result = client.tool('occt_build', request)
        assert not result['isError']
        built = result['structuredContent']
        report = built['report']
        assert len(report['outputs']) == 1000 and len(report['verification']) == 1000
        assert report['generated_variants'] == 1
        assert all(r['status'] == 'passed' for r in report['verification'])
        assert all(o['valid'] and abs(o['volume_mm3'] - 1500 * 3.141592653589793) < 1e-7 for o in report['outputs'])
        resource = client.call('resources/read', dict(uri=built['resources']['model.json']))['result']['contents'][0]
        saved = json.loads(resource['text'])
        assert len(saved['instances']) == 1000
        elapsed = time.monotonic() - started
        assert elapsed < 10
        print(f'PASS MCP 1000-part build, evidence/report, accepted model resource: {elapsed:.3f}s / 10s')
        started = time.monotonic()
        inventory_model = request['model']
        for i in range(1000, 10000):
            inventory_model['instances'].append({'clone': dict(id=f'shaft-{i}', source='shaft', overrides={}, provenance='scale')})
        inventory = client.tool('occt_inspect_model', dict(schema='occb-model-inspection-v1', model=inventory_model, limit=25))
        assert not inventory['isError']
        inspection = inventory['structuredContent']
        assert not inspection['geometry_generated'] and inspection['instances']['total'] == 10000
        assert len(inspection['instances']['items']) == 25 and inspection['instances']['next_offset'] == 25
        elapsed = time.monotonic() - started
        assert elapsed < 10
        print(f'PASS MCP 10000-instance read-only paged inventory: {elapsed:.3f}s / 10s')
        model = client.tool('occt_get_example', dict(name='shaft'))['structuredContent']['model']
        def point(x, y, z):
            return {'literal': {key: dict(value=value, dimension='length', unit='millimeter') for key,value in zip('xyz', (x,y,z))}}
        model['family']['features'] = [dict(id='body' if i==999 else f'box-{i}', operation={'box': dict(origin=point(i*20,0,0),size=point(10,10,10))}) for i in range(1000)]
        started = time.monotonic()
        inspection = client.tool('occt_inspect_model', dict(schema='occb-model-inspection-v1', model=model, instance='shaft', output='body', limit=2))
        assert not inspection['isError']
        report = inspection['structuredContent']
        assert report['instance']['features']['total'] == 1000 and len(report['instance']['features']['items']) == 2
        assert report['geometry']['faces']['total'] == 6 and len(report['geometry']['faces']['items']) == 2
        assert report['geometry']['edges']['total'] == 12 and len(report['geometry']['edges']['items']) == 2
        assert abs(report['geometry']['volume_mm3']-1000) < 1e-7
        assert report['authoring_regenerations'] == 1
        elapsed = time.monotonic() - started
        assert elapsed < 10
        print(f'PASS MCP 1000-feature regeneration and bounded face/edge inspection: {elapsed:.3f}s / 10s')
        # Guarded editing over 10,000 feature identities, including semantic
        # revision serialization and preservation of the source snapshot.
        model['family']['features'] = [dict(id='body' if i==9999 else f'box-{i}', operation={'box': dict(origin=point(i*20,0,0),size=point(10,10,10))}) for i in range(10000)]
        parent = client.tool('occt_build', dict(schema='occb-model-request-v1', model=model, outputs=[dict(instance='shaft',output='body')], step=False,stl=False,preview=False))['structuredContent']
        text = client.call('resources/read',dict(uri=parent['resources']['model.json']))['result']['contents'][0]['text']
        baseline = json.loads(text)
        changes = []
        for feature in baseline['family']['features']:
            replacement = json.loads(json.dumps(feature))
            replacement['operation']['box']['size']['literal']['x']['value'] = 12
            changes.append(dict(action='replace_feature',family='shaft',expected=feature,feature=replacement))
        started = time.monotonic()
        edited = client.tool('occt_edit_build', dict(build_id=parent['build_id'],expected_model_sha256=parent['model_sha256'],changes=changes,
            outputs=[dict(instance='shaft',output='body')],revision=dict(id='scale-edit-1',author='scale',recorded_at='scale-time',message='increase all box widths'),step=False,stl=False,preview=False))
        assert not edited['isError'], edited
        child = edited['structuredContent']
        assert abs(child['report']['outputs'][0]['volume_mm3']-1200) < 1e-7
        saved = json.loads(client.call('resources/read',dict(uri=child['resources']['model.json']))['result']['contents'][0]['text'])
        assert len(saved['family']['features']) == 10000 and saved['family']['version'] == 2
        assert all(f['operation']['box']['size']['literal']['x']['value']==12 for f in saved['family']['features'])
        assert saved['family']['requirements'] == baseline['family']['requirements']
        assert len(saved['revisions']) == 1 and len(saved['revisions'][0]['changes']) == 10001
        assert client.call('resources/read',dict(uri=parent['resources']['model.json']))['result']['contents'][0]['text'] == text
        elapsed = time.monotonic() - started
        assert elapsed < 30
        print(f'PASS MCP 10000 guarded feature edits, verified build, revision ledger, source preservation: {elapsed:.3f}s / 30s')
        # Native constraint diagnostics and annotated sketch output at scale.
        model = client.tool('occt_get_example',dict(name='sketch-block'))['structuredContent']['model']
        sketch = model['family']['features'][0]['operation']['sketch_face']['sketch']
        def literal(v):return {'literal':dict(value=v,dimension='length',unit='millimeter')}
        sketch['points'] = [dict(id=f'{end}{i}',x=literal(i*2+(end=='B')),y=literal(0),fixed=True)for i in range(1000)for end in ('A','B')]
        sketch['lines'] = [dict(id=f'L{i}',start=f'A{i}',end=f'B{i}')for i in range(1000)]
        sketch['constraints'] = [{'horizontal':dict(line=f'L{i}')}for i in range(1000)]
        sketch['profile'] = ['L0']
        model['family']['features'] = [dict(id='profile',operation={'sketch_open_wire':dict(sketch=sketch)})]
        model['family']['requirements'] = []
        started = time.monotonic()
        result = client.tool('occt_visualize_model',dict(schema='occb-model-view-v1',model=model,outputs=[]))
        assert not result['isError'],result
        viewed = result['structuredContent']
        data = json.loads(client.call('resources/read',dict(uri=viewed['resources']['view.json']))['result']['contents'][0]['text'])
        assert len(data['scenes'])==1 and data['scenes'][0]['solver']['solved']
        assert len([a for a in data['scenes'][0]['annotations'] if a['id'].startswith('constraint-')])==1000
        assert all(a['status']=='passed' for a in data['scenes'][0]['annotations'] if a['id'].startswith('constraint-'))
        elapsed = time.monotonic()-started
        assert elapsed<10
        print(f'PASS MCP 1000 sketch constraints, native curves, annotations and snapshot artifacts: {elapsed:.3f}s / 10s')
        # All declared scene/mesh/annotation budgets remain global across parts.
        model = client.tool('occt_get_example',dict(name='shaft'))['structuredContent']['model']
        model['family']['features']=[dict(id='body',operation={'box':dict(origin=point(0,0,0),size=point(10,10,10))})]
        for i in range(1,1000):model['instances'].append({'clone':dict(id=f'shaft-{i}',source='shaft',overrides={},provenance='scale')})
        outputs=[dict(instance='shaft' if i==0 else f'shaft-{i}',output='body')for i in range(1000)]
        started=time.monotonic()
        result=client.tool('occt_visualize_model',dict(schema='occb-model-view-v1',model=model,outputs=outputs,sketches=False))
        assert not result['isError'],result
        viewed=result['structuredContent'];assert viewed['report']['scenes']==1000
        data=json.loads(client.call('resources/read',dict(uri=viewed['resources']['view.json']))['result']['contents'][0]['text'])
        assert len(data['scenes'])==1000 and all(len(s['mesh'])==12 for s in data['scenes'])
        assert all(any(a['detail'].get('measured_mm')==10 for a in s['annotations']) for s in data['scenes'])
        elapsed=time.monotonic()-started
        assert elapsed<30
        print(f'PASS MCP 1000 annotated solid scenes, global budgets, dimension data and artifacts: {elapsed:.3f}s / 30s')
        # Native sweep-route measurements and bounded sampled curve overlays.
        pipe=client.tool('occt_get_example',dict(name='curved-pipe'))['structuredContent']
        pipe['sketches']=False
        pipe['options']=dict(maximum_triangles=120000,maximum_vertices=400000)
        for i in range(1,100):pipe['model']['instances'].append({'clone':dict(id=f'pipe-{i}',source='pipe',overrides={},provenance='scale')})
        pipe['outputs']=[dict(instance='pipe' if i==0 else f'pipe-{i}',output='body')for i in range(100)]
        started=time.monotonic()
        result=client.tool('occt_visualize_model',pipe)
        assert not result['isError'],result
        data=json.loads(client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        assert len(data['scenes'])==100
        for scene in data['scenes']:
            route=next(a for a in scene['annotations'] if a['id']=='sweep-route-length')
            assert abs(route['detail']['value_mm']-(10+5*math.pi))<1e-7
            assert route['parameters']==['bend_radius','run']
            assert len(route['detail']['dimension_paths'])==2
            assert sum(len(p) for p in route['detail']['dimension_paths'])==64
        elapsed=time.monotonic()-started
        assert elapsed<10
        print(f'PASS MCP 100 curved sweep scenes, native route lengths and bounded overlays: {elapsed:.3f}s / 10s')
        ring=client.tool('occt_get_example',dict(name='revolved-ring'))['structuredContent']
        ring['sketches']=False
        for i in range(1,100):ring['model']['instances'].append({'clone':dict(id=f'ring-{i}',source='ring',overrides={},provenance='scale')})
        ring['outputs']=[dict(instance='ring' if i==0 else f'ring-{i}',output='body')for i in range(100)]
        started=time.monotonic()
        result=client.tool('occt_visualize_model',ring)
        assert not result['isError'],result
        data=json.loads(client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        assert len(data['scenes'])==100
        for scene in data['scenes']:
            angle=next(a for a in scene['annotations'] if a['id']=='driving-revolve-angle')
            assert abs(angle['detail']['value_radians']-math.tau)<1e-12
            assert abs(angle['detail']['arc_radius_mm']-7)<1e-7
            assert angle['parameters']==['angle']
            assert len(angle['detail']['angular_arc'])==65
        elapsed=time.monotonic()-started
        assert elapsed<10
        print(f'PASS MCP 100 revolved scenes, signed angle arcs and linked controls: {elapsed:.3f}s / 10s')
        spring=client.tool('occt_get_example',dict(name='spring'))['structuredContent']
        spring['sketches']=False
        spring['options']=dict(maximum_triangles=1000000,maximum_vertices=1000000)
        for i in range(1,10):spring['model']['instances'].append({'clone':dict(id=f'spring-{i}',source='spring',overrides={},provenance='scale')})
        spring['outputs']=[dict(instance='spring' if i==0 else f'spring-{i}',output='body')for i in range(10)]
        started=time.monotonic()
        result=client.tool('occt_visualize_model',spring)
        assert not result['isError'],result
        resource=client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))
        assert resource['error']['code']==-32002 and '32 MiB' in resource['error']['message']
        # Dense spring meshes exceed the bounded MCP read size; use the returned local artifact.
        data=json.loads((pathlib.Path(result['structuredContent']['directory'])/'view.json').read_text())
        assert len(data['scenes'])==10
        for scene in data['scenes']:
            assert scene['valid']
            annotations={a['id']:a for a in scene['annotations']}
            assert annotations['helix-rise']['detail']['value']==20
            assert annotations['helix-radius']['parameters']==['coil_radius']
            assert annotations['helix-pitch']['parameters']==['pitch']
            route=annotations['sweep-route-length']
            assert abs(route['detail']['value_mm']-5*math.hypot(20*math.pi,4))<1e-4
            assert len(route['detail']['dimension_paths'][0])==161
        elapsed=time.monotonic()-started
        assert elapsed<30
        print(f'PASS MCP 10 spring scenes, helix dimensions and samples per turn: {elapsed:.3f}s / 30s')
        helix=client.tool('occt_get_example',dict(name='spring'))['structuredContent']
        helix['sketches']=False
        helix['model']['family']['features']=helix['model']['family']['features'][:1]
        helix['model']['family']['requirements']=[]
        for i in range(1,100):helix['model']['instances'].append({'clone':dict(id=f'coil-{i}',source='spring',overrides={},provenance='scale')})
        helix['outputs']=[dict(instance='spring' if i==0 else f'coil-{i}',output='coil')for i in range(100)]
        started=time.monotonic()
        result=client.tool('occt_visualize_model',helix)
        assert not result['isError'],result
        data=json.loads(client.call('resources/read',dict(uri=result['structuredContent']['resources']['view.json']))['result']['contents'][0]['text'])
        assert len(data['scenes'])==100
        for scene in data['scenes']:
            assert len(scene['lines'][0])==161
            assert next(a for a in scene['annotations'] if a['id']=='helix-rise')['detail']['value']==20
        elapsed=time.monotonic()-started
        assert elapsed<10
        print(f'PASS MCP 100 helix wire scenes, axial dimensions and bounded route samples: {elapsed:.3f}s / 10s')
    finally:
        client.close()
