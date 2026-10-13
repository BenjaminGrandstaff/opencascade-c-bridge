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
