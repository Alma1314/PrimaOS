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
    let imports = match pe.imports() {
        Ok(imports) => imports.filter(|entry| entry.is_ok()).count(),
        Err(_) => 0,
    };
    let relocations = match pe.relocations() {
        Ok(blocks) => blocks.filter(|block| block.is_ok()).count(),
        Err(_) => 0,
    };
    println!(
        "imports={imports} relocation_blocks={relocations} tls={}",
        pe.tls().ok().flatten().is_some()
    );
}
