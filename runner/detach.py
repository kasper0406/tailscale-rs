"""Run a command in a new session (setsid), detached from the caller's process group and
terminal, with stdout/stderr appended to a log file. Prints the child's pid.
usage: python3 detach.py <logfile> <cmd> [args...]"""
import os, sys

log, cmd = sys.argv[1], sys.argv[2:]
pid = os.fork()
if pid:
    print(pid)
    sys.exit(0)
os.setsid()
fd = os.open(log, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o600)
os.dup2(fd, 1)
os.dup2(fd, 2)
null = os.open(os.devnull, os.O_RDONLY)
os.dup2(null, 0)
os.execvp(cmd[0], cmd)
