//go:build !darwin || !cgo

package settings

import "errors"

func keychainAvailable() bool { return false }

func keychainExists(service, account string) bool { return false }

func keychainGet(service, account string) ([]byte, bool, error) {
	return nil, false, errors.New("no Keychain on this system")
}

func keychainDelete(service, account string) error { return nil }
