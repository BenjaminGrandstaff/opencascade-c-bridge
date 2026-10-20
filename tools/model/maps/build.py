#!/usr/bin/env python3
"""Build and verify all five maps through the actual stdio MCP server."""
import argparse
import base64
import collections
import json
from pathlib import Path
import select
import struct
import subprocess
import sys
import zipfile

from generate import ROOT, examples

class Client:
    def __init__(self, binary, root):
        self.id=0
        self.process=subprocess.Popen([sys.executable,str(ROOT/'tools/model/mcp_server.py'),'--binary',str(binary),'--output-root',str(root),'--timeout','300'],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
        self.call('initialize',dict(protocolVersion='2025-11-25',capabilities={},clientInfo=dict(name='printable-map-example',version='1')))
        self.send(dict(jsonrpc='2.0',method='notifications/initialized'))

    def send(self, value):
        self.process.stdin.write((json.dumps(value)+'\n').encode());self.process.stdin.flush()

    def call(self, method, params):
        self.id+=1;self.send(dict(jsonrpc='2.0',id=self.id,method=method,params=params))
        if not select.select([self.process.stdout],[],[],330)[0]:raise TimeoutError('MCP response timed out')
        line=self.process.stdout.readline()
        if not line:raise RuntimeError(self.process.stderr.read().decode())
        result=json.loads(line)
        if result.get('id')!=self.id or 'error' in result:raise RuntimeError(result)
        return result['result']

    def tool(self, name, request):
        result=self.call('tools/call',dict(name=name,arguments=request))
        if result['isError']:raise RuntimeError(result['structuredContent'])
        return result['structuredContent']

    def resource(self, uri):
        item=self.call('resources/read',dict(uri=uri))['contents'][0]
        return item['text'].encode() if 'text' in item else base64.b64decode(item['blob'])

    def close(self):
        self.process.stdin.close()
        try:self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:self.process.kill();self.process.wait()
        self.process.stdout.close();self.process.stderr.close()

def clean_stl(data):
    """Drop zero-area pole triangles in the delivered copy, then check closed edges."""
    if len(data)<84:raise ValueError('Expected a binary STL export')
    count=struct.unpack_from('<I',data,80)[0]
    if len(data)!=84+count*50:raise ValueError('Expected a binary STL export')
    edges=collections.Counter();kept=[];removed=0
    for i in range(count):
        block=data[84+50*i:134+50*i];values=struct.unpack('<12fH',block)[3:12]
        points=[tuple(round(v,5) for v in values[j:j+3]) for j in (0,3,6)]
        if len(set(points))<3:removed+=1;continue
        kept.append(block)
        for j in range(3):edges[tuple(sorted((points[j],points[(j+1)%3])))]+=1
    bad=sum(n!=2 for n in edges.values())
    if bad:raise ValueError(f'Export has {bad} mesh edges without exactly two incident triangles')
    return data[:80]+struct.pack('<I',len(kept))+b''.join(kept),dict(triangles=len(kept),zero_area_triangles_removed=removed,nonmanifold_edges=bad)

def build_all(binary, out):
    out.mkdir(parents=True,exist_ok=False)
    client=Client(binary,out/'accepted-builds');gallery=[]
    try:
        for entry in examples():
            slug=entry['slug'];folder=out/slug;folder.mkdir()
            request=client.tool('occt_get_example',dict(name='map-'+slug))
            if request!=entry['request']:raise ValueError('Generated request differs from the MCP example; run generate.py')
            (folder/'request.json').write_text(json.dumps(request,indent=2)+'\n')
            built=client.tool('occt_build',request);report=built['report'];output=report['outputs'][0]
            if not all(r['status']=='passed' for r in report['verification']):raise ValueError(report)
            bounds=output['bounds_mm'];size=[bounds['max'][i]-bounds['min'][i] for i in range(3)]
            if not output['valid'] or abs(bounds['min'][2])>1e-6 or max(size[:2])>220:raise ValueError(output)
            for source,target in [('model.json','model.json'),(report['artifacts']['step'],'map.step')]:
                (folder/target).write_bytes(client.resource(built['resources'][source]))
            data,mesh=clean_stl(client.resource(built['resources'][output['stl']]))
            (folder/'map.stl').write_bytes(data)
            for image in output['renders']:
                (folder/(image['view']+'.png')).write_bytes(client.resource(built['resources'][image['file']]))
            (folder/'report.json').write_text(json.dumps(report,indent=2)+'\n')
            (folder/'mesh-checks.json').write_text(json.dumps(mesh,indent=2)+'\n')
            view=client.tool('occt_visualize_model',dict(schema='occb-model-view-v1',model=request['model'],outputs=[dict(instance='map',output=p['id']) for p in entry['parts']],sketches=False))
            scenes=json.loads(client.resource(view['resources']['view.json']))['scenes'];by_id={s['feature']:s for s in scenes}
            item={k:entry[k] for k in ('slug','title','description','size')};item.update(dimensions_mm=size,mesh=mesh,parts=[])
            for part in entry['parts']:
                scene=by_id[part['id']]
                if not scene['valid']:raise ValueError(scene)
                item['parts'].append(dict(part,triangles=scene['mesh'],bounds=scene['bounds']))
            gallery.append(item)
            print(f"PASS {entry['title']}: one solid, {mesh['triangles']} closed STL triangles, {size[0]:.1f} × {size[1]:.1f} mm",flush=True)
    finally:client.close()
    (out/'gallery-data.json').write_text(json.dumps(gallery,separators=(',',':')))
    return gallery

def package(out):
    text='''# Five printable dungeon maps\n\nBuilt with the local MCP tools and exact Open Cascade geometry.\nAll maps use a 25.4 mm (1-inch) grid and include STL, STEP, editable model,\nsource request, native isometric/top renders and verification reports.\nEach map is one connected solid with a flat bottom at Z=0.\n\nOpen index.html for the self-contained colored gallery. Colors are suggested\npainting colors and are not stored in the STL. Each map folder has map.stl,\nmap.step, model.json and request.json. Import STL in millimeters at 100%.\nThe largest footprint is under 220 × 220 mm; check your actual printable bed\narea and support needs, especially below the bridge and its rails.\n\nThese are fixed single-piece maps with open tops, not modular locking tiles\nor moving mechanisms. Well and tomb details can occupy grid space.\nPrint strength, supports, miniature clearance and physical fit remain untested.\nGeometry checks verify the declared properties, not successful printing.\n\nDelivered STL copies have zero-area pole triangles removed where necessary.\nAccepted MCP builds remain unchanged. mesh-checks.json records the closed-edge\nchecks, and report.json records native validity and one-solid requirements.\n'''
    (out/'README.md').write_text(text)
    with zipfile.ZipFile(out/'printable-dungeon-maps.zip','w',zipfile.ZIP_DEFLATED) as z:
        for folder in [p for p in out.iterdir() if p.is_dir() and p.name!='accepted-builds']:
            for file in sorted(folder.iterdir()):
                if file.is_file():z.write(file,str(file.relative_to(out)))
        for name in ['README.md','index.html','gallery.png']:
            if (out/name).exists():z.write(out/name,name)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();binary=args.binary.resolve(strict=True);out=args.output.resolve()
    data=build_all(binary,out)
    from gallery import write_gallery
    write_gallery(out,data);package(out)
    print('Gallery:',out/'index.html',flush=True)

if __name__=='__main__':main()
