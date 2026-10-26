#version 100

//_DEFINES_

#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif

// Liquid glass for blur shapes: the blurred background bent at a curved
// edge, tinted, and lit along the rim.
//
// The edge profile, the SDF-gradient refraction with its red/blue parting,
// and the adaptive tint follow kwin-effects-glass
// (https://github.com/4v3ngR/kwin-effects-glass, GPL-3.0) and its niri port
// Niri-glass (https://github.com/zaroutt/Niri-glass, GPL-3.0), rewritten for
// shapes in niri's geometry space.

precision highp float;
#if defined(EXTERNAL)
uniform samplerExternalOES tex;
#else
uniform sampler2D tex;
#endif

uniform float alpha;
varying vec2 v_coords;

#if defined(DEBUG_FLAGS)
uniform float tint;
#endif

uniform float niri_scale;

uniform vec2 geo_size;
uniform vec4 corner_radius;
uniform mat3 input_to_geo;
// From geometry space back to texture coordinates, to move a sample.
uniform mat3 geo_to_input;

// Premultiplied colour laid over the background.
uniform vec4 glass_tint;
// refraction (px), bevel (px), dispersion, rim
uniform vec4 glass_shape;
// light direction (x, y, towards the light), far rim, specular
uniform vec4 glass_light;
// tint strength, adaptive tint, tone
uniform vec3 glass_tint_params;

float niri_rounding_alpha(vec2 coords, vec2 size, vec4 corner_radius);
vec4 postprocess(vec4 color);

const vec3 LUMA = vec3(0.2126, 0.7152, 0.0722);

// Distance to the rounded rectangle's edge, below 0 inside; p from the centre,
// y down, radii top-left, top-right, bottom-right, bottom-left.
float edge_distance(vec2 p, vec2 half_size, vec4 r) {
    float radius = p.x < 0.0 ? (p.y < 0.0 ? r.x : r.w) : (p.y < 0.0 ? r.y : r.z);
    vec2 q = abs(p) - half_size + radius;
    return min(max(q.x, q.y), 0.0) + length(max(q, 0.0)) - radius;
}

// The texture `offset` logical pixels away from this fragment.
vec4 sample_at(vec2 offset) {
    vec2 moved = (geo_to_input * vec3(offset / geo_size, 0.0)).xy;
    return texture2D(tex, clamp(v_coords + moved, 0.0, 1.0));
}

void main() {
    vec3 coords_geo = input_to_geo * vec3(v_coords, 1.0);
    vec2 half_size = geo_size * 0.5;
    vec2 p = coords_geo.xy * geo_size - half_size;
    float d = edge_distance(p, half_size, corner_radius);

    // The curved edge: 1 at the rim, 0 a bevel in, rising like a lens's lip.
    float bevel = max(glass_shape.y, 0.001);
    float rise = smoothstep(0.0, 1.0, clamp(1.0 + d / bevel, 0.0, 1.0));
    float lip = 1.0 - sqrt(max(0.0, 1.0 - rise * rise));

    // Which way the edge faces, outwards.
    vec2 grad = vec2(
        edge_distance(p + vec2(1.0, 0.0), half_size, corner_radius)
            - edge_distance(p - vec2(1.0, 0.0), half_size, corner_radius),
        edge_distance(p + vec2(0.0, 1.0), half_size, corner_radius)
            - edge_distance(p - vec2(0.0, 1.0), half_size, corner_radius));
    vec2 normal = length(grad) > 0.0001 ? normalize(grad) : vec2(0.0);

    // At the lip the glass magnifies what's behind it, drawing it in from further
    // inside, as a thick lens does; it never shows what lies past its own edge.
    vec2 offset = -normal * lip * glass_shape.x;
    vec4 color;
    if (glass_shape.z > 0.001 && lip > 0.001) {
        float parting = glass_shape.z * 0.3;
        vec4 green = sample_at(offset);
        color = vec4(
            sample_at(offset * (1.0 + parting)).r,
            green.g,
            sample_at(offset * (1.0 - parting)).b,
            green.a);
    } else {
        color = sample_at(offset);
    }
#if defined(NO_ALPHA)
    color = vec4(color.rgb, 1.0);
#endif

    // The tint, thicker where the background is close to it in brightness, so
    // what's on the glass stays readable over any wallpaper.
    float tint_alpha = glass_tint.a * glass_tint_params.x;
    if (tint_alpha > 0.0) {
        vec3 tint_rgb = glass_tint.rgb / max(glass_tint.a, 0.0001);

        // The tone: the background's brightness squeezed toward the tint's side,
        // its hue kept, as Apple's darken and lighten layers do.
        float tone = glass_tint_params.z;
        if (tone < 1.0) {
            color.rgb = dot(tint_rgb, LUMA) < 0.5
                ? color.rgb * tone
                : color.a - (color.a - color.rgb) * tone;
        }

        float gap = abs(dot(color.rgb, LUMA) - dot(tint_rgb, LUMA));
        float thicken = mix(1.0, 1.6 - gap, glass_tint_params.y);
        tint_alpha = clamp(tint_alpha * thicken, 0.0, 1.0);
        color.rgb = tint_rgb * tint_alpha * color.a + color.rgb * (1.0 - tint_alpha);
    }

    // The rim: a thin line brightest where the edge faces the light, a fainter
    // catch on the far side, a soft glow over the lip, and a highlight.
    float facing = dot(normal, glass_light.xy);
    float near = pow(max(facing, 0.0), 2.0);
    float far = pow(max(-facing, 0.0), 3.0) * glass_light.z;
    float line = 1.0 - smoothstep(0.0, 1.5, -d);
    float lit = glass_shape.w * (line * (0.2 + 0.8 * near + far) + lip * 0.25 * (near + far));
    color.rgb = mix(color.rgb, vec3(color.a), clamp(lit, 0.0, 1.0));
    color.rgb += vec3(glass_light.w * line * pow(max(facing, 0.0), 8.0)) * color.a;

    color = postprocess(color);

    if (coords_geo.x < 0.0 || 1.0 < coords_geo.x || coords_geo.y < 0.0 || 1.0 < coords_geo.y) {
        // Clip outside geometry.
        color = vec4(0.0);
    } else {
        // Apply corner rounding inside geometry.
        color = color * niri_rounding_alpha(coords_geo.xy * geo_size, geo_size, corner_radius);
    }

    color = color * alpha;

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        color = vec4(0.0, 0.2, 0.0, 0.2) + color * 0.8;
#endif

    gl_FragColor = color;
}
