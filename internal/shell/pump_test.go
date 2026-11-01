package shell

import (
	"bytes"
	"sync"
	"testing"
	"time"
)

type sink struct {
	mu     sync.Mutex
	chunks [][]byte
}

func (s *sink) emit(b []byte) {
	s.mu.Lock()
	s.chunks = append(s.chunks, append([]byte(nil), b...))
	s.mu.Unlock()
}

func (s *sink) all() ([]byte, int) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return bytes.Join(s.chunks, nil), len(s.chunks)
}

func TestPumpBatchesInOrder(t *testing.T) {
	s := &sink{}
	p := NewPump(s.emit)
	var want []byte
	for i := 0; i < 2000; i++ {
		line := []byte{byte('a' + i%26), '\n'}
		want = append(want, line...)
		p.Write(line)
	}
	p.Close()
	got, n := s.all()
	if !bytes.Equal(got, want) {
		t.Fatal("output changed or reordered")
	}
	if n > 20 {
		t.Fatalf("2000 tiny writes became %d messages; want a few batches", n)
	}
}

func TestPumpBackpressure(t *testing.T) {
	s := &sink{}
	p := NewPump(s.emit)
	p.MaxUnacked, p.MaxPending = 1000, 1000
	chunk := bytes.Repeat([]byte("x"), 500)

	// Nobody acknowledges: after ~1000 sent + ~1000 pending, Write must block.
	blocked := make(chan struct{})
	go func() {
		for i := 0; i < 20; i++ {
			p.Write(chunk)
		}
		close(blocked)
	}()
	select {
	case <-blocked:
		t.Fatal("writer was never held back")
	case <-time.After(300 * time.Millisecond):
	}
	sent, _ := s.all()
	if len(sent) > 2000 {
		t.Fatalf("sent %d bytes without acknowledgements", len(sent))
	}
	// Acknowledge as the window draws: everything gets through.
	go func() {
		for {
			select {
			case <-blocked:
				return
			case <-time.After(5 * time.Millisecond):
				p.Ack(1000)
			}
		}
	}()
	select {
	case <-blocked:
	case <-time.After(3 * time.Second):
		t.Fatal("writer stayed blocked after acknowledgements")
	}
	p.Close()
	if got, _ := s.all(); len(got) != 20*500 {
		t.Fatalf("got %d bytes, want %d", len(got), 20*500)
	}
}

func TestPumpCloseUnblocksWriter(t *testing.T) {
	p := NewPump(func([]byte) {})
	p.MaxUnacked, p.MaxPending = 10, 10
	done := make(chan struct{})
	go func() {
		for i := 0; i < 100; i++ {
			p.Write([]byte("0123456789"))
		}
		close(done)
	}()
	time.Sleep(100 * time.Millisecond)
	p.Close()
	select {
	case <-done:
	case <-time.After(2 * time.Second):
		t.Fatal("Close left the writer blocked")
	}
}
