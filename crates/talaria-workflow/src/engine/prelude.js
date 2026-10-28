(function (host, compile, configText) {
  "use strict";
  const config = JSON.parse(configText);
  const encode = JSON.stringify;
  const decode = JSON.parse;
  const call = (op, value) => decode(host(op, encode(value)));
  const send = value => host("send", encode(value));
  const paths = new WeakMap();
  const pending = new Map();
  let nextId = 0;
  let rootState = null;
  const ws = /^[\u0009-\u000d \u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]+|[\u0009-\u000d \u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]+$/gu;
  const trim = value => value.replace(ws, "");
  const nonblank = value => typeof value === "string" && trim(value).length > 0;
  const error = (name, message) => Object.assign(new Error(message), {name});
  const bad = message => { throw error("WorkflowOptionError", message); };
  const plain = value => value !== null && typeof value === "object" &&
    (Object.getPrototypeOf(value) === Object.prototype || Object.getPrototypeOf(value) === null);
  const cut = (value, length) => {
    const chars = Array.from(value);
    return chars.length > length ? chars.slice(0, length).join("") + "…" : value;
  };
  const strictJSON = value => {
    const text = encode(value, function (key, item) {
      if (typeof item === "function" || typeof item === "symbol") throw new TypeError("function or symbol");
      if (typeof item === "number" && !Number.isFinite(item)) throw new TypeError("non-finite number");
      if (item === undefined && Array.isArray(this) && Object.hasOwn(this, key)) throw new TypeError("undefined array element");
      return item;
    });
    return text === undefined ? "null" : text;
  };
  const schemaJSON = schema => {
    const seen = new Set();
    const fail = (path, reason) => { throw error("SchemaError", `agent(): schema ${path}: ${reason}`); };
    function visit(value, path) {
      if (value === null || typeof value === "string" || typeof value === "boolean") return;
      if (typeof value === "number" && Number.isFinite(value)) return;
      if (typeof value !== "object") fail(path, "must be plain JSON data");
      if (!Array.isArray(value) && !plain(value)) fail(path, "must be a plain object");
      if ("toJSON" in value) fail(path, "toJSON is not allowed");
      if (seen.has(value)) fail(path, "cycle");
      seen.add(value);
      const descriptors = Object.getOwnPropertyDescriptors(value);
      for (const key of Reflect.ownKeys(descriptors)) {
        if (Array.isArray(value) && key === "length") continue;
        const childPath = path ? `${path}.${String(key)}` : String(key);
        if (typeof key === "symbol") fail(childPath, "symbol key");
        const d = descriptors[key];
        if (!("value" in d)) fail(childPath, "accessor property");
        if (!d.enumerable) fail(childPath, "non-enumerable property");
        visit(d.value, childPath);
      }
      if (Array.isArray(value)) {
        for (let i = 0; i < value.length; i++) if (!Object.hasOwn(value, i)) fail(`${path}.${i}`, "array hole");
      }
      seen.delete(value);
    }
    visit(schema, "");
    if (!plain(schema) || schema.type !== "object") fail("root", "must have type 'object'");
    if (!plain(schema.properties)) fail("properties", "must be an object");
    if (schema.required !== undefined && (!Array.isArray(schema.required) ||
      schema.required.some(key => typeof key !== "string" || !Object.hasOwn(schema.properties, key)))) {
      fail("required", "must be an array naming only properties");
    }
    return decode(encode(schema));
  };
  const allowed = ["label", "phase", "schema", "tier", "model", "provider", "effort", "profile", "toolsets", "skills", "isolation", "maxTurns", "stallMs"];
  const normalize = (prompt, opts) => {
    if (!nonblank(prompt)) bad("agent(): prompt must be a non-blank string");
    if (opts == null) opts = {};
    if (!plain(opts)) bad("agent(): options must be a plain object");
    const result = {prompt};
    for (const key of Reflect.ownKeys(opts)) {
      if (!allowed.includes(key)) {
        if (key === "agentType") bad("agentType is a Claude Code option; Hermes has no agent types. Use tier, toolsets or skills.");
        bad(`agent(): unknown option \`${String(key)}\`; allowed: ${allowed.join(", ")}`);
      }
      const value = opts[key];
      if (value === undefined) continue;
      if (value === null) bad(`agent(): ${key} must not be null`);
      if (["label", "phase", "tier", "model", "provider", "profile"].includes(key)) {
        if (!nonblank(value)) bad(`agent(): ${key} must be a non-blank string`);
        if (key === "label" && Array.from(value).length > 120) bad("agent(): label exceeds 120 characters");
      } else if (key === "effort") {
        if (!["none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"].includes(value)) bad("agent(): invalid effort");
      } else if (key === "isolation") {
        if (value !== "worktree") bad("agent(): isolation must be 'worktree'");
      } else if (key === "maxTurns" || key === "stallMs") {
        if (!Number.isInteger(value) || value <= 0 || (key === "maxTurns" && value > 10000)) bad(`agent(): ${key} must be a positive integer${key === "maxTurns" ? " at most 10000" : ""}`);
      } else if (key === "toolsets" || key === "skills") {
        if (!Array.isArray(value) || value.length === 0 || value.some(v => !nonblank(v)) || new Set(value).size !== value.length) bad(`agent(): ${key} must be a non-empty array of distinct non-blank strings`);
        for (let i = 0; i < value.length; i++) if (!Object.hasOwn(value, i)) bad(`agent(): ${key} has an array hole`);
      }
      result[key === "maxTurns" ? "max_turns" : key] = key === "schema" ? schemaJSON(value) : value;
    }
    result.label ??= cut(trim(prompt.split("\n").find(nonblank)), 60);
    return result;
  };
  const attributed = (value, path) => {
    if (value !== null && (typeof value === "object" || typeof value === "function") && !paths.has(value)) paths.set(value, path);
    return value;
  };
  const describe = (value, fallback) => {
    const object = value !== null && (typeof value === "object" || typeof value === "function");
    return {name: object && typeof value.name === "string" ? value.name : "Error",
      message: object && typeof value.message === "string" ? value.message : String(value),
      stack: object && typeof value.stack === "string" ? value.stack : null,
      workflow_path: object && paths.has(value) ? paths.get(value) : fallback};
  };
  const budget = {};
  function budgetView(money) {
    const snap = () => call("budget", null);
    return Object.freeze({
      get total() { const s = snap(); return money ? s.money_total : s.tokens_total; },
      spent() { const s = snap(); return money ? s.money_spent : s.tokens_spent; },
      remaining() { const s = snap(); const total = money ? s.money_total : s.tokens_total;
        return total === null ? Infinity : Math.max(0, total - (money ? s.money_spent : s.tokens_spent)); }
    });
  }
  const tokens = budgetView(false);
  Object.defineProperties(budget, {
    total: {get: () => tokens.total}, spent: {value: tokens.spent}, remaining: {value: tokens.remaining},
    money: {value: budgetView(true)}
  });
  Object.freeze(budget);

  const RealDate = Date;
  const fail = (what, hint) => { throw error("DeterminismError", `${what} is unavailable in workflow scripts (it would break resume); ${hint}`); };
  function SafeDate(...a) {
    if (!new.target) fail("Date()", "pass timestamps in through args, or use now()");
    if (a.length === 0) fail("new Date() without arguments", "pass timestamps in through args, or use now()");
    return Reflect.construct(RealDate, a, new.target);
  }
  SafeDate.prototype = RealDate.prototype;
  Object.defineProperty(RealDate.prototype, "constructor", {value: SafeDate});
  SafeDate.UTC = RealDate.UTC;
  SafeDate.parse = RealDate.parse;
  SafeDate.now = () => fail("Date.now()", "pass timestamps in through args, or use now()");
  Object.defineProperty(globalThis, "Date", {value: SafeDate, writable: false, configurable: false});
  Object.defineProperty(Math, "random", {value: () => fail("Math.random()", "use random(key) or vary prompts by index"), writable: false, configurable: false});
  for (const name of ["performance", "Intl", "WeakRef", "FinalizationRegistry", "navigator", "console"]) delete globalThis[name];

  function instance(path, args, child) {
    let currentPhase = null;
    function agent(prompt, opts) {
      // Validation is synchronous, but every refusal is a promise rejection.
      try {
        const request = normalize(prompt, opts);
        request.phase ??= currentPhase;
        request.workflow_path = path;
        const id = nextId++;
        const promise = new Promise((resolve, reject) => pending.set(id, {resolve, reject}));
        send({kind: "agent", id, request});
        return promise;
      } catch (e) { return Promise.reject(e); }
    }
    function parallel(thunks) {
      if (!Array.isArray(thunks)) return Promise.reject(error("WorkflowOptionError", "parallel(): expected an array"));
      if (thunks.length > config.max_items) return Promise.reject(error("ItemLimitError", `parallel(): ${thunks.length} items, the limit is ${config.max_items}`));
      for (let i = 0; i < thunks.length; i++) if (typeof thunks[i] !== "function") return Promise.reject(error("WorkflowOptionError", `parallel(): item ${i} must be a function`));
      return Promise.all(thunks.map(thunk => { try { return Promise.resolve(thunk()).catch(() => null); } catch (_) { return null; } }));
    }
    function pipeline(items, ...stages) {
      if (!Array.isArray(items)) return Promise.reject(error("WorkflowOptionError", "pipeline(): expected an array"));
      if (items.length > config.max_items) return Promise.reject(error("ItemLimitError", `pipeline(): ${items.length} items, the limit is ${config.max_items}`));
      if (stages.length === 0 || stages.some(stage => typeof stage !== "function")) return Promise.reject(error("WorkflowOptionError", "pipeline(): at least one function stage is required"));
      return Promise.all(Array.from(items, async (item, i) => {
        let previous = item;
        try { for (const stage of stages) previous = await stage(previous, item, i); return previous; }
        catch (_) { return null; }
      }));
    }
    function phase(title) {
      const text = trim(String(title)); if (!text) bad("phase(): title must not be blank");
      currentPhase = text; send({kind: "phase", title: text, workflow_path: path});
    }
    function log(message) { send({kind: "log", message: cut(String(message), 4000), workflow_path: path}); }
    async function workflow(reference, childArgs) {
      if (child) throw error("NestedWorkflowError", "workflows nest one level only");
      let ref;
      if (typeof reference === "string" && reference.length > 0) ref = {name: reference};
      else if (plain(reference) && Reflect.ownKeys(reference).length === 1 && typeof reference.scriptPath === "string" && reference.scriptPath.length > 0) ref = {script_path: reference.scriptPath};
      else bad("workflow(): expected a name or {scriptPath}");
      let argText;
      try { argText = strictJSON(childArgs); } catch (e) { bad(`workflow(): args is not JSON-serialisable: ${e.message}`); }
      const info = call("resolve", {reference: ref, args: decode(argText)});
      if (info.error) throw error(info.error.name, info.error.message);
      let fn;
      try { fn = compile(info.body, info.filename, true); }
      catch (e) {
        const position = /\.js:(\d+)(?::(\d+))?/.exec(e.stack || "");
        const where = position ? ` line ${position[1]}${position[2] ? ", column " + position[2] : ""}` : "";
        throw error("WorkflowScriptError", `workflow \`${info.display}\`${where}: ${e.message}`);
      }
      const id = nextId++;
      send({kind: "child_started", id, workflow_path: info.path, meta: info.meta, warnings: info.warnings});
      const api = instance(info.path, arguments.length < 2 || childArgs === undefined ? undefined : decode(argText), true);
      let ok = false;
      try {
        const result = await fn(...Object.values(api));
        let converted;
        try { converted = decode(strictJSON(result)); }
        catch (e) { throw attributed(error("WorkflowScriptError", `workflow \`${info.display}\` returned a value that is not JSON-serialisable: ${e.message}`), path); }
        ok = true;
        return converted;
      } catch (e) { throw attributed(e, info.path); }
      finally { send({kind: "child_finished", id, workflow_path: info.path, ok}); }
    }
    function random(key) {
      if (typeof key !== "string" || key.length === 0) bad("random(): a non-empty string key is required");
      return call("random", {path, key});
    }
    return {agent, parallel, pipeline, phase, log, args, budget, workflow, now: () => config.started_at_ms, random};
  }
  const api = instance([], config.has_args ? config.args : undefined, false);
  for (const [name, value] of Object.entries(api)) Object.defineProperty(globalThis, name, {value, writable: false, configurable: false});
  return {
    start(body, filename) {
      try {
        const promise = compile(body, filename, false);
        promise.then(result => {
          try { rootState = {result: decode(strictJSON(result))}; }
          catch (e) { rootState = {error: describe(error("TypeError", `the workflow result is not JSON-serialisable: ${e.message}`), [])}; }
        }, e => { rootState = {error: describe(e, [])}; });
      } catch (e) { rootState = {error: describe(e, [])}; }
    },
    state() { return encode(rootState); },
    deliver(text) {
      const value = decode(text), entry = pending.get(value.id);
      if (!entry) return;
      pending.delete(value.id);
      if (value.error) entry.reject(error(value.error.name, value.error.message));
      else entry.resolve(value.value);
    }
  };
})
