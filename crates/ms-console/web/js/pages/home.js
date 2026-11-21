import { api } from '../api.js';
import { el, append, clear, setProp, setText } from '../dom.js';
import { localTime, duration } from '../format.js';
import { avatar, avatarStack } from '../components/avatar.js';
import { statusPill } from '../components/pill.js';
import { sentenceNodes } from '../components/step.js';
import { startSessionForm } from '../components/start-session.js';
import { markDecisionHere } from './approvals.js';
export async function renderHome(root, t, refresh = () => renderHome(root, t)) {
  clear(root); let data;
  try { data = await api('/home'); } catch (error) { append(root, el('section', { class: 'panel', role: 'alert' }, [error.message])); return; }
  const needs = data.summary?.needs_you || 0, running = data.summary?.running || 0;
  const needsCards = (data.needs_you || []).length ? data.needs_you.map(item => approvalCard(item, t, refresh)) : [el('p', { class: 'home-empty' }, [t('home.nothing_needs_you')])];
  const workspace = el('div', { class: 'home-page' }, [
    el('header', {}, [el('h1', { class: 'home-summary' }, [t(needs === 1 ? 'home.needs_one' : 'home.needs_many', { count: needs }), ' ', t(running === 1 ? 'home.running_one' : 'home.running_many', { count: running })]), el('p', { class: 'muted' }, [data.system?.containers > 0 ? t('home.system', { containers: data.system.containers, approvals: data.system.approvals_today || 0 }) : t('home.system_unknown')])]),
    el('section', { class: 'home-start panel' }, [el('h2', {}, [t('home.start_session')]), startSessionForm(t, refresh)]),
    el('section', { class: 'home-needs' }, [el('h2', {}, [t('home.needs_title')]), ...needsCards]),
    sessionsTable(data.running || [], t('home.running'), false, t),
    sessionsTable(data.ended_today || [], t('home.ended_today'), true, t),
  ]);
  append(root, workspace);
}
function approvalCard(item, t, refresh) {
  const error = el('p', { class: 'inline-error', role: 'alert', hidden: true });
  const buttons = ['deny', 'approve'].map(action => el('button', { class: action === 'approve' ? 'primary-button' : 'secondary-button deny-button', onclick: async () => {
    buttons.forEach(button => setProp(button, 'disabled', true));
    try { await api(`/approvals/${encodeURIComponent(item.code)}/${action}`, { method: 'POST', body: '{}' }); markDecisionHere(item.approval_id); await refresh(); }
    catch (failure) { setText(error, failure.code === 'already_resolved' ? t('approvals.already_decided') : failure.message); setProp(error, 'hidden', false); buttons.forEach(button => setProp(button, 'disabled', false)); }
  } }, [t(`approvals.${action}`)]));
  const reach = item.runs_in || {};
  return el('article', { class: 'home-approval' }, [avatar(item.agent), el('div', {}, [el('h3', {}, sentenceNodes(item.sentence || { key: 'step.other', args: {} }, t)), el('p', { class: 'muted' }, [el('a', { href: `#/sessions/${item.session_id}` }, [item.session_title || t('sessions.new')]), ' · ', t('approval.container'), ' · ', t(reach.network === 'none' || !reach.network ? 'reach.no_network' : `reach.network.${reach.network}`), ' · ', t('approval.expires', { time: localTime(item.expires_at) })]), el('a', { href: `#/approvals/${item.code}` }, [t('approvals.more_details')]), error]), el('div', { class: 'home-approval-actions' }, buttons)]);
}
function sessionsTable(rows, title, ended, t) {
  const latest = row => {
    let value = ended ? row.last_line : row.latest?.text_or_sentence;
    if (value && typeof value === 'object' && !value.key) value = value.text_or_sentence;
    return value?.key ? sentenceNodes(value, t) : [typeof value === 'string' ? value : t('home.no_latest')];
  };
  return el('section', { class: 'home-sessions' }, [el('h2', {}, [title]), rows.length ? el('div', { class: 'home-table-scroll', tabindex: '0', role: 'region', 'aria-label': title }, [el('table', { class: 'home-session-table' }, [
    el('caption', { class: 'visually-hidden' }, [title]), el('thead', {}, [el('tr', {}, ['home.session', 'home.status', 'home.team', 'home.latest', ended ? 'home.ended_at' : 'home.elapsed'].map(key => el('th', { scope: 'col' }, [t(key)])))]),
    el('tbody', {}, rows.map(row => el('tr', {}, [el('td', {}, [el('a', { href: `#/sessions/${row.session_id}${ended ? '/trajectory' : ''}` }, [row.title || t('sessions.new')]), el('span', { class: 'session-meta' }, [row.channel || ''])]), el('td', {}, [statusPill(ended ? { key: row.outcome_key || 'status.ended' } : row.status, t)]), el('td', {}, [avatarStack(row.agents || [], t)]), el('td', { class: 'home-latest' }, latest(row)), el('td', { class: 'mono' }, [ended ? localTime(row.ended_at) : duration(Date.now() - Date.parse(row.started_at || new Date()))])]))),
  ])]) : el('p', { class: 'home-empty' }, [t(ended ? 'home.no_ended' : 'home.no_running')])]);
}
