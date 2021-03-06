

use std::{error::Error};
use itertools::{Itertools};
use std::collections::BTreeMap;
use std::net::{UdpSocket, SocketAddr, ToSocketAddrs};
use std::cell::RefCell;

const MAX_DATAGRAM_SIZE : usize = 1024;

const CHUNK_HEADER : [u8; 1] = [b'S'];
const CHUNK_HEADER_SIZE : usize = CHUNK_HEADER.len();

const CMD_ID_INDEX : usize = CHUNK_HEADER_SIZE;
const MSG_ID_INDEX : usize = CMD_ID_INDEX + 1;
const PKT_NM_INDEX : usize = MSG_ID_INDEX + 4;

const CHUNK_TITLE_SIZE : usize = CHUNK_HEADER_SIZE + 6;

const CHUNK_FOOTER : [u8; 1] = [b'\n'];
const CHUNK_FOOTER_SIZE : usize = CHUNK_FOOTER.len();


const CHUNK_METADATA_SIZE : usize = CHUNK_TITLE_SIZE + CHUNK_FOOTER_SIZE;

pub type PacketNum = u8;
pub type CommandId = u8;
pub type MsgId = u32;
pub type ChunkedMsg = BTreeMap<PacketNum, Vec<u8>>;

mod serializeable;
pub use serializeable::Serializeable;

pub mod remote;
pub use remote::*;

pub mod game_controller;

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

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
#[repr(u8)]
pub enum ObjectType {
    Laser = 0, Ship, Asteroid, Any, Hook
}

impl TryFrom<u8> for ObjectType {
    type Error = String;
    fn try_from(val: u8) -> Result<Self, Self::Error> {
        match val {
            0 => Ok(ObjectType::Laser),
            1 => Ok(ObjectType::Ship),
            2 => Ok(ObjectType::Asteroid),
            3 => Ok(ObjectType::Any),
            4 => Ok(ObjectType::Hook),
            _ => Err(format!("Invalid object type byte representation: {}", val))
        }
    }
}

impl ObjectType {
    /// Returns the byte representation of the object type
    /// 
    /// Requires `val` is a valid representation for an `ObjectType`
    /// Undefined behavior if this condition is not met
    pub unsafe fn from_unchecked(val: u8) -> Self {
        std::mem::transmute(val)
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct RemoteObject {
    pub mat: [[f64; 4]; 4],
    pub id: u32,
    pub typ: ObjectType,
}

/// The packed size for a RemoteObject
/// 
/// This is the amount of bytes sent over the network for a RemoteObject
const REMOTE_OBJECT_SIZE : usize = 133;

impl RemoteObject {
    #[inline(always)]
    fn base_eq(&self, other: &Self) -> bool {
        self.id == other.id && self.typ == other.typ
    }
}

impl PartialEq for RemoteObject {
    #[cfg(test)]
    fn eq(&self, other: &Self) -> bool {
        self.base_eq(other) &&
        unsafe { std::mem::transmute_copy::<_, [u64; 16]>(&self.mat) } == 
        unsafe { std::mem::transmute_copy::<_, [u64; 16]>(&other.mat) }
    }

    #[cfg(not(test))]
    fn eq(&self, other: &Self) -> bool {
        self.base_eq(other)
    }
}

impl Eq for RemoteObject {}


/// A command that is sent from the client to the server
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ClientCommandType {
    Login(String),
    Update(Vec<RemoteObject>),
}


/// Commands sent from the server to the client
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerCommandType {
    ReturnId(u32),
    Update(Vec<RemoteObject>),
}