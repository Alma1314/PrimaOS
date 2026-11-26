/* Same-source libc execve(path), envp, CLOEXEC and failure atomicity. */
#define _GNU_SOURCE
#include <errno.h>
#include <stdint.h>
#include <unistd.h>
#include <fcntl.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <signal.h>
#include <termios.h>
#ifndef MFD_EXEC
#define MFD_EXEC 0x0010U
#endif
#include "prima_contract.h"
#ifndef PRIMA_HOST
#define PRIMA_ABI_NO_IOVEC
#include "prima_syscall.h"
#endif
extern const unsigned char prima_exec_child_start[], prima_exec_child_end[];
static int failures, first_error = 0, first_field = 1;
static unsigned long text_len(const char *t){unsigned long n=0;while(t[n])n++;return n;}
static void emit(const char *t){ssize_t i=write(1,t,text_len(t));(void)i;}
static void emit_dec(long v){char d[20];int i=20;unsigned long m=v<0?(unsigned long)(-v):(unsigned long)v;do{d[--i]=(char)('0'+m%10);m/=10;}while(m);if(v<0)d[--i]='-';ssize_t g=write(1,&d[i],(unsigned long)(20-i));(void)g;}
static void field(const char *n,int p,long e){emit(first_field?"|":";");first_field=0;emit(n);emit("=");emit_dec(p?1:0);if(!p){failures=1;if(!first_error)first_error=(int)e;}}

/* Copy this position-independent body into a compact static ELF. The Linux
 * body probes descriptor syscalls; Prima probes the imported fd view and the
 * authenticated OFD service. Both reject absent envp or a surviving CLOEXEC fd. */
extern const unsigned char exec_body_start[], exec_body_end[];
#ifdef PRIMA_HOST
#define EXEC_FD_BODY \
    "mov $3, %edi\nmov $1, %esi\nmov $72, %eax\nsyscall\ntest %rax, %rax\njs exec_fd_bad\n" \
    "mov $4, %edi\nmov $1, %esi\nmov $72, %eax\nsyscall\ncmp $-9, %rax\njne exec_fd_bad\n"
#define EXEC_EXIT "mov $60, %eax\nsyscall\nud2\n"
#else
#define EXEC_FD_BODY \
    "test %r15, %r15\njz exec_fd_bad\ncmpb $3, 108(%r15)\njne exec_fd_bad\n" \
    "cmpb $0, 112(%r15)\njne exec_fd_bad\ncmpq $1, 4096(%r15)\njne exec_fd_bad\n" \
    "sub $48, %rsp\nmov 4120(%r15), %rbp\nmovabs $0x500000000000, %rax\nxor %rax, %rbp\n" \
    "mov 4104(%r15), %rdi\nmov 4112(%r15), %rsi\nmov %rbp, %rdx\nxor %r10d, %r10d\n" \
    "mov $17, %eax\nsyscall\ntest %rax, %rax\njs exec_fd_bad\n" \
    "mov %rsp, %rdi\nmov $60, %eax\nsyscall\nmov 24(%rsp), %rdx\ncmp 4136(%r15), %rdx\njne exec_fd_bad\n" \
    "shr $32, %rax\nand $255, %eax\ncmp $9, %eax\njne exec_fd_bad\n" \
    "movabs $0x70000000000, %rax\nxor %rax, %rbp\n" \
    "mov 4104(%r15), %rdi\nmov 4112(%r15), %rsi\nmov %rbp, %rdx\nxor %r10d, %r10d\n" \
    "mov $17, %eax\nsyscall\ntest %rax, %rax\njs exec_fd_bad\n" \
    "mov %rsp, %rdi\nmov $60, %eax\nsyscall\nmov 24(%rsp), %rdx\ncmp 4136(%r15), %rdx\njne exec_fd_bad\n" \
    "shr $32, %rax\nand $255, %eax\njnz exec_fd_bad\n"
#define EXEC_EXIT "mov $54, %eax\nsyscall\nud2\n"
#endif
__asm__(".pushsection .text.exec_body,\"ax\",@progbits\n"
        ".global exec_body_start, exec_body_end\nexec_body_start:\n"
        "mov (%rsp), %r14\nmov 32(%rsp), %rax\ntest %rax, %rax\njz exec_env_bad\n"
        "cmpl $0x43455845, (%rax)\njne exec_env_bad\n"
        EXEC_FD_BODY
        "mov %r14, %rdi\n" EXEC_EXIT
        "exec_env_bad:\nmov $97, %edi\n" EXEC_EXIT
        "exec_fd_bad:\nmov $98, %edi\n" EXEC_EXIT
        "exec_body_end:\n.popsection\n");
static unsigned char image[768];
static void put16(unsigned off, uint16_t n) { image[off]=n; image[off+1]=n>>8; }
static void put64(unsigned off, uint64_t n) { for (unsigned i=0;i<8;i++) image[off+i]=n>>(8*i); }
static unsigned build_image(void) {
    unsigned body = (unsigned)((uintptr_t)exec_body_end-(uintptr_t)exec_body_start);
    if (body > sizeof image-224) return 0;
    memcpy(image, "\177ELF\2\1\1", 7);
    put16(16,2); put16(18,62); image[20]=1;
    put64(24,0x4000b0); put64(32,64);
    put16(52,64); put16(54,56); put16(56,2);
    image[64]=1; image[68]=5;
    put64(80,0x400000); put64(88,0x400000);
    unsigned note_at = (176+body+3)&~3U;
    put64(96,note_at+44); put64(104,4096); put64(112,4096);
    image[120]=4; put64(128,note_at); put64(152,44); put64(168,4);
    const uint32_t note[11] = PRIMA_CONTRACT_WORDS;
    memcpy(image+176,exec_body_start,body);
    memcpy(image+note_at,note,sizeof note);
    return note_at+sizeof note;
}
#ifndef PRIMA_HOST
/* A fully loadable minimal ELF isolates the incompatible-note rejection.
 * It would exit successfully if the contract guard were absent. Keeping this
 * negative fixture small avoids repeating the 3-byte VFS transport workload. */
static void incompatible_image(unsigned char out[232]) {
    memcpy(out, image, 176);
    uint64_t entry=0x4000dc, size=232, note_at=176;
    memcpy(out+24,&entry,8);
    memcpy(out+96,&size,8);
    memcpy(out+128,&note_at,8);
    const uint32_t note[11] = PRIMA_CONTRACT_WORDS;
    memcpy(out+176,note,sizeof note);
    out[208]=9; /* persona */
    const unsigned char exit_code[12]={0xbf,2,0,0,0,0xb8,54,0,0,0,0x0f,0x05};
    memcpy(out+220,exit_code,sizeof exit_code);
}
#endif
static int create_fixture(char *path, const void *data, size_t length) {
#ifdef PRIMA_HOST
    int fd = mkstemp(path);
#else
    int fd = open(path,O_CREAT|O_TRUNC|O_RDWR,0700);
#endif
    if (fd<0) return -1;
    int ok = write(fd,data,length)==(ssize_t)length && fchmod(fd,0755)==0;
    if (close(fd)) ok=0;
    return ok?0:-1;
}
static void caught_before_exec(int number) { (void)number; }
static long exec_c_image(char *const *args, char *const *env) {
    size_t length = (size_t)((uintptr_t)prima_exec_child_end - (uintptr_t)prima_exec_child_start);
#ifdef PRIMA_HOST
    int fd = memfd_create("prima-exec-crt", MFD_CLOEXEC | MFD_EXEC);
    if (fd >= 0) {
        int result = write(fd, prima_exec_child_start, length) == (ssize_t)length ? fexecve(fd, args, env) : -1;
        int saved = errno;
        close(fd);
        /* Some kernels or sandboxes deny exec of memfd; use an unlinked
         * temporary file so the witness stays descriptor-based. */
        if (saved != EACCES && saved != EPERM) return result < 0 ? -saved : result;
    }
    char path[] = "/tmp/prima-exec-crt-XXXXXX";
    int temp = mkstemp(path);
    if (temp < 0) return -errno;
    if (write(temp, prima_exec_child_start, length) != (ssize_t)length || fchmod(temp, 0755) != 0) {
        int error = errno ? errno : EIO;
        close(temp);
        unlink(path);
        return -error;
    }
    close(temp);
    int readable = open(path, O_RDONLY);
    unlink(path);
    if (readable < 0) return -errno;
    fexecve(readable, args, env);
    int error = errno;
    close(readable);
    return -error;
#else
    return prima_exec_image(prima_exec_child_start, length, args, env);
#endif
}
int main(void) {
    emit("EXECVE");
#ifdef PRIMA_HOST
    char path[]="/tmp/prima-execve-XXXXXX", bad[]="/tmp/prima-execbad-XXXXXX";
#else
    char path[]="/tmp/exec-check", bad[]="/tmp/exec-bad";
#endif
    unsigned length=build_image();
    if (!length || create_fixture(path,image,length) || create_fixture(bad,"bad",3)) return 1;
    char *args[]={"tiny","x",0}, *env[]={"EXEC_ENV=1",0};
    int fd=open(path,O_RDONLY);
    ssize_t read_count=fd>=0?read(fd,image,sizeof image):-1;
    if(fd>=0) close(fd);
    field("read",read_count==(ssize_t)length,read_count);
    int protected=open(bad,O_RDONLY|O_CLOEXEC);
    errno=0;
    int failed=execve(bad,args,env);
    int atomic=failed==-1 && errno==ENOEXEC && protected>=0 &&
               fcntl(protected,F_GETFD)==FD_CLOEXEC;
    char value=0;
    atomic=atomic && read(protected,&value,1)==1 && value=='b';
    if(protected>=0) close(protected);
    int contract_ok = prima_contract_accepts(image,length);
    unsigned note_at = (unsigned)prima_contract_u64(image+128);
    for (unsigned offset=20; offset<=40; offset+=4) {
        unsigned char saved = image[note_at+offset];
        image[note_at+offset] = 9;
        contract_ok = contract_ok && !prima_contract_accepts(image,length);
#ifndef PRIMA_HOST
        /* Linux ignores this user-side declaration. Exercise Prima's actual
         * C exec policy here; host parity covers only the common parser. */
        if (offset==32) {
            unsigned char incompatible[232];
            incompatible_image(incompatible);
            int guard = -1;
            if (create_fixture(bad,incompatible,sizeof incompatible)==0) guard=open(path,O_RDONLY|O_CLOEXEC);
            errno=0;
            int refused=execve(bad,args,env);
            contract_ok = contract_ok && refused==-1 && errno==ENOEXEC && guard>=0 &&
                fcntl(guard,F_GETFD)==FD_CLOEXEC;
            if (guard>=0) close(guard);
        }
#endif
        image[note_at+offset] = saved;
    }
#ifndef PRIMA_HOST
    if (create_fixture(bad,"bad",3)) contract_ok=0;
#endif
    pid_t child=fork();
    if(!child) {
        int kept=open(path,O_RDONLY), closed=open(path,O_RDONLY|O_CLOEXEC);
        if(kept!=3 || closed!=4) _exit(96);
        execve(path,args,env);
        _exit(99);
    }
    int status=0;
    pid_t waited=child>0?waitpid(child,&status,0):-1;
    int executed=WIFEXITED(status) && WEXITSTATUS(status)==2;
    field("exec",executed,status); field("wait",waited==child,waited);
    field("env",executed,status); field("cloexec",executed,status); field("atomic",atomic,errno);
    field("contract",contract_ok,ENOEXEC);
    /* The tiny path witness above is independent of this full C crt witness.
     * OFD cursor changes in the new image must be visible to the old parent. */
    int kept = open(bad, O_RDONLY), closed = open(path, O_RDONLY|O_CLOEXEC);
    value = 0;
    int crt_ok = kept == 3 && closed == 4 && read(kept, &value, 1) == 1 && value == 'b';
    child = crt_ok ? fork() : -1;
    if (!child) {
        struct sigaction action = {0};
        action.sa_handler = SIG_IGN;
        sigemptyset(&action.sa_mask);
        if (sigaction(SIGUSR1, &action, 0)) _exit(90);
        action.sa_handler = caught_before_exec;
        if (sigaction(SIGTERM, &action, 0)) _exit(91);
        sigset_t mask;
        sigemptyset(&mask); sigaddset(&mask, SIGUSR2);
        if (sigprocmask(SIG_BLOCK, &mask, 0) || chdir("/tmp")) _exit(92);
        umask(027);
#ifndef PRIMA_HOST
        struct termios term;
        if (tcgetattr(0, &term)) _exit(93);
        term.c_lflag &= (tcflag_t)~ECHO;
        if (tcsetattr(0, TCSANOW, &term)) _exit(94);
#endif
        char *crt_args[] = {"exec-crt", "verify", 0}, *crt_env[] = {"EXEC_CRT=1", 0};
        (void)exec_c_image(crt_args, crt_env);
        _exit(95);
    }
    waited = child > 0 ? waitpid(child, &status, 0) : -1;
    crt_ok = crt_ok && waited == child && WIFEXITED(status) && WEXITSTATUS(status) == 2;
    field("crt", crt_ok, status);
    field("reexec", crt_ok, status);
    value = 0;
    field("ofd", crt_ok && read(kept, &value, 1) == 1 && value == 'd', value);
    if (kept >= 0) close(kept);
    if (closed >= 0) close(closed);
    unlink(path); unlink(bad);
    emit("|ok=");emit_dec(failures?0:1);emit(";err=");emit_dec(first_error);emit("\n");
    return failures?1:0;
}
