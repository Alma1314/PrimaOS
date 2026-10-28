/* The fixture differs only at the native exit boundary: Prima54 / Linux60. */
#ifndef PRIMA_STAGE5_EXEC_FIXTURE_H
#define PRIMA_STAGE5_EXEC_FIXTURE_H
#ifdef PRIMA_HOST
#include <fcntl.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/stat.h>
#ifndef MFD_EXEC
#define MFD_EXEC 0x0010U
#endif
#define TINY_EXIT_SYSCALL 60
#else
#define TINY_EXIT_SYSCALL 54
#endif
static const unsigned char tiny_elf[530] = {
    [0]=127,'E','L','F',2,1,1, [16]=2, [18]=62, [20]=1,
    [25]=2,64, [32]=64, [52]=64, [54]=56, [56]=2,
    [64]=1, [68]=5, [82]=64, [90]=64, [96]=18,2, [105]=16, [113]=16,
    [120]=4, [128]=176, [152]=44,
    [176]=6, [180]=24, [184]=0x4d,0x49,0x52,0x50, 'P','r','i','m','a',0,0,0,
    [196]=1, [200]=1, [208]=1, [212]=1,
    [512]=0x48,0x8b,0x04,0x24,0x48,0x89,0xc7,0x48,0xc7,0xc0,TINY_EXIT_SYSCALL,0,0,0,0x0f,0x05,0xeb,0xfe,
};

static long exec_fixture(const unsigned char *image, unsigned long length,
                         char **args, unsigned long argc) {
#ifdef PRIMA_HOST
    if (argc > 4) return -E2BIG;
    char *argv[5] = {0};
    char *envp[1] = {0};
    for (unsigned long i=0; i<argc; i++) argv[i]=args[i];
    int fd = memfd_create("prima-exec-reference", MFD_CLOEXEC | MFD_EXEC);
    if (fd >= 0) {
        if (write(fd, image, length) == (ssize_t)length) fexecve(fd, argv, envp);
        int error = errno;
        close(fd);
        /* Some kernels or sandboxes deny exec of memfd; fall back to an
         * unlinked temporary file so the witness stays descriptor-based. */
        if (error != EACCES && error != EPERM) return -error;
    }
    char path[] = "/tmp/prima-exec-fixture-XXXXXX";
    int temp = mkstemp(path);
    if (temp < 0) return -errno;
    if (write(temp, image, length) != (ssize_t)length || fchmod(temp, 0755) != 0) {
        int error = errno ? errno : EIO;
        close(temp);
        unlink(path);
        return -error;
    }
    close(temp);
    int readable = open(path, O_RDONLY);
    unlink(path);
    if (readable < 0) return -errno;
    fexecve(readable, argv, envp);
    int error = errno;
    close(readable);
    return -error;
#else
    return prima_native(55, (long)image, length, (long)args, argc, 0, 0);
#endif
}
#endif
