/* PS5 adapters outside shim/ (kept byte-identical to upstream). libc's FreeBSD 11 ABI asks for
 * statfs@FBSD_1.0 and fstatfs@FBSD_1.0; the SDK exports them unversioned with the same struct
 * (MNAMELEN 88), so forwarding is enough. */
#include <sys/param.h>
#include <sys/mount.h>

int ps5_statfs(const char *path, struct statfs *buffer) {
    return statfs(path, buffer);
}
int ps5_fstatfs(int fd, struct statfs *buffer) {
    return fstatfs(fd, buffer);
}
__asm__(".symver ps5_statfs,statfs@FBSD_1.0");
__asm__(".symver ps5_fstatfs,fstatfs@FBSD_1.0");

/* std's available_parallelism asks the PID's cpuset with a 256-bit set; the PS5 refuses that
 * (ERANGE) and std saw 16 CPUs where a payload runs on 5 (mask 0xea00): every pool 3x
 * oversubscribed (Kraken 9 MB/s at 15 threads, 43 at 4). The thread query with the kernel's
 * 128-bit set works, and a thread starts with its process's mask. */
#include <string.h>
#include <sys/types.h>
int __real_cpuset_getaffinity(int level, int which, id_t id, size_t size, void *mask);
int __wrap_cpuset_getaffinity(int level, int which, id_t id, size_t size, void *mask) {
    enum { WHICH_TID = 1, WHICH_PID = 2, KERNEL_SET = 16 };
    if (which == WHICH_PID && id == (id_t)-1 && size >= KERNEL_SET) {
        memset(mask, 0, size);
        return __real_cpuset_getaffinity(level, WHICH_TID, -1, KERNEL_SET, mask);
    }
    return __real_cpuset_getaffinity(level, which, id, size, mask);
}
