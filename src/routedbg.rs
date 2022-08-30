use std::sync::mpsc::Sender;

use rosc::{self, OscPacket};

pub struct VORDebug {
    pub debug_enabled: bool,
    pub vor_dbg_packets: Vec<DebugPacket>,
    //pub vor_out_packets: Vec<OPacket>,
    pub options: VORDebugOptions,
    pub sig_channel_handler: DebugChannelHandler,
    pub ui_opts: VORUIOptions,
}

impl VORDebug {
    pub fn new() -> Self {
        VORDebug {
            debug_enabled: false,
            vor_dbg_packets: Vec::new(),
            //vor_out_packets: Vec::new(),
            options: VORDebugOptions {
                inc_dbg_mode: IncomingDebugMode::ALLOWED,
                route_dbg_mode: OutgoingDebugMode::ALL,
            },
            sig_channel_handler: DebugChannelHandler::new(),
            ui_opts: VORUIOptions::default(),
        }
    }
}

/*
#[derive(Debug)]
pub struct OSCParsedPacket {
    pub address: String,
    pub args: Vec<OscType>
}*/

pub struct VORUIOptions {
    pub show_incoming: bool,
    pub show_outgoing: bool,
    pub show_dropped: bool,
    pub show_allowed: bool,
    pub search_query: String,
}

impl Default for VORUIOptions {
    fn default() -> Self {
        VORUIOptions {
            show_incoming: true,
            show_outgoing: true,
            show_dropped: true,
            show_allowed: true,
            search_query: String::new(),
        }
    }
}

pub struct DebugChannelHandler {
    pub debug_in: DebugChannel,
}

impl DebugChannelHandler {
    pub fn new() -> Self {
        Self {
            debug_in: DebugChannel::new(),
        }
    }
}

pub struct DebugChannel {
    pub tx: std::sync::mpsc::Sender<DebugPacket>,
    pub rx: std::sync::mpsc::Receiver<DebugPacket>,
}

impl DebugChannel {
    pub fn new() -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        Self { tx, rx }
    }
}

/*
// Debug signal sent or received to a route or the vor receiver
pub enum DebugSignal {
    PACKET(DebugPacket),
    OPTIONS(VORDebugOptions),
}*/

// Debug options
#[derive(Debug, Clone)]
pub struct VORDebugOptions {
    pub inc_dbg_mode: IncomingDebugMode,
    pub route_dbg_mode: OutgoingDebugMode,
}

#[derive(Debug)]
pub struct IPacket {
    pub packet_buffer: Vec<u8>,
    pub osc_packet: Option<OscPacket>,
    pub mode: IncomingDebugMode,
    pub from_address: String,
}

#[derive(Debug)]
pub struct OPacket {
    pub packet_buffer: Vec<u8>,
    pub osc_packet: Option<rosc::OscPacket>,
    pub route: String,
    pub to_address: String,
}

// A packet in the form of a debug wrapper
#[derive(Debug)]
pub enum DebugPacket {
    INCOMING(IPacket),
    OUTGOING(OPacket),
}

impl DebugPacket {
    pub fn search(&self, query: String) -> bool {
        if query.is_empty() {
            return false;// Optimization for when string is empty (for now)
        }
        let query = query.to_lowercase();
        match self {
            Self::INCOMING(i) => {
                if format!("{:?}", self).to_lowercase().contains(&query) {
                    return true;
                }

                // query in src address
                if i.from_address.to_lowercase().contains(&query) {
                    return true;
                }

                // query in mode
                if format!("{:?}", i.mode).to_lowercase().contains(&query) {
                    return true;
                }

                // query in osc packet address
                match i.osc_packet.as_ref().unwrap() {
                    &OscPacket::Message(ref msg) => {
                        if msg.addr.to_lowercase().contains(&query) {
                            return true;
                        }
                        // Maybe allow search for args values?
                    }
                    _ => {}
                }
                return false;
            }
            Self::OUTGOING(o) => {
                if format!("{:?}", self).to_lowercase().contains(&query) {
                    return true;
                }

                if o.to_address.to_lowercase().contains(&query) {
                    return true;
                }

                // query  route
                if format!("{:?}", o.route).to_lowercase().contains(&query) {
                    return true;
                }

                // query in osc packet address
                match o.osc_packet.as_ref().unwrap() {
                    &OscPacket::Message(ref msg) => {
                        if msg.addr.to_lowercase().contains(&query) {
                            return true;
                        }
                        // Maybe allow search for args values?
                    }
                    _ => {}
                }
                return false;
            }
        }
    }
}

// Incoming debug mode
#[derive(Debug, Clone)]
pub enum IncomingDebugMode {
    ALLOWED,
    DROPPED,
}

impl IncomingDebugMode {
    pub fn is_allowed(&self) -> bool {
        if let Self::ALLOWED = self {
            true
        } else {
            false
        }
    }

    pub fn is_dropped(&self) -> bool {
        if let Self::DROPPED = self {
            true
        } else {
            false
        }
    }
}

// Outgoing debug mode (kind of just a filler)
#[derive(Debug, Clone)]
pub enum OutgoingDebugMode {
    /* Filter route option? Could just do this on UI side */
    ALL,
}

pub fn send_indbg_packet(
    dbgs: &Sender<DebugPacket>,
    buf: &[u8],
    osc_packet: Option<OscPacket>,
    from_address: String,
    mode: IncomingDebugMode,
) {
    let _ = dbgs.send(DebugPacket::INCOMING(IPacket {
        packet_buffer: buf.to_vec(),
        osc_packet,
        mode,
        from_address,
    }));
}

pub fn send_outdbg_packet(
    dbgs: &Sender<DebugPacket>,
    route: String,
    to_address: String,
    buf: &[u8],
    osc_packet: Option<OscPacket>,
) {
    let _ = dbgs.send(DebugPacket::OUTGOING(OPacket {
        packet_buffer: buf.to_vec(),
        osc_packet,
        route,
        to_address,
    }));
}
