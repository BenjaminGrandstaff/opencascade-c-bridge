import {loftSeamGuides,drawLoftSeamGuides} from './sketch-studio-loft-seams.mjs';
import {createLoftAnchorPicker} from './sketch-studio-loft-pick.mjs';
import {loftTangentGuides,drawLoftTangentGuides} from './sketch-studio-loft-guides.mjs';
import {createSolidSourcePicker} from './sketch-studio-limit-pick.mjs';
import {createSolidEditor} from './solid-editor.mjs';
import {planeNormal} from './sketch-studio-plane.mjs';

export function installStudioSolids(studio, document) {
  const {html} = studio;
  const panel = html('section', {class: 'sheet', role: 'dialog', 'aria-label': 'Sketch to solid', hidden: true});
  document.body.append(panel);
  let session = null, ownAction = false, guides=[],seams=[];
  const decorate=studio.decorate;
  studio.decorate=context=>[...(decorate?.(context)??[]),...(session&&session.checkpoint()?[...drawLoftTangentGuides(guides,context),...drawLoftSeamGuides(seams,context)]:[])];
  const picker=createSolidSourcePicker(studio,(active,kind)=>{
    const old=ownAction;ownAction=true;
    try{if(active){studio.setTool('select');studio.tool=kind==='edge'?'solid-finish-edge':'solid-limit-face';}else if(['solid-limit-face','solid-finish-edge'].includes(studio.tool))studio.tool='select';studio.refresh();}
    finally{ownAction=old;}
  },()=>document.getElementById('view').getBoundingClientRect());
  const anchorPicker=createLoftAnchorPicker(studio,active=>{
    const old=ownAction;ownAction=true;
    try{if(active){studio.setTool('select');studio.tool='solid-loft-anchor';}else if(studio.tool==='solid-loft-anchor')studio.tool='select';studio.refresh();}
    finally{ownAction=old;}
  },()=>document.getElementById('view').getBoundingClientRect());
  const clearPreview = () => {const shown=guides.length>0||seams.length>0;guides=[];seams=[];studio.setSolidPreview(null);if(shown)studio.redraw();};
  function close() {
    if (ownAction) return;
    picker.cancel();anchorPicker.cancel();session = null; panel.hidden = true; clearPreview();
  }
  function open() {
    if (studio.modelPending) return false;
    if (studio.hasDraftHistory?.() || studio.draft?.previewing || studio.activeLayer()?.local) {
      studio.say('Apply or discard the sketch draft before creating or editing a solid.', true); return false;
    }
    if (studio.picking || studio.isPlanePicking?.()) { studio.say('Finish or cancel the face or plane pick first.', true); return false; }
    close(); studio.setTool('select'); studio.drawing.discard(true);
    const s = {checkpoint: studio.draftCheckpoint(), turn: 0, response: null}; session = s;
    const current = () => session === s && s.checkpoint() && !studio.hasDraftHistory?.();
    const request = async (path, options) => {
      if (!current()) throw Error('The model or sketch changed. Reopen Sketch to solid.');
      const body = options ? JSON.parse(options.body) : null, ticket = ++s.turn;
      if(body)anchorPicker.cancel();
      s.response = null; clearPreview();
      if (body?.apply) {
        ownAction = true; panel.inert = true; let accepted = false;
        try {
          return await studio.runModelOperation('Applying solid', async () => {
            const result = await studio.api(path, options);
            if (!result.applied) throw Error(result.error ?? 'The solid was not applied.');
            accepted = true;
            await studio.reload();
            return result;
          });
        } catch (error) {
          if (accepted) {
            studio.say(`Solid applied, but refreshing the view failed: ${error.message}. Reload the page.`, true);
          }
          throw error;
        } finally {
          ownAction = false; panel.inert = false;
          if (accepted) close();
          else if (session === s) s.checkpoint = studio.draftCheckpoint();
        }
      }
      const result = await studio.api(path, options);
      if (!current() || result.version !== studio.version || body && (result.instance !== studio.instance || result.output !== body.output)) throw Error('The model or sketch changed. Reopen Sketch to solid.');
      if (ticket !== s.turn) throw Error('Solid values changed. Preview again.');
      s.response = result;
      if (body && result.can_apply === false) studio.say(`Solid preview rejected: ${result.error}`, true);
      return result;
    };
    const host = html('div');
    studio.fill(panel, html('header', {}, html('h2', {}, 'Sketch to solid'), html('button', {onclick: close}, 'Close')), host);
    panel.hidden = false;
    createSolidEditor({host, request, instance: () => studio.instance, autoStart: true,
      started: (feature,output) => {
        if(current()&&feature?.operation.profile_loft&&feature.id===output){guides=loftTangentGuides({scenes:studio.scenes},studio.instance,output);seams=loftSeamGuides({scenes:studio.scenes},studio.instance,output);studio.redraw();}
      },
      pickLoftAnchor: profile => anchorPicker.pick(profile,current),
      pickHolePosition: target => picker.pick(target,current,'hole'),
      pickSolidEdge: target => picker.pick(target,current,'edge'),
      pickLimitingFace: target => picker.pick(target,current),
      cancelLimitingFacePick: () => {picker.cancel();anchorPicker.cancel();},
      initialProfile: () => studio.activeLayer()?.feature,
      profileDirection: feature => {
        const frame = studio.layers.find(layer => layer.feature === feature)?.frame;
        return frame ? planeNormal(frame) : null;
      },
      profileAxis: feature => {
        const frame = studio.layers.find(layer => layer.feature === feature)?.frame;
        return frame ? {origin: frame.origin_mm, axis: frame.y_axis} : null;
      },
      invalidatePreview: () => { if (current()) { s.turn++; s.response = null; const shown = Boolean(studio.solidPreview); clearPreview(); if (shown) studio.say('Solid values changed. Preview again to see the updated result.'); } },
      preview: async (_, result) => {
        if (!current() || s.response !== result) return;
        if (result.can_apply && result.gltf) {
          guides=loftTangentGuides(result.annotations,studio.instance,result.output);
          seams=loftSeamGuides(result.annotations,studio.instance,result.output);
          studio.setSolidPreview(result.gltf);
          studio.say('Solid preview shown in 3D. Apply commits it; Close restores the accepted model.');
        } else {
          clearPreview(); studio.say(`Solid preview rejected: ${result.error ?? 'No verified solid was generated.'}`, true);
        }
      },
      applied: async () => studio.say('Solid applied. Save writes the updated model to its file.'),
      restore: async () => close(),
      showSketch: feature => {
        if (!current() || feature === studio.activeLayer()?.feature) return;
        ownAction = true;
        try { studio.activate(feature); }
        finally { ownAction = false; s.checkpoint = studio.draftCheckpoint(); }
      },
    });
    return true;
  }
  const tool = studio.setTool; studio.setTool = id => { close(); tool(id); };
  const cancel = studio.cancelSelectionGesture; studio.cancelSelectionGesture = () => { cancel?.(); close(); };
  const change = studio.changeDraft; studio.changeDraft = (...args) => { close(); return change(...args); };
  const activate = studio.onActivate; studio.onActivate = layer => { close(); activate?.(layer); };
  const load = studio.onLoad; studio.onLoad = () => { close(); load?.(); };
  const key = studio.onKey; studio.onKey = event => { if (event.key === 'Escape' && session) { if(picker.active||anchorPicker.active){picker.cancel();anchorPicker.cancel();studio.say('Source picking canceled. Solid draft retained.');}else close(); return true; } return key?.(event) ?? false; };
  panel.addEventListener('keydown', event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); if(picker.active||anchorPicker.active){picker.cancel();anchorPicker.cancel();studio.say('Source picking canceled. Solid draft retained.');}else close(); } });
  studio.fill(document.getElementById('solid-features'), html('h2', {}, 'Solid features'), html('button', {onclick: open}, 'Build or finish a solid'));
  return {open, close, picker, anchorPicker};
}
