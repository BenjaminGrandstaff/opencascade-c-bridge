import {test} from 'node:test';
import assert from 'node:assert/strict';
import {addElement, length, createSketchEditor} from './sketch-editor.mjs';

class Element {
  constructor(tag) { this.tagName=tag;this.children=[];this.handlers={};this.dataset={};this.style={};this.value=''; }
  append(...nodes) {this.children.push(...nodes);}
  prepend(...nodes) {this.children.unshift(...nodes);}
  replaceChildren(...nodes) {this.children=nodes;}
  setAttribute() {}
  addEventListener(name, fn) {this.handlers[name]=fn;}
  emit(name) {return this.handlers[name]?.();}
}
const all = element => [element, ...element.children.flatMap(all)];
const find = (host, text) => all(host).find(e => e.textContent === text);

test('primitive names cannot collide with points or other curve kinds', () => {
  const draft={points:[{id:'A'}],lines:[{id:'AB'}]};
  assert.throws(()=>addElement(draft,'circles',{id:'AB'}), /already exists/);
  assert.throws(()=>addElement(draft,'points',{id:' '}), /name/);
  const point={id:'B',x:length(5),y:length(0),fixed:true};
  addElement(draft,'points',point); point.x.literal.value=100;
  assert.equal(draft.points[1].x.literal.value,5);
});

test('coordinate edits preview a private guarded draft and remain available after rejected apply', async () => {
  globalThis.document={createElement:tag=>new Element(tag)};
  globalThis.Option=class extends Element {constructor(text,value){super('option');this.textContent=text;this.value=value;}};
  const source={id:'sketch',points:[{id:'A',x:length(0),y:length(0),fixed:true}],constraints:[],profile:[]};
  const host=new Element('section'), calls=[], previews=[];
  let accepted=null, reject=true, shown=null;
  createSketchEditor({host,instance:()=> 'part',showSketch:id=>shown=id,
    preview:async view=>previews.push(view), applied:async state=>accepted=state,restore:async()=>{},
    request:async(path,options)=>{
      if(path==='/api/sketches')return {version:4,sketches:[{feature:'profile',definition:source}]};
      const body=JSON.parse(options.body);calls.push(body);
      if(body.apply&&reject)throw new Error('Model changed. Reload the sketch.');
      return body.apply?{state:{version:5}}:{can_apply:true,annotations:{preview:true}};
    }});
  await find(host,'Edit sketch geometry').emit('click');
  const choice=all(host).find(e=>e.tagName==='select');choice.value='profile';
  await find(host,'Load sketch draft').emit('click');assert.equal(shown,'profile');
  const coordinate=find(host,'X (mm)').children[0];coordinate.value='7';coordinate.emit('change');
  await find(host,'Preview draft').emit('click');
  assert.equal(source.points[0].x.literal.value,0);
  assert.equal(calls[0].expected.points[0].x.literal.value,0);
  assert.equal(calls[0].definition.points[0].x.literal.value,7);
  assert.equal(calls[0].version,4);assert.equal(previews.length,1);assert.equal(accepted,null);
  await find(host,'Apply verified draft').emit('click');
  assert(find(host,'Model changed. Reload the sketch.'));
  assert(find(host,'Apply verified draft'));assert.equal(accepted,null);
  reject=false;await find(host,'Apply verified draft').emit('click');assert.deepEqual(accepted,{version:5});
  assert(!find(host,'Apply verified draft'));
  delete globalThis.document;delete globalThis.Option;
});
