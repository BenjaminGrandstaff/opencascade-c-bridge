import {test} from 'node:test';
import assert from 'node:assert/strict';
import {placedPoint,screenPoint,drawAssemblyAnnotations} from './assembly_annotations.mjs';
import {fitCamera,viewProjection} from './viewer.mjs';

const identity=[1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1];
class Element {
  constructor(tag='svg'){this.tag=tag;this.attributes={};this.children=[];this.handlers={};this.ownerDocument={createElementNS:(_,tag)=>new Element(tag)};}
  setAttribute(k,v){this.attributes[k]=v;}
  append(...nodes){this.children.push(...nodes);}
  replaceChildren(...nodes){this.children=nodes;}
  addEventListener(name,fn){this.handlers[name]=fn;}
}

test('family-local mm anchors follow a rotated, translated assembly node in glTF meters',()=>{
  assert.deepEqual(placedPoint([1000,2000,3000],identity),[1,3,-2]);
  const placed=[0,0,-1,0,0,1,0,0,1,0,0,0,5,0,0,1];
  assert.deepEqual(placedPoint([1000,2000,3000],placed),[3,3,-1]);
  const a=placedPoint([0,0,0],placed),b=placedPoint([40,0,0],placed);
  assert(Math.abs(Math.hypot(...b.map((v,i)=>v-a[i]))-.04)<1e-12,'placement preserves the true 40 mm dimension');
});

test('projection uses the WebGL camera and clips anchors behind or outside its depth range',()=>{
  const camera=fitCamera({min:[0,0,0],max:[1,1,1]},1.5);
  const projection=viewProjection(camera,1.5);
  const center=screenPoint(camera.target,projection,900,600);
  assert(Math.abs(center[0]-450)<1e-4&&Math.abs(center[1]-300)<1e-4);
  assert.equal(screenPoint([0,0,0],[...identity.slice(0,15),-1],900,600),null);
  assert.equal(screenPoint([0,0,2],identity,900,600),null);
  assert.equal(screenPoint([NaN,0,0],identity,900,600),null);
});

test('assembly labels select their native annotation and update with transforms and visibility',()=>{
  const root=new Element(),camera=fitCamera({min:[0,0,0],max:[.15,.05,.05]},1.5),selected=[];
  const annotation={id:'width',kind:'dimension',status:'driving',label:'40 mm',anchors:[[0,0,0],[40,0,0]],parameters:['width']};
  const scene={instance:'copy',annotations:[annotation,{id:'valid',kind:'requirement',status:'passed',label:'valid',anchors:[[20,10,0]]}]};
  const node={annotationMatrix:identity},options={width:900,height:600};
  const select=(a,s)=>selected.push([a,s]);
  drawAssemblyAnnotations(root,scene,node,camera,options,select);
  const label=root.children.find(e=>e.attributes['data-annotation']==='width');assert(label);
  label.handlers.click({stopPropagation(){}});assert.equal(selected[0][0],annotation);assert.equal(selected[0][1],scene);
  label.handlers.keydown({key:'Enter',preventDefault(){}});assert.equal(selected.length,2);
  const original=label.attributes.transform;
  const shifted=[...identity];shifted[12]=.1;
  drawAssemblyAnnotations(root,scene,{annotationMatrix:shifted},camera,options,select);
  assert.notEqual(root.children.find(e=>e.attributes['data-annotation']==='width').attributes.transform,original);
  assert.equal(root.children.find(e=>e.attributes['data-annotation']==='width').children[1].textContent,'40 mm');
  drawAssemblyAnnotations(root,scene,node,camera,{...options,dimensions:false},select);
  assert(!root.children.some(e=>e.attributes['data-annotation']==='width'));
  assert(root.children.some(e=>e.attributes['data-annotation']==='valid'));
  drawAssemblyAnnotations(root,null,node,camera,options,select);assert.equal(root.children.length,0);
});
