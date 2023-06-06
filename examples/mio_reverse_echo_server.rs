// Based on https://github.com/tokio-rs/mio/blob/master/examples/tcp_server.rs
// $ cargo run --example mio_reverse_echo_server
// $ telnet 0 4242
// Trying 0.0.0.0...
// Connected to 0.
// Escape character is '^]'.
// some random data
// atad modnar emos
use std::{
    collections::HashMap,
    io::{self, Read, Write},
    str::from_utf8,
};

use mio::{
    event::Event,
    net::{TcpListener, TcpStream},
    {self, Events, Interest, Poll, Registry, Token},
};

const SERVER: Token = Token(0);

struct Conn {
    stream: TcpStream,
    buf: Vec<u8>,
    buf_len: usize,
}

struct EchoSrv {}

impl EchoSrv {
    fn token_next(curr: &mut Token) -> Token {
        let next = curr.0;
        curr.0 += 1;
        Token(next)
    }

    fn handle_event(registry: &Registry, conn: &mut Conn, event: &Event) -> io::Result<bool> {
        if event.is_writable() {
            let recv_data = &conn.buf[..conn.buf_len];

            let str_data = if let Ok(data) = from_utf8(recv_data) {
                let res = data.trim_end();
                format!("{}{}", res.chars().rev().collect::<String>(), "\r\n")
            } else {
                "__bad_input__".to_string()
            };

            match conn.stream.write(str_data.as_bytes()) {
                Ok(n) if n < str_data.len() => return Err(io::ErrorKind::WriteZero.into()),
                Ok(_) => {
                    registry.reregister(&mut conn.stream, event.token(), Interest::READABLE)?
                }
                Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {}
                Err(ref err) if err.kind() == io::ErrorKind::Interrupted => {
                    return EchoSrv::handle_event(registry, conn, event);
                }
                Err(err) => return Err(err),
            }
        }

        if event.is_readable() {
            let mut conn_closed = false;
            let mut bytes_read = 0;

            conn.buf.resize(4096, 0);

            loop {
                match conn.stream.read(&mut conn.buf[bytes_read..]) {
                    Ok(0) => {
                        conn_closed = true;
                        break;
                    }
                    Ok(n) => {
                        bytes_read += n;
                        if bytes_read == conn.buf.len() {
                            conn.buf.resize(conn.buf.len() + 4096, 0)
                        }
                        conn.buf_len = bytes_read;
                        registry.reregister(&mut conn.stream, event.token(), Interest::WRITABLE)?
                    }
                    Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => break,
                    Err(ref err) if err.kind() == io::ErrorKind::Interrupted => continue,
                    Err(err) => return Err(err),
                }
            }

            if conn_closed {
                println!("Conn closed");
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn run() -> io::Result<()> {
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(1024);
        let addr = format!("127.0.0.1:4242").parse().unwrap();
        let mut listener = TcpListener::bind(addr)?;

        poll.registry()
            .register(&mut listener, SERVER, Interest::READABLE)?;

        let mut connections: HashMap<Token, Conn> = HashMap::new();
        let mut unique_token = Token(SERVER.0 + 1);

        loop {
            poll.poll(&mut events, None)?;

            for event in events.iter() {
                match event.token() {
                    SERVER => loop {
                        let (mut conn, addr) = match listener.accept() {
                            Ok((conn, addr)) => (conn, addr),
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                break;
                            }
                            Err(e) => {
                                return Err(e);
                            }
                        };

                        println!("Accept from: {}", addr);

                        let token = EchoSrv::token_next(&mut unique_token);
                        poll.registry()
                            .register(&mut conn, token, Interest::READABLE)?;

                        let connection = Conn {
                            stream: conn,
                            buf: Vec::new(),
                            buf_len: 0,
                        };

                        connections.insert(token, connection);
                    },

                    token => {
                        let done = if let Some(conn) = connections.get_mut(&token) {
                            EchoSrv::handle_event(poll.registry(), conn, event)?
                        } else {
                            false
                        };

                        if done {
                            if let Some(mut conn) = connections.remove(&token) {
                                println!("Cleanup for {}", conn.stream.peer_addr()?);
                                poll.registry().deregister(&mut conn.stream)?;
                            }
                        }
                    }
                }
            }
        }
    }
}

fn main() -> io::Result<()> {
    EchoSrv::run()
}
