use super::*;
/// Adds the string `"END"` to the end of the data
/// Adds the end delimiter to the last packet if it fits, otherwise creates a new packet
///
/// Requires `chunks` to be well-formed
pub(crate) fn add_end_chunk(mut chunks: ChunkedMsg) -> ChunkedMsg {
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
pub(crate) fn remove_end_chunk(
    mut chunks: ChunkedMsg,
) -> Result<ChunkedMsg, Box<dyn Error>> {
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
            Err("Invalid END position in chunk")?
        }
    } else {
        Err("No end chunk")?
    }
}

/// Sends a command to the specified socket
///
/// Adds an `END` token to the end of the data
/// # Errors
/// Returns an error of the data cannot be serialized or the socket cannot be sent to
pub fn send_data<T: Serializeable, S: ToSocketAddrs>(
    sock: &UdpSocket,
    addr: S,
    data: &T,
    msg_id: MsgId,
) -> Result<(), Box<dyn Error>> {
    let chunks = add_end_chunk(data.serialize(msg_id)?);
    for (_, chunk) in chunks {
        sock.send_to(&chunk, &addr)?;
    }
    Ok(())
}

/// A serizeable message that's fully or partially received from the socket
pub enum RemoteData<T: Serializeable> {
    Buffering(ChunkedMsg),
    Ready(T),
}

impl<T: Serializeable> RemoteData<T> {
    /// Converts the message to a ready message
    ///
    /// Fails if a buffered message cannot be deserialized
    /// # Errors
    /// Returns an error if the message cannot be deserialized
    pub fn to_ready(self) -> Result<Self, Box<dyn Error>> {
        use RemoteData::*;
        match self {
            Buffering(chunks) => {
                let (cmd, _) = T::deserialize(remove_end_chunk(chunks)?)?;
                Ok(Ready(cmd))
            }
            Ready(_) => Ok(self),
        }
    }

    /// Determines if a buffered message contains the END token and is not missing packets
    /// or the data is already ready
    pub fn is_ready(&self) -> bool {
        use RemoteData::*;
        match self {
            Ready(_) => true,
            Buffering(msg) => msg.iter().rev().next().map_or(
                false,
                |(last_pack_num, last_chunk)| {
                    last_chunk.windows(3).rev().any(|x| x == b"END")
                        && *last_pack_num as usize == msg.len() - 1
                },
            ),
        }
    }

    /// Adds a new packet to a buffering message. If this new packet makes the buffering
    /// message ready, converts the message to a ready message
    ///
    /// Fails if the message is already ready or if the packet is too small
    /// or if the packet number is a duplicate
    fn add_packet(self, packet: Vec<u8>) -> Result<Self, Box<dyn Error>> {
        use RemoteData::*;
        match self {
            Buffering(mut msg) if packet.len() >= CHUNK_METADATA_SIZE => {
                let pk_id = packet[PKT_NM_INDEX];
                if let std::collections::btree_map::Entry::Vacant(e) =
                    msg.entry(pk_id)
                {
                    e.insert(packet);
                    let this = Buffering(msg);
                    Ok(this.to_ready()?)
                } else {
                    Err("Duplicate packet")?
                }
            }
            Ready(_) => Err("Cannot add packet to ready message")?,
            Buffering(_) => Err("Packet too small")?,
        }
    }
}

/// Encapsulates a `RemoteData<T>` and its last access time
pub struct TimestampedRemoteData<T: Serializeable> {
    pub data: RemoteData<T>,
    last_access: std::time::Instant,
}

impl<T: Serializeable> Default for TimestampedRemoteData<T> {
    fn default() -> Self {
        Self {
            data: RemoteData::Buffering(BTreeMap::default()),
            last_access: std::time::Instant::now(),
        }
    }
}

impl<T: Serializeable> std::ops::Deref for TimestampedRemoteData<T> {
    type Target = RemoteData<T>;
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T: Serializeable> std::ops::DerefMut for TimestampedRemoteData<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<T: Serializeable> TimestampedRemoteData<T> {
    pub fn from(data: RemoteData<T>) -> Self {
        Self {
            data,
            last_access: std::time::Instant::now(),
        }
    }
}

impl<T: Serializeable> From<RemoteData<T>> for TimestampedRemoteData<T> {
    fn from(data: RemoteData<T>) -> Self {
        Self {
            data,
            last_access: std::time::Instant::now(),
        }
    }
}

/// Gets the command id and the packet num from the chunk
///
/// Requires `chunk` is a well-formed data chunk
#[inline]
fn get_cmd_ids_and_nums(chunk: &[u8]) -> (CommandId, MsgId, PacketNum) {
    (
        chunk[CMD_ID_INDEX],
        u32::from_be_bytes(
            chunk[MSG_ID_INDEX..MSG_ID_INDEX + 4].try_into().unwrap(),
        ),
        chunk[PKT_NM_INDEX],
    )
}

pub type ClientData<T> = TimestampedRemoteData<T>;
pub type ClientBuffer<T> = std::collections::HashMap<
    SocketAddr,
    BTreeMap<(CommandId, MsgId), ClientData<T>>,
>;

/// Receives a single packet from `socket`. If the packet is well-formed,
/// adds the packet to a buffering command. If the packet completes a buffering
/// command, removes the buffering command and returns the deserialized command
/// along with the sender address
///
/// The packet is dropped if it is malformed or the complete message cannot be deserialized
#[inline]
fn recv_data_helper<T: Serializeable, F>(
    data: &mut ClientBuffer<T>,
    recv_func: F,
) -> Result<Option<(T, SocketAddr)>, Box<dyn Error>>
where
    F: Fn(
        &std::rc::Rc<RefCell<[u8; MAX_DATAGRAM_SIZE]>>,
    ) -> std::io::Result<(usize, SocketAddr)>,
{
    use std::rc::Rc;
    std::thread_local!(
        static BUF : Rc<RefCell<[u8; MAX_DATAGRAM_SIZE]>> = Rc::new(RefCell::new([0; MAX_DATAGRAM_SIZE]))
    );
    if let Ok((amt, src)) = BUF.with(recv_func) {
        if amt > CHUNK_METADATA_SIZE {
            let msg = BUF.with(Clone::clone);
            let msg = msg.borrow();
            let (cmd_id, msg_id, _pn) = get_cmd_ids_and_nums(&*msg);
            let client_data = data.entry(src).or_insert(BTreeMap::new());
            let id = (cmd_id, msg_id);

            if let Ok(new_data) = client_data
                .remove(&id)
                .unwrap_or_default()
                .data
                .add_packet(msg[..amt].to_vec())
            {
                match new_data {
                    new_data @ RemoteData::Buffering(_) => {
                        client_data.insert(id, new_data.into());
                        Ok(None)
                    }
                    RemoteData::Ready(data) => Ok(Some((data, src))),
                }
            } else {
                Err("Could not add packet to buffering command")?
            }
        } else {
            Err("Packet too small")?
        }
    } else {
        Err("Failed to receive data")?
    }
}

/// Receives a single packet from `socket`. If the packet is well-formed,
/// adds the packet to a buffering command. If the packet completes a buffering
/// command, removes the buffering command and returns the deserialized command
/// along with the sender address
///
/// The packet is dropped if it is malformed or the complete message cannot be deserialized
///
/// Returns An error if the packet is too small, malformed, or the socket read fails,
/// returns `None` if the new packet does not finish a command, and returns the command
/// and sender address if the packet completes a command
/// # Errors
/// Returns an error if the packet is too small, malformed, or the socket read fails
pub fn recv_data<T: Serializeable>(
    socket: &UdpSocket,
    data: &mut ClientBuffer<T>,
) -> Result<Option<(T, SocketAddr)>, Box<dyn Error>> {
    recv_data_helper(data, |buf| socket.recv_from(&mut *buf.borrow_mut()))
}

/// Same as `recv_data` except only receives packets from the connected peer
///
/// Requires `socket` is a connected socket
/// # Errors
/// Returns an error if the packet is too small, malformed, or the socket read fails
pub fn recv_data_filtered<T: Serializeable>(
    socket: &UdpSocket,
    data: &mut ClientBuffer<T>,
) -> Result<Option<T>, Box<dyn Error>> {
    let dummy_addr: SocketAddr = SocketAddr::from(([0, 0, 0, 0], 0));
    match recv_data_helper(data, |buf| {
        socket
            .recv(&mut *buf.borrow_mut())
            .map(|res| (res, dummy_addr))
    }) {
        Ok(Some((data, _))) => Ok(Some(data)),
        Ok(None) => Ok(None),
        Err(e) => Err(e),
    }
}

/// Removes any buffering message whose last access time is older than `timeout`
pub fn clear_old_messages<T: Serializeable>(
    data: &mut ClientBuffer<T>,
    timeout: std::time::Duration,
) {
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

/// Arguments for `send_important`
pub struct ImportantArguments {
    pub max_recv_tries: u32,
    pub max_send_tries: u32,
    pub trial_recv_timeout: std::time::Duration,
    pub trial_send_timeout: std::time::Duration,
    pub total_send_attempts: u32,
}

impl Default for ImportantArguments {
    fn default() -> Self {
        Self {
            max_recv_tries: 5,
            max_send_tries: 3,
            total_send_attempts: 5,
            trial_recv_timeout: std::time::Duration::from_secs(2),
            trial_send_timeout: std::time::Duration::from_millis(300),
        }
    }
}

/// Helper for `send_important`
#[inline]
fn send_with_retries(
    chunks: ChunkedMsg,
    args: &ImportantArguments,
    sock: &UdpSocket,
) -> Result<(), Box<dyn Error>> {
    let mut total_send_attempts = 0;
    for (_, chunk) in chunks {
        let mut send_attempts = 0;
        while sock.send(&chunk).is_err() {
            if send_attempts >= args.max_send_tries
                || total_send_attempts >= args.total_send_attempts
            {
                return Err("Failed to send data")?;
            }
            send_attempts += 1;
            total_send_attempts += 1;
        }
    }
    Ok(())
}

/// Helper for `send_important`
#[inline]
fn recv_with_retries<R: Serializeable>(
    sock: &UdpSocket,
    args: &ImportantArguments,
    recv_data: &mut ClientBuffer<R>,
) -> Result<R, Box<dyn Error>> {
    let mut recv_attempts = 0;
    let old_timeout = sock.read_timeout();
    std::mem::drop(sock.set_read_timeout(Some(args.trial_recv_timeout)));
    loop {
        match recv_data_filtered(sock, recv_data) {
            Err(_) if recv_attempts < args.max_recv_tries => recv_attempts += 1,
            Err(_) => {
                std::mem::drop(
                    old_timeout
                        .map(|old_timeout| sock.set_read_timeout(old_timeout)),
                );
                return Err("Failed to receive data")?;
            }
            Ok(None) => (),
            Ok(Some(msg)) => {
                std::mem::drop(
                    old_timeout
                        .map(|old_timeout| sock.set_read_timeout(old_timeout)),
                );
                return Ok(msg);
            }
        }
    }
}

/// Sends a packet to `socket`, and waits for a response
/// Utilizes `args` to determine the timeouts and max send attempts
///
/// Requires `socket` is a connected socket and blocking
/// # Errors
/// Returns an error if the packet is too small, malformed, or the socket read fails
pub fn send_important<S: Serializeable, R: Serializeable>(
    sock: &UdpSocket,
    send_data: &S,
    send_msg_id: MsgId,
    recv_data: &mut ClientBuffer<R>,
    args: &ImportantArguments,
) -> Result<R, Box<dyn Error>> {
    let chunks = add_end_chunk(send_data.serialize(send_msg_id)?);
    send_with_retries(chunks, args, sock)?;
    recv_with_retries(sock, args, recv_data)
    // TODO: This assumes that the message was received by the peer successfully.
    // Also, it does not guarantee that the response is for the request that was just sent
}
