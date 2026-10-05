// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// WebGPU line renderer of the 3D view: coloured line segments with a
// depth buffer. `OlsGpu.create(canvas)` resolves to null when the browser
// has no WebGPU (the page then draws with Canvas2D).
"use strict";

const OlsGpu = (() => {
  const SHADER = `
struct U { m: mat4x4<f32> };
@group(0) @binding(0) var<uniform> u: U;
struct O { @builtin(position) p: vec4<f32>, @location(0) c: vec3<f32> };
@vertex fn vs(@location(0) p: vec3<f32>, @location(1) c: vec3<f32>) -> O {
  var o: O;
  o.p = u.m * vec4<f32>(p, 1.0);
  o.c = c;
  return o;
}
@fragment fn fs(i: O) -> @location(0) vec4<f32> { return vec4<f32>(i.c, 1.0); }
`;

  async function create(canvas) {
    if (!navigator.gpu) return null;
    let adapter;
    try {
      adapter = await navigator.gpu.requestAdapter();
    } catch (e) {
      return null;
    }
    if (!adapter) return null;
    const device = await adapter.requestDevice();
    const context = canvas.getContext("webgpu");
    if (!context) return null;
    const format = navigator.gpu.getPreferredCanvasFormat();
    context.configure({ device, format, alphaMode: "opaque" });
    const module = device.createShaderModule({ code: SHADER });
    const pipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: {
        module,
        entryPoint: "vs",
        buffers: [{
          arrayStride: 24,
          attributes: [
            { shaderLocation: 0, offset: 0, format: "float32x3" },
            { shaderLocation: 1, offset: 12, format: "float32x3" },
          ],
        }],
      },
      fragment: { module, entryPoint: "fs", targets: [{ format }] },
      primitive: { topology: "line-list" },
      depthStencil: { format: "depth24plus", depthWriteEnabled: true, depthCompare: "less" },
    });
    const uniform = device.createBuffer({
      size: 64,
      usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
    });
    const bindGroup = device.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [{ binding: 0, resource: { buffer: uniform } }],
    });
    let vertices = null;
    let count = 0;
    let depth = null;
    let lost = false;
    device.lost.then(() => { lost = true; });

    return {
      get lost() { return lost; },
      // x, y, z, r, g, b per vertex; two vertices per segment.
      setLines(data) {
        if (vertices) vertices.destroy();
        count = data.length / 6;
        vertices = null;
        if (count === 0) return;
        vertices = device.createBuffer({
          size: data.byteLength,
          usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST,
        });
        device.queue.writeBuffer(vertices, 0, data);
      },
      // `matrix`: column-major view-projection (Float32Array of 16).
      draw(matrix, background) {
        const w = canvas.width, h = canvas.height;
        if (w === 0 || h === 0) return;
        if (!depth || depth.width !== w || depth.height !== h) {
          if (depth) depth.destroy();
          depth = device.createTexture({
            size: [w, h],
            format: "depth24plus",
            usage: GPUTextureUsage.RENDER_ATTACHMENT,
          });
        }
        device.queue.writeBuffer(uniform, 0, matrix);
        const encoder = device.createCommandEncoder();
        const pass = encoder.beginRenderPass({
          colorAttachments: [{
            view: context.getCurrentTexture().createView(),
            clearValue: background,
            loadOp: "clear",
            storeOp: "store",
          }],
          depthStencilAttachment: {
            view: depth.createView(),
            depthClearValue: 1.0,
            depthLoadOp: "clear",
            depthStoreOp: "store",
          },
        });
        if (vertices && count > 0) {
          pass.setPipeline(pipeline);
          pass.setBindGroup(0, bindGroup);
          pass.setVertexBuffer(0, vertices);
          pass.draw(count);
        }
        pass.end();
        device.queue.submit([encoder.finish()]);
      },
    };
  }

  return { create };
})();
