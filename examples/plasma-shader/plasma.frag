// A Shadertoy-style plasma. Paste-compatible: mainImage, iTime, iResolution, iMouse.
void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    // The pointer nudges the center, if the user allowed mouse access.
    vec2 center = iMouse.x > 0.0 ? iMouse.xy / iResolution.xy : vec2(0.5);
    float t = iTime * 0.4;
    float v = sin(uv.x * 9.0 + t) + sin((uv.y * 9.0 + t) * 0.5);
    v += sin((uv.x * 6.0 + uv.y * 6.0 + t) * 0.5);
    vec2 c = (uv - center) * 10.0 + vec2(sin(t / 3.0), cos(t / 2.0)) * 3.0;
    v += sin(sqrt(dot(c, c) + 1.0) + t);
    vec3 col = 0.5 + 0.5 * cos(v + vec3(0.0, 2.1, 4.2));
    fragColor = vec4(col * 0.85, 1.0);
}
