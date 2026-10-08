/* PS5-only adapters outside shim/ (which stays byte-identical to upstream). libc's FreeBSD 11 ABI
 * (--cfg libc_unstable_freebsd_version="11") asks for statfs@FBSD_1.0 and fstatfs@FBSD_1.0; the SDK
 * exports them unversioned, with the same FreeBSD 11 struct statfs (MNAMELEN 88), so forwarding is
 * enough, as shim/freebsd11.c does for stat. Plain fstatfs is the shim's --wrap=fstatfs (raw
 * syscall 397, the FreeBSD 11 layout).
 */
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
