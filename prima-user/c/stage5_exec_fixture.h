/* The fixture differs only at the native exit boundary: Prima54 / Linux60. */
#ifndef PRIMA_STAGE5_EXEC_FIXTURE_H
#define PRIMA_STAGE5_EXEC_FIXTURE_H
#ifdef PRIMA_HOST
#include <sys/mman.h>
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
    int fd = memfd_create("prima-exec-reference", MFD_CLOEXEC);
    if (fd < 0) return -errno;
    if (write(fd, image, length) != (ssize_t)length) {
        int error = errno ? errno : EIO;
        close(fd);
        return -error;
    }
    char *argv[5] = {0};
    char *envp[1] = {0};
    for (unsigned long i=0; i<argc; i++) argv[i]=args[i];
    fexecve(fd, argv, envp);
    int error = errno;
    close(fd);
    return -error;
#else
    return prima_native(55, (long)image, length, (long)args, argc, 0, 0);
#endif
}
#endif
