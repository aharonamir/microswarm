import { api } from '../api.js';
import { el, append, remove, setProp, setText, openModal, closeDialog, focus } from '../dom.js';
import { sentenceNodes } from './step.js';
import { avatar } from './avatar.js';
import { markDecisionHere } from '../pages/approvals.js';
export function openApprovalSheet(item, t, onChange) {
  const role=item.agent?.role||item.requesting_role||t('sessions.member');
  const command=item.sentence?.args?.command||item.tool_call?.arguments?.command;
  const reach=item.runs_in||{};
  const error=el('p',{class:'inline-error',role:'status',hidden:true});
  const dialog=el('dialog',{class:'approval-sheet','aria-labelledby':'sheet-title'},[]);
  const close=el('button',{class:'secondary-button',type:'button',onclick:()=>closeDialog(dialog)},[t('common.close')]);
  const actions=['deny','approve'].map(action=>el('button',{class:action==='approve'?'primary-button':'secondary-button deny-button',type:'button',onclick:async()=>{
    actions.forEach(button=>setProp(button,'disabled',true));
    try{await api(`/approvals/${encodeURIComponent(item.code)}/${action}`,{method:'POST',body:'{}'});markDecisionHere(item.id);closeDialog(dialog);await onChange();focus(document.querySelector('#message-input'));}
    catch(failure){setText(error,failure.code==='already_resolved'?t('approvals.already_decided'):failure.message);setProp(error,'hidden',false);actions.forEach(button=>setProp(button,'disabled',false));}
  }},[t(action==='approve'?'dock.approve_command':'dock.deny')]));
  const folders=(reach.folders||[]).map(folder=>`${folder.path} ${t(folder.mode==='rw'?'reach.read_write':'reach.read_only')}`).join(' · ')||t('reach.no_file_access');
  append(dialog,el('div',{class:'sheet-handle','aria-hidden':'true'}),el('header',{class:'sheet-header'},[avatar(item.agent||{role}),el('h2',{id:'sheet-title'},command?[t('sheet.request',{role})]:sentenceNodes(item.sentence,t)),close]),el('div',{class:'sheet-content'},[
    command?el('pre',{class:'command-block mono'},[command]):el('p',{},[item.spawn?.task||'']),
    el('ul',{class:'sheet-facts'},[t('approval.container'),folders,t(reach.network==='none'||!reach.network?'reach.no_network':`reach.network.${reach.network}`),reach.credentials?.length?t('approval.credentials',{count:reach.credentials.length}):t('reach.no_credentials')].map(text=>el('li',{},[text]))),
    el('p',{class:'binding-notice'},[t(command?'sheet.binding':'approval.spawn_binding')]),error,
    el('a',{href:`#/approvals/${item.code}`,onclick:()=>closeDialog(dialog)},[t('approvals.more_details')]),
  ]),el('footer',{class:'sheet-actions'},actions));
  dialog.addEventListener('close',()=>remove(dialog));
  append(document.body,dialog);openModal(dialog);focus(close);
  return dialog;
}
