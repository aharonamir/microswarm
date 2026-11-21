import { api } from '../api.js';
import { el, append } from '../dom.js';
import { startSessionForm } from '../components/start-session.js';
export async function renderNewSession(root, t, refresh) {
  try {
    const [active, ended] = await Promise.all([api('/sessions?group=active'), api('/sessions?group=ended')]);
    const links = (items, closed = false) => items.map(item => el('a', {class: 'session-link', href: closed ? `#/trace/${item.root_id || item.id}` : `#/sessions/${item.id}`}, [
      el('strong', {}, [item.title || t('sessions.new')]), el('span', {class: 'status-line'}, [t(item.status_words?.key || 'status.unknown', item.status_words?.args)]),
    ]));
    append(root, el('aside', {class: 'page-rail panel', 'aria-label': t('sessions.your_sessions')}, [
      el('h2', {class: 'section-title'}, [t('sessions.your_sessions')]),
      el('div', {class: 'session-list'}, active.sessions?.length ? links(active.sessions) : [el('p', {class: 'muted'}, [t('sessions.none')])]),
      ...(ended.sessions?.length ? [el('h2', {class: 'section-title rail-heading'}, [t('sessions.ended_today')]), el('div', {class: 'session-list'}, links(ended.sessions, true))] : []),
    ]), el('section', {class: 'page-main panel new-session-page'}, [el('h1', {class: 'page-title'}, [t('sessions.new_title')]), el('p', {class: 'muted'}, [t('sessions.chat_helper')]), startSessionForm(t, refresh, {home: false})]));
  } catch (error) { append(root, el('p', {role: 'alert'}, [error.message])); }
}
