

use std::{error::Error};
use itertools::{Itertools};
use std::collections::BTreeMap;
use std::net::{UdpSocket, SocketAddr};

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

pub trait Serializeable {
    fn serialize(&self, msg_id: MsgId) -> Result<ChunkedMsg, Box<dyn Error>>;
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
pub fn get_cmd_id_and_packet_num(chunk: &[u8]) -> (u8, u8) {
    (chunk[CHUNK_HEADER_SIZE], chunk[CHUNK_HEADER_SIZE + 1])
}

pub fn send_data<T : Serializeable>(sock: &UdpSocket, addr: &SocketAddr, data: &T, msg_id: MsgId) 
    -> Result<(), Box<dyn Error>> 
{
    let chunks = add_end_chunk(data.serialize(msg_id)?);
    for (_, chunk) in chunks {
        sock.send_to(&chunk, addr)?;
    }
    Ok(())
}

/// A serizeable message fully or partially received from the socket
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
                let (cmd, _) = T::deserialize(chunks)?;
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
}