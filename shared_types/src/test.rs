use super::*;

#[test]
fn cct_serialize_deserialize() {
    let cct = ClientCommandType::Login("test".to_string());
    let msg_id = 0x22458;
    let chunks = cct.serialize(msg_id).unwrap();
    let cct2 = ClientCommandType::deserialize(chunks).unwrap();
    assert_eq!((cct, msg_id), cct2);
}

#[test]
fn sct_serialize_deserialize() {
    let sct = ServerCommandType::ReturnId(0x12345678);
    let msg_id = 0x2A458;
    let chunks = sct.serialize(msg_id).unwrap();
    let sct2 = ServerCommandType::deserialize(chunks).unwrap();
    assert_eq!((sct, msg_id), sct2);
}

#[test]
fn add_rem_end() {
    let cmd = ServerCommandType::ReturnId(475893);
    let msg = cmd.serialize(0x2A458).unwrap();
    assert_eq!(remove_end_chunk(add_end_chunk(msg.clone())).unwrap(), msg);

    let cmd = ClientCommandType::Login("HellloThereIAmGroot".to_string());
    let msg = cmd.serialize(0x2A458).unwrap();
    assert_eq!(remove_end_chunk(add_end_chunk(msg.clone())).unwrap(), msg);
}