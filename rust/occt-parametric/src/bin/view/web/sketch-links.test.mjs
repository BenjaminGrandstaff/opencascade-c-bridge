import {test} from 'node:test';
import assert from 'node:assert/strict';
import {availableSources,attachPickedFace,requireConstructionProfile,createSketchLinks} from './sketch-links.mjs';
import {geometryIds,makeConstraint} from './constraint-controls.mjs';
const features=[{id:'plate',operation:{box:{}}},{id:'sketch',operation:{sketch_face:{sketch:{face_support:{input:'plate'}}}}},{id:'tool',operation:{extrude:{input:'sketch'}}},{id:'body',operation:{cut:{object:'plate',tool:'tool'}}}];
test('source choices exclude the edited sketch and transitive dependents',()=>assert.deepEqual(availableSources(features,'sketch').map(f=>f.id),['plate']));
test('reattachment preserves local points and offset expressions and replaces the support frame',()=>{
 const points=[{id:'A',x:{parameter:'x'}}],offset={parameter:'offset'},draft={points,face_support:{offset},datum_plane:'old'};
 const pick={face_support:{input:'plate',face:{}},x_axis:{x:1},y_axis:{y:1}};
 attachPickedFace(draft,pick);assert.equal(draft.points,points);assert.deepEqual(draft.face_support.offset,offset);assert.equal(draft.datum_plane,null);assert.deepEqual(draft.x_axis,pick.x_axis);
});
test('projected references are selectable for typed constraints without unstable spline poles',()=>{
 const d={points:[{id:'A'}],lines:[{id:'AB'}],projections:[{id:'ref',kind:'line'},{id:'curve',kind:'spline'}]};
 assert.deepEqual(geometryIds(d,'lines'),['AB','ref']);assert(geometryIds(d,'points').includes('ref:start'));assert(!geometryIds(d,'points').some(id=>id.includes('pole')));
 assert.deepEqual(makeConstraint(d,'coincident',{first:'A',second:'ref:start'}),{coincident:{first:'A',second:'ref:start'}});
 assert.throws(()=>requireConstructionProfile({profile:[]}),/explicit/);requireConstructionProfile({profile:['AB']});
 requireConstructionProfile({region_discovery:{entities:['AB']}});
});
class Element{constructor(tag){this.tagName=tag;this.children=[];this.value='';}append(...e){this.children.push(...e);}setAttribute(k,v){this[k]=v;}}
const all=e=>[e,...e.children.flatMap(all)];
const button=(h,t)=>all(h).find(e=>e.tagName==='button'&&e.textContent===t);
test('source selection is read-only and cancellation discards a late load or pick',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 const host=new Element('div'),draft={points:[],profile:['circle']},calls=[],messages=[];let resolveSource,resolvePick,changed=0;
 const links=createSketchLinks({context:()=>({draft,feature:'sketch',features,version:3,instance:'part'}),request:async(path,options)=>{
  calls.push([path,JSON.parse(options.body)]);
  if(path==='/api/sketch-source-view')return new Promise(r=>resolveSource=r);
  return new Promise(r=>resolvePick=r);
 },preview:async()=>{},showSolid(){},changed:()=>changed++,message:m=>messages.push(m),addProjection:(d,p)=>(d.projections??=[]).push(p)});
 links.render(host);
 const loading=button(host,'Project picked edge').onclick();assert(!links.edgePicking);links.cancel();resolveSource({version:3});await loading;assert(!links.edgePicking);
 const starting=button(host,'Project picked edge').onclick();resolveSource({version:3});await starting;assert(links.edgePicking);
 const selecting=links.pickEdge({accepted:true,version:3,instance:'part',output:'plate',edge:2});links.cancel();resolvePick({version:3,projection:{id:'ref'}});await selecting;assert.equal(changed,0);
 const again=button(host,'Project picked edge').onclick();resolveSource({version:3});await again;
 const chosen=links.pickEdge({accepted:true,version:3,instance:'part',output:'plate',edge:4});resolvePick({version:3,projection:{id:'reference',input:'plate',kind:'line',edge:{at_extreme:{}}}});await chosen;
 assert.equal(changed,1);assert.equal(draft.projections[0].id,'reference');assert.equal(calls.at(-1)[1].edge,4);
 const nextHost=new Element('div');links.render(nextHost);assert.equal(all(nextHost).find(e=>e['aria-label']==='Projection name').value,'reference-2');
 delete globalThis.document;delete globalThis.Option;
});
test('projected curve picks are armed, cancellable, and select the refinement panel without mutating links',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.value=value;}};
 const draft={profile:['ref'],projections:[{id:'ref',input:'plate',kind:'spline',edge:{all:true}}]},host=new Element('div'),highlights=[];let changes=0,picks=0;
 const links=createSketchLinks({context:()=>({draft,feature:'sketch',features}),request:async()=>{},preview:async()=>{},changed:()=>changes++,message(){},highlight:ids=>highlights.push(ids),beginProjectionPicking:()=>picks++});links.render(host);
 assert.equal(links.pickProjection('ref'),false);await button(host,'Select projected spline from canvas').onclick();assert.equal(picks,1);assert.equal(links.pickProjection('unknown'),true);assert.equal(links.pickProjection('ref'),true);assert.deepEqual(highlights,[['ref']]);assert.equal(links.pickProjection('ref'),false);assert.equal(changes,0);
 await button(host,'Select projected spline from canvas').onclick();links.cancel();assert.equal(links.pickProjection('ref'),false);
 delete globalThis.document;delete globalThis.Option;
});
