#version 440

// The bar's copy of hyprcustard's shape shader (crates/custard-gui/shaders/
// neu.frag): a rounded rectangle raised out of the background or pressed
// into it, with its soft shadows, drawn in one pass. One change: a raised
// shape's shadows are drawn only outside it. The bar floats over the
// wallpaper and its fill may be see-through, so a shadow under the fill
// would darken it and hide the wallpaper.
//
// The shadows are a Gaussian falloff from the shape's signed distance, so
// they have no stops to show as lines, and the result is dithered, so a
// shadow that crosses only a handful of 8-bit levels does not show them as
// steps. The dither is sized by what reaches the screen, not by the shadow's
// alpha: a faint shadow tail is where the levels are widest apart, so that is
// where the dither is needed most. `make shaders` compiles this into neu.frag.qsb beside it, which is committed
// and written into the bar's folder with its QML.

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    vec2 size;      // the effect item, in pixels
    vec4 shape;     // the shape inside it: x, y, width, height
    float radius;
    float offset;   // how far the light moves the shadows
    float blur;     // the shadows' Gaussian sigma
    float sunk;     // 0 raised, 1 pressed in
    vec4 fill;      // premultiplied, as Qt passes colors
    vec4 dark;
    vec4 light;
    vec4 ground;    // the background the shadows fall on
};

float box(vec2 p, vec2 half_, float r)
{
    vec2 q = abs(p) - half_ + r;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

// Abramowitz and Stegun 7.1.26: good to 1.5e-7, far below one 8-bit level.
float erf_(float x)
{
    float a = abs(x);
    float t = 1.0 / (1.0 + 0.3275911 * a);
    float y = 1.0 - ((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t
                      - 0.284496736) * t + 0.254829592) * t * exp(-a * a);
    return sign(x) * y;
}

// How much of a shape blurred by `blur` lies over a point `d` from its edge.
float soft(float d)
{
    return 0.5 - 0.5 * erf_(d / (max(blur, 0.5) * 1.41421356));
}

// Premultiplied `top` over `under`.
vec4 over(vec4 top, vec4 under)
{
    return top + under * (1.0 - top.a);
}

void main()
{
    vec2 half_ = shape.zw * 0.5;
    vec2 p = qt_TexCoord0 * size - (shape.xy + half_);
    float r = min(radius, min(half_.x, half_.y));
    vec2 o = vec2(offset);

    float d = box(p, half_, r);
    float cover = clamp(0.5 - d, 0.0, 1.0);

    vec4 c;
    if (sunk < 0.5) {
        // Lit at the top left, so the dark shadow falls to the bottom right
        // and the light one to the top left.
        vec4 shadows = over(light * soft(box(p + o, half_, r)),
                            dark * soft(box(p - o, half_, r)));
        c = over(fill * cover, shadows * (1.0 - cover));
    } else {
        // The rim stands over the floor: its top and left edges shade it,
        // and light comes back off its bottom and right.
        float shade = 1.0 - soft(box(p - o, half_, r));
        float glint = 1.0 - soft(box(p + o, half_, r));
        c = over(light * glint, over(dark * shade, fill)) * cover;
    }

    // How far this pixel moves the ground under it, in 8-bit levels. A flat
    // face the color of the ground moves it not at all and stays clean; any
    // shading that moves it by a level or more gets the full dither.
    vec3 moved = abs(c.rgb - ground.rgb * c.a) * 255.0;
    float w = clamp(max(moved.r, max(moved.g, moved.b)), 0.0, 1.0);

    // Interleaved gradient noise, half a level either way on the screen.
    // Added to premultiplied color it shifts the blended result by the same
    // amount whatever the alpha, so it is not scaled down by it.
    float n = fract(52.9829189 * fract(dot(gl_FragCoord.xy, vec2(0.06711056, 0.00583715))));
    c.rgb = max(c.rgb + (n - 0.5) / 255.0 * w, 0.0);

    fragColor = c * qt_Opacity;
}
