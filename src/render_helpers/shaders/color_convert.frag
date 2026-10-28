
// Brings a surface with its own colour description (wp-color-management-v1) into the
// composition: decode its transfer function, scale so its reference white is 1.0, convert its
// primaries to BT.709, roll highlights off to what the target shows, and encode as the
// composition's extended gamma 2.2.

// 0 a pure power, 1 piecewise sRGB, 2 PQ, 3 linear.
uniform float cm_tf;
uniform float cm_power;
uniform mat3 cm_matrix;
// Relative light of a decoded 1.0, its reference white being 1.0.
uniform float cm_scale;
// The content's peak and the target's, in the same relative units.
uniform float cm_content_peak;
uniform float cm_target_peak;

vec3 cm_eotf(vec3 e) {
    if (cm_tf < 0.5) {
        return sign(e) * pow(abs(e), vec3(cm_power));
    } else if (cm_tf < 1.5) {
        vec3 a = abs(e);
        vec3 lo = a / 12.92;
        vec3 hi = pow((a + 0.055) / 1.055, vec3(2.4));
        return sign(e) * mix(lo, hi, step(0.04045, a));
    } else if (cm_tf < 2.5) {
        const float m1 = 0.1593017578125;
        const float m2 = 78.84375;
        const float c1 = 0.8359375;
        const float c2 = 18.8515625;
        const float c3 = 18.6875;
        vec3 p = pow(clamp(e, 0.0, 1.0), vec3(1.0 / m2));
        return pow(max(p - c1, 0.0) / (c2 - c3 * p), vec3(1.0 / m1));
    }
    return e;
}

vec4 cm(vec4 color) {
    if (color.a <= 0.0)
        return color;

    vec3 rel = cm_matrix * (cm_eotf(color.rgb / color.a) * cm_scale);

    // past three quarters of what the target shows, highlights bend smoothly towards its peak
    float m = max(max(rel.r, rel.g), rel.b);
    if (cm_content_peak > cm_target_peak && m > 0.0) {
        float knee = 0.75 * cm_target_peak;
        if (m > knee) {
            float over = m - knee;
            float bent = knee + over / (1.0 + over / (cm_target_peak - knee));
            rel *= bent / m;
        }
    }

    vec3 encoded = sign(rel) * pow(abs(rel), vec3(1.0 / 2.2));
    return vec4(encoded * color.a, color.a);
}
