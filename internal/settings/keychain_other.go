//go:build !darwin || !cgo

package settings

import "errors"

// Other systems (development builds) keep keys in 0600 files instead.

func nativeKeychainAvailable() bool { return false }

func keychainGet(service, account string) ([]byte, bool, error) {
	return nil, false, errors.New("no Keychain on this system")
}

func keychainSet(service, account string, data []byte) error {
	return errors.New("no Keychain on this system")
}

func keychainDelete(service, account string) error { return nil }
