//! Package headers: the finalized-image header (`fih::parse`) and the container it points to
//! (`cnt::read`), with every digest check the verifier runs on them.
#![no_main]

use libfuzzer_sys::fuzz_target;
use ps5_dump_forge_fuzz::sparse::Sparse;
use ps5upload_fpkg::{PkgFile, cnt, fih};

fuzz_target!(|data: &[u8]| {
    let image = Sparse::decode(data);
    let len = image.len();
    let mut file = PkgFile::from_reader(Box::new(image), len);
    let Ok(head) = file.read_at(0, fih::HEADER_LEN) else {
        return;
    };
    let Ok(header) = fih::parse(&head) else {
        return;
    };
    let _ = header.is_debug();
    let Ok(container) = cnt::read(&mut file, header.cnt_offset) else {
        return;
    };
    let _ = container.package_digest_ok();
    let _ = container.header_rollup_ok();
    let _ = container.sc_entries2_ok();
    let _ = container.body_digest_ok();
    let _ = container.fih_digest_ok(&head);
    let _ = container.descriptor_ok();
    let _ = container.digest_table_digest_ok();
    let _ = container.general_digests(&header.game_digest);
    let _ = container.image_digests();
    for (id, _) in container.entry_digests() {
        if let Some(entry) = container.entry(id) {
            let _ = container.payload(entry).len();
            let _ = container.padded_payload(entry).len();
        }
    }
});
