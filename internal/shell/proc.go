package shell

import (
	"golang.org/x/sys/unix"
)

// Foreground reports the program running in front in this tab: the
// process group the terminal belongs to, when it isn't the shell itself.
// It returns 0, "" while the shell is waiting at its prompt.
func (s *Session) Foreground() (pid int, name string) {
	select {
	case <-s.done:
		return 0, ""
	default:
	}
	pgid, err := unix.IoctlGetInt(int(s.ptmx.Fd()), unix.TIOCGPGRP)
	if err != nil || pgid <= 0 || pgid == s.Pid() {
		return 0, ""
	}
	return pgid, procName(pgid)
}

// Busy: something other than the shell is running in this tab.
func (s *Session) Busy() bool {
	pid, _ := s.Foreground()
	return pid != 0
}

// PasswordMode: the program in front has turned echo off while reading a
// line, which is what password prompts (sudo, ssh, gpg…) do.
func (s *Session) PasswordMode() bool {
	select {
	case <-s.done:
		return false
	default:
	}
	t, err := unix.IoctlGetTermios(int(s.ptmx.Fd()), ioctlGetTermios)
	if err != nil {
		return false
	}
	return t.Lflag&unix.ECHO == 0 && t.Lflag&unix.ICANON != 0
}
