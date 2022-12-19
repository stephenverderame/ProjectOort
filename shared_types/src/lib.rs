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
use itertools::Itertools;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::error::Error;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

const MAX_DATAGRAM_SIZE: usize = 1024;

const CHUNK_HEADER: [u8; 1] = [b'S'];
const CHUNK_HEADER_SIZE: usize = CHUNK_HEADER.len();

const CMD_ID_INDEX: usize = CHUNK_HEADER_SIZE;
const MSG_ID_INDEX: usize = CMD_ID_INDEX + 1;
const PKT_NM_INDEX: usize = MSG_ID_INDEX + 4;

const CHUNK_TITLE_SIZE: usize = CHUNK_HEADER_SIZE + 6;

const CHUNK_FOOTER: [u8; 1] = [b'\n'];
const CHUNK_FOOTER_SIZE: usize = CHUNK_FOOTER.len();

const CHUNK_METADATA_SIZE: usize = CHUNK_TITLE_SIZE + CHUNK_FOOTER_SIZE;

pub type PacketNum = u8;
pub type CommandId = u8;
pub type MsgId = u32;
pub type ChunkedMsg = BTreeMap<PacketNum, Vec<u8>>;

mod serializeable;
pub use serializeable::Serializeable;

pub mod node;
pub mod remote;
pub use remote::*;

pub mod game_controller;
pub mod game_map;
pub mod id_list;

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
    Laser = 0,
    Ship,
    Asteroid,
    Skybox,
    Hook,
    Planet,
    Cloud,
}

impl TryFrom<u8> for ObjectType {
    type Error = String;
    fn try_from(val: u8) -> Result<Self, Self::Error> {
        match val {
            0 => Ok(Self::Laser),
            1 => Ok(Self::Ship),
            2 => Ok(Self::Asteroid),
            3 => Ok(Self::Skybox),
            4 => Ok(Self::Hook),
            5 => Ok(Self::Cloud),
            _ => {
                Err(format!("Invalid object type byte representation: {}", val))
            }
        }
    }
}

impl ObjectType {
    /// Returns the byte representation of the object type
    ///
    /// Requires `val` is a valid representation for an `ObjectType`
    /// Undefined behavior if this condition is not met
    ///
    /// # Safety
    /// `val` must be between 0 and 5 inclusive
    #[must_use]
    pub unsafe fn from_unchecked(val: u8) -> Self {
        std::mem::transmute(val)
    }

    /// Returns `true` if the object type does not have a corresponding rigid body,
    /// that is if it an entity only
    #[must_use]
    pub const fn is_non_physical(&self) -> bool {
        matches!(self, Self::Cloud | Self::Skybox)
    }
}

type ObjectIdType = u32;
#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct ObjectId {
    id: ObjectIdType,
}

impl ObjectId {
    #[inline]
    #[must_use]
    pub const fn new(id: ObjectIdType) -> Self {
        Self { id }
    }

    /// Gets the next object ID after this one
    #[inline]
    #[must_use]
    pub const fn next(&self) -> Self {
        Self {
            id: self.id.wrapping_add(1),
        }
    }

    /// Gets the current object ID and consumes (increments) it
    #[inline]
    #[must_use]
    pub fn consume(&mut self) -> Self {
        let id = self.id;
        self.id = self.id.wrapping_add(1);
        Self { id }
    }

    /// Converts this ID to the ID n ids after this one
    #[inline]
    #[must_use]
    pub const fn incr(self, n: u32) -> Self {
        Self {
            id: self.id.wrapping_add(n),
        }
    }

    /// Converts this ID to its big endian byte representation
    #[inline]
    #[must_use]
    pub const fn to_be_bytes(
        &self,
    ) -> [u8; std::mem::size_of::<ObjectIdType>()] {
        self.id.to_be_bytes()
    }

    /// Creates an `ObjectId` from its big endian byte representation
    #[inline]
    #[must_use]
    pub const fn from_be_bytes(
        bytes: [u8; std::mem::size_of::<ObjectIdType>()],
    ) -> Self {
        Self {
            id: u32::from_be_bytes(bytes),
        }
    }

    #[inline]
    #[must_use]
    pub const fn as_underlying_type(&self) -> ObjectIdType {
        self.id
    }
}

type ObjData = [[f64; 4]; 5];

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct RemoteObject {
    pub mat: ObjData,
    pub id: ObjectId,
    pub typ: ObjectType,
}

/// The packed size for a `RemoteObject`
///
/// This is the amount of bytes sent over the network for a `RemoteObject`
const REMOTE_OBJECT_SIZE: usize = std::mem::size_of::<ObjData>()
    + std::mem::size_of::<ObjectId>()
    + std::mem::size_of::<ObjectType>();

#[derive(Copy, Clone, Debug)]
pub struct RemoteObjectUpdate {
    pub delta_vel: [f64; 3],
    pub delta_rot: [f64; 3],
    pub id: ObjectId,
}

impl RemoteObject {
    #[inline]
    fn base_eq(&self, other: &Self) -> bool {
        self.id == other.id && self.typ == other.typ
    }
}

impl PartialEq for RemoteObject {
    #[cfg(test)]
    fn eq(&self, other: &Self) -> bool {
        const ARRAY_SIZE: usize =
            std::mem::size_of::<ObjData>() / std::mem::size_of::<f64>();
        self.base_eq(other)
            && unsafe {
                std::mem::transmute_copy::<_, [u64; ARRAY_SIZE]>(&self.mat)
            } == unsafe {
                std::mem::transmute_copy::<_, [u64; ARRAY_SIZE]>(&other.mat)
            }
    }

    #[cfg(not(test))]
    fn eq(&self, other: &Self) -> bool {
        self.base_eq(other)
    }
}

impl Eq for RemoteObject {}

/// A command that is sent from the client to the server
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ClientCommandType<'a> {
    Login(String),
    Update(Vec<RemoteObject>),
    UpdateReadOnly(&'a [RemoteObject]),
    GetIds(u32),
}

#[derive(Clone, PartialEq, Debug)]
pub struct LoginInfo {
    pub pid: ObjectId,
    pub lighting: game_map::GlobalLightingInfo,
    pub spawn_pos: [f64; 3],
    pub starting_ids: (ObjectId, ObjectId),
}

const LOGIN_MIN_SIZE: usize = std::mem::size_of::<ObjectId>()
    + std::mem::size_of::<[f32; 3]>()
    + 2 * 2
    + std::mem::size_of::<[f64; 3]>()
    + std::mem::size_of::<(ObjectId, ObjectId)>();

impl Eq for LoginInfo {}

/// Commands sent from the server to the client
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerCommandType {
    ReturnLogin(LoginInfo),
    Update(Vec<RemoteObject>),
    ReturnIds((ObjectId, ObjectId)),
}
