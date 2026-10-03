# ADR-0009: GUI toolkit — egui with wgpu

- **Status:** Accepted (settles the toolkit left open in ADR-0002)
- **Date:** 2026-10-03

## Context
Phase 3 starts the desktop application. ADR-0002 deferred the toolkit to a comparison of Slint, egui and Qt (cxx-qt). The application is mostly views rendered by our own wgpu renderer (3D, plan, profile — ADR-0006) and data-heavy panels and tools that engineers will ask for one by one; it must never block on calculations (DESIGN_GOALS §1).

## Decision
- **egui** (immediate-mode UI, pure Rust) through **eframe** with its **wgpu** backend; **egui_dock** for dockable, rearrangeable tabs.
- The renderer (`ols-render`) is independent of the UI: it draws into an offscreen texture that the views show, and renders headless for images and tests.
- The application (`ols-app`, binary `ols-cad`) loads and solves models on background threads and shares one selection and weather case between views.
- UI tests run the whole window with `egui_kittest`, including the 3D view when a GPU is available.

## Consequences
- Fast iteration: new tools are plain Rust functions drawing widgets; no binding layer, one language, one build.
- The look is egui's own, not native widgets; accessibility goes through AccessKit.
- The same code can later run in a browser (wgpu → WebGPU) for the server mode.
- On Linux only the X11 backend is enabled for now (works on Wayland desktops through XWayland); native Wayland needs `libwayland` at build time and can be added.
