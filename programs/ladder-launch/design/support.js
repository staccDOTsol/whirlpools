// Standalone runtime for the *.dc.html artboards, so they render outside the canvas.
// Grammar implemented (everything the boards use):
//   {{path}}                      text / attribute substitution; dotted paths resolve loop vars
//   onClick="{{fn}}" onInput=...  event handlers taken from renderVals()
//   value="{{x}}"                 also sets the input's live value
//   <sc-for list="{{xs}}" as="t"> repeat children once per item
//   <sc-if value="{{cond}}">      keep children when truthy
//   <helmet>                      children move to <head>
//   <script type="text/x-dc" data-dc-script data-props='{...}'>
//                                 defines `class Component extends DCLogic`; props come
//                                 from each prop's `default` (overridable with ?prop=value)
(function () {
  class DCLogic {
    constructor(props) { this.props = props || {}; this.state = {}; }
    setState(patch) { Object.assign(this.state, typeof patch === 'function' ? patch(this.state) : patch); if (this.__render) this.__render(); }
  }
  window.DCLogic = DCLogic;

  const RE = /\{\{\s*([^}]+?)\s*\}\}/g;

  function lookup(path, scopes) {
    const parts = path.split('.');
    for (let i = scopes.length - 1; i >= 0; i--) {
      if (scopes[i] && parts[0] in scopes[i]) {
        let v = scopes[i];
        for (const p of parts) { if (v == null) return undefined; v = v[p]; }
        return v;
      }
    }
    return undefined;
  }
  const interpolate = (s, scopes) => s.replace(RE, (_, p) => { const v = lookup(p, scopes); return v == null ? '' : String(v); });

  function expand(node, scopes) {
    if (node.nodeType === Node.TEXT_NODE) {
      if (node.nodeValue.includes('{{')) node.nodeValue = interpolate(node.nodeValue, scopes);
      return;
    }
    if (node.nodeType !== Node.ELEMENT_NODE) return;
    const tag = node.tagName.toLowerCase();
    if (tag === 'script' || tag === 'style' || tag === 'helmet') return;

    if (tag === 'sc-for') {
      const m = /\{\{\s*([^}]+?)\s*\}\}/.exec(node.getAttribute('list') || '');
      const list = m ? lookup(m[1].trim(), scopes) : [];
      const as = node.getAttribute('as') || 'item';
      const frag = document.createDocumentFragment();
      (Array.isArray(list) ? list : []).forEach((item, index) => {
        const scope = { [as]: item, index };
        for (const child of Array.from(node.childNodes)) {
          const c = child.cloneNode(true);
          expand(c, scopes.concat(scope));
          frag.appendChild(c);
        }
      });
      node.replaceWith(frag);
      return;
    }
    if (tag === 'sc-if') {
      const m = /\{\{\s*([^}]+?)\s*\}\}/.exec(node.getAttribute('value') || '');
      const ok = m ? !!lookup(m[1].trim(), scopes) : false;
      if (!ok) { node.remove(); return; }
      const frag = document.createDocumentFragment();
      for (const child of Array.from(node.childNodes)) { expand(child, scopes); frag.appendChild(child); }
      node.replaceWith(frag);
      return;
    }

    for (const attr of Array.from(node.attributes)) {
      const name = attr.name, val = attr.value;
      if (name.startsWith('hint-')) { node.removeAttribute(name); continue; }
      if (!val.includes('{{')) continue;
      if (name.startsWith('on')) {
        node.removeAttribute(name);
        const m = /\{\{\s*([^}]+?)\s*\}\}/.exec(val);
        const fn = m ? lookup(m[1].trim(), scopes) : null;
        if (typeof fn === 'function') node.addEventListener(name.slice(2), fn);
        continue;
      }
      const out = interpolate(val, scopes);
      node.setAttribute(name, out);
      if (name === 'value' && 'value' in node) node.value = out;
    }
    for (const child of Array.from(node.childNodes)) expand(child, scopes);
  }

  function focusPath(root) {
    const el = document.activeElement;
    if (!el || !root.contains(el)) return null;
    const inputs = Array.from(root.querySelectorAll('input,textarea,select,button'));
    const i = inputs.indexOf(el);
    return i < 0 ? null : { i, start: el.selectionStart, end: el.selectionEnd };
  }
  function restoreFocus(root, fp) {
    if (!fp) return;
    const el = root.querySelectorAll('input,textarea,select,button')[fp.i];
    if (!el) return;
    el.focus();
    try { if (fp.start != null) el.setSelectionRange(fp.start, fp.end); } catch (e) {}
  }

  function boot() {
    document.querySelectorAll('helmet').forEach((h) => { while (h.firstChild) document.head.appendChild(h.firstChild); h.remove(); });
    let root = document.querySelector('x-dc');
    if (!root) return;
    const scriptEl = document.querySelector('script[data-dc-script]');
    let spec = {};
    try { spec = JSON.parse((scriptEl && scriptEl.dataset.props) || '{}'); } catch (e) {}
    const props = {};
    for (const [k, v] of Object.entries(spec)) if (k !== '$preview' && v && 'default' in v) props[k] = v.default;
    for (const [k, v] of new URLSearchParams(location.search)) if (k in props) { const n = Number(v); props[k] = Number.isNaN(n) ? v : n; }

    let instance = null;
    if (scriptEl) {
      try {
        const Component = new Function('DCLogic', scriptEl.textContent + '\nreturn typeof Component === "function" ? Component : null;')(DCLogic);
        if (Component) instance = new Component(props);
      } catch (e) { console.error('dc component failed', e); }
    }
    const template = root.cloneNode(true);
    const render = () => {
      const vals = instance ? instance.renderVals() : {};
      const fp = focusPath(root);
      const fresh = template.cloneNode(true);
      for (const child of Array.from(fresh.childNodes)) expand(child, [props, vals]);
      root.replaceWith(fresh);
      root = fresh;
      restoreFocus(root, fp);
    };
    if (instance) instance.__render = render;
    render();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot); else boot();
})();
