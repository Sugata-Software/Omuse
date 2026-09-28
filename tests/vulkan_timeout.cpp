// Test-only interposition after a real queue submission. The real fence is
// waited on first, so simulating a late completion cannot free in-flight work.
#include <vulkan/vulkan.h>
#include <atomic>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>

static std::atomic<int> waits{0}, destroys{0}, idleWaits{0};

extern "C" VKAPI_ATTR VkResult VKAPI_CALL vkWaitForFences(
    VkDevice device, uint32_t count, const VkFence *fences, VkBool32 all, uint64_t timeout) {
    waits += timeout == 2000000000ULL ? 1 : 100;
    auto realWait = reinterpret_cast<PFN_vkWaitForFences>(dlsym(RTLD_NEXT, "vkWaitForFences"));
    if (!realWait || realWait(device, count, fences, all, 2000000000ULL) != VK_SUCCESS) return VK_ERROR_UNKNOWN;
    return VK_TIMEOUT;
}
extern "C" VKAPI_ATTR VkResult VKAPI_CALL vkGetFenceStatus(VkDevice, VkFence) {
    const char *outcome = std::getenv("COMPOSITOR_TEST_FENCE");
    if (outcome && !std::strcmp(outcome, "completed")) return VK_SUCCESS;
    if (outcome && !std::strcmp(outcome, "lost")) return VK_ERROR_DEVICE_LOST;
    return VK_NOT_READY;
}
extern "C" VKAPI_ATTR void VKAPI_CALL vkDestroyDevice(VkDevice device, const VkAllocationCallbacks *allocator) {
    ++destroys;
    auto realDestroy = reinterpret_cast<PFN_vkDestroyDevice>(dlsym(RTLD_NEXT, "vkDestroyDevice"));
    if (realDestroy) realDestroy(device, allocator);
}
extern "C" VKAPI_ATTR VkResult VKAPI_CALL vkDeviceWaitIdle(VkDevice) {
    ++idleWaits;
    return VK_ERROR_UNKNOWN;
}
extern "C" int compositor_test_waits() { return waits; }
extern "C" int compositor_test_destroys() { return destroys; }
extern "C" int compositor_test_idle_waits() { return idleWaits; }
