import {test} from 'node:test';import assert from 'node:assert/strict';
import {crossingReviews,createCrossingReview} from './crossing-review.mjs';
const c=(point='q')=>({crossing_point:{point,first:'circle',second:'guide',near:{parameter:'hint'},maximum_distance:{parameter:'radius'}}});
const scene=(constraint=c())=>({annotations:[{status:'passed',parameters:['hint','radius'],detail:{constraint,crossing_window:{center:[8,5,0],radius:3,distance:0.66}}}]});
class Element{constructor(tag){this.tagName=tag;this.children=[];this.value='';}append(...nodes){this.children.push(...nodes);}replaceChildren(...nodes){this.children=nodes;}setAttribute(k,v){this[k]=v;}}
const all=e=>[e,...e.children.flatMap(all)],find=(e,t)=>all(e).find(x=>x.textContent===t);
test('review accepts only matching native passed windows and never evaluates expressions',()=>{
 const d={constraints:[c()]},before=structuredClone(d),native=scene();assert.equal(crossingReviews(d,native)[0].window.radius,3);assert.deepEqual(d,before);
 for(const [view,current]of [[native,false],[null,true],[scene(c('other')),true]])assert.equal(crossingReviews(d,view,current)[0].window,null);
 assert.deepEqual(crossingReviews(d,null,false)[0].drivers,['hint','radius']);
 native.annotations[0].status='failed';assert.equal(crossingReviews(d,native)[0].window,null);native.annotations[0].status='passed';native.annotations[0].detail.crossing_window.radius=Infinity;assert.equal(crossingReviews(d,native)[0].window,null);
});
test('review filtering, inspection and overlays are read-only and stale controls refuse display',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};
 try{const d={constraints:[c('right'),c('left')]},v={annotations:[...scene(c('right')).annotations,...scene(c('left')).annotations]},host=new Element('div'),shown=[],highlighted=[];let fresh=true,inspected;
 const panel=createCrossingReview({host,definition:d,review:()=>v,current:()=>fresh,show:w=>shown.push(w),highlight:ids=>highlighted.push(ids),inspect:id=>inspected=id});const before=structuredClone(d);
 find(host,'Show matching crossing windows').onclick();assert.equal(shown.at(-1).length,2);assert(highlighted.at(-1).includes('right'));find(host,'Inspect crossing left').onclick();assert.equal(inspected,'left');
 const filter=all(host).find(e=>e['aria-label']==='Filter crossing windows');filter.value='left';filter.oninput();find(host,'Show matching crossing windows').onclick();assert.deepEqual(shown.at(-1).map(w=>w.point),['left']);
 const staleButton=find(host,'Show window left');fresh=false;staleButton.onclick();assert.deepEqual(shown.at(-1),[]);assert(find(host,'Show matching crossing windows').disabled);panel.refresh();assert.deepEqual(d,before);
 }finally{delete globalThis.document;}
});
test('large crossing reviews bound rows and overlays to 100 while preserving searchable access',()=>{
 globalThis.document={createElement:tag=>new Element(tag)};
 try{const constraints=Array.from({length:10000},(_,i)=>c(`q${i}`)),d={constraints},v={annotations:constraints.map(c=>scene(c).annotations[0])},host=new Element('div');let shown=[];const now=performance.now();
 createCrossingReview({host,definition:d,review:()=>v,current:()=>true,show:w=>shown=w});assert.equal(all(host).filter(e=>e.tagName==='fieldset').length,100);find(host,'Show matching crossing windows').onclick();assert.equal(shown.length,100);
 const filter=all(host).find(e=>e['aria-label']==='Filter crossing windows');filter.value='q9999';filter.oninput();find(host,'Show matching crossing windows').onclick();assert.equal(shown[0].point,'q9999');assert(performance.now()-now<2000);
 }finally{delete globalThis.document;}
});
