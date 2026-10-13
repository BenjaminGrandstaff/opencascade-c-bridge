import {test} from 'node:test';
import assert from 'node:assert/strict';
import {profileIds,makeProfileOperation,createProfileControls} from './profile-controls.mjs';
const mm=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
const scalar=value=>({literal:{value,dimension:'scalar',unit:null}});
const definition={points:[{id:'A'},{id:'B'}],lines:[{id:'leg'}],circles:[{id:'head'}],profile:['leg','head']};
test('native crossing trims, directed point bounds and signed offsets retain expressions',()=>{
  const from={crossing:{curve:'leg',near:'A'}},to={point:{point:'B'}};
  assert.deepEqual(makeProfileOperation('trim_between',{entity:'head',from,to}),{trim_between:{entity:'head',from,to}});
  assert.deepEqual(makeProfileOperation('trim',{entity:'leg',first:scalar(.2),last:{parameter:'end_fraction'}}).trim.last,{parameter:'end_fraction'});
  const formula={multiply:[{parameter:'wall'},scalar(-1)]};
  assert.deepEqual(makeProfileOperation('offset',{distance:formula,join:'intersection'}).offset.distance,formula);
  assert.doesNotThrow(()=>makeProfileOperation('offset',{distance:mm(-2),join:'arc'}));
  assert.throws(()=>makeProfileOperation('trim_between',{entity:'head',from,to:from}),/distinct/);
  assert.throws(()=>makeProfileOperation('trim_between',{entity:'head',from:'start',to:{crossing:{curve:'head',near:'A'}}}),/different/);
  assert.throws(()=>makeProfileOperation('trim_between',{entity:'head',from:'start',to:{crossing:{curve:'leg',near:''}}}),/hint/);
  assert.throws(()=>makeProfileOperation('trim',{entity:'leg',first:scalar(.8),last:scalar(.2)}),/fractions/);
  assert.throws(()=>makeProfileOperation('offset',{distance:mm(NaN),join:'arc'}),/finite/);
  assert.throws(()=>makeProfileOperation('offset',{distance:scalar(3),join:'arc'}),/units/);
});
test('trim selectors include stable projected crossings, shape boundaries and split pieces',()=>{
  const d={...definition,shapes:[{rectangle:{id:'box'}},{conic:{id:'conic'}}],projections:[{id:'ref',kind:'line',intersections:[{id:'cross'}]}],curve_operations:[{split:{before:'left',after:'right'}}]};
  assert(profileIds(d,'curves').includes('box:side3'));assert(profileIds(d,'curves').includes('conic'));assert(profileIds(d,'curves').includes('right'));
  assert(profileIds(d,'points').includes('ref:intersection:cross'));assert(profileIds(d,'points').includes('box:corner1'));
});
test('corner operations retain expressions and reject nonpositive or wrongly dimensioned distances',()=>{
  const formula={multiply:[{parameter:'radius'},scalar(.5)]};
  assert.deepEqual(makeProfileOperation('fillet',{point:'A',radius:formula}),{fillet:{point:'A',radius:formula}});
  assert.deepEqual(makeProfileOperation('chamfer',{point:'B',incoming:mm(2),outgoing:mm(3)}),{chamfer:{point:'B',incoming:mm(2),outgoing:mm(3)}});
  assert.throws(()=>makeProfileOperation('fillet',{point:'A',radius:mm(0)}),/positive/);
  assert.throws(()=>makeProfileOperation('chamfer',{point:'B',incoming:mm(2),outgoing:scalar(3)}),/units/);
});
class Element{
  constructor(tag){this.tagName=tag;this.children=[];this.value='';}
  append(...children){this.children.push(...children);}replaceChildren(...children){this.children=children;}setAttribute(k,v){this[k]=v;}
}
const all=e=>[e,...e.children.flatMap(all)];
const field=(host,name)=>all(host).find(e=>e['aria-label']===name);
const click=(host,text)=>all(host).find(e=>e.tagName==='button'&&e.textContent===text).onclick();
const choose=(host,label,value)=>{const e=field(host,label);e.value=value;e.onchange();};
test('forms keep formulas, author exact crossings before boundary edits, and remove only chosen edits',()=>{
  globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
  const formula={multiply:[{parameter:'wall'},scalar(2)]},d=structuredClone(definition);d.profile_operations=[{offset:{distance:formula,join:'arc'}}];
  const host=new Element('div'),messages=[];let changes=0;
  const render=()=>{host.replaceChildren();createProfileControls({host,definition:d,parameters:[{id:'wall',parameter_type:{scalar:'length'}}],changed:()=>{changes++;render();},message:m=>messages.push(m)});};render();
  click(host,'1. offset · outer profile');click(host,'Update profile operation');assert.deepEqual(d.profile_operations[0].offset.distance,formula);
  choose(host,'Trim curve','head');choose(host,'From bound','crossing');choose(host,'From crossing curve','leg');choose(host,'From hint point','A');choose(host,'To bound','point');choose(host,'To point','B');
  click(host,'Add profile operation');assert.equal(d.profile_operations[0].trim_between.from.crossing.near,'A');assert.deepEqual(d.profile_operations[1].offset.distance,formula);
  click(host,'2. offset · outer profile');choose(host,'Offset distance (mm) source','parameter:wall');click(host,'Update profile operation');assert.deepEqual(d.profile_operations[1].offset.distance,{parameter:'wall'});
  click(host,'Remove profile operation 1');assert.equal(d.profile_operations.length,1);assert.equal(changes,4);assert.equal(d.points.length,2);
  delete globalThis.document;delete globalThis.Option;
});
test('armed canvas picks fill only the requested field, cancel safely, and insert corners before offsets',()=>{
  globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
  const d=structuredClone(definition),host=new Element('div');d.profile_operations=[{offset:{distance:mm(2),join:'arc'}}];let changes=0;
  const controls=createProfileControls({host,definition:d,changed:()=>changes++,message:()=>{}});
  assert.equal(controls.pick('head'),false);
  click(host,'Pick trim curve on canvas');assert.equal(controls.pick('A'),true);assert.equal(field(host,'Trim curve').value,'leg');
  controls.pick('head');assert.equal(field(host,'Trim curve').value,'head');assert.equal(controls.pick('leg'),false);
  choose(host,'From bound','crossing');click(host,'Pick from crossing curve on canvas');controls.pick('head');assert.equal(field(host,'From crossing curve').value,'');controls.pick('leg');
  click(host,'Pick from hint point on canvas');controls.pick('A');assert.equal(field(host,'From hint point').value,'A');
  choose(host,'To bound','point');click(host,'Pick to point on canvas');controls.cancelPick();assert.equal(controls.pick('B'),false);assert.equal(field(host,'To point').value,'');
  click(host,'Pick to point on canvas');controls.pick('B');click(host,'Add profile operation');assert.equal(d.profile_operations[0].trim_between.to.point.point,'B');
  choose(host,'Profile operation','chamfer');click(host,'Pick corner point on canvas');controls.pick('B');
  const incoming=field(host,'Incoming distance (mm)');incoming.value='2';incoming.oninput();click(host,'Add profile operation');
  assert.equal(d.profile_operations[1].chamfer.incoming.literal.value,2);assert(d.profile_operations[2].offset);assert.equal(changes,2);
  delete globalThis.document;delete globalThis.Option;
});
