#!/usr/bin/env node
/**
 * Extract lucide-animated (pqoqubbw/icons) TSX → Jadix recipe JSON.
 * Usage: node scripts/assets/extract_lucide_animated.mjs [.tmp/lucide-animated/icons]
 */
import fs from "node:fs";
import path from "node:path";

const iconsDir = path.resolve(
  process.argv[2] ?? ".tmp/lucide-animated/icons"
);
const outPath = path.resolve(
  process.argv[3] ?? "crates/jadix_lucide/assets/lucide_animated.json"
);

function extractBalancedObject(src, startIdx) {
  if (src[startIdx] !== "{") return null;
  let depth = 0;
  let inStr = null;
  let escape = false;
  for (let i = startIdx; i < src.length; i++) {
    const c = src[i];
    if (inStr) {
      if (escape) {
        escape = false;
        continue;
      }
      if (c === "\\") {
        escape = true;
        continue;
      }
      if (c === inStr) inStr = null;
      continue;
    }
    if (c === '"' || c === "'" || c === "`") {
      inStr = c;
      continue;
    }
    if (c === "{") depth++;
    else if (c === "}") {
      depth--;
      if (depth === 0) return src.slice(startIdx, i + 1);
    }
  }
  return null;
}

function parseKeyframeValue(raw) {
  const s = raw.trim();
  if (s.startsWith("[")) {
    const inner = s.slice(1, s.lastIndexOf("]"));
    return inner.split(",").map((x) => parseScalar(x.trim())).filter((x) => x != null);
  }
  return parseScalar(s);
}

function parseScalar(s) {
  if (!s) return null;
  if (/^['"]/.test(s)) return s.slice(1, -1);
  const deg = s.match(/^(-?[\d.]+)deg$/);
  if (deg) return parseFloat(deg[1]);
  const n = Number(s);
  if (!Number.isNaN(n)) return n;
  return s;
}

/** Parse a Motions variants-like object into { normal, animate, transition? } loosely. */
function parseVariantsObject(objSrc) {
  const states = {};
  const keyRe =
    /\b(normal|animate|hover|fadeIn|fadeOut|initial|active|visible|hidden|firstState|secondState|blink)\s*:\s*\{/g;
  let m;
  while ((m = keyRe.exec(objSrc))) {
    const key = m[1];
    const brace = objSrc.indexOf("{", m.index + key.length);
    let body = extractBalancedObject(objSrc, brace);
    if (!body) continue;
    let transition = null;
    const tIdx = body.search(/\btransition\s*:\s*\{/);
    if (tIdx >= 0) {
      const tBrace = body.indexOf("{", tIdx);
      const tBody = extractBalancedObject(body, tBrace);
      if (tBody) {
        transition = parseTransition(tBody);
        // Strip transition so prop regex does not eat `opacity: { duration }`
        body = body.slice(0, tIdx) + body.slice(tBrace + tBody.length);
      }
    }
    const props = {};
    const propRe =
      /\b(opacity|pathLength|pathOffset|pathSpacing|scale|scaleX|scaleY|rotate|x|y|x1|x2|y1|y2|cx|cy|r|width|height|strokeWidth|translateX|translateY|originX|originY|fill|d)\s*:\s*(\[[^\]]*\]|-?[\d.]+(?:deg)?|"[^"]*"|'[^']*'|[^,}\n]+)/g;
    let pm;
    while ((pm = propRe.exec(body))) {
      const prop = normalizeProp(pm[1]);
      let raw = pm[2].trim();
      if (raw.endsWith(",")) raw = raw.slice(0, -1).trim();
      // Skip leftover object fragments
      if (raw.startsWith("{")) continue;
      props[prop] = parseKeyframeValue(raw);
    }
    states[key] = { props, transition };
  }
  return states;
}

function normalizeProp(p) {
  if (p === "translateX") return "x";
  if (p === "translateY") return "y";
  if (p === "strokeWidth") return "strokeWidth";
  return p;
}

/**
 * Collapse alternate Motion variant names into normal/animate.
 * wifi uses fadeOut→fadeIn on hover; cloud-download uses initial/active
 * (enter→initial, leave→active).
 */
function normalizeToNormalAnimate(states, indexHint = null) {
  if (!states || typeof states !== "object") return states;
  const out = { ...states };

  if (!out.normal && out.fadeIn) out.normal = out.fadeIn;
  if (!out.normal && out.active) out.normal = out.active;
  if (!out.normal && out.visible) out.normal = out.visible;
  if (!out.normal && out.initial) out.normal = out.initial;

  if (!out.animate && out.initial && out.active) {
    // cloud-download: enter starts "initial", leave "active"
    out.animate = out.initial;
    out.normal = out.active;
  }
  if (!out.animate && out.initial && !out.active) out.animate = out.initial;
  // chart-*: variants are visible/hidden; hover often hides then shows.
  if (!out.animate && out.hidden && out.visible) {
    out.normal = out.visible;
    const hp = out.hidden.props || {};
    const vp = out.visible.props || {};
    const props = {};
    for (const k of new Set([...Object.keys(hp), ...Object.keys(vp)])) {
      const a = vp[k];
      const b = hp[k];
      const c = vp[k];
      if (typeof a === "number" && typeof b === "number" && typeof c === "number") {
        props[k] = [a, b, c];
      } else {
        props[k] = b ?? a;
      }
    }
    out.animate = {
      props,
      transition: {
        duration: 0.6,
        ...(out.hidden.transition || {}),
        ...(out.visible.transition || {}),
      },
    };
  } else if (!out.animate && out.hidden) {
    out.animate = out.hidden;
  }

  if (!out.animate && (out.fadeOut || out.fadeIn)) {
    const fo = out.fadeOut?.props || {};
    const fi = out.fadeIn?.props || out.normal?.props || {};
    const props = {};
    const keys = new Set([...Object.keys(fo), ...Object.keys(fi)]);
    for (const k of keys) {
      const a = fo[k];
      const b = fi[k] ?? a;
      if (a == null && b == null) continue;
      if (typeof a === "number" && typeof b === "number" && a !== b) {
        props[k] = [a, b];
      } else if (Array.isArray(a) || Array.isArray(b)) {
        props[k] = [].concat(a ?? b, b ?? a);
      } else if (a != null && b != null && a !== b) {
        props[k] = [a, b];
      } else {
        props[k] = b ?? a;
      }
    }
    // wifi: fadeOut then fadeIn — three stops reads better when values differ
    if (
      typeof fo.opacity === "number" &&
      typeof fi.opacity === "number" &&
      fo.opacity !== fi.opacity
    ) {
      const idle = out.normal?.props?.opacity ?? 1;
      props.opacity = [idle, fo.opacity, fi.opacity];
    }
    const tr = {
      ...(out.fadeOut?.transition || {}),
      ...(out.fadeIn?.transition || {}),
    };
    // Prefer summed duration so both phases play
    const d0 = out.fadeOut?.transition?.duration ?? 0.2;
    const d1 = out.fadeIn?.transition?.duration ?? 0.4;
    tr.duration = (typeof d0 === "number" ? d0 : 0.2) + (typeof d1 === "number" ? d1 : 0.4);
    if (out.fadeIn?.transition?.delay != null) tr.delay = out.fadeIn.transition.delay;
    out.animate = { props, transition: tr };
  }

  // Drop aliases from emitted tracks — runtime only samples normal/animate.
  const keep = {};
  if (out.normal) keep.normal = out.normal;
  if (out.animate) keep.animate = out.animate;
  if (out.hover && !keep.animate) keep.animate = out.hover;
  return keep;
}

function parseTransition(tBody) {
  const out = {};
  const dur = tBody.match(/\bduration\s*:\s*([\d.]+)/);
  if (dur) out.duration = parseFloat(dur[1]);
  const delay = tBody.match(/\bdelay\s*:\s*([\d.]+)/);
  if (delay) out.delay = parseFloat(delay[1]);
  // delay: index * 0.07 — keep as expression string for later eval
  if (out.delay == null) {
    const delayExpr = tBody.match(/\bdelay\s*:\s*([^,}\n]+)/);
    if (delayExpr) {
      const ev = evalJsExpr(delayExpr[1].trim());
      if (typeof ev === "number") out.delay = ev;
    }
  }
  const ease = tBody.match(/\bease\s*:\s*["']([^"']+)["']/);
  if (ease) out.ease = ease[1];
  const repeat = tBody.match(/\brepeat\s*:\s*(Infinity|[\d.]+)/);
  if (repeat) out.repeat = repeat[1] === "Infinity" ? -1 : parseFloat(repeat[1]);
  const type = tBody.match(/\btype\s*:\s*["']([^"']+)["']/);
  if (type) out.type = type[1];
  const stiff = tBody.match(/\bstiffness\s*:\s*([\d.]+)/);
  if (stiff) out.stiffness = parseFloat(stiff[1]);
  const damp = tBody.match(/\bdamping\s*:\s*([\d.]+)/);
  if (damp) out.damping = parseFloat(damp[1]);
  const times = tBody.match(/\btimes\s*:\s*(\[[^\]]*\])/);
  if (times) {
    const arr = evalJsExpr(times[1]);
    if (Array.isArray(arr)) out.times = arr.map(Number).filter((n) => !Number.isNaN(n));
  }
  // per-prop duration overrides (opacity: { duration: 0.1 })
  const per = {};
  const perRe =
    /\b(opacity|pathLength|pathOffset|scale|scaleX|scaleY|rotate|x|y|x1|x2|y1|y2|width|height|strokeWidth)\s*:\s*\{([^}]*)\}/g;
  let pm;
  while ((pm = perRe.exec(tBody))) {
    const d = pm[2].match(/\bduration\s*:\s*([\d.]+)/);
    if (d) per[pm[1]] = { duration: parseFloat(d[1]) };
  }
  if (Object.keys(per).length) out.perProp = per;
  return out;
}

function extractSvgShell(src) {
  const open = src.match(
    /<(?:motion\.)?svg\b([^>]*)>/
  );
  const attrs = {};
  if (open) {
    const a = open[1];
    for (const m of a.matchAll(
      /\b(viewBox|strokeWidth|strokeLinecap|strokeLinejoin|fill|width|height)\s*=\s*(?:\{[^}]*\}|"([^"]*)"|'([^']*)')/g
    )) {
      attrs[m[1]] = m[2] ?? m[3] ?? "";
    }
  }
  return {
    viewBox: attrs.viewBox || "0 0 24 24",
    strokeWidth: parseFloat(attrs.strokeWidth || "2") || 2,
    strokeLinecap: attrs.strokeLinecap || "round",
    strokeLinejoin: attrs.strokeLinejoin || "round",
  };
}

function evalJsExpr(expr) {
  try {
    return Function(`"use strict"; return (${expr});`)();
  } catch {
    return null;
  }
}

/** Parse `animate: (custom|i|index) => ({ ... })` by substituting a concrete custom value. */
function parseFunctionAnimateState(objSrc, customVal) {
  const m = objSrc.match(
    /animate\s*:\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)(?:\s*:\s*[^)]*)?\)\s*=>/
  );
  if (!m) return null;
  const paramName = m[1];
  const arrow = objSrc.indexOf("=>", m.index);
  // Body may be `({...})` or `{...}`
  let brace = objSrc.indexOf("{", arrow);
  if (brace < 0) return null;
  let body = extractBalancedObject(objSrc, brace);
  if (!body) return null;
  let inner = body.slice(1, -1);
  // Substitute the parameter (custom / i / index / …)
  const paramRe = new RegExp(`\\b${paramName}\\b`, "g");
  inner = inner.replace(paramRe, String(customVal));
  inner = inner.replace(/\bcustom\b/g, String(customVal));

  const props = {};
  // Prefer array values first
  const arrRe =
    /\b(opacity|pathLength|pathOffset|scale|scaleX|scaleY|rotate|x|y|x1|x2|y1|y2|cx|cy|r|width|height|strokeWidth|translateX|translateY)\s*:\s*(\[[^\]]*\])/g;
  let pm;
  while ((pm = arrRe.exec(inner))) {
    props[normalizeProp(pm[1])] = parseKeyframeValue(pm[2]);
  }
  const propRe =
    /\b(opacity|pathLength|pathOffset|scale|scaleX|scaleY|rotate|x|y|x1|x2|y1|y2|cx|cy|r|width|height|strokeWidth|translateX|translateY)\s*:\s*([^,}\n\[]+)/g;
  while ((pm = propRe.exec(inner))) {
    const prop = normalizeProp(pm[1]);
    if (props[prop] != null) continue;
    let raw = pm[2].trim();
    if (raw.endsWith(",")) raw = raw.slice(0, -1).trim();
    if (raw.startsWith("{")) continue;
    const ev = evalJsExpr(raw);
    if (ev == null) continue;
    if (typeof ev === "number") props[prop] = ev;
    else if (Array.isArray(ev)) props[prop] = ev;
    else props[prop] = parseKeyframeValue(String(ev));
  }

  let transition = null;
  const tIdx = body.search(/\btransition\s*:\s*\{/);
  if (tIdx >= 0) {
    const tBrace = body.indexOf("{", tIdx);
    let tBody = extractBalancedObject(body, tBrace);
    if (tBody) {
      tBody = tBody.replace(paramRe, String(customVal)).replace(/\bcustom\b/g, String(customVal));
      transition = parseTransition(tBody);
      const delayExpr = tBody.match(/\bdelay\s*:\s*([^,}\n]+)/);
      if (delayExpr) {
        const ev = evalJsExpr(delayExpr[1].trim());
        if (typeof ev === "number") transition.delay = ev;
      }
    }
  }
  return { props, transition };
}

function tracksFromVariantStates(states) {
  const normalized = normalizeToNormalAnimate(states);
  const byProp = {};
  for (const [state, { props, transition }] of Object.entries(normalized || {})) {
    for (const [prop, value] of Object.entries(props || {})) {
      if (!byProp[prop]) byProp[prop] = { prop, keyframes: {} };
      byProp[prop].keyframes[state] = value;
      if (transition)
        byProp[prop].transition = {
          ...(byProp[prop].transition || {}),
          ...transition,
        };
    }
  }
  return Object.values(byProp);
}

function parseNodeAttrs(attrSrc) {
  const attrs = {};
  const d =
    attrSrc.match(/\bd\s*=\s*"([^"]*)"/) ||
    attrSrc.match(/\bd\s*=\s*\{`([^`]*)`\}/);
  if (d) attrs.d = d[1];
  for (const am of attrSrc.matchAll(
    /\b(cx|cy|r|rx|ry|x1|y1|x2|y2|x|y|width|height|points|fill|stroke)\s*=\s*"([^"]*)"/g
  )) {
    attrs[am[1]] = am[2];
  }
  // JSX numeric literals: cx={19} r={1}
  for (const am of attrSrc.matchAll(
    /\b(cx|cy|r|rx|ry|x1|y1|x2|y2|x|y|width|height)\s*=\s*\{(\d+(?:\.\d+)?)\}/g
  )) {
    attrs[am[1]] = am[2];
  }
  // JSX fill={...} / stroke={...} / fill="currentColor"
  const fillExpr = attrSrc.match(/\bfill\s*=\s*\{([^}]*)\}/);
  if (fillExpr) attrs.fill = fillExpr[1].replace(/['"]/g, "").trim();
  const strokeExpr = attrSrc.match(/\bstroke\s*=\s*\{([^}]*)\}/);
  if (strokeExpr) attrs.stroke = strokeExpr[1].replace(/['"]/g, "").trim();
  return attrs;
}

/** Parse `const NAME = [ ... ]` array literals (string[] or object[]). */
function parseConstArray(src, name) {
  const m = src.match(
    new RegExp(`(?:const|let)\\s+${name}\\s*=\\s*(\\[[\\s\\S]*?\\n\\s*\\])`)
  );
  if (!m) return null;
  const ev = evalJsExpr(m[1]);
  return Array.isArray(ev) ? ev : null;
}

/**
 * Expand `.map(...)` that emit motion.path / motion.line with dynamic `d`.
 * Covers sun (string array) and wifi (object array with level.d).
 */
function expandMappedMotionNodes(src, named, nextId) {
  const nodes = [];

  // Pattern A: {[ "M..", ... ].map((d, index) => ( <motion.path d={d} custom={...} variants={X} /> ))}
  const strMapRe =
    /\{\s*\[([\s\S]*?)\]\s*\.map\s*\(\s*\(([^)]*)\)\s*=>\s*\(\s*<motion\.(path|line|circle|rect)\b([\s\S]*?)(?:\/>|><\/motion\.\3>)/g;
  let m;
  while ((m = strMapRe.exec(src))) {
    const arrLit = "[" + m[1] + "]";
    const items = evalJsExpr(arrLit);
    if (!Array.isArray(items) || items.length === 0) continue;
    if (typeof items[0] !== "string") continue;
    const tag = m[3];
    const attrSrc = m[4];
    const customExpr = attrSrc.match(/\bcustom\s*=\s*\{([^}]*)\}/);
    const vRef = attrSrc.match(/\bvariants\s*=\s*\{([A-Za-z_][A-Za-z0-9_]*)\}/);
    const vInline = /\bvariants\s*=\s*\{\{/.test(attrSrc);
    let inlineBody = null;
    if (vInline) {
      const vIdx = attrSrc.search(/\bvariants\s*=\s*\{/);
      const start = attrSrc.indexOf("{", vIdx);
      inlineBody = extractBalancedObject(attrSrc, start);
    }
    items.forEach((dVal, index) => {
      let customVal = index;
      if (customExpr) {
        const ev = evalJsExpr(
          customExpr[1].replace(/\bindex\b/g, String(index))
        );
        if (typeof ev === "number") customVal = ev;
      }
      let variants = null;
      if (vRef) {
        const name = vRef[1];
        const raw = named[name + "__raw"];
        variants = named[name] ? JSON.parse(JSON.stringify(named[name])) : {};
        if (raw) {
          const fnState = parseFunctionAnimateState(raw, customVal);
          if (fnState) {
            if (!variants.normal) {
              const parsed = parseVariantsObject(raw);
              if (parsed.normal) variants.normal = parsed.normal;
            }
            variants.animate = fnState;
          } else if (!variants.animate) {
            variants = parseVariantsObject(raw);
          }
        }
      } else if (inlineBody) {
        variants = parseVariantsObject(inlineBody);
        const fnState = parseFunctionAnimateState(inlineBody, customVal);
        if (fnState) variants.animate = fnState;
      }
      const attrs = parseNodeAttrs(attrSrc);
      attrs.d = dVal;
      nodes.push({
        id: `n${nextId()}`,
        tag,
        attrs,
        tracks: variants ? tracksFromVariantStates(variants) : [],
        children: [],
      });
    });
  }

  // Pattern B: {NAME.map((level, index) => ( <motion.path d={level.d} ... /> ))}
  const namedMapRe =
    /\{\s*([A-Z][A-Za-z0-9_]*)\s*\.map\s*\(\s*\(([^)]*)\)\s*=>\s*\(\s*<motion\.(path|line|circle|rect)\b([\s\S]*?)(?:\/>|><\/motion\.\3>)/g;
  while ((m = namedMapRe.exec(src))) {
    const arrName = m[1];
    const items = parseConstArray(src, arrName);
    if (!Array.isArray(items) || items.length === 0) continue;
    const tag = m[3];
    const attrSrc = m[4];
    const vInline = /\bvariants\s*=\s*\{\{/.test(attrSrc);
    let inlineBody = null;
    if (vInline) {
      const vIdx = attrSrc.search(/\bvariants\s*=\s*\{/);
      const start = attrSrc.indexOf("{", vIdx);
      // variants prop value is `{ { fadeOut... } }` — extractBalanced from first `{`
      inlineBody = extractBalancedObject(attrSrc, start);
    }
    const vRef = attrSrc.match(/\bvariants\s*=\s*\{([A-Za-z_][A-Za-z0-9_]*)\}/);
    items.forEach((item, index) => {
      const attrs = parseNodeAttrs(attrSrc);
      if (item && typeof item === "object") {
        if (item.d) attrs.d = item.d;
        if (item.cx != null) attrs.cx = String(item.cx);
        if (item.cy != null) attrs.cy = String(item.cy);
        if (item.r != null) attrs.r = String(item.r);
      } else if (typeof item === "string") {
        attrs.d = item;
      }
      let customVal = index;
      const customExpr = attrSrc.match(/\bcustom\s*=\s*\{([^}]*)\}/);
      if (customExpr) {
        const ev = evalJsExpr(
          customExpr[1].replace(/\bindex\b/g, String(index))
        );
        if (typeof ev === "number") customVal = ev;
      }
      let variants = null;
      if (vRef) {
        const name = vRef[1];
        variants = named[name] ? JSON.parse(JSON.stringify(named[name])) : {};
        const raw = named[name + "__raw"];
        if (raw) {
          const fnState = parseFunctionAnimateState(raw, customVal);
          if (fnState) {
            if (!variants.normal) {
              const parsed = parseVariantsObject(raw);
              if (parsed.normal) variants.normal = parsed.normal;
            }
            variants.animate = fnState;
          } else if (!variants.animate) {
            variants = parseVariantsObject(raw);
          }
        }
      } else if (inlineBody) {
        // Evaluate `index === 0 ? …` BEFORE any raw `\bindex\b` digit substitution —
        // otherwise `opacity: 2 === 0 ? 1 : 0` is misread as `opacity: 2`.
        let body = inlineBody;
        body = body.replace(
          /index\s*===\s*0\s*\?\s*([^:?]+?)\s*:\s*([^,}\n]+)/g,
          (_, a, b) => String(index === 0 ? a.trim() : b.trim())
        );
        if (item && typeof item === "object" && item.delay != null) {
          body = body.replace(/\blevel\.delay\b/g, String(item.delay));
        }
        variants = parseVariantsObject(body);
        if (item && typeof item === "object" && item.delay != null) {
          if (variants.fadeIn) {
            variants.fadeIn.transition = {
              ...(variants.fadeIn.transition || {}),
              delay: item.delay,
            };
          }
        }
        for (const st of Object.values(variants)) {
          for (const [pk, pv] of Object.entries(st.props || {})) {
            if (typeof pv === "string" && /index|level/.test(pv)) {
              const ev = evalJsExpr(
                pv
                  .replace(/\bindex\b/g, String(index))
                  .replace(/\blevel\.delay\b/g, String(item?.delay ?? 0))
              );
              if (typeof ev === "number") st.props[pk] = ev;
            }
          }
        }
      }
      // initialOpacity from level
      if (
        item &&
        typeof item === "object" &&
        item.initialOpacity != null &&
        variants
      ) {
        if (!variants.normal) variants.normal = { props: {}, transition: null };
        if (variants.normal.props.opacity == null) {
          variants.normal.props.opacity = item.initialOpacity;
        }
      }
      nodes.push({
        id: `n${nextId()}`,
        tag,
        attrs,
        tracks: variants ? tracksFromVariantStates(variants) : [],
        children: [],
      });
    });
  }

  return nodes;
}

function resolveVariantsForAttrs(src, attrSrc, named, customVal) {
  let variants = null;
  const vRef = attrSrc.match(/\bvariants\s*=\s*\{([A-Za-z_][A-Za-z0-9_]*)\}/);
  const vInline = attrSrc.match(/\bvariants\s*=\s*\{\{/);
  if (vRef) {
    const name = vRef[1];
    // Prefer pre-parsed named table
    if (named[name]) {
      variants = { ...named[name] };
    } else {
      const decl = src.match(
        new RegExp(`(?:const|let)\\s+${name}(?:\\s*:\\s*[^=]+)?\\s*=\\s*(\\{)`)
      );
      if (decl) {
        const brace = src.indexOf("{", decl.index + decl[0].length - 1);
        const body = extractBalancedObject(src, brace);
        if (body) {
          variants = parseVariantsObject(body);
          // Function animate?
          if (customVal != null && (!variants.animate || !variants.animate.props || Object.keys(variants.animate.props || {}).length === 0)) {
            const fnState = parseFunctionAnimateState(body, customVal);
            if (fnState) {
              variants.animate = fnState;
              if (!variants.normal) {
                // ensure normal from parseVariantsObject already
              }
            }
          }
        }
      }
    }
    // Named table may only have normal if animate was a function — materialize
    if (customVal != null && named[name + "__raw"]) {
      const fnState = parseFunctionAnimateState(named[name + "__raw"], customVal);
      if (fnState) {
        variants = variants || {};
        variants.animate = fnState;
      }
    }
  } else if (vInline) {
    const start = src.indexOf("{", src.indexOf(attrSrc) >= 0
      ? /* fallback */ -1
      : -1);
    // Locate variants={{ relative to attrSrc occurrence — scan attrSrc only for inline
    const localStart = attrSrc.indexOf("variants");
    // Use a wider window from src
  }
  return variants;
}

function extractNodes(src, named = {}) {
  // Store raw variant object sources for function-animate materialization
  for (const m of src.matchAll(
    /(?:const|let)\s+(\w+)\s*(?::\s*[^=]+)?=\s*(\{)/g
  )) {
    if (!/variants?/i.test(m[1])) continue;
    const brace = src.indexOf("{", m.index + m[0].length - 1);
    const body = extractBalancedObject(src, brace);
    if (body) {
      named[m[1] + "__raw"] = body;
      if (!named[m[1]]) named[m[1]] = parseVariantsObject(body);
    }
  }

  // Named transition consts (DEFAULT_TRANSITION, …)
  const transitions = {};
  for (const m of src.matchAll(
    /(?:const|let)\s+([A-Z][A-Za-z0-9_]*)\s*(?::\s*[^=]+)?=\s*(\{)/g
  )) {
    if (!/TRANSITION/i.test(m[1])) continue;
    const brace = src.indexOf("{", m.index + m[0].length - 1);
    const body = extractBalancedObject(src, brace);
    if (body) transitions[m[1]] = parseTransition(body);
  }

  const nodes = [];
  let idx = 0;

  function parseBlock(blockSrc, from = 0, to = blockSrc.length) {
    const slice = blockSrc.slice(from, to);
    const tagRe =
      /<(motion\.)?(path|circle|line|rect|polyline|polygon|g)\b([^>]*?)(\/>|>)/g;
    let m;
    const localNodes = [];
    while ((m = tagRe.exec(slice))) {
      const absStart = from + m.index;
      const tag = m[2];
      const attrSrc = m[3];
      const selfClosing = m[4] === "/>";
      const isMotion = !!m[1];
      const node = {
        id: `n${idx++}`,
        tag,
        attrs: parseNodeAttrs(attrSrc),
        tracks: [],
        children: [],
      };

      const customM = attrSrc.match(/\bcustom\s*=\s*\{(\d+)\}/);
      const customVal = customM ? parseInt(customM[1], 10) : null;

      // variants
      let variants = null;
      const vRef = attrSrc.match(/\bvariants\s*=\s*\{([A-Za-z_][A-Za-z0-9_]*)\}/);
      if (vRef) {
        const name = vRef[1];
        if (named[name]) variants = JSON.parse(JSON.stringify(named[name]));
        const raw = named[name + "__raw"];
        if (raw && customVal != null) {
          const fnState = parseFunctionAnimateState(raw, customVal);
          if (fnState) {
            variants = variants || {};
            // Keep normal from object parse
            if (!variants.normal) {
              const parsed = parseVariantsObject(raw);
              if (parsed.normal) variants.normal = parsed.normal;
            }
            variants.animate = fnState;
          }
        } else if (raw && (!variants || !variants.animate)) {
          const parsed = parseVariantsObject(raw);
          variants = parsed;
        }
      } else if (/\bvariants\s*=\s*\{\{/.test(attrSrc)) {
        // Inline variants — find in full src near absStart
        const window = blockSrc.slice(absStart, absStart + 600);
        const vIdx = window.search(/\bvariants\s*=\s*\{/);
        if (vIdx >= 0) {
          const start = window.indexOf("{", vIdx);
          const body = extractBalancedObject(window, start);
          if (body) variants = parseVariantsObject(body);
        }
      }

      if (variants) {
        node.tracks = tracksFromVariantStates(variants);
      }

      // element transition= — object literal or named const ref
      const local = blockSrc.slice(absStart, absStart + 800);
      const tRef = local.match(/\btransition\s*=\s*\{([A-Za-z_][A-Za-z0-9_]*)\}/);
      if (tRef && transitions[tRef[1]]) {
        const elTransition = transitions[tRef[1]];
        for (const track of node.tracks) {
          track.transition = { ...(track.transition || {}), ...elTransition };
        }
      } else {
        const tm = local.match(/\btransition\s*=\s*\{/);
        if (tm) {
          const abs = absStart + tm.index + tm[0].length - 1;
          const body = extractBalancedObject(blockSrc, abs);
          if (body && !/^[A-Za-z_]/.test(body.slice(1).trim())) {
            const elTransition = parseTransition(body);
            for (const track of node.tracks) {
              track.transition = { ...(track.transition || {}), ...elTransition };
            }
          }
        }
      }

      let endPos = absStart + m[0].length;
      if (!selfClosing && tag === "g") {
        // Find matching </motion.g> or </g>
        const rest = blockSrc.slice(endPos);
        const close = rest.match(/<\/(?:motion\.)?g\s*>/);
        if (close) {
          const innerFrom = endPos;
          const innerTo = endPos + close.index;
          node.children = parseBlock(blockSrc, innerFrom, innerTo);
          endPos = innerTo + close[0].length;
          // Advance tagRe past the consumed inner content
          tagRe.lastIndex = endPos - from;
        }
      }

      const hasGeom =
        node.attrs.d ||
        node.attrs.cx != null ||
        node.attrs.x1 != null ||
        node.attrs.points ||
        (node.attrs.x != null && node.attrs.width != null) ||
        node.children.length > 0;
      if (tag === "g" && !hasGeom && node.tracks.length === 0 && !isMotion) {
        continue;
      }
      if (tag !== "g" && !hasGeom && node.tracks.length === 0) {
        if (!node.attrs.d && tag === "path") continue;
      }
      localNodes.push(node);
    }
    return localNodes;
  }

  return parseBlock(src, 0, src.length);
}

function extractRootSvgVariants(src) {
  // motion.svg with variants={X} or inline
  const svgOpen = src.match(/<motion\.svg\b([^>]*)>/);
  if (!svgOpen) {
    // transition + variants on motion.svg spanning lines — search block
    const block = src.match(/<motion\.svg[\s\S]*?<\/motion\.svg>/);
    if (!block) return { tracks: [], transition: null };
    return extractRootFromBlock(block[0]);
  }
  return extractRootFromBlock(src.slice(svgOpen.index, svgOpen.index + 1200));
}

function extractRootFromBlock(block) {
  const tracks = [];
  let transition = null;
  const vRef = block.match(/\bvariants\s*=\s*\{([A-Za-z_][A-Za-z0-9_]*)\}/);
  let variants = null;
  if (vRef) {
    // look in full file via outer — handled by caller passing enough context
  }
  const vInline = block.match(/\bvariants\s*=\s*\{\{([\s\S]*?)\}\}/);
  if (vInline) {
    variants = parseVariantsObject("{" + vInline[1] + "}");
  }
  const tMatch = block.match(/\btransition\s*=\s*\{([\s\S]*?)\n\s*\}/);
  // better balanced
  const tIdx = block.search(/\btransition\s*=\s*\{/);
  if (tIdx >= 0) {
    const start = block.indexOf("{", tIdx);
    const body = extractBalancedObject(block, start);
    if (body) transition = parseTransition(body);
  }
  // inline variants={{ normal: { scale: 1 }, animate: { scale: [...] } }}
  const vIdx = block.search(/\bvariants\s*=\s*\{/);
  if (vIdx >= 0 && !variants) {
    const start = block.indexOf("{", vIdx);
    // may be {{ → one {
    const body = extractBalancedObject(block, start);
    if (body) variants = parseVariantsObject(body);
  }
  if (variants) {
    const byProp = {};
    for (const [state, { props, transition: st }] of Object.entries(variants)) {
      for (const [prop, value] of Object.entries(props)) {
        if (!byProp[prop]) byProp[prop] = { prop, keyframes: {} };
        byProp[prop].keyframes[state] = value;
        if (st) byProp[prop].transition = { ...(byProp[prop].transition || {}), ...st };
      }
    }
    tracks.push(...Object.values(byProp));
  }
  if (transition) {
    for (const t of tracks) t.transition = { ...(t.transition || {}), ...transition };
  }
  return { tracks, transition };
}

function extractFile(filePath) {
  const src = fs.readFileSync(filePath, "utf8");
  const id = path.basename(filePath, ".tsx");
  const shell = extractSvgShell(src);

  // Named variant consts — include lowercase `faceVariants` and UPPER `VARIANTS`.
  const named = {};
  for (const m of src.matchAll(
    /(?:const|let)\s+(\w+)\s*(?::\s*[^=]+)?=\s*(\{)/g
  )) {
    const name = m[1];
    if (!/variants?/i.test(name) && !/^(PATH_|SVG_|GROUP_|BODY_|FRAME_)/i.test(name)) {
      continue;
    }
    const brace = src.indexOf("{", m.index + m[0].length - 1);
    const body = extractBalancedObject(src, brace);
    if (body) {
      named[name] = parseVariantsObject(body);
      named[name + "__raw"] = body;
    }
  }

  let root = extractRootSvgVariants(src);
  // Resolve variants={SVG_VARIANTS}
  const svgBlock = src.match(/<motion\.svg\b[\s\S]{0,800}/);
  if (svgBlock) {
    const vRef = svgBlock[0].match(/\bvariants\s*=\s*\{([A-Za-z_][A-Za-z0-9_]*)\}/);
    if (vRef && named[vRef[1]]) {
      const byProp = {};
      for (const [state, { props, transition }] of Object.entries(named[vRef[1]])) {
        for (const [prop, value] of Object.entries(props)) {
          if (!byProp[prop]) byProp[prop] = { prop, keyframes: {} };
          byProp[prop].keyframes[state] = value;
          if (transition)
            byProp[prop].transition = {
              ...(byProp[prop].transition || {}),
              ...transition,
            };
        }
      }
      root.tracks = Object.values(byProp);
      const tIdx = svgBlock[0].search(/\btransition\s*=\s*\{/);
      if (tIdx >= 0) {
        const abs = src.indexOf(svgBlock[0]) + tIdx;
        const start = src.indexOf("{", abs);
        const body = extractBalancedObject(src, start);
        if (body) {
          const tr = parseTransition(body);
          for (const t of root.tracks)
            t.transition = { ...(t.transition || {}), ...tr };
        }
      }
    }
  }

  let nodes = extractNodes(src, named);

  // Expand .map()-generated motion nodes (sun rays, wifi levels, …) and prefer them
  // when the flat scan missed dynamic `d={...}` paths.
  let idCounter = nodes.length;
  const mapped = expandMappedMotionNodes(src, named, () => idCounter++);
  if (mapped.length > 0) {
    // Drop empty/path-without-d placeholders that the flat scan left behind.
    const staticGeom = nodes.filter(
      (n) =>
        n.tag === "g" ||
        n.attrs.d ||
        n.attrs.cx != null ||
        n.attrs.x1 != null ||
        n.attrs.points ||
        (n.attrs.x != null && n.attrs.width != null) ||
        (n.children && n.children.length)
    );
    // Keep non-mapped static geometry (e.g. sun's <circle>), append mapped paths.
    const mappedDs = new Set(mapped.map((n) => n.attrs.d).filter(Boolean));
    nodes = [
      ...staticGeom.filter((n) => !n.attrs.d || !mappedDs.has(n.attrs.d)),
      ...mapped,
    ];
  }

  // Attach named variants referenced in source per motion.path
  const pathBlocks = [...src.matchAll(/<motion\.path\b([^>]*)\/??>/g)];
  for (let i = 0; i < pathBlocks.length; i++) {
    const attrSrc = pathBlocks[i][1];
    const vRef = attrSrc.match(/\bvariants\s*=\s*\{([A-Za-z_][A-Za-z0-9_]*)\}/);
    if (!vRef || !named[vRef[1]]) continue;
    // find matching node by d
    const d = attrSrc.match(/\bd\s*=\s*"([^"]*)"/);
    let node = d
      ? nodes.find((n) => n.attrs.d === d[1])
      : nodes.filter((n) => n.tag === "path")[i];
    if (!node) continue;
    if (node.tracks && node.tracks.length > 0) continue;
    node.tracks = tracksFromVariantStates(named[vRef[1]]);
  }

  // If no geometry nodes, pull static paths from lucide-like markup
  const hasGeomDeep = (list) =>
    list.some(
      (n) =>
        n.attrs.d ||
        n.attrs.cx != null ||
        n.attrs.x1 != null ||
        n.attrs.points ||
        (n.children && hasGeomDeep(n.children))
    );
  if (!hasGeomDeep(nodes)) {
    for (const pm of src.matchAll(/<path\b([^>]*)\/?>/g)) {
      const d = pm[1].match(/\bd\s*=\s*"([^"]*)"/);
      if (d)
        nodes.push({
          id: `n${nodes.length}`,
          tag: "path",
          attrs: { d: d[1] },
          tracks: [],
          children: [],
        });
    }
  }

  // volume: AnimatePresence swaps mute-X ↔ sound waves on hover.
  if (id === "volume" || /AnimatePresence[\s\S]*volume-icon/i.test(src)) {
    nodes = synthesizeVolumeNodes(src, nodes);
  }
  // keyboard: KEYBOARD_PATHS + imperative opacity pulse via controls.start((i)=>…)
  if (id === "keyboard" || /KEYBOARD_PATHS/.test(src)) {
    nodes = synthesizeKeyboardNodes(src, nodes);
  }

  const nodeHasAnim = (n) =>
    (n.tracks && n.tracks.length > 0) ||
    (n.children || []).some(nodeHasAnim);
  const hasAnim = root.tracks.length > 0 || nodes.some(nodeHasAnim);

  return {
    id,
    lucide: id,
    viewBox: shell.viewBox,
    strokeWidth: shell.strokeWidth,
    strokeLinecap: shell.strokeLinecap,
    strokeLinejoin: shell.strokeLinejoin,
    trigger: {
      default: "hover",
      controlledDisablesHover: true,
    },
    root: { tracks: root.tracks },
    nodes,
    animated: hasAnim,
  };
}

function synthesizeVolumeNodes(src, nodes) {
  // Keep the speaker cone path; add waves (visible on animate) + mute X (visible on normal).
  const speaker = [];
  for (const pm of src.matchAll(/<path\b([^>]*)\/?>/g)) {
    const d = pm[1].match(/\bd\s*=\s*"([^"]*)"/);
    // Skip motion paths inside AnimatePresence (waves).
    if (!d) continue;
    if (d[1].startsWith("M16 ") || d[1].startsWith("M19.364")) continue;
    speaker.push({
      id: `n${speaker.length}`,
      tag: "path",
      attrs: { d: d[1] },
      tracks: [],
      children: [],
    });
  }
  const waves = [
    { d: "M16 9a5 5 0 0 1 0 6", delay: 0.1 },
    { d: "M19.364 18.364a9 9 0 0 0 0-12.728", delay: 0.2 },
  ].map((w, i) => ({
    id: `nw${i}`,
    tag: "path",
    attrs: { d: w.d },
    tracks: [
      {
        prop: "opacity",
        keyframes: { normal: 0, animate: 1 },
        transition: { duration: 0.25, delay: w.delay },
      },
    ],
    children: [],
  }));
  const mutes = [
    { x1: "22", x2: "16", y1: "9", y2: "15", delay: 0.1 },
    { x1: "16", x2: "22", y1: "9", y2: "15", delay: 0.2 },
  ].map((m, i) => ({
    id: `nm${i}`,
    tag: "line",
    attrs: { x1: m.x1, x2: m.x2, y1: m.y1, y2: m.y2 },
    tracks: [
      {
        prop: "opacity",
        keyframes: { normal: 1, animate: 0 },
        transition: { duration: 0.2, delay: m.delay },
      },
      {
        prop: "pathLength",
        keyframes: { normal: 1, animate: 0 },
        transition: { duration: 0.2, delay: m.delay },
      },
    ],
    children: [],
  }));
  const base = speaker.length ? speaker : nodes.filter((n) => n.tag === "path" && n.attrs.d);
  return [...base, ...waves, ...mutes];
}

function synthesizeKeyboardNodes(src, nodes) {
  const keys = parseConstArray(src, "KEYBOARD_PATHS");
  const rect = nodes.find((n) => n.tag === "rect") || {
    id: "nrect",
    tag: "rect",
    attrs: { height: "16", rx: "2", width: "20", x: "2", y: "4" },
    tracks: [],
    children: [],
  };
  if (!Array.isArray(keys) || keys.length === 0) {
    // Fall back: attach pulse to existing path children
    return nodes.map((n, i) => {
      if (n.tag !== "path" || !n.attrs.d) return n;
      return {
        ...n,
        tracks: [
          {
            prop: "opacity",
            keyframes: { normal: 1, animate: [1, 0.2, 1] },
            transition: {
              duration: 1.5,
              delay: i * 0.15,
              times: [0, 0.5, 1],
              repeat: 1,
            },
          },
        ],
      };
    });
  }
  const keyNodes = keys.map((k, i) => ({
    id: `nk${i}`,
    tag: "path",
    attrs: { d: typeof k === "object" ? k.d : String(k) },
    tracks: [
      {
        prop: "opacity",
        keyframes: { normal: 1, animate: [1, 0.2, 1] },
        transition: {
          duration: 1.5,
          delay: +(i * 0.15).toFixed(3),
          times: [0, 0.5, 1],
          repeat: 1,
        },
      },
    ],
    children: [],
  }));
  return [rect, ...keyNodes];
}

const files = fs
  .readdirSync(iconsDir)
  .filter((f) => f.endsWith(".tsx"))
  .sort();

const recipes = {};
let animated = 0;
let failed = 0;
for (const f of files) {
  try {
    const r = extractFile(path.join(iconsDir, f));
    recipes[r.id] = r;
    if (r.animated) animated++;
  } catch (e) {
    failed++;
    console.warn("fail", f, e.message);
  }
}

fs.mkdirSync(path.dirname(outPath), { recursive: true });
fs.writeFileSync(outPath, JSON.stringify(recipes));
console.log(
  JSON.stringify(
    {
      icons: Object.keys(recipes).length,
      animated,
      failed,
      outPath,
      bytes: fs.statSync(outPath).size,
    },
    null,
    2
  )
);
