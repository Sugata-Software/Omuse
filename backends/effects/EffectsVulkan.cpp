// Vulkan tier of the layer-effects chain: the nine passes as one compute shader (shaders/effects.comp), selected by a push
// constant and run back to back with a barrier between them. Any failure returns -2 so the caller falls through to the
// next backend (the C++ tier); after a failure the context stays out of use. Working buffers are host-visible: simple and
// correct everywhere including software rasterizers; device-local staging is a later optimisation.
#include "CompositorEffectsBackend.h"
#include "shaders/effects_spv.h"
#include <vulkan/vulkan.h>
#include <algorithm>
#include <array>
#include <cstring>
#include <memory>
#include <mutex>
#include <stdexcept>
#include <vector>

namespace {
void check(VkResult result) { if (result != VK_SUCCESS) throw std::runtime_error("Vulkan effects operation failed"); }

struct Buffer {
    VkDevice device = VK_NULL_HANDLE;
    VkBuffer buffer = VK_NULL_HANDLE;
    VkDeviceMemory memory = VK_NULL_HANDLE;
    void *mapped = nullptr;
    bool coherent = false;
    VkDeviceSize size = 0;
    ~Buffer() {
        if (mapped) vkUnmapMemory(device, memory);
        if (buffer) vkDestroyBuffer(device, buffer, nullptr);
        if (memory) vkFreeMemory(device, memory, nullptr);
    }
    void create(VkDevice dev, VkPhysicalDevice physical, VkDeviceSize bytes) {
        device = dev; size = bytes;
        VkBufferCreateInfo info{VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO};
        info.size = bytes; info.usage = VK_BUFFER_USAGE_STORAGE_BUFFER_BIT; info.sharingMode = VK_SHARING_MODE_EXCLUSIVE;
        check(vkCreateBuffer(device, &info, nullptr, &buffer));
        VkMemoryRequirements requirements{}; vkGetBufferMemoryRequirements(device, buffer, &requirements);
        VkPhysicalDeviceMemoryProperties properties{}; vkGetPhysicalDeviceMemoryProperties(physical, &properties);
        uint32_t type = UINT32_MAX;
        for (uint32_t i = 0; i < properties.memoryTypeCount; ++i) {
            const auto flags = properties.memoryTypes[i].propertyFlags;
            if ((requirements.memoryTypeBits & (1u << i)) && (flags & VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT)) {
                type = i;
                if (flags & VK_MEMORY_PROPERTY_HOST_COHERENT_BIT) break;
            }
        }
        if (type == UINT32_MAX) throw std::runtime_error("No host-visible Vulkan memory");
        coherent = (properties.memoryTypes[type].propertyFlags & VK_MEMORY_PROPERTY_HOST_COHERENT_BIT) != 0;
        VkMemoryAllocateInfo allocation{VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
        allocation.allocationSize = requirements.size; allocation.memoryTypeIndex = type;
        check(vkAllocateMemory(device, &allocation, nullptr, &memory));
        check(vkBindBufferMemory(device, buffer, memory, 0));
        check(vkMapMemory(device, memory, 0, VK_WHOLE_SIZE, 0, &mapped));
    }
    void flush() {
        if (coherent) return;
        VkMappedMemoryRange range{VK_STRUCTURE_TYPE_MAPPED_MEMORY_RANGE}; range.memory = memory; range.size = VK_WHOLE_SIZE;
        check(vkFlushMappedMemoryRanges(device, 1, &range));
    }
    void invalidate() {
        if (coherent) return;
        VkMappedMemoryRange range{VK_STRUCTURE_TYPE_MAPPED_MEMORY_RANGE}; range.memory = memory; range.size = VK_WHOLE_SIZE;
        check(vkInvalidateMappedMemoryRanges(device, 1, &range));
    }
};

// std430 layout of the shader's Params block.
struct GpuParams {
    uint32_t width, height, strokeReach, flags;
    float shadowDx, shadowDy, shadowSigma, innerDx;
    float innerDy, innerSigma, outerSigma, pad1;
    float stroke[4], shadow[4], overlay[4], inner[4], outer[4];
};
static_assert(sizeof(GpuParams) == 128, "Params block layout mismatch");

constexpr uint32_t kBindings = 11;         // pixels, result, eight planes, params
constexpr size_t kMaxPixels = 100000000;
}  // namespace

struct CompositorVulkanEffects {
    std::mutex lock;
    VkInstance instance = VK_NULL_HANDLE;
    VkPhysicalDevice physical = VK_NULL_HANDLE;
    VkDevice device = VK_NULL_HANDLE;
    VkQueue queue = VK_NULL_HANDLE;
    VkDescriptorSetLayout descriptorLayout = VK_NULL_HANDLE;
    VkDescriptorPool descriptorPool = VK_NULL_HANDLE;
    VkDescriptorSet descriptorSet = VK_NULL_HANDLE;
    VkPipelineLayout pipelineLayout = VK_NULL_HANDLE;
    VkPipeline pipeline = VK_NULL_HANDLE;
    VkShaderModule shader = VK_NULL_HANDLE;
    VkCommandPool commandPool = VK_NULL_HANDLE;
    VkCommandBuffer command = VK_NULL_HANDLE;
    VkFence fence = VK_NULL_HANDLE;
    VkPhysicalDeviceProperties properties{};
    bool usable = true;

    ~CompositorVulkanEffects() {
        if (device) vkDeviceWaitIdle(device);
        if (fence) vkDestroyFence(device, fence, nullptr);
        if (commandPool) vkDestroyCommandPool(device, commandPool, nullptr);
        if (pipeline) vkDestroyPipeline(device, pipeline, nullptr);
        if (shader) vkDestroyShaderModule(device, shader, nullptr);
        if (pipelineLayout) vkDestroyPipelineLayout(device, pipelineLayout, nullptr);
        if (descriptorPool) vkDestroyDescriptorPool(device, descriptorPool, nullptr);
        if (descriptorLayout) vkDestroyDescriptorSetLayout(device, descriptorLayout, nullptr);
        if (device) vkDestroyDevice(device, nullptr);
        if (instance) vkDestroyInstance(instance, nullptr);
    }

    void initialize() {
        VkApplicationInfo application{VK_STRUCTURE_TYPE_APPLICATION_INFO};
        application.pApplicationName = "Compositor effects"; application.apiVersion = VK_API_VERSION_1_0;
        VkInstanceCreateInfo info{VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO}; info.pApplicationInfo = &application;
        check(vkCreateInstance(&info, nullptr, &instance));
        uint32_t count = 0; check(vkEnumeratePhysicalDevices(instance, &count, nullptr));
        std::vector<VkPhysicalDevice> devices(count); check(vkEnumeratePhysicalDevices(instance, &count, devices.data()));
        uint32_t family = 0; int best = -1;
        for (auto candidate : devices) {
            VkPhysicalDeviceProperties props{}; vkGetPhysicalDeviceProperties(candidate, &props);
            if (props.limits.maxComputeWorkGroupInvocations < 256 || props.limits.maxComputeWorkGroupSize[0] < 16 ||
                props.limits.maxComputeWorkGroupSize[1] < 16 || props.limits.maxPerStageDescriptorStorageBuffers < kBindings) continue;
            uint32_t families = 0; vkGetPhysicalDeviceQueueFamilyProperties(candidate, &families, nullptr);
            std::vector<VkQueueFamilyProperties> queues(families); vkGetPhysicalDeviceQueueFamilyProperties(candidate, &families, queues.data());
            const int score = props.deviceType == VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU ? 3 :
                props.deviceType == VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU ? 2 : props.deviceType == VK_PHYSICAL_DEVICE_TYPE_CPU ? 0 : 1;
            for (uint32_t i = 0; i < families; ++i)
                if (queues[i].queueCount && (queues[i].queueFlags & VK_QUEUE_COMPUTE_BIT) && score > best) {
                    physical = candidate; family = i; best = score; properties = props;
                }
        }
        if (!physical) throw std::runtime_error("No Vulkan compute queue");
        float priority = 1;
        VkDeviceQueueCreateInfo queueInfo{VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO};
        queueInfo.queueFamilyIndex = family; queueInfo.queueCount = 1; queueInfo.pQueuePriorities = &priority;
        VkDeviceCreateInfo deviceInfo{VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO}; deviceInfo.queueCreateInfoCount = 1; deviceInfo.pQueueCreateInfos = &queueInfo;
        check(vkCreateDevice(physical, &deviceInfo, nullptr, &device)); vkGetDeviceQueue(device, family, 0, &queue);
        VkDescriptorSetLayoutBinding bindings[kBindings]{};
        for (uint32_t i = 0; i < kBindings; ++i) {
            bindings[i].binding = i; bindings[i].descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER;
            bindings[i].descriptorCount = 1; bindings[i].stageFlags = VK_SHADER_STAGE_COMPUTE_BIT;
        }
        VkDescriptorSetLayoutCreateInfo layoutInfo{VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO};
        layoutInfo.bindingCount = kBindings; layoutInfo.pBindings = bindings;
        check(vkCreateDescriptorSetLayout(device, &layoutInfo, nullptr, &descriptorLayout));
        VkPushConstantRange constants{VK_SHADER_STAGE_COMPUTE_BIT, 0, 4};
        VkPipelineLayoutCreateInfo pipelineInfo{VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO};
        pipelineInfo.setLayoutCount = 1; pipelineInfo.pSetLayouts = &descriptorLayout;
        pipelineInfo.pushConstantRangeCount = 1; pipelineInfo.pPushConstantRanges = &constants;
        check(vkCreatePipelineLayout(device, &pipelineInfo, nullptr, &pipelineLayout));
        VkShaderModuleCreateInfo shaderInfo{VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO};
        shaderInfo.codeSize = sizeof(compositor_effects_spv); shaderInfo.pCode = compositor_effects_spv;
        check(vkCreateShaderModule(device, &shaderInfo, nullptr, &shader));
        VkComputePipelineCreateInfo compute{VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO};
        compute.stage.sType = VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
        compute.stage.stage = VK_SHADER_STAGE_COMPUTE_BIT; compute.stage.module = shader; compute.stage.pName = "main";
        compute.layout = pipelineLayout;
        check(vkCreateComputePipelines(device, VK_NULL_HANDLE, 1, &compute, nullptr, &pipeline));
        VkDescriptorPoolSize poolSize{VK_DESCRIPTOR_TYPE_STORAGE_BUFFER, kBindings};
        VkDescriptorPoolCreateInfo poolInfo{VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO};
        poolInfo.maxSets = 1; poolInfo.poolSizeCount = 1; poolInfo.pPoolSizes = &poolSize;
        check(vkCreateDescriptorPool(device, &poolInfo, nullptr, &descriptorPool));
        VkDescriptorSetAllocateInfo allocate{VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO};
        allocate.descriptorPool = descriptorPool; allocate.descriptorSetCount = 1; allocate.pSetLayouts = &descriptorLayout;
        check(vkAllocateDescriptorSets(device, &allocate, &descriptorSet));
        VkCommandPoolCreateInfo commandInfo{VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO};
        commandInfo.flags = VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT; commandInfo.queueFamilyIndex = family;
        check(vkCreateCommandPool(device, &commandInfo, nullptr, &commandPool));
        VkCommandBufferAllocateInfo commandAllocate{VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO};
        commandAllocate.commandPool = commandPool; commandAllocate.level = VK_COMMAND_BUFFER_LEVEL_PRIMARY; commandAllocate.commandBufferCount = 1;
        check(vkAllocateCommandBuffers(device, &commandAllocate, &command));
        VkFenceCreateInfo fenceInfo{VK_STRUCTURE_TYPE_FENCE_CREATE_INFO}; check(vkCreateFence(device, &fenceInfo, nullptr, &fence));
    }

    static GpuParams gpuParams(const CompositorEffectsParams &p) {
        GpuParams g{};
        g.width = p.width; g.height = p.height; g.strokeReach = std::max(1u, p.stroke_reach);
        g.flags = (p.has_stroke ? 1u : 0u) | (p.stroke_inside ? 2u : 0u) | (p.has_shadow ? 4u : 0u) |
                  (p.has_overlay ? 8u : 0u) | (p.has_inner ? 16u : 0u) | (p.has_outer ? 32u : 0u);
        g.shadowDx = p.shadow_dx; g.shadowDy = p.shadow_dy; g.shadowSigma = p.shadow_sigma;
        g.innerDx = p.inner_dx; g.innerDy = p.inner_dy; g.innerSigma = p.inner_sigma; g.outerSigma = p.outer_sigma;
        auto put = [](float (&to)[4], const CompositorEffectColor &c) { to[0] = c.r; to[1] = c.g; to[2] = c.b; to[3] = c.opacity; };
        put(g.stroke, p.stroke); put(g.shadow, p.shadow); put(g.overlay, p.overlay); put(g.inner, p.inner); put(g.outer, p.outer);
        return g;
    }

    void render(const CompositorEffectsParams &p, const uint8_t *pixels, uint8_t *out) {
        const size_t count = static_cast<size_t>(p.width) * p.height;
        const VkDeviceSize planeBytes = count * sizeof(float), pixelBytes = count * 4;
        if (planeBytes > properties.limits.maxStorageBufferRange) throw std::runtime_error("Image too large for this device");
        std::array<std::unique_ptr<Buffer>, kBindings> buffers;
        const VkDeviceSize sizes[kBindings] = {pixelBytes, pixelBytes, planeBytes, planeBytes, planeBytes, planeBytes,
                                               planeBytes, planeBytes, planeBytes, planeBytes, sizeof(GpuParams)};
        VkDescriptorBufferInfo bufferInfo[kBindings]{}; VkWriteDescriptorSet writes[kBindings]{};
        for (uint32_t i = 0; i < kBindings; ++i) {
            buffers[i] = std::make_unique<Buffer>(); buffers[i]->create(device, physical, sizes[i]);
            bufferInfo[i] = {buffers[i]->buffer, 0, sizes[i]};
            writes[i].sType = VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET; writes[i].dstSet = descriptorSet; writes[i].dstBinding = i;
            writes[i].descriptorCount = 1; writes[i].descriptorType = VK_DESCRIPTOR_TYPE_STORAGE_BUFFER; writes[i].pBufferInfo = &bufferInfo[i];
        }
        vkUpdateDescriptorSets(device, kBindings, writes, 0, nullptr);
        std::memcpy(buffers[0]->mapped, pixels, pixelBytes);
        const GpuParams gpu = gpuParams(p);
        std::memcpy(buffers[10]->mapped, &gpu, sizeof gpu);
        buffers[0]->flush(); buffers[10]->flush();

        // The same sequence as the C++ tier: alpha; stroke reach + ring; shadow move + blur; inner move + blur + inside;
        // outer glow blur + exclude interior; compose.
        std::vector<uint32_t> passes{0};
        if (p.has_stroke) passes.insert(passes.end(), {1, 2, 3});
        if (p.has_shadow) { passes.push_back(4); if (p.shadow_sigma > 0.01f) passes.insert(passes.end(), {5, 6}); }
        if (p.has_inner) { passes.push_back(7); if (p.inner_sigma > 0.01f) passes.insert(passes.end(), {8, 9}); passes.push_back(10); }
        if (p.has_outer) {
            if (p.outer_sigma > 0.01f) passes.insert(passes.end(), {11, 12});
            else { passes.push_back(14); }   // pass 14: p7 = p0 (copy, no blur) before the exclude step
            passes.push_back(13);
        }
        passes.push_back(99);   // compose (any value the switch doesn't name explicitly)

        check(vkResetFences(device, 1, &fence)); check(vkResetCommandBuffer(command, 0));
        VkCommandBufferBeginInfo begin{VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO}; begin.flags = VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
        check(vkBeginCommandBuffer(command, &begin));
        VkMemoryBarrier upload{VK_STRUCTURE_TYPE_MEMORY_BARRIER}; upload.srcAccessMask = VK_ACCESS_HOST_WRITE_BIT;
        upload.dstAccessMask = VK_ACCESS_SHADER_READ_BIT | VK_ACCESS_SHADER_WRITE_BIT;
        vkCmdPipelineBarrier(command, VK_PIPELINE_STAGE_HOST_BIT, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, 0, 1, &upload, 0, nullptr, 0, nullptr);
        vkCmdBindPipeline(command, VK_PIPELINE_BIND_POINT_COMPUTE, pipeline);
        vkCmdBindDescriptorSets(command, VK_PIPELINE_BIND_POINT_COMPUTE, pipelineLayout, 0, 1, &descriptorSet, 0, nullptr);
        for (uint32_t pass : passes) {
            vkCmdPushConstants(command, pipelineLayout, VK_SHADER_STAGE_COMPUTE_BIT, 0, 4, &pass);
            vkCmdDispatch(command, (p.width + 15) / 16, (p.height + 15) / 16, 1);
            // Each pass reads what the one before wrote.
            VkMemoryBarrier between{VK_STRUCTURE_TYPE_MEMORY_BARRIER};
            between.srcAccessMask = VK_ACCESS_SHADER_WRITE_BIT; between.dstAccessMask = VK_ACCESS_SHADER_READ_BIT | VK_ACCESS_SHADER_WRITE_BIT;
            vkCmdPipelineBarrier(command, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, 0, 1, &between, 0, nullptr, 0, nullptr);
        }
        VkMemoryBarrier download{VK_STRUCTURE_TYPE_MEMORY_BARRIER}; download.srcAccessMask = VK_ACCESS_SHADER_WRITE_BIT; download.dstAccessMask = VK_ACCESS_HOST_READ_BIT;
        vkCmdPipelineBarrier(command, VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT, VK_PIPELINE_STAGE_HOST_BIT, 0, 1, &download, 0, nullptr, 0, nullptr);
        check(vkEndCommandBuffer(command));
        VkSubmitInfo submit{VK_STRUCTURE_TYPE_SUBMIT_INFO}; submit.commandBufferCount = 1; submit.pCommandBuffers = &command;
        check(vkQueueSubmit(queue, 1, &submit, fence));
        check(vkWaitForFences(device, 1, &fence, VK_TRUE, UINT64_MAX));
        buffers[1]->invalidate();
        // Copied only after the fence and the visibility operation.
        std::memcpy(out, buffers[1]->mapped, pixelBytes);
    }
};

extern "C" CompositorVulkanEffects *compositor_vulkan_effects_create(void) {
    try { auto context = std::make_unique<CompositorVulkanEffects>(); context->initialize(); return context.release(); }
    catch (...) { return nullptr; }
}
extern "C" void compositor_vulkan_effects_destroy(CompositorVulkanEffects *context) { delete context; }
extern "C" int compositor_vulkan_effects_render(CompositorVulkanEffects *context, const CompositorEffectsParams *p,
                                                const uint8_t *pixels, uint8_t *out) {
    if (!p || !pixels || !out || p->width == 0 || p->height == 0 || static_cast<size_t>(p->width) * p->height > kMaxPixels) return -1;
    if (!context) return -2;
    std::lock_guard<std::mutex> guard(context->lock);
    if (!context->usable) return -2;
    try { context->render(*p, pixels, out); return 0; }
    catch (...) { context->usable = false; return -2; }
}
extern "C" const char *compositor_vulkan_effects_device(CompositorVulkanEffects *c) { return c ? c->properties.deviceName : "unavailable"; }
extern "C" uint32_t compositor_vulkan_effects_device_type(CompositorVulkanEffects *c) { return c ? c->properties.deviceType : 0; }
