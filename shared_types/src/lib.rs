

use std::{error::Error};
use itertools::{Itertools};
use std::collections::BTreeMap;
use std::net::{UdpSocket, SocketAddr, ToSocketAddrs};
use std::cell::RefCell;

pub const MAX_DATAGRAM_SIZE : usize = 1024;

pub const CHUNK_HEADER : [u8; 1] = [b'S'];
pub const CHUNK_HEADER_SIZE : usize = CHUNK_HEADER.len();

pub const CMD_ID_INDEX : usize = CHUNK_HEADER_SIZE;
pub const MSG_ID_INDEX : usize = CMD_ID_INDEX + 1;
pub const PKT_NM_INDEX : usize = MSG_ID_INDEX + 4;

pub const CHUNK_TITLE_SIZE : usize = CHUNK_HEADER_SIZE + 6;

pub const CHUNK_FOOTER : [u8; 1] = [b'\n'];
pub const CHUNK_FOOTER_SIZE : usize = CHUNK_FOOTER.len();

pub const CHUNK_METADATA_SIZE : usize = CHUNK_TITLE_SIZE + CHUNK_FOOTER_SIZE;

pub type PacketNum = u8;
pub type CommandId = u8;
pub type MsgId = u32;
pub type ChunkedMsg = BTreeMap<PacketNum, Vec<u8>>;

#[cfg(test)]
mod test;

/*
 MESSAGE FORMAT:

 S<cmd_id (u8)><msg_id (u32)><packet_number (u8)>
 <data>
 \n



 The header is S
 The title is S<cmd_id (u8)><msg_id (u32)><packet_number (u8)>
 The footer is \n

 The cmd_id is the type of command being sent. Unique among every command.
 The msg_id is a unique id for the message as determined by the sender, 
    in big endian byte order.
 The packet_number is the order of the chunked packet in the message 
*/

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ObjectType {
    Laser = 0, Ship, Asteroid, Any, Hook
}

/// A command that is sent from the client to the server
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ClientCommandType {
    Login(String)
}

/// Converts a command into chunks of `MAX_DATAGRAM_SIZE` bytes.
/// 
/// `cmd_id` - the command type id
/// 
/// Panics if the data cannot fit into `256` of `MAX_DATAGRAM_SIZE` sized chunks
fn chunk_serialized_data<T>(cmd_id: CommandId, data: T, msg_id: MsgId) 
    -> ChunkedMsg where T : Iterator<Item = u8>
{
    let data = data.chunks(MAX_DATAGRAM_SIZE - CHUNK_METADATA_SIZE);

    let mut packet_num : PacketNum = 0;

    data.into_iter()
    .map(|chunk| 
    {
        let v : Vec<_> = CHUNK_HEADER.into_iter().chain([cmd_id].into_iter())
            .chain(msg_id.to_be_bytes()).chain([packet_num].into_iter())
            .chain(chunk).chain(CHUNK_FOOTER.into_iter()).collect();
        let res = (packet_num, v);
        packet_num += 1;
        assert_ne!(packet_num, 0); // More than 255 packets
        res
    }).collect()
}

/// Checks the contained value of `opt` equals `val`
/// 
/// If `opt` is `None`, sets `val` as the contained value and does not fail
/// 
/// Fails only if `opt` is `Some` and the contained value is not `val`
fn check_option_equals<T>(opt: &mut Option<T>, val: T) 
    -> Result<(), Box<dyn Error>> 
    where T : PartialEq + std::fmt::Debug
{
    match opt {
        Some(opt_val) if *opt_val != val =>
            Err(format!("Option mismatch: {:?} != {:?}", opt_val, val).into())
        ,
        None => Ok(*opt = Some(val)),
        _ => Ok(()),
    }
}

/// Converts a chunked command into a single byte buffer
/// Requires each input chunk to be in order, for the same command, and contain
/// the title and footer
/// 
/// Returns the combined payload data and the command type id. The payload does not
/// contain the header, or footer (this includes the command id and packet number)
/// 
/// Fails if the chunks are not in order, not all for the same command, or do not
/// contain the title and footer
fn dechunk_serialized_data(chunks: ChunkedMsg) 
    -> Result<(Vec<u8>, CommandId, MsgId), Box<dyn Error>> 
{
    let mut res = Vec::new();
    let mut last_cmd_id : Option<CommandId> = None;
    let mut last_msg_id : Option<MsgId> = None;
    let mut expected_packet_num : PacketNum = 0;
    for (_, chunk) in chunks {
        let chunk_len = chunk.len();
        if chunk_len < CHUNK_METADATA_SIZE {
            return Err(format!("Chunk too short: {}", chunk.len()).into());
        }
        if &chunk[0..CHUNK_HEADER_SIZE] != CHUNK_HEADER {
            return Err(format!("Invalid chunk title: {:?}", &chunk[0..CHUNK_TITLE_SIZE]).into());
        }
        if &chunk[chunk_len - CHUNK_FOOTER_SIZE..] != CHUNK_FOOTER {
            return Err(format!("Invalid chunk footer: {:?}", &chunk[chunk_len - CHUNK_FOOTER_SIZE..]).into());
        }
        let cmd_id = chunk[CMD_ID_INDEX];
        let msg_id = u32::from_be_bytes(chunk[MSG_ID_INDEX..MSG_ID_INDEX + 4].try_into()?);
        check_option_equals(&mut last_cmd_id, cmd_id)?;
        check_option_equals(&mut last_msg_id, msg_id)?;
        if expected_packet_num != chunk[PKT_NM_INDEX] {
            return Err("Chunk packet numbers are not in order")?;
        }
        expected_packet_num += 1;
        res.extend_from_slice(&chunk[CHUNK_TITLE_SIZE .. chunk_len - CHUNK_FOOTER_SIZE]);
    }
    Ok((res, last_cmd_id.unwrap(), last_msg_id.unwrap()))
}

/// A type that can be converted to a network message
pub trait Serializeable {
    /// Converts the object into a chunked message without the `END` token
    /// 
    /// `msg_id` - the message id of the message. Must be unique for each message, for
    ///            each sender.
    /// 
    /// Fails if the object is not well-formed or violates invariants for the particular
    ///     implementor of the trait.
    fn serialize(&self, msg_id: MsgId) -> Result<ChunkedMsg, Box<dyn Error>>;

    /// Converts a chunked message without the `END` token into the object
    /// 
    /// Returns the object and its message id if its well-formed
    /// 
    /// Fails if `chunks` is missing packets, contains malformed packets, 
    ///    contains packets of different commands or message ids, or its data
    ///    cannot be deserialized as determined by the implementor of the trait.
    fn deserialize(chunks: ChunkedMsg) -> Result<(Self, MsgId), Box<dyn Error>>
        where Self: Sized;
}

impl Serializeable for ClientCommandType {
    fn serialize(&self, msg_id: MsgId) -> Result<ChunkedMsg, Box<dyn Error>> {
        let (cmd_id, data) = match self {
            ClientCommandType::Login(name) => {
                if name.len() > 255 {
                    return Err("Login name too long")?;
                }
                let mut data = Vec::new();
                data.push(name.len() as u8);
                data.extend(name.bytes());
                (b'L', data)
            },
        };
        
        Ok(chunk_serialized_data(cmd_id, data.into_iter(), msg_id))
    }

    fn deserialize(chunks : ChunkedMsg) -> Result<(Self, MsgId), Box<dyn Error>> {
        let (data, cmd_id, msg_id) = dechunk_serialized_data(chunks)?;
        match cmd_id {
            b'L' => {
                if data.is_empty() {
                    return Err("Login command too short")?;
                }
                let name_len = data[0] as usize;
                if data.len() < 1 + name_len {
                    return Err("Login command too short")?;
                }
                let name = std::str::from_utf8(&data[1..])?;
                Ok((ClientCommandType::Login(name.to_string()), msg_id))
            },
            _ => Err("Unknown command")?,
        }
    }
}


/// Commands sent from the server to the client
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ServerCommandType {
    ReturnId(u32),
}

impl Serializeable for ServerCommandType {
    fn serialize(&self, msg_id: MsgId) -> Result<ChunkedMsg, Box<dyn Error>> {
        let (data, cmd_id) = match self {
            ServerCommandType::ReturnId(id) => {
                (id.to_be_bytes().to_vec(), b'I')
            },
        };
        Ok(chunk_serialized_data(cmd_id, data.into_iter(), msg_id))
    }

    fn deserialize(chunks: ChunkedMsg) -> Result<(Self, MsgId), Box<dyn Error>> {
        let (data, cmd_id, msg_id) = dechunk_serialized_data(chunks)?;
        match cmd_id {
            b'I' => {
                if data.len() != 4 {
                    return Err("Invalid id length")?;
                }
                let id = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
                Ok((ServerCommandType::ReturnId(id), msg_id))
            },
            _ => Err("Unknown command")?,
        }
    }
}

/// Adds the string `"END"` to the end of the data
/// Adds the end delimiter to the last packet if it fits, otherwise creates a new packet
/// 
/// Requires `chunks` to be well-formed
fn add_end_chunk(mut chunks: ChunkedMsg) -> ChunkedMsg {
    let (last_pack_num, last_packet) = chunks.iter().rev().next().unwrap();
    if last_packet.len() <= MAX_DATAGRAM_SIZE - 3 {
        let (_, last_packet) = chunks.iter_mut().rev().next().unwrap();
        last_packet.extend(b"END");
    } else {
        let mut new_last_pack = last_packet[0..CHUNK_TITLE_SIZE].to_vec();
        new_last_pack[PKT_NM_INDEX] = last_pack_num + 1;
        new_last_pack.extend(b"END");
        new_last_pack.extend(CHUNK_FOOTER);
        chunks.insert(last_pack_num + 1, new_last_pack);
    }
    chunks
}

/// Removes the string `"END"` from the end of the data
/// 
/// Fails if END is not at the beginning nor end of the last chunk
fn remove_end_chunk(mut chunks: ChunkedMsg) -> Result<ChunkedMsg, Box<dyn Error>> {
    let (last_pack_num, last_packet) = chunks.iter().rev().next().unwrap();
    if let Some(pos) = last_packet.windows(3).rposition(|x| x == b"END") {
        if pos == CHUNK_TITLE_SIZE {
            let last_pack_num = *last_pack_num;
            chunks.remove(&last_pack_num);
            Ok(chunks)
        } else if pos == last_packet.len() - 3 {
            let (_, last_packet) = chunks.iter_mut().rev().next().unwrap();
            last_packet.truncate(last_packet.len() - 3);
            Ok(chunks)
        } else {
            return Err("Invalid END position in chunk")?;
        }
    } else {
        return Err("No end chunk")?;
    }
    
}

/// Gets the command id and the packet num from the chunk
/// 
/// Requires `chunk` is a well-formed data chunk
#[inline]
pub fn get_cmd_ids_and_nums(chunk: &[u8]) -> (CommandId, MsgId, PacketNum) {
    (chunk[CMD_ID_INDEX], 
        u32::from_be_bytes(chunk[MSG_ID_INDEX .. MSG_ID_INDEX + 4].try_into().unwrap()), 
        chunk[PKT_NM_INDEX])
}

/// Sends a command to the specified socket
/// 
/// Adds an `END` token to the end of the data
pub fn send_data<T : Serializeable, S : ToSocketAddrs>(sock: &UdpSocket, addr: S, data: &T, msg_id: MsgId) 
    -> Result<(), Box<dyn Error>> 
{
    let chunks = add_end_chunk(data.serialize(msg_id)?);
    for (_, chunk) in chunks {
        sock.send_to(&chunk, &addr)?;
    }
    Ok(())
}

/// A serizeable message that's fully or partially received from the socket
pub enum RemoteData<T : Serializeable> {
    Buffering(ChunkedMsg),
    Ready(T),
}

impl<T : Serializeable> RemoteData<T> {
    /// Converts the message to a ready message
    /// 
    /// Fails if a buffered message cannot be deserialized
    pub fn to_ready(self) -> Result<Self, Box<dyn Error>> {
        use RemoteData::*;
        match self {
            Buffering(chunks) => {
                let (cmd, _) = T::deserialize(remove_end_chunk(chunks)?)?;
                Ok(Ready(cmd))
            },
            Ready(_) => Ok(self),
        }
    }

    /// Determines if a buffered message contains the END token and is not missing packets
    /// or the data is already ready
    pub fn is_ready(&self) -> bool {
        use RemoteData::*;
        match self {
            Ready(_) => true,
            Buffering(msg) => {
                msg.iter().rev().next().map(|(last_pack_num, last_chunk)| {
                    last_chunk.windows(3).rev().position(|x| x == b"END")
                        .is_some() && *last_pack_num as usize == msg.len() - 1
                }).unwrap_or(false)
            },
        }
    }

    /// Adds a new packet to a buffering message. If this new packet makes the buffering
    /// message ready, converts the message to a ready message
    /// 
    /// Fails if the message is already ready or if the packet is too small
    /// or if the packet number is a duplicate
    pub fn add_packet(self, packet: Vec<u8>) -> Result<Self, Box<dyn Error>> {
        use RemoteData::*;
        match self {
            Buffering(mut msg) if packet.len() >= CHUNK_METADATA_SIZE => {
                let pk_id = packet[PKT_NM_INDEX];
                if msg.contains_key(&pk_id) {
                    Err("Duplicate packet")?
                } else {
                    msg.insert(pk_id, packet);
                    let this = Buffering(msg);
                    if this.is_ready() {
                        Ok(this.to_ready()?)
                    } else {
                        Ok(this)
                    }
                }
            },
            Ready(_) => Err("Cannot add packet to ready message")?,
            Buffering(_) => Err("Packet too small")?,
        }
    }
}

/// Encapsulates a `RemoteData<T>` and its last access time
pub struct TimestampedRemoteData<T : Serializeable> {
    pub data: RemoteData<T>,
    last_access: std::time::Instant,
}

impl<T : Serializeable> Default for TimestampedRemoteData<T> {
    fn default() -> Self {
        TimestampedRemoteData {
            data: RemoteData::Buffering(Default::default()),
            last_access: std::time::Instant::now(),
        }
    }
}

impl<T : Serializeable> std::ops::Deref for TimestampedRemoteData<T> {
    type Target = RemoteData<T>;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T : Serializeable> std::ops::DerefMut for TimestampedRemoteData<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<T : Serializeable> TimestampedRemoteData<T> {
    pub fn from(data: RemoteData<T>) -> Self {
        TimestampedRemoteData {
            data,
            last_access: std::time::Instant::now(),
        }
    }
}

impl<T : Serializeable> From<RemoteData<T>> for TimestampedRemoteData<T> {
    fn from(data: RemoteData<T>) -> Self {
        TimestampedRemoteData {
            data,
            last_access: std::time::Instant::now(),
        }
    }
}

pub type ClientData<T> = TimestampedRemoteData<T>;
pub type ClientBuffer<T> = 
    std::collections::HashMap<SocketAddr, BTreeMap<(CommandId, MsgId), ClientData<T>>>;

/// Receives a single packet from `socket`. If the packet is well-formed,
/// adds the packet to a buffering command. If the packet completes a buffering
/// command, removes the buffering command and returns the deserialized command
/// along with the sender address
/// 
/// The packet is dropped if it is malformed or the complete message cannot be deserialized
pub fn recv_data<T : Serializeable>(socket: &UdpSocket, data: &mut ClientBuffer<T>) 
    -> Option<(T, SocketAddr)> 
{
    use std::rc::Rc;
    std::thread_local!(
        static BUF : Rc<RefCell<[u8; MAX_DATAGRAM_SIZE]>> = Rc::new(RefCell::new([0; MAX_DATAGRAM_SIZE]))
    );
    if let Ok((amt, src)) = BUF.with(|buf| socket.recv_from(&mut *buf.borrow_mut())) {
        if amt > CHUNK_METADATA_SIZE {
            let msg = BUF.with(|buf| buf.clone());
            let msg = msg.borrow();
            let (cmd_id, msg_id, _pn) = get_cmd_ids_and_nums(&*msg);
            let client_data = data.entry(src).or_insert(BTreeMap::new());
            let id = (cmd_id, msg_id);
            
            if let Ok(new_data) = client_data.remove(&id).unwrap_or_default()
                .data.add_packet(msg[..amt].to_vec()) 
            {
                match new_data {
                    new_data @ RemoteData::Buffering(_) => {
                        client_data.insert(id, new_data.into());
                    },
                    RemoteData::Ready(data) => 
                        return Some((data, src)),
                }
            }
        } 
    }
    None
    
}

/// Removes any buffering message whose last access time is older than `timeout`
pub fn clear_old_messages<T : Serializeable>(data: &mut ClientBuffer<T>, timeout: std::time::Duration) {
    let now = std::time::Instant::now();
    for (_, client_data) in data.iter_mut() {
        let mut dead_ids = Vec::new();
        for (id, client_data) in client_data.iter() {
            if now.duration_since(client_data.last_access) > timeout {
                dead_ids.push(*id);
            }
        }
        for dead_id in dead_ids {
            client_data.remove(&dead_id);
        }
    }
}