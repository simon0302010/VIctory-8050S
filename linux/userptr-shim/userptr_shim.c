/*
 * victory userptr shim: lets RADV import file-backed host memory.
 *
 * Under Wine every section (CreateFileMapping) is a shared mapping of a
 * file (wineserver's anonymous mapping file). vkd3d-proton opens title
 * memory as a GPU heap with VK_EXT_external_memory_host, which RADV turns
 * into an amdgpu userptr through libdrm's amdgpu_create_bo_from_user_mem.
 * libdrm always asks for AMDGPU_GEM_USERPTR_ANONONLY, and the kernel then
 * refuses any range backed by a file. vkd3d-proton falls back to a fresh
 * allocation of its own and reports success: the GPU reads other memory.
 *
 * This shim, loaded with LD_PRELOAD, drops ANONONLY from the userptr
 * request and nothing else. The kernel's HMM path then pins the shared
 * pages themselves, so the GPU sees the bytes every view of the section
 * shows.
 *
 * Caveat: a userptr BO follows the virtual range it was made on. The range
 * must stay mapped for as long as the heap is used (vkd3d-proton's
 * OpenExistingHeapFromFileMapping unmaps its view at once: do not use it).
 *
 * Build: cc -O2 -shared -fPIC -o libvictory_userptr.so userptr_shim.c -ldl
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define DRM_AMDGPU_GEM_USERPTR 0x11
#define AMDGPU_GEM_USERPTR_ANONONLY (1u << 1)
#define AMDGPU_GEM_USERPTR_REGISTER (1u << 3)

/* include/uapi/drm/amdgpu_drm.h */
struct drm_amdgpu_gem_userptr {
    uint64_t addr;
    uint64_t size;
    uint32_t flags;
    uint32_t handle;
};

typedef int (*command_write_read)(int, unsigned long, void *, unsigned long);

static int verbose(void)
{
    static int level = -1;
    if (level < 0) {
        const char *v = getenv("VICTORY_USERPTR_SHIM_LOG");
        level = v && *v && strcmp(v, "0") != 0;
    }
    return level;
}

/* The real drmCommandWriteRead. libdrm comes in with the Vulkan driver,
 * which the loader opens RTLD_LOCAL: RTLD_NEXT does not see it, so it is
 * looked up in libdrm.so.2 itself (already loaded by the caller). */
static command_write_read real(void)
{
    static command_write_read next;
    if (next)
        return next;
    next = (command_write_read)dlsym(RTLD_NEXT, "drmCommandWriteRead");
    if (!next) {
        void *libdrm = dlopen("libdrm.so.2", RTLD_NOW | RTLD_NOLOAD);
        if (!libdrm)
            libdrm = dlopen("libdrm.so.2", RTLD_NOW | RTLD_LOCAL);
        if (libdrm)
            next = (command_write_read)dlsym(libdrm, "drmCommandWriteRead");
    }
    if (!next)
        fprintf(stderr, "victory userptr shim: no drmCommandWriteRead: %s\n", dlerror());
    return next;
}

int drmCommandWriteRead(int fd, unsigned long index, void *data, unsigned long size)
{
    command_write_read next = real();
    if (!next)
        return -1;
    if (index == DRM_AMDGPU_GEM_USERPTR && size == sizeof(struct drm_amdgpu_gem_userptr) && data) {
        struct drm_amdgpu_gem_userptr *args = data;
        /* Only RADV's host imports (registered userptrs) are touched. */
        if ((args->flags & AMDGPU_GEM_USERPTR_REGISTER) && (args->flags & AMDGPU_GEM_USERPTR_ANONONLY)) {
            args->flags &= ~AMDGPU_GEM_USERPTR_ANONONLY;
            int r = next(fd, index, data, size);
            if (verbose())
                fprintf(stderr, "victory userptr shim: %#lx+%#lx -> %d\n",
                        (unsigned long)args->addr, (unsigned long)args->size, r);
            return r;
        }
    }
    return next(fd, index, data, size);
}
