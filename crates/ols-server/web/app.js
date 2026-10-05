// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The OLS CAD web client: joins a server session over a WebSocket (the
// server's JSON messages), draws the line it receives (structures, the
// alignment, wires solved on the server) in 3D (WebGPU, else Canvas2D),
// plan and profile, shows who else is connected and the edits as they
// arrive. Designers can move a structure.
"use strict";

(() => {
  const PROTOCOL = 1;
  const PALETTE = ["#d0703a", "#4a9be0", "#58b368", "#c9b03c", "#b06ad0", "#40b5b0"];
  const BACKGROUND = { r: 0.106, g: 0.118, b: 0.133, a: 1 };
  const STRUCTURE = "#a7b0b9";
  const ALIGNMENT = "#5f8a63";
  const SELECTED = "#ffd24a";
  const MAX_LOG = 200;

  const $ = (id) => document.getElementById(id);
  const overlay = $("overlay");
  const gpuCanvas = $("gpu");

  const state = {
    ws: null,
    user: "",
    role: null,
    scene: null,
    origin: [0, 0, 0],
    peers: [],
    locks: {},
    selection: null, // {kind: "structure", id} | {kind: "span", wire, span}
    view: "3d",
    gpu: null,
    nextId: 1,
    pending: {},
    // 3D orbit camera (local coordinates).
    cam: { target: [0, 0, 0], yaw: -2.3, pitch: 0.45, dist: 500 },
    // Plan and profile: centre (local) and pixels per metre.
    plan: null,
    profile: null,
    geometry: null,
    presenceSent: "",
    presenceTimer: 0,
  };

  // ---- small helpers ------------------------------------------------

  function setStatus(text, error) {
    const s = $("status");
    s.textContent = text;
    s.className = error ? "error" : "weak";
  }

  function hexColor(text) {
    const m = /^#([0-9a-f]{6})$/i.exec(text || "");
    if (!m) return null;
    const n = parseInt(m[1], 16);
    return [(n >> 16) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
  }

  function el(tag, text, cls) {
    const e = document.createElement(tag);
    if (text !== undefined) e.textContent = text;
    if (cls) e.className = cls;
    return e;
  }

  function fmt(x, digits) {
    return Number(x).toFixed(digits === undefined ? 2 : digits);
  }

  function describe(op) {
    switch (op.op) {
      case "do": return ((op.command && op.command.op) || "command").replace(/_/g, " ");
      case "undo": return "undo";
      case "redo": return "redo";
      case "rollback": return `rollback of entry ${op.seq}`;
      case "rollback_after": return `rollback to entry ${op.seq}`;
      case "set_user": return `${op.name} made ${op.role}`;
      case "put_library": return `library ${op.kind}/${op.name} saved`;
      case "delete_library": return `library ${op.kind}/${op.name} removed`;
      case "save_snapshot": return `project line ${op.name} saved`;
      case "delete_snapshot": return `project line ${op.name} deleted`;
      case "set_setting": return `setting ${op.key}`;
      default: return String(op.op);
    }
  }

  // ---- joining ------------------------------------------------------

  function fragment() {
    const out = {};
    for (const part of location.hash.replace(/^#/, "").split("&")) {
      const i = part.indexOf("=");
      if (i > 0) {
        try {
          out[part.slice(0, i)] = decodeURIComponent(part.slice(i + 1));
        } catch (e) {
          // A malformed value is left out.
        }
      }
    }
    return out;
  }

  function remembered() {
    try {
      return localStorage.getItem("ols.user") || "";
    } catch (e) {
      return "";
    }
  }

  function remember(user) {
    try {
      localStorage.setItem("ols.user", user);
    } catch (e) {
      // Storage refused: nothing to keep.
    }
  }

  function connect(user, code) {
    if (state.ws) state.ws.close();
    $("join-error").textContent = "";
    setStatus("connecting…");
    state.user = user;
    const ws = new WebSocket(`wss://${location.host}/ws`);
    state.ws = ws;
    ws.onopen = () => {
      ws.send(JSON.stringify({ type: "hello", protocol: PROTOCOL, user, code }));
    };
    ws.onmessage = (e) => {
      let msg;
      try {
        msg = JSON.parse(e.data);
      } catch (err) {
        return;
      }
      receive(msg);
    };
    ws.onclose = () => {
      if (state.ws !== ws) return;
      state.ws = null;
      if (state.role) {
        setStatus("disconnected: join again to continue", true);
        $("join").hidden = false;
        $("join-error").textContent = "The connection ended.";
      } else {
        setStatus("not connected");
      }
      state.role = null;
      updateMoveForm();
    };
  }

  function send(msg) {
    if (state.ws && state.ws.readyState === WebSocket.OPEN) {
      state.ws.send(JSON.stringify(msg));
    }
  }

  function request(op, what) {
    const id = state.nextId++;
    state.pending[id] = what;
    send({ type: "request", id, op });
  }

  // ---- messages -----------------------------------------------------

  function receive(msg) {
    switch (msg.type) {
      case "welcome":
        state.role = msg.role;
        state.peers = msg.peers || [];
        state.locks = msg.locks || {};
        remember(state.user);
        $("project").textContent = msg.project;
        setStatus(`joined as ${state.user} (${msg.role}); loading the line…`);
        $("join").hidden = true;
        $("views").hidden = false;
        $("side").hidden = false;
        showPeers();
        updateMoveForm();
        resize();
        break;
      case "refused":
        $("join-error").textContent = `Refused: ${msg.reason}`;
        setStatus("not connected");
        break;
      case "scene": {
        const first = !state.scene;
        state.scene = msg;
        prepare(first);
        setStatus(`joined as ${state.user} (${state.role}) — line at log entry ${msg.seq}`);
        showNotes();
        showSelection();
        draw();
        break;
      }
      case "applied": {
        const li = el("li", `${msg.seq === null || msg.seq === undefined ? "–" : "#" + msg.seq} ${msg.user}: ${describe(msg.op)}`);
        const log = $("log");
        log.insertBefore(li, log.firstChild);
        while (log.children.length > MAX_LOG) log.removeChild(log.lastChild);
        if (msg.op.op === "set_user" && msg.op.name === state.user) {
          state.role = msg.op.role;
          updateMoveForm();
        }
        break;
      }
      case "response": {
        const what = state.pending[msg.id];
        if (!what) break;
        delete state.pending[msg.id];
        if (msg.result && msg.result.Err !== undefined) {
          setStatus(`${what} refused: ${msg.result.Err}`, true);
        } else {
          setStatus(`${what}: done`);
        }
        break;
      }
      case "peers":
        state.peers = msg.peers || [];
        showPeers();
        draw();
        break;
      case "locks":
        state.locks = msg.locks || {};
        showPeers();
        break;
      case "notice":
        setStatus(msg.text, true);
        break;
      default:
        break;
    }
  }

  // ---- geometry -----------------------------------------------------

  function local(p) {
    return [p[0] - state.origin[0], p[1] - state.origin[1], p[2] - state.origin[2]];
  }

  function wireColor(line, section, k) {
    if (section && section.color) return section.color;
    const phases = (line.display && line.display.phase_colors) || {};
    if (section && phases[section.phase_label]) return phases[section.phase_label];
    return PALETTE[k % PALETTE.length];
  }

  // Lines to draw and things to pick, in local coordinates.
  function prepare(first) {
    const scene = state.scene;
    const line = scene.line;
    if (first && line.structures.length > 0) state.origin = line.structures[0].position.slice();
    const structures = line.structures.map((s) => {
      let top = s.position[2] + 10;
      const parts = [];
      for (const a of s.attachments || []) {
        top = Math.max(top, a.insulator_point[2], a.wire_point[2]);
        parts.push([local(a.insulator_point), local(a.wire_point)]);
        for (const leg of a.legs || []) parts.push([local(leg), local(a.wire_point)]);
      }
      for (const m of s.members || []) {
        for (let i = 0; i + 1 < m.length; i++) {
          top = Math.max(top, m[i][2], m[i + 1][2]);
          parts.push([local(m[i]), local(m[i + 1])]);
        }
      }
      const base = local(s.position);
      const head = [base[0], base[1], top - state.origin[2]];
      if (!(s.members && s.members.length)) parts.push([base, head]);
      return { id: s.id, structure: s, base, head, parts };
    });
    const byId = {};
    for (const s of structures) byId[s.id] = s;
    // The main route: its structures in order (else the line's order).
    const route = line.routes && line.routes[0];
    const order = route ? route.structures : line.structures.map((s) => s.id);
    const path = order.map((id) => byId[id]).filter(Boolean).map((s) => s.base);
    // Alignments: each route's points (a point is [x, y, z] or tied to a
    // structure), else through its structures.
    const point = (a) => local(Array.isArray(a) ? a : a.position);
    const through = (ids) => ids.map((id) => byId[id]).filter(Boolean).map((s) => s.base);
    const alignments = (line.routes && line.routes.length)
      ? line.routes.map((r) => (r.alignment && r.alignment.length ? r.alignment.map(point) : through(r.structures)))
      : [line.alignment && line.alignment.length ? line.alignment.map(point) : path];
    const alignment = [];
    for (const points of alignments) {
      for (let i = 0; i + 1 < points.length; i++) alignment.push([points[i], points[i + 1]]);
    }
    const sections = {};
    line.sections.forEach((s) => { sections[s.id] = s; });
    const wires = scene.wires.map((w, k) => {
      const section = sections[w.section];
      return {
        wire: w,
        section,
        color: wireColor(line, section, k),
        spans: w.spans.map((s) => s.points.map(local)),
      };
    });
    // Stations along the main route, for the profile.
    const stations = [0];
    for (let i = 1; i < path.length; i++) {
      stations.push(stations[i - 1] + Math.hypot(path[i][0] - path[i - 1][0], path[i][1] - path[i - 1][1]));
    }
    state.geometry = { structures, byId, alignment, wires, path, stations };
    if (first || !state.plan) fit();
    uploadGpu();
  }

  // Distance along the main route of a point (its nearest segment).
  function station(p) {
    const g = state.geometry;
    if (g.path.length < 2) return p[0];
    let best = Infinity, out = 0;
    for (let i = 0; i + 1 < g.path.length; i++) {
      const a = g.path[i], b = g.path[i + 1];
      const dx = b[0] - a[0], dy = b[1] - a[1];
      const len2 = dx * dx + dy * dy || 1;
      const t = Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2));
      const qx = a[0] + t * dx - p[0], qy = a[1] + t * dy - p[1];
      const d = qx * qx + qy * qy;
      if (d < best) {
        best = d;
        out = g.stations[i] + t * Math.sqrt(len2);
      }
    }
    return out;
  }

  function bounds() {
    const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
    const add = (p) => { for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], p[k]); hi[k] = Math.max(hi[k], p[k]); } };
    for (const s of state.geometry.structures) { add(s.base); add(s.head); }
    for (const w of state.geometry.wires) for (const span of w.spans) for (const p of span) add(p);
    if (lo[0] === Infinity) return { lo: [-50, -50, 0], hi: [50, 50, 50] };
    return { lo, hi };
  }

  function fit() {
    const { lo, hi } = bounds();
    const w = overlay.clientWidth || 800, h = overlay.clientHeight || 600;
    const centre = [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, (lo[2] + hi[2]) / 2];
    const size = Math.max(hi[0] - lo[0], hi[1] - lo[1], 20);
    state.plan = { c: centre, s: 0.9 * Math.min(w / Math.max(hi[0] - lo[0], 1), h / Math.max(hi[1] - lo[1], 1)) };
    const g = state.geometry;
    const total = g.stations.length ? g.stations[g.stations.length - 1] : hi[0] - lo[0];
    state.profile = {
      c: [total / 2, centre[2]],
      sx: 0.9 * w / Math.max(total, 1),
      sz: 0.8 * h / Math.max(hi[2] - lo[2], 1),
    };
    state.cam.target = centre;
    state.cam.dist = (size * 1.2) / Math.min(1, w / h);
  }

  // ---- 3D camera ----------------------------------------------------

  function eye() {
    const c = state.cam;
    return [
      c.target[0] + c.dist * Math.cos(c.pitch) * Math.cos(c.yaw),
      c.target[1] + c.dist * Math.cos(c.pitch) * Math.sin(c.yaw),
      c.target[2] + c.dist * Math.sin(c.pitch),
    ];
  }

  function sub(a, b) { return [a[0] - b[0], a[1] - b[1], a[2] - b[2]]; }
  function cross(a, b) { return [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]; }
  function dot(a, b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }
  function norm(a) { const l = Math.hypot(a[0], a[1], a[2]) || 1; return [a[0] / l, a[1] / l, a[2] / l]; }

  // Column-major view-projection (depth 0..1).
  function matrix() {
    const w = overlay.clientWidth || 1, h = overlay.clientHeight || 1;
    const e = eye();
    const z = norm(sub(e, state.cam.target));
    const x = norm(cross([0, 0, 1], z));
    const y = cross(z, x);
    const view = [
      x[0], y[0], z[0], 0,
      x[1], y[1], z[1], 0,
      x[2], y[2], z[2], 0,
      -dot(x, e), -dot(y, e), -dot(z, e), 1,
    ];
    const near = Math.max(0.1, state.cam.dist / 1000), far = state.cam.dist * 50;
    const f = 1 / Math.tan((50 * Math.PI) / 360);
    const proj = [
      f / (w / h), 0, 0, 0,
      0, f, 0, 0,
      0, 0, far / (near - far), -1,
      0, 0, (near * far) / (near - far), 0,
    ];
    const m = new Array(16).fill(0);
    for (let c = 0; c < 4; c++) {
      for (let r = 0; r < 4; r++) {
        let s = 0;
        for (let k = 0; k < 4; k++) s += proj[k * 4 + r] * view[c * 4 + k];
        m[c * 4 + r] = s;
      }
    }
    return m;
  }

  function clip(m, p) {
    return [
      m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
      m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
      m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
      m[3] * p[0] + m[7] * p[1] + m[11] * p[2] + m[15],
    ];
  }

  function screenOf(c) {
    const w = overlay.clientWidth, h = overlay.clientHeight;
    return [((c[0] / c[3] + 1) / 2) * w, ((1 - c[1] / c[3]) / 2) * h];
  }

  // ---- what each view draws -----------------------------------------

  function isSelected(kind, a, b) {
    const s = state.selection;
    if (!s || s.kind !== kind) return false;
    return kind === "structure" ? s.id === a : s.wire === a && s.span === b;
  }

  // Segments: [a, b, colour] in local coordinates.
  function segments() {
    const g = state.geometry;
    const out = [];
    for (const [a, b] of g.alignment) out.push([a, b, ALIGNMENT]);
    for (const s of g.structures) {
      const c = isSelected("structure", s.id) ? SELECTED : STRUCTURE;
      for (const [a, b] of s.parts) out.push([a, b, c]);
    }
    g.wires.forEach((w, i) => {
      w.spans.forEach((span, j) => {
        const c = isSelected("span", i, j) ? SELECTED : w.color;
        for (let k = 0; k + 1 < span.length; k++) out.push([span[k], span[k + 1], c]);
      });
    });
    return out;
  }

  function uploadGpu() {
    if (!state.gpu || !state.geometry) return;
    const segs = segments();
    const data = new Float32Array(segs.length * 12);
    let o = 0;
    for (const [a, b, colour] of segs) {
      const c = hexColor(colour) || [1, 1, 1];
      for (const p of [a, b]) {
        data[o++] = p[0]; data[o++] = p[1]; data[o++] = p[2];
        data[o++] = c[0]; data[o++] = c[1]; data[o++] = c[2];
      }
    }
    state.gpu.setLines(data);
  }

  // ---- drawing ------------------------------------------------------

  function resize() {
    const r = window.devicePixelRatio || 1;
    for (const c of [overlay, gpuCanvas]) {
      c.width = Math.max(1, Math.round(c.clientWidth * r));
      c.height = Math.max(1, Math.round(c.clientHeight * r));
    }
    draw();
  }

  let frame = 0;
  function draw() {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      render();
    });
  }

  function render() {
    const ctx = overlay.getContext("2d");
    const r = window.devicePixelRatio || 1;
    ctx.setTransform(r, 0, 0, r, 0, 0);
    const w = overlay.clientWidth, h = overlay.clientHeight;
    ctx.clearRect(0, 0, w, h);
    if (!state.geometry) return;
    const gpu3d = state.view === "3d" && state.gpu && !state.gpu.lost;
    gpuCanvas.hidden = !gpu3d;
    if (!gpu3d) {
      ctx.fillStyle = "#1b1e22";
      ctx.fillRect(0, 0, w, h);
    }
    ctx.font = "12px system-ui, sans-serif";
    if (state.view === "3d") render3d(ctx, gpu3d);
    else if (state.view === "plan") renderPlan(ctx);
    else renderProfile(ctx);
    schedulePresence();
  }

  function render3d(ctx, gpu3d) {
    const m = matrix();
    if (gpu3d) {
      state.gpu.draw(new Float32Array(m), BACKGROUND);
    } else {
      const eps = 1e-3;
      ctx.lineWidth = 1;
      for (const [a, b, colour] of segments()) {
        let ca = clip(m, a), cb = clip(m, b);
        if (ca[3] < eps && cb[3] < eps) continue;
        if (ca[3] < eps || cb[3] < eps) {
          const t = (eps - ca[3]) / (cb[3] - ca[3]);
          const p = ca.map((v, k) => v + t * (cb[k] - v));
          if (ca[3] < eps) ca = p; else cb = p;
        }
        const sa = screenOf(ca), sb = screenOf(cb);
        ctx.strokeStyle = colour;
        ctx.beginPath();
        ctx.moveTo(sa[0], sa[1]);
        ctx.lineTo(sb[0], sb[1]);
        ctx.stroke();
      }
    }
    // Labels: structures, and where the others look.
    ctx.fillStyle = "#c8ced4";
    for (const s of state.geometry.structures) {
      const c = clip(m, s.head);
      if (c[3] <= 0) continue;
      const p = screenOf(c);
      ctx.fillText(s.id, p[0] + 4, p[1] - 4);
    }
    for (const peer of others()) {
      const c = clip(m, local(peer.presence.camera));
      if (c[3] <= 0) continue;
      const p = screenOf(c);
      marker(ctx, p, peer.user);
    }
  }

  function others() {
    return state.peers.filter((p) => p.user !== state.user && p.presence && p.presence.camera);
  }

  function marker(ctx, p, name) {
    ctx.fillStyle = "#e07bd8";
    ctx.beginPath();
    ctx.moveTo(p[0], p[1] - 6);
    ctx.lineTo(p[0] + 6, p[1]);
    ctx.lineTo(p[0], p[1] + 6);
    ctx.lineTo(p[0] - 6, p[1]);
    ctx.closePath();
    ctx.fill();
    ctx.fillText(name, p[0] + 8, p[1] + 4);
  }

  function planXY(p) {
    const v = state.plan, w = overlay.clientWidth, h = overlay.clientHeight;
    return [w / 2 + (p[0] - v.c[0]) * v.s, h / 2 - (p[1] - v.c[1]) * v.s];
  }

  function profileXY(st, z) {
    const v = state.profile, w = overlay.clientWidth, h = overlay.clientHeight;
    return [w / 2 + (st - v.c[0]) * v.sx, h / 2 - (z - v.c[1]) * v.sz];
  }

  function polyline(ctx, pts, colour, width) {
    if (pts.length < 2) return;
    ctx.strokeStyle = colour;
    ctx.lineWidth = width || 1;
    ctx.beginPath();
    ctx.moveTo(pts[0][0], pts[0][1]);
    for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i][0], pts[i][1]);
    ctx.stroke();
  }

  function renderPlan(ctx) {
    const g = state.geometry;
    for (const [a, b] of g.alignment) polyline(ctx, [planXY(a), planXY(b)], ALIGNMENT, 1);
    g.wires.forEach((w, i) => {
      w.spans.forEach((span, j) => {
        const sel = isSelected("span", i, j);
        polyline(ctx, span.map(planXY), sel ? SELECTED : w.color, sel ? 2.5 : 1);
      });
    });
    for (const s of g.structures) {
      const p = planXY(s.base);
      ctx.fillStyle = isSelected("structure", s.id) ? SELECTED : STRUCTURE;
      ctx.fillRect(p[0] - 4, p[1] - 4, 8, 8);
      ctx.fillStyle = "#c8ced4";
      ctx.fillText(s.id, p[0] + 6, p[1] - 6);
    }
    for (const peer of others()) marker(ctx, planXY(local(peer.presence.camera)), peer.user);
  }

  function renderProfile(ctx) {
    const g = state.geometry;
    // The ground under the structures (no terrain in the web client yet).
    polyline(ctx, g.path.map((p, i) => profileXY(g.stations[i], p[2])), ALIGNMENT, 1);
    g.wires.forEach((w, i) => {
      w.spans.forEach((span, j) => {
        const sel = isSelected("span", i, j);
        polyline(ctx, span.map((p) => profileXY(station(p), p[2])), sel ? SELECTED : w.color, sel ? 2.5 : 1);
      });
    });
    for (const s of g.structures) {
      const st = station(s.base);
      const a = profileXY(st, s.base[2]), b = profileXY(st, s.head[2]);
      polyline(ctx, [a, b], isSelected("structure", s.id) ? SELECTED : STRUCTURE, 2);
      ctx.fillStyle = "#c8ced4";
      ctx.fillText(s.id, b[0] + 4, b[1] - 4);
    }
  }

  // ---- picking ------------------------------------------------------

  // Screen position of a local point in the current view (null: hidden).
  function project() {
    if (state.view === "plan") return planXY;
    if (state.view === "profile") return (p) => profileXY(station(p), p[2]);
    const m = matrix();
    return (p) => {
      const c = clip(m, p);
      return c[3] > 0 ? screenOf(c) : null;
    };
  }

  function pick(x, y) {
    if (!state.geometry) return;
    const to = project();
    let best = 12 * 12, found = null;
    const near = (p, what) => {
      const s = to(p);
      if (!s) return;
      const d = (s[0] - x) ** 2 + (s[1] - y) ** 2;
      if (d < best) { best = d; found = what; }
    };
    for (const s of state.geometry.structures) {
      near(s.base, { kind: "structure", id: s.id });
      near(s.head, { kind: "structure", id: s.id });
    }
    state.geometry.wires.forEach((w, i) => {
      w.spans.forEach((span, j) => {
        for (const p of span) near(p, { kind: "span", wire: i, span: j });
      });
    });
    state.selection = found;
    uploadGpu();
    showSelection();
    draw();
  }

  // ---- side panel ---------------------------------------------------

  function selectionText() {
    const s = state.selection;
    if (!s || !state.geometry) return "";
    if (s.kind === "structure") return `structure ${s.id}`;
    const w = state.geometry.wires[s.wire];
    return w ? `section ${w.wire.section} span ${s.span + 1}` : "";
  }

  function properties(rows) {
    const dl = el("dl");
    for (const [k, v] of rows) {
      dl.appendChild(el("dt", k));
      dl.appendChild(el("dd", v));
    }
    return dl;
  }

  function showSelection() {
    const box = $("props");
    box.replaceChildren();
    box.className = "";
    const s = state.selection;
    const g = state.geometry;
    if (!s || !g) {
      box.className = "weak";
      box.textContent = "Click a structure or a wire.";
      updateMoveForm();
      return;
    }
    if (s.kind === "structure") {
      const st = g.byId[s.id];
      if (!st) {
        state.selection = null;
        return showSelection();
      }
      const p = st.structure.position;
      box.appendChild(properties([
        ["structure", st.id],
        ["type", st.structure.type || "—"],
        ["extension", st.structure.extension || "—"],
        ["position", `${fmt(p[0])}, ${fmt(p[1])}, ${fmt(p[2])}`],
        ["orientation", `${fmt(st.structure.orientation_deg || 0, 1)}°`],
        ["attachments", String((st.structure.attachments || []).length)],
        ["top", `${fmt(st.head[2] + state.origin[2])} m`],
      ]));
    } else {
      const w = g.wires[s.wire];
      const span = w && w.wire.spans[s.span];
      if (!span) {
        state.selection = null;
        return showSelection();
      }
      const sec = w.section || {};
      const pts = sec.points || [];
      const from = pts[s.span] ? pts[s.span].structure : "?";
      const to = pts[s.span + 1] ? pts[s.span + 1].structure : "?";
      box.appendChild(properties([
        ["section", w.wire.section],
        ["span", `${s.span + 1}: ${from} – ${to}`],
        ["cable", sec.cable || "—"],
        ["circuit", sec.circuit || "—"],
        ["phase", sec.phase_label || "—"],
        ["sag", `${fmt(span.sag)} m`],
        ["max. tension", `${fmt(span.max_tension / 1000, 1)} kN`],
        ["weather", state.scene.case],
      ]));
    }
    updateMoveForm();
  }

  function canEdit() {
    return state.ws && (state.role === "designer" || state.role === "admin");
  }

  function updateMoveForm() {
    const form = $("move");
    const s = state.selection;
    const st = s && s.kind === "structure" && state.geometry && state.geometry.byId[s.id];
    form.hidden = !(canEdit() && st);
    if (st && document.activeElement && form.contains(document.activeElement)) return;
    if (st) {
      const p = st.structure.position;
      $("mx").value = fmt(p[0]);
      $("my").value = fmt(p[1]);
      $("mz").value = fmt(p[2]);
    }
  }

  function showPeers() {
    const ul = $("people");
    ul.replaceChildren();
    for (const p of state.peers) {
      const editing = Object.keys(state.locks).filter((t) => state.locks[t] === p.user);
      let text = `${p.user}${p.user === state.user ? " (you)" : ""} — ${p.role}`;
      if (p.presence && p.presence.selection) text += `, ${p.presence.selection}`;
      if (editing.length) text += `, editing ${editing.join(", ")}`;
      const li = el("li", text);
      if (p.user !== state.user && p.presence && p.presence.camera) {
        const go = el("button", "Look");
        go.title = "Look where they look";
        go.onclick = () => lookAt(local(p.presence.camera));
        li.appendChild(go);
      }
      ul.appendChild(li);
    }
  }

  function showNotes() {
    const ul = $("notes");
    ul.replaceChildren();
    const scene = state.scene;
    ul.appendChild(el("li", `Wires solved on the server, ${scene.case}.`));
    for (const n of scene.notes) ul.appendChild(el("li", n));
    for (const w of scene.wires) {
      if (w.error) ul.appendChild(el("li", `section ${w.section}: ${w.error}`, "error"));
    }
  }

  function lookAt(p) {
    state.cam.target = [p[0], p[1], state.cam.target[2]];
    if (state.plan) state.plan.c = [p[0], p[1], 0];
    draw();
  }

  // ---- presence -----------------------------------------------------

  // Where this user looks, in line coordinates: the 3D eye, or the plan's
  // centre.
  function lookingAt() {
    let p;
    if (state.view === "plan" && state.plan) p = [state.plan.c[0], state.plan.c[1], state.cam.target[2]];
    else p = eye();
    return [p[0] + state.origin[0], p[1] + state.origin[1], p[2] + state.origin[2]];
  }

  function schedulePresence() {
    if (state.presenceTimer || !state.ws || !state.role) return;
    state.presenceTimer = setTimeout(() => {
      state.presenceTimer = 0;
      if (!state.geometry) return;
      const camera = lookingAt().map((v) => Math.round(v * 10) / 10);
      const msg = { type: "presence", selection: selectionText(), camera };
      const text = JSON.stringify(msg);
      if (text !== state.presenceSent) {
        state.presenceSent = text;
        send(msg);
      }
    }, 500);
  }

  // ---- input --------------------------------------------------------

  function setView(view) {
    state.view = view;
    for (const b of document.querySelectorAll(".tabs button")) {
      b.classList.toggle("active", b.dataset.view === view);
    }
    $("hint").textContent = view === "3d"
      ? "drag: orbit · shift+drag: pan · wheel: zoom · click: select"
      : "drag: pan · wheel: zoom · click: select";
    draw();
  }

  let drag = null;
  overlay.addEventListener("pointerdown", (e) => {
    try {
      overlay.setPointerCapture(e.pointerId);
    } catch (err) {
      // No capture (a synthetic pointer): dragging still works inside.
    }
    drag = { x: e.clientX, y: e.clientY, moved: false, pan: e.shiftKey || e.button === 2 };
  });
  overlay.addEventListener("pointermove", (e) => {
    if (!drag || !state.geometry) return;
    const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
    if (Math.abs(dx) + Math.abs(dy) > 2) drag.moved = true;
    if (!drag.moved) return;
    drag.x = e.clientX;
    drag.y = e.clientY;
    if (state.view === "plan") {
      state.plan.c[0] -= dx / state.plan.s;
      state.plan.c[1] += dy / state.plan.s;
    } else if (state.view === "profile") {
      state.profile.c[0] -= dx / state.profile.sx;
      state.profile.c[1] += dy / state.profile.sz;
    } else if (drag.pan) {
      const c = state.cam;
      const k = c.dist / (overlay.clientHeight || 1);
      const right = [-Math.sin(c.yaw), Math.cos(c.yaw), 0];
      const fwd = [-Math.cos(c.yaw), -Math.sin(c.yaw), 0];
      for (let i = 0; i < 3; i++) c.target[i] += (-dx * right[i] + dy * fwd[i]) * k;
    } else {
      state.cam.yaw -= dx * 0.008;
      state.cam.pitch = Math.max(-1.5, Math.min(1.5, state.cam.pitch + dy * 0.008));
    }
    draw();
  });
  overlay.addEventListener("pointerup", (e) => {
    if (drag && !drag.moved) {
      const r = overlay.getBoundingClientRect();
      pick(e.clientX - r.left, e.clientY - r.top);
    }
    drag = null;
  });
  overlay.addEventListener("contextmenu", (e) => e.preventDefault());
  overlay.addEventListener("wheel", (e) => {
    e.preventDefault();
    if (!state.geometry) return;
    const k = Math.exp(-e.deltaY * 0.0015);
    if (state.view === "plan") state.plan.s *= k;
    else if (state.view === "profile") { state.profile.sx *= k; state.profile.sz *= k; }
    else state.cam.dist = Math.max(1, state.cam.dist / k);
    draw();
  }, { passive: false });

  for (const b of document.querySelectorAll(".tabs button")) {
    b.addEventListener("click", () => setView(b.dataset.view));
  }

  $("join-form").addEventListener("submit", (e) => {
    e.preventDefault();
    const user = $("user").value.trim();
    const code = $("code").value.trim();
    if (user && code) connect(user, code);
  });

  $("move").addEventListener("submit", (e) => {
    e.preventDefault();
    const s = state.selection;
    if (!s || s.kind !== "structure" || !canEdit()) return;
    const position = [$("mx"), $("my"), $("mz")].map((i) => Number(i.value));
    if (position.some((v) => !Number.isFinite(v))) {
      setStatus("a position of three numbers", true);
      return;
    }
    request({ op: "do", command: { op: "move_structure", id: s.id, position } }, `move of ${s.id}`);
  });

  if (window.ResizeObserver) new ResizeObserver(resize).observe($("stage"));
  else window.addEventListener("resize", resize);

  // ---- start --------------------------------------------------------

  async function start() {
    setView("3d");
    try {
      state.gpu = await OlsGpu.create(gpuCanvas);
    } catch (e) {
      state.gpu = null;
    }
    $("renderer").textContent = state.gpu ? "3D: WebGPU" : "3D: Canvas2D (no WebGPU in this browser)";
    if (!state.gpu) gpuCanvas.hidden = true;
    const f = fragment();
    $("user").value = f.user || remembered();
    $("code").value = f.code || "";
    if ($("user").value && $("code").value) connect($("user").value, $("code").value);
  }

  start();
})();
