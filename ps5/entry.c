/* Payload entry, linked in front of Rust's main with -Wl,--wrap=main. elfldr starts a payload
 * with no arguments and no terminal, so with none it runs "serve" (the web UI), its stdout and
 * stderr in /data/ps5-dump-forge/serve-<pid>.txt. The web launcher starts the saved copy with
 * elfldr's args=serve: "serve" as the first argument is that same launch (never the args file).
 * A developer hook: an args file there (one argument per line, the command first: "convert",
 * "/mnt/usb0/GAME", "--to", "ffpkg") runs once instead of a no-argument serve, renamed to
 * args.done before it runs, its output in log-<pid>.txt. Each launch that logs first deletes
 * the logs of earlier runs, keeping the newest one. Run with other arguments, it is the plain CLI.
 */
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <signal.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#ifndef FORGE_DIR // ps5/test-entry.sh points it at a temporary folder
#define FORGE_DIR "/data/ps5-dump-forge"
#endif

extern int __real_main(int argc, char **argv);

struct notification {
    char reserved[45];
    char message[3075];
};
_Static_assert(sizeof(struct notification) == 3120, "SDK notification ABI");
int sceKernelSendNotificationRequest(int, struct notification *, size_t, int);

// Exported for the Rust side (the server's URL). Copies at most 3,074 bytes,
// never reading past them, and always NUL-terminates.
int ps5_notify(const char *message) {
    struct notification request = {0};
    memcpy(request.message, message, strnlen(message, sizeof(request.message) - 1));
    return sceKernelSendNotificationRequest(0, &request, sizeof(request), 0);
}

// The pid in a "serve-<pid>.txt" or "log-<pid>.txt" whose process is gone, else 0.
static long dead_log(const char *name) {
    const char *digits = strncmp(name, "serve-", 6) == 0 ? name + 6
                         : strncmp(name, "log-", 4) == 0 ? name + 4
                                                         : NULL;
    if (!digits || *digits < '1' || *digits > '9') return 0;
    char *end;
    long pid = strtol(digits, &end, 10);
    if (strcmp(end, ".txt") != 0 || pid == (long)getpid()) return 0;
    // EPERM: alive, only not ours to signal.
    return kill((pid_t)pid, 0) != 0 && errno == ESRCH ? pid : 0;
}

// Logs of earlier runs: those whose process is gone are deleted, except the newest (the run
// before this one, for a crash's last lines). A running Forge's log is kept (a second launch
// only answers "already running"). Best effort: a log that can't go stays.
static void prune_logs(void) {
    DIR *dir = opendir(FORGE_DIR);
    if (!dir) return;
    char path[512], newest[256] = "";
    time_t newest_time = 0;
    struct dirent *e;
    struct stat st;
    while ((e = readdir(dir)))
        if (dead_log(e->d_name)) {
            snprintf(path, sizeof(path), FORGE_DIR "/%s", e->d_name);
            if (lstat(path, &st) == 0 && (!*newest || st.st_mtime > newest_time)) {
                newest_time = st.st_mtime;
                snprintf(newest, sizeof(newest), "%s", e->d_name);
            }
        }
    rewinddir(dir);
    while ((e = readdir(dir)))
        if (dead_log(e->d_name) && strcmp(e->d_name, newest) != 0) {
            snprintf(path, sizeof(path), FORGE_DIR "/%s", e->d_name);
            unlink(path);
        }
    closedir(dir);
}

// stdout and stderr to FORGE_DIR/<name>-<pid>.txt, after pruning earlier runs' logs.
static int redirect(const char *name) {
    prune_logs();
    char log[128];
    snprintf(log, sizeof(log), FORGE_DIR "/%s-%ld.txt", name, (long)getpid());
    int fd = open(log, O_WRONLY | O_CREAT | O_TRUNC | O_NOFOLLOW, 0666);
    if (fd < 0 || dup2(fd, STDOUT_FILENO) < 0 || dup2(fd, STDERR_FILENO) < 0) {
        ps5_notify("PS5 Dump Forge: cannot write its log in " FORGE_DIR);
        return -1;
    }
    if (fd > STDERR_FILENO) close(fd);
    setvbuf(stdout, NULL, _IOLBF, 0);
    return 0;
}

// The server, logged to serve-<pid>.txt. It notifies its URL itself and exits from inside on
// quit, so only a failure comes back here.
static int serve(int argc, char **argv) {
    if (redirect("serve") != 0) return 1;
    int result = __real_main(argc, argv);
    fflush(NULL);
    if (result != 0) ps5_notify("PS5 Dump Forge failed; see the log in " FORGE_DIR);
    return result;
}

int __wrap_main(int argc, char **argv) {
    int serving = argc > 1 && strcmp(argv[1], "serve") == 0;
    if (argc > 1 && !serving) return __real_main(argc, argv);
    if (mkdir(FORGE_DIR, 0777) != 0 && errno != EEXIST) {
        ps5_notify("PS5 Dump Forge: cannot create " FORGE_DIR);
        return 1;
    }
    if (serving) return serve(argc, argv);
    // One-shot: renamed (replacing an older args.done) before anything runs, so an autoloaded
    // boot never replays it, even one that fails to parse.
    if (rename(FORGE_DIR "/args", FORGE_DIR "/args.done") != 0) {
        if (errno != ENOENT) {
            ps5_notify("PS5 Dump Forge: cannot rename " FORGE_DIR "/args to args.done");
            return 1;
        }
        // No args file: the normal launch.
        static char *serve_args[] = {"ps5-dump-forge", "serve", NULL};
        return serve(2, serve_args);
    }
    // ponytail: one fixed buffer and 63 arguments; the CLI never needs more than ten. Anything
    // that doesn't fit is refused, never cut short into a different command.
    static char text[16384];
    static char *args[64];
    FILE *file = fopen(FORGE_DIR "/args.done", "r");
    if (!file) {
        ps5_notify("PS5 Dump Forge: cannot read " FORGE_DIR "/args.done");
        return 2;
    }
    size_t len = fread(text, 1, sizeof(text), file);
    int failed = ferror(file);
    fclose(file);
    if (failed || len == sizeof(text) || memchr(text, 0, len)) {
        ps5_notify("PS5 Dump Forge: " FORGE_DIR "/args.done is unreadable, over 16 KiB or holds a NUL");
        return 2;
    }
    text[len] = 0;
    // One argument per line; a trailing CR is dropped and blank lines are skipped (an empty
    // argument means nothing to the CLI).
    int n = 0;
    args[n++] = "ps5-dump-forge";
    for (char *line = text; *line;) {
        char *end = strchr(line, '\n');
        char *next = end ? end + 1 : line + strlen(line);
        if (end) *end = 0;
        size_t l = strlen(line);
        if (l && line[l - 1] == '\r') line[--l] = 0;
        if (l) {
            if (n == 63) {
                ps5_notify("PS5 Dump Forge: " FORGE_DIR "/args.done has over 62 arguments");
                return 2;
            }
            args[n++] = line;
        }
        line = next;
    }
    args[n] = NULL;

    if (redirect("log") != 0) return 1;
    ps5_notify("PS5 Dump Forge started; log in " FORGE_DIR);
    int result = __real_main(n, args);
    fflush(NULL);
    ps5_notify(result == 0 ? "PS5 Dump Forge: done" : "PS5 Dump Forge failed; see the log in " FORGE_DIR);
    return result;
}
