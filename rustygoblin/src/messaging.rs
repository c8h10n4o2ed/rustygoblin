#![allow(unused_assignments, unused_variables, unused_mut, dead_code, unused_parens, deprecated, unused_imports)]

use serde::{Deserialize, Serialize};
use std::thread;
use std::time::Duration;
use std::sync::mpsc;

#[derive(Serialize, Deserialize)]
pub struct BasicZMQMessage {
    pub timestamp: f64,
    pub packet: Vec<u8>
}

pub enum ZMQChannelPayload {
    BasicMsg(BasicZMQMessage),
    None
}

pub fn launch_zmq_client (connect_str : &str) -> mpsc::SyncSender<ZMQChannelPayload> {
    let csocket = create_client_socket(connect_str);
    let (snd, rcv) = mpsc::sync_channel(50);
    println!("Launching ZMQ client");

    let _ = thread::spawn(move || {
        while let Ok(payload) = rcv.recv() {
            match payload {
                ZMQChannelPayload::BasicMsg(msg) => {
                    let jstr = serde_json::to_string(&msg).unwrap();
                    csocket.send(&jstr as &str, 0).unwrap();
                    csocket.recv_string(0).unwrap().unwrap();
                },
                ZMQChannelPayload::None => {
                }
            }
        }
    });

    return snd;
}

pub fn launch_zmq_server (bind_str : &str) -> mpsc::Receiver<ZMQChannelPayload> {
    let ssocket = create_server_socket(bind_str);
    let (snd, rcv) = mpsc::sync_channel(50);
    println!("Launching ZMQ server");

    let _ = thread::spawn(move || {
        loop {
            for idx in 0..3 {
                match ssocket.recv_string(0).unwrap() {
                    Ok(data) => {
                        //identity field = 0
                        //envelope field = 1
                        //message field = 2
                        if (idx == 2) {
                            let msg : BasicZMQMessage = serde_json::from_str(&data).unwrap();
                            snd.send(ZMQChannelPayload::BasicMsg(msg)).unwrap();
                        }
                    },
                    Err(_) => {
                    }
                }
            }

            //identity
            ssocket.send("test_client", zmq::SNDMORE).unwrap();

            //envelope
            ssocket.send("", zmq::SNDMORE).unwrap();

            //data
            ssocket.send("",0).unwrap();
        }
    });

    return rcv;
}

pub fn create_client_socket (connect_str : &str) -> zmq::Socket {
    let ctx = zmq::Context::new();

    let mut socket = ctx.socket(zmq::REQ).unwrap();
    socket.set_identity(b"test_client").unwrap();
    socket.connect(connect_str).unwrap();

    return socket;
}

pub fn create_server_socket (bind_str : &str) -> zmq::Socket {
    let ctx = zmq::Context::new();

    let mut socket = ctx.socket(zmq::ROUTER).unwrap();
    assert!(socket.bind(bind_str).is_ok());
    
    return socket;
}

#[test]
fn test_serdes_1 () {
    let msg = BasicZMQMessage {
        timestamp: 1.2345,
        packet: vec![0u8; 32]
    };
    let jstr = serde_json::to_string(&msg).unwrap();
    assert_eq!(jstr, "{\"timestamp\":1.2345,\"packet\":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}");

    let jstr2 = jstr.clone();
    let msg2 : BasicZMQMessage = serde_json::from_str(&jstr2).unwrap();

    assert_eq!(msg.timestamp, msg2.timestamp);
    assert_eq!(msg.packet, msg2.packet);
}

#[test]
fn test_zmq_1 () {
    let ssocket = create_server_socket("tcp://*:5556");
    let csocket = create_client_socket("tcp://127.0.0.1:5556");

    let cthread = thread::spawn(move || {
        for _ in 0..5 {
            csocket.send("test_msg", 0).unwrap();

            //response message
            csocket.recv_string(0).unwrap().unwrap();

            thread::sleep(Duration::from_millis(1000));
        }
    });

    let sthread = thread::spawn(move || {
        let mut cnt = 0;
        loop {
            cnt += 1;
            for idx in 0..3 {
                match ssocket.recv_string(0).unwrap() {
                    Ok(data) => {
                        //identity field = 0
                        //envelope field = 1
                        //message field = 2
                        if (idx == 2) {
                            println!("count:{}, msg:{}", cnt, data);
                            assert_eq!(data, "test_msg");
                        }
                    },
                    Err(_) => {                        
                    }
                }
            }

            //identity
            ssocket.send("test_client", zmq::SNDMORE).unwrap();

            //envelope
            ssocket.send("", zmq::SNDMORE).unwrap();

            //data
            ssocket.send("",0).unwrap();
        }
    });

    cthread.join().unwrap();
}
