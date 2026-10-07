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
