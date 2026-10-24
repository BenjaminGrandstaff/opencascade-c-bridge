// New design from a template: the engine's parameterized starting designs
// (tools/model/templates), chosen by what they are for and filled from a
// form. Preview builds and checks the design without changing the model;
// Start replaces the studio's model with it as one undoable edit, and only
// Save writes it to the model file.

/** The inputs to send: each entered value, checked as the engine checks it,
 * or the default when left blank. Throws naming the first bad input. */
export function templateInputs(template, entered) {
  const inputs = {};
  for (const item of template.inputs) {
    const raw = entered[item.id];
    if (raw === undefined || raw === '') continue;
    if (item.kind === 'choice') {
      if (!item.choices.includes(raw)) throw new Error(`${item.id} must be one of ${item.choices.join(', ')}.`);
      inputs[item.id] = raw;
      continue;
    }
    const value = Number(raw);
    if (!Number.isFinite(value)) throw new Error(`${item.id} must be a number${item.kind === 'length' ? ' in millimeters' : ''}.`);
    if ((item.minimum ?? -Infinity) > value || value > (item.maximum ?? Infinity)) throw new Error(`${item.id} must be between ${item.minimum} and ${item.maximum}.`);
    if (item.allowed && !item.allowed.includes(value)) throw new Error(`${item.id} must be one of ${item.allowed.join(', ')}.`);
    inputs[item.id] = value;
  }
  return inputs;
}

/** Adds the Templates button and its sheet to the studio's top bar. */
export function installTemplates(studio, document) {
  const { html } = studio;
  const sheet = html('section', { class: 'sheet', role: 'dialog', 'aria-label': 'New from template', hidden: true });
  document.body.append(sheet);
  let templates = null, chosen = null, entered = {}, turn = 0;
  const status = html('p', { role: 'status' });
  const say = (text) => { status.textContent = text; };
  const close = () => {
    sheet.hidden = true;
    turn++;
    if (studio.solidPreview) { studio.setSolidPreview?.(null); studio.redraw?.(); }
  };
  const header = (title) => html('header', {}, html('h2', { style: 'margin:0' }, title), html('button', { onclick: close }, 'Close'));

  function list() {
    chosen = null;
    studio.fill(sheet, header('New from template'),
      html('p', { style: 'color:var(--muted);margin-top:0' }, 'Starting designs with their checks built in. Pick one by what it is for, fill its inputs, preview, then start. Undo returns to the current model.'),
      templates.map((template) => html('div', { class: 'linked-edit', style: 'border-top:1px solid var(--line);padding-top:6px' },
        html('strong', {}, template.title),
        html('span', {}, template.summary),
        html('span', { style: 'color:var(--muted)' }, `Use for: ${template.use_when}`),
        html('span', { style: 'color:var(--muted)' }, `Not for: ${template.not_for}`),
        html('button', { onclick: () => form(template) }, `Use ${template.title}`))),
      status);
  }

  function form(template) {
    chosen = template;
    entered = {};
    turn++;
    const rows = template.inputs.map((item) => {
      const label = `${item.id}${item.kind === 'length' ? ' (mm)' : ''}`;
      const control = item.kind === 'choice'
        ? html('select', { 'aria-label': item.id, onchange: (event) => { entered[item.id] = event.target.value; } },
          item.choices.map((choice) => html('option', { value: choice, ...(choice === item.default ? { selected: true } : {}) }, choice)))
        : html('input', { type: 'number', step: 'any', 'aria-label': item.id, placeholder: String(item.default),
          ...(item.minimum !== undefined ? { min: item.minimum } : {}), ...(item.maximum !== undefined ? { max: item.maximum } : {}),
          oninput: (event) => { entered[item.id] = event.target.value.trim(); } });
      return html('label', {}, label, control, html('span', { style: 'color:var(--muted)' }, item.description ?? ''));
    });
    studio.fill(sheet, header(template.title),
      html('p', { style: 'margin-top:0' }, template.summary),
      html('p', { style: 'color:var(--muted)' }, `Checks: ${template.checks.join('; ')}.`),
      html('div', { class: 'linked-edit' }, rows),
      html('div', { style: 'display:flex;gap:6px;flex-wrap:wrap;margin-top:8px' },
        html('button', { onclick: () => submit(false) }, 'Preview design'),
        html('button', { onclick: () => submit(true) }, 'Start from template'),
        html('button', { onclick: list }, 'Back to templates')),
      html('p', { style: 'color:var(--muted)' }, 'Start replaces the studio’s model with this design. Undo returns to the current model; Save writes the design to the model file.'),
      status);
    say('Blank inputs use their defaults.');
  }

  async function submit(apply) {
    const mine = ++turn, template = chosen;
    try {
      const inputs = templateInputs(template, entered);
      say(apply ? 'Starting the design and checking it…' : 'Building the design and checking it…');
      const response = await studio.api('/api/template', { method: 'POST', body: JSON.stringify({ version: studio.version, template: template.id, inputs, apply }) });
      if (mine !== turn || chosen !== template) return;
      if (apply) {
        sheet.hidden = true;
        studio.setSolidPreview?.(null);
        studio.previewScenes = {};
        await studio.reload();
        document.getElementById('fit')?.click();
        studio.say(`Started from ${template.title}. Undo returns to the previous model; Save writes this design to the model file.`);
        return;
      }
      if (!response.can_apply) { say(`This design does not pass: ${response.error}`); return; }
      studio.setSolidPreview?.(response.gltf);
      document.getElementById('fit')?.click();
      say(`${template.title} builds and passes its checks. Start to use it.`);
    } catch (error) {
      if (mine === turn) say(error.message);
    }
  }

  async function open() {
    if (studio.modelPending) return;
    if (!sheet.hidden) { close(); return; }
    if (studio.draft?.dirty) {
      studio.fill(sheet, header('New from template'), html('p', {}, `Apply or discard the sketch draft of ${studio.draft.feature} first.`));
      sheet.hidden = false;
      return;
    }
    sheet.hidden = false;
    try {
      templates ??= (await studio.api('/api/templates')).templates;
      list();
    } catch (error) {
      studio.fill(sheet, header('New from template'), html('p', {}, error.message));
    }
  }

  const bar = document.getElementById('bar');
  const button = html('button', { title: 'Start a new design from a template with built-in checks', onclick: open }, 'Templates');
  bar.insertBefore(html('div', { class: 'group' }, button), document.getElementById('draft-actions'));
  return { open, sheet };
}
