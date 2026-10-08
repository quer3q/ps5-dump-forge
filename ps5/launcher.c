/* Registers the home-screen tile whose files the Rust side wrote under /user/app/<title_id>
 * (crates/ps5-dump-forge-server/src/tile.rs). Adapted from ps5-ai-cli's launcher/platform_ps5.c
 * (GPL-3.0-or-later, itself after saawant12/orbit-store-ps5-source): the title-directory install
 * by NID, found through the kernel's module table or else the SDK's runtime linker, or failing
 * both the broader AppInstallAll scan. Logs the method and its return code to stdout (the serve
 * log); returns that code (0 on success).
 */
#include <dlfcn.h>
#include <ps5/kernel.h>
#include <stdint.h>
#include <stdio.h>

int sceAppInstUtilInitialize(void);
int sceAppInstUtilTerminate(void);
int sceAppInstUtilAppInstallAll(void *);
typedef int (*install_title_fn)(const char *title_id, const char *dir, void *reserved);

int ps5_register_title(const char *title_id) {
    uint32_t handle = 0;
    install_title_fn install_title = NULL;
    // -1: this process. Wudg3Xe3heE is sceAppInstUtilAppInstallTitleDir.
    if (kernel_dynlib_handle(-1, "libSceAppInstUtil.sprx", &handle) == 0)
        install_title = (install_title_fn)kernel_dynlib_resolve(-1, handle, "Wudg3Xe3heE");
    if (!install_title) install_title = (install_title_fn)dlsym(RTLD_DEFAULT, "Wudg3Xe3heE");
    // ponytail: the sceAppInstUtilAppInstallAll fallback (the path Payload Manager and
    // ps5-ai-cli use) scans all of /user/app, not just our title; it only runs when the
    // title-specific function can't be resolved.
    const char *method = install_title ? "AppInstallTitleDir" : "AppInstallAll";

    int result = sceAppInstUtilInitialize();
    if (result != 0) {
        printf("ps5-dump-forge serve: home-screen tile %s: sceAppInstUtilInitialize: 0x%x\n",
               title_id, (unsigned)result);
    } else {
        result = install_title ? install_title(title_id, "/user/app/", NULL)
                               : sceAppInstUtilAppInstallAll(NULL);
        printf("ps5-dump-forge serve: home-screen tile %s: %s: 0x%x\n", title_id, method,
               (unsigned)result);
        sceAppInstUtilTerminate();
    }
    fflush(stdout);
    return result;
}
