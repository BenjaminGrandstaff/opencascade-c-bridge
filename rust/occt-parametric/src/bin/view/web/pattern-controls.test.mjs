import {test} from 'node:test';
import assert from 'node:assert/strict';
import {makePattern,patternGeometry,createPatternControls} from './pattern-controls.mjs';
import {captureGeneratedPoints,bindGeneratedPoints} from './generated-points.mjs';
const mm=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
const base=()=>({points:[{id:'a'},{id:'b'}],lines:[{id:'axis',start:'a',end:'b'}],circles:[{id:'hole',center:'b',rim:'a'}],patterns:[]});
test('pattern recipes validate native roles, counts, IDs and ordered generated sources',()=>{
 const d=base(),linear={id:'row',entities:['hole'],direction:'axis',count:3,spacing:mm(-20)};
 assert.deepEqual(makePattern(d,'linear',linear),{linear});assert.throws(()=>makePattern(d,'linear',{...linear,entities:['rect']}),/known source/);
 assert.deepEqual(makePattern(d,'circular',{id:'ring',entities:['hole'],center:'a',count:6}),{circular:{id:'ring',entities:['hole'],center:'a',count:6}});
 assert.deepEqual(makePattern(d,'mirror',{id:'mirror',entities:['hole'],axis:'axis',on_axis:['a']}),{mirror:{id:'mirror',entities:['hole'],axis:'axis',on_axis:['a']}});
 for(const fields of [{...linear,count:1},{...linear,count:2.5},{...linear,spacing:mm(0)},{...linear,entities:[]},{...linear,entities:['hole','hole']},{...linear,id:'hole'},{...linear,id:'bad:name'},{...linear,direction:'hole'},{...linear,entities:['axis']}])assert.throws(()=>makePattern(d,'linear',fields));
 d.patterns=[{linear},{linear:{...linear,id:'later'}}];bindGeneratedPoints(d,captureGeneratedPoints(d,['row:1:a','later:1:a'],{circles:['row:1:hole','later:1:hole']}));
 assert.deepEqual(patternGeometry(d,'curves',1),['axis','hole','row:1:hole']);
 assert.throws(()=>makePattern(d,'linear',{...linear,id:'row'}),/already/);
 assert.throws(()=>makePattern(d,'linear',{...linear,id:'rename'},0),/keep/);
 assert.throws(()=>makePattern(d,'linear',{...linear,entities:['row:1:hole']},0),/before/);
 assert.doesNotThrow(()=>makePattern(d,'linear',{...linear,id:'later',entities:['row:1:hole']},1));
 assert.throws(()=>makePattern(d,'linear',{...linear,id:'later',entities:['later:1:hole']},1),/before/);
});
class Element{
 constructor(tag){this.tagName=tag;this.children=[];this.value='';}
 append(...nodes){for(const n of nodes)n.parentElement=this;this.children.push(...nodes);}
 replaceChildren(...nodes){this.children=[];this.append(...nodes);}
 setAttribute(k,v){this[k]=v;}
 get selectedOptions(){return this.children.filter(c=>c.selected);}
}
const all=e=>[e,...e.children.flatMap(all)];
test('pattern forms add recipes, preserve formulas, expose full-turn defaults and remove selected patterns',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
  const d=base(),host=new Element('div');let changes=0;
  const control=createPatternControls({host,definition:d,changed:()=>changes++,message:()=>{}}),input=name=>all(host).find(e=>e['aria-label']===name),button=name=>all(host).find(e=>e.textContent===name&&e.tagName==='button');
  input('Pattern name').value='row';input('Pattern source curves').children.find(o=>o.value==='hole').selected=true;input('Pattern spacing (mm)').value='20';button('Add sketch pattern').onclick();
  assert.equal(changes,1);assert.equal(control.selected,'row');assert.deepEqual(d.patterns[0],{linear:{id:'row',entities:['hole'],direction:'axis',count:2,spacing:mm(20)}});
  assert(control.inspect({linear:{spacing:mm(20),count:2,direction:'axis',entities:['hole'],id:'row'}}));
  const formula={multiply:[{parameter:'pitch'},mm(2)]};d.patterns[0].linear.spacing=formula;assert.equal(control.inspect({linear:{id:'row',entities:['hole'],direction:'axis',count:2,spacing:mm(20)}}),false);
  const h=new Element('div');createPatternControls({host:h,definition:d,initialSelection:'row',changed:()=>changes++,message:()=>{}});
  const f=name=>all(h).find(e=>e['aria-label']===name);assert(f('Pattern name').readOnly);assert.equal(f('Pattern value driver').value,'preserve');assert(f('Pattern spacing (mm)').parentElement.hidden);
  f('Pattern count (including source)').value='4';all(h).find(e=>e.textContent==='Update sketch pattern').onclick();assert.deepEqual(d.patterns[0].linear.spacing,formula);assert.equal(d.patterns[0].linear.count,4);
  all(h).find(e=>e.textContent==='Remove sketch pattern').onclick();assert.deepEqual(d.patterns,[]);
  input('Pattern to edit').value='';input('Pattern to edit').onchange();
  input('Pattern type').value='circular';input('Pattern type').onchange();assert.equal(input('Pattern value driver').value,'default');
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('unavailable existing source choices remain selected and cannot be silently dropped on update',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
  const d=base();d.patterns=[{linear:{id:'earlier',entities:['hole'],direction:'axis',spacing:mm(10),count:3}},{linear:{id:'later',entities:['hole','earlier:2:hole'],direction:'axis',spacing:mm(10),count:2}}];
  bindGeneratedPoints(d,captureGeneratedPoints(d,[],{circles:['earlier:2:hole']}));d.patterns[0].linear.count=2;
  const host=new Element('div'),messages=[];let changes=0;createPatternControls({host,definition:d,initialSelection:'later',changed:()=>changes++,message:m=>messages.push(m)});
  const sources=all(host).find(e=>e['aria-label']==='Pattern source curves');assert.deepEqual(sources.selectedOptions.map(o=>o.value),['hole','earlier:2:hole']);
  all(host).find(e=>e.textContent==='Update sketch pattern').onclick();assert.equal(changes,0);assert.match(messages.at(-1),/known source/);assert.deepEqual(d.patterns[1].linear.entities,['hole','earlier:2:hole']);
 }finally{delete globalThis.document;delete globalThis.Option;}
});
