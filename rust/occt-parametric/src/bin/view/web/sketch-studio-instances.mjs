// The studio's instance menu. Every primary instance shares the family's
// sketches; the chosen instance supplies the parameter values its sketches
// are solved and drawn with, its placement, and the faces sketches attach
// to. The menu shows only when the model has more than one instance.

/** Menu entries for `/api/instances`: `{value, label, title}`, in its order. */
export function instanceOptions(instances) {
  return (instances ?? []).map(({ id, overrides = [] }) => ({
    value: id,
    label: overrides.length ? `${id} · ${overrides.length} override${overrides.length === 1 ? '' : 's'}` : id,
    title: overrides.length ? `Overrides: ${overrides.join(', ')}` : 'Family defaults',
  }));
}

/**
 * Why the instance cannot change now, or null: a pending model request or a
 * draft with edits (its preview belongs to the current instance).
 */
export function instanceBlock(studio) {
  if (studio.modelPending) return 'Wait for the model to finish updating.';
  if (studio.hasDraftHistory?.() || studio.draft?.dirty) return `Apply or discard the draft of ${studio.draft?.feature} before changing instance.`;
  return null;
}

/** Adds the instance menu to the header and keeps it in step with loads. */
export function installInstances(studio, document) {
  const { html } = studio;
  const select = html('select', { id: 'instance', 'aria-label': 'Instance', title: 'Instance whose parameter values the sketches are solved with', hidden: true });
  const group = document.getElementById('look-at')?.parentElement;
  group?.prepend(select);
  document.head.append(html('style', {}, '#instance { max-width: 180px; } #bar .group button { white-space: nowrap; }'));

  function render() {
    const options = instanceOptions(studio.instances);
    select.hidden = options.length < 2;
    const key = options.map((o) => `${o.value}\u0000${o.label}`).join('\u0001');
    if (select.dataset.key !== key) {
      select.replaceChildren(...options.map((o) => html('option', { value: o.value, title: o.title }, o.label)));
      select.dataset.key = key;
    }
    select.value = studio.instance ?? '';
    select.title = options.find((o) => o.value === studio.instance)?.title ?? '';
  }

  select.addEventListener('change', async () => {
    const wanted = select.value;
    const blocked = instanceBlock(studio);
    if (blocked) {
      select.value = studio.instance;
      studio.say(blocked, true);
      return;
    }
    if (studio.draft) studio.drawing?.discard(true);
    studio.instance = wanted;
    try {
      await studio.reload();
      const option = instanceOptions(studio.instances).find((o) => o.value === wanted);
      studio.say(`Showing ${wanted} (${option?.title.toLowerCase() ?? 'family defaults'}). Sketch edits change every instance.`);
    } catch (error) {
      studio.say(error.message, true);
    }
  });

  const load = studio.onLoad;
  studio.onLoad = () => {
    render();
    load?.();
  };
}
