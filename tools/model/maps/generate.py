#!/usr/bin/env python3
"""Author five self-contained map requests; --check detects stale generated examples."""
import argparse
import json
import math
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[3]
GRID = 25.4
MARGIN = 2.8
MAPS = [
    ('crossroads', 'Crossroads Keep', 'Four passages meet at a guarded central well.', '6 × 6 squares'),
    ('catacombs', 'Winding Catacombs', 'A dogleg passage with sealed stone tombs.', '8 × 6 squares'),
    ('shrine', 'The Sunken Shrine', 'A circular sanctuary, column ring and stepped altar.', 'Round · 6-inch base'),
    ('bridge', 'The Broken Crossing', 'Two banks and a timber bridge over a recessed channel.', '8 × 5 squares'),
    ('cavern', 'Emberfall Cavern', 'An irregular cave divided by a molten channel.', '6 × 6 squares'),
]

def quantity(value, dimension='length'):
    return dict(value=value, dimension=dimension, unit='millimeter' if dimension == 'length' else None)

def scalar(value):
    return dict(literal=quantity(value))

def vector(values, dimension='length'):
    return dict(literal={axis: quantity(value, dimension) for axis, value in zip('xyz', values)})

class Map:
    def __init__(self, slug, title):
        self.slug, self.title = slug, title
        self.features, self.parts = [], []

    def feature(self, kind, **args):
        name = f'g{len(self.features):04}'
        self.features.append(dict(id=name, operation={kind: args}))
        return name

    def box(self, x, y, z, w, d, h):
        return self.feature('box', origin=vector((x, y, z)), size=vector((w, d, h)))

    def cylinder(self, x, y, z, radius, height, axis=(0, 0, 1)):
        return self.feature('cylinder', origin=vector((x, y, z)), axis=vector(axis, 'scalar'), radius=scalar(radius), height=scalar(height))

    def cut(self, a, b):
        return self.feature('cut', object=a, tool=b)

    def join(self, items):
        items = list(items)
        assert items
        a = items[0]
        for b in items[1:]:
            a = self.feature('fuse', left=a, right=b)
        return a

    def compound(self, items):
        return self.feature('compound', inputs=list(items))

    def polygon(self, points, z, height):
        return self.feature('loft', sections=[dict(profile=points, origin=vector((0, 0, level)), x_axis=vector((1, 0, 0), 'scalar'), y_axis=vector((0, 1, 0), 'scalar'), scale=scalar(1)) for level in (z, z+height)], smooth=False, ruled=True)

    def rock(self, x, y, radius, height, sides=6):
        rows = []
        for z, r in [(2.2, radius), (2.2+height*.7, radius*.8), (2.2+height, radius*.3)]:
            points = [[x+r*math.cos(i*2*math.pi/sides), y+r*math.sin(i*2*math.pi/sides)] for i in range(sides)]
            rows.append(dict(profile=points, origin=vector((0, 0, z)), x_axis=vector((1, 0, 0), 'scalar'), y_axis=vector((0, 1, 0), 'scalar'), scale=scalar(1)))
        return self.feature('loft', sections=rows, smooth=False, ruled=True)

    def part(self, name, label, shape, color, group):
        self.features.append(dict(id=name, operation={'compound': dict(inputs=[shape])}))
        self.parts.append(dict(id=name, label=label, color=color, group=group))

    def base(self, w, d, color='#414b4b'):
        self.part('base', 'Flat-bottomed map base', self.box(0, 0, 0, w, d, 2.4), color, 'base')

    def tiles(self, cells, height=1.4):
        shapes = [self.box(MARGIN+x*GRID+.35, MARGIN+y*GRID+.35, 2.2, GRID-.7, GRID-.7, height) for x, y in sorted(cells)]
        self.part('floor', 'One-inch flagstone grid', self.join(shapes), '#8c8876', 'floor')

    def perimeter(self, cells, openings=()):
        cells, openings = set(cells), set(openings)
        walls = []
        corners = {}
        t = 2.8
        for x, y in sorted(cells):
            px, py = MARGIN+x*GRID, MARGIN+y*GRID
            for dx, dy in ((-1, 0), (1, 0), (0, -1), (0, 1)):
                if (x+dx, y+dy) in cells or (x, y, dx, dy) in openings:
                    continue
                if dx:
                    xx = px-t if dx < 0 else px+GRID
                    wall = self.box(xx, py, 2.2, t, GRID, 13.8)
                    groove = self.box(xx+(t-.45 if dx < 0 else -.05), py-.05, 8.2, .5, GRID+.1, .55)
                else:
                    yy = py-t if dy < 0 else py+GRID
                    wall = self.box(px, yy, 2.2, GRID, t, 13.8)
                    groove = self.box(px-.05, yy+(t-.45 if dy < 0 else -.05), 8.2, GRID+.1, .5, .55)
                walls.append(self.cut(wall, groove))
                ends = [(px if dx < 0 else px+GRID, py), (px if dx < 0 else px+GRID, py+GRID)] if dx else [(px, py if dy < 0 else py+GRID), (px+GRID, py if dy < 0 else py+GRID)]
                for point in ends:
                    key = tuple(round(v, 6) for v in point)
                    corners[key] = corners.get(key, 0)+1
        # Rounded junctions give corner-touching wall segments a volumetric join.
        for (x, y), count in sorted(corners.items()):
            if count >= 2:
                walls.append(self.cylinder(x, y, 2.2, 1.2, 13.8))
        self.part('walls', 'Low masonry walls', self.join(walls), '#77796d', 'walls')

    def request(self, version):
        final = self.join(p['id'] for p in self.parts)
        self.features.append(dict(id='map', operation={'compound': dict(inputs=[final])}))
        requirements = []
        for name, kind, statement, rule in [
            ('valid', 'validation', 'Map must be valid geometry', {'shape_valid': dict(output='map')}),
            ('one-solid', 'topological', 'Map must be one connected solid with no sealed voids', {'connectivity': dict(output='map', solids=1, allow_voids=False)}),
        ]:
            requirements.append(dict(id=name, version=1, kind=kind, priority='required', statement=statement, rule=rule, provenance='Demonstration tabletop terrain; physical print and fit trials remain necessary.', traces=[dict(feature='map')]))
        family = dict(id='map-'+self.slug, version=1, parameters=[], derived_parameters=[], derived_vector_parameters=[], constraints=[], requirements=requirements, datums=[], features=self.features)
        return dict(schema='occb-model-request-v1', model=dict(schema_version=version, family=family, instances=[{'base': dict(id='map', overrides={}, provenance=self.title+'; one-inch grid; dimensions in millimeters.')}]), outputs=[dict(instance='map', output='map')], step=True, stl=True, preview=False, renders=dict(views=['iso', 'top'], size_px=512))

def crossroads(m):
    cells={(x,y) for x in range(6) for y in range(6) if x in (2,3) or y in (2,3)}
    m.base(6*GRID+2*MARGIN, 6*GRID+2*MARGIN)
    m.tiles(cells)
    openings={(x,y,dx,dy) for x,y in cells for dx,dy in [(-1,0),(1,0),(0,-1),(0,1)] if (x==0 and dx==-1) or (x==5 and dx==1) or (y==0 and dy==-1) or (y==5 and dy==1)}
    m.perimeter(cells, openings)
    c=MARGIN+3*GRID
    well=m.cut(m.cylinder(c,c,3.4,8,4),m.cylinder(c,c,3.9,5.7,5))
    m.part('well','Central stone well',well,'#ada28b','props')

def catacombs(m):
    cells={(x,y) for x in range(8) for y in range(6) if (y<2 and x<6) or (x in (2,3)) or (y>=4 and x>=2)}
    m.base(8*GRID+2*MARGIN,6*GRID+2*MARGIN)
    m.tiles(cells)
    m.perimeter(cells,{(0,0,-1,0),(0,1,-1,0),(7,4,1,0),(7,5,1,0)})
    tombs=[]
    for x,y in [(5,0),(5,1),(7,4),(7,5)]:
        cx=MARGIN+(x+.5)*GRID;cy=MARGIN+(y+.5)*GRID
        pts=[[-5,-9],[3,-9],[5,-6],[5,7],[3,9],[-3,9],[-5,7],[-5,-6]]
        tombs.append(m.polygon([[cx+a,cy+b] for a,b in pts],3.4,5))
        tombs.append(m.box(cx-.55,cy-5,8.3,1.1,10,1.0))
        tombs.append(m.box(cx-2.6,cy-.55,8.3,5.2,1.1,1.0))
    m.part('tombs','Four raised stone tombs',m.join(tombs),'#a39d8d','props')

def shrine(m):
    c=76.2;r=76.2
    m.part('base','Circular flat-bottomed base',m.cylinder(c,c,0,r,2.4),'#414b4b','base')
    floor=m.cylinder(c,c,2.2,70.5,1.4)
    grooves=[]
    for i in range(1,6):
        p=i*GRID
        grooves.extend([m.box(p-.3,0,3.25,.6,153,.5),m.box(0,p-.3,3.25,153,.6,.5)])
    floor=m.cut(floor,m.compound(grooves))
    m.part('floor','Circular gridded sanctuary floor',floor,'#b4ac95','floor')
    wall=m.cut(m.cylinder(c,c,2.2,72,12),m.cylinder(c,c,2,68,13))
    wall=m.cut(wall,m.box(c-GRID,-1,2,2*GRID,35,14))
    m.part('walls','Curved sanctuary wall and open entrance',wall,'#7a817b','walls')
    columns=[]
    for degrees in [0,45,90,135,180]:
        a=math.radians(degrees);x=c+48*math.cos(a);y=c+48*math.sin(a)
        columns.extend([m.cylinder(x,y,3.4,5,1.4),m.cylinder(x,y,4.6,3.4,17),m.cylinder(x,y,21.4,4.5,1.5)])
    m.part('columns','Five sanctuary columns',m.join(columns),'#c2baa2','props')
    dais=m.join([m.cylinder(c,c,3.4,21,1.5),m.cylinder(c,c,4.7,17,1.6),m.box(c-9,c-5,6.1,18,10,5)])
    m.part('altar','Stepped central altar',dais,'#8d9e95','props')

def bridge(m):
    w=8*GRID+2*MARGIN;d=5*GRID+2*MARGIN;m.base(w,d,'#3d6268')
    banks=[]
    for x0 in [0,5]:
        banks.append(m.box(MARGIN+x0*GRID,MARGIN,2.2,3*GRID,5*GRID,5.4))
        # Raised flagstone surfaces on the two banks.
        for x in range(x0,x0+3):
            for y in range(5):banks.append(m.box(MARGIN+x*GRID+.35,MARGIN+y*GRID+.35,7.4,GRID-.7,GRID-.7,.8))
    m.part('banks','Two raised stone banks',m.join(banks),'#8b8675','floor')
    y=d/2;start=MARGIN+2.5*GRID;length=3*GRID
    beams=[m.box(start,y-15,6.9,length,2.6,2.1),m.box(start,y+12.4,6.9,length,2.6,2.1)]
    # Permanent piers connect the bridge to the map, avoiding floating geometry.
    for x in [start+20,start+55]:beams.append(m.box(x,y-15,2.2,4,30,5.1))
    for i in range(16):beams.append(m.box(start+i*length/16,y-17,8.7,length/16-.35,34,1.5))
    m.part('bridge','Static timber bridge and support piers',m.join(beams),'#9b7046','props')
    rails=[]
    for yy in [y-16,y+16]:
        rails.append(m.box(start-.9,yy-.9,18.5,length+1.8,1.8,2))
        for i in range(6):rails.append(m.box(start+i*length/5-.9,yy-.9,9.9,1.8,1.8,10.6))
    m.part('rails','Bridge posts and handrails',m.join(rails),'#624e3b','props')
    walls=[m.box(MARGIN,0,2.2,3*GRID,2.8,12),m.box(MARGIN+5*GRID,0,2.2,3*GRID,2.8,12),m.box(MARGIN,d-2.8,2.2,3*GRID,2.8,12),m.box(MARGIN+5*GRID,d-2.8,2.2,3*GRID,2.8,12)]
    m.part('walls','Cutaway bank walls',m.join(walls),'#72796d','walls')

def cavern(m):
    m.base(6*GRID+2*MARGIN,6*GRID+2*MARGIN,'#363c3c')
    outline=[[8,22],[24,5],[106,3],[144,20],[154,68],[144,136],[92,153],[31,145],[4,104]]
    floor=m.polygon(outline,2.2,1.4)
    river=[[77,0],[91,0],[99,37],[90,75],[106,114],[99,158],[78,158],[85,114],[68,75],[78,37]]
    floor=m.cut(floor,m.polygon(river,2.0,2.1))
    m.part('floor','Irregular cavern floor',floor,'#797e75','floor')
    lava=m.feature('common',left=m.polygon(river,2.25,.45),right=m.polygon(outline,2.2,1))
    m.part('lava','Recessed molten channel',lava,'#e28c39','flames')
    rocks=[]
    for i,(x,y) in enumerate(outline):
        nx,ny=outline[(i+1)%len(outline)];n=max(1,int(math.hypot(nx-x,ny-y)/19))
        for j in range(n):
            t=j/n;xx=x+(nx-x)*t;yy=y+(ny-y)*t
            # Pull the boundary rocks inward to keep the footprint compact.
            xx=.95*xx+.05*79;yy=.95*yy+.05*79
            rocks.append(m.rock(xx,yy,5+(i+j)%3,10+(i*3+j)%8,5+(i%2)))
    m.part('rocks','Faceted cavern perimeter',m.join(rocks),'#5e6866','walls')
    stones=[m.cylinder(x,76,2.2,10.5,3.2) for x in [61,82.5,104]]
    m.part('crossing','Raised stone crossing',m.join(stones),'#a09b80','props')
    spikes=[m.rock(x,y,3,9+i%3*2,5) for i,(x,y) in enumerate([(35,118),(43,123),(39,131),(126,39),(135,44)])]
    m.part('stalagmites','Stalagmite clusters',m.join(spikes),'#8c9282','props')

BUILDERS=[crossroads,catacombs,shrine,bridge,cavern]

def version():
    source=(ROOT/'rust/occt-parametric-core/src/document.rs').read_text()
    return int(re.search(r'CURRENT_SCHEMA_VERSION: u32 = (\d+)',source)[1])

def examples():
    for record,builder in zip(MAPS,BUILDERS):
        slug,title,description,size=record;m=Map(slug,title);builder(m)
        yield dict(slug=slug,title=title,description=description,size=size,parts=m.parts,request=m.request(version()))

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--check',action='store_true');args=parser.parse_args()
    for entry in examples():
        path=ROOT/'tools/model'/('map-'+entry['slug']+'.request.json');text=json.dumps(entry['request'],indent=2)+'\n'
        if args.check:
            if not path.is_file() or path.read_text()!=text:raise SystemExit(f'Stale generated example: {path}; run {__file__}')
        else:path.write_text(text)
        print(('Checked ' if args.check else 'Wrote ')+path.name)

if __name__=='__main__':main()
