#![forbid(unsafe_code)]
// Frozen mirrors extracted from kernel/user.rs; many constants are intentionally
// retained for byte-for-byte regeneration and future slices.
#![allow(dead_code)]

//! EXO-2C host-side init machine-code generator.
//!
//! `generate` produces the same two-page x86-64 code image that used to be built
//! inside `kernel/src/user.rs`. The kernel now treats it as an opaque ramdisk
//! ELF; this crate exists only in the build toolchain.

mod bootstrap;
mod code;
pub mod rights;
mod syscall;

pub use code::{USER_CODE_LEN, USER_CODE_VA, USER_CODE2_VA, generate};

/// Subset of a service image entry needed by the init code generator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageEntry {
    pub pages: usize,
    pub entry: u64,
}

/// Subset of the boot image layout embedded into the init machine code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootLayout {
    pub alpha: ImageEntry,
    pub servicemgr: ImageEntry,
    pub alpha_pages_off: usize,
    pub servicemgr_pages_off: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::*;
    use crate::code::*;
    use crate::syscall::*;

    fn test_layout() -> BootLayout {
        // Values mirror the old kernel `user::tests::test_layout` for the fields
        // that the init generator actually embeds.
        BootLayout {
            alpha: ImageEntry {
                pages: 5,
                entry: 0x1234,
            },
            servicemgr: ImageEntry {
                pages: 3,
                entry: 0x3456,
            },
            alpha_pages_off: 1,
            servicemgr_pages_off: 8,
        }
    }

    #[test]
    fn host_generated_image_matches_frozen_layout() {
        let layout = test_layout();
        let code = generate(&layout);
        assert_eq!(USER_CODE_LEN, USER_CODE_PAGES * FRAME_SIZE as usize);
        // COW 故障点与回执 0。
        assert_eq!(&code[0..2], &[0x48, 0xBA]); // mov rdx, imm64
        assert_eq!(&code[2..10], &USER_X_VA.to_le_bytes());
        assert_eq!(&code[10..12], &[0x48, 0xB8]); // mov rax, imm64
        assert_eq!(&code[12..20], &USER_STORE_VALUE.to_le_bytes());
        assert_eq!(&code[20..23], &[0x48, 0x89, 0x02]); // mov [rdx], rax (3 bytes)
        assert_eq!(&code[23..25], &[0x48, 0xB9]); // mov rcx, imm64
        assert_eq!(&code[25..33], &USER_Y_VA.to_le_bytes());
        assert_eq!(&code[33..36], &[0x48, 0x89, 0x01]); // mov [rcx], rax
        // 回执 1：SYS_ECHO。
        assert_eq!(&code[36..38], &[0x48, 0xBF]); // mov rdi, imm64
        assert_eq!(&code[38..46], &SYS_ECHO_ARG.to_le_bytes());
        assert_eq!(&code[46..48], &[0x48, 0xB8]); // mov rax, imm64
        assert_eq!(&code[48..56], &SYS_ECHO.to_le_bytes());
        assert_eq!(&code[56..58], &[0xCD, 0x80]); // int 0x80
        assert_eq!(&code[58..62], &[0x48, 0x89, 0x41, 0x08]); // mov [rcx+8], rax
        // 回执 2：SYS_SUM4，参数经 rdi/rsi/rdx/r10。
        assert_eq!(&code[62..64], &[0x48, 0xBF]);
        assert_eq!(&code[64..72], &SUM_ARGS[0].to_le_bytes());
        assert_eq!(&code[72..74], &[0x48, 0xBE]);
        assert_eq!(&code[74..82], &SUM_ARGS[1].to_le_bytes());
        assert_eq!(&code[82..84], &[0x48, 0xBA]);
        assert_eq!(&code[84..92], &SUM_ARGS[2].to_le_bytes());
        assert_eq!(&code[92..94], &[0x49, 0xBA]); // mov r10, imm64
        assert_eq!(&code[94..102], &SUM_ARGS[3].to_le_bytes());
        assert_eq!(&code[102..104], &[0x48, 0xB8]);
        assert_eq!(&code[104..112], &SYS_SUM4.to_le_bytes());
        assert_eq!(&code[112..114], &[0xCD, 0x80]);
        assert_eq!(&code[114..118], &[0x48, 0x89, 0x41, 0x10]);
        // 回执 3/4：SYS_COPY_IN 合法与坏指针。
        assert_eq!(&code[118..120], &[0x48, 0xBF]);
        assert_eq!(&code[120..128], &COPY_PTR.to_le_bytes());
        assert_eq!(&code[128..130], &[0x48, 0xBE]);
        assert_eq!(&code[130..138], &COPY_LEN.to_le_bytes());
        assert_eq!(&code[138..140], &[0x48, 0xB8]);
        assert_eq!(&code[140..148], &SYS_COPY_IN.to_le_bytes());
        assert_eq!(&code[148..150], &[0xCD, 0x80]);
        assert_eq!(&code[150..154], &[0x48, 0x89, 0x41, 0x18]);
        assert_eq!(&code[154..156], &[0x48, 0xBF]);
        assert_eq!(&code[156..164], &BAD_PTR.to_le_bytes());
        assert_eq!(&code[164..166], &[0x48, 0xBE]);
        assert_eq!(&code[166..174], &COPY_LEN.to_le_bytes());
        assert_eq!(&code[174..176], &[0x48, 0xB8]);
        assert_eq!(&code[176..184], &SYS_COPY_IN.to_le_bytes());
        assert_eq!(&code[184..186], &[0xCD, 0x80]);
        assert_eq!(&code[186..190], &[0x48, 0x89, 0x41, 0x20]);
        // 回执 5/6：保留号 0 与未登记号。
        assert_eq!(&code[190..192], &[0x48, 0xB8]);
        assert_eq!(&code[192..200], &0u64.to_le_bytes());
        assert_eq!(&code[200..202], &[0xCD, 0x80]);
        assert_eq!(&code[202..206], &[0x48, 0x89, 0x41, 0x28]);
        assert_eq!(&code[206..208], &[0x48, 0xB8]);
        assert_eq!(&code[208..216], &SYS_BAD.to_le_bytes());
        assert_eq!(&code[216..218], &[0xCD, 0x80]);
        assert_eq!(&code[218..222], &[0x48, 0x89, 0x41, 0x30]);
        // 授权查询三次。
        assert_eq!(&code[222..224], &[0x48, 0xBF]);
        assert_eq!(&code[224..232], &AUTHORIZE_CAP.to_le_bytes());
        assert_eq!(&code[232..234], &[0x48, 0xBE]);
        assert_eq!(&code[234..242], &RIGHT_SEGMAPR.to_le_bytes());
        assert_eq!(&code[242..244], &[0x48, 0xB8]);
        assert_eq!(&code[244..252], &SYS_AUTHORIZE.to_le_bytes());
        assert_eq!(&code[252..254], &[0xCD, 0x80]);
        assert_eq!(&code[254..258], &[0x48, 0x89, 0x41, 0x40]); // [rcx+64]
        assert_eq!(&code[258..260], &[0x48, 0xBE]);
        assert_eq!(&code[260..268], &RIGHT_SEGMAPW.to_le_bytes());
        assert_eq!(&code[268..270], &[0x48, 0xB8]);
        assert_eq!(&code[270..278], &SYS_AUTHORIZE.to_le_bytes());
        assert_eq!(&code[278..280], &[0xCD, 0x80]);
        assert_eq!(&code[280..284], &[0x48, 0x89, 0x41, 0x48]); // [rcx+72]
        assert_eq!(&code[284..286], &[0x48, 0xBF]);
        assert_eq!(&code[286..294], &UNSET_CAP.to_le_bytes());
        assert_eq!(&code[294..296], &[0x48, 0xBE]);
        assert_eq!(&code[296..304], &RIGHT_SEGMAPR.to_le_bytes());
        assert_eq!(&code[304..306], &[0x48, 0xB8]);
        assert_eq!(&code[306..314], &SYS_AUTHORIZE.to_le_bytes());
        assert_eq!(&code[314..316], &[0xCD, 0x80]);
        assert_eq!(&code[316..320], &[0x48, 0x89, 0x41, 0x50]); // [rcx+80]
        // 异常订阅六次（SPEC-050）。
        assert_eq!(&code[320..322], &[0x48, 0xBF]);
        assert_eq!(&code[322..330], &EXC_PAGE_FAULT_CAP.to_le_bytes());
        assert_eq!(&code[330..332], &[0x48, 0xBE]);
        assert_eq!(&code[332..340], &EXC_KIND_PAGEFAULT.to_le_bytes());
        assert_eq!(&code[340..342], &[0x48, 0xB8]);
        assert_eq!(&code[342..350], &SYS_EXC_SUBSCRIBE.to_le_bytes());
        assert_eq!(&code[350..352], &[0xCD, 0x80]);
        assert_eq!(&code[352..356], &[0x48, 0x89, 0x41, 0x58]); // [rcx+88]
        assert_eq!(&code[356..358], &[0x48, 0xBE]);
        assert_eq!(&code[358..366], &EXC_KIND_PAGEFAULT.to_le_bytes());
        assert_eq!(&code[366..368], &[0x48, 0xB8]);
        assert_eq!(&code[368..376], &SYS_EXC_SUBSCRIBE.to_le_bytes());
        assert_eq!(&code[376..378], &[0xCD, 0x80]);
        assert_eq!(&code[378..382], &[0x48, 0x89, 0x41, 0x60]); // [rcx+96]
        assert_eq!(&code[382..384], &[0x48, 0xBE]);
        assert_eq!(&code[384..392], &EXC_KIND_SYSCALLTRAP.to_le_bytes());
        assert_eq!(&code[392..394], &[0x48, 0xB8]);
        assert_eq!(&code[394..402], &SYS_EXC_SUBSCRIBE.to_le_bytes());
        assert_eq!(&code[402..404], &[0xCD, 0x80]);
        assert_eq!(&code[404..408], &[0x48, 0x89, 0x41, 0x68]); // [rcx+104]
        assert_eq!(&code[408..410], &[0x48, 0xBF]);
        assert_eq!(&code[410..418], &EXC_SYSCALL_TRAP_CAP.to_le_bytes());
        assert_eq!(&code[418..420], &[0x48, 0xBE]);
        assert_eq!(&code[420..428], &EXC_KIND_SYSCALLTRAP.to_le_bytes());
        assert_eq!(&code[428..430], &[0x48, 0xB8]);
        assert_eq!(&code[430..438], &SYS_EXC_SUBSCRIBE.to_le_bytes());
        assert_eq!(&code[438..440], &[0xCD, 0x80]);
        assert_eq!(&code[440..444], &[0x48, 0x89, 0x41, 0x70]); // [rcx+112]
        assert_eq!(&code[444..446], &[0x48, 0xBF]);
        assert_eq!(&code[446..454], &EXC_PAGE_FAULT_CAP.to_le_bytes());
        assert_eq!(&code[454..456], &[0x48, 0xB8]);
        assert_eq!(&code[456..464], &SYS_EXC_UNSUBSCRIBE.to_le_bytes());
        assert_eq!(&code[464..466], &[0xCD, 0x80]);
        assert_eq!(&code[466..470], &[0x48, 0x89, 0x41, 0x78]); // [rcx+120]
        assert_eq!(&code[470..472], &[0x48, 0xB8]);
        assert_eq!(&code[472..480], &SYS_EXC_UNSUBSCRIBE.to_le_bytes());
        assert_eq!(&code[480..482], &[0xCD, 0x80]);
        assert_eq!(&code[482..489], &[0x48, 0x89, 0x81, 0x80, 0, 0, 0]); // [rcx+128] disp32
        // 快路径：syscall 指令调用 ECHO，再重载 rcx 写 Y+56。
        assert_eq!(&code[489..491], &[0x48, 0xBF]);
        assert_eq!(&code[491..499], &FAST_ARG.to_le_bytes());
        assert_eq!(&code[499..501], &[0x48, 0xB8]);
        assert_eq!(&code[501..509], &SYS_ECHO.to_le_bytes());
        assert_eq!(&code[509..511], &[0x0F, 0x05]); // syscall
        assert_eq!(&code[511..513], &[0x48, 0xB9]); // mov rcx, imm64
        assert_eq!(&code[513..521], &USER_Y_VA.to_le_bytes());
        assert_eq!(&code[521..525], &[0x48, 0x89, 0x41, 0x38]); // mov [rcx+56], rax
        // 阶段二：task id → yield → 恢复后再取 task id → int3。
        assert_eq!(&code[525..527], &[0x48, 0xB8]);
        assert_eq!(&code[527..535], &SYS_TASK_ID.to_le_bytes());
        assert_eq!(&code[535..537], &[0xCD, 0x80]);
        assert_eq!(&code[537..544], &[0x48, 0x89, 0x81, 0x88, 0, 0, 0]); // [rcx+136] disp32
        assert_eq!(&code[544..546], &[0x48, 0xB8]);
        assert_eq!(&code[546..554], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[554..556], &[0xCD, 0x80]);
        assert_eq!(&code[556..563], &[0x48, 0x89, 0x81, 0x90, 0, 0, 0]); // [rcx+144] disp32
        assert_eq!(&code[563..565], &[0x48, 0xB8]);
        assert_eq!(&code[565..573], &SYS_TASK_ID.to_le_bytes());
        assert_eq!(&code[573..575], &[0xCD, 0x80]);
        assert_eq!(&code[575..582], &[0x48, 0x89, 0x81, 0x98, 0, 0, 0]); // [rcx+152] disp32
        // 阶段三：阻塞等待 task 2 触发的通知。
        assert_eq!(&code[582..584], &[0x48, 0xBF]);
        assert_eq!(&code[584..592], &EXC_SYSCALL_TRAP_CAP.to_le_bytes());
        assert_eq!(&code[592..594], &[0x48, 0xBE]);
        assert_eq!(&code[594..602], &EXC_KIND_SYSCALLTRAP.to_le_bytes());
        assert_eq!(&code[602..604], &[0x48, 0xB8]);
        assert_eq!(&code[604..612], &SYS_EXC_WAIT.to_le_bytes());
        assert_eq!(&code[612..614], &[0xCD, 0x80]);
        assert_eq!(&code[614..621], &[0x48, 0x89, 0x81, 0xa0, 0, 0, 0]); // [rcx+160] disp32
        // S2-D2 能力生命周期（回执 Y+168..Y+232，全部 disp32）。
        assert_eq!(&code[621..623], &[0x48, 0xBF]);
        assert_eq!(&code[623..631], &AUTHORIZE_CAP.to_le_bytes());
        assert_eq!(&code[631..633], &[0x48, 0xBE]);
        assert_eq!(&code[633..641], &RIGHTS_DERIVE_BITS.to_le_bytes());
        assert_eq!(&code[641..643], &[0x48, 0xB8]);
        assert_eq!(&code[643..651], &SYS_CAP_DERIVE.to_le_bytes());
        assert_eq!(&code[651..653], &[0xCD, 0x80]);
        assert_eq!(&code[653..660], &[0x48, 0x89, 0x81, 168, 0, 0, 0]);
        assert_eq!(&code[670..672], &[0x48, 0xBE]);
        assert_eq!(&code[672..680], &RIGHTS_SEGMAPW_BITS.to_le_bytes());
        assert_eq!(&code[692..699], &[0x48, 0x89, 0x81, 176, 0, 0, 0]);
        assert_eq!(&code[699..701], &[0x48, 0xBF]);
        assert_eq!(&code[701..709], &CAP_CHILD1_HANDLE.to_le_bytes());
        assert_eq!(&code[709..711], &[0x48, 0xB8]);
        assert_eq!(&code[711..719], &SYS_CAP_DROP.to_le_bytes());
        assert_eq!(&code[721..728], &[0x48, 0x89, 0x81, 184, 0, 0, 0]);
        assert_eq!(&code[738..740], &[0x48, 0xB8]);
        assert_eq!(&code[740..748], &SYS_CAP_DROP.to_le_bytes());
        assert_eq!(&code[750..757], &[0x48, 0x89, 0x81, 192, 0, 0, 0]);
        assert_eq!(&code[777..779], &[0x48, 0xB8]);
        assert_eq!(&code[779..787], &SYS_CAP_DERIVE.to_le_bytes());
        assert_eq!(&code[789..796], &[0x48, 0x89, 0x81, 200, 0, 0, 0]);
        assert_eq!(&code[796..798], &[0x48, 0xBF]);
        assert_eq!(&code[798..806], &CAP_CHILD1_HANDLE.to_le_bytes());
        assert_eq!(&code[806..808], &[0x48, 0xBE]);
        assert_eq!(&code[808..816], &RIGHT_DERIVE.to_le_bytes());
        assert_eq!(&code[816..818], &[0x48, 0xB8]);
        assert_eq!(&code[818..826], &SYS_AUTHORIZE.to_le_bytes());
        assert_eq!(&code[828..835], &[0x48, 0x89, 0x81, 208, 0, 0, 0]);
        assert_eq!(&code[835..837], &[0x48, 0xBF]);
        assert_eq!(&code[837..845], &CAP_CHILD2_HANDLE.to_le_bytes());
        assert_eq!(&code[867..874], &[0x48, 0x89, 0x81, 216, 0, 0, 0]);
        assert_eq!(&code[874..876], &[0x48, 0xBF]);
        assert_eq!(&code[876..884], &CAP_CHILD2_HANDLE.to_le_bytes());
        assert_eq!(&code[884..886], &[0x48, 0xB8]);
        assert_eq!(&code[886..894], &SYS_CAP_DROP.to_le_bytes());
        assert_eq!(&code[896..903], &[0x48, 0x89, 0x81, 224, 0, 0, 0]);
        assert_eq!(&code[903..905], &[0x48, 0xBF]);
        assert_eq!(
            &code[905..913],
            &NAMESPACE_CAP_HANDLE.encode().to_le_bytes()
        );
        assert_eq!(&code[913..915], &[0x48, 0xB8]);
        assert_eq!(&code[915..923], &SYS_CAP_DROP.to_le_bytes());
        assert_eq!(&code[925..932], &[0x48, 0x89, 0x81, 232, 0, 0, 0]);
        assert_eq!(code[932], 0x90); // NOP 对齐
        // S2-D3 Segment 原语（回执 Y+240..Y+328，全部 disp32）。
        assert_eq!(&code[933..943], &[0x48, 0xBF, 1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&code[953..955], &[0x48, 0xB8]);
        assert_eq!(&code[955..963], &SYS_MEMSEG_CREATE.to_le_bytes());
        assert_eq!(&code[963..965], &[0xCD, 0x80]);
        assert_eq!(&code[965..972], &[0x48, 0x89, 0x81, 240, 0, 0, 0]);
        assert_eq!(&code[972..974], &[0x48, 0xBF]);
        assert_eq!(&code[974..982], &MEMSEG_CAP1_HANDLE.to_le_bytes());
        assert_eq!(&code[1032..1034], &[0x48, 0xB8]);
        assert_eq!(&code[1034..1042], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[1051..1053], &[0x48, 0xBA]);
        assert_eq!(&code[1053..1061], &USER_SEG_VA.to_le_bytes());
        assert_eq!(&code[1061..1064], &[0x48, 0x8B, 0x02]); // mov rax, [rdx]
        assert_eq!(&code[1064..1071], &[0x48, 0x89, 0x81, 0, 1, 0, 0]); // [rcx+256] disp32
        assert_eq!(&code[1091..1094], &[0x48, 0x89, 0x02]); // mov [rdx], rax
        assert_eq!(&code[1134..1136], &[0x48, 0xB8]);
        assert_eq!(&code[1136..1144], &SYS_MEMSEG_UNMAP.to_le_bytes());
        assert_eq!(&code[1163..1165], &[0x48, 0xB8]);
        assert_eq!(&code[1165..1173], &SYS_MEMSEG_DESTROY.to_le_bytes());
        assert_eq!(&code[1242..1244], &[0x48, 0xB8]);
        assert_eq!(&code[1244..1252], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[1254..1261], &[0x48, 0x89, 0x81, 32, 1, 0, 0]); // [rcx+288] disp32
        assert_eq!(&code[1281..1283], &[0x48, 0xB8]);
        assert_eq!(&code[1283..1291], &SYS_MEMSEG_CREATE.to_le_bytes());
        assert_eq!(&code[1293..1300], &[0x48, 0x89, 0x81, 40, 1, 0, 0]); // [rcx+296] disp32
        assert_eq!(&code[1300..1302], &[0x48, 0xBF]);
        assert_eq!(&code[1302..1310], &MEMSEG_CAP2_HANDLE.to_le_bytes());
        assert_eq!(&code[1372..1379], &[0x48, 0x89, 0x81, 48, 1, 0, 0]); // [rcx+304] disp32
        assert_eq!(&code[1392..1399], &[0x48, 0x89, 0x81, 56, 1, 0, 0]); // [rcx+312] disp32
        assert_eq!(&code[1458..1460], &[0x48, 0xB8]);
        assert_eq!(&code[1460..1468], &SYS_MEMSEG_DESTROY.to_le_bytes());
        assert_eq!(&code[1470..1477], &[0x48, 0x89, 0x81, 72, 1, 0, 0]); // [rcx+328] disp32
        // 切片 S：共享 Segment —— create → map → 写魔数 → 排队路径送 cap → yield →
        //           直投路径送无 cap 消息 → yield → int3。
        assert_eq!(&code[1477..1479], &[0x48, 0xBF]);
        assert_eq!(&code[1479..1487], &1u64.to_le_bytes()); // page_count
        assert_eq!(&code[1497..1499], &[0x48, 0xB8]);
        assert_eq!(&code[1499..1507], &SYS_MEMSEG_CREATE.to_le_bytes());
        assert_eq!(&code[1507..1509], &[0xCD, 0x80]);
        assert_eq!(&code[1509..1516], &[0x48, 0x89, 0x81, 0x50, 0x01, 0, 0]); // [Y+336]
        assert_eq!(&code[1516..1523], &[0x48, 0x8B, 0xB9, 0x50, 0x01, 0, 0]); // rdi = [Y+336]
        assert_eq!(&code[1525..1533], &TASK_CAP_HANDLE.encode().to_le_bytes());
        assert_eq!(&code[1545..1553], &USER_SEG_SHARED_VA.to_le_bytes());
        assert_eq!(&code[1583..1585], &[0xCD, 0x80]);
        assert_eq!(&code[1585..1592], &[0x48, 0x89, 0x81, 0x58, 0x01, 0, 0]); // [Y+344]
        assert_eq!(&code[1594..1602], &USER_SEG_SHARED_VA.to_le_bytes());
        assert_eq!(&code[1604..1612], &IPC_SHARED_MAGIC.to_le_bytes());
        assert_eq!(&code[1612..1615], &[0x48, 0x89, 0x02]); // mov [rdx], rax
        assert_eq!(&code[1617..1625], &2u64.to_le_bytes()); // receiver
        assert_eq!(&code[1627..1635], &TASK2_SEND_CAP.to_le_bytes());
        assert_eq!(&code[1637..1645], &IPC_SHARED_MAGIC.to_le_bytes()); // payload
        assert_eq!(&code[1655..1662], &[0x4C, 0x8B, 0x81, 0x50, 0x01, 0, 0]); // r8 = [Y+336]
        assert_eq!(&code[1674..1682], &SYS_SEND.to_le_bytes());
        assert_eq!(&code[1684..1691], &[0x48, 0x89, 0x81, 0x60, 0x01, 0, 0]); // [Y+352]
        assert_eq!(&code[1693..1701], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[1701..1703], &[0xCD, 0x80]);
        assert_eq!(&code[1725..1733], &IPC_DIRECT_PAYLOAD.to_le_bytes());
        assert_eq!(&code[1765..1773], &SYS_SEND.to_le_bytes());
        assert_eq!(&code[1775..1782], &[0x48, 0x89, 0x81, 0x68, 0x01, 0, 0]); // [Y+360]
        assert_eq!(&code[1784..1792], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[1792..1794], &[0xCD, 0x80]);
        // S2-D4：请求块建在用户栈页 TASK_REQ_VA（rcx=Y 回执，rdx=请求块）。
        assert_eq!(&code[1794..1796], &[0x48, 0xB9]);
        assert_eq!(&code[1796..1804], &USER_Y_VA.to_le_bytes());
        assert_eq!(&code[1804..1806], &[0x48, 0xBA]);
        assert_eq!(&code[1806..1814], &TASK_REQ_VA.to_le_bytes());
        assert_eq!(&code[1816..1824], &TASK_CODE_CAP.to_le_bytes());
        assert_eq!(&code[1824..1827], &[0x48, 0x89, 0x02]); // [rdx+0] = code_seg_cap
        assert_eq!(&code[1837..1841], &[0x48, 0x89, 0x42, 0x08]); // code_offset = 0
        assert_eq!(
            &code[1843..1851],
            &(layout.alpha.pages as u64).to_le_bytes()
        );
        assert_eq!(&code[1851..1855], &[0x48, 0x89, 0x42, 0x10]); // code_pages
        assert_eq!(&code[1857..1865], &USER3_CODE_VA.to_le_bytes());
        assert_eq!(&code[1865..1869], &[0x48, 0x89, 0x42, 0x18]); // load_base
        assert_eq!(
            &code[1871..1879],
            &(USER3_CODE_VA + layout.alpha.entry).to_le_bytes()
        );
        assert_eq!(&code[1879..1883], &[0x48, 0x89, 0x42, 0x20]); // entry
        assert_eq!(
            &code[1885..1893],
            &(USER3_STACK_VA + FRAME_SIZE).to_le_bytes()
        );
        assert_eq!(&code[1893..1897], &[0x48, 0x89, 0x42, 0x28]); // stack_top
        assert_eq!(
            &code[1899..1907],
            &NAMESPACE_CAP_HANDLE.encode().to_le_bytes()
        );
        assert_eq!(&code[1907..1911], &[0x48, 0x89, 0x42, 0x30]); // parent_ns_cap
        assert_eq!(
            &code[1913..1921],
            &(TASK_CREATE_ROOT_NS_RIGHTS.bits() as u64).to_le_bytes()
        );
        assert_eq!(&code[1921..1925], &[0x48, 0x89, 0x42, 0x38]); // root_rights
        assert_eq!(&code[1927..1935], &TASK_SEND_CAP.to_le_bytes());
        assert_eq!(&code[1935..1939], &[0x48, 0x89, 0x42, 0x40]); // initial_cap0
        assert_eq!(&code[1949..1953], &[0x48, 0x89, 0x42, 0x48]); // initial_cap1 = 0
        // 栈段 = SYS_MEMSEG_CREATE(1, R|W) → [Y+368]
        assert_eq!(&code[1963..1965], &[0x48, 0xBE]);
        assert_eq!(&code[1965..1973], &MEMSEG_RW_BITS.to_le_bytes());
        assert_eq!(&code[1975..1983], &SYS_MEMSEG_CREATE.to_le_bytes());
        assert_eq!(&code[1983..1985], &[0xCD, 0x80]);
        assert_eq!(&code[1985..1992], &[0x48, 0x89, 0x81, 0x70, 0x01, 0, 0]);
        // mgr = SYS_TASK_CREATE(req) → [Y+376]
        assert_eq!(&code[1992..1995], &[0x48, 0x89, 0xD7]); // mov rdi, rdx
        assert_eq!(&code[1997..2005], &SYS_TASK_CREATE.to_le_bytes());
        assert_eq!(&code[2005..2007], &[0xCD, 0x80]);
        assert_eq!(&code[2007..2014], &[0x48, 0x89, 0x81, 0x78, 0x01, 0, 0]);
        // 负例 1：entry 越界 → [Y+384]；随后还原 entry。
        assert_eq!(
            &code[2016..2024],
            &(USER3_CODE_VA + layout.alpha.pages as u64 * FRAME_SIZE).to_le_bytes()
        );
        assert_eq!(&code[2024..2028], &[0x48, 0x89, 0x42, 0x20]);
        assert_eq!(&code[2043..2050], &[0x48, 0x89, 0x81, 0x80, 0x01, 0, 0]);
        assert_eq!(&code[2060..2064], &[0x48, 0x89, 0x42, 0x20]);
        // 负例 2：code_seg_cap 换成栈段 cap（缺 SegMapX）→ [Y+392]；随后还原。
        assert_eq!(&code[2064..2071], &[0x48, 0x8B, 0x81, 0x70, 0x01, 0, 0]);
        assert_eq!(&code[2071..2074], &[0x48, 0x89, 0x02]);
        assert_eq!(&code[2089..2096], &[0x48, 0x89, 0x81, 0x88, 0x01, 0, 0]);
        assert_eq!(&code[2098..2106], &TASK_CODE_CAP.to_le_bytes());
        // 栈映射进子任务根 → [Y+400]
        assert_eq!(&code[2109..2116], &[0x48, 0x8B, 0xB9, 0x70, 0x01, 0, 0]);
        assert_eq!(&code[2116..2123], &[0x48, 0x8B, 0xB1, 0x78, 0x01, 0, 0]);
        assert_eq!(&code[2135..2143], &USER3_STACK_VA.to_le_bytes());
        assert_eq!(&code[2165..2173], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[2175..2182], &[0x48, 0x89, 0x81, 0x90, 0x01, 0, 0]);
        // 两次让出：先排空 Ready 的 task 2，再轮到子任务。
        assert_eq!(&code[2184..2192], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[2196..2204], &SYS_YIELD.to_le_bytes());
        // recv → [Y+408]
        assert_eq!(&code[2208..2216], &TASK_RECV_BUF.to_le_bytes());
        assert_eq!(&code[2218..2226], &SYS_RECV.to_le_bytes());
        assert_eq!(&code[2228..2235], &[0x48, 0x89, 0x81, 0x98, 0x01, 0, 0]);
        // 缺 TaskDestroy 的销毁负例 → [Y+416]；销毁成功 → [Y+424]；重复销毁 → [Y+432]。
        assert_eq!(&code[2244..2252], &SYS_TASK_DESTROY.to_le_bytes());
        assert_eq!(&code[2254..2261], &[0x48, 0x89, 0x81, 0xA0, 0x01, 0, 0]);
        assert_eq!(&code[2270..2278], &SYS_TASK_DESTROY.to_le_bytes());
        assert_eq!(&code[2280..2287], &[0x48, 0x89, 0x81, 0xA8, 0x01, 0, 0]);
        assert_eq!(&code[2296..2304], &SYS_TASK_DESTROY.to_le_bytes());
        assert_eq!(&code[2306..2313], &[0x48, 0x89, 0x81, 0xB0, 0x01, 0, 0]);
        // 向已销毁任务映射 → [Y+440]
        assert_eq!(&code[2369..2377], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[2379..2386], &[0x48, 0x89, 0x81, 0xB8, 0x01, 0, 0]);
        // 唤醒 task 2（收尾阶段需要 Ready 任务可轮转）。
        assert_eq!(&code[2398..2406], &TASK2_SEND_CAP.to_le_bytes());
        assert_eq!(&code[2408..2416], &TASK2_WAKE_PAYLOAD.to_le_bytes());
        assert_eq!(&code[2448..2456], &SYS_SEND.to_le_bytes());
        assert_eq!(&code[2456..2458], &[0xCD, 0x80]);
        // SPEC-066：自持平台 cap 映射块。
        // 正例：rdi = X 段 cap（槽 36），rsi = Task(1) 管理 cap（槽 45）。
        assert_eq!(
            &code[2460..2468],
            &crate::bootstrap::platform_segment_cap_handle(6)
                .encode()
                .to_le_bytes()
        );
        assert_eq!(
            &code[2470..2478],
            &crate::bootstrap::PLATFORM_TASK1_CAP_HANDLE
                .encode()
                .to_le_bytes()
        );
        assert_eq!(&code[2490..2498], &USER_MAP_WITNESS_VA.to_le_bytes());
        assert_eq!(&code[2520..2528], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[2528..2530], &[0xCD, 0x80]);
        assert_eq!(&code[2530..2537], &[0x48, 0x89, 0x81, 0xC0, 0x01, 0, 0]);
        assert_eq!(&code[2539..2547], &USER_MAP_WITNESS_VA.to_le_bytes());
        assert_eq!(&code[2547..2550], &[0x48, 0x8B, 0x02]); // mov rax, [rdx]
        assert_eq!(&code[2550..2557], &[0x48, 0x89, 0x81, 0xC8, 0x01, 0, 0]);
        // 负例：rdi = task 2 所属的 TASK2_CODE 段 cap（槽 38）。
        assert_eq!(
            &code[2559..2567],
            &crate::bootstrap::platform_segment_cap_handle(8)
                .encode()
                .to_le_bytes()
        );
        assert_eq!(&code[2589..2597], &USER_MAP_WITNESS_VA2.to_le_bytes());
        assert_eq!(&code[2619..2627], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[2629..2636], &[0x48, 0x89, 0x81, 0xD0, 0x01, 0, 0]);
        // SPEC-067：task4 自毁见证块（rdx 重设为请求块，改写 entry 与 initial_cap0）。
        assert_eq!(&code[2638..2646], &TASK_REQ_VA.to_le_bytes());
        assert_eq!(
            &code[2648..2656],
            &(USER3_CODE_VA + TASK4_CODE_OFFSET).to_le_bytes()
        );
        assert_eq!(&code[2656..2660], &[0x48, 0x89, 0x42, 0x20]);
        assert_eq!(
            &code[2662..2670],
            &crate::bootstrap::TASK4_SEND_CAP_HANDLE
                .encode()
                .to_le_bytes()
        );
        assert_eq!(&code[2670..2674], &[0x48, 0x89, 0x42, 0x40]);
        assert_eq!(&code[2679..2687], &SYS_TASK_CREATE.to_le_bytes());
        assert_eq!(&code[2689..2696], &[0x48, 0x89, 0x81, 0xD8, 0x01, 0, 0]);
        assert_eq!(&code[2722..2730], &USER3_STACK_VA.to_le_bytes());
        assert_eq!(&code[2752..2760], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[2762..2769], &[0x48, 0x89, 0x81, 0xE0, 0x01, 0, 0]);
        assert_eq!(&code[2771..2779], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[2783..2791], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[2795..2803], &(TASK_RECV_BUF + 0x20).to_le_bytes());
        assert_eq!(&code[2805..2813], &SYS_RECV.to_le_bytes());
        assert_eq!(&code[2815..2822], &[0x48, 0x89, 0x81, 0xE8, 0x01, 0, 0]);
        // 让出一次，等 task 4 完成自毁。
        assert_eq!(&code[2824..2832], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[2832..2834], &[0xCD, 0x80]);
        assert_eq!(&code[2860..2868], &USER3_STACK_VA.to_le_bytes());
        assert_eq!(&code[2890..2898], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[2900..2907], &[0x48, 0x89, 0x81, 0xF0, 0x01, 0, 0]);
        // 抢占阶段前再唤醒一次 task 2（第一次让出已把它排空）。
        assert_eq!(&code[2909..2917], &2u64.to_le_bytes());
        // SPEC-081：Namespace 块从 2979 起（`mov rcx, Y`），路径文本位于 Y 页。
        assert_eq!(&code[2979..2981], &[0x48, 0xB9]);
        assert_eq!(
            &NS_PATH_BYTES[NS_PATH_MNTSUB_OFF as usize..NS_PATH_MNTSUB_OFF as usize + 7],
            b"mnt/sub"
        );
        assert_eq!(NS_PATH_DATA_VA, USER_Y_VA + NS_PATH_DATA_OFFSET as u64);
        assert!(NS_PATH_DATA_OFFSET + NS_PATH_BYTES.len() <= FRAME_SIZE as usize);
        // 块以 `int3` 收尾：取区间内**最后**一个 0xCC（其后到页末之间是零填充，
        // 故它就是收尾的 `int3`，而机器码立即数里可能出现的 0xCC 都在它之前）。
        let int3 = (2979..USER_CODE_LEN)
            .rfind(|index| code[*index] == 0xCC)
            .expect("SPEC-081/082 块必须以 int3 收尾");
        assert!((int3 + 1..USER_CODE_LEN).all(|index| code[index] == 0));
    }
}
