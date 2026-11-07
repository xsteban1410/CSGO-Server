// Package rcon is the Source RCON protocol: a client for the agent and the test tools, and the packet
// functions the fake server shares with it. Administrative commands go through the Rust CLI instead.
package rcon

import (
	"encoding/binary"
	"errors"
	"fmt"
	"io"
	"net"
	"strings"
	"time"
)

const (
	TypeResponse = 0
	TypeExec     = 2
	TypeAuthResp = 2
	TypeAuth     = 3
	maxPacket    = 1 << 20
)

// ErrAuth means the server rejected the password; retrying cannot help.
var ErrAuth = errors.New("rcon: wrong password")

// WritePacket writes one packet.
func WritePacket(w io.Writer, id, kind int32, body string) error {
	b := make([]byte, 0, len(body)+14)
	b = binary.LittleEndian.AppendUint32(b, uint32(len(body)+10))
	b = binary.LittleEndian.AppendUint32(b, uint32(id))
	b = binary.LittleEndian.AppendUint32(b, uint32(kind))
	b = append(b, body...)
	b = append(b, 0, 0)
	_, err := w.Write(b)
	return err
}

// ReadPacket reads one packet.
func ReadPacket(r io.Reader) (id, kind int32, body string, err error) {
	var size int32
	if err = binary.Read(r, binary.LittleEndian, &size); err != nil {
		return
	}
	if size < 10 || size > maxPacket {
		return 0, 0, "", fmt.Errorf("rcon: invalid packet size %d", size)
	}
	d := make([]byte, size)
	if _, err = io.ReadFull(r, d); err != nil {
		return
	}
	id = int32(binary.LittleEndian.Uint32(d[0:4]))
	kind = int32(binary.LittleEndian.Uint32(d[4:8]))
	return id, kind, string(d[8 : len(d)-2]), nil
}

// Client is one authenticated connection. It is not safe for concurrent use.
type Client struct {
	conn net.Conn
	next int32
}

// Dial connects and authenticates.
func Dial(addr, password string, timeout time.Duration) (*Client, error) {
	conn, err := net.DialTimeout("tcp", addr, timeout)
	if err != nil {
		return nil, err
	}
	_ = conn.SetDeadline(time.Now().Add(5 * time.Second))
	if err := WritePacket(conn, 1, TypeAuth, password); err != nil {
		conn.Close()
		return nil, err
	}
	// srcds answers with an empty packet and then the result (type 2; id -1 = wrong password)
	for {
		id, kind, _, err := ReadPacket(conn)
		if err != nil {
			conn.Close()
			return nil, fmt.Errorf("rcon: no answer to authentication: %w", err)
		}
		if kind == TypeAuthResp {
			if id == -1 {
				conn.Close()
				return nil, ErrAuth
			}
			return &Client{conn: conn, next: 10}, nil
		}
	}
}

// Exec runs a command and returns its whole response. srcds answers an empty packet after the
// response, so that second packet marks the end, as in crates/core/src/rcon.rs.
func (c *Client) Exec(cmd string, timeout time.Duration) (string, error) {
	if strings.ContainsAny(cmd, "\x00\n") {
		return "", errors.New("rcon: command with forbidden characters")
	}
	id, marker := c.next, c.next+1
	c.next += 2
	_ = c.conn.SetDeadline(time.Now().Add(timeout))
	if err := WritePacket(c.conn, id, TypeExec, cmd); err != nil {
		return "", err
	}
	if err := WritePacket(c.conn, marker, TypeResponse, ""); err != nil {
		return "", err
	}
	var out strings.Builder
	for {
		rid, _, body, err := ReadPacket(c.conn)
		if err != nil {
			return out.String(), fmt.Errorf("rcon: connection lost: %w", err)
		}
		if rid == marker {
			return out.String(), nil
		}
		if rid == id {
			out.WriteString(body)
		}
	}
}

// Close closes the connection.
func (c *Client) Close() error { return c.conn.Close() }

// CvarValue reads `"bot_quota" = "5" ( def. "0" )`.
func CvarValue(out string) string {
	f := strings.Split(out, "\"")
	if len(f) > 3 {
		return f[3]
	}
	return ""
}
