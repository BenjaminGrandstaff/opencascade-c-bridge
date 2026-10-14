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
