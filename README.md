# Bevy Multiplayer Demo

This is a simple learning project built with [Bevy](https://bevyengine.org/) and [Renet2](https://github.com/UkoeHB/renet2).

It demonstrates core concepts like client-side prediction, server authority, and synchronizing entity states over the network.

---

## How to Use

1. **Start the server:**

```bash
cargo r -- server
```

Start the client in a new terminal:

```bash
cargo r -- client
```

(Optional) Start a second client instance to simulate multiple connected players:

```bash
cargo r -- client
```

## What It Does

This project creates a basic multiplayer environment where each client controls a colored square. The server maintains an authoritative state of all connected players and collectable boxes in the world.


- Server-rendered physics / full server authority on position.
- Players can collect boxes by moving near them. The server handles collection validation and broadcasts which boxes should be despawned to all clients.
- Remote players are spawned and despawned.