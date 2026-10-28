// Optional per-child conditional IFS rule. Runs inside the bounded GPU ancestry loop.
// level: 1..6; child: 0-based slot; path: stable ancestry encoded as two uints.
// key: deterministic seed/path hash. Helpers hash(uint) and random(uint) are available.
// Rule fields: attach [0,1], angle/azimuth (radians), lengthScale/radiusScale,
// bend/tropism [-1,1], activation [0,1]. The evaluator clamps these to safe ranges.
// Read-only parent fields: parentStart, parentControl, parentEnd, parentFrame, parentRadius.
// Example: if (level > 2) rule.activation *= smoothstep(0.2, 0.8, random(key));
void branchingRule(inout BranchRule rule, int level, int child, uvec2 path, uint key) {
}
void proceduralVertex(inout vec3 position, inout vec3 normal, vec2 uv) {
}
