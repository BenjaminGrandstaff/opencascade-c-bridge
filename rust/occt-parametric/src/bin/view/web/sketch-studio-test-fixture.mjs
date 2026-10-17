import {installDrawing} from './sketch-studio-draw.mjs';

export function fixture(t,{local=false,api}={}){
 t.mock.timers.enable({apis:['setTimeout']});
 const nodes=new Map(),calls=[],messages=[];
 const node=(tag='div',attrs={},...children)=>({tag,attrs,children:children.flat(),style:{},addEventListener(){},getBoundingClientRect(){return {left:0,top:0}},append(...v){this.children.push(...v)},replaceChildren(...v){this.children=v}});
 const document={head:node('head'),getElementById:id=>{if(!nodes.has(id))nodes.set(id,node());return nodes.get(id)}};
 const baseline={id:'profile',points:[],lines:[],constraints:[],profile:[]};
 const layer={feature:'profile',local,freeDegrees:local?null:2,error:null,create:{plane:'xy',offset:0}};
 let active=layer;
 const studio={version:1,instance:'part',features:[],tool:'select',tools:new Map(),layers:[layer],scenes:local?[]:[{kind:'sketch',feature:'profile',solver:{free_degrees:2}}],definitions:{profile:baseline},previewScenes:{},modelState:{undo_count:3,redo_count:2,dirty:true},
  html:node,fill:(host,...children)=>host.replaceChildren(...children.flat()),
  activeLayer:()=>active,activate:feature=>{active=studio.layers.find(l=>l.feature===feature)},
  redraw(){},renderStatus(){},say:(message,error=false)=>{studio.message=message;studio.error=error;messages.push(message)},
  refresh:()=>studio.renderToolbar?.(),reload:async()=>{studio.reloads++},reloads:0,
  api:async(path,options)=>{calls.push({path,...options});return api?api(path,options):path==='/api/sketch'?{can_apply:true,annotations:{scenes:[{kind:'sketch',feature:'profile',solver:{free_degrees:7}}]}}:{}},
 };
 const drawing=installDrawing(studio,document);
 const edit=n=>{studio.ensureDraft();studio.changeDraft({...studio.draft.definition,points:[{id:`p${n}`} ]})};
 const button=text=>nodes.get('draft-actions').children.find(n=>n.children.includes(text));
 const flush=async()=>{await Promise.resolve();await Promise.resolve();await Promise.resolve()};
 return {studio,drawing,document,baseline,layer,calls,messages,edit,button,flush};
}
