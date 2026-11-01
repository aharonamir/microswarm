const routes = new Set(["sessions", "approvals", "trace", "host", "members"]);
export function currentRoute() {
  const [page = "sessions", ...parts] = location.hash.replace(/^#\/?/, "").split("/");
  return { page: routes.has(page) ? page : "sessions", parts };
}
export function onRoute(callback) {
  let renderedHash = location.hash;
  addEventListener("hashchange", () => {
    if (location.hash === renderedHash) return;
    renderedHash = location.hash;
    callback();
  });
  callback();
}
