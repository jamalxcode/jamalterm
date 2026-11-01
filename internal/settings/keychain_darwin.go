//go:build darwin && cgo

package settings

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

// Replaces the item. Items added by an app through SecItemAdd trust only
// that app: other programs (including the `security` tool) must ask.
static int jt_set(const char *service, const char *account, const char *data, int len) {
	CFMutableDictionaryRef q = jt_query(service, account);
	SecItemDelete(q);
	CFDataRef d = CFDataCreate(NULL, (const UInt8 *)data, len);
	CFDictionarySetValue(q, kSecValueData, d);
	CFStringRef label = CFStringCreateWithCString(NULL, "jamalterm API keys", kCFStringEncodingUTF8);
	CFDictionarySetValue(q, kSecAttrLabel, label);
	OSStatus st = SecItemAdd(q, NULL);
	CFRelease(label);
	CFRelease(d);
	CFRelease(q);
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

func nativeKeychainAvailable() bool { return true }

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

func keychainSet(service, account string, data []byte) error {
	cs, ca := C.CString(service), C.CString(account)
	defer C.free(unsafe.Pointer(cs))
	defer C.free(unsafe.Pointer(ca))
	var p *C.char
	if len(data) > 0 {
		p = (*C.char)(unsafe.Pointer(&data[0]))
	}
	if st := C.jt_set(cs, ca, p, C.int(len(data))); st != 0 {
		return fmt.Errorf("Keychain error %d", int(st))
	}
	return nil
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
