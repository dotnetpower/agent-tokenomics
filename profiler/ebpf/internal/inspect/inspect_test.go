package inspect

import (
	"os/exec"
	"path/filepath"
	"testing"
)

func TestInspectStrippedAndUnstrippedGoTLSBinary(t *testing.T) {
	goBin, err := exec.LookPath("go")
	if err != nil {
		t.Skip("go toolchain not in PATH")
	}
	tmp := t.TempDir()
	src := filepath.Join(tmp, "main.go")
	code := `package main
import "net/http"
func main() { _, _ = http.Get("https://example.com") }
`
	if err := exec.Command("sh", "-c", "cat > "+src+" << 'EOF'\n"+code+"EOF\n").Run(); err != nil {
		t.Fatal(err)
	}
	for _, stripped := range []bool{false, true} {
		out := filepath.Join(tmp, "bin")
		args := []string{"build"}
		if stripped {
			args = append(args, "-ldflags=-s -w")
		}
		args = append(args, "-o", out, src)
		if b, err := exec.Command(goBin, args...).CombinedOutput(); err != nil {
			t.Fatalf("build stripped=%v: %v (%s)", stripped, err, b)
		}
		fi := inspectFile(out)
		if fi.provider == nil || fi.provider.Kind != "go-tls" {
			t.Fatalf("stripped=%v: expected go-tls provider, got %+v", stripped, fi.provider)
		}
		if fi.provider.Addresses[GoTLSWriteSym] == 0 || fi.provider.Addresses[GoTLSReadSym] == 0 {
			t.Fatalf("stripped=%v: missing symbol file offsets: %+v", stripped, fi.provider.Addresses)
		}
		if len(fi.provider.RetOffsets[GoTLSWriteSym]) == 0 || len(fi.provider.RetOffsets[GoTLSReadSym]) == 0 {
			t.Fatalf("stripped=%v: missing RET offsets: %+v", stripped, fi.provider.RetOffsets)
		}
	}
}
