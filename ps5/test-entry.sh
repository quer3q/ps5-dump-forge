#!/usr/bin/env bash
# Host check of entry.c (launch modes, args file, log redirection, the old folder's rename) with
# the host cc against stubs: __real_main prints its arguments (LONGNOTE: a 4,000-byte ps5_notify,
# FAIL: returns 3); the notification call appends to $NOTES.
# ponytail: dup2 failures aren't injected (that needs symbol interposition); a closed fd 1/2 is.
set -euo pipefail
cd "$(dirname "$0")"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cat > "$tmp/stub.c" <<'EOF'
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
struct notification { char reserved[45]; char message[3075]; };
int __wrap_main(int, char **);
int ps5_notify(const char *);
int __real_main(int argc, char **argv) {
    printf("argc=%d ", argc);
    for (int i = 1; i < argc; i++) printf("[%s]", argv[i]);
    printf("\n");
    fprintf(stderr, "stderr-reached\n");
    if (getenv("LONGNOTE")) {
        static char note[4001];
        memset(note, 'n', 4000);
        ps5_notify(note);
    }
    return getenv("FAIL") ? 3 : 0;
}
int sceKernelSendNotificationRequest(int a, struct notification *n, size_t s, int b) {
    (void)a; (void)s; (void)b;
    FILE *f = fopen(getenv("NOTES"), "a");
    if (f) { fprintf(f, "notify[%zu]: %s\n", strnlen(n->message, sizeof(n->message)), n->message); fclose(f); }
    return 0;
}
int main(int argc, char **argv) { return __wrap_main(argc, argv); }
EOF
cc -Wall -Wextra -Werror -DFORGE_DIR="\"$tmp/forge\"" -DOLD_FORGE_DIR="\"$tmp/ro/old\"" -DFORGE_LINK="\"../forge\"" entry.c "$tmp/stub.c" -o "$tmp/entry"

closed= old=
# run <args-file bytes or ""> <exit> <log text or ""> [notification]. An args file must end up as
# args.done (old=1 leaves an older args.done to replace).
run() {
  rm -rf "$tmp/forge" "$tmp/notes"
  mkdir "$tmp/forge"
  if [[ -n "$old" ]]; then echo stale > "$tmp/forge/args.done"; fi
  if [[ -n "$1" ]]; then printf "$1" > "$tmp/forge/args"; printf "$1" > "$tmp/expected"; fi
  set +e
  if [[ -n "$closed" ]]; then NOTES="$tmp/notes" "$tmp/entry" >&- 2>&-; else NOTES="$tmp/notes" "$tmp/entry"; fi
  code=$?; set -e
  log="$(cat "$tmp"/forge/{log,serve}-*.txt 2>/dev/null || true)"
  notes="$(cat "$tmp/notes" 2>/dev/null || true)"
  moved=1
  if [[ -n "$1" ]]; then
    [[ ! -e "$tmp/forge/args" ]] && cmp -s "$tmp/expected" "$tmp/forge/args.done" || moved=
  fi
  if [[ $code != "$2" || "$log" != *"$3"* || "$notes" != *"${4:-}"* || -z $moved ]] ||
     [[ -z "$3" && -n "$log" ]] || [[ $2 == 0 && "$log" != *stderr-reached* ]]; then
    echo "FAIL: args $(printf %q "$1"): exit $code"; echo "log: $log"; echo "notes: $notes"
    ls -l "$tmp/forge"; exit 1
  fi
}
run 'inspect\n/mnt/usb0/My Game\n--json\n' 0 'argc=4 [inspect][/mnt/usb0/My Game][--json]' 'started'
ls "$tmp"/forge/log-*.txt > /dev/null || { echo "FAIL: args output not in log-<pid>.txt"; exit 1; }
old=1; run 'convert\r\n\r\n/mnt/usb0/G\r\n--to\r\nffpkg' 0 'argc=5 [convert][/mnt/usb0/G][--to][ffpkg]'; old=
closed=1; run 'inspect\nx\n' 0 'argc=3 [inspect][x]'; closed=  # starts with fd 1 and 2 closed
run 'a\0b\n' 2 '' 'holds a NUL'
run "$(printf 'x\\n%.0s' {1..63})" 2 '' 'over 62 arguments'
run "$(printf 'x\\n%.0s' {1..62})" 0 "argc=63 $(printf '[x]%.0s' {1..62})"$'\n'
run "$(head -c 16384 /dev/zero | tr '\0' a)" 2 '' 'over 16 KiB'

# No args file: serve, logged to serve-<pid>.txt, no notification of its own, no args.done.
run '' 0 'argc=2 [serve]'
ls "$tmp"/forge/serve-*.txt > /dev/null && [[ ! -e "$tmp/forge/args.done" && -z "$notes" ]] ||
  { echo "FAIL: serve mode: $(ls "$tmp/forge") notes: $notes"; exit 1; }
FAIL=1 run '' 3 'argc=2 [serve]' 'failed; see the log'
# ps5_notify copies at most 3,074 bytes and NUL-terminates.
LONGNOTE=1 run '' 0 'argc=2 [serve]' "notify[3074]: nnnn"
# An args.done that can't be replaced (a non-empty folder) stops before anything runs.
rm -rf "$tmp/forge"; mkdir -p "$tmp/forge/args.done/x"; echo inspect > "$tmp/forge/args"
set +e; NOTES="$tmp/notes" "$tmp/entry" > "$tmp/out"; code=$?; set -e
[[ $code == 1 && ! -s "$tmp/out" && "$(cat "$tmp/notes")" == *"cannot rename"* ]] ||
  { echo "FAIL: rename refused: exit $code"; exit 1; }

# elfldr's args=serve (the web launcher starting the saved copy): the same serve launch, the
# folder created, an args file left as it is.
# launch <args...>: a fresh folder holding an args file; sets $code, $out, $log, $notes.
launch() {
  rm -rf "$tmp/forge" "$tmp/notes"
  mkdir "$tmp/forge"
  printf 'inspect\nx\n' > "$tmp/forge/args"
  set +e; NOTES="$tmp/notes" "$tmp/entry" "$@" > "$tmp/out" 2> "$tmp/err"; code=$?; set -e
  out="$(cat "$tmp/out")"
  log="$(cat "$tmp"/forge/serve-*.txt 2>/dev/null || true)"
  notes="$(cat "$tmp/notes" 2>/dev/null || true)"
  [[ "$(cat "$tmp/forge/args")" == $'inspect\nx' && ! -e "$tmp/forge/args.done" ]] ||
    { echo "FAIL: $*: the args file was touched: $(ls "$tmp/forge")"; exit 1; }
}
launch serve
[[ $code == 0 && -z "$out" && -z "$notes" && "$log" == *'argc=2 [serve]'* && "$log" == *stderr-reached* ]] ||
  { echo "FAIL: argv serve: exit $code out: $out log: $log notes: $notes"; exit 1; }
FAIL=1 launch serve --port 9000
[[ $code == 3 && "$log" == *'argc=4 [serve][--port][9000]'* && "$notes" == *'failed; see the log'* ]] ||
  { echo "FAIL: argv serve failing: exit $code log: $log notes: $notes"; exit 1; }
rm -rf "$tmp/forge"
set +e; NOTES="$tmp/notes" "$tmp/entry" serve > /dev/null; code=$?; set -e
[[ $code == 0 ]] && ls "$tmp"/forge/serve-*.txt > /dev/null ||
  { echo "FAIL: argv serve without the folder: exit $code"; exit 1; }
# Other arguments: the plain CLI on the caller's stdout, no log, the args file left as it is.
launch inspect x
[[ $code == 0 && "$out" == 'argc=3 [inspect][x]' && -z "$log" && -z "$notes" ]] ||
  { echo "FAIL: plain CLI: exit $code out: $out log: $log"; exit 1; }
# Earlier runs' logs: gone processes' logs deleted except the newest; a live process's log, other
# names and our own new log stay.
dead() { sh -c 'echo $$'; }  # the pid of a process that has exited
rm -rf "$tmp/forge"; mkdir "$tmp/forge"
d1=$(dead); d2=$(dead); d3=$(dead)
touch -t 202601010000 "$tmp/forge/serve-$d1.txt"
touch -t 202601020000 "$tmp/forge/log-$d2.txt"
touch -t 202601030000 "$tmp/forge/serve-$d3.txt"
touch "$tmp/forge/serve-$$.txt" "$tmp/forge/serve-abc.txt" "$tmp/forge/serve-$d1.txt.bak"
set +e; NOTES="$tmp/notes" "$tmp/entry" serve > /dev/null; code=$?; set -e
left="$(cd "$tmp/forge" && ls | sort | tr '\n' ' ')"
[[ $code == 0 && ! -e "$tmp/forge/serve-$d1.txt" && ! -e "$tmp/forge/log-$d2.txt" &&
   -e "$tmp/forge/serve-$d3.txt" && -e "$tmp/forge/serve-$$.txt" && -e "$tmp/forge/serve-abc.txt" &&
   -e "$tmp/forge/serve-$d1.txt.bak" && $(ls "$tmp"/forge/serve-*.txt | wc -l) -eq 4 ]] ||
  { echo "FAIL: log pruning: exit $code, left: $left"; exit 1; }
# Earlier versions' folder: renamed to the new one when that is missing, its files kept, and a
# link to the new one left in its place (what an old cached launcher page asks for); left alone
# when both exist; a rename that fails is notified and the launch goes on.
rm -rf "$tmp/forge" "$tmp/ro" "$tmp/notes"; mkdir -p "$tmp/ro/old"; echo elf > "$tmp/ro/old/ps5-dump-forge.elf"
set +e; NOTES="$tmp/notes" "$tmp/entry" serve > /dev/null; code=$?; set -e
[[ $code == 0 && -d "$tmp/forge" && ! -L "$tmp/forge" && -L "$tmp/ro/old" && "$(readlink "$tmp/ro/old")" == ../forge &&
   "$(cat "$tmp/forge/ps5-dump-forge.elf")" == elf && "$(cat "$tmp/ro/old/ps5-dump-forge.elf")" == elf && ! -e "$tmp/notes" ]] &&
  ls "$tmp"/forge/serve-*.txt > /dev/null || { echo "FAIL: old folder not renamed and linked: exit $code"; exit 1; }
# The link again on the next launch: left as it is, nothing renamed back.
set +e; NOTES="$tmp/notes" "$tmp/entry" serve > /dev/null; code=$?; set -e
[[ $code == 0 && -L "$tmp/ro/old" && -d "$tmp/forge" && ! -L "$tmp/forge" && ! -e "$tmp/notes" ]] ||
  { echo "FAIL: second launch with the link: exit $code"; exit 1; }
# Migrated by a build without the link (new folder, nothing at the old path): the link added.
rm "$tmp/ro/old"
set +e; NOTES="$tmp/notes" "$tmp/entry" > /dev/null; code=$?; set -e
[[ $code == 0 && "$(readlink "$tmp/ro/old")" == ../forge && ! -e "$tmp/notes" ]] ||
  { echo "FAIL: link not added: exit $code"; exit 1; }
rm "$tmp/ro/old"; mkdir "$tmp/ro/old"; echo old > "$tmp/ro/old/ps5-dump-forge.elf"
set +e; NOTES="$tmp/notes" "$tmp/entry" > /dev/null; code=$?; set -e
[[ $code == 0 && ! -L "$tmp/ro/old" && "$(cat "$tmp/ro/old/ps5-dump-forge.elf")" == old && "$(cat "$tmp/forge/ps5-dump-forge.elf")" == elf ]] ||
  { echo "FAIL: both folders: exit $code"; exit 1; }
if [[ $(id -u) != 0 ]]; then  # root ignores the read-only parent
  rm -rf "$tmp/forge"; chmod 555 "$tmp/ro"
  set +e; NOTES="$tmp/notes" "$tmp/entry" serve > /dev/null; code=$?; set -e
  chmod 755 "$tmp/ro"
  [[ $code == 0 && -d "$tmp/ro/old" && ! -L "$tmp/ro/old" && "$(cat "$tmp/notes")" == *"cannot move $tmp/ro/old to $tmp/forge"* ]] &&
    ls "$tmp"/forge/serve-*.txt > /dev/null || { echo "FAIL: rename refused: exit $code"; exit 1; }
fi
echo "entry.c launch modes, args parsing and the old folder: ok"
