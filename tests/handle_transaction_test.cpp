/* Exercise real mid-publication failures without attempting machine-wide OOM. */
#include "bridge_internal.hpp"

#include <algorithm>
#include <array>
#include <cstdio>
#include <limits>

namespace {
bool expect(occt_bridge_status_t status, occt_bridge_status_t expected, const char* name) {
    if (status == expected) { return true; }
    (void)std::fprintf(stderr, "%s returned %d, expected %d\n", name, status, expected);
    return false;
}
}

int main() {
    occt_bridge_session_t* session = nullptr;
    if (!expect(occt_bridge_session_create(OCCT_BRIDGE_ABI_VERSION, &session), OCCT_BRIDGE_OK, "create session")) { return 1; }
    const occt_bridge_vec3_t origin{0,0,0};
    const occt_bridge_vec3_t up{0,0,1};
    const occt_bridge_vec3_t right{1,0,0};
    occt_bridge_shape_id_t body = 0;
    bool passed = expect(occt_bridge_create_box(session, origin, {1,1,1}, &body), OCCT_BRIDGE_OK, "create box");

    // The first output gets the last valid ID; the next store must fail. The
    // APIs must retract the first handle and leave no partially published array.
    session->next_shape_id = std::numeric_limits<occt_bridge_shape_id_t>::max();
    std::array<occt_bridge_shape_id_t,12> edges{};
    edges.fill(111);
    size_t count = 999;
    passed = expect(occt_bridge_shape_subshapes(session, body, OCCT_BRIDGE_SHAPE_EDGE, edges.data(), edges.size(), &count), OCCT_BRIDGE_INTERNAL_ERROR, "bulk rollback") && passed;
    passed = count == 0 && std::all_of(edges.begin(), edges.end(), [](auto id) { return id == 111; }) && passed;
    size_t handles = 0;
    passed = expect(occt_bridge_session_shape_count(session, &handles), OCCT_BRIDGE_OK, "shape count after bulk failure") && handles == 1 && passed;

    session->next_shape_id = std::numeric_limits<occt_bridge_shape_id_t>::max();
    occt_bridge_shape_id_t visible = 999, hidden = 999;
    passed = expect(occt_bridge_orthographic_projection(session, body, origin, up, right, &visible, &hidden), OCCT_BRIDGE_INTERNAL_ERROR, "projection rollback") && passed;
    passed = visible == 0 && hidden == 0 && passed;
    passed = expect(occt_bridge_session_shape_count(session, &handles), OCCT_BRIDGE_OK, "shape count after projection failure") && handles == 1 && passed;

    occt_bridge_session_destroy(session);
    if (!passed) { (void)std::fprintf(stderr, "transactional handle checks failed\n"); }
    return passed ? 0 : 1;
}
