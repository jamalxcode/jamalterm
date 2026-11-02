//go:build !darwin || !cgo

// Package secureinput turns on macOS Secure Keyboard Entry. Elsewhere it
// only remembers the state.
package secureinput

import "sync"

var (
	mu sync.Mutex
	on bool
)

func Set(want bool) { mu.Lock(); on = want; mu.Unlock() }

func On() bool { mu.Lock(); defer mu.Unlock(); return on }

const Supported = false

func SystemOn() bool { return false }
