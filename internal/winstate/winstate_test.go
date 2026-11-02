package winstate

import (
	"reflect"
	"testing"
)

func TestNames(t *testing.T) {
	if Name(12) != "jt-12" || Num("jt-12") != 12 || Num("other") != 0 || Num("jt-x") != 0 {
		t.Fatal("names")
	}
}

func TestEncodeOrdersWindowsAndDecodes(t *testing.T) {
	s := New()
	s.Set(Name(10), `{"w":10}`)
	s.Set(Name(2), `{"w":2}`)
	s.Set(Name(1), `{"w":1}`)
	if err := s.Set(Name(3), `{broken`); err == nil {
		t.Fatal("bad JSON accepted")
	}
	s.Drop(Name(1))
	got := Decode(s.Encode())
	if !reflect.DeepEqual(got, []string{`{"w":2}`, `{"w":10}`}) {
		t.Fatalf("got %q (want window 2 before 10, window 1 dropped)", got)
	}
	s.Clear()
	if got := Decode(s.Encode()); len(got) != 0 {
		t.Fatalf("after clear: %q", got)
	}
}

func TestDecodeOldAndBadFiles(t *testing.T) {
	old := `{"version":2,"active":0,"groups":[{"tab":{"cwd":"/x"}}]}`
	if got := Decode([]byte(old)); !reflect.DeepEqual(got, []string{old}) {
		t.Fatalf("2.x file: %q", got)
	}
	for _, bad := range []string{"", "  ", "not json", `{"version":3,"windows":`} {
		if got := Decode([]byte(bad)); got != nil {
			t.Errorf("%q gave %q", bad, got)
		}
	}
}
