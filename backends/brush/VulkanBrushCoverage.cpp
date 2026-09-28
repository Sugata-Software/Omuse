#include "BrushCoverageValidation.h"
#include "shaders/continuous_brush_spv.h"
#include <vulkan/vulkan.h>
#include <algorithm>
#include <array>
#include <cstring>
#include <memory>
#include <mutex>
#include <stdexcept>
#include <vector>

namespace {
void check(VkResult result) { if (result != VK_SUCCESS) throw std::runtime_error("Vulkan brush operation failed"); }
struct Buffer {
    VkDevice device = VK_NULL_HANDLE;
    VkBuffer buffer = VK_NULL_HANDLE;
    VkDeviceMemory memory = VK_NULL_HANDLE;
    void *mapped = nullptr;
    bool coherent = false;
    ~Buffer() {
        if (mapped) vkUnmapMemory(device,memory);
        if (buffer) vkDestroyBuffer(device,buffer,nullptr);
        if (memory) vkFreeMemory(device,memory,nullptr);
    }
    void create(VkDevice dev, VkPhysicalDevice physical, VkDeviceSize bytes) {
        device = dev;
        VkBufferCreateInfo info{VK_STRUCTURE_TYPE_BUFFER_CREATE_INFO};
        info.size=bytes; info.usage=VK_BUFFER_USAGE_STORAGE_BUFFER_BIT; info.sharingMode=VK_SHARING_MODE_EXCLUSIVE;
        check(vkCreateBuffer(device,&info,nullptr,&buffer));
        VkMemoryRequirements requirements{}; vkGetBufferMemoryRequirements(device,buffer,&requirements);
        VkPhysicalDeviceMemoryProperties properties{}; vkGetPhysicalDeviceMemoryProperties(physical,&properties);
        uint32_t type = UINT32_MAX;
        for (uint32_t i=0;i<properties.memoryTypeCount;++i) {
            const auto flags=properties.memoryTypes[i].propertyFlags;
            if ((requirements.memoryTypeBits&(1u<<i)) && (flags&VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT)) {
                type=i;
                if (flags&VK_MEMORY_PROPERTY_HOST_COHERENT_BIT) break;
            }
        }
        if (type==UINT32_MAX) throw std::runtime_error("No host-visible Vulkan memory");
        coherent=(properties.memoryTypes[type].propertyFlags&VK_MEMORY_PROPERTY_HOST_COHERENT_BIT)!=0;
        VkMemoryAllocateInfo allocation{VK_STRUCTURE_TYPE_MEMORY_ALLOCATE_INFO};
        allocation.allocationSize=requirements.size; allocation.memoryTypeIndex=type;
        check(vkAllocateMemory(device,&allocation,nullptr,&memory));
        check(vkBindBufferMemory(device,buffer,memory,0));
        check(vkMapMemory(device,memory,0,VK_WHOLE_SIZE,0,&mapped));
    }
    void flush() {
        if (coherent) return;
        VkMappedMemoryRange range{VK_STRUCTURE_TYPE_MAPPED_MEMORY_RANGE}; range.memory=memory; range.size=VK_WHOLE_SIZE;
        check(vkFlushMappedMemoryRanges(device,1,&range));
    }
    void invalidate() {
        if (coherent) return;
        VkMappedMemoryRange range{VK_STRUCTURE_TYPE_MAPPED_MEMORY_RANGE}; range.memory=memory; range.size=VK_WHOLE_SIZE;
        check(vkInvalidateMappedMemoryRanges(device,1,&range));
    }
};
}

struct CompositorVulkanBrush {
    std::mutex lock;
    VkInstance instance=VK_NULL_HANDLE;
    VkDevice device=VK_NULL_HANDLE;
    VkQueue queue=VK_NULL_HANDLE;
    VkDescriptorSetLayout descriptorLayout=VK_NULL_HANDLE;
    VkDescriptorPool descriptorPool=VK_NULL_HANDLE;
    VkDescriptorSet descriptorSet=VK_NULL_HANDLE;
    VkPipelineLayout pipelineLayout=VK_NULL_HANDLE;
    VkPipeline pipeline=VK_NULL_HANDLE;
    VkShaderModule shader=VK_NULL_HANDLE;
    VkCommandPool commandPool=VK_NULL_HANDLE;
    VkCommandBuffer command=VK_NULL_HANDLE;
    VkFence fence=VK_NULL_HANDLE;
    VkPhysicalDeviceProperties properties{};
    std::array<std::unique_ptr<Buffer>,3> buffers;
    bool usable=true;
    bool pending=false;

    ~CompositorVulkanBrush() {
        // Every submitted command completed its fence before normal teardown.
        // A timed-out context is quarantined by the public destroy function.
        if (fence) vkDestroyFence(device,fence,nullptr);
        if (commandPool) vkDestroyCommandPool(device,commandPool,nullptr);
        if (pipeline) vkDestroyPipeline(device,pipeline,nullptr);
        if (shader) vkDestroyShaderModule(device,shader,nullptr);
        if (pipelineLayout) vkDestroyPipelineLayout(device,pipelineLayout,nullptr);
        if (descriptorPool) vkDestroyDescriptorPool(device,descriptorPool,nullptr);
        if (descriptorLayout) vkDestroyDescriptorSetLayout(device,descriptorLayout,nullptr);
        for (auto &buffer: buffers) buffer.reset();
        if (device) vkDestroyDevice(device,nullptr);
        if (instance) vkDestroyInstance(instance,nullptr);
    }
    void initialize() {
        static_assert(sizeof(CompositorBrushUniforms)==64,"Push constant layout mismatch");
        static_assert(sizeof(CompositorBrushSegment)==16,"Segment layout mismatch");
        VkApplicationInfo application{VK_STRUCTURE_TYPE_APPLICATION_INFO};
        application.pApplicationName="Compositor brush"; application.apiVersion=VK_API_VERSION_1_0;
        VkInstanceCreateInfo info{VK_STRUCTURE_TYPE_INSTANCE_CREATE_INFO}; info.pApplicationInfo=&application;
        check(vkCreateInstance(&info,nullptr,&instance));
        uint32_t count=0; check(vkEnumeratePhysicalDevices(instance,&count,nullptr));
        std::vector<VkPhysicalDevice> devices(count); check(vkEnumeratePhysicalDevices(instance,&count,devices.data()));
        VkPhysicalDevice chosen=VK_NULL_HANDLE; uint32_t family=0; int best=-1;
        for (auto candidate: devices) {
            VkPhysicalDeviceProperties props{}; vkGetPhysicalDeviceProperties(candidate,&props);
            if (props.limits.maxComputeWorkGroupInvocations<256 || props.limits.maxComputeWorkGroupSize[0]<16 ||
                props.limits.maxComputeWorkGroupSize[1]<16 || props.limits.maxStorageBufferRange<256*256*4) continue;
            uint32_t families=0; vkGetPhysicalDeviceQueueFamilyProperties(candidate,&families,nullptr);
            std::vector<VkQueueFamilyProperties> queues(families); vkGetPhysicalDeviceQueueFamilyProperties(candidate,&families,queues.data());
            const int score=props.deviceType==VK_PHYSICAL_DEVICE_TYPE_DISCRETE_GPU?3:
                props.deviceType==VK_PHYSICAL_DEVICE_TYPE_INTEGRATED_GPU?2:props.deviceType==VK_PHYSICAL_DEVICE_TYPE_CPU?0:1;
            for (uint32_t i=0;i<families;++i) if (queues[i].queueCount && (queues[i].queueFlags&VK_QUEUE_COMPUTE_BIT) && score>best) {
                chosen=candidate; family=i; best=score; properties=props;
            }
        }
        if (!chosen) throw std::runtime_error("No Vulkan compute queue");
        float priority=1;
        VkDeviceQueueCreateInfo queueInfo{VK_STRUCTURE_TYPE_DEVICE_QUEUE_CREATE_INFO};
        queueInfo.queueFamilyIndex=family; queueInfo.queueCount=1; queueInfo.pQueuePriorities=&priority;
        VkDeviceCreateInfo deviceInfo{VK_STRUCTURE_TYPE_DEVICE_CREATE_INFO}; deviceInfo.queueCreateInfoCount=1; deviceInfo.pQueueCreateInfos=&queueInfo;
        check(vkCreateDevice(chosen,&deviceInfo,nullptr,&device)); vkGetDeviceQueue(device,family,0,&queue);
        VkDescriptorSetLayoutBinding bindings[3]{};
        for (uint32_t i=0;i<3;++i) {
            bindings[i].binding=i; bindings[i].descriptorType=VK_DESCRIPTOR_TYPE_STORAGE_BUFFER;
            bindings[i].descriptorCount=1; bindings[i].stageFlags=VK_SHADER_STAGE_COMPUTE_BIT;
        }
        VkDescriptorSetLayoutCreateInfo layoutInfo{VK_STRUCTURE_TYPE_DESCRIPTOR_SET_LAYOUT_CREATE_INFO};
        layoutInfo.bindingCount=3; layoutInfo.pBindings=bindings;
        check(vkCreateDescriptorSetLayout(device,&layoutInfo,nullptr,&descriptorLayout));
        VkPushConstantRange constants{VK_SHADER_STAGE_COMPUTE_BIT,0,64};
        VkPipelineLayoutCreateInfo pipelineInfo{VK_STRUCTURE_TYPE_PIPELINE_LAYOUT_CREATE_INFO};
        pipelineInfo.setLayoutCount=1; pipelineInfo.pSetLayouts=&descriptorLayout;
        pipelineInfo.pushConstantRangeCount=1; pipelineInfo.pPushConstantRanges=&constants;
        check(vkCreatePipelineLayout(device,&pipelineInfo,nullptr,&pipelineLayout));
        VkShaderModuleCreateInfo shaderInfo{VK_STRUCTURE_TYPE_SHADER_MODULE_CREATE_INFO};
        shaderInfo.codeSize=sizeof(compositor_brush_spv); shaderInfo.pCode=compositor_brush_spv;
        check(vkCreateShaderModule(device,&shaderInfo,nullptr,&shader));
        VkComputePipelineCreateInfo compute{VK_STRUCTURE_TYPE_COMPUTE_PIPELINE_CREATE_INFO};
        compute.stage.sType=VK_STRUCTURE_TYPE_PIPELINE_SHADER_STAGE_CREATE_INFO;
        compute.stage.stage=VK_SHADER_STAGE_COMPUTE_BIT; compute.stage.module=shader; compute.stage.pName="main";
        compute.layout=pipelineLayout;
        check(vkCreateComputePipelines(device,VK_NULL_HANDLE,1,&compute,nullptr,&pipeline));
        VkDescriptorPoolSize poolSize{VK_DESCRIPTOR_TYPE_STORAGE_BUFFER,3};
        VkDescriptorPoolCreateInfo poolInfo{VK_STRUCTURE_TYPE_DESCRIPTOR_POOL_CREATE_INFO};
        poolInfo.maxSets=1; poolInfo.poolSizeCount=1; poolInfo.pPoolSizes=&poolSize;
        check(vkCreateDescriptorPool(device,&poolInfo,nullptr,&descriptorPool));
        VkDescriptorSetAllocateInfo allocate{VK_STRUCTURE_TYPE_DESCRIPTOR_SET_ALLOCATE_INFO};
        allocate.descriptorPool=descriptorPool; allocate.descriptorSetCount=1; allocate.pSetLayouts=&descriptorLayout;
        check(vkAllocateDescriptorSets(device,&allocate,&descriptorSet));
        const VkDeviceSize sizes[3]={256*256*4,256*256*4,2048*16};
        VkDescriptorBufferInfo bufferInfo[3]{}; VkWriteDescriptorSet writes[3]{};
        for (uint32_t i=0;i<3;++i) {
            buffers[i]=std::make_unique<Buffer>(); buffers[i]->create(device,chosen,sizes[i]);
            bufferInfo[i]={buffers[i]->buffer,0,sizes[i]};
            writes[i].sType=VK_STRUCTURE_TYPE_WRITE_DESCRIPTOR_SET; writes[i].dstSet=descriptorSet; writes[i].dstBinding=i;
            writes[i].descriptorCount=1; writes[i].descriptorType=VK_DESCRIPTOR_TYPE_STORAGE_BUFFER; writes[i].pBufferInfo=&bufferInfo[i];
        }
        vkUpdateDescriptorSets(device,3,writes,0,nullptr);
        VkCommandPoolCreateInfo commandInfo{VK_STRUCTURE_TYPE_COMMAND_POOL_CREATE_INFO};
        commandInfo.flags=VK_COMMAND_POOL_CREATE_RESET_COMMAND_BUFFER_BIT; commandInfo.queueFamilyIndex=family;
        check(vkCreateCommandPool(device,&commandInfo,nullptr,&commandPool));
        VkCommandBufferAllocateInfo commandAllocate{VK_STRUCTURE_TYPE_COMMAND_BUFFER_ALLOCATE_INFO};
        commandAllocate.commandPool=commandPool; commandAllocate.level=VK_COMMAND_BUFFER_LEVEL_PRIMARY; commandAllocate.commandBufferCount=1;
        check(vkAllocateCommandBuffers(device,&commandAllocate,&command));
        VkFenceCreateInfo fenceInfo{VK_STRUCTURE_TYPE_FENCE_CREATE_INFO}; check(vkCreateFence(device,&fenceInfo,nullptr,&fence));
    }
    void render(const CompositorBrushUniforms &u,const CompositorBrushSegment *segments,
                const float *permanent,size_t pixels,float *next,uint8_t *preview) {
        std::memcpy(buffers[0]->mapped,permanent,pixels*sizeof(float));
        if (u.segment_count) std::memcpy(buffers[2]->mapped,segments,u.segment_count*sizeof(CompositorBrushSegment));
        buffers[0]->flush(); buffers[2]->flush();
        check(vkResetFences(device,1,&fence)); check(vkResetCommandBuffer(command,0));
        VkCommandBufferBeginInfo begin{VK_STRUCTURE_TYPE_COMMAND_BUFFER_BEGIN_INFO}; begin.flags=VK_COMMAND_BUFFER_USAGE_ONE_TIME_SUBMIT_BIT;
        check(vkBeginCommandBuffer(command,&begin));
        VkMemoryBarrier upload{VK_STRUCTURE_TYPE_MEMORY_BARRIER}; upload.srcAccessMask=VK_ACCESS_HOST_WRITE_BIT;
        upload.dstAccessMask=VK_ACCESS_SHADER_READ_BIT|VK_ACCESS_SHADER_WRITE_BIT;
        vkCmdPipelineBarrier(command,VK_PIPELINE_STAGE_HOST_BIT,VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT,0,1,&upload,0,nullptr,0,nullptr);
        vkCmdBindPipeline(command,VK_PIPELINE_BIND_POINT_COMPUTE,pipeline);
        vkCmdBindDescriptorSets(command,VK_PIPELINE_BIND_POINT_COMPUTE,pipelineLayout,0,1,&descriptorSet,0,nullptr);
        vkCmdPushConstants(command,pipelineLayout,VK_SHADER_STAGE_COMPUTE_BIT,0,64,&u);
        vkCmdDispatch(command,(u.width+15)/16,(u.height+15)/16,1);
        VkMemoryBarrier download{VK_STRUCTURE_TYPE_MEMORY_BARRIER}; download.srcAccessMask=VK_ACCESS_SHADER_WRITE_BIT; download.dstAccessMask=VK_ACCESS_HOST_READ_BIT;
        vkCmdPipelineBarrier(command,VK_PIPELINE_STAGE_COMPUTE_SHADER_BIT,VK_PIPELINE_STAGE_HOST_BIT,0,1,&download,0,nullptr,0,nullptr);
        check(vkEndCommandBuffer(command));
        VkSubmitInfo submit{VK_STRUCTURE_TYPE_SUBMIT_INFO}; submit.commandBufferCount=1; submit.pCommandBuffers=&command;
        check(vkQueueSubmit(queue,1,&submit,fence));
        pending=true;
        // Tiles normally take milliseconds. A stalled queue must not freeze the
        // editor forever; the adapter falls back to CPU with unchanged inputs.
        check(vkWaitForFences(device,1,&fence,VK_TRUE,2ULL*1000*1000*1000));
        pending=false;
        buffers[0]->invalidate(); buffers[1]->invalidate();
        // Copy only after fence completion and successful visibility operations.
        std::memcpy(next,buffers[0]->mapped,pixels*sizeof(float));
        auto *values=static_cast<const uint32_t *>(buffers[1]->mapped);
        for (size_t i=0;i<pixels;++i) preview[i]=static_cast<uint8_t>(std::min(values[i],255u));
    }
};
extern "C" CompositorVulkanBrush *compositor_vulkan_brush_create() {
    try { auto context=std::make_unique<CompositorVulkanBrush>(); context->initialize(); return context.release(); }
    catch (...) { return nullptr; }
}
extern "C" void compositor_vulkan_brush_destroy(CompositorVulkanBrush *context) {
    if (!context) return;
    // Freeing buffers still in use by a timed-out GPU is unsafe. Keep this one
    // disabled context until process exit if its fence has not completed. There
    // is no unbounded vkDeviceWaitIdle on the UI or shutdown path.
    if (context->pending && vkGetFenceStatus(context->device,context->fence)!=VK_SUCCESS) return;
    delete context;
}
extern "C" int compositor_vulkan_brush_render(CompositorVulkanBrush *context,const CompositorBrushUniforms *u,
    const CompositorBrushSegment *segments,size_t count,const float *permanent,size_t pixels,float *next,uint8_t *preview) {
    if (!validBrush(u,segments,count,permanent,pixels,next,preview)) return -1;
    if (!context) return -2;
    std::lock_guard<std::mutex> guard(context->lock);
    if (!context->usable) return -2;
    try { context->render(*u,segments,permanent,pixels,next,preview); return 0; }
    catch (...) { context->usable=false; return -2; }
}
extern "C" const char *compositor_vulkan_brush_device(CompositorVulkanBrush *c) { return c?c->properties.deviceName:"unavailable"; }
extern "C" uint32_t compositor_vulkan_brush_device_type(CompositorVulkanBrush *c) { return c?c->properties.deviceType:0; }
extern "C" uint32_t compositor_vulkan_brush_driver_version(CompositorVulkanBrush *c) { return c?c->properties.driverVersion:0; }
