const routeNames = new Set(["home", "sessions", "approvals", "trajectories", "trajectory", "host", "member"]);

function redirect(hash) {
  if (location.hash !== hash) location.replace(hash);
}

export function currentRoute() {
  let value = location.hash.replace(/^#\/?/, "");
  const [routeValue, section] = value.split("#", 2);
  value = routeValue;
  if (!value) {
    redirect("#/home");
    value = "home";
  }
  if (value === "overview") {
    redirect("#/home");
    value = "home";
  }
  if (value.startsWith("trace/")) {
    const id = value.split("/")[1] || "";
    redirect(`#/sessions/${id}/trajectory`);
    value = `sessions/${id}/trajectory`;
  } else if (value.startsWith("members/")) {
    const id = value.split("/")[1] || "";
    redirect(`#/sessions/${id}/members/${id}`);
    value = `sessions/${id}/members/${id}`;
  }
  const [name = "home", ...parts] = value.split("/");
  if (name === "sessions" && parts[1] === "trajectory") return { page: "trajectory", parts: [parts[0], parts[2]].filter(Boolean) };
  if (name === "sessions" && parts[1] === "members") return { page: "member", parts: [parts[0], parts[2]].filter(Boolean) };
  if (name === "sessions") return { page: "sessions", parts };
  if (name === "trace") return { page: "trajectory", parts };
  return { page: routeNames.has(name) ? name : "home", parts: name === "host" && section ? [section, ...parts] : parts };
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
