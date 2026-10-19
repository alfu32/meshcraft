#version 330
out vec4 finalColor;
// A regular RGBA render target with a hardware depth attachment. Store 24-bit
// depth in RGB so this works without a backend-specific depth-texture allocator.
void main() {
    float depth = floor(min(gl_FragCoord.z, 0.99999994) * 16777215.0);
    finalColor = vec4(floor(depth / 65536.0), mod(floor(depth / 256.0), 256.0), mod(depth, 256.0), 255.0) / 255.0;
}
