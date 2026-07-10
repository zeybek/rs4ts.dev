---
title: "Low-Level Networking"
description: "Build blocking TCP and UDP servers in Rust with std::net, using a thread per connection where Node's net module leans on a single event loop."
---

The Rust standard library ships a small, synchronous, blocking networking API in `std::net`: TCP via `TcpListener`/`TcpStream` and UDP via `UdpSocket`. It is the rough equivalent of Node's `net` and `dgram` modules, but blocking and thread-based instead of event-loop-based.

---

## Quick Overview

`std::net` gives you raw TCP and UDP sockets with no runtime, no framework, and no dependencies. The calls are **blocking**: `accept()`, `read()`, and `write()` each park the current thread until the OS has data, so you get concurrency by spawning a thread per connection rather than by registering callbacks on an event loop.

For a TypeScript/JavaScript developer the mental model flips: in Node, `net.createServer` is non-blocking and single-threaded by default; in Rust's `std::net` every socket operation blocks, and you reach for [threads](/26-systems-programming/00-threads/) (or an async runtime like Tokio) to handle many clients at once. This file covers the blocking standard-library path: perfect for CLI tools, internal services, and learning how sockets actually work. For high-concurrency production servers you would usually move to `tokio::net`, which mirrors this API in an async form.

> **Note:** `std::net` is intentionally minimal. It has no TLS, no HTTP, and no connection pooling. For HTTPS clients reach for `reqwest`; for HTTP servers reach for `axum`; for encrypted transport see [Section 27: Security](/27-security/).

---

## TypeScript/JavaScript Example

In Node, a TCP echo server is built around the event loop. `net.createServer` registers a callback that fires for each connection, and you attach `data`/`end` listeners to each socket.

```typescript
// echo-server.ts — Node v22, run with: node --experimental-strip-types echo-server.ts
import net from "node:net";

const server = net.createServer((socket) => {
  console.log(`New connection from ${socket.remoteAddress}:${socket.remotePort}`);

  // 'data' fires every time bytes arrive; echo them straight back.
  socket.on("data", (chunk: Buffer) => {
    socket.write(chunk);
  });

  socket.on("end", () => {
    console.log("Connection closed by client");
  });

  socket.on("error", (err) => {
    console.error("socket error:", err.message);
  });
});

server.listen(7878, "127.0.0.1", () => {
  console.log("Echo server listening on 127.0.0.1:7878");
});
```

A matching client:

```typescript
// echo-client.ts
import net from "node:net";

const client = net.connect(7878, "127.0.0.1", () => {
  client.write("hello echo");
});

client.on("data", (data: Buffer) => {
  console.log("Server replied:", data.toString());
  client.end();
});
```

**Key points:**

- One thread, one event loop; thousands of idle connections cost almost nothing.
- You never call `read()` yourself; the runtime pushes `data` events at you.
- `chunk` is a `Buffer`; there is no built-in framing, so a single logical message can arrive split across several `data` events.

---

## Rust Equivalent

The standard-library version is blocking. `listener.incoming()` yields one connection at a time; we hand each off to a freshly spawned thread so the accept loop can keep going.

```rust
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn handle_client(mut stream: TcpStream) -> std::io::Result<()> {
    let peer = stream.peer_addr()?;
    println!("New connection from {peer}");

    let mut buf = [0u8; 1024];
    loop {
        // read() blocks until bytes arrive. It returns Ok(0) at EOF,
        // i.e. when the peer has closed its half of the connection.
        let n = stream.read(&mut buf)?;
        if n == 0 {
            println!("Connection closed by {peer}");
            return Ok(());
        }
        // Echo exactly the bytes we received back to the client.
        stream.write_all(&buf[..n])?;
    }
}

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:7878")?;
    println!("Echo server listening on {}", listener.local_addr()?);

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                // One thread per connection. The closure takes ownership
                // of `stream` via `move`, so it lives as long as the thread.
                thread::spawn(move || {
                    if let Err(e) = handle_client(stream) {
                        eprintln!("client error: {e}");
                    }
                });
            }
            Err(e) => eprintln!("accept failed: {e}"),
        }
    }
    Ok(())
}
```

A matching client:

```rust playground
use std::io::{Read, Write};
use std::net::TcpStream;

fn main() -> std::io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:7878")?;
    println!("Connected to {}", stream.peer_addr()?);

    stream.write_all(b"hello echo")?;

    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf)?;
    let reply = String::from_utf8_lossy(&buf[..n]);
    println!("Server replied: {reply}");
    Ok(())
}
```

Running the server in one terminal and the client in another produces this **real** output:

```text
# client terminal
Connected to 127.0.0.1:7878
Server replied: hello echo
```

```text
# server terminal
Echo server listening on 127.0.0.1:7878
New connection from 127.0.0.1:50777
Connection closed by 127.0.0.1:50777
```

The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically, and everything above is plain `std` — no `cargo add` required.

---

## Detailed Explanation

### Binding and accepting

`TcpListener::bind("127.0.0.1:7878")` asks the OS for a listening socket on that address and port. The argument implements `ToSocketAddrs`, so you can pass `"127.0.0.1:7878"`, `("127.0.0.1", 7878)`, a parsed `SocketAddr`, or even a hostname like `"localhost:7878"` (which resolves via DNS and may yield several addresses). Binding to `"0.0.0.0:7878"` listens on every IPv4 interface; `"127.0.0.1"` restricts it to the loopback (local-only) interface.

`listener.incoming()` returns an iterator of `Result<TcpStream>`. Each call to `.next()` performs a blocking `accept()`: the thread sleeps until a client connects. Because it is an infinite iterator, the `for` loop is your server's main loop.

### The `Read` and `Write` traits

A `TcpStream` is just a byte stream. It implements the standard [`std::io::Read`](https://doc.rust-lang.org/std/io/trait.Read.html) and [`std::io::Write`](https://doc.rust-lang.org/std/io/trait.Write.html) traits, the very same traits files implement. That is why you must bring them into scope with `use std::io::{Read, Write}`; the `read`/`write_all` methods come from those traits, not from `TcpStream` itself.

- `read(&mut buf)` fills part of `buf` and returns how many bytes it wrote, as `Ok(n)`. **`Ok(0)` means end-of-stream**: the peer closed the connection. This is the single most important contract to internalize: you loop until you see `Ok(0)`.
- `write_all(&buf[..n])` keeps calling the underlying `write` until every byte is sent or an error occurs. Prefer it over raw `write`, which (like POSIX `write`) may send only part of the buffer.

### Why a thread per connection

A blocking `read()` parks the whole thread. If you handled clients inline in the accept loop, a single slow client would freeze the server for everyone else. Spawning a thread per connection (`thread::spawn(move || ...)`) gives each client its own blocking stack. The `move` keyword transfers ownership of `stream` into the closure so it outlives the loop iteration. See [Section 26: Threads](/26-systems-programming/00-threads/) for the spawn/join mechanics, and [Thread Pools](/26-systems-programming/01-thread-pools/) if you want to cap the number of live threads.

### EOF vs. error

`read` returning `Ok(0)` is a clean shutdown, not an error. An actual failure (connection reset, broken pipe) comes back as `Err(e)`, which the `?` operator propagates out of `handle_client`, where the spawned closure logs it. This separation of "stream ended" from "stream broke" is more explicit than Node's split between the `end` and `error` events.

---

## Key Differences

| Concept | Node.js (`net`/`dgram`) | Rust (`std::net`) |
| --- | --- | --- |
| Concurrency model | Single-threaded event loop, non-blocking | Blocking calls + one thread per connection |
| Receiving data | `socket.on("data", cb)` push | You call `stream.read(&mut buf)` (pull) |
| End of stream | `"end"` event | `read` returns `Ok(0)` |
| Errors | `"error"` event | `Result` returned from each call |
| Partial writes | Hidden by the runtime's buffering | `write` may be partial; use `write_all` |
| Message framing | None (TCP is a byte stream) | None (TCP is a byte stream) |
| Buffer type | `Buffer` (auto-allocated per chunk) | `&[u8]` / `&mut [u8]` you allocate yourself |
| Bidirectional split | One `socket` object does both | One stream; `try_clone()` for separate read/write handles |
| TLS / HTTP | Built into `tls`/`http` modules | Not in `std`; use `rustls`, `reqwest`, `axum` |

The deepest difference is the **blocking-vs-event-loop** split. In Node, idle connections are nearly free, but CPU-bound work in a handler blocks every other connection. In blocking Rust, each connection costs a real OS thread (a megabyte or two of stack), but CPU work in one handler does not stall the others, and the code reads top-to-bottom with no callback nesting. When you outgrow thread-per-connection, you move to async (`tokio::net`), which keeps a near-identical API while multiplexing thousands of connections onto a few threads.

> **Note:** TCP is a stream of bytes, not a stream of messages — in *both* languages. One `write_all(b"hello")` is not guaranteed to surface as one `read`. If your protocol has discrete messages you must add framing yourself (length prefixes, newline delimiters, etc.). The Real-World Example below uses newline framing.

---

## UDP: connectionless datagrams

UDP has no connection, no accept loop, and no streams, just a socket you `send_to`/`recv_from`. It maps to Node's `dgram` module.

```rust
use std::net::UdpSocket;

fn main() -> std::io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:9000")?;
    println!("UDP echo server on {}", socket.local_addr()?);

    let mut buf = [0u8; 1500]; // a buffer larger than a typical MTU
    loop {
        // recv_from blocks until one datagram arrives, returning
        // the byte count and the sender's address.
        let (n, src) = socket.recv_from(&mut buf)?;
        println!("Got {n} bytes from {src}");
        socket.send_to(&buf[..n], src)?; // echo the datagram back
    }
}
```

A UDP client. Calling `connect` on a UDP socket does not open a connection; it just sets a default peer so you can use `send`/`recv` instead of `send_to`/`recv_from`:

```rust playground
use std::net::UdpSocket;

fn main() -> std::io::Result<()> {
    // Port 0 tells the OS to pick a free ephemeral port for us.
    let socket = UdpSocket::bind("127.0.0.1:0")?;
    socket.connect("127.0.0.1:9000")?;

    socket.send(b"ping")?;

    let mut buf = [0u8; 1500];
    let n = socket.recv(&mut buf)?;
    println!("Echoed back: {}", String::from_utf8_lossy(&buf[..n]));
    Ok(())
}
```

Real output from running the server and then the client (the server's loop replaced with a single `recv_from` for the demo):

```text
# client
Echoed back: ping
```

```text
# server
UDP echo server on 127.0.0.1:9000
Got 4 bytes from 127.0.0.1:53497
```

> **Warning:** Always size your UDP receive buffer to hold the largest datagram you expect (1500 is a common Ethernet MTU). If a datagram is larger than your buffer, the excess bytes are **silently discarded** — `recv_from` returns the truncated length and the rest is gone, unlike TCP where a short read just leaves the remainder for the next call.

---

## Common Pitfalls

### Forgetting `use std::io::{Read, Write}`

`read`, `write`, and `write_all` are trait methods. Without the trait in scope the compiler cannot find them, even though the type is correct. The fix is the import; the error otherwise reads like "no method named `read` found for struct `TcpStream`... method is available... use `std::io::Read`".

### Treating one `read` as one message

```rust
use std::io::Read;
use std::net::TcpStream;

fn read_message(mut stream: TcpStream) -> std::io::Result<String> {
    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf)?;          // logic bug, not a compile error
    Ok(String::from_utf8_lossy(&buf[..n]).into_owned())
}
```

This compiles and *usually* works on loopback, which lulls you into a false sense of security. Over a real network the client's `write_all(b"hello world")` can split across two TCP segments, so the first `read` returns only `"hello "`. TCP guarantees *order*, not *message boundaries*. Use a loop, a `BufReader::read_line`, or a length-prefix protocol instead.

### Reading and writing through the same `&mut` stream from two threads

You cannot read in one thread and write in another using a single owned `TcpStream`, because both need `&mut self`. Use `stream.try_clone()?` to get a second independent handle to the same underlying socket — one for the reader thread, one for the writer thread. (The Real-World Example does exactly this.)

### Moving the stream into a thread, then using it again

```rust
use std::net::TcpStream;
use std::thread;

fn main() {
    let stream = TcpStream::connect("127.0.0.1:7878").unwrap();
    thread::spawn(move || {
        let _ = stream.peer_addr();
    });
    let _ = stream.peer_addr(); // does not compile (error[E0382]: borrow of moved value)
}
```

The real compiler error:

```text
error[E0382]: borrow of moved value: `stream`
  --> src/main.rs:10:13
   |
 5 |     let stream = TcpStream::connect("127.0.0.1:7878").unwrap();
   |         ------ move occurs because `stream` has type `TcpStream`, which does not implement the `Copy` trait
 6 |     thread::spawn(move || {
   |                   ------- value moved into closure here
 7 |         let _ = stream.peer_addr();
   |                 ------ variable moved due to use in closure
...
10 |     let _ = stream.peer_addr();
   |             ^^^^^^ value borrowed here after move
```

If both the spawned thread and `main` need the socket, call `try_clone()` *before* the `move` and give each side its own handle.

### "Address already in use"

Binding to a port that is already taken (or recently used and still in `TIME_WAIT`) fails. Reproduced by binding the same port twice in one process:

```rust
use std::net::TcpListener;

fn main() {
    let _first = TcpListener::bind("127.0.0.1:7878").expect("first bind");
    match TcpListener::bind("127.0.0.1:7878") {
        Ok(_) => println!("second bind ok?!"),
        Err(e) => println!("kind={:?} -> {e}", e.kind()),
    }
}
```

Real output:

```text
kind=AddrInUse -> Address already in use (os error 48)
```

Always match on `e.kind()` (an [`ErrorKind`](https://doc.rust-lang.org/std/io/enum.ErrorKind.html)) rather than the OS-specific message string, since the number (`48` here on macOS, `98` on Linux) differs per platform.

### Connecting to nothing

`TcpStream::connect` to a closed port returns `ErrorKind::ConnectionRefused`:

```text
kind=ConnectionRefused -> Connection refused (os error 61)
```

Match on the kind so your retry/backoff logic is portable.

---

## Best Practices

### Bound your operations with timeouts

A blocking `read` with no data, or a `connect` to a black-holed host, will otherwise hang the thread for the OS default (often ~75 seconds for `connect`). Set explicit limits:

```rust playground
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

fn main() {
    let addr: SocketAddr = "127.0.0.1:7878".parse().unwrap();
    match TcpStream::connect_timeout(&addr, Duration::from_secs(2)) {
        Ok(stream) => {
            // Cap how long any single read/write may block.
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            stream.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
            println!("connected, timeouts set");
        }
        Err(e) => println!("connect failed: {e}"),
    }
}
```

When a read timeout fires, `read` returns `Err` with kind `WouldBlock` (`TimedOut` on some platforms), not `Ok(0)`. Real output from a 200 ms read timeout that elapses with no data:

```text
listening on 127.0.0.1:50981
timed out: kind=WouldBlock -> Resource temporarily unavailable (os error 35)
```

### Disable Nagle for latency-sensitive traffic

For request/response protocols, set `stream.set_nodelay(true)` to disable Nagle's algorithm so small writes are not buffered waiting for more data.

### Buffer your reads and writes

Wrap streams in `BufReader`/`BufWriter` to coalesce many small syscalls into a few large ones, and to get convenient line-based helpers like `read_line` and `lines()`. Remember to `flush()` a `BufWriter` (or rely on `writeln!` + an explicit flush) before you expect the peer to see the bytes.

### Match on `ErrorKind`, never on the message string

Error messages and OS error numbers are platform-specific; `ErrorKind` is portable. Branch on `e.kind() == ErrorKind::ConnectionRefused`, not on `e.to_string().contains("refused")`.

### Reach for async when connections scale

Thread-per-connection is excellent up to a few hundred concurrent clients. Beyond that, the per-thread stack memory and context-switching cost dominate, and `tokio::net::{TcpListener, TcpStream}` — which mirror these APIs in async form — let a handful of threads service tens of thousands of connections. Treat `std::net` as the foundation, not the ceiling.

---

## Real-World Example

A line-based echo server that handles many clients concurrently, frames messages on newlines, supports a `quit` command, and uses `try_clone()` to read and write the same socket. This is the shape of a real internal protocol service (a debug console, a metrics ingestion port, a simple chat backend).

```rust
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

fn handle_client(stream: TcpStream) -> std::io::Result<()> {
    let peer = stream.peer_addr()?;

    // try_clone() gives us a second handle to the SAME socket, so we can
    // read through `reader` and write through `writer` independently.
    let mut writer = stream.try_clone()?;
    let reader = BufReader::new(stream);

    // BufReader::lines() splits the byte stream on '\n' for us — proper
    // message framing instead of trusting one read == one message.
    for line in reader.lines() {
        let line = line?;
        println!("[{peer}] {line}");

        if line.trim() == "quit" {
            writer.write_all(b"bye\n")?;
            break;
        }
        // Echo each line back, uppercased. writeln! adds the trailing '\n'.
        writeln!(writer, "{}", line.to_uppercase())?;
    }

    println!("[{peer}] disconnected");
    Ok(())
}

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:7878")?;
    println!("Line echo server on {}", listener.local_addr()?);

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                thread::spawn(move || {
                    if let Err(e) = handle_client(stream) {
                        eprintln!("client error: {e}");
                    }
                });
            }
            Err(e) => eprintln!("accept failed: {e}"),
        }
    }
    Ok(())
}
```

A small client that drives it through a `hello`/`world`/`quit` exchange:

```rust playground
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;

fn main() -> std::io::Result<()> {
    let stream = TcpStream::connect("127.0.0.1:7878")?;
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    for msg in ["hello", "world", "quit"] {
        writeln!(writer, "{msg}")?;           // send one line
        let mut line = String::new();
        reader.read_line(&mut line)?;          // read one line back
        print!("server: {line}");              // the line already ends in '\n'
    }
    Ok(())
}
```

Real captured output:

```text
# client
server: HELLO
server: WORLD
server: bye
```

```text
# server
Line echo server on 127.0.0.1:7878
[127.0.0.1:50962] hello
[127.0.0.1:50962] world
[127.0.0.1:50962] quit
[127.0.0.1:50962] disconnected
```

Notice the framing: even though TCP could have delivered `hello\nworld\n` in one packet, `BufReader::lines()` and `read_line` split it into exactly the lines we sent. That is the framing discipline TCP itself does not give you.

---

## Further Reading

- [`std::net` module](https://doc.rust-lang.org/std/net/index.html) — official documentation for `TcpListener`, `TcpStream`, `UdpSocket`, and `SocketAddr`.
- [`std::io::Read`](https://doc.rust-lang.org/std/io/trait.Read.html) and [`std::io::Write`](https://doc.rust-lang.org/std/io/trait.Write.html): the traits every socket implements.
- [`std::io::ErrorKind`](https://doc.rust-lang.org/std/io/enum.ErrorKind.html): portable, matchable error categories.
- [Tokio: `tokio::net`](https://docs.rs/tokio/latest/tokio/net/index.html) — the async counterpart for high-concurrency servers.
- Within this guide:
  - [Threads](/26-systems-programming/00-threads/): the spawn/join model behind thread-per-connection.
  - [Thread Pools](/26-systems-programming/01-thread-pools/): capping concurrency instead of one thread per client.
  - [Channels](/26-systems-programming/03-channels/): moving received messages to worker threads.
  - [Signals](/26-systems-programming/08-signals/) — clean shutdown of a server on SIGINT/SIGTERM.
  - [Section 27: Security](/27-security/) — adding TLS and validating untrusted input.
  - Foundational background: [Getting Started](/01-getting-started/) and [Basics](/02-basics/).

---

## Exercises

### Exercise 1: Count bytes, then disconnect

**Difficulty:** Beginner

**Objective:** Get comfortable with the blocking read loop and the `Ok(0)` EOF contract.

**Instructions:** Write a TCP server that accepts one connection, reads from it until EOF, prints the total number of bytes received, and then exits. Use a single accept (no thread spawning needed). Verify it by connecting with a client that writes a known string and then closes its half of the connection.

<details>
<summary>Solution</summary>

```rust
use std::io::Read;
use std::net::TcpListener;

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:7878")?;
    println!("counting server on {}", listener.local_addr()?);

    // accept() returns the first connection; ignore the peer address here.
    let (mut stream, _peer) = listener.accept()?;

    let mut buf = [0u8; 1024];
    let mut total = 0usize;
    loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            break; // peer closed its write half -> EOF
        }
        total += n;
    }
    println!("received {total} bytes total");
    Ok(())
}
```

</details>

### Exercise 2: A networked key-value store

**Difficulty:** Intermediate

**Objective:** Combine threads, shared state, message framing, and a tiny text protocol.

**Instructions:** Build a multi-client TCP server speaking a line protocol: `SET <key> <value>` stores a value and replies `OK`; `GET <key>` replies with the value or `(nil)`; `QUIT` closes the connection. Share the map across all client threads with `Arc<Mutex<HashMap<String, String>>>`. Use `BufReader::lines()` for framing.

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

type Store = Arc<Mutex<HashMap<String, String>>>;

fn handle(stream: TcpStream, store: Store) -> std::io::Result<()> {
    let mut writer = stream.try_clone()?;
    let reader = BufReader::new(stream);

    for line in reader.lines() {
        let line = line?;
        // Split into at most 3 parts: command, key, value.
        let mut parts = line.splitn(3, ' ');
        match parts.next() {
            Some("SET") => {
                if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                    store.lock().unwrap().insert(k.to_string(), v.to_string());
                    writeln!(writer, "OK")?;
                } else {
                    writeln!(writer, "ERR usage: SET <key> <value>")?;
                }
            }
            Some("GET") => {
                let key = parts.next().unwrap_or("");
                match store.lock().unwrap().get(key) {
                    Some(v) => writeln!(writer, "{v}")?,
                    None => writeln!(writer, "(nil)")?,
                }
            }
            Some("QUIT") => break,
            _ => writeln!(writer, "ERR unknown command")?,
        }
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:7878")?;
    println!("kv store on {}", listener.local_addr()?);

    let store: Store = Arc::new(Mutex::new(HashMap::new()));
    for incoming in listener.incoming() {
        let stream = incoming?;
        let store = store.clone(); // clone the Arc, not the map
        thread::spawn(move || {
            if let Err(e) = handle(stream, store) {
                eprintln!("client error: {e}");
            }
        });
    }
    Ok(())
}
```

> **Tip:** `store.clone()` clones the `Arc` (a cheap reference-count bump), giving every thread shared ownership of the *same* map. See [Channels](/26-systems-programming/03-channels/) for an alternative that avoids the shared lock by routing all mutations through one owner thread.

</details>

### Exercise 3: A UDP time service

**Difficulty:** Intermediate

**Objective:** Practice connectionless datagrams and a full request/response round trip over UDP.

**Instructions:** Write a UDP server that, on receiving a datagram containing `TIME`, replies with the current Unix timestamp (seconds since the epoch) as ASCII text. Write a client that sends `TIME`, prints the reply, and exits. Bind the client to port `0` so the OS assigns an ephemeral port.

<details>
<summary>Solution</summary>

```rust playground
use std::net::UdpSocket;
use std::time::{SystemTime, UNIX_EPOCH};

fn run_server(socket: &UdpSocket) -> std::io::Result<()> {
    let mut buf = [0u8; 64];
    let (n, src) = socket.recv_from(&mut buf)?;
    if &buf[..n] == b"TIME" {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        socket.send_to(now.to_string().as_bytes(), src)?;
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    // Server and client in one process for a self-contained demo.
    let server = UdpSocket::bind("127.0.0.1:0")?;
    let server_addr = server.local_addr()?;

    let client = UdpSocket::bind("127.0.0.1:0")?; // port 0 -> OS picks one
    client.send_to(b"TIME", server_addr)?;

    run_server(&server)?;

    let mut buf = [0u8; 64];
    let n = client.recv(&mut buf)?;
    println!("server time = {}", String::from_utf8_lossy(&buf[..n]));
    Ok(())
}
```

Real output (your timestamp will differ):

```text
server time = 1780382543
```

> **Note:** UDP gives no delivery guarantee. In production you would set a read timeout on the client and retry if no reply arrives, since either the request or the response can be silently dropped.

</details>
