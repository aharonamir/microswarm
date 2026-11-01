export function connectEvents(onEvent) {
  let source;
  let stopped = false;
  let retry = 500;
  const open = () => {
    if (stopped) return;
    source = new EventSource("/api/v1/events");
    for (const type of ["supervisor", "agent", "status.changed", "chat.reply", "chat.pending", "chat.error", "resync", "supervisor.down", "supervisor.up"]) {
      source.addEventListener(type, event => { retry = 500; onEvent(type, event.data, event.lastEventId); });
    }
    source.onerror = () => { source.close(); setTimeout(open, retry); retry = Math.min(10000, retry * 2); };
  };
  open();
  return () => { stopped = true; source?.close(); };
}
