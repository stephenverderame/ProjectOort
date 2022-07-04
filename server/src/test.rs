use std::thread;
use std::net::UdpSocket;

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