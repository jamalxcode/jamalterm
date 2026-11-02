package shell

import (
	"sync"
	"time"
)

// Pump batches shell output for the window and applies backpressure.
//
// Output is collected and sent at most every FlushEvery as one chunk, so a
// command that prints megabytes doesn't flood the window with tiny
// messages. The window acknowledges what it has drawn (Ack). When it falls
// MaxUnacked bytes behind, sending pauses; when MaxPending bytes are also
// waiting, Write blocks — which stops reading from the shell, so a program
// like `yes` slows down instead of freezing the app.
type Pump struct {
	FlushEvery time.Duration
	MaxUnacked int
	MaxPending int

	emit func([]byte)

	mu      sync.Mutex
	cond    *sync.Cond
	buf     []byte
	unacked int
	closed  bool
	recent  []byte // the last RecentBytes sent, to redraw a reloaded window
	stop    chan struct{}
	done    chan struct{}
}

// RecentBytes is how much recent output a pump keeps for Recent.
const RecentBytes = 256 << 10

// NewPump starts a pump that hands batches to emit.
func NewPump(emit func([]byte)) *Pump {
	p := &Pump{
		FlushEvery: 16 * time.Millisecond,
		MaxUnacked: 1 << 20, // 1 MiB drawn-but-not-acknowledged
		MaxPending: 4 << 20, // 4 MiB waiting to be sent
		emit:       emit,
		stop:       make(chan struct{}),
		done:       make(chan struct{}),
	}
	p.cond = sync.NewCond(&p.mu)
	go p.loop()
	return p
}

// Write queues output, blocking while too much is waiting.
func (p *Pump) Write(b []byte) {
	p.mu.Lock()
	defer p.mu.Unlock()
	for !p.closed && len(p.buf) >= p.MaxPending {
		p.cond.Wait()
	}
	if p.closed {
		return
	}
	p.buf = append(p.buf, b...)
}

// Ack records that the window has drawn n bytes.
func (p *Pump) Ack(n int) {
	p.mu.Lock()
	p.unacked -= n
	if p.unacked < 0 {
		p.unacked = 0
	}
	p.mu.Unlock()
	p.cond.Broadcast()
}

func (p *Pump) loop() {
	defer close(p.done)
	t := time.NewTicker(p.FlushEvery)
	defer t.Stop()
	for {
		select {
		case <-p.stop:
			return
		case <-t.C:
			p.flush(false)
		}
	}
}

func (p *Pump) flush(force bool) {
	p.mu.Lock()
	if len(p.buf) == 0 || (!force && p.unacked >= p.MaxUnacked) {
		p.mu.Unlock()
		return
	}
	data := p.buf
	p.buf = nil
	p.unacked += len(data)
	p.recent = append(p.recent, data...)
	if over := len(p.recent) - RecentBytes; over > 0 {
		p.recent = append([]byte(nil), p.recent[over:]...)
	}
	p.mu.Unlock()
	p.cond.Broadcast()
	p.emit(data)
}

// Close sends whatever is left and stops. Call it before reporting that
// the shell exited, so the last output arrives first.
func (p *Pump) Close() {
	p.mu.Lock()
	if p.closed {
		p.mu.Unlock()
		return
	}
	p.mu.Unlock()
	close(p.stop)
	<-p.done
	p.flush(true)
	p.mu.Lock()
	p.closed = true
	p.mu.Unlock()
	p.cond.Broadcast()
}

// Recent returns the last output sent (up to RecentBytes) and forgets
// what the old page hadn't acknowledged: a reloaded window draws this,
// then output carries on.
func (p *Pump) Recent() []byte {
	p.mu.Lock()
	out := append([]byte(nil), p.recent...)
	p.unacked = 0
	p.mu.Unlock()
	p.cond.Broadcast()
	return out
}
