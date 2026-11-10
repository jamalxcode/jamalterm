//go:build darwin && cgo

package settings

// The Keychain is no longer where keys are kept (see secret.go). This only
// lets "Move keys from the Keychain" read the item older versions saved,
// once, when you ask, and then delete it.

/*
#cgo LDFLAGS: -framework Security -framework CoreFoundation
#include <Security/Security.h>
#include <CoreFoundation/CoreFoundation.h>
#include <stdlib.h>
#include <string.h>

static CFMutableDictionaryRef jt_query(const char *service, const char *account) {
	CFMutableDictionaryRef q = CFDictionaryCreateMutable(NULL, 0,
		&kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
	CFStringRef s = CFStringCreateWithCString(NULL, service, kCFStringEncodingUTF8);
	CFStringRef a = CFStringCreateWithCString(NULL, account, kCFStringEncodingUTF8);
	CFDictionarySetValue(q, kSecClass, kSecClassGenericPassword);
	CFDictionarySetValue(q, kSecAttrService, s);
	CFDictionarySetValue(q, kSecAttrAccount, a);
	CFRelease(s);
	CFRelease(a);
	return q;
}

// Whether the item exists, asking only for its attributes: that never
// shows a password prompt (reading the secret data would).
static int jt_exists(const char *service, const char *account) {
	CFMutableDictionaryRef q = jt_query(service, account);
	CFDictionarySetValue(q, kSecReturnAttributes, kCFBooleanTrue);
	CFDictionarySetValue(q, kSecMatchLimit, kSecMatchLimitOne);
	CFTypeRef res = NULL;
	OSStatus st = SecItemCopyMatching(q, &res);
	CFRelease(q);
	if (res != NULL) CFRelease(res);
	return (int)st;
}

static int jt_get(const char *service, const char *account, char **out, int *outLen) {
	CFMutableDictionaryRef q = jt_query(service, account);
	CFDictionarySetValue(q, kSecReturnData, kCFBooleanTrue);
	CFDictionarySetValue(q, kSecMatchLimit, kSecMatchLimitOne);
	CFTypeRef res = NULL;
	OSStatus st = SecItemCopyMatching(q, &res);
	CFRelease(q);
	if (st == errSecSuccess && res != NULL) {
		CFDataRef d = (CFDataRef)res;
		CFIndex n = CFDataGetLength(d);
		*out = (char *)malloc(n > 0 ? n : 1);
		memcpy(*out, CFDataGetBytePtr(d), n);
		*outLen = (int)n;
		CFRelease(res);
	}
	return (int)st;
}

static int jt_delete(const char *service, const char *account) {
	CFMutableDictionaryRef q = jt_query(service, account);
	OSStatus st = SecItemDelete(q);
	CFRelease(q);
	return (int)st;
}
*/
import "C"

import (
	"fmt"
	"unsafe"
)

const errSecItemNotFound = -25300

func keychainAvailable() bool { return true }

// keychainGet returns the item's data; found is false when there is none.
func keychainGet(service, account string) (data []byte, found bool, err error) {
	cs, ca := C.CString(service), C.CString(account)
	defer C.free(unsafe.Pointer(cs))
	defer C.free(unsafe.Pointer(ca))
	var out *C.char
	var n C.int
	st := C.jt_get(cs, ca, &out, &n)
	if st == errSecItemNotFound {
		return nil, false, nil
	}
	if st != 0 {
		return nil, false, fmt.Errorf("Keychain error %d", int(st))
	}
	defer C.free(unsafe.Pointer(out))
	return C.GoBytes(unsafe.Pointer(out), n), true, nil
}

// keychainExists reports whether the item is there, without a prompt.
func keychainExists(service, account string) bool {
	cs, ca := C.CString(service), C.CString(account)
	defer C.free(unsafe.Pointer(cs))
	defer C.free(unsafe.Pointer(ca))
	return C.jt_exists(cs, ca) == 0
}

func keychainDelete(service, account string) error {
	cs, ca := C.CString(service), C.CString(account)
	defer C.free(unsafe.Pointer(cs))
	defer C.free(unsafe.Pointer(ca))
	if st := C.jt_delete(cs, ca); st != 0 && st != errSecItemNotFound {
		return fmt.Errorf("Keychain error %d", int(st))
	}
	return nil
}
