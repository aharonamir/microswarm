export async function api(path, options = {}) {
  const headers = new Headers(options.headers || {});
  headers.set("X-MS-Console", "1");
  if (options.body !== undefined) headers.set("Content-Type", "application/json");
  const response = await fetch(`/api/v1${path}`, { ...options, headers, credentials: "same-origin" });
  if (response.status === 204) return null;
  const body = await response.json();
  if (!response.ok) throw Object.assign(new Error(body.message || body.error), { code: body.error, status: response.status });
  return body;
}
