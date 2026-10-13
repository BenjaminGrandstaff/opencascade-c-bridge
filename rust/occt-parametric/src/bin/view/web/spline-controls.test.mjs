import {test} from 'node:test';
import assert from 'node:assert/strict';
import {editSpline,editSplineBasis,resizeSplinePoles,createSplineControls} from './spline-controls.mjs';
const scalar=value=>({literal:{value,dimension:'scalar',unit:null}}),basis={degree:2,knots:[0,1],multiplicities:[3,3]},explicit={id:'curve',points:['a','b','c'],basis:{...basis,weights:[scalar(1),{parameter:'weight'},scalar(1)]}},known=['a','b','c','d','start','end'];
test('spline edits preserve basis, identity and endpoints while validating weights and closure',()=>{
 const weights=[scalar(1),{multiply:[{parameter:'weight'},scalar(2)]},scalar(1)],next=editSpline(explicit,{points:['a','d','c'],weights},known);
 assert.equal(next.id,'curve');assert.deepEqual(next.basis.knots,[0,1]);assert.deepEqual(next.basis.weights,weights);assert.equal(explicit.points[1],'b');
 assert.throws(()=>editSpline(explicit,{points:['a','b'],weights},known),/pole count/);
 assert.throws(()=>editSpline(explicit,{points:['a','b','c'],weights:[scalar(1),scalar(0),scalar(1)]},known),/positive/);
 assert.throws(()=>editSpline({...explicit,endpoints:{start:'start',end:'end'}},{points:['a','start','c']},known),/separate/);
 assert.deepEqual(editSpline({id:'s',points:['a','b','c','a']},{points:['a','b','c'],closed:false},known).points,['a','b','c']);
 assert.deepEqual(editSpline({id:'s',points:['a','b','c']},{points:['a','d','c'],closed:true},known).points,['a','d','c','a']);
 assert.throws(()=>editSpline({id:'s',points:['a','b']},{points:['a','a']},known),/distinct/);
});
test('basis edits validate clamped, periodic and non-clamped contracts without replacing weight expressions',()=>{
 const next=editSplineBasis(explicit,{degree:1,knots:[0,.5,1],multiplicities:[2,1,2],mode:'clamped'},known);
 assert.equal(next.basis.degree,1);assert.deepEqual(next.basis.weights,explicit.basis.weights);assert.deepEqual(next.points,explicit.points);
 assert.throws(()=>editSplineBasis(explicit,{degree:3,knots:[0,1],multiplicities:[4,4],mode:'clamped'},known),/degree \+ 1/);
 assert.throws(()=>editSplineBasis(explicit,{degree:2,knots:[0,0],multiplicities:[3,3],mode:'clamped'},known),/increasing/);
 assert.throws(()=>editSplineBasis(explicit,{degree:2,knots:[0,.5,1],multiplicities:[3,1,3],mode:'clamped'},known),/sum/);
 const nonClamped=editSplineBasis(explicit,{degree:2,knots:[0,1,2,3,4,5],multiplicities:[1,1,1,1,1,1],mode:'non_clamped',endpoints:{start:'start',end:'end'}},known);
 assert.equal(nonClamped.basis.non_clamped,true);assert.deepEqual(nonClamped.endpoints,{start:'start',end:'end'});
 assert.throws(()=>editSplineBasis(explicit,{degree:2,knots:[0,1,2,3,4,5],multiplicities:[1,1,1,1,1,1],mode:'non_clamped',endpoints:{start:'a',end:'end'}},known),/distinct/);
 const periodic=editSplineBasis({...explicit,points:['a','b','c','d'],basis:{...basis,weights:[]}},{degree:3,knots:[0,1,2,3,4],multiplicities:[1,1,1,1,1],mode:'periodic'},known);assert.equal(periodic.basis.periodic,true);
});
class Element{constructor(tag){this.tagName=tag;this.children=[];this.value='';}append(...children){this.children.push(...children);}replaceChildren(...children){this.children=children;}setAttribute(k,v){this[k]=v;}}
const all=e=>[e,...e.children.flatMap(all)],field=(h,name)=>all(h).find(e=>e['aria-label']===name),click=(h,text)=>all(h).find(e=>e.tagName==='button'&&e.textContent===text).onclick();
test('weight forms preserve formulas and implicit unit weights; canvas picks and point focus remain explicit',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 const formula={multiply:[{parameter:'weight'},scalar(2)]},d={points:known.map(id=>({id})),splines:[structuredClone(explicit)]};d.splines[0].basis.weights[1]=formula;
 const host=new Element('div'),focused=[];let controls,changes=0;const render=()=>{const selected=controls?.selected;host.replaceChildren();controls=createSplineControls({host,definition:d,parameters:[{id:'weight',parameter_type:{scalar:'scalar'}}],initialSelection:selected,changed:()=>{changes++;render();},message(){},focusPoint:id=>focused.push(id)});};render();
 click(host,'Update spline definition');assert.deepEqual(d.splines[0].basis.weights[1],formula);
 const driver=field(host,'Weight 2 source');driver.value='literal';driver.onchange();const value=field(host,'Weight 2');value.value='2';value.oninput();click(host,'Update spline definition');assert.equal(d.splines[0].basis.weights[1].literal.value,2);
 click(host,'Edit point 2: b');assert.deepEqual(focused,['b']);assert.equal(controls.pick('curve'),false);click(host,'Select spline from canvas');controls.pick('curve');assert.equal(controls.selected,'curve');
 click(host,'Use unit weights');assert.deepEqual(d.splines[0].basis.weights,[]);click(host,'Update spline definition');assert.deepEqual(d.splines[0].basis.weights,[]);assert.equal(changes,4);
 delete globalThis.document;delete globalThis.Option;
});
test('basis form applies degree and knot edits atomically with weights, and retains the draft after invalid input',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.value=value;}};
 const d={points:known.map(id=>({id})),splines:[structuredClone(explicit)]},host=new Element('div'),messages=[];let changes=0;
 createSplineControls({host,definition:d,changed:()=>changes++,message:m=>messages.push(m)});
 field(host,'Spline degree').value='1';field(host,'Unique knots (comma separated)').value='0, .5, 1';field(host,'Knot multiplicities (comma separated)').value='2, 1, 2';click(host,'Update spline definition');assert.equal(d.splines[0].basis.degree,1);assert.deepEqual(d.splines[0].basis.weights,explicit.basis.weights);
 const fresh=new Element('div');createSplineControls({host:fresh,definition:d,changed:()=>changes++,message:m=>messages.push(m)});field(fresh,'Unique knots (comma separated)').value='0, , 1';click(fresh,'Update spline definition');assert.equal(changes,1);assert(messages.at(-1).includes('increasing'));
 delete globalThis.document;delete globalThis.Option;
});

test('pole insertion and removal preserve drivers and rebuild all basis modes over the active domain',()=>{
 const inserted=resizeSplinePoles(explicit,{action:'insert',index:1,point:'d'},known);
 assert.deepEqual(inserted.points,['a','d','b','c']);assert.deepEqual(inserted.basis.knots,[0,.5,1]);assert.deepEqual(inserted.basis.multiplicities,[3,1,3]);
 assert.deepEqual(inserted.basis.weights,[scalar(1),scalar(1),{parameter:'weight'},scalar(1)]);
 assert.deepEqual(resizeSplinePoles(inserted,{action:'remove',index:1},known),explicit);
 assert.throws(()=>resizeSplinePoles(explicit,{action:'remove',index:1},known),/degree \+ 1/);
 assert.throws(()=>resizeSplinePoles(explicit,{action:'insert',index:4,point:'d'},known),/position/);
 assert.throws(()=>resizeSplinePoles(explicit,{action:'insert',index:1,point:'missing'},known),/known/);
 for(const mode of ['periodic','non_clamped']){
  const source=editSplineBasis(explicit,{degree:2,knots:mode==='periodic'?[0,1,2,3]:[0,1,2,3,4,5],multiplicities:mode==='periodic'?[1,1,1,1]:[1,1,1,1,1,1],mode,endpoints:{start:'start',end:'end'}},known);source.basis.weights=[];
  const result=resizeSplinePoles(source,{action:'insert',index:1,point:'d'},known);assert.deepEqual(result.basis.weights,[]);assert.equal(result.basis.degree,2);
  if(mode==='periodic'){assert.deepEqual(result.basis.knots,[0,.75,1.5,2.25,3]);assert.equal(result.basis.periodic,true);}
  else{assert.deepEqual(result.basis.knots,[1,1.5,2,2.5,3,3.5,4]);assert.deepEqual(result.endpoints,source.endpoints);assert.throws(()=>resizeSplinePoles(source,{action:'insert',index:1,point:'start'},known),/separate/);}
 }
 assert.deepEqual(explicit.points,['a','b','c']);
});
test('pole count controls include staged weights and reject invalid removals without changing the draft',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.value=value;}};
 const d={points:known.map(id=>({id})),splines:[structuredClone(explicit)]},host=new Element('div'),messages=[];let changes=0;
 const render=()=>{host.replaceChildren();createSplineControls({host,definition:d,changed:()=>{changes++;render();},message:m=>messages.push(m)});};render();
 field(host,'Control point to insert').value='d';field(host,'Control pole position').value='2';
 field(host,'Weight 2 source').value='literal';field(host,'Weight 2 source').onchange();field(host,'Weight 2').value='1.5';field(host,'Weight 2').oninput();
 click(host,'Insert control pole');assert.deepEqual(d.splines[0].points,['a','d','b','c']);assert.equal(d.splines[0].basis.weights[2].literal.value,1.5);assert.equal(changes,1);
 field(host,'Control pole position').value='2';click(host,'Remove control pole');assert.deepEqual(d.splines[0].points,['a','b','c']);assert.equal(changes,2);
 field(host,'Control pole position').value='2';click(host,'Remove control pole');assert.equal(changes,2);assert.match(messages.at(-1),/degree \+ 1/);assert.equal(d.points.length,known.length);
 delete globalThis.document;delete globalThis.Option;
});
