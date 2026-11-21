import { api } from '../api.js';
import { el, append, clear, setProp } from '../dom.js';
let draft = '', busy = false, error = '', requestId = null;
const completed = new Set();
export function startSessionForm(t, refresh, { home = true } = {}) {
  const message = el('p', { class: 'inline-error', role: 'alert', hidden: !error }, [error]);
  const label = t(home ? 'home.prompt' : 'sessions.message_label');
  const input = el('textarea', { id: 'message-input', rows: 3, maxlength: '8000', placeholder: t('sessions.placeholder'), disabled: busy, oninput: event => { draft = event.currentTarget.value; } });
  setProp(input, 'value', draft);
  const button = el('button', { class: 'primary-button', type: 'submit', disabled: busy }, [t(home ? 'home.start' : 'sessions.send')]);
  const form = el('form', { class: 'start-session-form', onsubmit: async event => {
    event.preventDefault(); const text = input.value.trim(); if (!text || busy) return;
    busy = true; error = ''; setProp(input, 'disabled', true); setProp(button, 'disabled', true); setProp(message, 'hidden', true);
    try {
      const result = await api('/sessions', { method: 'POST', body: JSON.stringify({ text }) });
      draft = ''; requestId = result.request_id;
      if (completed.delete(requestId)) busy = false;
      if (result.session_id) location.hash = `#/sessions/${result.session_id}`;
    } catch (failure) { busy = false; error = failure.code === 'model_not_configured' ? t('sessions.model_not_configured') : failure.message; }
    if (['#/home', '#/sessions'].includes(location.hash)) await refresh();
  } }, [el('label', { for: 'message-input' }, [label]), input, el('div', { class: 'composer-row' }, [el('span', { class: 'muted' }, [t(busy ? 'sessions.still_answering' : 'sessions.no_mirror')]), button]), message]);
  return form;
}
export function handleStartEvent(type, raw, t) {
  let data; try { data = JSON.parse(raw); } catch { return; }
  if (type === 'chat.reply' || type === 'chat.error') {
    completed.add(data.request_id); if (data.request_id === requestId) { busy = false; requestId = null; }
    if (type === 'chat.error') error = t(data.error === 'model_not_configured' ? 'sessions.model_not_configured' : data.error === 'supervisor_unavailable' ? 'sessions.supervisor_unavailable' : 'sessions.chat_failed');
  }
  if (['chat.pending', 'chat.reply'].includes(type) && data.session_id && ['#/sessions', '#/home'].includes(location.hash)) location.hash = `#/sessions/${data.session_id}`;
}
