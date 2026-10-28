#version 100

//_DEFINES_

#if defined(EXTERNAL)
#extension GL_OES_EGL_image_external : require
#endif

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

// How bright 1.0 (SDR white) is, in nits.
uniform float sdr_white;
// The gamma ramp a gamma-control client asked for (night light), fitted per channel as
// gain * x^exponent on the encoded value; gain 1 and exponent 1 leave it alone.
uniform vec3 ramp_gain;
uniform vec3 ramp_exponent;

// The composition is encoded as extended gamma 2.2: 1.0 is SDR white, values past it are
// brighter, and the curve is mirrored for negatives (colours outside BT.709).
vec3 decode(vec3 encoded) {
    return sign(encoded) * pow(abs(encoded), vec3(2.2));
}

vec3 apply_ramp(vec3 encoded) {
    vec3 inside = ramp_gain * pow(clamp(encoded, 0.0, 1.0), ramp_exponent);
    // past SDR white the ramp's gain carries on, so highlights keep its tint
    vec3 outside = ramp_gain * encoded;
    return mix(inside, outside, step(1.0, encoded));
}

// BT.709 primaries to BT.2020, in linear light.
vec3 to_bt2020(vec3 c) {
    return vec3(
        dot(vec3(0.6274040, 0.3292820, 0.0433136), c),
        dot(vec3(0.0690970, 0.9195400, 0.0113612), c),
        dot(vec3(0.0163916, 0.0880132, 0.8955950), c)
    );
}

// SMPTE ST 2084 (PQ) from nits.
vec3 pq(vec3 nits) {
    const float m1 = 0.1593017578125;
    const float m2 = 78.84375;
    const float c1 = 0.8359375;
    const float c2 = 18.8515625;
    const float c3 = 18.6875;
    vec3 y = pow(clamp(nits / 10000.0, 0.0, 1.0), vec3(m1));
    return pow((c1 + c2 * y) / (1.0 + c3 * y), vec3(m2));
}

void main() {
    vec4 color = texture2D(tex, v_coords);

    vec3 linear = decode(apply_ramp(color.rgb)) * sdr_white;
    vec3 encoded = pq(max(to_bt2020(linear), 0.0));

#if defined(DEBUG_FLAGS)
    if (tint == 1.0)
        encoded = vec3(0.0, 0.2, 0.0) + encoded * 0.8;
#endif

    gl_FragColor = vec4(encoded, 1.0) * alpha;
}
