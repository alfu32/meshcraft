// A landscape canopy built from the GPU's seeded branch hierarchy.
// Each child inherits its parent's curved frame; no mesh is generated on CPU.
void branchingRule(inout BranchRule rule, int level, int child,
                   uvec2 path, uint key) {
    float variation = random(key);
    if (level == 1) {
        rule.lengthScale *= mix(0.82, 1.06, variation);
        rule.angle += 0.12 * sin(float(child) * 2.39996);
    }
    if (level > 2) rule.tropism += 0.12;
}
void proceduralVertex(inout vec3 position, inout vec3 normal, vec2 uv) {
}
