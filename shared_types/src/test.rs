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
    let sct = ServerCommandType::ReturnLogin(LoginInfo {
        pid: ObjectId::new(475893),
        starting_ids: (ObjectId::new(0x123456), ObjectId::new(0x12345679)),
        spawn_pos: [0.0, 1.0, -2.0],
        lighting: game_map::GlobalLightingInfo {
            dir_light: cgmath::vec3(10.1f32, -20.2, 30.3),
            hdr: String::from("assets/hdr/hdr.hdr"),
            skybox: String::from("assets/skybox.jpg"),
        },
    });
    let msg_id = 0x2A458;
    let chunks = sct.serialize(msg_id).unwrap();
    let sct2 = ServerCommandType::deserialize(chunks).unwrap();
    assert_eq!((sct, msg_id), sct2);

    let objs = vec![
        RemoteObject {
            mat: [
                [1.0, 2.0, 3.0, 4.0],
                [5.0, 6.0, 7.0, 8.0],
                [9.0, 10.0, 11.0, 12.0],
                [13.0, 14.0, 15.0, 16.0],
                [17., 18., 19., 20.],
            ],
            id: ObjectId::new(0x12345678),
            typ: ObjectType::Asteroid,
        },
        RemoteObject {
            mat: [
                [1.0, 2.0, 3.0, 4.0],
                [5.0, 6.0, 7.0, 8.0],
                [9.0, 10.0, 11.0, 12.0],
                [13.0, 14.0, 15.0, 16.0],
                [17., 18., 19., 20.],
            ],
            id: ObjectId::new(0x12345679),
            typ: ObjectType::Ship,
        },
    ];
    let sct = ServerCommandType::Update(objs);
    let msg_id = 0x2A458;
    let chunks = sct.serialize(msg_id).unwrap();
    let sct2 = ServerCommandType::deserialize(chunks).unwrap();
    assert_eq!((sct, msg_id), sct2);

    let mut objs = Vec::new();
    for i in 0..20 {
        let idx = i as f64;
        objs.push(RemoteObject {
            mat: [
                [1.0 * idx, 2.0 * idx, 3.0 * idx, 4.0 * idx],
                [-5.0 * idx, 6.0 * idx, 7.0 * idx, 8.0 * idx],
                [9.0, 10.0, 11.0, 12.0],
                [13.0 * idx, 14.0 * idx, 15.0 * idx, 16.0 * idx],
                [17., 18., 19., 20.],
            ],
            id: ObjectId::new(0x12345678 + i as super::ObjectIdType),
            typ: ObjectType::Asteroid,
        });
    }

    let sct = ServerCommandType::Update(objs);
    let msg_id = 0x2A458;
    let chunks = sct.serialize(msg_id).unwrap();
    let sct2 = ServerCommandType::deserialize(chunks).unwrap();
    assert_eq!((sct, msg_id), sct2);
}

#[test]
fn add_rem_end() {
    let cmd = ServerCommandType::ReturnLogin(LoginInfo {
        pid: ObjectId::new(475893),
        starting_ids: (ObjectId::new(0x12345678), ObjectId::new(0x12345679)),
        spawn_pos: [0.0, 1.0, -2.0],
        lighting: game_map::GlobalLightingInfo {
            dir_light: cgmath::vec3(10.1f32, -20.2, 30.3),
            hdr: String::from("assets/hdr/hdr.hdr"),
            skybox: String::from("assets/skybox.jpg"),
        },
    });
    let msg = cmd.serialize(0x2A458).unwrap();
    assert_eq!(remove_end_chunk(add_end_chunk(msg.clone())).unwrap(), msg);

    let cmd = ClientCommandType::Login("HellloThereIAmGroot".to_string());
    let msg = cmd.serialize(0x2A458).unwrap();
    assert_eq!(remove_end_chunk(add_end_chunk(msg.clone())).unwrap(), msg);
}
