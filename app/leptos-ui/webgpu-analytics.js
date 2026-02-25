(function () {
  "use strict";

  async function run_probe_webgpu(workgroups) {
    try {
      if (typeof navigator === "undefined" || !navigator.gpu) {
        return JSON.stringify({
          supported: false,
          error: "navigator.gpu unavailable",
        });
      }

      const adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
      if (!adapter) {
        return JSON.stringify({
          supported: false,
          error: "no WebGPU adapter available",
        });
      }

      const device = await adapter.requestDevice();
      const elementCount = 1024;
      const wg = Math.max(1, Number(workgroups) || 256);

      const storageBuffer = device.createBuffer({
        size: elementCount * 4,
        usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC,
      });

      const readbackBuffer = device.createBuffer({
        size: elementCount * 4,
        usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
      });

      const shaderModule = device.createShaderModule({
        code: `
@group(0) @binding(0) var<storage, read_write> output_data: array<u32>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let idx = gid.x;
  if (idx < 1024u) {
    output_data[idx] = idx * 2u + 1u;
  }
}
`,
      });

      const pipeline = device.createComputePipeline({
        layout: "auto",
        compute: {
          module: shaderModule,
          entryPoint: "main",
        },
      });

      const bindGroup = device.createBindGroup({
        layout: pipeline.getBindGroupLayout(0),
        entries: [{ binding: 0, resource: { buffer: storageBuffer } }],
      });

      const started = performance.now();
      const encoder = device.createCommandEncoder();
      const pass = encoder.beginComputePass();
      pass.setPipeline(pipeline);
      pass.setBindGroup(0, bindGroup);
      pass.dispatchWorkgroups(Math.ceil(elementCount / 64));
      pass.end();
      encoder.copyBufferToBuffer(storageBuffer, 0, readbackBuffer, 0, elementCount * 4);
      device.queue.submit([encoder.finish()]);

      await readbackBuffer.mapAsync(GPUMapMode.READ);
      const view = new Uint32Array(readbackBuffer.getMappedRange());
      const sample = Array.from(view.slice(0, 8));
      readbackBuffer.unmap();
      const elapsedMs = performance.now() - started;

      let adapterName = "unknown";
      let adapterBackend = null;
      if (typeof adapter.requestAdapterInfo === "function") {
        try {
          const info = await adapter.requestAdapterInfo();
          adapterName = info.description || adapterName;
          adapterBackend = info.architecture || null;
        } catch (_) {
          // Safari may gate adapter info; ignore.
        }
      }

      return JSON.stringify({
        supported: true,
        adapter: adapterName,
        architecture: adapterBackend,
        elapsed_ms: elapsedMs,
        sample,
        workgroups: wg,
      });
    } catch (error) {
      return JSON.stringify({
        supported: false,
        error: String(error),
      });
    }
  }

  globalThis.webgpuAnalytics = globalThis.webgpuAnalytics || {};
  Object.assign(globalThis.webgpuAnalytics, {
    run_probe_webgpu,
  });
})();
