import test from 'node:test';
import assert from 'node:assert/strict';
import {installFeatureHistory} from './sketch-studio-feature-history.mjs';
function setup() {
  const html = (tag, attrs = {}, ...children) => ({tag, attrs, children, append(...items) {this.children.push(...items);}});
  const host = html('div'); const calls = [];
  const studio = {html, fill: (host, ...children) => {host.children = children;}, version: 3,
    features: [{id:'base'},{id:'cut'},{id:'later'}], featureHistory:{head:'cut',suppressed:['cut']},
    featureStates:[{feature:'base',state:'active'},{feature:'cut',state:'suppressed',passes:'base'},{feature:'later',state:'rolled_back'}],
    api: async (path, options) => {calls.push({path, body:JSON.parse(options.body)});},
    reload: async () => {calls.push('reload');}, runModelOperation: async (_, action) => action(),
    say: (text, error) => {studio.message={text,error};}};
  const controls = installFeatureHistory(studio, {getElementById:()=>host});
  const all = node => [node,...node.children.filter(c=>c && typeof c==='object').flatMap(all)];
  return {studio,host,calls,controls,button:label=>all(host).find(n=>n.attrs['aria-label']===label)};
}
test('feature list shows all states and sends guarded checkout, suppression and moves',async()=>{
  const f=setup(); const rows=f.host.children[0].children; f.controls.render(); assert.equal(f.host.children[0].children,rows,'ordinary refresh retains feature rows'); assert.match(JSON.stringify(f.host),/rolled back/); assert.match(JSON.stringify(f.host),/passes base/);
  await f.button('Restore cut').attrs.onclick();
  assert.deepEqual(f.calls[0],{path:'/api/feature-history',body:{version:3,expected:{head:'cut',suppressed:['cut']},expected_order:['base','cut','later'],action:'suppress',feature:'cut',suppressed:false}});
  await f.button('Restore the full feature timeline').attrs.onclick();assert.equal(f.calls[2].body.head,null);
  await f.button('Move later earlier').attrs.onclick();assert.equal(f.calls[4].body.after,'base');
  assert.equal(f.button('Move base earlier').attrs.disabled,true);assert.equal(f.button('Move later later').attrs.disabled,true);
});
test('drafts and picks block history; rejected edits retain the accepted view',async()=>{
  const f=setup();f.studio.hasDraftHistory=()=>true;f.controls.render();
  assert.equal(f.button('Restore cut').attrs.disabled,true);await f.controls.apply({action:'checkout',head:null});assert.equal(f.calls.length,0);
  f.studio.hasDraftHistory=()=>false;f.studio.api=async()=>{throw Error('moving cut would put it before its input');};
  await f.controls.apply({action:'move',feature:'cut',after:null});assert.equal(f.calls.length,0);assert.match(f.studio.message.text,/before its input/);
});
