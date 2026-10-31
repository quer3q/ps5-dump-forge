/* Hands a package to the PS5's installer: sceAppInstUtilInstallByPackage with an http:// URL that
 * Forge serves on 127.0.0.1 (crates/ps5-dump-forge-core/src/install.rs). Sony accepts or refuses at
 * once, then fetches the package over HTTP Range requests on its own threads. Behaviour after
 * ps5upload's installer (payload/installer/sony.c, include/sceAppInstUtil.h, authid.h); our own code.
 * Returns 0 when Sony accepts the package; else -1 with its error in *code. Invalid URLs are
 * rejected locally with SCE kernel EINVAL. Logs to stdout (the serve log).
 */
#include <ps5/kernel.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

/* BGFT's gate for an http:// install and for the library's initialization: the SYSTEM authid. */
#define SYSTEM_INSTALL_AUTHID 0x4801000000000013ULL
/* ShellCore's: what InstallByPackage wants on FW < 11, where the SYSTEM one gets 0x80B22404 (Sony's
 * pre-flight of the URL refused, no byte fetched; ps5upload's authid.h, etaHEN's pkg write-up). */
#define SHELLCORE_AUTHID 0x3800000000000010ULL
#define REFUSED_PREFLIGHT 0x80B22404u
#define MAX_INSTALL_URL 127
/* SCE kernel EINVAL, used only if the caller violates the URL limit. */
#define BAD_INSTALL_URL 0x80020016u

/* The published layout is six pointers, but Sony reads the 8 bytes after icon_url: nonzero there
 * sends a plain install down the patch path (0x80B2116F). The whole struct is zeroed first. */
typedef struct {
    const char *uri;
    const char *ex_uri;
    const char *playgo_scenario_id;
    const char *content_id;
    const char *content_name;
    const char *icon_url;
    uint64_t after_icon_url; /* 0: a plain install */
    uint64_t reserved[7];
} meta_info;
_Static_assert(sizeof(meta_info) == 0x70, "meta_info is 0x70 bytes");

typedef struct {
    char content_id[0x30];
    int content_type;
    int content_platform;
} pkg_info;

typedef struct {
    char languages[30][8];
    char playgo_scenario_ids[64][3];
    char content_ids[64][0x30];
    unsigned char tail[6480];
} playgo_info;

int sceAppInstUtilInitialize(void);
int sceAppInstUtilInstallByPackage(meta_info *meta, pkg_info *pkg, playgo_info *playgo);

static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static int initialized;

/* Every sceAppInstUtil user (an install here, launcher.c's tile registration) runs between
 * these, one at a time, so launcher.c's Terminate never lands under an install. Returns
 * whether an install has initialized the library (kept, never terminated: then the caller
 * neither initializes nor terminates it). */
int forge_appinst_begin(void) {
    pthread_mutex_lock(&lock);
    return initialized;
}

void forge_appinst_end(void) { pthread_mutex_unlock(&lock); }

/* Static, not on the thread's stack: PlayGoInfo is about 10 KiB. Used under `lock`. */
static meta_info meta;
static pkg_info pkg;
static playgo_info playgo;

/* This process's credentials as the installer needs them, and what they were before. */
typedef struct {
    uint64_t authid;
    uint8_t caps[16];
    uint8_t attrs[32];
    int have_caps, have_attrs;
} creds;

/* Takes on ps5upload's installer credentials (its escalate.c): the SYSTEM authid, every
 * capability, attrs 0x80. With the authid alone Sony refused at once (0x80B22404, on the
 * user's console). Returns what to restore. */
static creds creds_acquire(void) {
    pid_t pid = getpid();
    creds saved;
    memset(&saved, 0, sizeof(saved));
    saved.authid = kernel_get_ucred_authid(pid);
    saved.have_caps = kernel_get_ucred_caps(pid, saved.caps) == 0;
    saved.have_attrs = kernel_get_ucred_attrs(pid, saved.attrs) == 0;
    if (kernel_set_ucred_authid(pid, SYSTEM_INSTALL_AUTHID) != 0)
        printf("ps5-dump-forge serve: install: can't swap the authid\n");
    uint8_t caps[16];
    memset(caps, 0xff, sizeof(caps));
    if (kernel_set_ucred_caps(pid, caps) != 0)
        printf("ps5-dump-forge serve: install: can't raise the capabilities\n");
    uint8_t attrs[32];
    memset(attrs, 0, sizeof(attrs));
    attrs[0] = 0x80;
    if (kernel_set_ucred_attrs(pid, attrs) != 0)
        printf("ps5-dump-forge serve: install: can't set the attrs\n");
    return saved;
}

static void creds_release(const creds *saved) {
    pid_t pid = getpid();
    if (saved->have_caps) (void)kernel_set_ucred_caps(pid, saved->caps);
    if (saved->have_attrs) (void)kernel_set_ucred_attrs(pid, saved->attrs);
    if (saved->authid == 0) return;
    for (int i = 0; i < 3; i++) {
        (void)kernel_set_ucred_authid(pid, saved->authid);
        if (kernel_get_ucred_authid(pid) == saved->authid) return;
    }
    printf("ps5-dump-forge serve: install: can't restore the authid 0x%llx\n",
           (unsigned long long)saved->authid);
}

int forge_pkg_install(const char *url, const char *name, unsigned *code) {
    *code = 0;
    if (!url || !*url || strlen(url) > MAX_INSTALL_URL) {
        *code = BAD_INSTALL_URL;
        return -1;
    }
    pthread_mutex_lock(&lock);
    creds saved = creds_acquire();
    /* ponytail: initialized once and never terminated, as ps5upload does (after the
     * credentials: Initialize runs as SYSTEM). Only a success is kept: a failed Initialize
     * is tried again by the next install. */
    int rc = 0;
    if (!initialized) {
        rc = sceAppInstUtilInitialize();
        if (rc == 0)
            initialized = 1;
        else
            printf("ps5-dump-forge serve: install: sceAppInstUtilInitialize: 0x%08x\n",
                   (unsigned)rc);
    }
    if (rc == 0) {
        memset(&meta, 0, sizeof(meta));
        memset(&pkg, 0, sizeof(pkg));
        memset(&playgo, 0, sizeof(playgo));
        meta.uri = url;
        meta.ex_uri = "";
        meta.playgo_scenario_id = "";
        meta.content_id = "";
        meta.content_name = name ? name : "";
        meta.icon_url = "";
        rc = sceAppInstUtilInstallByPackage(&meta, &pkg, &playgo);
        printf("ps5-dump-forge serve: install: InstallByPackage %s: 0x%08x\n", url, (unsigned)rc);
        /* ponytail: asked again as ShellCore rather than reading the firmware version first. */
        if ((unsigned)rc == REFUSED_PREFLIGHT &&
            kernel_set_ucred_authid(getpid(), SHELLCORE_AUTHID) == 0) {
            rc = sceAppInstUtilInstallByPackage(&meta, &pkg, &playgo);
            printf("ps5-dump-forge serve: install: as ShellCore: 0x%08x\n", (unsigned)rc);
        }
    }
    creds_release(&saved);
    pthread_mutex_unlock(&lock);
    fflush(stdout);
    if (rc == 0) return 0;
    *code = (unsigned)rc;
    return -1;
}
