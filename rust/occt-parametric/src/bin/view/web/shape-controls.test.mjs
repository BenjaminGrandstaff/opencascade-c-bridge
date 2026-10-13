import {test} from 'node:test';
import assert from 'node:assert/strict';
import {editShapeParameter,shapeCurveIds,createShapeControls} from './shape-controls.mjs';
const mm=value=>({literal:{value,dimension:'length',unit:'millimeter'}}),scalar=value=>({literal:{value,dimension:'scalar',unit:null}});
test('shape parameter edits retain names, endpoint references and unit/formula expressions',()=>{
  const slot={slot:{id:'slot',start:'A',end:'B',radius:mm(2)}},formula={multiply:[{parameter:'wall'},scalar(2)]};
  const updated=editShapeParameter(slot,formula);assert.deepEqual(updated.slot,{id:'slot',start:'A',end:'B',radius:formula});assert.equal(slot.slot.radius.literal.value,2);
  assert.throws(()=>editShapeParameter(slot,mm(-1)),/positive/);assert.throws(()=>editShapeParameter(slot,scalar(3)),/units/);
  const polygon={polygon:{id:'hex',center:'C',vertex:'V',sides:6}};
  assert.equal(editShapeParameter(polygon,5).polygon.sides,5);assert.throws(()=>editShapeParameter(polygon,3.5),/integer/);
  const conic={conic:{id:'nose',start:'S',control:'T',end:'E'}};
  assert.deepEqual(editShapeParameter(conic,undefined),conic);assert.throws(()=>editShapeParameter(conic,scalar(1)),/strictly/);
  assert.equal(shapeCurveIds(polygon).length,7);assert(shapeCurveIds(slot).includes('slot:end_cap'));
});
class Element{constructor(tag){this.tagName=tag;this.children=[];this.value='';}append(...children){this.children.push(...children);}replaceChildren(...children){this.children=children;}setAttribute(k,v){this[k]=v;}}
const all=e=>[e,...e.children.flatMap(all)],field=(h,name)=>all(h).find(e=>e['aria-label']===name),click=(h,text)=>all(h).find(e=>e.tagName==='button'&&e.textContent===text).onclick();
const choose=(h,name,value)=>{const e=field(h,name);e.value=value;e.onchange();};
test('forms preserve unchanged formulas and units, filter compatible parameters and convert angles to radians',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.value=value;this.textContent=text;}};
 const formula={multiply:[{parameter:'wall'},scalar(2)]},d={shapes:[{slot:{id:'slot',start:'A',end:'B',radius:formula}},{rectangle:{id:'box',first:'A',opposite:'B',angle:scalar(Math.PI/2)}},{polygon:{id:'poly',center:'A',vertex:'B',sides:6}}]},host=new Element('div'),messages=[];let changes=0,controls;
 const render=()=>{const selection=controls?.selected;host.replaceChildren();controls=createShapeControls({host,definition:d,parameters:[{id:'wall',parameter_type:{scalar:'length'}},{id:'theta',parameter_type:{scalar:'scalar'}}],initialSelection:selection,changed:()=>{changes++;render();},message:m=>messages.push(m)});};render();
 assert(field(host,'Shape driving value').children.some(e=>e.value==='parameter:wall'));assert(!field(host,'Shape driving value').children.some(e=>e.value==='parameter:theta'));
 click(host,'Update shape parameter');assert.deepEqual(d.shapes[0].slot.radius,formula);choose(host,'Shape driving value','parameter:wall');click(host,'Update shape parameter');assert.deepEqual(d.shapes[0].slot.radius,{parameter:'wall'});
 choose(host,'Shape to edit','box');assert.equal(field(host,'Rectangle angle (degrees)').value,90);const angle=field(host,'Rectangle angle (degrees)');angle.value='45';angle.oninput();click(host,'Update shape parameter');assert.equal(d.shapes[1].rectangle.angle.literal.value,Math.PI/4);
 choose(host,'Shape driving value','default');click(host,'Update shape parameter');assert(!Object.hasOwn(d.shapes[1].rectangle,'angle'));
 choose(host,'Shape to edit','poly');const sides=field(host,'Polygon sides');sides.value='2';sides.oninput();click(host,'Update shape parameter');assert.equal(d.shapes[2].polygon.sides,6);assert.equal(changes,4);assert(messages.at(-1).includes('integer'));
 // Exact unit metadata stays authored until the numeric value is edited.
 d.shapes[0].slot.radius={literal:{value:1,dimension:'length',unit:'inch'}};render();choose(host,'Shape to edit','slot');assert.equal(field(host,'Slot radius (mm)').value,25.4);click(host,'Update shape parameter');assert.equal(d.shapes[0].slot.radius.literal.unit,'inch');
 delete globalThis.document;delete globalThis.Option;
});
test('canvas shape selection is armed, rejects unrelated curves and cancels without changing the draft',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.value=value;}};
 const d={shapes:[{slot:{id:'slot',start:'A',end:'B',radius:mm(2)}},{polygon:{id:'poly',center:'A',vertex:'B',sides:6}}]},original=structuredClone(d),host=new Element('div');
 const controls=createShapeControls({host,definition:d,changed(){},message(){}});assert.equal(controls.pick('poly:side2'),false);
 click(host,'Select shape from canvas');assert.equal(controls.pick('unrelated'),true);controls.pick('poly:side2');assert.equal(controls.selected,'poly');
 click(host,'Select shape from canvas');controls.cancelPick();assert.equal(controls.pick('slot:end_cap'),false);assert.deepEqual(d,original);
 delete globalThis.document;delete globalThis.Option;
});
