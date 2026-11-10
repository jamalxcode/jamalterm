package main

import (
	"strings"
	"testing"
)

func TestCrashSummary(t *testing.T) {
	trace := "panic: runtime error: index out of range [3] with length 2\n\ngoroutine 42 [running]:\nmain.(*App).Start(...)\n\t~/jamalterm/app.go:200\nmain.foo()\n\t~/jamalterm/x.go:1\n\ngoroutine 1 [select]:\nmain.main()\n"
	got := crashSummary(trace)
	if !strings.HasPrefix(got, "panic: runtime error") || strings.Contains(got, "goroutine 1 [select]") {
		t.Fatalf("summary:\n%s", got)
	}
}
