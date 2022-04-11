use rosc::{self, OscPacket};
use tokio::sync::broadcast::error::TryRecvError;
use tokio::sync::broadcast::{self, Sender as bcst_Sender, Receiver as bcst_Receiver};
use rosc::decoder::MTU;
use std::net::UdpSocket;
use std::sync::mpsc::{self, Sender, Receiver};
use std::thread;
use serde::{Deserialize, Serialize};

use crate::{
    VORAppIdentifier,
    VORAppStatus,
    VORConfig,
    app_error,
};

pub enum RouterMsg {
    ShutdownAll,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RouterConfig {
    pub bind_host: String,
    pub bind_port: String,
    pub vrc_host: String,
    pub vrc_port: String,
    pub vor_buffer_size: String,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct PacketFilter {
    pub enabled: bool,
    pub filter_bad_packets: bool,
    pub wl_enabled: bool,
    //pub wl_editing: bool,
    pub address_wl: Vec<(String, bool)>,
    pub bl_enabled: bool,
    //pub bl_editing: bool,
    pub address_bl: Vec<(String, bool)>,
}

fn route_app(mut rx: bcst_Receiver<Vec<u8>>, router_rx: Receiver<bool>, app_stat_tx_at: Sender<VORAppIdentifier>, ai: i64, app: VORConfig) {
    let rhp = format!("{}:{}", app.app_host, app.app_port);
    let lhp = format!("{}:{}", app.bind_host, app.bind_port);
    let sock = match UdpSocket::bind(lhp) {
        Ok(s) => s,
        Err(_e) => {
            let _ = app_stat_tx_at.send(app_error(ai, -2, format!("Failed to bind app UdpSocket: {}", _e)));
            return;// Close app route thread because app failed to bind
        }
    };
    println!("[*] OSC App: [{}] Route Initialized..", app.app_name);
    let _ = app_stat_tx_at.send(VORAppIdentifier { index: ai, status: VORAppStatus::Running });
    loop {

        match router_rx.try_recv() {
            Ok(signal) => {
                //println!("[!] signal: {}", signal);
                if signal {
                    let _ = app_stat_tx_at.send(VORAppIdentifier { index: ai, status: VORAppStatus::Stopped });
                    println!("[!] Send Stopped status");
                    return;
                }
            },
            _ => {/*println!("[!] Try recv errors")*/},
        }

        // Get vrc OSC buffer
        let buffer = match rx.try_recv() {
            Ok(b) => b,
            Err(TryRecvError::Empty) => continue,
            Err(TryRecvError::Lagged(_)) => continue,
            Err(TryRecvError::Closed) => {

                // VRC OSC BUFFER CHANNEL DIED SO KILL ROUTE THREAD
                let _ = app_stat_tx_at.send(VORAppIdentifier { index: ai, status: VORAppStatus::Stopped });

                return;
            }
        };

        // Route buffer
        match sock.send_to(&buffer, &rhp) {
            Ok(_bs) => {},
            Err(_e) => {
                let _ = app_stat_tx_at.send(app_error(ai, -3, format!("Failed to send VRC OSC buffer to app: {}", _e)));
            }
        }
    }
}

fn parse_vrc_osc(bcst_tx: bcst_Sender<Vec<u8>>, router_rx: Receiver<bool>, pf: PacketFilter, vrc_sock: UdpSocket) {
    let pf_wl: Vec<String> = pf.address_wl.iter().map(|i| i.0.clone()).collect();
    let pf_bl: Vec<String> = pf.address_bl.iter().map(|i| i.0.clone()).collect();
    let mut buf = [0u8; MTU];
    vrc_sock.set_nonblocking(true).unwrap();
    loop {

        match vrc_sock.recv_from(&mut buf) {
            Ok((br, _a)) => {

                if br <= 0 {// If got bytes send them to routers otherwise restart loop
                    continue;
                } else {


                    // Packet Filtering

                    if pf.enabled {// PF enabled
                        if pf.wl_enabled {// Whitelist
                            match rosc::decoder::decode_udp(&buf) {
                                Ok(pkt) => {
                                    if let OscPacket::Message(msg) = pkt.1 {
                                        if pf_wl.contains(&msg.addr) {
                                            bcst_tx.send(buf.to_vec()).unwrap();
                                        }
                                    }
                                },
                                Err(_e) => {
                                    if !pf.filter_bad_packets {
                                        println!("[*] Routing bad OSC packet!");
                                        bcst_tx.send(buf.to_vec()).unwrap();
                                    }
                                },
                            }
                        } else if pf.bl_enabled {// Blacklist
                            match rosc::decoder::decode_udp(&buf) {
                                Ok(pkt) => {
                                    if let OscPacket::Message(msg) = pkt.1 {
                                        if !pf_bl.contains(&msg.addr) {
                                            bcst_tx.send(buf.to_vec()).unwrap();
                                        }
                                    }
                                },
                                Err(_e) => {// Packet was bad should it still be sent?
                                    if !pf.filter_bad_packets {
                                        println!("[*] Routing bad OSC packet!");
                                        bcst_tx.send(buf.to_vec()).unwrap();
                                    }
                                },
                            }
                        } else {// No mode selected

                            if pf.filter_bad_packets {// If filter bad packets enabled check if bad packet
                                if let Ok(_) = rosc::decoder::decode_udp(&buf) {
                                    bcst_tx.send(buf.to_vec()).unwrap();
                                } else {println!("[*] Filtered bad packet");}
                            } else {// If filter bad packets not enabled then send!
                                bcst_tx.send(buf.to_vec()).unwrap();
                            }
                        }
                    } else {// PF disabled
                        bcst_tx.send(buf.to_vec()).unwrap();
                    }

                    match router_rx.try_recv() {
                        Ok(sig) => {
                            if sig {
                                println!("[!] VRC OSC thread shutdown");
                                return;
                            }
                        },
                        Err(_) => {},
                    }
                }
            },
            Err(_e) => {
                match router_rx.try_recv() {
                    Ok(sig) => {
                        if sig {
                            println!("[!] VRC OSC thread shutdown");
                            return;
                        }
                    },
                    Err(_) => {},
                }
            },
        }// vrc recv sock
    }// loop
}

pub fn route_main(
    router_bind_target: String,
    router_rx: Receiver<RouterMsg>,
    app_stat_tx: Sender<VORAppIdentifier>,
    configs: Vec<(VORConfig, i64)>,
    pf: PacketFilter,
    vor_queue_size: usize
) {

    let vrc_sock = match UdpSocket::bind(router_bind_target) {
        Ok(s) => s,
        Err(_e) => {
            let _ = app_stat_tx.send(app_error(-1, -1, "Failed to bind VOR socket.".to_string()));
            return;
        }
    };
    vrc_sock.set_nonblocking(true).unwrap();

    let mut artc = Vec::new();
    //let mut indexer: i64 = 0;

    //let async_rt = Runtime::new().unwrap();
    let (bcst_tx, _bcst_rx) = broadcast::channel(vor_queue_size);

    for (app, id) in configs {

        let (router_tx, router_rx) = mpsc::channel();
        artc.push(router_tx);

        let app_stat_tx_at = app_stat_tx.clone();
        let bcst_app_rx = bcst_tx.subscribe();
        //async_rt.spawn(route_app(bcst_app_rx, router_rx, app_stat_tx_at, indexer, app));
        thread::spawn(move || route_app(bcst_app_rx, router_rx, app_stat_tx_at, id, app));
        //indexer += 1;
    }
    drop(_bcst_rx);// Dont need this rx

    let (osc_parse_tx, osc_parse_rx): (Sender<bool>, Receiver<bool>) = mpsc::channel();
    thread::spawn(move || {parse_vrc_osc(bcst_tx, osc_parse_rx, pf, vrc_sock);});
    println!("[+] Started VRChat OSC Router.");

    // Listen for GUI events
    loop {
        match router_rx.recv().unwrap() {
            RouterMsg::ShutdownAll => {
                // Send shutdown to all threads

                // Shutdown osc parse thread first
                osc_parse_tx.send(true).unwrap();
                println!("[*] Shutdown signal: OSC receive thread");

                // Shutdown app route threads
                for app_route_thread_channel in artc {
                    let _ = app_route_thread_channel.send(true);
                }
                println!("[*] Shutdown signal: Route threads");

                // Shutdown router thread last
                println!("[*] Shutdown signal: Router thread");
                return;// Shutdown router thread.
            },
            //_ =>{},
        }
    }
}