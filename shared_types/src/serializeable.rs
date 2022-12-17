use super::*;

/// A type that can be converted to a network message
pub trait Serializeable {
    /// Converts the object into a chunked message without the `END` token
    ///
    /// `msg_id` - the message id of the message. Must be unique for each message, for
    ///            each sender.
    ///
    /// # Errors
    /// Fails if the object is not well-formed or violates invariants for the particular
    ///     implementor of the trait.
    fn serialize(&self, msg_id: MsgId) -> Result<ChunkedMsg, Box<dyn Error>>;

    /// Converts a chunked message without the `END` token into the object
    ///
    /// Returns the object and its message id if its well-formed
    ///
    /// # Errors
    /// Fails if `chunks` is missing packets, contains malformed packets,
    ///    contains packets of different commands or message ids, or its data
    ///    cannot be deserialized as determined by the implementor of the trait.
    fn deserialize(chunks: ChunkedMsg) -> Result<(Self, MsgId), Box<dyn Error>>
    where
        Self: Sized;
}

/// Converts a command into chunks of `MAX_DATAGRAM_SIZE` bytes.
///
/// `cmd_id` - the command type id
///
/// Panics if the data cannot fit into `255` of `MAX_DATAGRAM_SIZE` sized chunks (~260 KB)
/// If `data` serializes to an empty message, returns
/// one packet containing a header and footer only
fn chunk_serialized_data<T>(
    cmd_id: CommandId,
    data: T,
    msg_id: MsgId,
) -> ChunkedMsg
where
    T: Iterator<Item = u8>,
{
    let data = data.chunks(MAX_DATAGRAM_SIZE - CHUNK_METADATA_SIZE);

    let mut packet_num: PacketNum = 0;

    let res: ChunkedMsg = data
        .into_iter()
        .map(|chunk| {
            let v: Vec<_> = CHUNK_HEADER
                .into_iter()
                .chain([cmd_id].into_iter())
                .chain(msg_id.to_be_bytes())
                .chain([packet_num].into_iter())
                .chain(chunk)
                .chain(CHUNK_FOOTER.into_iter())
                .collect();
            let res = (packet_num, v);
            packet_num += 1; // Will panic on overflows (More than 255 packets)
            res
        })
        .collect();
    if res.is_empty() {
        let mut out = BTreeMap::new();
        let data: Vec<_> = CHUNK_HEADER
            .into_iter()
            .chain([cmd_id].into_iter())
            .chain(msg_id.to_be_bytes())
            .chain([packet_num].into_iter())
            .chain(CHUNK_FOOTER.into_iter())
            .collect();
        out.insert(packet_num, data);
        out
    } else {
        res
    }
}

/// Checks the contained value of `opt` equals `val`
///
/// If `opt` is `None`, sets `val` as the contained value and does not fail
///
/// Fails only if `opt` is `Some` and the contained value is not `val`
fn check_option_equals<T>(
    opt: &mut Option<T>,
    val: T,
) -> Result<(), Box<dyn Error>>
where
    T: PartialEq + std::fmt::Debug,
{
    match opt {
        Some(opt_val) if *opt_val != val => {
            Err(format!("Option mismatch: {:?} != {:?}", opt_val, val).into())
        }
        None => {
            *opt = Some(val);
            Ok(())
        }
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
fn dechunk_serialized_data(
    chunks: ChunkedMsg,
) -> Result<(Vec<u8>, CommandId, MsgId), Box<dyn Error>> {
    let mut res = Vec::new();
    let mut last_cmd_id: Option<CommandId> = None;
    let mut last_msg_id: Option<MsgId> = None;
    for (expected_packet_num, (_, chunk)) in (0_u8..).zip(chunks.into_iter()) {
        let chunk_len = chunk.len();
        if chunk_len < CHUNK_METADATA_SIZE {
            return Err(format!("Chunk too short: {}", chunk.len()).into());
        }
        if chunk[0..CHUNK_HEADER_SIZE] != CHUNK_HEADER {
            return Err(format!(
                "Invalid chunk title: {:?}",
                &chunk[0..CHUNK_TITLE_SIZE]
            )
            .into());
        }
        if chunk[chunk_len - CHUNK_FOOTER_SIZE..] != CHUNK_FOOTER {
            return Err(format!(
                "Invalid chunk footer: {:?}",
                &chunk[chunk_len - CHUNK_FOOTER_SIZE..]
            )
            .into());
        }
        let cmd_id = chunk[CMD_ID_INDEX];
        let msg_id = MsgId::from_be_bytes(
            chunk[MSG_ID_INDEX..MSG_ID_INDEX + 4].try_into()?,
        );
        check_option_equals(&mut last_cmd_id, cmd_id)?;
        check_option_equals(&mut last_msg_id, msg_id)?;
        if expected_packet_num != chunk[PKT_NM_INDEX] {
            return Err("Chunk packet numbers are not in order")?;
        }
        res.extend_from_slice(
            &chunk[CHUNK_TITLE_SIZE..chunk_len - CHUNK_FOOTER_SIZE],
        );
    }
    Ok((res, last_cmd_id.unwrap(), last_msg_id.unwrap()))
}

fn serialize_objects(objects: &[RemoteObject]) -> (Vec<u8>, u8) {
    (
        objects
            .iter()
            .flat_map(|obj| {
                obj.mat
                    .iter()
                    .flatten()
                    .flat_map(|flt| flt.to_be_bytes())
                    .chain(obj.id.to_be_bytes())
                    .chain([obj.typ as u8])
            })
            .collect(),
        b'U',
    )
}

fn deserialize_update(
    data: Vec<u8>,
) -> Result<Vec<RemoteObject>, Box<dyn Error>> {
    const MAT_SIZE: usize = std::mem::size_of::<ObjData>();
    #[allow(clippy::if_not_else)]
    if data.len() % REMOTE_OBJECT_SIZE != 0 {
        Err("Invalid update length")?
    } else {
        let objs = data
            .into_iter()
            .chunks(REMOTE_OBJECT_SIZE)
            .into_iter()
            .map(|chunk| {
                let vec: Vec<_> = chunk.collect();
                let floats = vec
                    .iter()
                    .copied()
                    .take(MAT_SIZE)
                    .chunks(std::mem::size_of::<f64>())
                    .into_iter()
                    .map(|flt| {
                        let flt_bytes: Vec<u8> = flt.collect();
                        match flt_bytes.try_into() {
                            Ok(flt) => Ok(f64::from_be_bytes(flt)),
                            Err(_) => Err("Could not parse bytes to f64"),
                        }
                    })
                    .collect::<Result<Vec<f64>, _>>()?;

                let mat: ObjData = floats
                    .chunks(4)
                    .into_iter()
                    .map(|row| {
                        if row.len() != 4 {
                            Err("Invalid matrix row length")
                        } else {
                            Ok([row[0], row[1], row[2], row[3]])
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .try_into()
                    .map_err(|_| "Invalid # matrix rows")?;
                let id = ObjectId::from_be_bytes(
                    vec[MAT_SIZE..MAT_SIZE + 4].try_into()?,
                );
                let typ = vec[MAT_SIZE + 4].try_into()?;
                Ok(RemoteObject { mat, id, typ })
            })
            .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
        Ok(objs)
    }
}

fn serialize_login(login: &LoginInfo) -> (Vec<u8>, u8) {
    assert!(login.lighting.hdr.len() <= std::u8::MAX.into());
    assert!(login.lighting.skybox.len() <= std::u8::MAX.into());
    let data: Vec<_> = login
        .pid
        .to_be_bytes()
        .into_iter()
        .chain(login.spawn_pos[0].to_be_bytes().into_iter())
        .chain(login.spawn_pos[1].to_be_bytes().into_iter())
        .chain(login.spawn_pos[2].to_be_bytes().into_iter())
        .chain(login.lighting.dir_light.x.to_be_bytes().into_iter())
        .chain(login.lighting.dir_light.y.to_be_bytes().into_iter())
        .chain(login.lighting.dir_light.z.to_be_bytes().into_iter())
        .chain(login.starting_ids.0.to_be_bytes().into_iter())
        .chain(login.starting_ids.1.to_be_bytes().into_iter())
        .chain(std::iter::once(login.lighting.hdr.len() as u8))
        .chain(login.lighting.hdr.as_bytes().iter().copied())
        .chain(std::iter::once(login.lighting.skybox.len() as u8))
        .chain(login.lighting.skybox.as_bytes().iter().copied())
        .collect();
    (data, b'L')
}

fn deserialize_login(data: &[u8]) -> Result<LoginInfo, Box<dyn Error>> {
    if data.len() < LOGIN_MIN_SIZE {
        return Err("Login too short")?;
    }
    let pid = ObjectId::from_be_bytes(data[0..4].try_into()?);
    let spawn_pos: [f64; 3] = [
        f64::from_be_bytes(data[4..12].try_into()?),
        f64::from_be_bytes(data[12..20].try_into()?),
        f64::from_be_bytes(data[20..28].try_into()?),
    ];
    let light_dir: [f32; 3] = [
        f32::from_be_bytes(data[28..32].try_into()?),
        f32::from_be_bytes(data[32..36].try_into()?),
        f32::from_be_bytes(data[36..40].try_into()?),
    ];
    let starting_id = ObjectId::from_be_bytes(data[40..44].try_into()?);
    let ending_id = ObjectId::from_be_bytes(data[44..48].try_into()?);
    let hdr_len = data[48] as usize;
    let hdr = std::str::from_utf8(&data[49..49 + hdr_len])?.to_string();
    let skybox_len = data[49 + hdr_len] as usize;
    let skybox =
        std::str::from_utf8(&data[50 + hdr_len..50 + hdr_len + skybox_len])?
            .to_string();
    Ok(LoginInfo {
        pid,
        spawn_pos,
        lighting: game_map::GlobalLightingInfo {
            dir_light: From::from(light_dir),
            hdr,
            skybox,
        },
        starting_ids: (starting_id, ending_id),
    })
}

impl Serializeable for ClientCommandType {
    fn serialize(&self, msg_id: MsgId) -> Result<ChunkedMsg, Box<dyn Error>> {
        let (data, cmd_id) = match self {
            ClientCommandType::Login(name) => {
                if name.len() > 256 {
                    return Err("Login name too long")?;
                }
                let mut data = vec![name.len() as u8];
                data.extend(name.bytes());
                (data, b'L')
            }
            ClientCommandType::Update(objects) => serialize_objects(objects),
        };

        Ok(chunk_serialized_data(cmd_id, data.into_iter(), msg_id))
    }

    fn deserialize(
        chunks: ChunkedMsg,
    ) -> Result<(Self, MsgId), Box<dyn Error>> {
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
                Ok((Self::Login(name.to_string()), msg_id))
            }
            b'U' => Ok((
                Self::Update(deserialize_update(data)?),
                msg_id,
            )),
            _ => Err("Unknown command")?,
        }
    }
}

impl Serializeable for ServerCommandType {
    fn serialize(&self, msg_id: MsgId) -> Result<ChunkedMsg, Box<dyn Error>> {
        let (data, cmd_id) = match self {
            ServerCommandType::ReturnLogin(login) => serialize_login(login),
            ServerCommandType::Update(objects) => serialize_objects(objects),
        };
        Ok(chunk_serialized_data(cmd_id, data.into_iter(), msg_id))
    }

    fn deserialize(
        chunks: ChunkedMsg,
    ) -> Result<(Self, MsgId), Box<dyn Error>> {
        let (data, cmd_id, msg_id) = dechunk_serialized_data(chunks)?;
        match cmd_id {
            b'L' => {
                let login = deserialize_login(&data)?;
                Ok((Self::ReturnLogin(login), msg_id))
            }
            b'U' => Ok((
                Self::Update(deserialize_update(data)?),
                msg_id,
            )),
            _ => Err("Unknown command")?,
        }
    }
}
