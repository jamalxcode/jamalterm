//go:build !darwin

package main

// setDockMenu: only macOS has a Dock.
func setDockMenu([]switchItem) {}
