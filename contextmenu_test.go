package main

import "testing"

func TestFrameRoundTrip(t *testing.T) {
	if _, ok := frameOf(`{"version":2,"groups":[]}`); ok {
		t.Fatal("frame from nothing")
	}
	if _, ok := frameOf(`{"frame":{"x":1,"y":2,"w":50,"h":40}}`); ok {
		t.Fatal("tiny frame accepted")
	}
	f, ok := frameOf(`{"groups":[],"frame":{"x":40,"y":60,"w":1200,"h":800}}`)
	if !ok || f != (frame{40, 60, 1200, 800}) {
		t.Fatalf("got %+v %v", f, ok)
	}
}
