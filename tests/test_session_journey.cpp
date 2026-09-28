// Host integration test: the retained Swift editor session driven through the C
// ABI (ENG-1 seam), no GUI. This is the host-verifiable slice of the file-map's
// "First packaged open/paint/undo/save/reopen/export journey" workstream, run
// against the *real* Swift core (libCompositorCore.a), not the C reference
// stand-in used by CompositorCore.Abi.
//
// Journey: create session -> new canvas -> paint a brush stroke -> render and
// assert pixels changed -> undo -> render and assert pixels restored -> query
// state JSON -> close. Exercises compositor_session_create/command/render/state/
// close. Only linked when CompositorCore_SWIFT_STATIC_LIB is provided.
//
// TOOLCHAIN BLOCKER (Freedesktop Swift 6.3 SDK): this C++ main cannot drive the
// Swift core directly. There is no `swift_initSwiftRuntime` entry point, so a
// non-Swift `main` cannot bootstrap the Swift runtime:
//   - Static stdlib embedded in C++: generic class metadata (`_DictionaryStorage`)
//     is never initialized; the first `Dictionary` insertion SEGVs. The static
//     archives' `.init_array` constructors are registered but insufficient.
//   - Shared stdlib: stdlib `Dictionary` works, but Foundation's `JSONDecoder` /
//     `Data.withUnsafeBytes` (`__DataStorage`) traps (UnsafeRawBufferPointer:229)
//     even on pure-Swift `Data`, so the JSON command path is unusable from C++.
// The only supported way to use Foundation is a Swift entry point. The file map's
// composition root must therefore be a Swift `@main` that bootstraps the runtime
// and calls into the Qt host via a C ABI (inverse of this test's structure), OR
// the Qt host must call a Swift init shim before any Foundation use. Until that
// composition-root change lands, this test is gated out of the default build and
// the same journey is verified in Swift under `swift test` (SessionJourneyTests),
// where Foundation is properly bootstrapped by SwiftPM.

#include "CompositorCore.h"

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

static int g_failures = 0;
#define CHECK(cond, msg) do { if (!(cond)) { std::fprintf(stderr, "FAIL: %s (%s:%d)\n", msg, __FILE__, __LINE__); ++g_failures; } } while (0)

static int32_t cmd(uint64_t h, const std::string &json) {
    return compositor_session_command(h, reinterpret_cast<const uint8_t *>(json.data()), json.size());
}

// Minimal JSON field probe: finds `"key":number` and returns the number, or -1.
static long jsonInt(const std::string &s, const char *key) {
    std::string needle = std::string("\"") + key + "\":";
    size_t p = s.find(needle);
    if (p == std::string::npos) return -1;
    p += needle.size();
    while (p < s.size() && (s[p] == ' ' || s[p] == '\t')) ++p;
    char *end = nullptr;
    long v = std::strtol(s.c_str() + p, &end, 10);
    return end == s.c_str() + p ? -1 : v;
}

// `busy`/`canUndo`/`canRedo` are Bool → JSON `true`/`false` (not 0/1).
static bool jsonBool(const std::string &s, const char *key, bool &out) {
    std::string needle = std::string("\"") + key + "\":";
    size_t p = s.find(needle);
    if (p == std::string::npos) return false;
    p += needle.size();
    while (p < s.size() && (s[p] == ' ' || s[p] == '\t')) ++p;
    if (s.compare(p, 4, "true") == 0) { out = true; return true; }
    if (s.compare(p, 5, "false") == 0) { out = false; return true; }
    return false;
}

static std::vector<uint8_t> renderAll(uint64_t h, int64_t expectedBytes) {
    std::vector<uint8_t> out(expectedBytes);
    int64_t n = compositor_session_render(h, out.data(), out.size());
    CHECK(n == expectedBytes, "render byte count matches state");
    return out;
}

int main() {
    std::fprintf(stderr, "step: create\n");
    uint64_t h = compositor_session_create();
    CHECK(h != 0, "session create");

    std::fprintf(stderr, "step: new\n");
    // New 4x4 canvas (Layer 1 active, blank/transparent).
    CHECK(cmd(h, R"({"version":1,"action":"new","width":4,"height":4})") == 0, "new canvas");

    std::fprintf(stderr, "step: addLayer\n");
    // Add a second layer so the document is non-trivial; brush paints the active one.
    CHECK(cmd(h, R"({"version":1,"action":"addLayer"})") == 0, "add layer");

    std::fprintf(stderr, "step: brushBegin\n");
    // Paint a solid red stroke: diameter 3, hardness 1, opacity 1, red=1.
    CHECK(cmd(h, R"({"version":1,"action":"brushBegin","x":1,"y":1,"parameters":{"diameter":3,"hardness":1,"opacity":1,"red":1,"green":0,"blue":0,"erasing":0,"mask":0}})") == 0, "brush begin");
    std::fprintf(stderr, "step: brushMove\n");
    CHECK(cmd(h, R"({"version":1,"action":"brushMove","x":2,"y":2})") == 0, "brush move");
    std::fprintf(stderr, "step: brushEnd\n");
    CHECK(cmd(h, R"({"version":1,"action":"brushEnd"})") == 0, "brush end");

    std::fprintf(stderr, "step: state1\n");
    // State: width/height 4, busy false, one undo available.
    int64_t stateN = compositor_session_state(h, nullptr, 0);
    CHECK(stateN > 0, "state byte count");
    std::vector<uint8_t> stateBuf(stateN);
    compositor_session_state(h, stateBuf.data(), stateBuf.size());
    std::string state(stateBuf.begin(), stateBuf.end());
    CHECK(jsonInt(state, "width") == 4, "state width");
    CHECK(jsonInt(state, "height") == 4, "state height");
    {
        bool b = false;
        CHECK(jsonBool(state, "busy", b) && b == false, "state not busy after stroke");
        bool u = false;
        CHECK(jsonBool(state, "canUndo", u) && u == true, "state can undo after stroke");
    }

    std::fprintf(stderr, "step: render1\n");
    // Render: 4*4*4 = 64 bytes premultiplied RGBA. The stroke painted red somewhere.
    std::vector<uint8_t> after = renderAll(h, 64);
    bool anyRed = false;
    for (int i = 0; i < 64; i += 4) {
        // Solid red premultiplied: R>0, G==0, B==0, A>0.
        if (after[i] > 0 && after[i + 1] == 0 && after[i + 2] == 0 && after[i + 3] > 0) { anyRed = true; break; }
    }
    CHECK(anyRed, "render shows red paint after stroke");

    std::fprintf(stderr, "step: undo\n");
    // Undo restores the blank layer.
    CHECK(cmd(h, R"({"version":1,"action":"undo"})") == 0, "undo");
    int64_t stateN2 = compositor_session_state(h, nullptr, 0);
    std::vector<uint8_t> sb2(stateN2);
    compositor_session_state(h, sb2.data(), sb2.size());
    std::string state2(sb2.begin(), sb2.end());
    {
        bool r = false;
        CHECK(jsonBool(state2, "canRedo", r) && r == true, "state can redo after undo");
    }
    std::vector<uint8_t> restored = renderAll(h, 64);
    bool allBlank = true;
    for (int i = 0; i < 64; i += 4) {
        if (after.size()) {} // keep `after` referenced
        if (restored[i] != 0 || restored[i + 1] != 0 || restored[i + 2] != 0 || restored[i + 3] != 0) { allBlank = false; break; }
    }
    CHECK(allBlank, "render is blank after undo");

    // Redo reapplies the stroke.
    CHECK(cmd(h, R"({"version":1,"action":"redo"})") == 0, "redo");
    std::vector<uint8_t> redone = renderAll(h, 64);
    bool anyRed2 = false;
    for (int i = 0; i < 64; i += 4) {
        if (redone[i] > 0 && redone[i + 1] == 0 && redone[i + 2] == 0 && redone[i + 3] > 0) { anyRed2 = true; break; }
    }
    CHECK(anyRed2, "render shows red paint after redo");

    compositor_session_close(h);

    // A closed handle must reject commands (-6 invalid/closed handle).
    CHECK(cmd(h, R"({"version":1,"action":"undo"})") == -6, "closed handle rejected");

    if (g_failures == 0) { std::printf("All session journey tests passed.\n"); return 0; }
    std::fprintf(stderr, "%d session journey test(s) failed.\n", g_failures);
    return 1;
}