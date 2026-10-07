// Links native annotation scenes to the studio's existing parameter edit queue.
// Requests are versioned and scoped: a late reply cannot replace a newer part.
export function editableControls(annotation, parameters) {
  const wanted = new Set(annotation?.parameters ?? []);
  return parameters.filter(p => wanted.has(p.id) && ['scalar', 'integer', 'boolean', 'choice'].includes(p.kind));
}

export function createAnnotationController({ request, mount, changed = () => {} }) {
  let serial = 0, viewer = null, mounting = null, key = '', pending = '', rejected = '';
  async function show(data) {
    if (!viewer) {
      try { viewer = await (mounting ??= Promise.resolve(mount(data))); }
      finally { mounting = null; }
    }
    return viewer;
  }
  return {
    get viewer() { return viewer; },
    async refresh(instance, version) {
      const next = JSON.stringify([instance, version]);
      if (!instance) { serial += 1; key = ''; pending = ''; changed('Select an instance to inspect its dimensions and sketches.'); return; }
      if (next === key || next === pending || next === rejected) return;
      rejected = '';
      const turn = ++serial;
      pending = next;
      changed('Updating dimensions and constraints…');
      try {
        const data = await request(instance);
        if (turn !== serial) return;
        if (data.version !== version) { changed('Model changed; updating the view…'); return; }
        await show(data);
        if (turn !== serial) return;
        viewer.update(data);
        key = next;
        changed('');
      } catch (error) {
        if (turn === serial) changed(`Annotations unavailable: ${error.message}`);
      } finally { if (turn === serial) pending = ''; }
    },
    async preview(data) {
      const turn = ++serial;
      key = ''; pending = '';
      rejected = JSON.stringify([data.scenes[0]?.instance, data.version]);
      await show(data);
      if (turn !== serial) return;
      viewer.update(data);
      changed('Rejected edit preview — the accepted model is unchanged.', false);
    },
    invalidate() { serial += 1; key = ''; pending = ''; rejected = ''; },
    fit() { viewer?.fit(); },
  };
}
