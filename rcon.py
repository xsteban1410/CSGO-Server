"""Cliente RCON mínimo (protocolo Source). Uso: python3 rcon.py "comando" ["comando"...]
Lee RCON_HOST, PUERTO y RCON_PASSWORD de config.env (junto a este archivo)."""
import os
import socket
import struct
import sys


def config():
    cfg = {}
    with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "config.env")) as f:
        for line in f:
            line = line.split("#", 1)[0].strip()
            if "=" in line:
                k, v = line.split("=", 1)
                cfg[k.strip()] = v.strip().strip('"')
    return cfg


def packet(req_id, kind, body):
    data = struct.pack("<ii", req_id, kind) + body.encode() + b"\0\0"
    return struct.pack("<i", len(data)) + data


def read(sock):
    size = struct.unpack("<i", sock.recv(4))[0]
    data = b""
    while len(data) < size:
        data += sock.recv(size - len(data))
    return struct.unpack("<i", data[:4])[0], data[8:-2].decode(errors="replace")


def main():
    cfg = config()
    s = socket.create_connection((cfg.get("RCON_HOST", "127.0.0.1"), int(cfg["PUERTO"])), timeout=5)
    s.send(packet(1, 3, cfg["RCON_PASSWORD"]))
    # el servidor responde con un paquete vacío y luego el resultado de la autenticación (-1 = fallo)
    read(s)
    if read(s)[0] == -1:
        sys.exit("rcon: contraseña incorrecta")
    for cmd in sys.argv[1:]:
        s.send(packet(2, 2, cmd))
        print(read(s)[1])


if __name__ == "__main__":
    main()
