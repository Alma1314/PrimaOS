/* A separate, versioned argv/envp startup image exceeding the old 256-byte
 * stack allowance. Compiled unchanged against Linux libc and Prima musl. */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/auxv.h>

int main(int argc, char **argv, char **envp) {
    int args = argc == 16 && argv[argc] == NULL;
    if (args) {
        args = strstr(argv[0], "stage5-start") != NULL && !strcmp(argv[1], "") &&
            !strcmp(argv[2], "argument with spaces") && !strcmp(argv[3], "中文启动");
        for (int i = 4; i < 16; ++i) {
            size_t len = i == 4 ? 255 : 1;
            args = args && strlen(argv[i]) == len;
            for (size_t j = 0; j < len; ++j)
                args = args && argv[i][j] == 'x';
        }
    }
    int envc = 0;
    while (envp[envc]) ++envc;
    const char *abi = getenv("PRIMA_ABI"), *spaces = getenv("START_ENV");
    const char *empty = getenv("START_EMPTY"), *longenv = getenv("START_LONG");
    int env = envc == 8 && abi && !strcmp(abi, "1") && spaces &&
        !strcmp(spaces, "with spaces") && empty && !*empty && longenv && strlen(longenv) == 200;
    if (longenv)
        for (size_t i = 0; longenv[i]; ++i) env = env && longenv[i] == 'y';
    int aux = getauxval(AT_PAGESZ) == 4096 && getauxval(AT_PHDR) &&
        getauxval(AT_PHENT) == 56 && getauxval(AT_PHNUM) && getauxval(AT_ENTRY) &&
        getauxval(AT_EXECFN) && *(const char *)getauxval(AT_EXECFN);
    printf("START|argv=%d;envp=%d;auxv=%d|ok=%d\n", args, env, aux, args && env && aux);
    return !(args && env && aux);
}
