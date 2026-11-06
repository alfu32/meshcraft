// Called once per emitted vertex. Every three consecutive vertices form a triangle.
//   vertexId:   0 .. vertexCount-1 within one instance (vertexCount is 36 here)
//   instanceId: 0 .. instances-1
// Write the local position, normal and uv. Parameters are uniforms named u_<id>;
// customVertexCount, customInstanceCount and time (seconds) are also available.
void customVertex(int vertexId, int instanceId, out vec3 position, out vec3 normal, out vec2 uv) {
    const vec3 faces[6] = vec3[](vec3(1, 0, 0), vec3(-1, 0, 0), vec3(0, 1, 0),
                                 vec3(0, -1, 0), vec3(0, 0, 1), vec3(0, 0, -1));
    const vec2 corners[6] = vec2[](vec2(0, 0), vec2(1, 0), vec2(1, 1),
                                   vec2(0, 0), vec2(1, 1), vec2(0, 1));
    normal = faces[vertexId / 6];
    uv = corners[vertexId % 6];
    vec3 u = abs(normal.y) > 0.5 ? vec3(1, 0, 0) : vec3(0, 1, 0);
    vec3 v = cross(normal, u);
    vec2 s = uv * 2.0 - 1.0;
    position = (normal + u * s.x + v * s.y) * 0.5 + vec3(0, 0.5, 0);
    position *= u_size;
}
