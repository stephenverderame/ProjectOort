use std::thread;
use std::net::UdpSocket;

use crate::{run_game_server, argument_parser::DEFAULT_PORT};
use std::sync::{atomic::{Ordering, AtomicBool}, Arc};
use shared_types::*;

#[test]
fn server_multiplex() {
    let server = thread::spawn(|| {
        let sock = UdpSocket::bind("127.0.0.1:8888").unwrap();
        let mut buf = [0; 1024];
        for _ in 0..2 {
            let (amt, src) = sock.recv_from(&mut buf).unwrap();
            let response = &buf[..amt];
            //println!("Server got \"{}\"", String::from_utf8_lossy(response));
            sock.send_to(response, &src).unwrap();
        }
        //println!("Server done");
    });

    let make_client = |id : u16| {
        move || {
            let sock = UdpSocket::bind(("127.0.0.1", 8888 + id)).unwrap();
            let mut buf = [0; 1024];
            sock.send_to(format!("Client {}", id).as_bytes(), "127.0.0.1:8888").unwrap();
            //println!("Client {} sent", id);
            let (amt, _) = sock.recv_from(&mut buf).unwrap();
            assert_eq!(&buf[..amt], format!("Client {}", id).as_bytes());
            //println!("Client {} got \"{}\"", id, String::from_utf8_lossy(&buf[..amt]));
        }
    };

    let client_1 = thread::spawn(make_client(1));
    make_client(2)();
    server.join().unwrap();
    client_1.join().unwrap();


}

#[test]
fn login() {
    let done = Arc::new(AtomicBool::new(false));
    let d2 = done.clone();
    let server = thread::spawn(move || {
        let _ = run_game_server(Default::default(), d2);
    });
    let make_client = |id : u16| {
        assert!(id > 0);
        move || {
            let client_addr = ("127.0.0.1", DEFAULT_PORT + id);
            let server_addr = ("127.0.0.1", DEFAULT_PORT);
            let sock = UdpSocket::bind(client_addr).unwrap();
            let cmd = ClientCommandType::Login(format!("Client_{}", id));
            let _ = send_data(&sock, server_addr,  &cmd, 0);
            //println!("Client {} sent", id);
            let mut data : ClientBuffer<ServerCommandType> = Default::default();
            if let Some((response, _)) = recv_data(&sock, &mut data) {
                #[allow(irrefutable_let_patterns)]
                if let ServerCommandType::ReturnId(_) = response { }
                else { panic!("Unexpected response: {:?}", response); }
            } else {
                panic!("Unexpected None");
            }
            //println!("Client {} got \"{}\"", id, String::from_utf8_lossy(&buf[..amt]));
        }
    };
    let c1 = thread::spawn(make_client(1));
    let c2 = thread::spawn(make_client(2));
    let c3 = thread::spawn(make_client(3));
    c1.join().unwrap();
    c2.join().unwrap();
    c3.join().unwrap();
    make_client(4)();
    done.store(true, Ordering::SeqCst);
    server.join().unwrap();



}