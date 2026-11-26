use ps5upload_fpkg::source::SourceTree;

fn main() -> ps5upload_fpkg::Result<()> {
    let path = std::env::args()
        .nth(1)
        .expect("usage: list_large_files <pkg>");
    let tree = ps5upload_fpkg::pkg_source::open(
        std::path::Path::new(&path),
        ps5upload_fpkg::crypto::DEFAULT_PASSCODE,
    )?;
    let min = std::env::args()
        .nth(2)
        .map(|s| s.trim_start_matches("0x").to_owned())
        .map(|s| u64::from_str_radix(&s, 16).expect("min is hexadecimal"))
        .unwrap_or(0x10_0000);
    for file in tree.files().iter().filter(|file| file.size >= min) {
        println!("{:#012x} {}", file.size, file.path);
    }
    Ok(())
}
