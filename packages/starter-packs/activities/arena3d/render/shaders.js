// @ts-check
// The one forward-shading program, in WGSL (WebGPU) and GLSL ES 3.00 (WebGL2), kept side by side so they stay
// the same: one directional light plus ambient, vertex colour x material tint x optional texture, an optional
// procedural grid (world-space lines on the floor, no texture needed) and distance fog. Faceted low-poly
// shading comes from the meshes' per-face normals, not from the shader.
//
// Uniform layout (both backends):
//   frame: viewProj mat4 | eye vec4 | lightDir vec4 (xyz, direction the light travels) | lightColor vec4 |
//          ambient vec4 | fog vec4 (rgb colour, w density)
//   draw:  model mat4 | tint vec4 | params vec4 (x grid strength, y unused, z unused, w unused)

export const WGSL = /* wgsl */ `
struct Frame {
  viewProj: mat4x4f,
  eye: vec4f,
  lightDir: vec4f,
  lightColor: vec4f,
  ambient: vec4f,
  fog: vec4f,
};
struct Draw {
  model: mat4x4f,
  tint: vec4f,
  params: vec4f,
};
@group(0) @binding(0) var<uniform> frame: Frame;
@group(0) @binding(1) var<uniform> draw: Draw;
@group(1) @binding(0) var samp: sampler;
@group(1) @binding(1) var tex: texture_2d<f32>;

struct VOut {
  @builtin(position) pos: vec4f,
  @location(0) world: vec3f,
  @location(1) normal: vec3f,
  @location(2) color: vec3f,
  @location(3) uv: vec2f,
};

@vertex
fn vs(@location(0) p: vec3f, @location(1) n: vec3f, @location(2) c: vec3f, @location(3) uv: vec2f) -> VOut {
  var o: VOut;
  let w = draw.model * vec4f(p, 1.0);
  o.pos = frame.viewProj * w;
  o.world = w.xyz;
  o.normal = (draw.model * vec4f(n, 0.0)).xyz;
  o.color = c;
  o.uv = uv;
  return o;
}

@fragment
fn fs(i: VOut) -> @location(0) vec4f {
  let texel = textureSample(tex, samp, i.uv).rgb;
  let n = normalize(i.normal);
  let base = i.color * draw.tint.rgb * texel;
  var col = base * (frame.ambient.rgb + frame.lightColor.rgb * max(dot(n, -frame.lightDir.xyz), 0.0));
  let g = abs(fract(i.world.xz) - vec2f(0.5));
  let line = step(0.47, max(g.x, g.y));
  col = mix(col, col * 0.72, line * draw.params.x);
  let d = distance(i.world, frame.eye.xyz);
  let f = 1.0 - exp(-d * frame.fog.w);
  col = mix(col, frame.fog.rgb, f);
  // Colours are linear and the canvas is not an sRGB target, so encode here (gamma 2.2 is close enough).
  return vec4f(pow(col, vec3f(1.0 / 2.2)), 1.0);
}
`;

export const GLSL_VS = /* glsl */ `#version 300 es
layout(location = 0) in vec3 aPos;
layout(location = 1) in vec3 aNormal;
layout(location = 2) in vec3 aColor;
layout(location = 3) in vec2 aUv;
uniform mat4 uViewProj;
uniform mat4 uModel;
out vec3 vWorld;
out vec3 vNormal;
out vec3 vColor;
out vec2 vUv;
void main() {
  vec4 w = uModel * vec4(aPos, 1.0);
  gl_Position = uViewProj * w;
  vWorld = w.xyz;
  vNormal = (uModel * vec4(aNormal, 0.0)).xyz;
  vColor = aColor;
  vUv = aUv;
}
`;

export const GLSL_FS = /* glsl */ `#version 300 es
precision highp float;
in vec3 vWorld;
in vec3 vNormal;
in vec3 vColor;
in vec2 vUv;
uniform vec4 uEye;
uniform vec4 uLightDir;
uniform vec4 uLightColor;
uniform vec4 uAmbient;
uniform vec4 uFog;
uniform vec4 uTint;
uniform vec4 uParams;
uniform sampler2D uTex;
out vec4 outColor;
void main() {
  vec3 texel = texture(uTex, vUv).rgb;
  vec3 n = normalize(vNormal);
  vec3 base = vColor * uTint.rgb * texel;
  vec3 col = base * (uAmbient.rgb + uLightColor.rgb * max(dot(n, -uLightDir.xyz), 0.0));
  vec2 g = abs(fract(vWorld.xz) - vec2(0.5));
  float line = step(0.47, max(g.x, g.y));
  col = mix(col, col * 0.72, line * uParams.x);
  float d = distance(vWorld, uEye.xyz);
  float f = 1.0 - exp(-d * uFog.w);
  col = mix(col, uFog.rgb, f);
  outColor = vec4(pow(col, vec3(1.0 / 2.2)), 1.0);
}
`;
