// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// The OpenOHL web viewer: joins a server session over a WebSocket (the
// server's JSON messages) and shows the project's main line it receives
// (structures, alignments, wires solved on the server) in 3D (WebGPU, else
// Canvas2D), plan and profile; who else is connected and the edits as they
// arrive. It does not edit: G goes to a structure, the mouse and the keys
// move around, a click shows an object's properties. What is drawn is
// thinned with the distance (detail), so long lines stay fluid.
"use strict";

(() => {
  const PROTOCOL = 2;
  const PALETTE = ["#d0703a", "#4a9be0", "#58b368", "#c9b03c", "#b06ad0", "#40b5b0"];
  const BACKGROUND = { r: 0.106, g: 0.118, b: 0.133, a: 1 };
  const STRUCTURE = "#a7b0b9";
  const ALIGNMENT = "#5f8a63";
  const SELECTED = "#ffd24a";
  const LABEL = "#c8ced4";
  const MAX_LOG = 200;

  // Level of detail, by setting: a span shorter than `chord` px on the
  // screen is drawn as its chord, shorter than `mid` px with a few points,
  // else whole; shorter than `skip` px not at all. A structure lower than
  // `stick` px is one stroke. At most `names` names and `sags` sags are
  // written (nearest the middle of the view first, never overlapping).
  const DETAIL = {
    low: { skip: 3, chord: 60, mid: 260, stick: 18, names: 40, sags: 20 },
    medium: { skip: 1.5, chord: 30, mid: 140, stick: 10, names: 120, sags: 60 },
    high: { skip: 0, chord: 0, mid: 0, stick: 0, names: 400, sags: 200 },
  };
  const SHOW_DEFAULT = {
    structures: true, wires: true, alignment: true, names: true, sags: false, people: true,
  };

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
    // 3D orbit camera (local coordinates).
    cam: { target: [0, 0, 0], yaw: -2.3, pitch: 0.45, dist: 500 },
    // Plan and profile: centre (local) and pixels per metre.
    plan: null,
    profile: null,
    geometry: null,
    presenceSent: "",
    presenceTimer: 0,
    show: Object.assign({}, SHOW_DEFAULT),
    detail: "medium",
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
    const what = ((op.op === "do" && op.command && op.command.op) || op.op || "edit").replace(/_/g, " ");
    const name = op.name || op.from || op.line || op.key || op.message || "";
    return name ? `${what} ${name}` : what;
  }

  function stored(key, fallback) {
    try {
      const v = localStorage.getItem(key);
      return v === null ? fallback : v;
    } catch (e) {
      return fallback;
    }
  }

  function store(key, value) {
    try {
      localStorage.setItem(key, value);
    } catch (e) {
      // Storage refused: nothing to keep.
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
      try {
        receive(msg);
      } catch (err) {
        // A message the viewer cannot show must not stop it.
        setStatus(`could not show a message: ${err.message}`, true);
      }
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
    };
  }

  function send(msg) {
    if (state.ws && state.ws.readyState === WebSocket.OPEN) {
      state.ws.send(JSON.stringify(msg));
    }
  }

  // ---- messages -----------------------------------------------------

  function receive(msg) {
    switch (msg.type) {
      case "welcome":
        state.role = msg.role;
        state.peers = msg.peers || [];
        state.locks = msg.locks || {};
        store("ols.user", state.user);
        $("project").textContent = msg.project;
        setStatus(`joined as ${state.user}; loading the line…`);
        $("join").hidden = true;
        $("views").hidden = false;
        $("side").hidden = false;
        showPeers();
        resize();
        overlay.focus({ preventScroll: true });
        break;
      case "refused":
        $("join-error").textContent = `Refused: ${msg.reason}`;
        setStatus("not connected");
        break;
      case "scene": {
        const first = !state.scene;
        state.scene = msg;
        prepare(first);
        setStatus(`joined as ${state.user} — main line at log entry ${msg.seq}`);
        showNotes();
        showSelection();
        fillGoto();
        draw();
        break;
      }
      case "applied": {
        const where = msg.line && msg.line !== "main" ? ` (line ${msg.line})` : "";
        const seq = msg.seq === null || msg.seq === undefined ? "–" : "#" + msg.seq;
        const li = el("li", `${seq} ${msg.user}: ${describe(msg.op)}${where}`);
        const log = $("log");
        log.insertBefore(li, log.firstChild);
        while (log.children.length > MAX_LOG) log.removeChild(log.lastChild);
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

  // Every n-th point of a polyline, its ends kept: about `count` points.
  function thin(points, count) {
    if (points.length <= count) return points;
    const out = [];
    const step = (points.length - 1) / (count - 1);
    for (let i = 0; i < count; i++) out.push(points[Math.round(i * step)]);
    return out;
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
    // The structures in the line's order.
    const path = line.structures.map((s) => byId[s.id]).filter(Boolean).map((s) => s.base);
    // Alignments: each through its alignment points (by id), else the
    // structures in order.
    const at = {};
    for (const p of line.alignment_points || []) at[p.id] = p.position;
    const drawn = (line.alignments || [])
      .map((a) => a.points.map((id) => at[id]).filter(Boolean).map(local))
      .filter((pts) => pts.length >= 2);
    const alignments = drawn.length ? drawn : [path];
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
        // Each span whole, with a few points, and as its chord.
        spans: w.spans.map((s) => {
          const full = s.points.map(local);
          return {
            full,
            mid: thin(full, 8),
            chord: full.length ? [full[0], full[full.length - 1]] : [],
            centre: full.length ? full[Math.floor(full.length / 2)] : [0, 0, 0],
          };
        }),
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
    for (const w of state.geometry.wires) for (const span of w.spans) for (const p of span.chord) add(p);
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

  // Screen position of a local point in the current view (null: behind
  // the eye).
  function project() {
    if (state.view === "plan") return planXY;
    if (state.view === "profile") return (p) => profileXY(station(p), p[2]);
    const m = matrix();
    return (p) => {
      const c = clip(m, p);
      return c[3] > 1e-3 ? screenOf(c) : null;
    };
  }

  // ---- what is drawn ------------------------------------------------

  function isSelected(kind, a, b) {
    const s = state.selection;
    if (!s || s.kind !== kind) return false;
    return kind === "structure" ? s.id === a : s.wire === a && s.span === b;
  }

  // Every segment shown, whole (the WebGPU path draws them all; the GPU
  // does not mind a few hundred thousand).
  function segments() {
    const g = state.geometry;
    const out = [];
    if (state.show.alignment) for (const [a, b] of g.alignment) out.push([a, b, ALIGNMENT]);
    if (state.show.structures) {
      for (const s of g.structures) {
        const c = isSelected("structure", s.id) ? SELECTED : STRUCTURE;
        for (const [a, b] of s.parts) out.push([a, b, c]);
      }
    }
    if (state.show.wires) {
      g.wires.forEach((w, i) => {
        w.spans.forEach((span, j) => {
          const c = isSelected("span", i, j) ? SELECTED : w.color;
          const pts = state.detail === "low" ? span.mid : span.full;
          for (let k = 0; k + 1 < pts.length; k++) out.push([pts[k], pts[k + 1], c]);
        });
      });
    }
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
    try {
      state.gpu.setLines(data);
    } catch (e) {
      // Too much for this GPU: drawn with Canvas2D and its detail instead.
      state.gpu = null;
      gpuCanvas.hidden = true;
      $("renderer").textContent = "3D: Canvas2D (the GPU refused the line)";
    }
  }

  // Strokes batched by colour: one path per colour (Canvas2D).
  function batch() {
    const paths = new Map();
    let count = 0;
    return {
      add(points, colour) {
        if (points.length < 2) return;
        let p = paths.get(colour);
        if (!p) { p = []; paths.set(colour, p); }
        p.push(points);
        count += points.length - 1;
      },
      stroke(ctx, width) {
        for (const [colour, lines] of paths) {
          ctx.strokeStyle = colour;
          ctx.lineWidth = colour === SELECTED ? width + 1.5 : width;
          ctx.beginPath();
          for (const pts of lines) {
            ctx.moveTo(pts[0][0], pts[0][1]);
            for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i][0], pts[i][1]);
          }
          ctx.stroke();
        }
        return count;
      },
    };
  }

  // A polyline's points on the screen (null: wholly hidden or off it).
  function onScreen(to, points, margin) {
    const w = overlay.clientWidth, h = overlay.clientHeight;
    const out = [];
    let lo = [Infinity, Infinity], hi = [-Infinity, -Infinity];
    for (const p of points) {
      const s = to(p);
      if (!s) return null;
      out.push(s);
      lo = [Math.min(lo[0], s[0]), Math.min(lo[1], s[1])];
      hi = [Math.max(hi[0], s[0]), Math.max(hi[1], s[1])];
    }
    if (hi[0] < -margin || hi[1] < -margin || lo[0] > w + margin || lo[1] > h + margin) return null;
    return out;
  }

  // Wires, structures and alignments with their level of detail (every
  // view but the 3D one with WebGPU). Returns the screen positions of the
  // spans drawn (for the sags) and the segments drawn.
  function drawLines(ctx, to) {
    const g = state.geometry;
    const d = DETAIL[state.detail];
    const lines = batch();
    const spansDrawn = [];
    // In the profile the ground under the main route stands for them.
    if (state.show.alignment && state.view !== "profile") {
      for (const [a, b] of g.alignment) {
        const s = onScreen(to, [a, b], 0);
        if (s) lines.add(s, ALIGNMENT);
      }
    }
    if (state.show.wires) {
      g.wires.forEach((w, i) => {
        w.spans.forEach((span, j) => {
          const ends = onScreen(to, span.chord, 50);
          if (!ends) return;
          const len = Math.hypot(ends[1][0] - ends[0][0], ends[1][1] - ends[0][1]);
          if (len < d.skip && !isSelected("span", i, j)) return;
          const pts = len < d.chord ? ends : onScreen(to, len < d.mid ? span.mid : span.full, 1e9);
          if (!pts) return;
          lines.add(pts, isSelected("span", i, j) ? SELECTED : w.color);
          spansDrawn.push({ w, i, j, len });
        });
      });
    }
    if (state.show.structures) {
      for (const s of g.structures) {
        const c = isSelected("structure", s.id) ? SELECTED : STRUCTURE;
        const stick = onScreen(to, [s.base, s.head], 20);
        if (!stick) continue;
        const height = Math.hypot(stick[1][0] - stick[0][0], stick[1][1] - stick[0][1]);
        if (state.view !== "3d" || height < d.stick) {
          lines.add(stick, c);
          continue;
        }
        for (const [a, b] of s.parts) {
          const seg = onScreen(to, [a, b], 1e9);
          if (seg) lines.add(seg, c);
        }
      }
    }
    const count = lines.stroke(ctx, 1);
    return { spansDrawn, count };
  }

  // Text where there is room: nearest the middle of the view first, at
  // most `budget`, none over another.
  function labels(ctx, items, budget) {
    const w = overlay.clientWidth, h = overlay.clientHeight;
    items.sort((a, b) => Math.hypot(a.p[0] - w / 2, a.p[1] - h / 2) - Math.hypot(b.p[0] - w / 2, b.p[1] - h / 2));
    const taken = [];
    let n = 0;
    for (const it of items) {
      if (n >= budget) break;
      const tw = ctx.measureText(it.text).width;
      const box = [it.p[0] + 4, it.p[1] - 16, it.p[0] + 6 + tw, it.p[1]];
      if (box[2] < 0 || box[0] > w || box[3] < 0 || box[1] > h) continue;
      if (taken.some((t) => box[0] < t[2] && box[2] > t[0] && box[1] < t[3] && box[3] > t[1])) continue;
      taken.push(box);
      ctx.fillStyle = it.color;
      ctx.fillText(it.text, box[0], it.p[1] - 4);
      n++;
    }
    return n;
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
      try {
        render();
      } catch (err) {
        setStatus(`drawing failed: ${err.message}`, true);
      }
    });
  }

  function render() {
    const started = performance.now();
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
    const to = project();
    let drawn = { spansDrawn: [], count: 0 };
    if (gpu3d) {
      state.gpu.draw(new Float32Array(matrix()), BACKGROUND);
      // The spans in view, for their sags.
      if (state.show.wires && state.show.sags) {
        state.geometry.wires.forEach((wire, i) => {
          wire.spans.forEach((span, j) => {
            const ends = onScreen(to, span.chord, 0);
            if (ends) drawn.spansDrawn.push({ w: wire, i, j, len: Math.hypot(ends[1][0] - ends[0][0], ends[1][1] - ends[0][1]) });
          });
        });
      }
    } else {
      if (state.view === "profile" && state.show.alignment) {
        const g = state.geometry;
        // The ground under the structures (no terrain in the web viewer).
        const ground = g.path.map((p, i) => profileXY(g.stations[i], p[2]));
        ctx.strokeStyle = ALIGNMENT;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ground.forEach((p, i) => (i ? ctx.lineTo(p[0], p[1]) : ctx.moveTo(p[0], p[1])));
        ctx.stroke();
      }
      drawn = drawLines(ctx, to);
    }
    const d = DETAIL[state.detail];
    let names = 0;
    if (state.show.names && state.show.structures) {
      const items = [];
      for (const s of state.geometry.structures) {
        const p = to(state.view === "plan" ? s.base : s.head);
        if (p) items.push({ p, text: s.id, color: isSelected("structure", s.id) ? SELECTED : LABEL });
      }
      names = labels(ctx, items, d.names);
    }
    if (state.show.sags) {
      const items = [];
      for (const { w: wire, j, len } of drawn.spansDrawn) {
        if (len < 80) continue;
        const span = wire.wire.spans[j];
        const p = to(wire.spans[j].centre);
        if (p && span) items.push({ p: [p[0] - 20, p[1] + 16], text: `${fmt(span.sag)} m`, color: wire.color });
      }
      labels(ctx, items, d.sags);
    }
    if (state.show.people) {
      for (const peer of others()) {
        const p = to(local(peer.presence.camera));
        if (p) marker(ctx, p, peer.user);
      }
    }
    const ms = Math.round(performance.now() - started);
    $("drawn").textContent = gpu3d
      ? `WebGPU: every segment; ${names} names; ${ms} ms`
      : `${drawn.count} segments drawn, ${names} names, in ${ms} ms`;
    schedulePresence();
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

  // ---- picking ------------------------------------------------------

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
    if (state.show.structures) {
      for (const s of state.geometry.structures) {
        near(s.base, { kind: "structure", id: s.id });
        near(s.head, { kind: "structure", id: s.id });
      }
    }
    if (state.show.wires) {
      state.geometry.wires.forEach((w, i) => {
        w.spans.forEach((span, j) => {
          for (const p of span.mid) near(p, { kind: "span", wire: i, span: j });
        });
      });
    }
    select(found);
  }

  function select(what) {
    state.selection = what;
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

  function xyz(p) {
    return `${fmt(p[0])}, ${fmt(p[1])}, ${fmt(p[2])}`;
  }

  function showSelection() {
    const box = $("props");
    box.replaceChildren();
    box.className = "";
    const s = state.selection;
    const g = state.geometry;
    if (!s || !g) {
      box.className = "weak";
      box.textContent = "Click a structure or a wire (G: go to a structure).";
      return;
    }
    const line = state.scene.line;
    if (s.kind === "structure") {
      const st = g.byId[s.id];
      if (!st) {
        state.selection = null;
        return showSelection();
      }
      const str = st.structure;
      const sections = line.sections
        .filter((sec) => (sec.points || []).some((p) => p.structure === s.id))
        .map((sec) => sec.id);
      const rows = [
        ["structure", st.id],
        ["type", str.type || "—"],
        ["extension", str.extension !== undefined && str.extension !== null ? String(str.extension) : "—"],
        ["position (E, N, Z)", xyz(str.position)],
        ["orientation", `${fmt(str.orientation_deg || 0, 1)}°`],
        ["height", `${fmt(st.head[2] - st.base[2])} m`],
        ["sections", sections.length ? sections.join(", ") : "—"],
      ];
      box.appendChild(properties(rows));
      const atts = str.attachments || [];
      if (atts.length) {
        box.appendChild(el("h3", `${atts.length} attachments`));
        const ul = el("ul", undefined, "small");
        for (const a of atts) {
          const name = a.label || `set ${a.set} phase ${a.phase}`;
          ul.appendChild(el("li", `${name}: wire point ${fmt(a.wire_point[2])} m, string ${fmt(a.insulator_point[2] - a.wire_point[2])} m`));
        }
        box.appendChild(ul);
      }
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
      const full = w.spans[s.span].full;
      const a = full[0], b = full[full.length - 1];
      const horizontal = a && b ? Math.hypot(b[0] - a[0], b[1] - a[1]) : 0;
      const chord = a && b ? Math.hypot(b[0] - a[0], b[1] - a[1], b[2] - a[2]) : 0;
      box.appendChild(properties([
        ["section", w.wire.section],
        ["span", `${s.span + 1} of ${w.wire.spans.length}: ${from} – ${to}`],
        ["length", `${fmt(horizontal)} m (chord ${fmt(chord)} m)`],
        ["height difference", a && b ? `${fmt(b[2] - a[2])} m` : "—"],
        ["sag", `${fmt(span.sag)} m`],
        ["max. tension", `${fmt(span.max_tension / 1000, 1)} kN`],
        ["cable", sec.cable || "—"],
        ["bundle", sec.bundle && sec.bundle > 1 ? `${sec.bundle} subconductors` : "single"],
        ["circuit", sec.circuit || "—"],
        ["phase", sec.phase_label || "—"],
        ["weather", state.scene.case],
      ]));
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
    ul.appendChild(el("li", `The main line; wires solved on the server, ${scene.case}.`));
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

  // ---- go to a structure (G) ----------------------------------------

  function fillGoto() {
    const list = $("goto-list");
    list.replaceChildren();
    for (const s of state.geometry.structures) {
      const o = el("option");
      o.value = s.id;
      list.appendChild(o);
    }
  }

  function openGoto() {
    if (!state.geometry) return;
    $("goto").hidden = false;
    $("goto-error").textContent = "";
    const input = $("goto-id");
    input.value = "";
    input.focus({ preventScroll: true });
  }

  function closeGoto() {
    $("goto").hidden = true;
    overlay.focus({ preventScroll: true });
  }

  // The structure named `text`: its id, else the same ignoring case, else
  // the only one starting so.
  function findStructure(text) {
    const g = state.geometry;
    const t = text.trim();
    if (!t) return null;
    if (g.byId[t]) return g.byId[t];
    const lower = t.toLowerCase();
    const same = g.structures.find((s) => s.id.toLowerCase() === lower);
    if (same) return same;
    const starts = g.structures.filter((s) => s.id.toLowerCase().startsWith(lower));
    return starts.length === 1 ? starts[0] : null;
  }

  function goTo(s) {
    const mid = [s.base[0], s.base[1], (s.base[2] + s.head[2]) / 2];
    state.cam.target = mid;
    state.cam.dist = Math.min(state.cam.dist, 120);
    if (state.plan) {
      state.plan.c = [s.base[0], s.base[1], 0];
      state.plan.s = Math.max(state.plan.s, 2);
    }
    if (state.profile) {
      state.profile.c = [station(s.base), mid[2]];
      state.profile.sx = Math.max(state.profile.sx, 1);
    }
    select({ kind: "structure", id: s.id });
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

  // ---- moving around ------------------------------------------------

  function setView(view) {
    state.view = view;
    for (const b of document.querySelectorAll(".tabs button")) {
      b.classList.toggle("active", b.dataset.view === view);
    }
    const hint = "G: go to a structure · click: properties · "
      + (view === "3d" ? "drag: orbit · shift/right drag: pan" : "drag: pan")
      + " · wheel or +/−: zoom · WASD/arrows: move · F: fit";
    $("hint").textContent = hint;
    $("hint").title = hint;
    draw();
  }

  // Moves the view by (dx, dy) pixels on the screen.
  function pan(dx, dy) {
    if (state.view === "plan") {
      state.plan.c[0] -= dx / state.plan.s;
      state.plan.c[1] += dy / state.plan.s;
    } else if (state.view === "profile") {
      state.profile.c[0] -= dx / state.profile.sx;
      state.profile.c[1] += dy / state.profile.sz;
    } else {
      const c = state.cam;
      const k = c.dist / (overlay.clientHeight || 1);
      const right = [-Math.sin(c.yaw), Math.cos(c.yaw), 0];
      const fwd = [-Math.cos(c.yaw), -Math.sin(c.yaw), 0];
      for (let i = 0; i < 3; i++) c.target[i] += (-dx * right[i] + dy * fwd[i]) * k;
    }
    draw();
  }

  function zoom(k) {
    if (state.view === "plan") state.plan.s *= k;
    else if (state.view === "profile") { state.profile.sx *= k; state.profile.sz *= k; }
    else state.cam.dist = Math.max(1, state.cam.dist / k);
    draw();
  }

  let drag = null;
  overlay.addEventListener("pointerdown", (e) => {
    overlay.focus({ preventScroll: true });
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
    if (state.view !== "3d" || drag.pan) {
      pan(dx, dy);
    } else {
      state.cam.yaw -= dx * 0.008;
      state.cam.pitch = Math.max(-1.5, Math.min(1.5, state.cam.pitch + dy * 0.008));
      draw();
    }
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
    zoom(Math.exp(-e.deltaY * 0.0015));
  }, { passive: false });

  window.addEventListener("keydown", (e) => {
    if (!state.geometry || e.metaKey || e.ctrlKey || e.altKey) return;
    const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement;
    if (typing) {
      if (e.key === "Escape" && !$("goto").hidden) closeGoto();
      return;
    }
    const step = 60;
    switch (e.key) {
      case "g": case "G": openGoto(); break;
      case "w": case "W": case "ArrowUp": pan(0, step); break;
      case "s": case "S": case "ArrowDown": pan(0, -step); break;
      case "a": case "A": case "ArrowLeft": pan(step, 0); break;
      case "d": case "D": case "ArrowRight": pan(-step, 0); break;
      case "+": case "=": zoom(1.25); break;
      case "-": case "_": zoom(1 / 1.25); break;
      case "f": case "F": fit(); draw(); break;
      case "Escape": select(null); break;
      default: return;
    }
    e.preventDefault();
  });

  $("goto").addEventListener("submit", (e) => {
    e.preventDefault();
    const s = findStructure($("goto-id").value);
    if (!s) {
      $("goto-error").textContent = `no structure "${$("goto-id").value}"`;
      return;
    }
    closeGoto();
    goTo(s);
  });

  for (const b of document.querySelectorAll(".tabs button")) {
    b.addEventListener("click", () => { setView(b.dataset.view); overlay.focus({ preventScroll: true }); });
  }

  // ---- display options ----------------------------------------------

  function loadDisplay() {
    try {
      const saved = JSON.parse(stored("ols.display", "{}"));
      for (const k of Object.keys(SHOW_DEFAULT)) {
        if (typeof saved[k] === "boolean") state.show[k] = saved[k];
      }
    } catch (e) {
      // Nothing remembered.
    }
    const detail = stored("ols.detail", "medium");
    if (DETAIL[detail]) state.detail = detail;
    for (const box of document.querySelectorAll("[data-show]")) {
      box.checked = state.show[box.dataset.show];
      box.addEventListener("change", () => {
        state.show[box.dataset.show] = box.checked;
        store("ols.display", JSON.stringify(state.show));
        uploadGpu();
        draw();
      });
    }
    $("detail").value = state.detail;
    $("detail").addEventListener("change", () => {
      state.detail = $("detail").value;
      store("ols.detail", state.detail);
      uploadGpu();
      draw();
    });
  }

  $("join-form").addEventListener("submit", (e) => {
    e.preventDefault();
    const user = $("user").value.trim();
    const code = $("code").value.trim();
    if (user && code) connect(user, code);
  });

  if (window.ResizeObserver) new ResizeObserver(resize).observe($("stage"));
  else window.addEventListener("resize", resize);

  // ---- start --------------------------------------------------------

  async function start() {
    loadDisplay();
    setView("3d");
    try {
      state.gpu = await OhlGpu.create(gpuCanvas);
    } catch (e) {
      state.gpu = null;
    }
    $("renderer").textContent = state.gpu ? "3D: WebGPU" : "3D: Canvas2D";
    if (!state.gpu) gpuCanvas.hidden = true;
    const f = fragment();
    $("user").value = f.user || stored("ols.user", "");
    $("code").value = f.code || "";
    if ($("user").value && $("code").value) connect($("user").value, $("code").value);
  }

  start();
})();
