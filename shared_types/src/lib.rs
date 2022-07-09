

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

mod serializeable;
pub use serializeable::Serializeable;

pub mod remote;
pub use remote::*;

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


/// Commands sent from the server to the client
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum ServerCommandType {
    ReturnId(u32),
}