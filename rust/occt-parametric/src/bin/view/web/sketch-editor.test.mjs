import {test} from 'node:test';
import assert from 'node:assert/strict';
import {addElement, length, createSketchEditor} from './sketch-editor.mjs';

class Element {
  constructor(tag) { this.tagName=tag;this.children=[];this.handlers={};this.dataset={};this.style={};this.value=''; }
  append(...nodes) {this.children.push(...nodes);}
  prepend(...nodes) {this.children.unshift(...nodes);}
  replaceChildren(...nodes) {this.children=nodes;}
  setAttribute(name,value) {this[name]=value;}
  addEventListener(name, fn) {this.handlers[name]=fn;}
  emit(name) {return this.handlers[name]?.();}
}
const all = element => [element, ...element.children.flatMap(all)];
const find = (host, text) => all(host).find(e => e.textContent === text);

test('primitive names cannot collide with points or other curve kinds', () => {
  const draft={points:[{id:'A'}],lines:[{id:'AB'}]};
  assert.throws(()=>addElement(draft,'circles',{id:'AB'}), /already exists/);
  assert.throws(()=>addElement(draft,'points',{id:' '}), /name/);
  const point={id:'B',x:length(5),y:length(0),fixed:true};
  addElement(draft,'points',point); point.x.literal.value=100;
  assert.equal(draft.points[1].x.literal.value,5);
});

test('coordinate edits preview a private guarded draft and remain available after rejected apply', async () => {
  globalThis.document={createElement:tag=>new Element(tag)};
  globalThis.Option=class extends Element {constructor(text,value){super('option');this.textContent=text;this.value=value;}};
  const source={id:'sketch',points:[{id:'A',x:length(0),y:length(0),fixed:true}],constraints:[],profile:[]};
  const host=new Element('section'), calls=[], previews=[];
  let accepted=null, reject=true, shown=null;
  createSketchEditor({host,instance:()=> 'part',showSketch:id=>shown=id,
    preview:async view=>previews.push(view), applied:async state=>accepted=state,restore:async()=>{},
    request:async(path,options)=>{
      if(path==='/api/sketches')return {version:4,sketches:[{feature:'profile',definition:source}]};
      const body=JSON.parse(options.body);calls.push(body);
      if(body.apply&&reject)throw new Error('Model changed. Reload the sketch.');
      return body.apply?{state:{version:5}}:{can_apply:true,annotations:{preview:true}};
    }});
  await find(host,'Edit sketch geometry').emit('click');
  const choice=all(host).find(e=>e['aria-label']==='Sketch to edit');choice.value='profile';
  await find(host,'Load sketch draft').emit('click');assert.equal(shown,'profile');
  const coordinate=find(host,'X (mm)').children[0];coordinate.value='7';coordinate.emit('change');
  await find(host,'Preview draft').emit('click');
  assert.equal(source.points[0].x.literal.value,0);
  assert.equal(calls[0].expected.points[0].x.literal.value,0);
  assert.equal(calls[0].definition.points[0].x.literal.value,7);
  assert.equal(calls[0].version,4);assert.equal(previews.length,1);assert.equal(accepted,null);
  await find(host,'Apply verified draft').emit('click');
  assert(find(host,'Model changed. Reload the sketch.'));
  assert(find(host,'Apply verified draft'));assert.equal(accepted,null);
  reject=false;await find(host,'Apply verified draft').emit('click');assert.deepEqual(accepted,{version:5});
  assert(!find(host,'Apply verified draft'));
  delete globalThis.document;delete globalThis.Option;
});

test('canceling an in-flight drag discards its late result and preserves the local draft', async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};
 globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 const source={id:'s',points:[{id:'A',x:length(0),y:length(0),fixed:false}],constraints:[],profile:[]};
 const host=new Element('div'),calls=[];let release;
 const editor=createSketchEditor({host,instance:()=> 'part',preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{
  if(path==='/api/sketches')return {version:1,sketches:[{feature:'profile',definition:source}]};
  const body=JSON.parse(options.body);calls.push(body);
  if(path==='/api/sketch-drag')return new Promise(resolve=>release=resolve);
  return {can_apply:true,annotations:{scenes:[]}};
 }});
 await find(host,'Edit sketch geometry').emit('click');all(host).find(e=>e['aria-label']==='Sketch to edit').value='profile';await find(host,'Load sketch draft').emit('click');
 const tool=all(host).find(e=>e['aria-label']==='Canvas tool');tool.value='drag';tool.onchange();
 const event={scene:{feature:'profile',points:{A:[0,0,0]}},point:'A',position:[0,0],tolerance:0};
 editor.canvasDown(event);const pending=editor.canvasUp({...event,position:[10,0]});
 assert.equal(calls[0].point,'A');editor.canvasCancel();
 release({definition:{...source,points:[{...source.points[0],x:length(10)}]},annotations:{scenes:[]}});await pending;
 await find(host,'Preview draft').emit('click');assert.equal(calls[1].definition.points[0].x.literal.value,0);
 delete globalThis.document;delete globalThis.Option;
});

test('projection review and canvas dependency picks leave the sketch and constraint selection untouched',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};
 globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const source={id:'s',points:[{id:'A',x:length(0),y:length(0),fixed:true}],profile:['ref'],projections:[{id:'ref',kind:'spline',input:'source'}]},original=structuredClone(source),host=new Element('div'),calls=[],highlighted=[];
 const editor=createSketchEditor({host,instance:()=> 'part',preview:async()=>{},applied:async()=>{},restore:async()=>{},highlight:ids=>highlighted.push(ids),request:async(path)=>{
  calls.push(path);return {version:1,sketches:[{feature:'profile',definition:source}],features:[{id:'source',operation:{box:{}}},{id:'profile',operation:{sketch_face:{sketch:source}}},{id:'body',operation:{extrude:{input:'profile'}}}]};
 }});
 await find(host,'Edit sketch geometry').emit('click');all(host).find(e=>e['aria-label']==='Sketch to edit').value='profile';await find(host,'Load sketch draft').emit('click');
 find(host,'Review dependencies ref').onclick();assert.deepEqual(highlighted.at(-1),['ref']);assert.equal(all(host).find(e=>e['aria-label']==='Inspect geometry dependencies').value,'ref');
 find(host,'Inspect dependencies from canvas').onclick();assert.equal(editor.selectGeometry('ref:start',{feature:'profile'}),true);
 assert.equal(all(host).find(e=>e['aria-label']==='Inspect geometry dependencies').value,'ref:start');assert(find(host,'Click sketch points or curves to select geometry. Selection order determines first and second.'));
 find(host,'Inspect dependencies from canvas').onclick();editor.cancelLinkPick();assert.equal(all(host).find(e=>e['aria-label']==='Inspect geometry dependencies').value,'ref:start');
 assert.deepEqual(calls,['/api/sketches']);assert.deepEqual(source,original);
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('crossing review clears edited windows, verifies previews and refuses unrelated native snapshots',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{const constraint={crossing_point:{point:'q',first:'circle',second:'guide',near:{parameter:'hint'},maximum_distance:{parameter:'radius'}}},source={id:'s',points:[{id:'a',x:length(0),y:length(0),fixed:true},{id:'q',x:length(8),y:length(5),fixed:false}],constraints:[constraint],profile:[]};
 const scene=()=>({feature:'profile',annotations:[{status:'passed',parameters:['hint','radius'],detail:{constraint,crossing_window:{center:[8,5,0],radius:3,distance:0.6}}}]});let native=scene(),isPreview=false;const shown=[],host=new Element('div');
 createSketchEditor({host,instance:()=> 'part',crossingWindows:w=>shown.push(w),reviewScene:(_,__,acceptedOnly)=>acceptedOnly&&isPreview?null:native,preview:async data=>{native=data.scenes[0];isPreview=true;},applied:async()=>{},restore:async()=>{},request:async path=>path==='/api/sketches'?{version:1,sketches:[{feature:'profile',definition:source}]}:{can_apply:true,annotations:{scenes:[scene()]}}});
 await find(host,'Edit sketch geometry').emit('click');all(host).find(e=>e['aria-label']==='Sketch to edit').value='profile';await find(host,'Load sketch draft').emit('click');
 let show=find(host,'Show matching crossing windows');assert(!show.disabled);show.onclick();assert.equal(shown.at(-1).length,1);
 const coordinate=find(host,'X (mm)').children[0];coordinate.value='2';coordinate.emit('change');assert.deepEqual(shown.at(-1),[]);assert(find(host,'Show matching crossing windows').disabled);
 await find(host,'Preview draft').emit('click');show=find(host,'Show matching crossing windows');assert(!show.disabled);show.onclick();assert.equal(shown.at(-1).length,1);
 native=scene();show.onclick();assert.deepEqual(shown.at(-1),[]);assert(find(host,'Show matching crossing windows').disabled);
 await find(host,'Discard draft').emit('click');assert.deepEqual(shown.at(-1),[]);
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('canvas crossing picks route through the sketch editor, stage locally and cancel on tool changes',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{const source={id:'s',points:[{id:'a',x:length(-10),y:length(0),fixed:true},{id:'b',x:length(10),y:length(0),fixed:true},{id:'c',x:length(0),y:length(-10),fixed:true},{id:'d',x:length(0),y:length(10),fixed:true}],lines:[{id:'h',start:'a',end:'b'},{id:'v',start:'c',end:'d'}],constraints:[],profile:[]},host=new Element('div'),calls=[],modes=[],queries=[],scene={feature:'profile',instance:'part',points:{}};
 const editor=createSketchEditor({host,instance:()=> 'part',reviewScene:()=>scene,canvasMode:m=>modes.push(m),preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,opts)=>{if(path==='/api/sketches')return {version:4,sketches:[{feature:'profile',definition:source}]};if(path==='/api/sketch-crossing-candidates'){const body=JSON.parse(opts.body);queries.push(body);return {version:4,instance:'part',feature:'profile',complete:true,first:body.first,second:body.second,coordinate_system:'sketch-local millimeters',points:[[0,0]]};}calls.push(JSON.parse(opts.body));return {can_apply:true,annotations:{scenes:[]}};}});
 await find(host,'Edit sketch geometry').emit('click');all(host).find(e=>e['aria-label']==='Sketch to edit').value='profile';await find(host,'Load sketch draft').emit('click');find(host,'Start crossing pick').onclick();assert.equal(modes.at(-1),'crossing');
 for(const entity of ['h','v',undefined]){const e={scene,entity,version:4,position:[0,0],tolerance:1};assert(editor.canvasDown(e));await editor.canvasUp(e);await new Promise(resolve=>setImmediate(resolve));}assert.equal(calls.length,0);assert.equal(queries.length,1);assert.equal(source.points.length,4);
 await find(host,'Preview draft').emit('click');assert.equal(calls[0].definition.points.at(-1).id,'Crossing1');assert.equal(calls[0].definition.constraints[0].crossing_point.first,'h');assert.equal(calls[0].expected.points.length,4);
 find(host,'Start crossing pick').onclick();const tool=all(host).find(e=>e['aria-label']==='Canvas tool');tool.value='line';tool.onchange();assert.equal(modes.at(-1),'line');editor.canvasCancel();await find(host,'Preview draft').emit('click');assert.equal(calls[1].definition.points.length,5);
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('editor loads generated point metadata and refreshes stale constraint choices only after Preview',async()=>{
 globalThis.document={createElement:tag=>new Element(tag)};
 globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
  const source={id:'s',points:[{id:'a',x:length(0),y:length(0),fixed:true},{id:'c',x:length(10),y:length(5),fixed:true}],lines:[],shapes:[{rectangle:{id:'rect',first:'a',opposite:'c'}}]};
  const host=new Element('div'),calls=[];
  createSketchEditor({host,instance:()=> 'part',preview:async()=>{},applied:async()=>{},restore:async()=>{},request:async(path,options)=>{
   if(path==='/api/sketches')return {version:1,sketches:[{feature:'profile',definition:source,generated_points:['rect:corner1','rect:corner3'],generated_curves:{lines:['rect:side0']}}]};
   calls.push(JSON.parse(options.body));return {can_apply:true,annotations:{},generated_points:['fresh:corner1'],generated_curves:{arcs:['fresh:cap']}};
  }});
  await find(host,'Edit sketch geometry').emit('click');
  all(host).find(e=>e['aria-label']==='Sketch to edit').value='profile';
  await find(host,'Load sketch draft').emit('click');
  const choosePointTool=()=>{const tool=all(host).find(e=>e['aria-label']==='Constraint type');tool.value='distance';tool.onchange();};
  const has=id=>all(host).some(e=>e.tagName==='option'&&e.value===id);
  choosePointTool();assert(has('rect:corner1'));
  const curveTool=all(host).find(e=>e['aria-label']==='Constraint type');curveTool.value='curve_length';curveTool.onchange();assert(has('rect:side0'));
  const input=find(host,'Sketch definition').children[0],next=structuredClone(source);next.shapes[0].rectangle.angle={literal:{value:.1,dimension:'scalar',unit:null}};
  input.value=JSON.stringify(next);input.emit('change');choosePointTool();assert(!has('rect:corner1'));assert(!has('rect:side0'));
  await find(host,'Preview draft').emit('click');choosePointTool();assert(has('fresh:corner1'));
  const radius=all(host).find(e=>e['aria-label']==='Constraint type');radius.value='radius';radius.onchange();assert(has('fresh:cap'));
  assert(!JSON.stringify(calls[0].definition).includes('generated_points'));assert.deepEqual(source.shapes,[{rectangle:{id:'rect',first:'a',opposite:'c'}}]);
 }finally{delete globalThis.document;delete globalThis.Option;}
});
