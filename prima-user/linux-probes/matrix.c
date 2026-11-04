/* Linux x86_64 raw ABI matrix. Variable identities/addresses are predicates;
   stable results and errno are printed verbatim for the guest differential. */
#include <stdint.h>

static long raw(long n, long a, long b, long c, long d, long e, long f) {
    register long r10 __asm__("r10") = d;
    register long r8 __asm__("r8") = e;
    register long r9 __asm__("r9") = f;
    __asm__ volatile("syscall" : "+a"(n) : "D"(a), "S"(b), "d"(c), "r"(r10), "r"(r8), "r"(r9) : "rcx", "r11", "memory");
    return n;
}
#define P(value) ((long)(uintptr_t)(value))
#define S(n,a,b,c,d,e,f) raw(n,P(a),P(b),P(c),P(d),P(e),P(f))
static void text(const char *s) { long n = 0; while (s[n]) ++n; S(1,1,s,n,0,0,0); }
static long observe(const char *name, long value) {
    char bytes[24];
    unsigned long magnitude = value < 0 ? 0UL - (unsigned long)value : (unsigned long)value;
    int i = 24;
    do { bytes[--i] = '0' + magnitude % 10; magnitude /= 10; } while (magnitude);
    if (value < 0) bytes[--i] = '-';
    text("M3|"); text(name); text("="); S(1,1,bytes+i,24-i,0,0,0); text("\n");
    return value;
}
#define CHECK(name, expression, expected) do { if (observe(name, (expression)) != (expected)) return 1; } while (0)
static uint64_t statbuf[18], limits[2], tls[4] = { 0x12345678 }, fs, mask, action[4] = {1};
static char buffer[4096];
static unsigned word;
static uint64_t zero_time[2];

long matrix_main(void) {
    CHECK("0.read.zero", S(0,0,0,0,0,0,0), 0);
    CHECK("0.read.ebadf", S(0,99,buffer,1,0,0,0), -9);
    CHECK("1.write.zero", S(1,1,0,0,0,0,0), 0);
    CHECK("1.write.efault", S(1,1,1,1,0,0,0), -14);
    CHECK("1.write.ebadf", S(1,99,buffer,1,0,0,0), -9);
    CHECK("20.writev.zero", S(20,1,0,0,0,0,0), 0);
    CHECK("2.open.enoent", S(2,"/missing",0,0,0,0,0), -2);
    CHECK("2.open.cloexec", S(2,"/fixture.txt",0x80000,0,0,0,0), 3);
    CHECK("72.getfd", S(72,3,1,0,0,0,0), 1);
    CHECK("72.setfd", S(72,3,2,0,0,0,0), 0);
    CHECK("72.getfd.changed", S(72,3,1,0,0,0,0), 0);
    CHECK("72.getfl.readonly", S(72,3,3,0,0,0,0) & 3, 0);
    CHECK("5.fstat", S(5,3,statbuf,0,0,0,0), 0);
    CHECK("5.fstat.size", statbuf[6], 17);
    CHECK("5.fstat.efault", S(5,3,1,0,0,0,0), -14);
    CHECK("0.read.file", S(0,3,buffer,5,0,0,0), 5);
    CHECK("0.read.bytes", buffer[0]=='h' && buffer[4]=='o', 1);
    CHECK("8.lseek", S(8,3,0,0,0,0,0), 0);
    CHECK("8.lseek.einval", S(8,3,0,99,0,0,0), -22);
    CHECK("3.close", S(3,3,0,0,0,0,0), 0);
    CHECK("3.close.ebadf", S(3,3,0,0,0,0,0), -9);
    CHECK("257.openat", S(257,-100,"/fixture.txt",0,0,0,0), 3);
    CHECK("262.newfstatat", S(262,3,"",statbuf,0x1000,0,0), 0);
    CHECK("262.newfstatat.size", statbuf[6], 17);
    CHECK("3.close.at", S(3,3,0,0,0,0,0), 0);
    CHECK("257.directory", S(257,-100,"/fixturedir",0x10000,0,0,0), 3);
    long bytes = S(217,3,buffer,sizeof buffer,0,0,0);
    long names = 0;
    for (long at=0; at<bytes;) {
        unsigned length = (unsigned char)buffer[at+16] | ((unsigned char)buffer[at+17]<<8);
        if (length < 24 || at+length > bytes) return 1;
        char *name = buffer+at+19;
        if (name[0]=='a' && name[1]=='l' && name[2]=='p' && name[3]=='h' && name[4]=='a' && name[5]==0) names |= 1;
        if (name[0]=='b' && name[1]=='e' && name[2]=='t' && name[3]=='a' && name[4]==0) names |= 2;
        at += length;
    }
    CHECK("217.getdents.names", names, 3);
    CHECK("3.close.dir", S(3,3,0,0,0,0,0), 0);
    long mapping = S(9,0,8192,3,0x22,-1,0);
    CHECK("9.mmap.address", mapping > 0, 1);
    volatile unsigned char *memory = (void *)(uintptr_t)mapping;
    CHECK("9.mmap.zero", memory[0]==0 && memory[8191]==0, 1);
    memory[0] = 0xa5;
    CHECK("10.mprotect.r", S(10,mapping,8192,1,0,0,0), 0);
    CHECK("10.mprotect.none", S(10,mapping,8192,0,0,0,0), 0);
    CHECK("10.mprotect.restore", S(10,mapping,8192,3,0,0,0), 0);
    CHECK("10.mprotect.retained", memory[0], 0xa5);
    CHECK("10.mprotect.einval", S(10,mapping+1,4096,1,0,0,0), -22);
    CHECK("11.munmap.einval", S(11,mapping+1,8192,0,0,0,0), -22);
    CHECK("11.munmap", S(11,mapping,8192,0,0,0,0), 0);
    long heap = S(12,0,0,0,0,0,0);
    CHECK("12.brk.query", heap > 0, 1);
    CHECK("12.brk.grow", S(12,heap+4096,0,0,0,0,0)==heap+4096, 1);
    *(volatile char *)(uintptr_t)heap = 7;
    CHECK("12.brk.write", *(volatile char *)(uintptr_t)heap, 7);
    CHECK("16.ioctl.enotty", S(16,0,0x5401,buffer,0,0,0), -25);
    long pid = S(39,0,0,0,0,0,0);
    CHECK("39.getpid.identity", pid > 0, 1);
    CHECK("186.gettid.identity", S(186,0,0,0,0,0,0)==pid, 1);
    CHECK("110.getppid.identity", S(110,0,0,0,0,0,0)>0, 1);
    CHECK("62.kill.probe", S(62,pid,0,0,0,0,0), 0);
    CHECK("234.tgkill.probe", S(234,pid,pid,0,0,0,0), 0);
    CHECK("13.sigaction.ignore", S(13,10,action,0,8,0,0), 0);
    CHECK("13.sigaction.einval", S(13,9,action,0,8,0,0), -22);
    mask = 1<<9;
    CHECK("14.sigprocmask.block", S(14,0,&mask,0,8,0,0), 0);
    CHECK("14.sigprocmask.query", S(14,0,0,&mask,8,0,0), 0);
    CHECK("14.sigprocmask.mask", mask, 1<<9);
    CHECK("14.sigprocmask.unblock", S(14,1,&mask,0,8,0,0), 0);
    CHECK("63.uname", S(63,buffer,0,0,0,0,0), 0);
    CHECK("63.uname.sysname", buffer[0]=='L' && buffer[4]=='x' && buffer[5]==0, 1);
    CHECK("79.getcwd", S(79,buffer,sizeof buffer,0,0,0,0), 2);
    CHECK("79.getcwd.root", buffer[0]=='/' && buffer[1]==0, 1);
    CHECK("89.readlink.present", S(89,"/proc/self/exe",buffer,sizeof buffer,0,0,0)>0, 1);
    CHECK("102.getuid", S(102,0,0,0,0,0,0), 0);
    CHECK("104.getgid", S(104,0,0,0,0,0,0), 0);
    CHECK("107.geteuid", S(107,0,0,0,0,0,0), 0);
    CHECK("108.getegid", S(108,0,0,0,0,0,0), 0);
    CHECK("158.arch_prctl.set", S(158,0x1002,tls,0,0,0,0), 0);
    CHECK("158.arch_prctl.get", S(158,0x1003,&fs,0,0,0,0), 0);
    CHECK("158.arch_prctl.identity", fs == (uintptr_t)tls, 1);
    unsigned long tls_value;
    __asm__ volatile("movq %%fs:0,%0" : "=r"(tls_value));
    CHECK("158.arch_prctl.fsload", tls_value, 0x12345678);
    CHECK("158.arch_prctl.reset", S(158,0x1002,0,0,0,0,0), 0);
    CHECK("218.set_tid_address", S(218,&word,0,0,0,0,0)==pid, 1);
    CHECK("202.futex.wake.empty", S(202,&word,129,1,0,0,0), 0);
    CHECK("202.futex.eagain", S(202,&word,128,1,0,0,0), -11);
    CHECK("202.futex.einval", S(202,(char *)&word+1,128,0,0,0,0), -22);
    CHECK("202.futex.efault", S(202,0,128,0,0,0,0), -14);
    CHECK("202.futex.timeout.zero", S(202,&word,128,0,zero_time,0,0), -110);
    CHECK("24.yield", S(24,0,0,0,0,0,0), 0);
    CHECK("302.prlimit.stack", S(302,0,3,0,limits,0,0), 0);
    CHECK("302.prlimit.bytes", limits[0]==32768 && limits[1]==32768, 1);
    CHECK("unknown.enosys", S(0x7fffffff,-1,-1,-1,-1,-1,-1), -38);
    return 0;
}
__asm__(".global _start\n_start:\nxor %ebp,%ebp\nandq $-16,%rsp\ncall matrix_main\nmov %rax,%rdi\nmov $231,%eax\nsyscall\nud2\n");
