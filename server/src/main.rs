mod argument_parser;
use std::net::*;

use argument_parser::ServerConfiguration;
use std::error::Error;
use shared_types::*;
use std::collections::{HashMap};
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};

#[cfg(test)]
mod test;

/// The current state of the client
/// 
/// Cannot be created from multiple threads
/// 
/// For implementing a state machine
enum ClientState {
    WaitingForAck,
    WaitingForRequest,
}

/// The data for each client
struct ClientData {
    state: ClientState,
    username: String,
    id: u32,
    last_msg_id: u32,
    client_objects: Vec<RemoteObject>,
}

impl Default for ClientData {
    fn default() -> Self {
        use std::sync::atomic::AtomicU32;
        static mut ID: AtomicU32 = AtomicU32::new(0);

        // safe bc atomic
        let id = unsafe { ID.fetch_add(1, Ordering::AcqRel) };
        ClientData {
            state: ClientState::WaitingForRequest,
            username: String::new(),
            id, last_msg_id: 0,
            client_objects: Vec::new(),
        }
    }
}

/// The data for the server
struct ServerState {
    users: HashMap<SocketAddr, ClientData>,
}

impl ServerState {
    fn get_all_objects(&self, requesting_client: &SocketAddr) -> Vec<RemoteObject> {
        self.users.iter().filter(|(client_addr, _)| requesting_client != *client_addr)
            .flat_map(|(_, client_data)| client_data.client_objects.iter())
            .map(|e| *e).collect()
    }
}

impl Default for ServerState {
    fn default() -> Self {
        ServerState {
            users: Default::default(),
        }
    }
}

#[inline]
fn respond_to_msg(msg: ClientCommandType, socket: &UdpSocket, 
    addr: SocketAddr, mut state: ServerState) -> ServerState 
{
    use ClientCommandType::*;
    let mut user_state = state.users.entry(addr).or_insert(Default::default());
    let last_msg_id = user_state.last_msg_id;
    user_state.last_msg_id += 1;
    let response = match (msg, user_state) {
        (Login(username), user_state) => {
            user_state.username = username;
            //user_state.state = WaitingForAck;
            ServerCommandType::ReturnId(user_state.id)
        },
        (Update(objects), user_state) => {
            user_state.client_objects = objects;
            ServerCommandType::Update(state.get_all_objects(&addr))
        },
    };
    if let Err(error) = send_data(socket, &addr, &response, last_msg_id) {
        println!("Error sending data: {}", error);
    }
    state
}

fn run_game_server(config: ServerConfiguration, stop_token: Arc<AtomicBool>) 
    -> Result<(), Box<dyn Error>> 
{
    use std::time::Duration;
    let socket = UdpSocket::bind(("127.0.0.1", config.port))?;
    socket.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut data : ClientBuffer<ClientCommandType> = ClientBuffer::new();
    let mut state = ServerState::default();
    while !stop_token.load(Ordering::SeqCst) {
        state = match recv_data(&socket, &mut data) {
            Ok(Some((cmd, src))) => {
                clear_old_messages(&mut data, std::time::Duration::from_secs(10));
                respond_to_msg(cmd, &socket, src, state)
            },
            _ => state,
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = argument_parser::parse_args(std::env::args())?;

    println!("Starting server with config:\n{}", config);

    run_game_server(config, Arc::new(AtomicBool::new(false)))
}
