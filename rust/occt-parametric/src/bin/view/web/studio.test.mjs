// Executes the real studio page logic with a minimal DOM/API host. Rendering
// and native geometry have their own tests; this checks the edit integration.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import { drawAssemblyAnnotations } from './assembly_annotations.mjs';
import { createAnnotationController, editableControls } from './annotations.mjs';

class Element {
  constructor(tag='div') { this.tagName=tag;this.children=[];this.handlers={};this.dataset={};this.attributes={};this.value='';this.style={};this.clientWidth=900;this.clientHeight=600;this.classList={toggle(){}}; }
  setAttribute(name,value){this.attributes[name]=value;}
  append(...children){this.children.push(...children);} prepend(...children){this.children.unshift(...children);}
  replaceChildren(...children){this.children=children;}
  addEventListener(name,fn){this.handlers[name]=fn;}
  emit(name,event={}){return this.handlers[name]?.({target:this,preventDefault(){},...event});}
  attachShadow(){this.shadowRoot=new Root();return this.shadowRoot;}
  matches(){return false;}
}
class Root {
  constructor(){this.elements={};}
  getElementById(id){return this.elements[id]??=new Element();}
  createElement(tag){return new Element(tag);}
}
const descendants=e=>[e,...e.children.flatMap(descendants)];
const settle=async check=>{for(let i=0;i<40;i++){if(check())return;await new Promise(r=>setImmediate(r));}assert(check(),'page did not settle');};

 test('select a dimension, edit only its instance, and wait for regeneration before Save', async()=>{
  const document=new Root(), calls=[];
  const $=id=>document.getElementById(id);
  $('view-mode').value='annotated';
  let currentVersion=1, width=40, saved=40, selectedAnnotation=null, selectedScene=null, onSelect, component, releaseEdit;
  let hold=true, failed=false;
  const state=()=>({version:currentVersion,family:'block-family',model:'block.json',dirty:width!==saved,parameters:[{id:'width',kind:'scalar',value:40,unit:'mm'},{id:'height',kind:'scalar',value:25,unit:'mm'}]});
  const annotation=()=>({id:'distance-width',kind:'dimension',status:failed?'failed':'passed',label:`${width} mm`,parameters:['width'],detail:{},targets:['AB']});
  const scenes=(id,accepted=true)=>({version:currentVersion,accepted,scenes:[{kind:'sketch',instance:id,feature:'profile',annotations:[annotation()]},{kind:'solid',instance:id,feature:'body',annotations:[{...annotation(),anchors:[[0,0,0],[40,0,0]]}]}]});
  const response=(body,ok=true)=>({ok,statusText:'rejected',json:async()=>body,text:async()=>'<div id="selection"></div>'});
  const fetch=async(path,options={})=>{
    calls.push([path,options]);
    if(path==='/api/state')return response(state());
    if(path==='/api/model.gltf')return response({nodes:[{name:'block'},{name:'copy'}],meshes:[]});
    if(path==='/api/requirements')return response({requirements:{},assembly:[],variants:{},instances:{}});
    if(path==='/api/instances')return response({instances:[{id:'block',overrides:[]},{id:'copy',overrides:[]}]});
    if(path.startsWith('/api/instance?'))return response({id:new URL(path,'http://local').searchParams.get('id'),parameters:[{id:'width',value:width,source:'own'},{id:'height',value:25}]});
    if(path.startsWith('/api/annotations?'))return response(scenes(new URL(path,'http://local').searchParams.get('id')));
    if(path==='/annotation-panel.html')return response({});
    if(path==='/api/parameters'){
      const body=JSON.parse(options.body);
      if(body.set.width===100){failed=true;return response({error:'conflicting constraints',annotations:scenes('copy',false)},false);}
      if(hold)await new Promise(r=>{releaseEdit=r;});
      width=body.set.width;currentVersion++;failed=false;return response(state());
    }
    if(path==='/api/save'){saved=width;return response(state());}
    if(path==='/api/revert'){width=saved;currentVersion++;failed=false;return response(state());}
    throw Error(`Unexpected API path ${path}`);
  };
  const renderer={draw(){},setSelection(){},setModel(){},setMarkers(){}};
  const context={document,fetch,console,URL,Option:class extends Element{constructor(label,value){super('option');this.textContent=label;this.value=value;}},ResizeObserver:class{observe(){}},setTimeout:()=>1,clearTimeout(){},requestAnimationFrame:()=>1,
    createAnnotationController,editableControls,drawAssemblyAnnotations,createRenderer:()=>renderer,fitCamera:()=>({}),modelToGltf:v=>v,pan:()=>({}),parseGltf:g=>({...g,bounds:null}),pick:()=>-1,ray:()=>({}),
    window:{createOcctAnnotatedViewer:(root,data,select)=>{onSelect=select;component={data,fit(){},update(next){this.data=next;if(selectedAnnotation){selectedScene=next.scenes[0];selectedAnnotation=selectedScene.annotations[0];root.getElementById('selection').replaceChildren();select(selectedAnnotation,selectedScene);}}};return component;}}};
  vm.createContext(context);
  const page=fs.readFileSync(new URL('./index.html',import.meta.url),'utf8').split('<script type="module">')[1].split('</script>')[0].replace(/^import .*;$/gm,'');
  vm.runInContext(page,context);
  await settle(()=>component?.data?.scenes[0]?.instance==='block');
  $('scope').value='copy';await $('scope').emit('change');
  assert.equal(component.data.scenes[0].instance,'copy');
  selectedScene=component.data.scenes[0];selectedAnnotation=selectedScene.annotations[0];
  onSelect(selectedAnnotation,selectedScene);
  const selection=$('annotated').shadowRoot.getElementById('selection');
  const input=descendants(selection).find(e=>e.attributes['aria-label']==='Edit width');
  assert(input,'selecting a linked dimension adds its parameter editor');
  input.value='60';input.emit('change');
  const editing=vm.runInContext('flush()',context);
  await settle(()=>releaseEdit);
  const saving=$('save').emit('click');
  await new Promise(r=>setImmediate(r));
  assert(!calls.some(([path])=>path==='/api/save'),'Save must wait for the pending regeneration');
  releaseEdit();await Promise.all([editing,saving]);hold=false;
  const edit=JSON.parse(calls.find(([path])=>path==='/api/parameters')[1].body);
  assert.deepEqual(edit,{instance:'copy',set:{width:60},clear:[]});
  assert.equal(saved,60);
  assert.equal(component.data.version,2);
  assert.equal(component.data.scenes[0].annotations[0].label,'60 mm');
  assert.equal(Number(descendants(selection).find(e=>e.attributes['aria-label']==='Edit width').value),60);
  // Assembly labels use the same scoped edit queue without changing views.
  $('view-mode').value='assembly';await $('view-mode').emit('change');
  vm.runInContext('selectAssemblyAnnotation(annotations.viewer.data.scenes[1].annotations[0],annotations.viewer.data.scenes[1])',context);
  const assemblyInput=descendants($('assembly-selection-content')).find(e=>e.attributes['aria-label']==='Edit width');
  assert(assemblyInput);assemblyInput.value='70';assemblyInput.emit('change');await vm.runInContext('flush()',context);
  assert.equal($('view-mode').value,'assembly');assert.equal(width,70);
  assert($('assembly-selection-content').children[0].textContent.includes('70 mm'));
  assert.equal(Number(descendants($('assembly-selection-content')).find(e=>e.attributes['aria-label']==='Edit width').value),70);
  await $('save').emit('click');assert.equal(saved,70);
  // A rejected edit displays diagnostic data but cannot be saved as accepted.
  const retry=descendants(selection).find(e=>e.attributes['aria-label']==='Edit width');
  retry.value='100';retry.emit('change');await vm.runInContext('flush()',context);
  assert.equal(component.data.accepted,false);assert.equal(width,70);assert.equal(saved,70);
  assert($('error').textContent.includes('Edit rejected'));
  assert($('annotation-message').textContent.includes('accepted model is unchanged'));
  await $('revert').emit('click');
  assert.equal(component.data.accepted,true);assert.equal(component.data.version,4);
});
