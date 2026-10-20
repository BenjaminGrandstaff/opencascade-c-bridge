// Names who changed the model through a shared session (an AI client over
// MCP), while that change is the version on screen; otherwise empty.
export function changeNotice(state) {
  const change = state?.last_change;
  return change && change.version === state.version ? `${change.author} changed the model: ${change.message}` : '';
}
