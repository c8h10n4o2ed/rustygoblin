#![allow(unused_assignments, unused_variables, unused_mut, dead_code, unused_parens, deprecated, unused_imports)]
extern crate pnet;
extern crate getopts;
extern crate zmq;
extern crate rand;
extern crate slog;
extern crate slog_term;
extern crate serde;
extern crate serde_json;
extern crate lazy_static;

use getopts::Options;
use pnet::datalink::Channel::Ethernet;
use pnet::datalink::{self, DataLinkReceiver, DataLinkSender, MacAddr, NetworkInterface};
use pnet::packet::ethernet::{EtherTypes, EtherType, EthernetPacket};
use pnet::packet::ipv4::Ipv4Packet;
use pnet::packet::PrimitiveValues;
use std::collections::HashMap;
use std::sync::mpsc;
use std::{thread};
use std::fmt;
use std::sync::Arc;
use slog::*;
use pcap_file::{Packet,PcapWriter};
use std::fs::File;

//These must be defined here in order for the relevant
//source files to be built by Rust.
mod hcor;
mod messaging;

enum Payload {
    Packet {
        source: MacAddr,
        destination: MacAddr,
        kind: EtherType,
        packet: Vec<u8>,
    },
    Pulse(u64),
}

fn main() -> () {
    let plain = slog_term::PlainSyncDecorator::new(std::io::stdout());
    let mut logger = Arc::new(Logger::root(
        slog_term::FullFormat::new(plain)
        .build().fuse(), o!()
    ));
    let mut logger2 = Arc::clone(&logger);
    info!(*Arc::make_mut(&mut logger2), "Started application!");

    let mut args: Vec<String> = std::env::args().collect();
    let program = args[0].clone();

    let mut src_intf: String;
    let mut opts = Options::new();
    opts.optopt("i", "interface", "Source Interface", "NAME");
    opts.optopt("b", "bind", "Bind Address", "tcp://*:5556");
    opts.optopt("c", "connect", "Connect string", "tcp://127.0.0.1:5556");
    opts.optflag("s", "server", "Enable server");
    opts.optflag("l", "client", "Enable client");
    opts.optflag("h", "help", "Show usage");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => { m }
        Err(f) => { panic!(f.to_string()) }
    };

    if matches.opt_present("h") {
        print_usage(&program, opts);
        return ();
    }

    src_intf = matches.opt_str("i").clone().unwrap();
    let pkt_rcv = launch_capture(logger, &src_intf);

    if matches.opt_present("s") {
        info!(*Arc::make_mut(&mut logger2), "Launching ZMQ server");
        let bindaddr = matches.opt_str("b").unwrap();
        let rcv = messaging::launch_zmq_server(&bindaddr);
        let t = thread::spawn(move || {
            while let Ok(payload) = rcv.recv() {
                match payload {
                    messaging::ZMQChannelPayload::BasicMsg(msg) => {
                        //let digest = hcor::calc_hash(msg.packet.as_ref());
                        let mut digest : [u8; 16] = Default::default();
                        //let digest : &[u8] = &msg.packet;
                        digest.copy_from_slice(&msg.packet);
                        //println!("Adding ZMQ packet hash: {}", HexSlice(&digest));
                        hcor::add_packet_hash(&digest);
                    },
                    messaging::ZMQChannelPayload::None => {
                    }
                }
            }
        });
        t.join().unwrap();
    } else if matches.opt_present("l") {
        info!(*Arc::make_mut(&mut logger2), "Launching ZMQ client");
        let connectaddr = matches.opt_str("c").unwrap();
        let snd = messaging::launch_zmq_client(&connectaddr);
        let t = thread::spawn(move || {
            while let Ok(payload) = pkt_rcv.recv() {
                match payload {
                    Payload::Pulse(id) => {
                    },
                    Payload::Packet {
                        source: src,
                        destination: dst,
                        kind: etype,
                        packet: pkt
                    } => {
                        let msg = messaging::ZMQChannelPayload::BasicMsg(
                            messaging::BasicZMQMessage {
                                timestamp: 0.0,
                                packet: pkt.to_vec()
                            }
                        );
                        if snd.try_send(msg).is_err() {                            
                        }
                    }
                }
            }
        });
        t.join().unwrap();
    }    
}

fn print_usage (program: &str, opts: Options) {
    let brief = format!("Usage: {} FILE [options]", program);
    print!("{}", opts.usage(&brief));
}

fn launch_capture (mut logger : Arc<Logger>, intf_name : &String) -> mpsc::Receiver<Payload> {
    let ifname_match = |iface: &NetworkInterface| {
        iface.name == *intf_name
    };

    let interfaces: Vec<NetworkInterface> = datalink::interfaces();
    let interface = interfaces.into_iter().filter(ifname_match).next().unwrap();

    let (snd, rcv) = mpsc::sync_channel(50);
    let mut logger2 = Arc::clone(&logger);
    let timer_snd = snd.clone();
    let _ = thread::spawn(move || timer(timer_snd));

    let mut logger2 = Arc::clone(&logger);
    let iface_handler = match datalink::channel(&interface, Default::default()) {
        Ok(Ethernet(tx, rx)) => {
            let snd = snd.clone();
            thread::spawn(move || {
                watch_interface(&mut logger2, tx, rx, snd)
            })
        }
        Ok(_) => panic!("Unhandled channel type!"),
        Err(e) => panic!("An error occured creating the datalink channel: {}", e)
    };
    return rcv;
}

fn timer (snd: mpsc::SyncSender<Payload>) -> () {
    use std::{thread, time};
    let one_second = time::Duration::from_millis(1000);

    let mut pulses = 0;
    loop {
        thread::sleep(one_second);
        snd.send(Payload::Pulse(pulses)).unwrap();
        pulses += 1;
    }
}

fn gather(mut logger2 : &mut Arc<Logger>, rcv: mpsc::Receiver<Payload>) -> () {
    let mut sources: HashMap<MacAddr, u64> = HashMap::new();
    let mut dest: HashMap<MacAddr, u64> = HashMap::new();
    let mut ethertypes: HashMap<EtherType, u64> = HashMap::new();
    let mut logger = Arc::make_mut(logger2);
    while let Ok(payload) = rcv.recv() {
        match payload {
            Payload::Pulse(id) => {
                info!(*logger, "SOURCES:");
                for (k, v) in sources.iter() {
                    info!(*logger, "    {}: {}", k, v);
                }

                info!(*logger, "DESTS:");
                for (k, v) in dest.iter() {
                    info!(*logger, "    {}: {}", k, v);
                }

                info!(*logger, "ETHERTYPES:");
                for (k, v) in ethertypes.iter() {
                    let (x,) = k.to_primitive_values();
                    info!(*logger, "    {}/{:04x}: {}", k, x, v);
                }
            }
            Payload::Packet {
                source: src,
                destination: dst,
                kind: etype,
                packet: pkt
            } => {
                let mut destination = dest.entry(dst).or_insert(0);
                *destination += 1;

                let mut source = sources.entry(src).or_insert(0);
                *source += 1;
                
                let mut ethertype = ethertypes.entry(etype).or_insert(0);
                *ethertype += 1;
            }
        }
    }
}

//See: https://stackoverflow.com/questions/27650312/show-u8-slice-in-hex-representation
struct HexSlice<'a>(&'a [u8]);

impl<'a> HexSlice<'a> {
    fn new<T>(data: &'a T) -> HexSlice<'a> 
        where T: ?Sized + AsRef<[u8]> + 'a
    {
        HexSlice(data.as_ref())
    }
}

// You can even choose to implement multiple traits, like Lower and UpperHex
impl<'a> fmt::Display for HexSlice<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for byte in self.0 {
            // Decide if you want to pad out the value here
            write!(f, "{:02X}", byte)?;
        }
        Ok(())
    }
}

struct EtherTypeWrapper (EtherType);

impl fmt::LowerHex for EtherTypeWrapper {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let (x,) = self.0.to_primitive_values();
        write!(f, "{:04x}", x)
    }
}

fn watch_interface(
    mut logger2 : &mut Arc<Logger>,
    mut tx: Box<DataLinkSender>,
    mut rx: Box<DataLinkReceiver>,
    snd: mpsc::SyncSender<Payload>
) {
    let mut logger = Arc::make_mut(logger2);
    let file = File::create("out.pcap").expect("Error creating out.pcap!");
    let mut pcap_writer = PcapWriter::new(file).unwrap();

    loop {
        match rx.next() {
            Ok(packetb) => {
                let packet = EthernetPacket::new(packetb).unwrap();
                let payload: Payload = Payload::Packet {
                    source: packet.get_source(),
                    destination: packet.get_destination(),
                    kind: packet.get_ethertype(),
                    packet: hcor::calc_hash(&packetb).to_vec()
                };
                let v4p = Ipv4Packet::new(&packetb[EthernetPacket::minimum_packet_size() ..]).unwrap();
                if (packet.get_ethertype() == EtherTypes::Ipv4) &&
                   (format!("{}", v4p.get_destination()) != "172.21.0.24") &&
                   (format!("{}", v4p.get_source()) != "172.21.0.24") {
                    info!(*logger, "EtherType: {:x}, Dst: {}, Packet:{}", EtherTypeWrapper(packet.get_ethertype()), v4p.get_destination(), HexSlice::new(&packetb[EthernetPacket::minimum_packet_size() ..]));
                    let digest = hcor::calc_hash(&packetb);
                    hcor::add_packet_hash(&digest);

                    if snd.try_send(payload).is_err() {
                        //SKIPPED_PACKETS.fetch_add(1, Ordering::Relaxed);
                    }

                    let pf_packet = Packet::new(0, 0, packetb.len() as u32, &packetb);
                    pcap_writer.write_packet(&pf_packet).unwrap();                    
                }
            }
            Err(_) => {

            }
        }
    }
}
