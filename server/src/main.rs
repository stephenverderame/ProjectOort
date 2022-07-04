mod argument_parser;
use std::net::*;

use argument_parser::ServerConfiguration;
use shared_types::Serializeable;
use std::error::Error;
use shared_types::*;
use std::collections::{BTreeMap, HashMap};

#[cfg(test)]
mod test;

type ClientData = RemoteData<ClientCommandType>;
type ClientBuffer = HashMap<SocketAddr, BTreeMap<CommandId, ClientData>>;

/// Inserts `msg` into `cmd_buffer`
/// 
/// Requires `cmd_buffer` does not hold a ready command
/// 
/// If `cmd_buffer` already holds a command, the new command is inserted into the buffer
fn insert_new_packet(packet_num: PacketNum, 
    msg: &[u8], mut cmd_buffer: &mut ClientData) 
{
    match &mut cmd_buffer {
        &mut ClientData::Buffering(buffer) => {
            if buffer.contains_key(&packet_num) {
                // Old command never received, new version of command coming in
                *buffer = BTreeMap::new();
            }
            buffer.insert(packet_num, msg.to_vec());
        },
        _ => panic!("Msg already ready"),
    }
}

fn recv_data(socket: &UdpSocket, data: &mut ClientBuffer) {
    let mut buf = [0; MAX_DATAGRAM_SIZE];
    if let Ok((amt, src)) = socket.recv_from(&mut buf) {
        let msg = &buf[..amt];
        let (cmd_id, packet_num) = get_cmd_id_and_packet_num(msg);
        let client_data = data.entry(src).or_insert(BTreeMap::new());
        let client_data_entry = client_data.entry(cmd_id)
            .or_insert(ClientData::Buffering(BTreeMap::new()));
        insert_new_packet(packet_num, msg, client_data_entry);
        if client_data_entry.is_ready() {
            let cmd = client_data.remove(&cmd_id).unwrap();
            if let Ok(cmd) = cmd.to_ready() {
                client_data.insert(cmd_id, cmd);
            }
        }
    }
    
}

fn run_game_server(config: ServerConfiguration) -> Result<(), Box<dyn Error>> {
    let socket = UdpSocket::bind(("127.0.0.1", config.port))?;
    let mut buf = [0; 1024];
    loop {
        let (amt, src) = socket.recv_from(&mut buf)?;
        println!("Received {} bytes from {}", amt, src);
        let response = format!("Hello {}!", src);
        socket.send_to(response.as_bytes(), &src)?;
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = argument_parser::parse_args(std::env::args())?;

    println!("Starting server with config:\n{}", config);

    run_game_server(config)
}
