//go:build unix

package inspect

import "syscall"

func statIno(path string) (uint64, uint64, bool) {
	var st syscall.Stat_t
	if err := syscall.Stat(path, &st); err == nil {
		return uint64(st.Dev), uint64(st.Ino), true
	}
	return 0, 0, false
}
