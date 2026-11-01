// @ts-check
// The WebGL2 fallback: the same program in GLSL ES 3.00, a vertex array object per mesh, plain uniforms per
// draw. Used when navigator.gpu is missing or gives no adapter.

import { GLSL_FS, GLSL_VS } from './shaders.js';
import { mat4 } from './math.js';
import { FLOATS_PER_VERTEX } from './meshes.js';

/** @typedef {import('./renderer.js').Renderer} Renderer */

/**
 * @param {WebGL2RenderingContext} gl @param {number} type @param {string} src
 */
function compile(gl, type, src) {
  const s = gl.createShader(type);
  if (!s) throw new Error('createShader failed');
  gl.shaderSource(s, src);
  gl.compileShader(s);
  if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(`shader: ${gl.getShaderInfoLog(s)}`);
  return s;
}

/**
 * @param {HTMLCanvasElement} canvas
 * @returns {Renderer | null}
 */
export function createWebGL2Renderer(canvas) {
  const gl = canvas.getContext('webgl2', { antialias: true, alpha: false });
  if (!gl) return null;
  const prog = gl.createProgram();
  if (!prog) return null;
  gl.attachShader(prog, compile(gl, gl.VERTEX_SHADER, GLSL_VS));
  gl.attachShader(prog, compile(gl, gl.FRAGMENT_SHADER, GLSL_FS));
  gl.linkProgram(prog);
  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) throw new Error(`link: ${gl.getProgramInfoLog(prog)}`);
  /** @param {string} name */
  const loc = (name) => gl.getUniformLocation(prog, name);
  const u = {
    viewProj: loc('uViewProj'), model: loc('uModel'), eye: loc('uEye'), lightDir: loc('uLightDir'),
    lightColor: loc('uLightColor'), ambient: loc('uAmbient'), fog: loc('uFog'), tint: loc('uTint'),
    params: loc('uParams'), tex: loc('uTex'),
  };

  /** @type {Map<number, { vao: WebGLVertexArrayObject, buffers: WebGLBuffer[], count: number }>} */
  const meshes = new Map();
  /** @type {Map<number, WebGLTexture>} */
  const textures = new Map();
  let nextId = 1;

  const white = gl.createTexture();
  gl.bindTexture(gl.TEXTURE_2D, white);
  gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([255, 255, 255, 255]));
  // No mipmaps, so the default minification filter (which wants them) would leave it incomplete (black).
  gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);

  /** @type {Renderer} */
  const r = {
    kind: 'webgl2',
    upload(mesh) {
      const vao = gl.createVertexArray();
      const vb = gl.createBuffer();
      const ib = gl.createBuffer();
      if (!vao || !vb || !ib) throw new Error('out of GL objects');
      gl.bindVertexArray(vao);
      gl.bindBuffer(gl.ARRAY_BUFFER, vb);
      gl.bufferData(gl.ARRAY_BUFFER, mesh.vertices, gl.STATIC_DRAW);
      const stride = FLOATS_PER_VERTEX * 4;
      [3, 3, 3, 2].reduce((offset, size, i) => {
        gl.enableVertexAttribArray(i);
        gl.vertexAttribPointer(i, size, gl.FLOAT, false, stride, offset);
        return offset + size * 4;
      }, 0);
      gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, ib);
      gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, mesh.indices, gl.STATIC_DRAW);
      gl.bindVertexArray(null);
      const id = nextId++;
      meshes.set(id, { vao, buffers: [vb, ib], count: mesh.indices.length });
      return { id, count: mesh.indices.length };
    },
    texture(image) {
      const tex = gl.createTexture();
      if (!tex) throw new Error('out of GL objects');
      gl.bindTexture(gl.TEXTURE_2D, tex);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, /** @type {any} */ (image));
      gl.generateMipmap(gl.TEXTURE_2D);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.REPEAT);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.REPEAT);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
      const id = nextId++;
      textures.set(id, tex);
      return { id, width: image.width, height: image.height };
    },
    resize(width, height) {
      canvas.width = Math.max(1, Math.floor(width));
      canvas.height = Math.max(1, Math.floor(height));
    },
    frame(p, draws) {
      gl.viewport(0, 0, canvas.width, canvas.height);
      gl.clearColor(p.fogColor[0], p.fogColor[1], p.fogColor[2], 1);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      gl.enable(gl.DEPTH_TEST);
      gl.depthFunc(gl.LESS);
      gl.enable(gl.CULL_FACE);
      gl.cullFace(gl.BACK);
      gl.frontFace(gl.CCW);
      gl.useProgram(prog);
      const proj = mat4.perspective(p.fovY, canvas.width / canvas.height, p.near, p.far, false);
      gl.uniformMatrix4fv(u.viewProj, false, mat4.multiply(proj, p.view));
      gl.uniform4f(u.eye, p.eye[0], p.eye[1], p.eye[2], 1);
      gl.uniform4f(u.lightDir, p.lightDir[0], p.lightDir[1], p.lightDir[2], 0);
      gl.uniform4f(u.lightColor, p.lightColor[0], p.lightColor[1], p.lightColor[2], 1);
      gl.uniform4f(u.ambient, p.ambient[0], p.ambient[1], p.ambient[2], 1);
      gl.uniform4f(u.fog, p.fogColor[0], p.fogColor[1], p.fogColor[2], p.fogDensity);
      gl.uniform1i(u.tex, 0);
      gl.activeTexture(gl.TEXTURE0);
      for (const d of draws) {
        const m = meshes.get(d.mesh.id);
        if (!m) continue;
        gl.uniformMatrix4fv(u.model, false, d.model);
        gl.uniform4f(u.tint, d.tint[0], d.tint[1], d.tint[2], 1);
        gl.uniform4f(u.params, d.grid ?? 0, 0, 0, 0);
        gl.bindTexture(gl.TEXTURE_2D, (d.texture && textures.get(d.texture.id)) || white);
        gl.bindVertexArray(m.vao);
        gl.drawElements(gl.TRIANGLES, m.count, gl.UNSIGNED_INT, 0);
      }
      gl.bindVertexArray(null);
    },
    destroy() {
      for (const m of meshes.values()) {
        gl.deleteVertexArray(m.vao);
        for (const b of m.buffers) gl.deleteBuffer(b);
      }
      for (const t of textures.values()) gl.deleteTexture(t);
      gl.deleteTexture(white);
      gl.deleteProgram(prog);
    },
  };
  return r;
}
