// Northern-light curtains in WGSL. Uniforms are fields of `ww`.
fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2<f32>(1.0, 0.0)), u.x),
               mix(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(1.0, 1.0)), u.x), u.y);
}

fn mainImage(fragCoord: vec2<f32>) -> vec4<f32> {
    let uv = fragCoord / ww.iResolution.xy;
    let t = ww.iTime * 0.15;
    var col = mix(vec3<f32>(0.01, 0.02, 0.06), vec3<f32>(0.02, 0.05, 0.12), uv.y);
    for (var i = 0; i < 3; i = i + 1) {
        let fi = f32(i);
        let wave = noise(vec2<f32>(uv.x * 3.0 + fi * 1.7, t + fi)) * 0.25;
        let band = exp(-abs(uv.y - (0.55 + wave + fi * 0.08)) * 18.0);
        let tint = vec3<f32>(0.2, 0.9, 0.6) * (1.0 - fi * 0.25) + vec3<f32>(0.3, 0.1, 0.5) * fi * 0.3;
        col = col + tint * band * (0.6 + 0.4 * noise(vec2<f32>(uv.x * 12.0, t * 3.0 + fi)));
    }
    let ridge = 0.18 + 0.04 * sin(uv.x * 7.0) + 0.02 * sin(uv.x * 23.0);
    if (uv.y < ridge) {
        col = vec3<f32>(0.005, 0.01, 0.02);
    }
    return vec4<f32>(col, 1.0);
}
