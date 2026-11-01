"""Puente UDP para amigos que entran por VPN (Tailscale, etc.) con el servidor en modo LAN (SV_LAN=1).

El servidor en modo LAN solo acepta IPs de red local (192.168.x, 10.x). El puente recibe a los amigos en
PUERTO+1 y reenvía su tráfico al servidor desde la IP local de este PC, así el servidor los ve como locales.

Steam además rechaza (STEAMAUTH código 10) a quien se conecta a una IP de VPN que no es privada (100.x de Tailscale),
así que el amigo lanza también un puente en su PC y se conecta a su propia IP local.

Uso:
  python3 -I puente.py                         en el PC del servidor (lee PUERTO de config.env, escucha en PUERTO+1)
  python3 -I puente.py 27030 100.107.0.27:27031  en el PC del amigo: escucha en 27030 y reenvía al puente del servidor;
                                               él se conecta con  connect <su IP local>:27030
"""
import os
import selectors
import socket
import subprocess
import sys
import time

INACTIVO = 120  # segundos sin tráfico para olvidar a un jugador


def ip_local():
    out = subprocess.run(["ip", "-4", "route", "get", "1.1.1.1"], capture_output=True, text=True).stdout.split()
    return out[out.index("src") + 1]


def main():
    local = ip_local()
    if len(sys.argv) == 3:  # puente del amigo: escucha en su PC y reenvía al puente del servidor
        escucha_puerto = int(sys.argv[1])
        ip, p = sys.argv[2].rsplit(":", 1)
        servidor, origen_bind = (ip, int(p)), "0.0.0.0"
    else:
        cfg = {}
        with open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "config.env")) as f:
            for linea in f:
                if "=" in linea and not linea.lstrip().startswith("#"):
                    k, v = linea.strip().split("=", 1)
                    cfg[k] = v
        puerto = int(cfg["PUERTO"])
        escucha_puerto, servidor, origen_bind = puerto + 1, (local, puerto), local

    escucha = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    escucha.bind(("0.0.0.0", escucha_puerto))
    sel = selectors.DefaultSelector()
    sel.register(escucha, selectors.EVENT_READ, None)
    clientes = {}  # dirección del amigo -> [socket hacia el servidor, último uso]
    print(f"Puente: {local}:{escucha_puerto}  ->  {servidor[0]}:{servidor[1]}", flush=True)

    while True:
        for key, _ in sel.select(timeout=10):
            s = key.fileobj
            datos, origen = s.recvfrom(65535)
            if key.data is None:  # paquete de un amigo
                c = clientes.get(origen)
                if c is None:
                    hacia = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
                    hacia.bind((origen_bind, 0))  # en el servidor sale con la IP local: lo ve en la red local
                    sel.register(hacia, selectors.EVENT_READ, origen)
                    c = clientes[origen] = [hacia, 0.0]
                    print(f"Puente: nuevo jugador {origen[0]}:{origen[1]}", flush=True)
                c[1] = time.monotonic()
                c[0].sendto(datos, servidor)
            elif origen == servidor:  # respuesta del servidor -> al amigo correspondiente
                escucha.sendto(datos, key.data)
                clientes[key.data][1] = time.monotonic()
        ahora = time.monotonic()
        for origen, (hacia, uso) in list(clientes.items()):
            if ahora - uso > INACTIVO:
                sel.unregister(hacia)
                hacia.close()
                del clientes[origen]


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        sys.exit(0)
