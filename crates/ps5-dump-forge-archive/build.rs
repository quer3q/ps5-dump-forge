// Compiles rar.rs's two UnRAR structs (verbatim from unrar_sys 0.5.8's dll.hpp, `pack(1)`) with
// UnRAR's C++ compiler and asserts the sizes and offsets rar.rs asserts, so a wchar_t mismatch
// fails the build. Nothing is linked.
// ponytail: the copy is checked by hand: an unrar_sys bump must diff its dll.hpp against it.
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    // UnRAR's Windows code needs advapi32, which unrar_sys doesn't link; MSVC fails without it.
    if var("CARGO_CFG_TARGET_OS") == "windows" {
        println!("cargo:rustc-link-lib=advapi32");
    }
    let w = if var("CARGO_CFG_TARGET_OS") == "windows" || var("CARGO_CFG_TARGET_ENV") == "ps5" {
        2
    } else {
        4
    };
    // RARHeaderDataEx: `Flags` follows ArcName, ArcNameW, FileName, FileNameW.
    let h = 2048 + 2048 * w;
    let header = [
        ("FileNameW", 2048 + 1024 * w),
        ("Flags", h),
        ("UnpSize", h + 12),
        ("UnpSizeHigh", h + 16),
        ("HostOS", h + 20),
        ("FileAttr", h + 40),
        ("RedirType", h + 104),
        ("RedirName", h + 108),
        ("FileNameEx", h + 160),
    ];
    let open = [
        ("ArcName", 0),
        ("ArcNameW", 8),
        ("OpenMode", 16),
        ("OpenResult", 20),
        ("Flags", 44),
        ("Callback", 48),
        ("UserData", 56),
        ("OpFlags", 64),
        ("CmtBufW", 68),
    ];
    let mut asserts = format!(
        "static_assert(sizeof(wchar_t) == {w}, \"wchar_t\");\n\
         static_assert(sizeof(void *) == 8 && sizeof(LPARAM) == 8, \"64-bit\");\n\
         static_assert(sizeof(RARHeaderDataEx) == {}, \"RARHeaderDataEx\");\n\
         static_assert(sizeof(RAROpenArchiveDataEx) == 176, \"RAROpenArchiveDataEx\");\n",
        h + 4100
    );
    for (s, fields) in [
        ("RARHeaderDataEx", &header[..]),
        ("RAROpenArchiveDataEx", &open[..]),
    ] {
        for (f, off) in fields {
            asserts += &format!("static_assert(offsetof({s}, {f}) == {off}, \"{s}.{f}\");\n");
        }
    }
    let src = std::path::Path::new(&var("OUT_DIR")).join("rar_abi.cpp");
    std::fs::write(&src, format!("{DLL_HPP}{asserts}")).expect("write rar_abi.cpp");
    cc::Build::new()
        .cpp(true)
        .cpp_link_stdlib(None)
        .cargo_metadata(false)
        .file(&src)
        .compile("forge_rar_abi");
}

const DLL_HPP: &str = r#"
#include <stddef.h>
#ifdef _WIN32
#include <windows.h>
#else
#define CALLBACK
#define LPARAM long
#define UINT unsigned int
#endif
#pragma pack(push, 1)
struct RARHeaderDataEx
{
  char         ArcName[1024];
  wchar_t      ArcNameW[1024];
  char         FileName[1024];
  wchar_t      FileNameW[1024];
  unsigned int Flags;
  unsigned int PackSize;
  unsigned int PackSizeHigh;
  unsigned int UnpSize;
  unsigned int UnpSizeHigh;
  unsigned int HostOS;
  unsigned int FileCRC;
  unsigned int FileTime;
  unsigned int UnpVer;
  unsigned int Method;
  unsigned int FileAttr;
  char         *CmtBuf;
  unsigned int CmtBufSize;
  unsigned int CmtSize;
  unsigned int CmtState;
  unsigned int DictSize;
  unsigned int HashType;
  char         Hash[32];
  unsigned int RedirType;
  wchar_t      *RedirName;
  unsigned int RedirNameSize;
  unsigned int DirTarget;
  unsigned int MtimeLow;
  unsigned int MtimeHigh;
  unsigned int CtimeLow;
  unsigned int CtimeHigh;
  unsigned int AtimeLow;
  unsigned int AtimeHigh;
  wchar_t      *ArcNameEx;
  unsigned int ArcNameExSize;
  wchar_t      *FileNameEx;
  unsigned int FileNameExSize;
  unsigned int Reserved[982];
};
typedef int (CALLBACK *UNRARCALLBACK)(UINT msg,LPARAM UserData,LPARAM P1,LPARAM P2);
struct RAROpenArchiveDataEx
{
  char         *ArcName;
  wchar_t      *ArcNameW;
  unsigned int  OpenMode;
  unsigned int  OpenResult;
  char         *CmtBuf;
  unsigned int  CmtBufSize;
  unsigned int  CmtSize;
  unsigned int  CmtState;
  unsigned int  Flags;
  UNRARCALLBACK Callback;
  LPARAM        UserData;
  unsigned int  OpFlags;
  wchar_t      *CmtBufW;
  unsigned int  Reserved[25];
};
#pragma pack(pop)
"#;
