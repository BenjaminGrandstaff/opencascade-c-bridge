#!/usr/bin/env python3
"""Exercise published AI examples and enforce command-scale time budgets."""
import json
import pathlib
import subprocess
import sys
import tempfile
import time

root = pathlib.Path(__file__).resolve().parents[2]
binary = pathlib.Path(sys.argv[1]).resolve()

def build(request, directory):
    source = directory.with_suffix('.request.json')
    source.write_text(json.dumps(request))
    started = time.monotonic()
    process = subprocess.run([str(binary), str(source), str(directory)], capture_output=True, text=True, timeout=30)
    elapsed = time.monotonic() - started
    if process.returncode:
        raise RuntimeError(process.stderr)
    status = json.loads(process.stderr.splitlines()[-1])
    persisted = json.loads((directory / 'report.json').read_text())
    assert status == persisted and status['status'] == 'built'
    assert all(o['valid'] for o in status['outputs'])
    assert all(r['status'] == 'passed' for r in status['verification'])
    return status, elapsed

with tempfile.TemporaryDirectory(prefix='occb-model-check-') as temporary:
    folder = pathlib.Path(temporary)
    for name in ['bracket', 'enclosure', 'shaft', 'mating-parts']:
        request = json.loads((root / 'tools/model' / (name + '.request.json')).read_text())
        report, elapsed = build(request, folder / name)
        assert (folder / name / 'parts.step').stat().st_size > 100
        for output in report['outputs']:
            assert (folder / name / output['stl']).stat().st_size > 84
            assert '<polyline' in (folder / name / output['preview']).read_text()
        assert elapsed < 10
        print(f'PASS {name}: build, requirements, STEP/STL/SVG, reloadable model ({elapsed:.3f}s)')
    bracket = json.loads((root / 'tools/model/bracket.request.json').read_text())
    accepted = (folder / 'bracket/report.json').read_bytes()
    bracket.update(step=False, stl=False, preview=False)
    def edit(parameter, value):
        return [dict(instance='bracket', parameter=parameter,
                     value={'scalar': dict(value=value, dimension='length', unit='millimeter')})]
    bracket['edits'] = edit('hole_spacing', 35)
    build(bracket, folder / 'bracket-edited')
    bracket['edits'] = edit('thickness', 1)
    rejected = folder / 'bracket-rejected'
    source = folder / 'rejected.request.json'
    source.write_text(json.dumps(bracket))
    process = subprocess.run([str(binary), str(source), str(rejected)], capture_output=True, text=True, timeout=30)
    assert process.returncode != 0 and not rejected.exists()
    error = json.loads(process.stderr.splitlines()[-1])
    assert error['status'] == 'failed' and error['stage'] == 'regeneration' and 'wall' in error['message']
    bracket['edits'] = edit('thickness', 5)
    build(bracket, folder / 'bracket-repaired')
    assert (folder / 'bracket/report.json').read_bytes() == accepted
    print('PASS bracket subprocess edit/reject/repair: structured failure, prior outputs preserved')
    request = json.loads((root / 'tools/model/shaft.request.json').read_text())
    request.update(step=False, stl=False, preview=False)
    for i in range(1, 1000):
        request['model']['instances'].append({'clone': {'id': f'shaft-{i}', 'source': 'shaft', 'overrides': {}, 'provenance': 'scale'}})
        request['outputs'].append({'instance': f'shaft-{i}', 'output': 'body'})
    report, elapsed = build(request, folder / 'scale')
    assert len(report['outputs']) == 1000 and len(report['verification']) == 1000
    assert report['generated_variants'] == 1
    assert all(abs(o['volume_mm3'] - 1500 * 3.141592653589793) < 1e-7 for o in report['outputs'])
    assert elapsed < 10
    print(f'PASS 1000 selected instances: one variant, validity, analytic volumes, requirements, persistence/report ({elapsed:.3f}s / 10s)')
    # Render limits: the most images one build may ask for, and the largest size.
    request = json.loads((root / 'tools/model/shaft.request.json').read_text())
    request.update(step=False, stl=False, preview=False)
    for i in range(1, 32):
        request['model']['instances'].append({'clone': {'id': f'shaft-{i}', 'source': 'shaft', 'overrides': {}, 'provenance': 'scale'}})
        request['outputs'].append({'instance': f'shaft-{i}', 'output': 'body'})
    views = ['iso', 'front', 'back', 'top', 'bottom', 'left', 'right']
    request['renders'] = dict(views=views, sections=[dict(id='axis', origin_mm=[0, 0, 0], normal=[1, 0, 0])])
    report, elapsed = build(request, folder / 'renders')
    images = [image['file'] for output in report['outputs'] for image in output['renders']]
    assert len(images) == 256 and all((folder / 'renders' / name).read_bytes()[:4] == b'\x89PNG' for name in images)
    assert elapsed < 10
    print(f'PASS 256 renders (32 outputs x 7 views + 1 section, 512 px) ({elapsed:.3f}s / 10s)')
    request = json.loads((root / 'tools/model/hemmed-tray.request.json').read_text())
    request.update(step=False, preview=False, renders=dict(views=views, sections=[dict(id='mid', origin_mm=[11.5, 15, -15], normal=[0, 1, 0])], size_px=1024))
    report, elapsed = build(request, folder / 'large-renders')
    assert sum(len(output['renders']) for output in report['outputs']) == 16
    assert elapsed < 10
    print(f'PASS 16 renders at 1024 px of a hemmed sheet-metal tray and its flat pattern ({elapsed:.3f}s / 10s)')
    # A full scene: 64 shafts 9 mm apart (Ø10, so each overlaps its
    # neighbors), with exact interference between every overlapping pair.
    request = json.loads((root / 'tools/model/shaft.request.json').read_text())
    request.update(step=False, stl=False, preview=False)
    base = request['model']['instances'][0]['base']
    request['model']['instances'] = []
    request['outputs'] = []
    for i in range(64):
        placed = dict(base, id=f'shaft-{i}', placement={'translation': {axis: {'value': 9.0 * i if axis == 'x' else 0, 'dimension': 'length', 'unit': 'millimeter'} for axis in 'xyz'}, 'rotation': None})
        request['model']['instances'].append({'base': placed})
        request['outputs'].append({'instance': f'shaft-{i}', 'output': 'body'})
    request['renders'] = dict(views=['iso'], sections=[dict(id='axes', origin_mm=[0, 0, 30], normal=[0, 0, 1])], size_px=512, scene=True)
    report, elapsed = build(request, folder / 'scene')
    clashes = report['scene']['interferences']
    assert len(clashes) == 63 and all(c['volume_mm3'] > 0 for c in clashes)
    assert elapsed < 10
    print(f'PASS 64-part scene: 63 exact interferences, 130 images ({elapsed:.3f}s / 10s)')
