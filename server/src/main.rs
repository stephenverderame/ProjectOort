#![warn(clippy::pedantic, clippy::nursery)]
#![deny(clippy::all)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::wildcard_imports,
    clippy::enum_glob_use,
    clippy::similar_names,
    clippy::module_name_repetitions
)]
#![allow(dead_code)]
mod argument_parser;
use std::net::*;
use std::ops::Deref;

use argument_parser::ServerConfiguration;
use shared_types::*;
use std::collections::HashMap;
use std::error::Error;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

// Maybe the server should just be a headless client

extern crate static_assertions;

#[cfg(test)]
mod test;

/// The current state of the client
///
/// Cannot be created from multiple threads
///
/// For implementing a state machine
enum ClientState {
    WaitingForRequest,
}

/// Requires usage from one thread only
static mut OBJ_ID: ObjectId = ObjectId::new(0);

/// The data for each client
struct ClientData {
    state: ClientState,
    username: String,
    id: ObjectId,
    last_msg_id: u32,
    client_objects: Vec<RemoteObject>,
}

impl ClientData {
    const fn new(id: ObjectId) -> Self {
        Self {
            state: ClientState::WaitingForRequest,
            username: String::new(),
            id,
            last_msg_id: 0,
            client_objects: Vec::new(),
        }
    }
}

/// The data for the server
struct ServerState {
    users: HashMap<SocketAddr, ClientData>,
    server_objects: Vec<RemoteObject>,
    server_lighting: game_map::GlobalLightingInfo,
    last_obj_id: ObjectId,
}

impl ServerState {
    fn get_all_objects(
        &self,
        requesting_client: &SocketAddr,
    ) -> Vec<RemoteObject> {
        self.users
            .iter()
            .filter(|(client_addr, _)| requesting_client != *client_addr)
            .flat_map(|(_, client_data)| client_data.client_objects.iter())
            .copied()
            .collect()
    }

    fn new<Dm: Deref<Target = dyn game_map::Map>>(map: Dm) -> Self {
        Self {
            users: HashMap::default(),
            server_objects: map.initial_objects(),
            server_lighting: map.lighting_info(),
            last_obj_id: ObjectId::default(),
        }
    }
}
/// Gets the response to a login client request
fn login_response(
    username: String,
    user_addr: &SocketAddr,
    state: &mut ServerState,
) -> ServerCommandType {
    let user_state = state.users.get_mut(user_addr).unwrap();
    user_state.username = username;
    let starting_id = state.last_obj_id;
    state.last_obj_id = state.last_obj_id.incr(1024);
    ServerCommandType::ReturnLogin(LoginInfo {
        pid: user_state.id,
        lighting: state.server_lighting.clone(),
        spawn_pos: [0., 0., 0.],
        starting_ids: (starting_id, state.last_obj_id),
    })
}
/// Gets the response to a client object update request
fn update_response(
    objects: Vec<RemoteObject>,
    user_addr: &SocketAddr,
    state: &mut ServerState,
) -> ServerCommandType {
    let user_state = state.users.get_mut(user_addr).unwrap();
    user_state.client_objects = objects;
    ServerCommandType::Update(state.get_all_objects(user_addr))
}

/// Gets the response to a client id allocation request
fn id_fetch_response(
    alloc_size: u32,
    state: &mut ServerState,
) -> ServerCommandType {
    let starting_id = state.last_obj_id;
    state.last_obj_id = state.last_obj_id.incr(alloc_size);
    ServerCommandType::ReturnIds((starting_id, state.last_obj_id))
}

/// Gets the last message id sent to the client, and increments it
/// for the next message
fn get_last_msg_id(state: &mut ServerState, user_addr: SocketAddr) -> u32 {
    let mut user_state = state
        .users
        .entry(user_addr)
        .or_insert_with(|| ClientData::new(state.last_obj_id.consume()));
    let last_msg_id = user_state.last_msg_id;
    user_state.last_msg_id += 1;
    last_msg_id
}

#[inline]
fn respond_to_msg(
    msg: ClientCommandType,
    socket: &UdpSocket,
    addr: SocketAddr,
    mut state: ServerState,
) -> ServerState {
    use ClientCommandType::*;
    let last_msg_id = get_last_msg_id(&mut state, addr);
    let response = match msg {
        Login(username) => login_response(username, &addr, &mut state),
        Update(objects) => update_response(objects, &addr, &mut state),
        UpdateReadOnly(objects) => {
            update_response(objects.to_vec(), &addr, &mut state)
        }
        GetIds(alloc_size) => id_fetch_response(alloc_size, &mut state),
    };
    if let Err(error) = send_data(socket, &addr, &response, last_msg_id) {
        println!("Error sending data: {}", error);
    }
    state
}

fn run_game_server(
    config: &ServerConfiguration,
    stop_token: &Arc<AtomicBool>,
) -> Result<(), Box<dyn Error>> {
    use std::time::Duration;
    let socket = UdpSocket::bind(("127.0.0.1", config.port))?;
    socket.set_read_timeout(Some(Duration::from_secs(3)))?;
    let mut data: ClientBuffer<ClientCommandType> = ClientBuffer::new();
    let mut state = ServerState::new(config.map.get_game_map());
    while !stop_token.load(Ordering::SeqCst) {
        state = match recv_data(&socket, &mut data) {
            Ok(Some((cmd, src))) => {
                clear_old_messages(
                    &mut data,
                    std::time::Duration::from_secs(10),
                );
                respond_to_msg(cmd, &socket, src, state)
            }
            _ => state,
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let config = argument_parser::parse_args(std::env::args())?;

    println!("Starting server with config:\n{}", config);

    run_game_server(&config, &Arc::new(AtomicBool::new(false)))
}
