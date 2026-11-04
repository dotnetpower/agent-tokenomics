//go:build !unix

package inspect

func statIno(_ string) (uint64, uint64, bool) {
	return 0, 0, false
}
