// Package winstate keeps each window's saved tabs and turns them into the
// session file (and back). It is plain data, so it can be tested without
// any windows.
package winstate

import (
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strconv"
	"strings"
	"sync"
)

// Name is a window's name for its number (1, 2, …).
func Name(n int) string { return fmt.Sprintf("jt-%d", n) }

// Num is a window's number from its name (0 if it isn't one of ours).
func Num(name string) int {
	if !strings.HasPrefix(name, "jt-") {
		return 0
	}
	n, _ := strconv.Atoi(strings.TrimPrefix(name, "jt-"))
	return n
}

// Sessions holds each window's latest saved tabs (JSON from the window).
type Sessions struct {
	mu sync.Mutex
	m  map[string]string
}

func New() *Sessions { return &Sessions{m: map[string]string{}} }

// Set stores a window's tabs. data must be JSON.
func (s *Sessions) Set(window, data string) error {
	if !json.Valid([]byte(data)) {
		return errors.New("the window sent tabs that aren't valid JSON")
	}
	s.mu.Lock()
	s.m[window] = data
	s.mu.Unlock()
	return nil
}

// Get returns a window's tabs ("" if none).
func (s *Sessions) Get(window string) string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.m[window]
}

// Drop forgets a window (it was closed).
func (s *Sessions) Drop(window string) {
	s.mu.Lock()
	delete(s.m, window)
	s.mu.Unlock()
}

// Clear forgets everything.
func (s *Sessions) Clear() {
	s.mu.Lock()
	s.m = map[string]string{}
	s.mu.Unlock()
}

type file struct {
	Version int               `json:"version"`
	Windows []json.RawMessage `json:"windows"`
}

// Encode is the session file: every window, oldest first.
func (s *Sessions) Encode() []byte {
	s.mu.Lock()
	names := make([]string, 0, len(s.m))
	for n := range s.m {
		names = append(names, n)
	}
	sort.Slice(names, func(i, j int) bool { return Num(names[i]) < Num(names[j]) })
	f := file{Version: 3, Windows: []json.RawMessage{}}
	for _, n := range names {
		f.Windows = append(f.Windows, json.RawMessage(s.m[n]))
	}
	s.mu.Unlock()
	data, _ := json.Marshal(f)
	return data
}

// Decode reads a session file into one entry per window, oldest first.
// Files from 2.x held a single window's tabs directly.
func Decode(data []byte) []string {
	if len(strings.TrimSpace(string(data))) == 0 || !json.Valid(data) {
		return nil
	}
	var f file
	if json.Unmarshal(data, &f) == nil && f.Version >= 3 {
		out := make([]string, 0, len(f.Windows))
		for _, w := range f.Windows {
			out = append(out, string(w))
		}
		return out
	}
	return []string{string(data)}
}
