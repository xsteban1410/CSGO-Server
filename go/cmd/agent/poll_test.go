package main

import (
	"errors"
	"slices"
	"testing"
	"time"
)

func TestPollStaysBetweenTheMarkers(t *testing.T) {
	var sent []string
	out, mode, err := pollServer(func(c string, _ time.Duration) (string, error) {
		sent = append(sent, c)
		return "reply to " + c, nil
	})
	if err != nil || out != "reply to status" || mode != "reply to modo_nombre" {
		t.Fatalf("got %q %q %v", out, mode, err)
	}
	if want := []string{"echo csls_poll_begin", "status", "modo_nombre", "echo csls_poll_end"}; !slices.Equal(sent, want) {
		t.Fatalf("sent %v", sent)
	}
}

func TestPollStopsAtTheFirstError(t *testing.T) {
	calls := 0
	_, _, err := pollServer(func(string, time.Duration) (string, error) {
		calls++
		return "", errors.New("lost")
	})
	if err == nil || calls != 1 {
		t.Fatalf("err %v after %d calls", err, calls)
	}
}
