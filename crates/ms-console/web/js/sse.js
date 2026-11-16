export function connectEvents(onEvent) {
  const source = new EventSource("/api/v1/events");
  for (const type of ["supervisor", "agent", "status.changed", "chat.reply", "chat.pending", "chat.error", "resync", "supervisor.down", "supervisor.up"]) {
    source.addEventListener(type, event => onEvent(type, event.data, event.lastEventId));
  }
  source.onopen = () => onEvent("console.connected", "{}", "");
  source.onerror = () => onEvent("console.reconnecting", "{}", "");
  return () => source.close();
}
