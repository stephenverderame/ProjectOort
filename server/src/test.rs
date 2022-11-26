use std::net::UdpSocket;
use std::thread;

use crate::{argument_parser::DEFAULT_PORT, run_game_server};
use shared_types::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
extern crate cgmath;
extern crate serial_test;

#[serial_test::serial]
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

    let make_client = |id: u16| {
        move || {
            let sock = UdpSocket::bind(("127.0.0.1", 8888 + id)).unwrap();
            let mut buf = [0; 1024];
            sock.send_to(format!("Client {}", id).as_bytes(), "127.0.0.1:8888")
                .unwrap();
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

#[serial_test::serial]
#[test]
fn login() {
    let done = Arc::new(AtomicBool::new(false));
    let d2 = done.clone();
    let server = thread::spawn(move || {
        let _ = run_game_server(Default::default(), d2);
    });
    let make_client = |id: u16| {
        assert!(id > 0);
        move || {
            let client_addr = ("127.0.0.1", DEFAULT_PORT + id);
            let server_addr = ("127.0.0.1", DEFAULT_PORT);
            let sock = UdpSocket::bind(client_addr).unwrap();
            let cmd = ClientCommandType::Login(format!("Client_{}", id));
            let _ = send_data(&sock, server_addr, &cmd, 0);
            //println!("Client {} sent", id);
            let mut data: ClientBuffer<ServerCommandType> = Default::default();
            if let Ok(Some((response, _))) = recv_data(&sock, &mut data) {
                #[allow(irrefutable_let_patterns)]
                if let ServerCommandType::ReturnLogin(_) = response {
                } else {
                    panic!("Unexpected response: {:?}", response);
                }
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

#[serial_test::serial]
#[test]
fn login_important() {
    let done = Arc::new(AtomicBool::new(false));
    let d2 = done.clone();
    let server = thread::spawn(move || {
        let _ = run_game_server(Default::default(), d2);
    });
    let make_client = |id: u16| {
        assert!(id > 0);
        move || {
            let client_addr = ("127.0.0.1", DEFAULT_PORT + id);
            let server_addr = ("127.0.0.1", DEFAULT_PORT);
            let sock = UdpSocket::bind(client_addr).unwrap();
            sock.connect(server_addr).unwrap();
            let cmd = ClientCommandType::Login(format!("Client_{}", id));
            let mut data: ClientBuffer<ServerCommandType> = Default::default();
            if let Ok(resp) = send_important(&sock, &cmd, 0, &mut data, Default::default()) {
                #[allow(irrefutable_let_patterns)]
                if let ServerCommandType::ReturnLogin(_) = resp {
                } else {
                    panic!("Unexpected response: {:?}", resp);
                }
            } else {
                panic!("Unexpected None");
            }
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

#[serial_test::serial]
#[test]
fn update_important() {
    use cgmath::*;
    use shared_types::node::*;
    let done = Arc::new(AtomicBool::new(false));
    let d2 = done.clone();
    let server = thread::spawn(move || {
        let _ = run_game_server(Default::default(), d2);
    });
    let make_client = |id: u16| {
        assert!(id > 0);
        move || {
            let client_addr = ("127.0.0.1", DEFAULT_PORT + id);
            let server_addr = ("127.0.0.1", DEFAULT_PORT);
            let sock = UdpSocket::bind(client_addr).unwrap();
            sock.connect(server_addr).unwrap();
            let cmd = ClientCommandType::Update(vec![node::to_remote_object(
                &Node::default().pos(point3(id as f64, id as f64, id as f64)),
                &vec3(0., 0., 0.),
                &vec3(0., 0., 0.),
                ObjectType::Ship,
                ObjectId::new(id as u32),
            )]);
            let mut data: ClientBuffer<ServerCommandType> = Default::default();
            if let Ok(resp) = send_important(&sock, &cmd, 0, &mut data, Default::default()) {
                if let ServerCommandType::Update(objs) = resp {
                    assert_eq!(objs.len(), (id - 1) as usize);
                    for (obj_node, vel, rot_vel, typ, obj_id) in
                        objs.iter().map(node::from_remote_object)
                    {
                        let obj_id = obj_id.as_underlying_type();
                        assert!(obj_id < (id as u32));
                        assert_relative_eq!(
                            obj_node.get_pos(),
                            point3(obj_id as f64, obj_id as f64, obj_id as f64)
                        );
                        assert_relative_eq!(vel, vec3(0., 0., 0.));
                        assert_relative_eq!(rot_vel, vec3(0., 0., 0.));
                        assert_eq!(typ, ObjectType::Ship);
                    }
                } else {
                    panic!("Unexpected response: {:?}", resp);
                }
            } else {
                panic!("Unexpected None");
            }
        }
    };
    thread::spawn(make_client(1)).join().unwrap();
    thread::spawn(make_client(2)).join().unwrap();
    thread::spawn(make_client(3)).join().unwrap();
    thread::spawn(make_client(4)).join().unwrap();
    done.store(true, Ordering::SeqCst);
    server.join().unwrap();
}
