import {el} from '../dom.js';
import {avatar} from './avatar.js';
import {statusPill} from './pill.js';
export function agentStrip(agents, groups, t) {
  const members=(groups||[]).flatMap(group=>group.members||[]);
  return el('div',{class:'session-agent-strip',role:'group',tabindex:'0','aria-label':t('session.agents')},agents.map(agent=>{
    const member=members.find(item=>item.agent?.id===agent.id);
    const state=member?.state||'working';
    return el('span',{class:'agent-strip-item'},[avatar(agent),el('span',{},[agent.role]),statusPill(member?.status||{key:`status.${state==='waiting_you'?'waiting_for_you':state}`},t)]);
  }));
}
