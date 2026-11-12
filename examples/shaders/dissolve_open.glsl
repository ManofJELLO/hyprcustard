#version 320 es
precision highp float;
// @label Dissolve open
// @desc Breaks in from noise as the window appears
// @duration 0.4

in  vec2 v_texcoord;
out vec4 fragColor;
uniform sampler2D tex;
uniform float progress;   // 0 -> 1 across the animation
uniform float seed;       // differs per window, so two windows break up differently

// @param 1.0 60.0 0.5 "Grain size"
const float GRAIN = 18.0;

// @param 0.0 0.6 0.01 "Edge softness"
const float SOFTNESS = 0.18;

float noise(vec2 p) {
    return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void main() {
    float n = noise(floor(v_texcoord * GRAIN) + seed);
    float edge = smoothstep(n - SOFTNESS, n + SOFTNESS, progress);

    vec4 col = texture(tex, v_texcoord);
    // Premultiplied: color never exceeds alpha, so the wrapper's clamp is a no-op.
    fragColor = col * edge;
}
