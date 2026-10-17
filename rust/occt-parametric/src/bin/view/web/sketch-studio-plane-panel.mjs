// The studio's Planes panel: the model's datum planes drawn in the 3D view
// and listed with how each is defined; New plane builds one of the five
// kinds from a base plane, picked faces or points, with a live preview;
// the inspector edits a plane's driving parameters, starts a sketch on it,
// or deletes it.
import { fromGltf, pickFace } from './sketch-studio-plane.mjs';
import {
  isPathFeature, PLANE_ICONS, PLANE_TYPES, describePlane, drivingParameters, pickPoint, planeEdit, planeOutline, planeType,
} from './sketch-studio-planes.mjs';

// Values within 1e-9 of zero (float noise in unit normals) show as 0.
const format = (value) => (Math.abs(value) < 1e-9 ? '0' : Number(Number(value).toPrecision(6)).toString());
const vectorText = (v) => `(${v.map(format).join(', ')})`;

export function installPlanes(studio, document) {
  const { html } = studio;
  studio.planes = [];
  studio.selectedPlane = null;
  /** The New plane form, or null: {type, name, base, distance, angle, hinge, faces, points, picking, preview, error}. */
  let form = null;
  let previewTimer = 0;
  let previewTurn = 0;

  const icon = (id) => {
    const span = html('span', { 'aria-hidden': 'true' });
    span.innerHTML = `<svg viewBox="0 0 24 24">${PLANE_ICONS[id]}</svg>`;
    return span;
  };
  const planeById = (id) => studio.planes.find((plane) => plane.id === id) ?? null;
  const parameterValue = (id) => studio.modelState?.parameters?.find((p) => p.id === id) ?? null;
  const takenParameters = () => (studio.modelState?.parameters ?? []).map((p) => p.id);
  const takenNames = () => [...studio.planes.map((p) => p.id), ...studio.features.map((f) => f.id)];

  // ── Drawing ─────────────────────────────────────────────────────────
  /** Model bounds centre (mm) and a half size that spans the model. */
  function extent() {
    const bounds = studio.parsed?.bounds;
    if (!bounds) return { center: null, half: 50 };
    const center = fromGltf(bounds.min.map((low, i) => (low + bounds.max[i]) / 2));
    const diagonal = Math.hypot(...bounds.max.map((high, i) => high - bounds.min[i])) * 1000;
    return { center, half: Math.max(10, 0.4 * diagonal) };
  }

  studio.underlay = ({ matrix, width, height, element }) => {
    const nodes = [];
    const { center, half } = extent();
    const draw = (plane, className) => {
      const d = planeOutline(plane, half, matrix, width, height, center);
      if (!d) return;
      const group = element('g', { class: className, 'data-plane': plane.id });
      element('path', { d }, group);
      // The name at the highest corner that is inside the view.
      const corners = [...d.matchAll(/([\d.-]+) ([\d.-]+)/g)].map((m) => [Number(m[1]), Number(m[2])])
        .filter(([x, y]) => x >= 0 && x <= width - 60 && y >= 16 && y <= height);
      if (corners.length) {
        const [x, y] = corners.sort((a, b) => a[1] - b[1])[0];
        const text = element('text', { x: x + 6, y: y + 16 }, group);
        text.textContent = plane.id;
      }
      nodes.push(group);
    };
    for (const plane of studio.planes) {
      if (plane.status !== 'resolved' || plane.hidden) continue;
      if (form?.preview && form.preview.id === plane.id) continue;
      draw(plane, `datum-plane${plane.id === studio.selectedPlane ? ' selected' : ''}`);
    }
    if (form?.preview?.status === 'resolved') draw(form.preview, 'datum-plane preview');
    if (form?.type === 'three_point') {
      const marks = element('g', { class: 'plane-points' });
      for (const point of form.points) {
        const at = point && studio.projectModel(point);
        if (at) element('circle', { cx: at[0], cy: at[1], r: 4 }, marks);
      }
      nodes.push(marks);
    }
    return nodes;
  };

  // ── Panel ───────────────────────────────────────────────────────────
  function renderPlanes() {
    const host = document.getElementById('planes');
    if (!host) return;
    const rows = studio.planes.map((plane) => {
      const failed = plane.status !== 'resolved';
      return html('div', {
        class: 'layer', role: 'button', tabindex: 0,
        'aria-current': plane.id === studio.selectedPlane ? 'true' : 'false',
        title: failed ? plane.error : describePlane(plane.definition),
        onclick: () => selectPlane(plane.id),
        onkeydown: (event) => { if (event.key === 'Enter' || event.key === ' ') selectPlane(plane.id); },
      },
      html('input', {
        type: 'checkbox', checked: !plane.hidden, 'aria-label': `Show ${plane.id}`,
        onclick: (event) => event.stopPropagation(),
        onchange: (event) => { plane.hidden = !event.target.checked; studio.redraw(); },
      }),
      html('span', { class: 'plane-icon' }, icon(PLANE_ICONS[planeType(plane.definition?.kind)] ? planeType(plane.definition?.kind) : 'offset')),
      html('span', { class: 'name' }, `${plane.id} `),
      html('span', { class: `meta${failed ? ' error' : ''}` }, failed ? plane.error : describePlane(plane.definition)));
    });
    studio.fill(host, html('h2', {}, 'Planes'), ...rows, form ? formPanel() : html('button', {
      style: 'margin-top:6px;width:100%', onclick: () => openForm(),
    }, '+ New plane'));
  }
  studio.renderPlanes = renderPlanes;
  studio.isPlanePicking = () => Boolean(form?.picking);

  function selectPlane(id) {
    studio.selectedPlane = studio.selectedPlane === id ? null : id;
    studio.refresh();
  }

  function openForm(type = 'offset', base = 'xy') {
    studio.cancelGesture?.();
    studio.selectedPlane = null;
    form = {
      type, name: nextPlaneName(), base, distance: 10, angle: 30, hinge: 'x',
      path: studio.features.find(isPathFeature)?.id ?? '', position: 0,
      faces: [], points: [null, null, null], picking: null, preview: null, error: null,
    };
    studio.refresh();
    schedulePreview();
    studio.say('Choose how the plane is defined. Its values become parameters you can edit later.');
  }
  studio.openPlaneForm = openForm;

  function nextPlaneName() {
    const taken = new Set(takenNames());
    for (let i = 1; ; i += 1) if (!taken.has(`plane-${i}`)) return `plane-${i}`;
  }

  function closeForm(message) {
    form = null;
    studio.redraw();
    studio.refresh();
    if (message) studio.say(message);
  }

  function formPanel() {
    const field = (label, control) => html('label', {}, label, control);
    const number = (key, label, step = 'any') => field(label, html('input', {
      type: 'number', step, value: form[key], 'aria-label': label,
      oninput: (event) => { form[key] = Number(event.target.value); schedulePreview(); },
    }));
    const types = html('div', { class: 'palette', role: 'radiogroup', 'aria-label': 'Plane type' },
      PLANE_TYPES.map((type) => html('button', {
        role: 'radio', 'aria-checked': form.type === type.id ? 'true' : 'false',
        'aria-pressed': form.type === type.id ? 'true' : 'false', title: type.hint,
        onclick: () => { form.type = type.id; form.faces = []; form.picking = null; form.preview = null; renderPlanes(); schedulePreview(); },
      }, icon(type.id), type.label)));
    const baseChoices = [['xy', 'XY'], ['xz', 'XZ'], ['yz', 'YZ'],
      ...studio.planes.filter((p) => p.status === 'resolved').map((p) => [`datum:${p.id}`, p.id])];
    const base = field('Base plane', html('select', {
      'aria-label': 'Base plane', onchange: (event) => { form.base = event.target.value; schedulePreview(); },
    }, baseChoices.map(([value, text]) => html('option', { value, selected: form.base === value }, text))));
    const pickButton = (index, label) => html('button', {
      'aria-pressed': form.picking?.index === index ? 'true' : 'false',
      title: form.type === 'three_point' ? `Pick point ${index + 1} on the model` : 'Pick a face in the view',
      onclick: () => beginPick(form.type === 'three_point' ? 'point' : 'face', index),
    }, form.picking?.index === index ? (form.type === 'three_point' ? '…' : 'Click a face…') : label);
    const facesDone = (index) => form.faces[index]
      ? html('span', { style: 'color:var(--muted)' }, `Face on ${form.faces[index].face_support.input} · normal ${vectorText(form.faces[index].normal)}`)
      : null;
    const controls = [];
    if (form.type === 'offset') controls.push(base, number('distance', 'Distance (mm)'));
    if (form.type === 'angle') {
      controls.push(base, number('angle', 'Angle (°)'), field('Hinge', html('select', {
        'aria-label': 'Hinge', onchange: (event) => { form.hinge = event.target.value; schedulePreview(); },
      }, html('option', { value: 'x', selected: form.hinge === 'x' }, 'Base x axis'),
      html('option', { value: 'y', selected: form.hinge === 'y' }, 'Base y axis'))));
    }
    if (form.type === 'face') {
      controls.push(html('div', { class: 'row' }, pickButton(0, form.faces[0] ? 'Pick again' : 'Pick a face'), facesDone(0)),
        number('distance', 'Distance from the face (mm)'));
    }
    if (form.type === 'midplane') {
      controls.push(html('div', { class: 'row' }, pickButton(0, form.faces[0] ? 'First face again' : 'Pick the first face'), facesDone(0)),
        html('div', { class: 'row' }, pickButton(1, form.faces[1] ? 'Second face again' : 'Pick the second face'), facesDone(1)));
    }
    if (form.type === 'path') {
      const paths = studio.features.filter(isPathFeature);
      controls.push(field('Path', html('select', {
        'aria-label': 'Path', onchange: (event) => { form.path = event.target.value; schedulePreview(); },
      }, paths.length ? paths.map((f) => html('option', { value: f.id, selected: form.path === f.id }, f.id))
        : html('option', { value: '' }, 'No sketch wire or helix yet'))),
      number('position', 'Position along the path (0 start – 1 end)'),
      html('p', { style: 'margin:0;color:var(--muted)' },
        'Sketches on this plane sit where the path crosses it and move with the path as its constraints change. Use 0 for a sweep profile and 1 for an end section.'));
    }
    if (form.type === 'three_point') {
      form.points.forEach((point, index) => {
        controls.push(html('div', { class: 'point-row' }, html('span', {}, `P${index + 1}`),
          ...[0, 1, 2].map((axis) => html('input', {
            type: 'number', step: 'any', value: point ? format(point[axis]) : '', 'aria-label': `Point ${index + 1} ${'xyz'[axis]} (mm)`,
            oninput: (event) => {
              const next = form.points[index] ? [...form.points[index]] : [0, 0, 0];
              next[axis] = Number(event.target.value);
              form.points[index] = next;
              schedulePreview();
            },
          })),
          pickButton(index, '⌖')));
      });
    }
    const status = form.error
      ? html('p', { class: 'form-error' }, form.error)
      : form.preview?.status === 'resolved'
        ? html('p', { style: 'margin:0;color:var(--muted)' }, `Origin ${vectorText(form.preview.origin_mm)} mm · normal ${vectorText(form.preview.normal)}`)
        : null;
    return html('div', { class: 'form' },
      html('h2', { style: 'margin:0' }, 'New plane'),
      types,
      field('Name', html('input', { value: form.name, 'aria-label': 'Plane name', oninput: (event) => { form.name = event.target.value; schedulePreview(); } })),
      ...controls,
      status,
      html('div', { class: 'row' },
        html('button', { disabled: Boolean(form.error) || !form.preview, onclick: create }, 'Create plane'),
        html('button', { onclick: () => closeForm('New plane canceled.') }, 'Cancel')));
  }

  // ── Preview and apply ───────────────────────────────────────────────
  function edit() {
    return planeEdit(form.type, form, studio.planes, takenParameters(), studio.features);
  }

  function schedulePreview() {
    clearTimeout(previewTimer);
    previewTimer = setTimeout(preview, 150);
  }

  async function preview() {
    if (!form) return;
    const turn = ++previewTurn;
    let request;
    try {
      request = edit();
    } catch (error) {
      form.error = error.message;
      form.preview = null;
      renderPlanes();
      studio.redraw();
      return;
    }
    try {
      const result = await studio.api('/api/datum', {
        method: 'POST',
        body: JSON.stringify({
          version: studio.version, instance: studio.instance, expected: studio.datums ?? [],
          datum: request.datum, parameters: request.parameters, apply: false,
        }),
      });
      if (turn !== previewTurn || !form) return;
      const plane = (result.datum_planes ?? []).find((p) => p.id === request.datum.id);
      form.preview = plane ?? null;
      form.error = plane?.status === 'resolved' ? null : plane?.error ?? 'The plane did not resolve.';
    } catch (error) {
      if (turn !== previewTurn || !form) return;
      form.preview = null;
      form.error = error.message;
    }
    renderPlanes();
    studio.redraw();
  }

  async function create() {
    try {
      const request = edit();
      studio.busy = true;
      await studio.api('/api/datum', {
        method: 'POST',
        body: JSON.stringify({
          version: studio.version, instance: studio.instance, expected: studio.datums ?? [],
          datum: request.datum, parameters: request.parameters, apply: true,
        }),
      });
      const id = request.datum.id;
      form = null;
      await studio.reload();
      studio.selectedPlane = id;
      studio.refresh();
      studio.say(`Plane ${id} added. Its values are parameters: edit them in the inspector.`);
    } catch (error) {
      if (form) form.error = error.message;
      studio.say(`Plane not added: ${error.message}`, true);
      renderPlanes();
    } finally {
      studio.busy = false;
    }
  }

  // ── Picking faces and points ────────────────────────────────────────
  function beginPick(kind, index) {
    studio.cancelGesture?.();
    form.picking = { kind, index };
    renderPlanes();
    studio.say(kind === 'face' ? 'Click a flat face of the model. Escape cancels.' : 'Click a point on the model; corners snap. Escape cancels.');
  }

  /** Handles a click in the view while picking; true when it was used. */
  studio.planePick = async (event) => {
    if (!form?.picking) return false;
    const solid = studio.scenes.find((s) => s.kind === 'solid' && Array.isArray(s.mesh));
    if (!solid) {
      studio.say('No solid is shown to pick from.', true);
      return true;
    }
    const ray = studio.rayAt(event.clientX, event.clientY);
    const { kind, index } = form.picking;
    try {
      if (kind === 'face') {
        const face = pickFace(solid.mesh, ray);
        if (face < 0) {
          studio.say('That click missed the model. Click a flat face.', true);
          return true;
        }
        const picked = await studio.api('/api/sketch-face-pick', {
          method: 'POST',
          body: JSON.stringify({ version: studio.version, instance: studio.instance, output: solid.feature, face }),
        });
        if (!form) return true;
        form.faces[index] = picked;
      } else {
        // Snap within 16 pixels of a corner: millimetres per pixel at the
        // target for the viewer's 45° field of view.
        const mmPerPixel = (2 * studio.camera.distance * 1000 * Math.tan(Math.PI / 8)) / Math.max(1, studio.viewport().height);
        const hit = pickPoint(solid.mesh, ray, 16 * mmPerPixel);
        // A pick is only as exact as the click: keep hundredths of a millimetre.
        const point = hit && hit.map((v) => Math.round(v * 100) / 100);
        if (!point) {
          studio.say('That click missed the model.', true);
          return true;
        }
        form.points[index] = point;
      }
      form.picking = null;
      // Move on to the next empty pick.
      if (form.type === 'midplane' && !form.faces[1] && index === 0) form.picking = { kind: 'face', index: 1 };
      if (form.type === 'three_point') {
        const next = form.points.findIndex((p) => !p);
        if (next >= 0) form.picking = { kind: 'point', index: next };
      }
      studio.say(form.picking ? 'Picked. Click the next one.' : 'Picked.');
      renderPlanes();
      schedulePreview();
    } catch (error) {
      studio.say(`Pick rejected: ${error.message}`, true);
    }
    return true;
  };

  // ── Inspector ───────────────────────────────────────────────────────
  studio.renderPlaneInspector = (host) => {
    const plane = planeById(studio.selectedPlane);
    if (!plane) return false;
    const rows = [['Defined', describePlane(plane.definition)]];
    if (plane.status === 'resolved') {
      rows.push(['Origin', `${vectorText(plane.origin_mm)} mm`], ['Normal', vectorText(plane.normal)]);
    } else {
      rows.push(['Error', plane.error]);
    }
    const driven = drivingParameters(plane.definition?.kind).map((driver) => {
      const parameter = parameterValue(driver.id);
      if (!parameter) return null;
      if (driver.vector) {
        const values = parameter.vector ?? [0, 0, 0];
        return html('label', {}, `${driver.label} · ${driver.id} (${driver.unit})`, html('div', { class: 'point-row' },
          ...[0, 1, 2].map((axis) => html('input', {
            type: 'number', step: 'any', value: format(values[axis]), 'aria-label': `${driver.label} ${'xyz'[axis]}`,
            onchange: (event) => {
              const next = [...values];
              next[axis] = Number(event.target.value);
              setParameter(driver.id, next);
            },
          }))));
      }
      const shown = driver.toView ? driver.toView(parameter.value) : parameter.value;
      return html('label', {}, `${driver.label} · ${driver.id} (${driver.unit})`, html('input', {
        type: 'number', step: 'any', value: format(shown), 'aria-label': driver.label,
        onchange: (event) => {
          const value = Number(event.target.value);
          if (!event.target.value.trim() || !Number.isFinite(value)) return;
          setParameter(driver.id, driver.fromView ? driver.fromView(value) : value);
        },
      }));
    }).filter(Boolean);
    const users = studio.features.filter((f) => studio.definitions[f.id]?.datum_plane === plane.id).map((f) => f.id);
    studio.fill(host,
      html('h2', {}, `Plane ${plane.id}`),
      html('dl', {}, ...rows.flatMap(([term, value]) => [html('dt', {}, term), html('dd', {}, value)])),
      driven.length ? html('div', { class: 'form' }, html('h2', { style: 'margin:0' }, 'Driven by'), ...driven) : null,
      users.length ? html('p', { style: 'color:var(--muted)' }, `Sketches on it: ${users.join(', ')}`) : null,
      html('div', { class: 'row', style: 'display:flex;gap:6px;flex-wrap:wrap;margin-top:10px' },
        html('button', { disabled: plane.status !== 'resolved', onclick: () => studio.newSketchOn?.(plane.id) }, 'Sketch on this plane'),
        html('button', { onclick: () => openForm('offset', `datum:${plane.id}`), disabled: plane.status !== 'resolved' }, 'Offset from it'),
        html('button', { onclick: () => removePlane(plane.id), disabled: users.length > 0, title: users.length ? 'Sketches use this plane' : 'Delete this plane' }, 'Delete')));
    return true;
  };

  async function setParameter(id, value) {
    try {
      studio.busy = true;
      await studio.api('/api/parameters', { method: 'POST', body: JSON.stringify({ set: { [id]: value } }) });
      await studio.reload();
      studio.say(`${id} updated; planes and sketches built on it followed.`);
    } catch (error) {
      studio.say(`${id} not changed: ${error.message}`, true);
      studio.refresh();
    } finally {
      studio.busy = false;
    }
  }

  async function removePlane(id) {
    try {
      studio.busy = true;
      await studio.api('/api/datum', {
        method: 'POST',
        body: JSON.stringify({ version: studio.version, instance: studio.instance, expected: studio.datums ?? [], remove: id, apply: true }),
      });
      studio.selectedPlane = null;
      await studio.reload();
      studio.say(`Plane ${id} deleted.`);
    } catch (error) {
      studio.say(`Plane not deleted: ${error.message}`, true);
    } finally {
      studio.busy = false;
    }
  }

  const previousKey = studio.onKey;
  studio.onKey = (event) => {
    if (event.key === 'Escape' && form?.picking) {
      form.picking = null;
      renderPlanes();
      studio.say('Picking canceled.');
      return true;
    }
    if (event.key === 'Escape' && studio.selectedPlane) {
      studio.selectedPlane = null;
      studio.refresh();
      return true;
    }
    return previousKey?.(event) ?? false;
  };

  return { openForm, form: () => form, preview };
}
