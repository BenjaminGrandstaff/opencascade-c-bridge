import {test} from 'node:test';
import assert from 'node:assert/strict';
import {snapPosition,drawPrimitive,primitiveGhost,nextId,createCanvasTools,arcEnd,ellipseMinor} from './canvas-tools.mjs';
const empty=()=>({points:[],lines:[],circles:[],constraints:[],profile:[]});
const click=(x,y,point=null)=>({xy:[x,y],point});

test('snapping prefers authored native point positions, then an optional grid',()=>{
  const d=drawPrimitive(empty(),'point',[click(2,3)]).definition;
  assert.deepEqual(snapPosition([10.2,8.1],d,{P1:[10,8,0]},1,.5),{point:'P1',xy:[10,8]});
  assert.deepEqual(snapPosition([4.4,7.6],d,{},1,.1),{point:null,xy:[4,8]});
  assert.deepEqual(snapPosition([4.4,7.6],d,{},0,.1),{point:null,xy:[4.4,7.6]});
  assert.throws(()=>snapPosition([NaN,1],d),/finite/);
});
test('drawing names stay stable, reuse picked points, and create closed constrained rectangles',()=>{
  const source=empty(),rect=drawPrimitive(source,'rectangle',[click(0,0),click(10,5)],{profile:true}).definition;
  assert.equal(source.points.length,0);assert.equal(rect.points.length,4);assert.equal(rect.lines.length,4);assert.equal(rect.constraints.length,4);assert.deepEqual(rect.profile,rect.lines.map(l=>l.id));
  for(let i=0;i<4;i++)assert.equal(rect.lines[i].end,rect.lines[(i+1)%4].start);
  const line=drawPrimitive(rect,'line',[click(0,0,'P1'),click(3,4)]).definition;assert.equal(line.lines.at(-1).start,'P1');assert.equal(line.points.length,5);assert.equal(nextId(line,'P'),'P6');
  const circle=drawPrimitive(source,'circle',[click(1,1),click(4,1)],{profile:true}).definition;assert.deepEqual(circle.profile,['C1']);assert.equal(circle.circles[0].center,'P1');
  assert.throws(()=>drawPrimitive(source,'rectangle',[click(0,0),click(0,2)]),/width/);
  assert.throws(()=>drawPrimitive({...source,holes:[['hole']]},'circle',[click(0,0),click(2,0)],{profile:true}),/profile controls/);
  assert.equal(primitiveGhost('circle',[click(0,0)],click(2,0)).length,65);
});
test('arc construction projects free ends to the radius and never moves an off-radius picked point',()=>{
  const source=empty(),arc=drawPrimitive(source,'arc',[click(0,0),click(5,0),click(0,9)],{clockwise:true}).definition;
  assert.equal(arc.arcs[0].clockwise,true);assert.equal(arc.points[2].y.literal.value,5);assert.equal(source.points.length,0);
  assert.throws(()=>arcEnd([click(0,0),click(5,0),click(0,9,'existing')]),/picked endpoint/);
  assert.throws(()=>arcEnd([click(0,0),click(5,0),click(8,0)]),/distinct/);
  const ccw=primitiveGhost('arc',[click(0,0),click(5,0)],click(0,9));assert.equal(ccw.length,65);assert(Math.abs(ccw.at(-1)[1]-5)<1e-10);assert(ccw[20][1]>0);
  const cw=primitiveGhost('arc',[click(0,0),click(5,0)],click(0,9),{clockwise:true});assert(cw[20][1]<0);
});
test('closed native spline definitions repeat the first reference and protect edited profiles',()=>{
  const source=empty(),points=[click(0,0),click(6,0),click(6,5),click(0,5)],d=drawPrimitive(source,'spline',points,{closed:true,profile:true}).definition;
  assert.deepEqual(d.splines[0].points,['P1','P2','P3','P4','P1']);assert.deepEqual(d.profile,['S1']);assert.equal(source.points.length,0);
  assert.equal(drawPrimitive(source,'spline',points).definition.profile.length,0);
  assert.throws(()=>drawPrimitive(source,'spline',[click(0,0),click(6,0)],{closed:true}),/at least 3/);
  assert.throws(()=>drawPrimitive(source,'spline',[...points,points[0]],{closed:true}),/distinct/);
  assert.throws(()=>drawPrimitive({...source,region_edits:[{}]},'spline',points,{closed:true,profile:true}),/profile controls/);
  assert.equal(nextId({...source,curve_operations:[{split:{before:'S1',after:'S2'}}]},'S'),'S3');
});
test('ellipses project a free minor width perpendicular to a rotated major axis without moving picked points',()=>{
  const clicks=[click(1,2),click(4,6),click(1,7)];
  const minor=ellipseMinor(clicks);assert(Math.abs((minor.xy[0]-1)*3+(minor.xy[1]-2)*4)<1e-9);
  const d=drawPrimitive(empty(),'ellipse',clicks,{profile:true}).definition;assert.deepEqual(d.profile,['E1']);assert.equal(d.ellipses[0].minor,'P3');
  assert.throws(()=>ellipseMinor([click(0,0),click(5,0),click(2,3,'old')]),/perpendicular/);
  assert.throws(()=>ellipseMinor([click(0,0),click(5,0),click(0,6)]),/exceed/);
  const ghost=primitiveGhost('ellipse',clicks.slice(0,2),clicks[2]);assert.equal(ghost.length,65);assert(Math.abs(ghost[0][0]-4)<1e-9);assert(Math.abs(ghost[16][0]-minor.xy[0])<1e-9);
});
test('slots and polygons save parametric shortcuts with closed boundary names and exact guides',()=>{
  const source=empty(),slot=drawPrimitive(source,'slot',[click(0,0),click(10,0),click(20,3)],{profile:true}).definition;
  assert.deepEqual(slot.shapes[0].slot,{id:'Slot1',start:'P1',end:'P2',radius:{literal:{value:3,dimension:'length',unit:'millimeter'}}});assert.equal(slot.points.length,2);assert.deepEqual(slot.profile,['Slot1']);
  const polygon=drawPrimitive(source,'polygon',[click(0,0),click(4,0)],{sides:5,profile:true}).definition;assert.equal(polygon.shapes[0].polygon.sides,5);assert.deepEqual(polygon.profile,['Polygon1']);assert.equal(source.points.length,0);
  assert.throws(()=>drawPrimitive(source,'slot',[click(0,0),click(10,0),click(5,0)]),/positive width/);
  assert.throws(()=>drawPrimitive(source,'polygon',[click(0,0),click(4,0)],{sides:3.5}),/integer/);
  assert.throws(()=>drawPrimitive({...source,holes:[['hole']]},'slot',[click(0,0),click(10,0),click(5,3)],{profile:true}),/profile controls/);
  const ghost=primitiveGhost('slot',[click(0,0),click(10,0)],click(5,3));assert.equal(ghost.length,67);assert(Math.abs(Math.min(...ghost.map(p=>p[0]))+3)<1e-9);assert(Math.abs(Math.max(...ghost.map(p=>p[0]))-13)<1e-9);
  assert.equal(primitiveGhost('polygon',[click(0,0)],click(4,0),{sides:5}).length,6);
});
class Element{constructor(tag){this.tagName=tag;this.children=[];this.value='';}append(...items){this.children.push(...items);}setAttribute(k,v){this[k]=v;}}
const all=e=>[e,...e.children.flatMap(all)];
test('canvas gestures cancel unfinished drawings and retain completed local ghosts until native preview',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.value=value;}};
 let d=empty(),renders=0,paths=[],drags=[];const host=new Element('div');
 const tools=createCanvasTools({host,getDraft:()=>d,setDraft:value=>d=value,changed:()=>renders++,message(){},ghost:p=>paths=p,dragPoint:async(id,target)=>drags.push([id,target])});
 const select=all(host).find(e=>e['aria-label']==='Canvas tool');select.value='line';select.onchange();
 const event=(x,y,point)=>({position:[x,y],point,tolerance:.01,scene:{points:{}},shiftKey:false});
 tools.down(event(0,0));tools.move(event(10,5));assert.equal(paths.length,1);tools.cancel();assert.equal(paths.length,0);assert.equal(d.points.length,0);
 tools.down(event(0,0));tools.down(event(10,5));assert.equal(d.lines.length,1);assert.equal(renders,1);assert.equal(paths.length,1);tools.reviewed();assert.equal(paths.length,0);
 select.value='drag';select.onchange();assert.equal(tools.down(event(0,0,'P1')),true); // Fixed point: no drag.
 await tools.up(event(4,4));assert.equal(drags.length,0);
 d.points[0].fixed=false;tools.down(event(0,0,'P1'));await tools.up(event(4,4));assert.deepEqual(drags,[['P1',[4,4]]]);
 delete globalThis.document;delete globalThis.Option;
});
test('multi-click arc/spline gestures stage points, cancel safely and finish only valid spline inputs',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.value=value;}};
 let d=empty(),renders=0;const host=new Element('div'),messages=[];
 const tools=createCanvasTools({host,getDraft:()=>d,setDraft:value=>d=value,changed:()=>renders++,message:m=>messages.push(m),ghost(){},dragPoint:async()=>{}});
 const field=name=>all(host).find(e=>e['aria-label']===name),select=field('Canvas tool'),finish=all(host).find(e=>e.textContent==='Finish spline');
 const event=(x,y)=>({position:[x,y],tolerance:0,scene:{points:{}},shiftKey:false});
 select.value='arc';select.onchange();tools.down(event(0,0));tools.down(event(0,0));tools.down(event(5,0));assert.equal(d.points.length,0);tools.down(event(0,9));assert.equal(d.arcs.length,1);assert.equal(renders,1);
 select.value='spline';select.onchange();tools.down(event(10,10));tools.down(event(15,10));assert.equal(d.splines?.length??0,0);assert.equal(finish.disabled,false);
 field('Close spline').checked=true;field('Close spline').onchange();assert.equal(finish.disabled,true);tools.down(event(15,15));assert.equal(finish.disabled,false);tools.cancel();assert.equal(finish.disabled,true);
 tools.down(event(10,10));tools.down(event(15,10));tools.down(event(15,15));finish.onclick();assert.equal(d.splines.length,1);assert.equal(d.splines[0].points[0],d.splines[0].points.at(-1));assert.equal(renders,2);
 delete globalThis.document;delete globalThis.Option;
});

test('drawing reuses exact projected endpoint, junction and intersection names rather than freezing points',()=>{
 const source={...empty(),projections:[{id:'reference',kind:'chain',junctions:[{id:'mid'}],intersections:[{id:'cut'}]}]},solved={'reference:start':[0,0,0],'reference:end':[10,0,0],'reference:junction:mid':[5,6,0],'reference:intersection:cut':[8,4,0]};
 for(const [id,p]of Object.entries(solved)){
  const snap=snapPosition([p[0]+.1,p[1]+.1],source,solved,1,.3);assert.equal(snap.point,id);
  const result=drawPrimitive(source,'line',[snap,click(20,20)]);assert.equal(result.definition.lines[0].start,id);assert.equal(result.definition.points.length,1);assert.equal(source.points.length,0);
 }
 const spline=drawPrimitive(source,'spline',[click(0,0,'reference:start'),click(5,6,'reference:junction:mid'),click(10,0,'reference:end')]);assert.equal(spline.definition.points.length,0);assert.deepEqual(spline.definition.splines[0].points,['reference:start','reference:junction:mid','reference:end']);
 const circle=drawPrimitive(source,'circle',[click(8,4,'reference:intersection:cut'),click(12,4)]);assert.equal(circle.definition.circles[0].center,'reference:intersection:cut');assert.equal(circle.definition.points.length,1);
});
test('projected snaps require finite native positions and honor an explicit clicked reference at coincident points',()=>{
 const d={...empty(),points:[{id:'fixed',x:{literal:{value:0,dimension:'length',unit:'millimeter'}},y:{literal:{value:0,dimension:'length',unit:'millimeter'}},fixed:true}],projections:[{id:'reference',kind:'line'}]},solved={'reference:start':[0,0,0],'reference:end':[NaN,2,0],unknown:[9,9,0]};
 assert.equal(snapPosition([0,0],d,solved,0,.1).point,'fixed');assert.equal(snapPosition([0,0],d,solved,0,.1,'reference:start').point,'reference:start');assert.equal(snapPosition([9,9],d,solved,0,.1).point,null);assert.equal(snapPosition([1,2],d,solved,0,.1).point,null);assert.equal(snapPosition([3,3],d,{},0,.1).point,null);
});

test('canvas drawing exposes the snapped reference and commits that alias without creating its point',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.value=value;}};
 try{let d={...empty(),projections:[{id:'reference',kind:'chain',junctions:[{id:'right'}]}]};const host=new Element('div'),tools=createCanvasTools({host,getDraft:()=>d,setDraft:value=>d=value,changed(){},message(){},ghost(){},dragPoint:async()=>{}}),tool=all(host).find(e=>e['aria-label']==='Canvas tool'),scene={points:{'reference:junction:right':[10,0,0]}};
 tool.value='line';tool.onchange();const event={scene,point:'reference:junction:right',position:[10,0],tolerance:.5};tools.move(event);assert(all(host).some(e=>e.textContent==='Snap: reference:junction:right'));tools.down(event);tools.down({scene,position:[20,10],tolerance:.5});assert.equal(d.lines[0].start,'reference:junction:right');assert.equal(d.points.length,1);assert(!all(host).some(e=>e.textContent==='Snap: reference:junction:right'));
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('projected point snapping remains bounded for ten thousand declared native references',()=>{
 const d={...empty(),projections:Array.from({length:5000},(_,i)=>({id:`ref${i}`,kind:'line'}))},solved={};for(let i=0;i<5000;i++){solved[`ref${i}:start`]=[6*i,0,0];solved[`ref${i}:end`]=[6*i+3,0,0];}
 const now=performance.now();for(let i=0;i<100;i++){const n=49*i;assert.equal(snapPosition([6*n+.01,0],d,solved,0,.1).point,`ref${n}:start`);}const elapsed=performance.now()-now;console.log(`100 snaps over 10000 projected point references: ${elapsed.toFixed(1)} ms`);assert(elapsed<2000);
});
