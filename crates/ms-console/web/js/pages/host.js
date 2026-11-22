import { api } from '../api.js';
import { append, clear, el, setProp, setText, openModal, closeDialog, focus } from '../dom.js';
let message = '';
const sections = ['health','channels','skills','tools','credentials','limits'];
export async function renderHost(root, t, selectedSection) {
  clear(root); let data;
  try { data = (await api('/host')).host || {}; }
  catch (error) { append(root, el('section',{class:'panel',role:'alert'},[error.message])); return; }
  const content = el('div',{class:'host-content'},[el('header',{},[el('h1',{class:'page-title'},[t('nav.host')]),el('p',{class:'muted'},[t('host.settings_helper')]),el('p',{role:'status'},[message])])]);
  const section = (key, children) => el('section',{class:'host-section panel',id:`host-${key}`},[el('h2',{class:'section-title',tabindex:'-1'},[t(key==='health'?'host.health':`host.${key}`)]),...children]);
  const health = section('health',[
    el('div',{class:'host-checks'},(data.doctor||[]).map(check=>el('article',{class:'host-check'},[el('strong',{},[check.check]),el('span',{class:'pill'},[t(check.result==='pass'?'host.check_pass':check.result==='warn'?'host.check_warn':'host.check_fail')]),el('p',{class:'muted'},[check.detail||''])]))),
    el('button',{class:'secondary-button',onclick:async()=>{await renderHost(root,t,'health');focus(root.querySelector('#host-health button'));}},[t('host.run_checks')]),
  ]);
  const channels = section('channels',(data.channels||[]).length ? data.channels.map(item=>el('article',{class:'host-row'},[el('strong',{},[item.kind]),el('span',{class:'pill'},[t(item.enabled?'host.on':'host.off')]),el('p',{class:'muted'},[item.detail||''])])) : [el('p',{class:'muted'},[t('host.no_channels')])]);
  const tools = section('tools',(data.tools||[]).map(item=>{
    const provider=item.provider==='tavily'?'Tavily':item.provider==='brave'?'Brave Search':'';
    const text=!item.configured?t('host.search_unconfigured'):!item.credential_present?t('host.search_missing'):!item.enabled?t('host.search_out_of_scope'):t('host.search_ready',{provider});
    return el('article',{class:'host-row'},[el('strong',{},[item.id]),el('p',{},[text]),el('p',{class:'muted'},[t('host.search_action')])]);
  }));
  const skills = section('skills',[
    ...(data.skills||[]).map(skill=>el('article',{class:'host-skill'},[el('h3',{},[`${skill.name} · ${skill.version}`]),...(skill.roles||[{role:skill.role,policy:skill.policy}]).map(role=>el('p',{},[role.role||'', ' · ', t(`host.policy.${['allow','ask','deny'].includes(role.policy)?role.policy:'ask'}`)]))])),
    ...(!data.skills?.length?[el('p',{class:'muted'},[t('host.no_skills')])]:[]),
    el('form',{class:'host-form',onsubmit:async event=>{event.preventDefault();const form=event.currentTarget;const button=form.querySelector('button');setProp(button,'disabled',true);try{await api('/skills/install',{method:'POST',body:JSON.stringify({path:form.elements.path.value})});message=t('host.skill_installed');await renderHost(root,t,'skills');}catch{message=t('host.operation_failed');setProp(button,'disabled',false);setText(form.querySelector('[role="alert"]'),message);}}},[
      el('label',{for:'skill-path'},[t('host.skill_path')]),el('input',{id:'skill-path',name:'path',required:true,placeholder:t('host.skill_path_placeholder')}),el('button',{class:'secondary-button'},[t('host.install_skill')]),el('p',{class:'inline-error',role:'alert'}),
    ]),
  ]);
  const credentials = section('credentials',[
    el('p',{class:'muted'},[t('host.values_never_shown')]),
    ...(data.credentials||[]).map(item=>el('article',{class:'credential-row'},[el('code',{},[item.name]),el('span',{class:'muted'},[t('host.used_by',{names:(item.used_by||[]).join(', ')||t('host.no_users')})]),el('button',{class:'secondary-button',onclick:event=>credentialDialog(root,item.name,t,event.currentTarget)},[t('host.set_value')])])),
    el('button',{class:'secondary-button',id:'add-credential',onclick:event=>credentialDialog(root,'',t,event.currentTarget)},[t('host.add_credential')]),
  ]);
  const limitRows = value => {
    const labels={memory_mb:'host.limit.memory',pids:'host.limit.processes',tmp_mb:'host.limit.temporary',max_turns:'host.limit.turns',max_tokens:'host.limit.tokens',max_members:'host.limit.members'};
    const rows=Object.entries(labels).filter(([key])=>value?.[key]!=null).flatMap(([key,label])=>[el('dt',{},[t(label)]),el('dd',{class:'mono'},[String(value[key])])]);
    return rows.length?el('dl',{class:'host-limit-list'},rows):el('p',{class:'muted'},[t('host.no_limits')]);
  };
  const limits = section('limits',[
    el('div',{class:'host-limits-grid'},[el('article',{},[el('h3',{},[t('host.per_session')]),limitRows(data.limits)]),el('article',{},[el('h3',{},[t('host.never_grantable')]),limitRows(data.ceiling?.limits||data.ceiling),el('p',{class:'muted'},[t('host.ceiling_note')])])]),
    el('p',{class:'muted'},[t('host.edit_on_host'),' ',el('code',{},['~/.microswarm/config.toml'])]),
    el('details',{'data-raw':'',class:'raw-detail'},[el('summary',{},[t('host.raw_config')]),el('pre',{class:'command-block mono'},[JSON.stringify({limits:data.limits||{},ceiling:data.ceiling||{}},null,2)])]),
  ]);
  append(content,health,channels,skills,tools,credentials,limits);
  append(root,el('div',{class:'host-workspace'},[el('nav',{class:'host-section-nav','aria-label':t('host.sections')},sections.map(key=>el('a',{href:`#/host#${key}`,'aria-current':selectedSection===key?'location':false},[t(key==='health'?'host.health':`host.${key}`)]))),content]));
  if(sections.includes(selectedSection)){const heading=root.querySelector(`#host-${selectedSection} h2`);heading.scrollIntoView({block:'start'});focus(heading);}
}
function credentialDialog(root, name, t, invoker) {
  const error=el('p',{role:'alert',class:'inline-error'});
  const nameField=el('input',{id:'credential-name',name:'name',required:true,pattern:'[A-Za-z0-9_\\-]+',readonly:Boolean(name),autocomplete:'off'});setProp(nameField,'value',name);
  const secret=el('input',{id:'credential-value',name:'value',type:'password',required:true,autocomplete:'new-password'});
  const save=el('button',{class:'primary-button',type:'submit'},[t('host.save_credential')]);
  const dialog=el('dialog',{class:'credential-dialog','aria-labelledby':'credential-title'},[]);
  const cancel=el('button',{class:'secondary-button',type:'button',onclick:()=>closeDialog(dialog)},[t('common.cancel')]);
  append(dialog,el('h2',{id:'credential-title'},[t('host.set_value')]),el('p',{class:'muted'},[t('host.values_never_shown')]),el('form',{class:'host-form',onsubmit:async event=>{
    event.preventDefault();setProp(save,'disabled',true);setProp(cancel,'disabled',true);
    try{await api(`/credentials/${encodeURIComponent(nameField.value)}`,{method:'POST',body:JSON.stringify({value:secret.value})});message=t('host.credential_saved');closeDialog(dialog);await renderHost(root,t,'credentials');focus(root.querySelector('#add-credential'));}
    catch{setText(error,t('host.operation_failed'));setProp(save,'disabled',false);setProp(cancel,'disabled',false);}
    finally{setProp(secret,'value','');}
  }},[el('label',{for:'credential-name'},[t('host.credential_name')]),nameField,el('label',{for:'credential-value'},[t('host.set_value')]),secret,error,el('div',{class:'dialog-actions'},[cancel,save])]));
  dialog.addEventListener('close',()=>{setProp(secret,'value','');});
  append(root,dialog);openModal(dialog);focus(name?secret:nameField);
}
