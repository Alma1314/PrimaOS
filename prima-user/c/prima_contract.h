/* Static program declaration. These fields are not kernel authorization. */
#ifndef PRIMA_CONTRACT_H
#define PRIMA_CONTRACT_H
#include <stdint.h>
#define PRIMA_CONTRACT_NOTE_TYPE UINT32_C(0x5052494d)
#define PRIMA_CONTRACT_WORDS \
    {6, 24, PRIMA_CONTRACT_NOTE_TYPE, UINT32_C(0x6d697250), 0x61, 1, 1, 0, 1, 1, 0}
extern const volatile uint32_t prima_libc_contract_v1_0[11];

static inline uint32_t prima_contract_u32(const unsigned char *p) {
    return (uint32_t)p[0] | (uint32_t)p[1]<<8 | (uint32_t)p[2]<<16 | (uint32_t)p[3]<<24;
}
static inline uint64_t prima_contract_u64(const unsigned char *p) {
    return prima_contract_u32(p) | (uint64_t)prima_contract_u32(p+4)<<32;
}

/* Bounds precede every offset calculation; ELF integers may be unaligned. */
static inline int prima_contract_accepts(const void *image, uint64_t length) {
    const unsigned char *elf = image;
    if (!elf || length < 64 || elf[0]!=127 || elf[1]!='E' || elf[2]!='L' || elf[3]!='F' ||
        elf[4]!=2 || elf[5]!=1 || elf[6]!=1 || elf[18]!=62 || elf[19] ||
        elf[54]!=56 || elf[55]) return 0;
    uint64_t phoff = prima_contract_u64(elf+32);
    unsigned count = elf[56] | (unsigned)elf[57]<<8;
    if (phoff > length || count > (length-phoff)/56) return 0;
    int found = 0;
    for (unsigned i=0; i<count; i++) {
        const unsigned char *ph = elf+phoff+(uint64_t)i*56;
        if (prima_contract_u32(ph)!=4) continue;
        uint64_t start = prima_contract_u64(ph+8), size = prima_contract_u64(ph+32);
        if (start > length || size > length-start) return 0;
        const unsigned char *notes = elf+start;
        uint64_t at = 0;
        while (at < size) {
            if (size-at < 12) return 0;
            uint32_t namesz = prima_contract_u32(notes+at), descsz = prima_contract_u32(notes+at+4);
            uint32_t type = prima_contract_u32(notes+at+8);
            at += 12;
            uint64_t name_pad = ((uint64_t)namesz+3)&~UINT64_C(3);
            uint64_t desc_pad = ((uint64_t)descsz+3)&~UINT64_C(3);
            if (name_pad > size-at) return 0;
            int match = type==PRIMA_CONTRACT_NOTE_TYPE && namesz==6 &&
                notes[at]=='P' && notes[at+1]=='r' && notes[at+2]=='i' &&
                notes[at+3]=='m' && notes[at+4]=='a' && !notes[at+5];
            at += name_pad;
            if (desc_pad > size-at) return 0;
            if (match) {
                const uint32_t expected[6] = {1,1,0,1,1,0};
                if (found || descsz!=24) return 0;
                found = 1;
                for (unsigned field=0; field<6; field++)
                    if (prima_contract_u32(notes+at+field*4)!=expected[field]) return 0;
            }
            at += desc_pad;
        }
    }
    return found;
}
#endif
