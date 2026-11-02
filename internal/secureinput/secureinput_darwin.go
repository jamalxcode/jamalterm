//go:build darwin && cgo

// Package secureinput turns on macOS Secure Keyboard Entry, which stops
// other apps from reading what you type (for example a password).
package secureinput

/*
#cgo LDFLAGS: -framework Carbon
#include <Carbon/Carbon.h>
static void jt_secure(int on) { if (on) EnableSecureEventInput(); else DisableSecureEventInput(); }
static int jt_secure_on(void) { return IsSecureEventInputEnabled() ? 1 : 0; }
*/
import "C"

import "sync"

var (
	mu sync.Mutex
	on bool
)

// Set turns Secure Keyboard Entry on or off for jamalterm. The calls
// are counted by macOS, so this keeps them balanced.
func Set(want bool) {
	mu.Lock()
	defer mu.Unlock()
	if want == on {
		return
	}
	if want {
		C.jt_secure(1)
	} else {
		C.jt_secure(0)
	}
	on = want
}

// On reports whether jamalterm has it turned on.
func On() bool {
	mu.Lock()
	defer mu.Unlock()
	return on
}

// Supported: this Mac build can protect typing.
const Supported = true

// SystemOn reports whether any app has it on (macOS-wide).
func SystemOn() bool { return C.jt_secure_on() == 1 }
