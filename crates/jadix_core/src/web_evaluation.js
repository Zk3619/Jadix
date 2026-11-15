((source, key) => {
  const fail = (code, error) => {
    let message = "JavaScript exception";
    try { message = String(error).slice(0, 512); } catch (_) {}
    return {status: "error", code, message};
  };
  const encode = value => {
    const ancestors = new Set();
    let nodes = 0;
    const copy = (value, depth) => {
      if (++nodes > 1048576 || depth > 64) throw new TypeError("JSON result depth or node budget exceeded");
      if (value === null || typeof value === "string" || typeof value === "boolean") return value;
      if (typeof value === "number" && Number.isFinite(value)) return value;
      if (typeof value !== "object" || ancestors.has(value)) throw new TypeError("Expected an acyclic JSON value");
      const array = Array.isArray(value);
      const prototype = Object.getPrototypeOf(value);
      if (!array && prototype !== Object.prototype && prototype !== null) throw new TypeError("Expected a plain JSON object");
      ancestors.add(value);
      const result = array ? [] : Object.create(null);
      if (array) {
        for (let i = 0; i < value.length; ++i) result.push(copy(value[i], depth + 1));
      } else {
        for (const name of Object.keys(value)) result[name] = copy(value[name], depth + 1);
      }
      ancestors.delete(value);
      return result;
    };
    try {
      const result = {status: "ok", value: copy(value, 0)};
      if (new TextEncoder().encode(JSON.stringify(result)).length > 1048576) return fail("too-large", "JSON result exceeds 1 MiB");
      return result;
    } catch (error) { return fail("invalid-result", error); }
  };
  try {
    const value = (0, eval)(source);
    // Read a hostile then getter exactly once, including when it throws.
    const then = value != null && (typeof value === "object" || typeof value === "function") ? value.then : null;
    if (typeof then !== "function") return encode(value);
    const cell = {result: {status: "pending"}};
    if (Object.prototype.hasOwnProperty.call(globalThis, key)) return fail("bridge-error", "evaluation result key is already in use");
    Object.defineProperty(globalThis, key, {value: cell, configurable: true});
    new Promise((resolve, reject) => then.call(value, resolve, reject)).then(
      answer => { if (globalThis[key] === cell) cell.result = encode(answer); },
      error => { if (globalThis[key] === cell) cell.result = fail("exception", error); }
    );
    return {status: "pending"};
  } catch (error) { return fail("exception", error); }
})(__JADIX_SOURCE__, __JADIX_TOKEN__)
