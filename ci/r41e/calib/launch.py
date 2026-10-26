# r41e calibration launcher: start argv[1:] with SIGPIPE and SIGUSR1 blocked
# and SIGPIPE/SIGXFSZ at their defaults (CPython ignores both itself).
import os, signal, sys
pid = os.posix_spawn(sys.argv[1], sys.argv[1:], os.environ,
                     setsigmask={signal.SIGPIPE, signal.SIGUSR1},
                     setsigdef={signal.SIGPIPE, signal.SIGXFSZ})
_, status = os.waitpid(pid, 0)
sys.exit(os.waitstatus_to_exitcode(status))
