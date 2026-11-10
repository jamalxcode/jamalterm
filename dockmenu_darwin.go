//go:build darwin

package main

/*
#cgo CFLAGS: -x objective-c
#cgo LDFLAGS: -framework Cocoa
#import <Cocoa/Cocoa.h>
#import <objc/runtime.h>

extern void jtDockPicked(int tag);

// The Dock menu (right-click the Dock icon): NSApplication asks its delegate
// for -applicationDockMenu:. Wails' delegate has no such method, so it is
// added to the delegate's class at launch.
@interface JTDockTarget : NSObject
- (void)pick:(NSMenuItem *)item;
@end
@implementation JTDockTarget
- (void)pick:(NSMenuItem *)item { jtDockPicked((int)[item tag]); }
@end

static JTDockTarget *jtTarget = nil;
static NSMenu *jtMenu = nil;

static NSMenu *jtDockMenu(id self, SEL _cmd, NSApplication *app) { return jtMenu; }

static void jtInstall(void) {
	id delegate = [NSApp delegate];
	if (delegate == nil || jtTarget != nil) return;
	jtTarget = [[JTDockTarget alloc] init];
	class_replaceMethod(object_getClass(delegate), @selector(applicationDockMenu:), (IMP)jtDockMenu, "@@:@");
}

// kinds: 0 item, 1 tab (indented), 2 checked item, 3 separator.
static void jtBuild(char **labels, int *kinds, int n) {
	NSMenu *menu = [[NSMenu alloc] initWithTitle:@""];
	[menu setAutoenablesItems:NO];
	for (int i = 0; i < n; i++) {
		if (kinds[i] == 3) { [menu addItem:[NSMenuItem separatorItem]]; continue; }
		NSString *title = [NSString stringWithUTF8String:labels[i]];
		NSMenuItem *it = [[NSMenuItem alloc] initWithTitle:title action:@selector(pick:) keyEquivalent:@""];
		[it setTarget:jtTarget];
		[it setTag:i];
		if (kinds[i] == 1) [it setIndentationLevel:1];
		if (kinds[i] == 2) [it setState:NSControlStateValueOn];
		[menu addItem:it];
		[it release];
	}
	NSMenu *old = jtMenu;
	jtMenu = menu;
	if (old) [old release];
}

static void jtSetDockMenu(char **labels, int *kinds, int n) {
	if ([NSThread isMainThread]) { @autoreleasepool { jtInstall(); jtBuild(labels, kinds, n); } return; }
	dispatch_sync(dispatch_get_main_queue(), ^{ @autoreleasepool { jtInstall(); jtBuild(labels, kinds, n); } });
}
*/
import "C"

import (
	"sync"
	"unsafe"
)

var (
	dockMu    sync.Mutex
	dockItems []switchItem
)

// dockPicked runs the Dock menu item at index i (dockexport_darwin.go).
func dockPicked(i int) {
	dockMu.Lock()
	var run func()
	if i >= 0 && i < len(dockItems) {
		run = dockItems[i].run
	}
	dockMu.Unlock()
	if run != nil {
		go run() // not on the menu's (main) thread
	}
}

// setDockMenu replaces the Dock menu's items.
func setDockMenu(items []switchItem) {
	dockMu.Lock()
	dockItems = items
	dockMu.Unlock()
	n := len(items)
	if n == 0 {
		return
	}
	labels := make([]*C.char, n)
	kinds := make([]C.int, n)
	for i, it := range items {
		labels[i] = C.CString(it.Label)
		switch {
		case it.Sep:
			kinds[i] = 3
		case it.Indent:
			kinds[i] = 1
		case it.Checked:
			kinds[i] = 2
		}
	}
	cLabels := (**C.char)(C.malloc(C.size_t(n) * C.size_t(unsafe.Sizeof(uintptr(0)))))
	cKinds := (*C.int)(C.malloc(C.size_t(n) * C.size_t(unsafe.Sizeof(C.int(0)))))
	ls := unsafe.Slice(cLabels, n)
	ks := unsafe.Slice(cKinds, n)
	copy(ls, labels)
	copy(ks, kinds)
	C.jtSetDockMenu(cLabels, cKinds, C.int(n))
	for _, l := range labels {
		C.free(unsafe.Pointer(l))
	}
	C.free(unsafe.Pointer(cLabels))
	C.free(unsafe.Pointer(cKinds))
}
