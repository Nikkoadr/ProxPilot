package runs

import (
	"fmt"
	"sync"
	"time"
)

// Event is one SSE message.
type Event struct {
	Type string // "log" | "done" | "error"
	Data string
}

// Run tracks an async job with replayable log + live subscribers.
type Run struct {
	ID      string
	Title   string
	Created time.Time

	mu     sync.Mutex
	log    []string
	status string // running|done|failed
	msg    string
	subs   map[chan Event]bool
	done   chan struct{}
}

func newRun(id, title string) *Run {
	return &Run{ID: id, Title: title, Created: time.Now(),
		status: "running", subs: map[chan Event]bool{}, done: make(chan struct{})}
}

func (r *Run) emit(t, d string) {
	r.mu.Lock()
	if t == "log" {
		r.log = append(r.log, d)
	} else {
		r.status = t
		r.msg = d
	}
	for ch := range r.subs {
		select {
		case ch <- Event{Type: t, Data: d}:
		default:
		}
	}
	r.mu.Unlock()
	if t == "done" || t == "error" {
		select {
		case <-r.done:
		default:
			close(r.done)
		}
	}
}

// Log appends a line.
func (r *Run) Log(format string, args ...any) { r.emit("log", fmt.Sprintf(format, args...)) }

// Done marks success. Fail marks error.
func (r *Run) Done(msg string) { r.emit("done", msg) }
func (r *Run) Fail(msg string) { r.emit("error", msg) }

// Snapshot returns status + message + full log.
func (r *Run) Snapshot() (string, string, []string) {
	r.mu.Lock()
	defer r.mu.Unlock()
	cp := append([]string{}, r.log...)
	return r.status, r.msg, cp
}

func (r *Run) subscribe() chan Event {
	ch := make(chan Event, 64)
	r.mu.Lock()
	r.subs[ch] = true
	r.mu.Unlock()
	return ch
}

func (r *Run) unsubscribe(ch chan Event) {
	r.mu.Lock()
	delete(r.subs, ch)
	r.mu.Unlock()
}

// Registry holds all runs.
type Registry struct {
	mu   sync.Mutex
	runs map[string]*Run
	n    int
}

func NewRegistry() *Registry { return &Registry{runs: map[string]*Run{}} }

// Start creates a run and executes fn in background.
func (reg *Registry) Start(title string, fn func(r *Run)) *Run {
	reg.mu.Lock()
	reg.n++
	id := fmt.Sprintf("r%d-%d", time.Now().Unix(), reg.n)
	r := newRun(id, title)
	reg.runs[id] = r
	reg.mu.Unlock()
	go func() {
		defer func() {
			if rec := recover(); rec != nil {
				r.Fail(fmt.Sprintf("panic: %v", rec))
			}
		}()
		fn(r)
	}()
	return r
}

// Get returns a run by id.
func (reg *Registry) Get(id string) (*Run, bool) {
	reg.mu.Lock()
	defer reg.mu.Unlock()
	r, ok := reg.runs[id]
	return r, ok
}

// Stream replays log then follows live events until done. Returns false if unknown id.
func (reg *Registry) Stream(id string, send func(ev Event) bool) bool {
	r, ok := reg.Get(id)
	if !ok {
		return false
	}
	status, msg, log := r.Snapshot()
	for _, line := range log {
		if !send(Event{Type: "log", Data: line}) {
			return true
		}
	}
	if status == "done" || status == "error" {
		send(Event{Type: status, Data: msg})
		return true
	}
	ch := r.subscribe()
	defer r.unsubscribe(ch)
	for {
		select {
		case ev := <-ch:
			if !send(ev) {
				return true
			}
			if ev.Type == "done" || ev.Type == "error" {
				return true
			}
		case <-r.done:
			st, msg, _ := r.Snapshot()
			send(Event{Type: st, Data: msg})
			return true
		}
	}
}
