import { captureSessionUi, renderSession, sessionState } from "./pages/session.js";

export function handleSessionLiveEvent(type, raw, sessionId, root, t, refresh = null) {
  if (!sessionId || !["agent", "supervisor"].includes(type)) return false;
  let payload;
  try { payload = JSON.parse(raw); } catch { return true; }
  const event = payload?.event && typeof payload.event === "object" ? payload.event : payload;
  const targetId = event.root_session_id || event.root_id || event.session_id || payload.root_session_id || payload.session_id;
  const state = sessionState(sessionId);
  if (targetId && targetId !== sessionId && !state.agentIds?.has(targetId)) return false;
  if (!targetId && type !== "supervisor") return false;
  if (state.liveTimer) return true;
  const delay = Math.max(0, 1000 - (Date.now() - (state.lastLiveAt || 0)));
  state.liveTimer = setTimeout(async () => {
    state.liveTimer = null;
    state.lastLiveAt = Date.now();
    const snapshot = captureSessionUi(root);
    if (refresh) await refresh(snapshot);
    else await renderSession(root, sessionId, t, { live: true, snapshot });
  }, delay);
  return true;
}
