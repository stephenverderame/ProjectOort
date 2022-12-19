#![allow(dead_code)]
use super::*;
pub use game_map::*;
use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;

pub struct GameStats {}

pub struct PlayerStats {
    pub pid: ObjectId,
    pub spawn_pos: cgmath::Point3<f64>,
}

pub trait GameController {
    fn get_game_objects(&self) -> &[RemoteObject];

    fn get_game_time(&self) -> std::time::Duration;

    fn get_game_stats(&self) -> &GameStats;

    fn get_player_stats(&self) -> &PlayerStats;

    fn set_objects(&mut self, objects: &[RemoteObject]);

    fn update_objects(&mut self, updates: &[RemoteObjectUpdate]);

    fn remove_objects(&mut self, ids: &[ObjectId]);

    /// Requests `n` ids to use for objects
    fn request_n_ids(&mut self, n: u32);

    /// Returns the first and last id in the range, note that the last
    /// id may be less than the first if the ids wrap around
    /// Returns `None` if there are no requested ids received
    fn get_requested_ids(&mut self) -> Option<(ObjectId, ObjectId)>;

    /// Called every loop to update state
    fn sync(&mut self);

    fn get_lighting_info(&self) -> &GlobalLightingInfo;
}

pub struct LocalGameController {
    last_id: ObjectId,
    objects: Vec<RemoteObject>,
    indices: HashMap<ObjectId, usize>,
    start_time: std::time::Instant,
    requested_ids: std::collections::VecDeque<(ObjectId, ObjectId)>,
    lighting: GlobalLightingInfo,
    player: PlayerStats,
}

impl LocalGameController {
    pub fn new<M: Map, Dm: std::ops::Deref<Target = M>>(map: Dm) -> Self {
        let objs = map.initial_objects();
        let indices = (0..objs.len()).map(|i| (objs[i].id, i)).collect();
        let player_id = objs.last().map(|o| o.id).unwrap_or_default();
        Self {
            last_id: player_id.next(),
            objects: objs,
            start_time: std::time::Instant::now(),
            indices,
            requested_ids: VecDeque::default(),
            lighting: map.lighting_info(),
            player: PlayerStats {
                pid: player_id,
                spawn_pos: cgmath::point3(0., 0., 0.),
            },
        }
    }
}

type RemoteObjectMapPair = (Vec<RemoteObject>, HashMap<ObjectId, usize>);

/// Sets the given objects to the given new objects
fn set_objects(
    objects: &mut Vec<RemoteObject>,
    indices: &mut HashMap<ObjectId, usize>,
    new_objects: &[RemoteObject],
) {
    for obj in new_objects {
        if let Some(i) = indices.get(&obj.id) {
            objects[*i] = *obj;
        } else {
            indices.insert(obj.id, objects.len());
            objects.push(*obj);
        }
    }
}

/// Updates the given objects with the given updates
fn update_objects(
    objects: &mut [RemoteObject],
    indices: &mut HashMap<ObjectId, usize>,
    updates: &[RemoteObjectUpdate],
) {
    for update in updates {
        if let Some(idx) = indices.get(&update.id) {
            let (node, mut vel, mut rot, typ, id) =
                node::from_remote_object(&objects[*idx]);
            vel += From::from(update.delta_vel);
            rot += From::from(update.delta_rot);

            // TODO: update position here?

            objects[*idx] = node::to_remote_object(&node, &vel, &rot, typ, id);
        }
    }
}

/// Removes the objects with the given ids from the given objects
fn remove_objects(
    objects: &mut Vec<RemoteObject>,
    indices: &mut HashMap<ObjectId, usize>,
    ids: &[ObjectId],
) {
    for id in ids {
        if let Some(index) = indices.remove(id) {
            if objects.len() > 1 {
                let last_index = objects.len() - 1;
                let last_id = objects[last_index].id;
                indices.insert(last_id, index);
            }
            objects.swap_remove(index);
        }
    }
}

impl GameController for LocalGameController {
    fn get_game_objects(&self) -> &[RemoteObject] {
        &self.objects
    }

    fn get_game_time(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }

    fn get_game_stats(&self) -> &GameStats {
        &GameStats {}
    }

    fn get_player_stats(&self) -> &PlayerStats {
        &self.player
    }

    fn set_objects(&mut self, objects: &[RemoteObject]) {
        set_objects(&mut self.objects, &mut self.indices, objects);
    }

    fn update_objects(&mut self, updates: &[RemoteObjectUpdate]) {
        update_objects(&mut self.objects, &mut self.indices, updates);
    }

    fn remove_objects(&mut self, ids: &[ObjectId]) {
        remove_objects(&mut self.objects, &mut self.indices, ids);
    }

    fn request_n_ids(&mut self, n: u32) {
        let id = self.last_id;
        self.last_id = self.last_id.incr(n);
        self.requested_ids.push_back((id, self.last_id));
    }

    fn get_requested_ids(&mut self) -> Option<(ObjectId, ObjectId)> {
        self.requested_ids.pop_front()
    }

    fn sync(&mut self) {}

    fn get_lighting_info(&self) -> &GlobalLightingInfo {
        &self.lighting
    }
}

#[allow(unused)]
pub struct RemoteGameController {
    client_objects: Vec<RemoteObject>,
    server_objects: Vec<RemoteObject>,
    game_objects: Vec<RemoteObject>,
    indices: HashMap<ObjectId, usize>,
    available_ids: id_list::IdList,
    lighting: GlobalLightingInfo,
    player: PlayerStats,
    sock: UdpSocket,
    peer: (IpAddr, u16),
    msg_buffer: ClientBuffer<ServerCommandType>,
    last_out_id: MsgId,
}

impl RemoteGameController {
    fn login(
        username: &str,
        sock: &UdpSocket,
        last_out_id: &mut MsgId,
        received_msgs: &mut ClientBuffer<ServerCommandType>,
    ) -> Result<LoginInfo, Box<dyn Error>> {
        let mut trials = 0;
        while trials < 3 {
            match remote::send_important(
                sock,
                &ClientCommandType::Login(username.to_owned()),
                *last_out_id,
                received_msgs,
                &ImportantArguments::default(),
            ) {
                Ok(ServerCommandType::ReturnLogin(login)) => {
                    *last_out_id = last_out_id.wrapping_add(1);
                    return Ok(login);
                }
                Err(_) => trials += 1,
                Ok(_) => panic!("Unexpected response"),
            }
        }
        Err("Could not receive data")?
    }

    fn get_initial_objects(
        sock: &UdpSocket,
        player: RemoteObject,
        last_out_id: &mut MsgId,
        received_msgs: &mut ClientBuffer<ServerCommandType>,
    ) -> Result<RemoteObjectMapPair, Box<dyn Error>> {
        let mut trials = 0;
        let mut objects = vec![player];
        let mut indices = HashMap::new();
        indices.insert(player.id, 0);
        while trials < 3 {
            match remote::send_important(
                sock,
                &ClientCommandType::Update(vec![player]),
                *last_out_id,
                received_msgs,
                &ImportantArguments::default(),
            ) {
                Ok(ServerCommandType::Update(objs)) => {
                    for (obj, idx) in objs.into_iter().zip(1..) {
                        indices.insert(obj.id, idx);
                        objects.push(obj);
                    }
                    *last_out_id = last_out_id.wrapping_add(1);
                    return Ok((objects, indices));
                }
                Err(_) => trials += 1,
                Ok(_) => panic!("Unexpected response"),
            }
        }
        Err("Could not receive data")?
    }

    /// Creates a new `RemoteGameController` and connects to the server
    /// # Errors
    /// If the socket cannot be created or bound or connecting fails
    pub fn new(
        username: &str,
        server: (IpAddr, u16),
    ) -> Result<Self, Box<dyn Error>> {
        let sock = UdpSocket::bind(&server)?;
        sock.connect(&server)?;
        let mut last_out_id = 0 as MsgId;
        let mut recieved_msgs = ClientBuffer::<ServerCommandType>::new();
        let mut available_ids = id_list::IdList::new();
        let login_info =
            Self::login(username, &sock, &mut last_out_id, &mut recieved_msgs)?;
        available_ids.add_ids(login_info.starting_ids);
        let player = node::to_remote_object(
            &node::Node::default().pos(From::from(login_info.spawn_pos)),
            &cgmath::vec3(0., 0., 0.),
            &cgmath::vec3(0., 0., 0.),
            ObjectType::Ship,
            login_info.pid,
        );
        let (server_objects, indices) = Self::get_initial_objects(
            &sock,
            player,
            &mut last_out_id,
            &mut recieved_msgs,
        )?;
        Ok(Self {
            server_objects,
            client_objects: vec![],
            game_objects: vec![],
            indices,
            available_ids,
            lighting: login_info.lighting,
            player: PlayerStats {
                pid: login_info.pid,
                spawn_pos: From::from(login_info.spawn_pos),
            },
            sock,
            peer: server,
            msg_buffer: recieved_msgs,
            last_out_id,
        })
    }

    /// Requests `n` ids from the server
    fn get_ids_from_server(&mut self, n: u32) {
        match remote::send_important(
            &self.sock,
            &ClientCommandType::GetIds(n),
            self.last_out_id,
            &mut self.msg_buffer,
            &ImportantArguments::default(),
        )
        .expect("Could not send ID request")
        {
            ServerCommandType::ReturnIds(ids) => {
                self.last_out_id = self.last_out_id.wrapping_add(1);
                self.available_ids.add_ids(ids);
            }
            _ => panic!("Unexpected response"),
        }
    }

    fn send_update(&mut self) -> Result<(), Box<dyn Error>> {
        if self.client_objects.is_empty() {
            return Ok(());
        }
        let out = remote::send_data(
            &self.sock,
            self.peer,
            &ClientCommandType::UpdateReadOnly(&self.client_objects),
            self.last_out_id,
        );
        self.last_out_id = self.last_out_id.wrapping_add(1);
        out
    }

    fn get_response(&mut self) {
        if let Ok(Some(response)) =
            remote::recv_data(&self.sock, &mut self.msg_buffer)
        {
            match response {
                (ServerCommandType::Update(objs), _) => {
                    self.server_objects = objs;
                }
                _ => panic!("Unexpected response"),
            }
        }
    }
}

impl GameController for RemoteGameController {
    fn get_game_objects(&self) -> &[RemoteObject] {
        &self.game_objects
    }

    fn get_game_time(&self) -> std::time::Duration {
        todo!()
    }

    fn get_game_stats(&self) -> &GameStats {
        &GameStats {}
    }

    fn get_player_stats(&self) -> &PlayerStats {
        &self.player
    }

    fn set_objects(&mut self, objects: &[RemoteObject]) {
        self.client_objects = objects.to_vec();
    }

    fn update_objects(&mut self, _updates: &[RemoteObjectUpdate]) {
        todo!()
    }

    fn remove_objects(&mut self, _ids: &[ObjectId]) {
        todo!()
    }

    fn request_n_ids(&mut self, n: u32) {
        self.get_ids_from_server(n);
    }

    fn get_requested_ids(&mut self) -> Option<(ObjectId, ObjectId)> {
        self.available_ids.pop_front()
    }

    fn sync(&mut self) {
        self.get_response();
    }

    fn get_lighting_info(&self) -> &GlobalLightingInfo {
        &self.lighting
    }
}
