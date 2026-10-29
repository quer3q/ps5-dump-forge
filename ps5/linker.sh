#!/usr/bin/env bash
# rustc's linker for the PS5 target (after ps5-ai-cli's tools/ps5-rust-linker.sh). FreeBSD splits
# these symbols into separate libraries; the PS5 SDK has them in libc and Sony's modules. A missing
# function still fails the link: no unresolved-symbol suppression.
set -euo pipefail
args=()
for arg in "$@"; do
  case "$arg" in
    -lm|-lrt|-lutil|-lexecinfo|-lkvm|-lmemstat|-lprocstat|-ldevstat) args+=(-lc) ;;
    -lgcc_s) args+=(-lunwind) ;;
    *) args+=("$arg") ;;
  esac
done
# A dependency that also declares a cdylib (sevenz-rust2) gets a .so nothing loads: link it bare,
# without the shim objects, whose versioned symbols only resolve in the payload's own link.
for arg in "$@"; do
  [[ "$arg" == -shared ]] && exec "$PS5_PAYLOAD_SDK/bin/prospero-clang" "${args[@]}"
done
obj=/work/target/ps5/shim
mapfile -t wraps < /work/ps5/shim/syscalls.link
# rustc passes -nodefaultlibs, so the SDK's native imports are named here; libc++ and libc++abi
# (static) are for UnRAR's C++ (unrar_sys).
exec "$PS5_PAYLOAD_SDK/bin/prospero-clang" "${args[@]}" \
  "$obj/freebsd11.o" "$obj/syscalls.o" "$obj/entry.o" "$obj/compat.o" "$obj/launcher.o" "$obj/installer.o" "${wraps[@]}" \
  -Wl,--wrap=main -Wl,--wrap=fcntl -Wl,--wrap=sysctl -Wl,--wrap=cpuset_getaffinity -Wl,--error-limit=0 \
  -ldl -lc++ -lc++abi -lunwind -lc -lkernel_web -lSceLibcInternal -lSceNet -lSceSystemService -lSceNotification -lSceAppInstUtil
