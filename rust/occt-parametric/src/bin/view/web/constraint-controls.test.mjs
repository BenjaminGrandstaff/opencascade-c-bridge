import {test} from 'node:test';
import assert from 'node:assert/strict';
import {geometryIds,makeConstraint,dimensionNumber,authoredConstraintIndex,createConstraintControls} from './constraint-controls.mjs';
const definition={points:[{id:'A'},{id:'B'}],lines:[{id:'AB'},{id:'CD'}],circles:[{id:'C1'},{id:'C2'}],arcs:[{id:'arc'}],splines:[{id:'spline'}],constraints:[]};
const mm=value=>({literal:{value,dimension:'length',unit:'millimeter'}});

test('typed dimensions and relationships use native entity roles and units',()=>{
  assert.deepEqual(makeConstraint(definition,'horizontal',{line:'AB'}),{horizontal:{line:'AB'}});
  assert.deepEqual(makeConstraint(definition,'distance',{first:'A',second:'B'},mm(12)),{distance:{first:'A',second:'B',value:mm(12)}});
  assert.deepEqual(makeConstraint(definition,'equal_length',{first:'AB',second:'CD'}),{equal_length:{first:'AB',second:'CD'}});
  assert.deepEqual(makeConstraint(definition,'line_circle_tangent',{line:'AB',circle:'C1',side:'left'}),{line_circle_tangent:{line:'AB',circle:'C1',side:'left'}});
  assert.deepEqual(makeConstraint(definition,'circle_circle_tangent',{first:'C1',second:'C2',mode:'internal'}),{circle_circle_tangent:{first:'C1',second:'C2',mode:'internal'}});
  assert.equal(dimensionNumber({literal:{value:2,dimension:'length',unit:'inch'}},'length'),50.8);
  assert.equal(dimensionNumber({literal:{value:Math.PI/2,dimension:'scalar',unit:null}},'angle'),90);
  assert.throws(()=>makeConstraint(definition,'horizontal',{line:'A'}),/known/);
  assert.throws(()=>makeConstraint(definition,'coincident',{first:'A',second:'A'}),/distinct/);
  assert.throws(()=>makeConstraint(definition,'radius',{curve:'AB'},mm(4)),/known/);
  assert.throws(()=>makeConstraint(definition,'radius',{curve:'C1'},mm(-1)),/positive/);
  assert.throws(()=>makeConstraint(definition,'distance',{first:'A',second:'B'},mm(NaN)),/finite/);
  assert.doesNotThrow(()=>makeConstraint(definition,'horizontal_distance',{first:'A',second:'B'},mm(-4)));
});

test('annotation editing matches authored constraints after removals and refuses generated or ambiguous matches',()=>{
  const first={horizontal:{line:'AB'}},second={distance:{first:'A',second:'B',value:mm(5)}};
  const d={constraints:[first,second]};
  const annotation={detail:{constraint_index:1,constraint:{distance:{value:mm(5),second:'B',first:'A'}}}};
  assert.equal(authoredConstraintIndex(d,annotation),1);
  d.constraints.shift();assert.equal(authoredConstraintIndex(d,annotation),0);
  assert.equal(authoredConstraintIndex(d,{detail:{constraint_index:0,constraint:{vertical:{line:'generated:side'}}}}),-1);
  d.constraints.push(second);assert.equal(authoredConstraintIndex(d,{detail:{constraint:second}}),-1);
});

class Element {
  constructor(tag){this.tagName=tag;this.children=[];this.style={};this.value='';}
  append(...children){this.children.push(...children);}replaceChildren(...children){this.children=children;}setAttribute(name,value){this[name]=value;}
}
const all=e=>[e,...e.children.flatMap(all)];
const label=(host,text)=>all(host).find(e=>e.tagName==='label'&&e.textContent===text).children[0];
const button=(host,text)=>all(host).find(e=>e.tagName==='button'&&e.textContent===text);

test('canvas picks feed typed forms; dimensions edit in degrees and formulas stay intact',()=>{
  globalThis.document={createElement:tag=>new Element(tag)};
  globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
  const d=structuredClone(definition),host=new Element('div'),highlights=[],messages=[];let changes=0;
  d.constraints=[{line_angle:{line:'AB',value:{literal:{value:Math.PI/2,dimension:'scalar',unit:null}}}},{distance:{first:'A',second:'B',value:{multiply:[{parameter:'width'},mm(2)]}}}];
  const controls=createConstraintControls({host,definition:d,parameters:[{id:'width',parameter_type:{scalar:'length'}},{id:'angle',parameter_type:{scalar:'scalar'}}],changed:()=>changes++,message:m=>messages.push(m),highlight:ids=>highlights.push(ids)});
  const select=all(host).find(e=>e['aria-label']==='Constraint type');select.value='coincident';select.onchange();
  controls.pick('B');controls.pick('A');
  assert.equal(label(host,'First').value,'B');assert.equal(label(host,'Second').value,'A');
  button(host,'Add dimension / constraint').onclick();assert.deepEqual(d.constraints[2],{coincident:{first:'B',second:'A'}});
  assert.equal(controls.pick('generated:side'),false);assert.equal(highlights.at(-1).length,2);
  controls.selectAnnotation({detail:{constraint_index:0,constraint:d.constraints[0]}});
  assert.deepEqual(controls.selection,['AB']);
  const degrees=label(host,'Value (degrees)');assert.equal(degrees.value,90);degrees.value='45';degrees.oninput();
  button(host,'Update selected constraint').onclick();assert.equal(d.constraints[0].line_angle.value.literal.value,Math.PI/4);
  controls.selectAnnotation({detail:{constraint_index:1,constraint:d.constraints[1]}});
  const formula=structuredClone(d.constraints[1].distance.value);
  button(host,'Update selected constraint').onclick();assert.deepEqual(d.constraints[1].distance.value,formula);
  const source=label(host,'Driving value');source.value='parameter:width';source.onchange();button(host,'Update selected constraint').onclick();
  assert.deepEqual(d.constraints[1].distance.value,{parameter:'width'});assert.equal(changes,4);
  delete globalThis.document;delete globalThis.Option;
});

test('projected named junctions and intersections are available to typed point constraints',()=>{
 const d={points:[{id:'authored'}],projections:[{id:'reference',kind:'chain',junctions:[{id:'mid'}],intersections:[{id:'cut'}]}]};
 assert.deepEqual(geometryIds(d,'points'),['authored','reference:start','reference:end','reference:junction:mid','reference:intersection:cut']);
 assert.deepEqual(makeConstraint(d,'distance',{first:'reference:junction:mid',second:'reference:intersection:cut'},mm(5)),{distance:{first:'reference:junction:mid',second:'reference:intersection:cut',value:mm(5)}});
});

test('reference dimensions validate geometry and names without introducing drivers',async()=>{
 const {makeMeasurement,authoredMeasurementIndex}=await import('./constraint-controls.mjs');
 const {captureGeneratedPoints,bindGeneratedPoints}=await import('./generated-points.mjs');
 const d=structuredClone(definition);d.shapes=[{rectangle:{id:'rect',first:'A',opposite:'B'}}];
 bindGeneratedPoints(d,captureGeneratedPoints(d,['rect:corner1'],{lines:['rect:side0']}));
 for(const [type,fields] of Object.entries({distance:{first:'A',second:'rect:corner1'},horizontal_distance:{first:'B',second:'A'},vertical_distance:{first:'A',second:'B'},point_line_distance:{point:'A',line:'rect:side0'},angle:{first:'AB',second:'rect:side0'},line_angle:{line:'AB'},arc_angle:{arc:'arc'},radius:{curve:'C1'},diameter:{curve:'arc'},curve_length:{curve:'spline'}})){
  const m=makeMeasurement(d,'readout',type,{...fields,value:mm(999)});
  assert.deepEqual(m,{id:'readout',measure:{[type]:fields}});
 }
 assert.throws(()=>makeMeasurement(d,'readout','horizontal',{line:'AB'}),/supported/);
 for(const id of ['',' spaced','A','rect','rect:corner1','rect:side0'])assert.throws(()=>makeMeasurement(d,id,'line_angle',{line:'AB'}));
 d.measurements=[makeMeasurement(d,'readout','line_angle',{line:'AB'})];
 assert.throws(()=>makeMeasurement(d,'readout','line_angle',{line:'AB'}),/already/);
 assert.doesNotThrow(()=>makeMeasurement(d,'readout','line_angle',{line:'AB'},0));
 const old={detail:{measurement:structuredClone(d.measurements[0])}};
 assert.equal(authoredMeasurementIndex(d,old),0);
 d.measurements[0].measure={line_angle:{line:'CD'}};assert.equal(authoredMeasurementIndex(d,old),-1);
 d.shapes[0].rectangle.opposite='A';assert.throws(()=>makeMeasurement(d,'lost','curve_length',{curve:'rect:side0'}),/known/);
 assert.throws(()=>makeMeasurement(d,'bad','distance',{first:'A',second:'A'}),/distinct/);
 assert.deepEqual(d.constraints,[]);
});

test('reference form supports canvas picks, annotation editing, mode switching and removal',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};
 globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
  const d=structuredClone(definition),host=new Element('div'),messages=[];let changes=0;
  const controls=createConstraintControls({host,definition:d,changed:()=>changes++,message:m=>messages.push(m)});
  const behavior=all(host).find(e=>e['aria-label']==='Dimension behavior');behavior.value='reference';behavior.onchange();
  assert.equal(controls.reference,true);assert.equal(controls.type,'distance');
  assert(!all(host).some(e=>e.textContent==='Driving value'));
  controls.pick('B');controls.pick('A');
  let name=label(host,'Reference name');name.value='span';name.oninput();
  button(host,'Add reference dimension').onclick();assert.equal(changes,1);
  assert.deepEqual(d.measurements,[{id:'span',measure:{distance:{first:'B',second:'A'}}}]);assert.deepEqual(d.constraints,[]);
  assert(controls.selectAnnotation({detail:{measurement:structuredClone(d.measurements[0])}}));
  assert.equal(label(host,'Reference name').value,'span');
  name=label(host,'Reference name');name.value='renamed';name.oninput();button(host,'Update reference dimension').onclick();assert.equal(d.measurements[0].id,'renamed');
  const type=all(host).find(e=>e['aria-label']==='Constraint type');type.value='horizontal_distance';type.onchange();
  assert.equal(label(host,'Reference name').value,'renamed');button(host,'Update reference dimension').onclick();
  assert.equal(d.measurements.length,1);assert.deepEqual(d.measurements[0].measure,{horizontal_distance:{first:'B',second:'A'}});
  behavior.value='driving';behavior.onchange();assert.equal(controls.reference,false);assert(label(host,'Driving value'));
  const host2=new Element('div');createConstraintControls({host:host2,definition:d,initialReference:true,initialType:'curve_length',changed:()=>changes++,message:m=>messages.push(m)});
  button(host2,'Remove reference renamed').onclick();assert.deepEqual(d.measurements,[]);assert.equal(changes,4);
 }finally{delete globalThis.document;delete globalThis.Option;}
});
