// Timeline controls operate on accepted model state, separately from sketch drafts.
export function installFeatureHistory(studio, document) {
  const {html} = studio;
  const host = html('section', {'aria-label': 'Feature history'});
  document.getElementById('solid-features').append(host);
  const blocked = () => Boolean(studio.modelPending || studio.hasDraftHistory?.() || studio.draft?.previewing || studio.activeLayer?.()?.local || studio.picking || studio.isPlanePicking?.());
  async function apply(change) {
    if (blocked()) { studio.say('Apply or discard the draft and finish picking before changing feature history.', true); return; }
    const request = {version: studio.version, expected: studio.featureHistory ?? {}, expected_order: studio.features.map(f => f.id), ...change};
    try {
      await studio.runModelOperation('Updating feature history', async () => {
        await studio.api('/api/feature-history', {method: 'POST', body: JSON.stringify(request)});
        await studio.reload();
      });
      studio.say('Feature history updated. Undo restores the previous timeline.');
    } catch (error) { studio.say(error.message, true); }
  }
  let previous = null;
  function render() {
    const disabled = blocked();
    // Pointer/draft refreshes should not rebuild every feature row unless
    // model state or the availability of its controls actually changed.
    const snapshot = [studio.version, studio.features, studio.featureStates, studio.featureHistory, disabled];
    if (previous && snapshot.every((value, index) => value === previous[index])) return;
    previous = snapshot;
    const states = new Map((studio.featureStates ?? []).map(s => [s.feature, s]));
    const button = (label, title, change, extra = false) => html('button', {title, 'aria-label': title, disabled: disabled || extra, onclick: () => apply(change)}, label);
    const rows = (studio.features ?? []).map((feature, index, features) => {
      const state = states.get(feature.id) ?? {state: 'active'};
      const suppressed = (studio.featureHistory?.suppressed ?? []).includes(feature.id);
      const reason = state.by ? ` · needs ${state.by}` : state.passes ? ` · passes ${state.passes}` : '';
      return html('li', {}, html('strong', {}, feature.id), html('small', {}, ` · ${state.state.replaceAll('_', ' ')}${reason}`),
        html('div', {},
          button('Checkout', `Checkout at ${feature.id}`, {action: 'checkout', head: feature.id}, studio.featureHistory?.head === feature.id),
          button(suppressed ? 'Restore' : 'Suppress', `${suppressed ? 'Restore' : 'Suppress'} ${feature.id}`, {action: 'suppress', feature: feature.id, suppressed: !suppressed}),
          button('↑', `Move ${feature.id} earlier`, {action: 'move', feature: feature.id, after: index > 1 ? features[index - 2].id : null}, index === 0),
          button('↓', `Move ${feature.id} later`, {action: 'move', feature: feature.id, after: features[index + 1]?.id ?? null}, index === features.length - 1)));
    });
    studio.fill(host, html('h3', {}, 'Feature history'),
      html('p', {}, studio.featureHistory?.head ? `Checked out at ${studio.featureHistory.head}` : 'Full timeline'),
      button('Restore latest', 'Restore the full feature timeline', {action: 'checkout', head: null}, !studio.featureHistory?.head),
      html('ol', {}, ...rows));
  }
  studio.renderFeatureHistory = render;
  render();
  return {apply, render};
}
