use prima_windows::load::LoadError;
use prima_windows::pe::{
    DataDirectoryIndex, ImportThunk, PeError, PeImage, Protection, RelocationEntry,
};

const LEN: usize = 0x1400;
const PE_OFF: usize = 0x80;
const OPT_OFF: usize = 0x98;
const SEC_OFF: usize = 0x188;

const E_LFANEW: usize = 0x3c;
const MACHINE: usize = PE_OFF + 4;
const OPT_SIZE: usize = PE_OFF + 20;
const MAGIC: usize = OPT_OFF;
const SECTION_ALIGN: usize = OPT_OFF + 0x20;
const FILE_ALIGN: usize = OPT_OFF + 0x24;
const NUM_DIRS: usize = OPT_OFF + 0x6c;
const IMPORT_NAME_RVA: usize = 0x40c;
const TLS_CALLBACKS: usize = 0x918;
const RELOC_BLOCK_SIZE: usize = 0xC04;

const RDATA_SEC: usize = SEC_OFF + 40;
const RDATA_VA: usize = RDATA_SEC + 12;
const RDATA_RAW_SIZE: usize = RDATA_SEC + 16;
const RDATA_RAW_PTR: usize = RDATA_SEC + 20;
const RDATA_CHARS: usize = RDATA_SEC + 36;

fn put16(buf: &mut [u8], off: usize, value: u16) {
    buf[off..off + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(buf: &mut [u8], off: usize, value: u32) {
    buf[off..off + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(buf: &mut [u8], off: usize, value: u64) {
    buf[off..off + 8].copy_from_slice(&value.to_le_bytes());
}

#[allow(clippy::too_many_arguments)]
fn write_section(
    buf: &mut [u8],
    index: usize,
    name: &[u8],
    virtual_size: u32,
    virtual_address: u32,
    raw_size: u32,
    raw_offset: u32,
    characteristics: u32,
) {
    let off = SEC_OFF + index * 40;
    buf[off..off + name.len()].copy_from_slice(name);
    put32(buf, off + 8, virtual_size);
    put32(buf, off + 12, virtual_address);
    put32(buf, off + 16, raw_size);
    put32(buf, off + 20, raw_offset);
    put32(buf, off + 36, characteristics);
}

fn build_pe() -> Vec<u8> {
    let mut buf = vec![0u8; LEN];
    buf[0] = b'M';
    buf[1] = b'Z';
    put32(&mut buf, E_LFANEW, PE_OFF as u32);
    buf[PE_OFF..PE_OFF + 4].copy_from_slice(b"PE\0\0");

    put16(&mut buf, MACHINE, 0x8664);
    put16(&mut buf, PE_OFF + 6, 2);
    put16(&mut buf, OPT_SIZE, 0xf0);
    put16(&mut buf, PE_OFF + 22, 0x0022);

    put16(&mut buf, MAGIC, 0x20b);
    put32(&mut buf, OPT_OFF + 0x10, 0x1000);
    put64(&mut buf, OPT_OFF + 0x18, 0x1_4000_0000);
    put32(&mut buf, SECTION_ALIGN, 0x1000);
    put32(&mut buf, FILE_ALIGN, 0x200);
    put32(&mut buf, OPT_OFF + 0x38, 0x3000);
    put32(&mut buf, OPT_OFF + 0x3c, 0x200);
    put16(&mut buf, OPT_OFF + 0x44, 3);
    put32(&mut buf, NUM_DIRS, 16);

    put32(&mut buf, OPT_OFF + 0x70, 0x2c00);
    put32(&mut buf, OPT_OFF + 0x70 + 4, 40);
    put32(&mut buf, OPT_OFF + 0x70 + 8, 0x2000);
    put32(&mut buf, OPT_OFF + 0x70 + 12, 0x28);
    put32(&mut buf, OPT_OFF + 0x70 + 40, 0x2800);
    put32(&mut buf, OPT_OFF + 0x70 + 44, 12);
    put32(&mut buf, OPT_OFF + 0x70 + 72, 0x2500);
    put32(&mut buf, OPT_OFF + 0x70 + 76, 40);

    write_section(
        &mut buf,
        0,
        b".text",
        0x200,
        0x1000,
        0x200,
        0x200,
        0x6000_0020,
    );
    write_section(
        &mut buf,
        1,
        b".rdata",
        0x1000,
        0x2000,
        0x1000,
        0x400,
        0x4000_0040,
    );

    put32(&mut buf, 0x400, 0x2100);
    put32(&mut buf, 0x40c, 0x2200);
    put32(&mut buf, 0x410, 0x2300);
    put64(&mut buf, 0x500, 0x2400);
    put64(&mut buf, 0x508, 0x8000_0000_0000_0005);
    buf[0x600..0x60d].copy_from_slice(b"KERNEL32.dll\0");
    put16(&mut buf, 0x800, 7);
    buf[0x802..0x80e].copy_from_slice(b"ExitProcess\0");

    put64(&mut buf, 0x900, 0x1_4000_2700);
    put64(&mut buf, 0x908, 0x1_4000_2710);
    put64(&mut buf, 0x910, 0x1_4000_2720);
    put64(&mut buf, TLS_CALLBACKS, 0x1_4000_2730);
    put64(&mut buf, 0xb30, 0x1_4000_1000);

    put32(&mut buf, 0xc00, 0x1000);
    put32(&mut buf, RELOC_BLOCK_SIZE, 12);
    put16(&mut buf, 0xc08, 0xa000);
    put16(&mut buf, 0xc0a, 0x0000);
    put64(&mut buf, 0x200, 0x1_4000_0000);

    // 导出目录 RVA 0x2c00：Base=0、2 个函数、2 个名称。
    put32(&mut buf, 0x1010, 0); // Base
    put32(&mut buf, 0x1014, 2); // NumberOfFunctions
    put32(&mut buf, 0x1018, 2); // NumberOfNames
    put32(&mut buf, 0x101c, 0x2c40); // AddressOfFunctions
    put32(&mut buf, 0x1020, 0x2c50); // AddressOfNames
    put32(&mut buf, 0x1024, 0x2c60); // AddressOfNameOrdinals
    put32(&mut buf, 0x1040, 0x1000);
    put32(&mut buf, 0x1044, 0x1010);
    put32(&mut buf, 0x1050, 0x2c80);
    put32(&mut buf, 0x1054, 0x2c90);
    put16(&mut buf, 0x1060, 0);
    put16(&mut buf, 0x1062, 1);
    buf[0x1080..0x1086].copy_from_slice(b"Alpha\0");
    buf[0x1090..0x1095].copy_from_slice(b"Beta\0");
    buf
}

#[test]
fn parses_pe32_plus_headers_and_sections() {
    let bytes = build_pe();
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    assert_eq!(pe.machine(), 0x8664);
    assert_eq!(pe.image_base(), 0x1_4000_0000);
    assert_eq!(pe.entry_rva(), 0x1000);
    assert_eq!(pe.size_of_image(), 0x3000);
    assert_eq!(pe.size_of_headers(), 0x200);
    assert_eq!(pe.file_alignment(), 0x200);
    assert_eq!(pe.section_alignment(), 0x1000);

    let sections: Vec<_> = pe.sections().collect();
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].name(), b".text");
    assert_eq!(sections[0].virtual_address(), 0x1000);
    assert_eq!(sections[0].protection(), Protection::ReadExecute);
    assert_eq!(sections[1].name(), b".rdata");
    assert_eq!(sections[1].protection(), Protection::ReadOnly);
}

#[test]
fn validates_mapping_of_fixture() {
    let bytes = build_pe();
    PeImage::parse(&bytes)
        .expect("valid fixture")
        .validate_mapping()
        .expect("consistent mapping");
}

#[test]
fn maps_rva_to_file_offset_and_slices() {
    let bytes = build_pe();
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    assert_eq!(pe.rva_to_offset(0x80, 4).expect("headers"), 0x80);
    assert_eq!(pe.rva_to_offset(0x1000, 4).expect("text"), 0x200);
    assert_eq!(pe.rva_to_offset(0x2100, 8).expect("rdata"), 0x500);
    assert_eq!(pe.slice_rva(0x2200, 12).expect("dll name"), b"KERNEL32.dll");
    assert_eq!(
        pe.slice_rva(0x2402, 11).expect("import name"),
        b"ExitProcess"
    );
    assert!(matches!(
        pe.rva_to_offset(0x3000, 1),
        Err(PeError::RvaNotMapped(0x3000))
    ));
    assert!(matches!(
        pe.rva_to_offset(u32::MAX, 2),
        Err(PeError::RangeOverflow)
    ));
}

#[test]
fn rejects_empty_and_missing_dos_magic() {
    assert!(matches!(PeImage::parse(&[]), Err(PeError::Truncated)));
    let mut bytes = build_pe();
    bytes[0] = b'N';
    assert!(matches!(PeImage::parse(&bytes), Err(PeError::DosMagic)));
}

#[test]
fn rejects_bad_pe_signature() {
    let mut bytes = build_pe();
    bytes[PE_OFF] = b'X';
    assert!(matches!(PeImage::parse(&bytes), Err(PeError::PeSignature)));
}

#[test]
fn rejects_pe32_optional_header() {
    let mut bytes = build_pe();
    put16(&mut bytes, MAGIC, 0x10b);
    assert!(matches!(
        PeImage::parse(&bytes),
        Err(PeError::OptionalMagic(0x10b))
    ));
}

#[test]
fn rejects_non_amd64_machine() {
    let mut bytes = build_pe();
    put16(&mut bytes, MACHINE, 0x14c);
    assert!(matches!(
        PeImage::parse(&bytes),
        Err(PeError::Machine(0x14c))
    ));
}

#[test]
fn rejects_truncated_section_table() {
    let mut bytes = build_pe();
    bytes.truncate(SEC_OFF + 40);
    assert!(matches!(
        PeImage::parse(&bytes),
        Err(PeError::SectionTableOutOfBounds)
    ));
}

#[test]
fn rejects_optional_header_too_small_for_declared_directories() {
    let mut bytes = build_pe();
    put16(&mut bytes, OPT_SIZE, 0xe0);
    assert!(matches!(
        PeImage::parse(&bytes),
        Err(PeError::OptionalHeaderTooSmall)
    ));
}

#[test]
fn rejects_invalid_alignments() {
    let mut bytes = build_pe();
    put32(&mut bytes, SECTION_ALIGN, 0x300);
    assert!(matches!(
        PeImage::parse(&bytes)
            .expect("headers still parse")
            .validate_mapping(),
        Err(PeError::Alignment)
    ));

    let mut bytes = build_pe();
    put32(&mut bytes, FILE_ALIGN, 0x1000);
    put32(&mut bytes, SECTION_ALIGN, 0x200);
    assert!(matches!(
        PeImage::parse(&bytes)
            .expect("headers still parse")
            .validate_mapping(),
        Err(PeError::Alignment)
    ));
}

#[test]
fn rejects_headers_smaller_than_section_table() {
    let mut bytes = build_pe();
    put32(&mut bytes, OPT_OFF + 0x3c, 0x100);
    assert!(matches!(
        PeImage::parse(&bytes)
            .expect("headers still parse")
            .validate_mapping(),
        Err(PeError::ImageRange)
    ));
}

#[test]
fn rejects_virtual_padding_without_file_bytes() {
    let mut bytes = build_pe();
    put32(&mut bytes, RDATA_RAW_SIZE, 0x200);
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    assert!(matches!(
        pe.rva_to_offset(0x2500, 1),
        Err(PeError::RvaNotInFile(0x2500))
    ));
}

#[test]
fn rejects_section_raw_range_outside_file() {
    let mut bytes = build_pe();
    put32(&mut bytes, RDATA_RAW_PTR, 0x10000);
    assert!(matches!(
        PeImage::parse(&bytes)
            .expect("headers still parse")
            .validate_mapping(),
        Err(PeError::FileRange)
    ));
}

#[test]
fn rejects_writable_executable_section() {
    let mut bytes = build_pe();
    put32(&mut bytes, RDATA_CHARS, 0xe000_0040);
    assert!(matches!(
        PeImage::parse(&bytes)
            .expect("headers still parse")
            .validate_mapping(),
        Err(PeError::WritableExecutable)
    ));
}

#[test]
fn rejects_overlapping_sections() {
    let mut bytes = build_pe();
    put32(&mut bytes, RDATA_VA, 0x1000);
    assert!(matches!(
        PeImage::parse(&bytes)
            .expect("headers still parse")
            .validate_mapping(),
        Err(PeError::SectionOverlap)
    ));
}

#[test]
fn parses_import_descriptors_and_thunks() {
    let bytes = build_pe();
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    assert_eq!(
        pe.data_directory(DataDirectoryIndex::Import)
            .expect("import directory")
            .size(),
        0x28
    );
    assert!(pe.data_directory(DataDirectoryIndex::Exception).is_none());

    let descriptors: Vec<_> = pe
        .imports()
        .expect("import directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("import descriptors");
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].name(), b"KERNEL32.dll");

    let thunks: Vec<_> = descriptors[0]
        .thunks()
        .expect("thunk table")
        .collect::<Result<Vec<_>, _>>()
        .expect("thunks");
    assert_eq!(thunks.len(), 2);
    assert_eq!(
        thunks[0],
        ImportThunk::Name {
            hint: 7,
            name: b"ExitProcess"
        }
    );
    assert_eq!(thunks[1], ImportThunk::Ordinal(5));
}

#[test]
fn rejects_import_name_outside_file() {
    let mut bytes = build_pe();
    put32(&mut bytes, IMPORT_NAME_RVA, 0x5000);
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    assert!(
        pe.imports()
            .expect("directory")
            .next()
            .expect("one descriptor")
            .is_err()
    );
}

#[test]
fn parses_base_relocations() {
    let bytes = build_pe();
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let blocks: Vec<_> = pe
        .relocations()
        .expect("relocation directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("blocks");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].page_rva(), 0x1000);
    let entries: Vec<_> = blocks[0].entries().collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0], RelocationEntry::Dir64 { offset: 0 });
    assert_eq!(entries[1], RelocationEntry::Absolute);
}

#[test]
fn rejects_relocation_block_with_invalid_size() {
    let mut bytes = build_pe();
    put32(&mut bytes, RELOC_BLOCK_SIZE, 0x11);
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let first = pe
        .relocations()
        .expect("relocation directory")
        .next()
        .expect("one block");
    assert!(matches!(first, Err(PeError::RelocationOutOfBounds)));
}

#[test]
fn parses_tls_directory_and_callbacks() {
    let bytes = build_pe();
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let tls = pe.tls().expect("tls parse").expect("tls directory");
    assert_eq!(tls.raw_start(), 0x1_4000_2700);
    assert_eq!(tls.callbacks_address(), 0x1_4000_2730);
    assert_eq!(tls.zero_fill(), 0);
    let callbacks: Vec<_> = pe
        .tls_callbacks()
        .expect("callbacks")
        .collect::<Result<Vec<_>, _>>()
        .expect("callback values");
    assert_eq!(callbacks, vec![0x1_4000_1000]);
}

#[test]
fn rejects_tls_callback_outside_image() {
    let mut bytes = build_pe();
    put64(&mut bytes, TLS_CALLBACKS, 0x1_4000_5000);
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let first = pe
        .tls_callbacks()
        .expect("callbacks")
        .next()
        .expect("one callback");
    assert!(matches!(first, Err(PeError::TlsOutOfBounds)));
}

fn stub_resolver() -> impl FnMut(&[u8], ImportThunk<'_>) -> Option<u64> {
    |dll, thunk| {
        if dll != b"KERNEL32.dll" {
            return None;
        }
        match thunk {
            ImportThunk::Name { name, .. } if name == b"ExitProcess" => Some(0x1_5000_9000),
            ImportThunk::Ordinal(5) => Some(0x1_5000_9008),
            _ => None,
        }
    }
}

fn read_u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("8 bytes"))
}

#[test]
fn loads_at_preferred_base_and_resolves_imports() {
    let bytes = build_pe();
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let mut image = vec![0u8; pe.size_of_image() as usize];
    let loaded = pe
        .load_into(pe.image_base(), &mut image, stub_resolver())
        .expect("load at preferred base");

    assert_eq!(loaded.base(), pe.image_base());
    assert_eq!(loaded.entry_va(), pe.image_base() + 0x1000);
    assert_eq!(&image[0..2], b"MZ");
    assert_eq!(read_u64_at(&image, 0x1000), pe.image_base());
    assert_eq!(read_u64_at(&image, 0x2300), 0x1_5000_9000);
    assert_eq!(read_u64_at(&image, 0x2308), 0x1_5000_9008);
    assert_eq!(loaded.imports_resolved(), 2);
    assert_eq!(loaded.relocations_applied(), 1);
}

#[test]
fn loads_at_new_base_and_applies_dir64_relocation() {
    let bytes = build_pe();
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let base = 0x1_5000_0000;
    let mut image = vec![0u8; pe.size_of_image() as usize];
    let loaded = pe
        .load_into(base, &mut image, stub_resolver())
        .expect("load at new base");

    assert_eq!(loaded.entry_va(), base + 0x1000);
    assert_eq!(read_u64_at(&image, 0x1000), base);
}

#[test]
fn rejects_small_output_buffer() {
    let bytes = build_pe();
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let mut image = vec![0u8; 0x2000];
    assert!(matches!(
        pe.load_into(pe.image_base(), &mut image, stub_resolver()),
        Err(LoadError::OutputTooSmall { .. })
    ));
}

#[test]
fn rejects_base_change_without_relocations() {
    let mut bytes = build_pe();
    put32(&mut bytes, OPT_OFF + 0x70 + 40, 0);
    put32(&mut bytes, OPT_OFF + 0x70 + 44, 0);
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let mut image = vec![0u8; pe.size_of_image() as usize];
    assert!(matches!(
        pe.load_into(pe.image_base() + 0x1000_0000, &mut image, stub_resolver()),
        Err(LoadError::BaseWithoutRelocations)
    ));
}

#[test]
fn applies_highlow_relocation_without_touching_upper_dword() {
    let mut bytes = build_pe();
    put16(&mut bytes, 0xc08, 0x3000);
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let base = 0x1_5000_0000;
    let mut image = vec![0u8; pe.size_of_image() as usize];
    pe.load_into(base, &mut image, stub_resolver())
        .expect("load with highlow");

    assert_eq!(
        u32::from_le_bytes(image[0x1000..0x1004].try_into().unwrap()),
        0x5000_0000
    );
    assert_eq!(
        u32::from_le_bytes(image[0x1004..0x1008].try_into().unwrap()),
        1
    );
}

#[test]
fn rejects_relocation_target_outside_image() {
    let mut bytes = build_pe();
    put32(&mut bytes, 0xc00, 0x5000);
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let mut image = vec![0u8; pe.size_of_image() as usize];
    assert!(matches!(
        pe.load_into(pe.image_base() + 0x1000_0000, &mut image, stub_resolver()),
        Err(LoadError::RelocationTargetOutOfImage)
    ));
}

#[test]
fn rejects_unsupported_relocation_type() {
    let mut bytes = build_pe();
    put16(&mut bytes, 0xc08, 0x1000);
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let mut image = vec![0u8; pe.size_of_image() as usize];
    assert!(matches!(
        pe.load_into(pe.image_base() + 0x1000_0000, &mut image, stub_resolver()),
        Err(LoadError::UnsupportedRelocation(1))
    ));
}

#[test]
fn rejects_unresolved_import() {
    let bytes = build_pe();
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let mut image = vec![0u8; pe.size_of_image() as usize];
    assert!(matches!(
        pe.load_into(pe.image_base(), &mut image, |_, _| None),
        Err(LoadError::UnresolvedImport)
    ));
}

#[test]
fn rejects_import_address_outside_image() {
    let mut bytes = build_pe();
    put32(&mut bytes, 0x410, 0x5000);
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let mut image = vec![0u8; pe.size_of_image() as usize];
    assert!(matches!(
        pe.load_into(pe.image_base(), &mut image, stub_resolver()),
        Err(LoadError::ImportAddressOutOfImage)
    ));
}

#[test]
fn rejects_zero_import_address_table() {
    let mut bytes = build_pe();
    put32(&mut bytes, 0x410, 0);
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let mut image = vec![0u8; pe.size_of_image() as usize];
    assert!(matches!(
        pe.load_into(pe.image_base(), &mut image, stub_resolver()),
        Err(LoadError::ImportAddressOutOfImage)
    ));
}

#[test]
fn rejects_wx_image_before_writing_output() {
    let mut bytes = build_pe();
    put32(&mut bytes, RDATA_CHARS, 0xe000_0040);
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let mut image = vec![0u8; pe.size_of_image() as usize];
    assert!(matches!(
        pe.load_into(pe.image_base(), &mut image, stub_resolver()),
        Err(LoadError::Pe(PeError::WritableExecutable))
    ));
    assert!(image.iter().all(|&byte| byte == 0));
}

#[test]
fn iterates_named_exports_with_ordinals_and_rvas() {
    let bytes = build_pe();
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let exports: Vec<_> = pe
        .exports()
        .expect("export directory")
        .collect::<Result<Vec<_>, _>>()
        .expect("exports");
    assert_eq!(exports.len(), 2);
    assert_eq!(exports[0].name(), b"Alpha");
    assert_eq!(exports[0].ordinal(), 0);
    assert_eq!(exports[0].rva(), 0x1000);
    assert_eq!(exports[1].name(), b"Beta");
    assert_eq!(exports[1].ordinal(), 1);
    assert_eq!(exports[1].rva(), 0x1010);

    let found = pe
        .export_by_name(b"Beta")
        .expect("export lookup")
        .expect("Beta export");
    assert_eq!(found.rva(), 0x1010);
    assert!(
        pe.export_by_name(b"Gamma")
            .expect("export lookup")
            .is_none()
    );
}

#[test]
fn rejects_forwarded_exports_instead_of_resolving_dir_rva() {
    let mut bytes = build_pe();
    put32(&mut bytes, 0x1040, 0x2c04);
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    let result: Result<Vec<_>, _> = pe.exports().expect("export directory").collect();
    assert!(matches!(result, Err(PeError::ForwardedExport)));
}

#[test]
fn rejects_truncated_export_directory() {
    let mut bytes = build_pe();
    put32(&mut bytes, OPT_OFF + 0x70 + 4, 8);
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    assert!(matches!(pe.exports(), Err(PeError::ExportOutOfBounds)));
}

#[test]
fn rejects_export_tables_outside_file_data() {
    let mut bytes = build_pe();
    put32(&mut bytes, 0x1020, 0x3f00);
    let pe = PeImage::parse(&bytes).expect("valid fixture");
    assert!(matches!(pe.exports(), Err(PeError::ExportOutOfBounds)));
}
