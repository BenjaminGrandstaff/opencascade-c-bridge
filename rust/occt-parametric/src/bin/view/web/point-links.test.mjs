import {test} from 'node:test';import assert from 'node:assert/strict';
import {linkNamedPoint,pointLinks,releasePointLink,createPointLinkControls} from './point-links.mjs';
import {sketchDependencyGraph,dependencyReach} from './sketch-dependencies.mjs';
const scalar=value=>({literal:{value,dimension:'scalar',unit:null}}),length=value=>({literal:{value,dimension:'length',unit:'millimeter'}});
function definition(){return {points:[{id:'a',x:length(0),y:length(0),fixed:true},{id:'b',x:length(10),y:length(5),fixed:true},{id:'p',x:length(4),y:length(2),fixed:true}],splines:[{id:'curve',points:['a','b'],basis:{degree:1,knots:[0,1],multiplicities:[2,2]}}],lines:[{id:'guide',start:'a',end:'b'}],constraints:[{distance:{first:'a',second:'b',value:length(10)}}],profile:['curve']};}
test('spline links preserve identity, other dimensions and parameter expressions while unfixing their target',()=>{
 const d=definition(),before=structuredClone(d),fraction={parameter:'location'},next=linkNamedPoint(d,{point:'p',mode:'spline',spline:'curve',fraction});assert.deepEqual(d,before);assert.equal(next.points[2].fixed,false);assert.deepEqual(next.constraints[0],d.constraints[0]);assert.deepEqual(pointLinks(next,'p'),[{spline_point:{point:'p',spline:'curve',fraction}}]);assert.deepEqual(next.profile,d.profile);
 const replaced=linkNamedPoint(next,{point:'p',mode:'spline',spline:'curve',fraction:scalar(.8)});assert.equal(pointLinks(replaced,'p').length,1);assert.equal(pointLinks(releasePointLink(replaced,'p'),'p').length,0);
 const graph=sketchDependencyGraph(next,{feature:'sketch'});assert(dependencyReach(graph,'geometry:a').includes('geometry:p'));
});
test('crossing links use two native memberships and reject self-references, invalid fractions or modes without mutation',()=>{
 const d=definition(),before=structuredClone(d);const next=linkNamedPoint(d,{point:'p',mode:'crossing',first:'curve',second:'guide'});assert.deepEqual(pointLinks(next,'p'),[{point_on_curve:{point:'p',curve:'curve'}},{point_on_curve:{point:'p',curve:'guide'}}]);
 for(const options of [{point:'a',mode:'spline',spline:'curve',fraction:scalar(.5)},{point:'p',mode:'spline',spline:'curve',fraction:scalar(1.1)},{point:'p',mode:'crossing',first:'curve',second:'curve'},{point:'a',mode:'crossing',first:'curve',second:'guide'}])assert.throws(()=>linkNamedPoint(d,options));assert.deepEqual(d,before);
});
class Element{constructor(tag){this.tagName=tag;this.children=[];this.value='';}append(...e){for(const child of e)child.parentElement=this;this.children.push(...e);}setAttribute(k,v){this[k]=v;}}
const all=e=>[e,...e.children.flatMap(all)],find=(e,t)=>all(e).find(c=>c.textContent===t);
test('named point controls stage links locally and keep the current parameter driver visible',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{const d=linkNamedPoint(definition(),{point:'p',mode:'spline',spline:'curve',fraction:{parameter:'location'}}),host=new Element('div');let changed;
 const controls=createPointLinkControls({host,definition:d,parameters:[{id:'location',parameter_type:{scalar:'scalar'}}],initialPoint:'p',changed:v=>changed=v,message(){}});
 // The DOM mock does not auto-select its first option.
 const input=label=>all(host).find(e=>e['aria-label']===label);assert.equal(input('Spline fraction driver').value,'location');assert(controls.inspect('p'));find(host,'Link named point').onclick();assert.deepEqual(pointLinks(changed,'p')[0].spline_point.fraction,{parameter:'location'});assert.deepEqual(d.constraints[1].spline_point.fraction,{parameter:'location'});
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('periodic explicit splines are linkable and selectable with inclusive seam fractions',()=>{
 const d=definition();d.points.push({id:'c',x:length(-10),y:length(0),fixed:true},{id:'d',x:length(0),y:length(-10),fixed:true});d.splines[0].points=['a','b','c','d'];d.splines[0].basis={periodic:true,degree:3,knots:[0,1,2,3,4],multiplicities:[1,1,1,1,1]};
 for(const value of [0,1])assert.deepEqual(pointLinks(linkNamedPoint(d,{point:'p',mode:'spline',spline:'curve',fraction:scalar(value)}),'p')[0].spline_point.fraction,scalar(value));
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{const host=new Element('div');createPointLinkControls({host,definition:d,changed(){},message(){}});const choice=all(host).find(e=>e['aria-label']==='Point link spline');assert(choice.children.some(e=>e.value==='curve'));assert(all(host).some(e=>e.textContent?.includes('fractions 0 and 1 name the same seam')));}
 finally{delete globalThis.document;delete globalThis.Option;}
});

test('selected crossing links retain hint expressions, replace seeded memberships and appear in dependencies',()=>{
 const d=definition(),seeded=linkNamedPoint(d,{point:'p',mode:'crossing',first:'curve',second:'guide'}),near={components:{x:{parameter:'hint_x'},y:{parameter:'guide_y'},z:length(0)}},maximum_distance={parameter:'search_radius'};
 const next=linkNamedPoint(seeded,{point:'p',mode:'selected_crossing',first:'curve',second:'guide',near,maximum_distance});assert.deepEqual(pointLinks(next,'p'),[{crossing_point:{point:'p',first:'curve',second:'guide',near,maximum_distance}}]);assert.equal(next.points[2].fixed,false);assert.deepEqual(next.constraints[0],d.constraints[0]);
 const graph=sketchDependencyGraph(next,{feature:'sketch'});assert(dependencyReach(graph,'geometry:a').includes('geometry:p'));assert.equal(pointLinks(releasePointLink(next,'p'),'p').length,0);
 const bad={point:'p',mode:'selected_crossing',first:'curve',second:'guide',near:{literal:{x:length(1).literal,y:length(2).literal,z:length(1).literal}},maximum_distance:length(2)};assert.throws(()=>linkNamedPoint(d,bad),/Z equal/);bad.near.literal.z=length(0).literal;bad.maximum_distance=length(0);assert.throws(()=>linkNamedPoint(d,bad),/positive/);
});
test('selected crossing controls keep parameterized hints and radii until explicitly replaced',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{const near={components:{x:{parameter:'hint_x'},y:{parameter:'guide_y'},z:length(0)}},maximum_distance={parameter:'search_radius'},d=linkNamedPoint(definition(),{point:'p',mode:'selected_crossing',first:'curve',second:'guide',near,maximum_distance}),host=new Element('div');let next;
 createPointLinkControls({host,definition:d,initialPoint:'p',changed:v=>next=v,message(){}});const input=label=>all(host).find(e=>e['aria-label']===label);assert.equal(input('Point link mode').value,'selected_crossing');find(host,'Link named point').onclick();assert.deepEqual(pointLinks(next,'p')[0].crossing_point.near,near);assert.deepEqual(pointLinks(next,'p')[0].crossing_point.maximum_distance,maximum_distance);
 input('Crossing hint X (mm)').value='8';input('Crossing hint X (mm)').oninput();input('Crossing hint Y (mm)').value='5';input('Crossing search radius (mm)').value='3';input('Crossing search radius (mm)').oninput();find(host,'Link named point').onclick();assert.equal(pointLinks(next,'p')[0].crossing_point.near.literal.x.value,8);assert.equal(pointLinks(next,'p')[0].crossing_point.maximum_distance.literal.value,3);
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('interpolated splines are selectable and preserve fraction expressions and dependencies',()=>{
 const d=definition();delete d.splines[0].basis;const next=linkNamedPoint(d,{point:'p',mode:'spline',spline:'curve',fraction:{parameter:'location'}});
 assert.deepEqual(pointLinks(next,'p')[0].spline_point.fraction,{parameter:'location'});assert(dependencyReach(sketchDependencyGraph(next,{feature:'sketch'}),'geometry:a').includes('geometry:p'));
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{const host=new Element('div');createPointLinkControls({host,definition:next,changed(){},message(){}});assert(all(host).find(e=>e['aria-label']==='Point link spline').children.some(e=>e.value==='curve'));}finally{delete globalThis.document;delete globalThis.Option;}
});
