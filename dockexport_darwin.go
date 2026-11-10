//go:build darwin

package main

// A file of its own: cgo allows only declarations in the preamble of a file
// with //export, and the Dock menu's Objective-C lives in dockmenu_darwin.go.

import "C"

//export jtDockPicked
func jtDockPicked(tag C.int) { dockPicked(int(tag)) }
