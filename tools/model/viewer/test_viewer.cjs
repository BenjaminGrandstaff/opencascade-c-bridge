// Interaction tests against the real viewer script, using a minimal DOM host.
// These exercise application logic, not a browser or a replacement renderer.
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict');
class Element {
 constructor(tag='div'){this.tagName=tag;this.children=[];this.attributes={};this.dataset={};this.handlers={};this.checked=true;this.value='';this.clientWidth=900;this.clientHeight=580;this.textContent='';}
 setAttribute(k,v){this.attributes[k]=String(v);if(k==='class')this.className=v;if(k.startsWith('data-'))this.dataset[k.slice(5)]=v;}
 append(...nodes){this.children.push(...nodes);}replaceChildren(...nodes){this.children=nodes;}
 addEventListener(name,fn){this.handlers[name]=fn;}setPointerCapture(){}getContext(){return null;}
 emit(name,event={}){this.handlers[name]?.({preventDefault(){},stopPropagation(){},...event});}
}
const source=fs.readFileSync(__dirname+'/viewer.js','utf8');
const file=process.argv[2];if(!file)throw Error('usage: node test_viewer.cjs VIEW.json');
const data=JSON.parse(fs.readFileSync(file,'utf8')),elements={};
for(const id of ['view-data','surface','overlay','viewport','scene','fit','dimensions','constraints','points','filter','annotations','annotation-count','selection','coordinates','empty','status','summary'])elements[id]=new Element();
elements['view-data'].textContent=JSON.stringify(data);
const context={document:{getElementById:id=>elements[id],createElement:tag=>new Element(tag),createElementNS:(_,tag)=>new Element(tag)},window:{devicePixelRatio:1},ResizeObserver:class {observe(){}disconnect(){}},console};vm.createContext(context);vm.runInContext(source,context);
const viewer=context.window.occtViewer;
assert(viewer.scene&&elements.scene.children.length===data.scenes.length);
assert(elements.overlay.children.length>0);
for(let i=0;i<data.scenes.length;i++){
 elements.scene.value=String(i);elements.scene.emit('change');
 assert.equal(viewer.scene.title,data.scenes[i].title);
 if(viewer.scene.error)continue;
 const dimension=viewer.scene.annotations.find(a=>a.kind==='dimension');
 assert(dimension,'scene needs a dimension annotation');
 // Click the same list button a user would select, and inspect related controls.
 const button=elements.annotations.children.find(e=>e.dataset.annotation===dimension.id);assert(button);button.emit('click');
 assert.equal(viewer.selected,dimension.id);
 const nodes=e=>[e,...e.children.flatMap(nodes)];assert(nodes(elements.selection).some(e=>e.tagName==='pre'&&e.textContent===JSON.stringify(dimension.detail,null,2)));
 assert(elements.overlay.children.some(e=>e.attributes['data-annotation']===dimension.id));
 const before=viewer.project(viewer.scene.bounds[1]);
 elements.viewport.emit('pointerdown',{clientX:50,clientY:50,pointerId:1,target:new Element()});
 elements.viewport.emit('pointermove',{clientX:90,clientY:70});elements.viewport.emit('pointerup');
 const after=viewer.project(viewer.scene.bounds[1]);assert.notDeepEqual([...before],[...after],'orbit/pan changes projection');
 elements.viewport.emit('wheel',{deltaY:-100});assert.notDeepEqual([...after],[...viewer.project(viewer.scene.bounds[1])],'zoom changes projection');
 elements.fit.emit('click');
 elements.dimensions.checked=false;elements.dimensions.emit('change');
 assert(!elements.overlay.children.some(e=>e.attributes['data-annotation']===dimension.id),'dimension toggle hides annotations');
 elements.dimensions.checked=true;elements.dimensions.emit('change');
 elements.viewport.emit('keydown',{key:'Escape'});assert.equal(viewer.selected,undefined);
 if(viewer.scene.solver&&!viewer.scene.solver.solved){assert(elements.status.textContent.includes('Conflicting'));assert(viewer.scene.annotations.some(a=>a.status==='failed'));}
}
// Native regeneration replaces scene data without resetting camera or selection.
const validIndex=data.scenes.findIndex(s=>!s.error);
elements.scene.value=String(validIndex);elements.scene.emit('change');
const selectedDimension=viewer.scene.annotations.find(a=>a.kind==='dimension');
viewer.selectAnnotation(selectedDimension.id);
elements.viewport.emit('wheel',{deltaY:-120});
const camera=JSON.stringify(viewer.camera);
const revised=JSON.parse(JSON.stringify(data));
const replacement=revised.scenes[validIndex].annotations.find(a=>a.id===selectedDimension.id);
replacement.label='Updated native dimension';
replacement.detail={value_mm:60};
viewer.update(revised);
assert.equal(viewer.selected,selectedDimension.id);
assert.equal(JSON.stringify(viewer.camera),camera,'regeneration preserves orbit, zoom and pan');
assert(elements.selection.children.some(e=>e.tagName==='h3'&&e.textContent==='Updated native dimension'));
viewer.destroy();
console.log('PASS viewer scene navigation, label selection, related controls, orbit/pan/zoom, visibility and failed states');
