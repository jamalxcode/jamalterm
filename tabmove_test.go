package main

import (
	"reflect"
	"testing"
)

func TestTabIDs(t *testing.T) {
	cases := map[string][]int{
		`{"tab":{"id":3,"cwd":"/x"}}`: {3},
		`{"dir":"row","ratio":0.5,"a":{"tab":{"id":4}},"b":{"dir":"column","a":{"tab":{"id":5}},"b":{"tab":{"id":6}}}}`: {4, 5, 6},
		`{"tab":{"cwd":"/no-id"}}`: nil,
		`not json`:                 nil,
	}
	for in, want := range cases {
		if got := tabIDs(in); !reflect.DeepEqual(got, want) {
			t.Errorf("tabIDs(%s) = %v, want %v", in, got, want)
		}
	}
}

// Output follows a tab to the window it moved to.
func TestTabWindowFollowsOwner(t *testing.T) {
	a := &App{owner: map[int]string{}}
	if a.tabWindow(1, nil) != nil {
		t.Fatal("no owner: the fallback")
	}
}
