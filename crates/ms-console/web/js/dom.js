export function el(tag, attrs = {}, children = []) {
  const node = document.createElement(tag);
  for (const [name, value] of Object.entries(attrs)) {
    if (name === "class") node.className = value;
    else if (name === "style" && value && typeof value === "object") {
      for (const [property, setting] of Object.entries(value)) {
        node.style.setProperty(property.replace(/[A-Z]/g, letter => `-${letter.toLowerCase()}`), String(setting));
      }
    }
    else if (name.startsWith("on") && typeof value === "function") node.addEventListener(name.slice(2), value);
    else if (value === true) node.setAttribute(name, "");
    else if (value !== false && value != null) node.setAttribute(name, String(value));
  }
  for (const child of children) node.append(child instanceof Node ? child : document.createTextNode(String(child)));
  return node;
}

export function svg(tag, attrs = {}, children = []) {
  const node = document.createElementNS("http://www.w3.org/2000/svg", tag);
  for (const [name, value] of Object.entries(attrs)) {
    if (value === true) node.setAttribute(name, "");
    else if (value !== false && value != null) node.setAttribute(name, String(value));
  }
  for (const child of children) node.append(child instanceof Node ? child : document.createTextNode(String(child)));
  return node;
}

export function clear(node) { while (node.firstChild) node.removeChild(node.firstChild); }
