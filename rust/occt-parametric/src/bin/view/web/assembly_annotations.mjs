import { viewProjection } from './viewer.mjs';

/** Family-local millimeters -> glTF coordinates -> explicit native family placement.
 * The mesh node's matrix includes recentering/shared-mesh offsets; it cannot
 * position native dimension anchors. Use extras.familyLocalMatrix instead. */
export function placedPoint(point, matrix) {
  const [x, y, z] = [point[0] / 1000, point[2] / 1000, -point[1] / 1000];
  return [0, 1, 2].map(i => matrix[i] * x + matrix[4 + i] * y + matrix[8 + i] * z + matrix[12 + i]);
}

/** Screen coordinates with explicit near/far clipping; points behind the camera disappear. */
export function screenPoint(point, matrix, width, height) {
  const clip = [0, 1, 2, 3].map(i => matrix[i] * point[0] + matrix[4 + i] * point[1] + matrix[8 + i] * point[2] + matrix[12 + i]);
  if (!clip.every(Number.isFinite) || clip[3] <= 0 || clip[2] < -clip[3] || clip[2] > clip[3]) return null;
  return [(clip[0] / clip[3] + 1) * width / 2, (1 - clip[1] / clip[3]) * height / 2];
}

/** Small SVG overlay for the selected solid, using the WebGL camera and node matrix. */
export function drawAssemblyAnnotations(root, scene, node, camera, options, select) {
  root.replaceChildren();
  const { width, height, dimensions = true, constraints = true, selected = null } = options;
  root.setAttribute('viewBox', `0 0 ${width} ${height}`);
  if (!scene || !node?.annotationMatrix || width <= 0 || height <= 0) return;
  const projection = viewProjection(camera, width / height);
  const make = (tag, attributes = {}, text) => {
    const element = root.ownerDocument.createElementNS('http://www.w3.org/2000/svg', tag);
    for (const [name, value] of Object.entries(attributes)) element.setAttribute(name, String(value));
    if (text !== undefined) element.textContent = text;
    return element;
  };
  const labels = [];
  const candidates = (scene.annotations ?? []).filter(a => a.anchors?.length && (a.kind !== 'parameter' || a.id === selected) && (a.kind === 'dimension' ? dimensions : constraints));
  for (const a of candidates) {
    if (a.kind === 'parameter' && a.id !== selected) continue;
    if (candidates.length > 120 && a.id !== selected && a.status !== 'failed') continue;
    const points = a.anchors.map(p => screenPoint(placedPoint(p, node.annotationMatrix), projection, width, height));
    if (points.some(p => !p)) continue;
    // Ignore annotations whose anchors are wholly outside the viewport.
    if (points.every(p => p[0] < 0 || p[0] > width || p[1] < 0 || p[1] > height)) continue;
    const color = a.status === 'failed' ? '#b52b36' : a.status === 'passed' ? '#24734d' : '#315fb3';
    if (a.kind === 'dimension' && points.length > 1) {
      const [p, q] = points;
      root.append(make('path', {d: `M${p[0]},${p[1]} L${q[0]},${q[1]}`, stroke:color, fill:'none', class:'assembly-dimension'}));
      const length = Math.hypot(q[0]-p[0], q[1]-p[1]);
      if (length > 1) for (const [end, other] of [[p,q],[q,p]]) {
        const [dx,dy] = [(other[0]-end[0])/length,(other[1]-end[1])/length];
        root.append(make('path', {d:`M${end[0]+dx*7-dy*3},${end[1]+dy*7+dx*3} L${end[0]},${end[1]} L${end[0]+dx*7+dy*3},${end[1]+dy*7-dx*3}`,stroke:color,fill:'none'}));
      }
    }
    const w = Math.min(300, Math.max(40, a.label.length * 6.5 + 16));
    let x = points.reduce((n,p)=>n+p[0],0)/points.length, y = points.reduce((n,p)=>n+p[1],0)/points.length - 18;
    const origin = [x,y];
    x = Math.max(w/2+4, Math.min(width-w/2-4,x));
    let shift = 0;
    while (labels.some(p=>Math.abs(x-p.x)<(w+p.w)/2 && Math.abs(y-p.y)<26) && shift < 150) { y -= 26;shift += 26; }
    y = Math.max(16, Math.min(height-16,y));labels.push({x,y,w});
    if (shift || Math.abs(origin[0]-x)>1) root.append(make('path',{d:`M${origin[0]},${origin[1]+18} L${x},${y}`,stroke:color,fill:'none',opacity:.5}));
    const group = make('g',{class:`assembly-annotation${a.id===selected?' selected':''}`,transform:`translate(${x},${y})`,tabindex:0,role:'button','aria-label':`${scene.instance}: ${a.label}, ${a.status}`,'data-annotation':a.id,style:`color:${color}`});
    group.append(make('rect',{x:-w/2,y:-12,width:w,height:24,rx:4}),make('text',{'text-anchor':'middle',y:0},a.label.length>44?a.label.slice(0,41)+'…':a.label));
    group.addEventListener('click',event=>{event.stopPropagation();select(a,scene);});
    group.addEventListener('keydown',event=>{if(event.key==='Enter'||event.key===' '){event.preventDefault();select(a,scene);}});
    root.append(group);
  }
}
