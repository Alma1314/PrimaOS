use prima_windows::pe::PeImage;

fn main() {
    let path = std::env::args().nth(1).expect("usage: inspect <pe-file>");
    let bytes = std::fs::read(&path).expect("read PE");
    let pe = PeImage::parse(&bytes).expect("parse PE32+");
    pe.validate_mapping().expect("validate mapping");
    println!(
        "machine={:#x} image_base={:#x} entry={:#x} image={:#x} headers={:#x}",
        pe.machine(),
        pe.image_base(),
        pe.entry_rva(),
        pe.size_of_image(),
        pe.size_of_headers()
    );
    for section in pe.sections() {
        println!(
            "section {:?} va={:#x} vsize={:#x} raw={:#x}+{:#x} {:?}",
            core::str::from_utf8(section.name()).unwrap_or("<non-utf8>"),
            section.virtual_address(),
            section.virtual_size(),
            section.raw_offset(),
            section.raw_size(),
            section.protection()
        );
    }
    let mut import_count = 0u32;
    if let Ok(imports) = pe.imports() {
        for descriptor in imports {
            let descriptor = match descriptor {
                Ok(descriptor) => descriptor,
                Err(error) => {
                    println!("import-error={error:?}");
                    break;
                }
            };
            import_count += 1;
            let dll = core::str::from_utf8(descriptor.name()).unwrap_or("<non-utf8>");
            println!("dll={dll}");
            if let Ok(thunks) = descriptor.thunks() {
                for (index, thunk) in thunks.enumerate() {
                    match thunk {
                        Ok(thunk) => println!("  thunk[{index}]={thunk:?}"),
                        Err(error) => println!("  thunk[{index}]-error={error:?}"),
                    }
                }
            }
        }
    }
    if let Ok(exports) = pe.exports() {
        for export in exports.flatten() {
            println!(
                "export name={:?} ordinal={} target={:?}",
                export
                    .name()
                    .and_then(|name| core::str::from_utf8(name).ok()),
                export.ordinal(),
                export.target()
            );
        }
    }
    let relocations = match pe.relocations() {
        Ok(blocks) => blocks.filter(|block| block.is_ok()).count(),
        Err(_) => 0,
    };
    println!(
        "imports={import_count} relocation_blocks={relocations} tls={}",
        pe.tls().ok().flatten().is_some()
    );
    if std::env::args().nth(2).as_deref() == Some("load") {
        let base = std::env::args()
            .nth(3)
            .map(|raw| u64::from_str_radix(raw.trim_start_matches("0x"), 16).expect("hex base"))
            .unwrap_or_else(|| pe.image_base());
        let mut image = vec![0u8; pe.size_of_image() as usize];
        let mut next_stub = 0x1_7000_0000u64;
        let loaded = pe
            .load_into(base, &mut image, |_, _| {
                let stub = next_stub;
                next_stub += 8;
                Some(stub)
            })
            .expect("load image");
        println!(
            "loaded base={:#x} entry={:#x} imports={} relocs={}",
            loaded.base(),
            loaded.entry_va(),
            loaded.imports_resolved(),
            loaded.relocations_applied()
        );
    }
}
