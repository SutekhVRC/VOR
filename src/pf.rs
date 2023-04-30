use std::sync::mpsc::Sender;

use rosc::{OscPacket, decoder::MTU, encoder};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::Sender as bcst_Sender;

use crate::routedbg;

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

pub fn packet_filter(
    pf: &PacketFilter,
    buf: [u8; MTU],
    address: &String,
    bcst_tx: &bcst_Sender<Vec<u8>>,
    debug_sender: &Option<Sender<routedbg::DebugPacket>>
) {
    let pf_wl: Vec<String> = pf.address_wl.iter().map(|i| i.0.clone()).collect();
    let pf_bl: Vec<String> = pf.address_bl.iter().map(|i| i.0.clone()).collect();

    if pf.wl_enabled {
        // Whitelist
        match rosc::decoder::decode_udp(&buf) {
            Ok(ref pkt) => {
                if let OscPacket::Message(msg) = &pkt.1 {
                    if pf_wl.contains(&msg.addr) {
                        // Here sending the decoded packet's buffer instead of the UDP buffer
                        // because some OSC libraries cant parse OSC packets with trailing NULL bytes.
                        // However if malformed OSC packets are relayed due to PF allowing bad packets through they will be sent with a full MTU (NULL BYTES PADDED)
                        let encoded_packet_buf = encoder::encode(&pkt.1).unwrap();
                        bcst_tx.send(encoded_packet_buf).unwrap();
                        if let Some(ref dbgs) = debug_sender {
                            routedbg::send_indbg_packet(
                                dbgs,
                                &buf,
                                Some(pkt.1.clone()),
                                address.to_string(),
                                routedbg::IncomingDebugMode::ALLOWED,
                            );
                        }
                    } else {
                        if let Some(ref dbgs) = debug_sender {
                            routedbg::send_indbg_packet(
                                dbgs,
                                &buf,
                                Some(pkt.1.clone()),
                                address.to_string(),
                                routedbg::IncomingDebugMode::DROPPED,
                            );
                        }
                    }
                }
            }
            Err(_e) => {
                if !pf.filter_bad_packets {
                    // Bad OSC packet routed
                    bcst_tx.send(buf.to_vec()).unwrap();
                    if let Some(ref dbgs) = debug_sender {
                        routedbg::send_indbg_packet(
                            dbgs,
                            &buf,
                            None,
                            address.to_string(),
                            routedbg::IncomingDebugMode::ALLOWED,
                        );
                    }
                } else {
                    if let Some(ref dbgs) = debug_sender {
                        routedbg::send_indbg_packet(
                            dbgs,
                            &buf,
                            None,
                            address.to_string(),
                            routedbg::IncomingDebugMode::DROPPED,
                        );
                    }
                }
            }
        }
    } else if pf.bl_enabled {
        // Blacklist
        match rosc::decoder::decode_udp(&buf) {
            Ok(ref pkt) => {
                if let OscPacket::Message(msg) = &pkt.1 {
                    if !pf_bl.contains(&msg.addr) {
                        let encoded_packet_buf = encoder::encode(&pkt.1).unwrap();
                        bcst_tx.send(encoded_packet_buf).unwrap();

                        if let Some(ref dbgs) = debug_sender {
                            routedbg::send_indbg_packet(
                                dbgs,
                                &buf,
                                Some(pkt.1.clone()),
                                address.to_string(),
                                routedbg::IncomingDebugMode::ALLOWED,
                            );
                        }
                    } else {
                        if let Some(ref dbgs) = debug_sender {
                            routedbg::send_indbg_packet(
                                dbgs,
                                &buf,
                                Some(pkt.1.clone()),
                                address.to_string(),
                                routedbg::IncomingDebugMode::DROPPED,
                            );
                        }
                    }
                }
            }
            Err(_e) => {
                // Packet was bad should it still be sent?
                if !pf.filter_bad_packets {
                    // Bad OSC packet routed
                    bcst_tx.send(buf.to_vec()).unwrap();
                    if let Some(ref dbgs) = debug_sender {
                        routedbg::send_indbg_packet(
                            dbgs,
                            &buf,
                            None,
                            address.to_string(),
                            routedbg::IncomingDebugMode::ALLOWED,
                        );
                    }
                } else {
                    if let Some(ref dbgs) = debug_sender {
                        routedbg::send_indbg_packet(
                            dbgs,
                            &buf,
                            None,
                            address.to_string(),
                            routedbg::IncomingDebugMode::DROPPED,
                        );
                    }
                }
            }
        }
    } else {
        // No mode selected

        if pf.filter_bad_packets {
            // If filter bad packets enabled check if bad packet
            if let Ok(pkt) = rosc::decoder::decode_udp(&buf) {

                let encoded_packet_buf = encoder::encode(&pkt.1).unwrap();
                bcst_tx.send(encoded_packet_buf).unwrap();
                if let Some(ref dbgs) = debug_sender {
                    routedbg::send_indbg_packet(
                        dbgs,
                        &buf,
                        Some(pkt.1),
                        address.to_string(),
                        routedbg::IncomingDebugMode::ALLOWED,
                    );
                }
            } else {
                /*println!("[*] Filtered bad packet");*/
                if let Some(ref dbgs) = debug_sender {
                    routedbg::send_indbg_packet(
                        dbgs,
                        &buf,
                        None,
                        address.to_string(),
                        routedbg::IncomingDebugMode::DROPPED,
                    );
                }
            }
        } else {
            // If filter bad packets not enabled then send!
            bcst_tx.send(buf.to_vec()).unwrap();
            if let Some(ref dbgs) = debug_sender {
                // Try to get parsed packet
                if let Ok(pkt) = rosc::decoder::decode_udp(&buf) {
                    routedbg::send_indbg_packet(
                        dbgs,
                        &buf,
                        Some(pkt.1),
                        address.to_string(),
                        routedbg::IncomingDebugMode::ALLOWED,
                    );
                } else {
                    // Still ALLOWED because filter bad packets is disabled
                    routedbg::send_indbg_packet(
                        dbgs,
                        &buf,
                        None,
                        address.to_string(),
                        routedbg::IncomingDebugMode::ALLOWED,
                    );
                }
            }
        }
    }
}