export function localTime(value) {
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat(undefined, { hour: "2-digit", minute: "2-digit", hourCycle: "h23" }).format(date);
}

export function duration(milliseconds) {
  const seconds = Math.max(0, Number(milliseconds) || 0) / 1000;
  if (seconds < 1) return `${seconds.toFixed(1)} s`;
  if (seconds < 60) return `${Math.round(seconds)} s`;
  const minutes = Math.floor(seconds / 60);
  const remaining = Math.floor(seconds % 60);
  if (minutes < 60) return remaining ? `${minutes} min ${remaining} s` : `${minutes} min`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest ? `${hours} h ${rest} min` : `${hours} h`;
}

export function relativeTime(milliseconds, now = Date.now()) {
  const remaining = Math.max(0, Number(milliseconds) - now);
  return `${duration(remaining)} left`;
}

export function shortId(value) {
  const text = String(value ?? "");
  return text.length > 8 ? `${text.slice(0, 6)}…${text.slice(-3)}` : text;
}
