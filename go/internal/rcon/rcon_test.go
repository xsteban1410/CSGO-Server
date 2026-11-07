package rcon

import (
	"bytes"
	"errors"
	"net"
	"testing"
	"time"
)

func TestPacketRoundTrip(t *testing.T) {
	var b bytes.Buffer
	if err := WritePacket(&b, 7, TypeExec, "status"); err != nil {
		t.Fatal(err)
	}
	id, kind, body, err := ReadPacket(&b)
	if err != nil || id != 7 || kind != TypeExec || body != "status" {
		t.Fatalf("got %d %d %q %v", id, kind, body, err)
	}
}

func TestRejectsBadSize(t *testing.T) {
	if _, _, _, err := ReadPacket(bytes.NewReader([]byte{1, 0, 0, 0})); err == nil {
		t.Fatal("a size under the minimum must fail")
	}
}

func TestWrongPassword(t *testing.T) {
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatal(err)
	}
	defer l.Close()
	go func() {
		c, _ := l.Accept()
		defer c.Close()
		_, _, _, _ = ReadPacket(c)
		_ = WritePacket(c, 1, TypeResponse, "")
		_ = WritePacket(c, -1, TypeAuthResp, "")
	}()
	if _, err := Dial(l.Addr().String(), "x", time.Second); !errors.Is(err, ErrAuth) {
		t.Fatalf("got %v", err)
	}
}

func TestForbiddenCharacters(t *testing.T) {
	if _, err := (&Client{}).Exec("a\nb", time.Second); err == nil {
		t.Fatal("a line break must be refused")
	}
}

func TestCvarValue(t *testing.T) {
	if v := CvarValue(`"bot_quota" = "5" ( def. "0" )`); v != "5" {
		t.Fatal(v)
	}
	if v := CvarValue("Unknown command"); v != "" {
		t.Fatal(v)
	}
}
