/* r41e (wolf-interp#307): the signal state this process started with,
 * printed as Linux prints it in /proc/self/status, for a host with no
 * /proc (macOS). Bit n - 1 stands for signal n: SigBlk holds the signals
 * in the mask (sigprocmask), SigIgn those whose action is SIG_IGN and
 * SigCgt those with a handler (sigaction with no new action). Both calls
 * only read. With an argument the report goes to that file, not stdout. */
#include <signal.h>
#include <stdio.h>

int main(int argc, char **argv) {
    unsigned long long blk = 0, ign = 0, cgt = 0;
    sigset_t mask;
    if (sigprocmask(SIG_BLOCK, NULL, &mask) != 0) {
        return 2;
    }
    for (int n = 1; n < 32; n++) {
        struct sigaction action;
        if (sigaction(n, NULL, &action) != 0) {
            continue;
        }
        if (action.sa_handler == SIG_IGN) {
            ign |= 1ULL << (n - 1);
        } else if (action.sa_handler != SIG_DFL) {
            cgt |= 1ULL << (n - 1);
        }
        if (sigismember(&mask, n) == 1) {
            blk |= 1ULL << (n - 1);
        }
    }
    FILE *out = argc > 1 ? fopen(argv[1], "w") : stdout;
    if (out == NULL) {
        return 3;
    }
    fprintf(out, "Name:\tsigprobe\nSigBlk:\t%016llx\nSigIgn:\t%016llx\nSigCgt:\t%016llx\n",
            blk, ign, cgt);
    return fclose(out) == 0 ? 0 : 4;
}
