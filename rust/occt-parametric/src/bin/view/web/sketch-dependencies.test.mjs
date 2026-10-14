import {test} from 'node:test';
import assert from 'node:assert/strict';
import {sketchDependencyGraph,dependencyReach,createDependencyControls} from './sketch-dependencies.mjs';
const node=(graph,id)=>graph.nodes.get(graph.geometry.get(id));
const keys=(graph,id,direction)=>[...node(graph,id)[direction]];
const features=[{id:'source',operation:{box:{}}},{id:'profile',operation:{sketch_face:{sketch:{}}}},{id:'body',operation:{extrude:{input:'profile'}}},{id:'cut',operation:{cut:{object:'body',tool:'tool'}}},{id:'unrelated',operation:{box:{}}}];
test('projection aliases, weighted links and constraints lead to profiles and downstream features',()=>{
 const d={points:[{id:'follow'},{id:'seed'}],splines:[{id:'authored',points:['follow','seed']}],projections:[{id:'reference',input:'source',kind:'spline',junctions:[{id:'join'}],intersections:[{id:'cross',other:'guide'}]},{id:'guide',input:'source',kind:'line'}],constraints:[{weighted_point:{point:'follow',terms:[{point:'reference:junction:join',weight:{parameter:'seed'}}]}},{point_on_curve:{point:'seed',curve:'authored'}}],profile:['reference','authored'],measurements:[{id:'length',measure:{curve_length:{curve:'authored'}}}]};
 const original=structuredClone(d),graph=sketchDependencyGraph(d,{feature:'profile',features});
 assert.deepEqual(keys(graph,'reference','dependencies'),['feature:source']);
 assert(keys(graph,'reference:intersection:cross','dependencies').includes('geometry:guide'));
 const affected=dependencyReach(graph,'geometry:reference');
 for(const key of ['geometry:follow','geometry:authored','constraint:0','constraint:1','measurement:length','profile:outer','feature:body','feature:cut'])assert(affected.includes(key),key);
 assert(!affected.includes('feature:unrelated'));assert(!affected.includes('geometry:seed'),'constraints do not invent solver motion');
 assert.deepEqual(d,original);
});
test('split outputs, nested patterns, generated shape points and trim crossings retain paths',()=>{
 const d={points:[{id:'A'},{id:'B'},{id:'hint'}],lines:[{id:'axis',start:'A',end:'B'}],shapes:[{rectangle:{id:'box',first:'A',opposite:'B'}}],patterns:[{mirror:{id:'copy',entities:['box:side0'],axis:'axis'}}],curve_operations:[{split:{entity:'copy:1:box:side0',at:{crossing:{curve:'axis',near:'hint'}},before:'piece1',after:'piece2'}}],profile:['piece1'],profile_operations:[{trim_between:{entity:'piece1',from:{point:{point:'hint'}},to:'end'}}]};
 const graph=sketchDependencyGraph(d,{feature:'profile',features});
 const affected=dependencyReach(graph,'geometry:A');
 for(const key of ['geometry:box','geometry:box:side0','pattern:copy','geometry:copy:1:box:side0','curve-operation:0','geometry:piece1','profile:outer','profile-operation:0'])assert(affected.includes(key),key);
 assert(keys(graph,'piece1','dependencies').includes('curve-operation:0'));
 assert(graph.nodes.get('curve-operation:0').dependencies.has('geometry:hint'));
});
test('text content, parameter names, selectors and literal values are not geometry references',()=>{
 const d={points:[{id:'A'},{id:'B'}],texts:[{id:'text',text:'B',font:'B',anchor:'A',height:{parameter:'B'}}],projections:[{id:'ref',input:'source',kind:'line',edge:{near:{literal:{x:'B'}}}}],constraints:[{distance:{first:'A',second:'B',value:{formula:{expression:'A',parameters:['A']}}}}]};
 const graph=sketchDependencyGraph(d,{feature:'profile',features});
 assert.deepEqual(keys(graph,'text','dependencies'),['geometry:A']);assert.deepEqual(keys(graph,'ref','dependencies'),['feature:source']);
});
test('cycles are bounded, IDs containing colons stay authored and missing draft references remain visible',()=>{
 const d={points:[{id:'a:1:b'}],projections:[{id:'a',input:'source',kind:'line'}],constraints:[{weighted_point:{point:'one',terms:[{point:'two'}]}},{weighted_point:{point:'two',terms:[{point:'one'}]}}],profile:['missing']};
 const graph=sketchDependencyGraph(d,{feature:'profile',features});assert.equal(node(graph,'a:1:b').kind,'points');assert.equal(node(graph,'missing').kind,'unresolved');
 const affected=dependencyReach(graph,'geometry:one');assert.equal(new Set(affected).size,affected.length);assert(!affected.includes('geometry:one'));assert(affected.includes('geometry:two'));
});
test('large reference graphs traverse once and cap rendered consumer rows',()=>{
 const d={points:[{id:'source'}],constraints:[],lines:[],profile:[]};for(let i=0;i<10000;i++)d.constraints.push({coincident:{first:'source',second:'other'}});
 const start=performance.now(),graph=sketchDependencyGraph(d,{feature:'profile',features}),affected=dependencyReach(graph,'geometry:source');assert.equal(affected.filter(k=>k.startsWith('constraint:')).length,10000);assert(performance.now()-start<2000);
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.value=value;}};
 try{const host=new Element('div');createDependencyControls({host,definition:d,feature:'profile',features});assert(all(host).some(e=>e.textContent==='Showing 100 of 10001 references.'));assert.equal(all(host).filter(e=>e.tagName==='li'&&e.textContent.startsWith('Constraint ')).length,100);}
 finally{delete globalThis.document;delete globalThis.Option;}

});
class Element{constructor(tag){this.tagName=tag;this.children=[];this.value='';}append(...e){this.children.push(...e);}prepend(...e){this.children.unshift(...e);}replaceChildren(...e){this.children=e;}setAttribute(k,v){this[k]=v;}}
const all=e=>[e,...e.children.flatMap(all)];
test('dependency inspection is read-only, armed canvas picks cancel and highlights follow references',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};globalThis.Option=class extends Element{constructor(text,value){super('option');this.textContent=text;this.value=value;}};
 try{
 const d={points:[{id:'A'},{id:'B'}],lines:[{id:'line',start:'A',end:'B'}],constraints:[{horizontal:{line:'line'}}],profile:['line']},original=structuredClone(d),host=new Element('div'),highlights=[];let begins=0;
 const controls=createDependencyControls({host,definition:d,features,feature:'profile',highlight:ids=>highlights.push(ids),beginPicking:()=>begins++});
 assert.equal(controls.pick('line'),false);const b=all(host).find(e=>e.textContent==='Inspect dependencies from canvas');b.onclick();assert.equal(begins,1);assert.equal(controls.pick('unknown'),true);assert.equal(controls.pick('line'),true);assert.equal(controls.selected,'line');assert.equal(controls.open,true);assert.deepEqual(highlights.at(-1),['line']);
 b.onclick();controls.cancelPick();assert.equal(controls.pick('A'),false);assert(controls.inspect('A'));assert(all(host).some(e=>e.textContent==='Used by (2)'));assert.deepEqual(d,original);
 }finally{delete globalThis.document;delete globalThis.Option;}
});

test('generated canvas references resolve lazily without expanding every pattern copy',()=>{
 const graph=sketchDependencyGraph({points:[{id:'A'},{id:'B'}],lines:[{id:'line',start:'A',end:'B'}],patterns:[{linear:{id:'copy',entities:['line'],direction:'line',count:1000}}]},{feature:'profile',features});
 assert(!graph.geometry.has('copy:999:line'));const key=graph.resolveGenerated('copy:999:line');
 assert.deepEqual([...graph.nodes.get(key).dependencies],['pattern:copy','geometry:line']);
 assert(dependencyReach(graph,key).includes('feature:body'));assert(graph.nodes.get('profile:outer').dependencies.has('pattern:copy'));
 assert.equal(graph.resolveGenerated('unknown:curve'),null);assert(graph.nodes.size<30);
});

test('canonical native feature inputs cover multi-profile and named-selector dependencies',()=>{
 const graph=sketchDependencyGraph({points:[{id:'A'}]},{feature:'profile',features:[...features,{id:'loft',operation:{profile_loft:{profiles:['profile','other']}}},{id:'finishing',operation:{fillet:{input:'loft'}}}],featureDependencies:{source:[],profile:['source'],body:['profile'],cut:['body'],unrelated:[],loft:['profile','other'],finishing:['loft','selector-source']}});
 const affected=dependencyReach(graph,'geometry:A');assert(affected.includes('feature:loft'));assert(affected.includes('feature:finishing'));assert(graph.nodes.get('feature:finishing').dependencies.has('feature:selector-source'));
 // Current local sketch links replace the accepted sketch's dependency metadata after detach.
 assert(!graph.nodes.get('feature:profile').dependencies.has('feature:source'));
});
